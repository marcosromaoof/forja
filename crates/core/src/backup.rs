use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub created_at: String,
    pub entries: Vec<Entry>,
}
fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Arquivo de backup deve ser regular"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Reparse point não permitido no backup"
        );
    }
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Diretório de backup inválido"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Junction não permitida no backup"
        );
    }
    Ok(())
}
fn digest(path: &Path, destination: Option<&Path>) -> Result<(String, u64)> {
    regular(path)?;
    let mut source = File::open(path)?;
    let mut target = destination
        .map(|p| OpenOptions::new().create_new(true).write(true).open(p))
        .transpose()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes = 0;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(
            bytes <= 16 * 1024 * 1024 * 1024,
            "Arquivo excede limite de 16 GB por item"
        );
        hash.update(&buffer[..n]);
        if let Some(file) = target.as_mut() {
            file.write_all(&buffer[..n])?;
        }
    }
    if let Some(file) = target {
        file.sync_all()?;
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn entry_path(value: &str) -> Result<()> {
    ensure!(
        value == "forja.sqlite" || value.strip_prefix("blobs/").is_some_and(valid_hash),
        "Caminho inválido no manifesto"
    );
    Ok(())
}
pub(crate) fn complete(snapshot: &Path, blobs: &Path) -> Result<()> {
    directory(blobs)?;
    let (sha256, bytes) = digest(&snapshot.join("forja.sqlite"), None)?;
    let mut entries = vec![Entry {
        path: "forja.sqlite".into(),
        sha256,
        bytes,
    }];
    fs::create_dir(snapshot.join("blobs"))?;
    for entry in fs::read_dir(blobs)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Nome de blob inválido"))?;
        // An interrupted atomic write is not a published blob.
        if name.starts_with(".pending-") {
            continue;
        }
        ensure!(valid_hash(&name), "Nome de blob inválido");
        let path = format!("blobs/{name}");
        let (sha256, bytes) = digest(&entry.path(), Some(&snapshot.join(&path)))?;
        ensure!(sha256 == name, "Blob corrompido; backup não publicado");
        entries.push(Entry {
            path,
            sha256,
            bytes,
        });
        ensure!(entries.len() <= 100_001, "Backup excede 100 mil blobs");
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: "forja-backup".into(),
        version: 1,
        created_at: crate::now(),
        entries,
    };
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(snapshot.join("manifest.json"))?;
    output.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    output.sync_all()?;
    Ok(())
}
fn manifest(source: &Path) -> Result<Manifest> {
    directory(source)?;
    directory(&source.join("blobs"))?;
    let file = source.join("manifest.json");
    regular(&file)?;
    ensure!(
        fs::metadata(&file)?.len() <= 32_000_000,
        "Manifesto excede limite"
    );
    let value: Manifest = serde_json::from_slice(&fs::read(file)?)?;
    ensure!(
        value.format == "forja-backup" && value.version == 1,
        "Formato de backup não suportado"
    );
    ensure!(
        !value.entries.is_empty() && value.entries.len() <= 100_001,
        "Número inválido de itens"
    );
    let mut seen = HashSet::new();
    for entry in &value.entries {
        entry_path(&entry.path)?;
        ensure!(
            valid_hash(&entry.sha256) && seen.insert(&entry.path),
            "Hash ou entrada duplicada inválida"
        );
        if let Some(name) = entry.path.strip_prefix("blobs/") {
            ensure!(entry.sha256 == name, "Hash não corresponde ao nome do blob");
        }
    }
    ensure!(
        seen.contains(&"forja.sqlite".to_string()),
        "Banco ausente no backup"
    );
    Ok(value)
}
fn verify_entries(source: &Path, value: &Manifest) -> Result<()> {
    for entry in &value.entries {
        let (hash, bytes) = digest(&source.join(&entry.path), None)?;
        ensure!(
            hash == entry.sha256 && bytes == entry.bytes,
            "Integridade inválida: {}",
            entry.path
        );
    }
    let database = rusqlite::Connection::open_with_flags(
        source.join("forja.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let integrity: String = database.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    ensure!(integrity == "ok", "Banco do backup está corrompido");
    let version: u32 = database.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        (1..=2).contains(&version),
        "Versão do banco não suportada por este aplicativo"
    );
    for table in ["documents", "events", "file_index"] {
        let exists: bool = database.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
            [table],
            |r| r.get(0),
        )?;
        ensure!(exists, "Tabela obrigatória ausente no backup");
    }
    Ok(())
}
pub fn verify(source: &Path) -> Result<Manifest> {
    let value = manifest(source)?;
    verify_entries(source, &value)?;
    Ok(value)
}
/// Restores into a new, exclusive directory. Never changes the current data directory.
/// A failed restore leaves an INCOMPLETE marker, which Store::open refuses to use.
pub fn restore(source: &Path, destination: &Path) -> Result<PathBuf> {
    ensure!(
        destination.is_absolute(),
        "Destino da restauração deve ser absoluto"
    );
    let source = source.canonicalize()?;
    let value = verify(&source)?;
    let parent = destination
        .parent()
        .context("Destino inválido")?
        .canonicalize()?;
    let target = parent.join(
        destination
            .file_name()
            .context("Nome de destino inválido")?,
    );
    ensure!(
        !target.starts_with(&source),
        "Destino não pode ficar dentro do backup"
    );
    fs::create_dir(&target).context("Escolha uma pasta de destino que ainda não exista")?;
    let marker = target.join("RESTORE_INCOMPLETE");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker)?;
    file.write_all(b"FORJA restore in progress. Do not open this directory as data.")?;
    file.sync_all()?;
    drop(file);
    fs::create_dir(target.join("blobs"))?;
    for entry in &value.entries {
        let (hash, bytes) = digest(&source.join(&entry.path), Some(&target.join(&entry.path)))?;
        ensure!(
            hash == entry.sha256 && bytes == entry.bytes,
            "Backup mudou durante a restauração"
        );
    }
    verify_entries(&target, &value)?;
    // Credentials and endpoint tokens are deliberately not part of a portable backup.
    fs::remove_file(marker)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;
    #[test]
    fn roundtrip_includes_blobs_events_and_recovers_without_reexecution() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("source")).unwrap();
        let hash = store.blob(b"user code before patch").unwrap();
        store
            .put("checkpoint", "c", &serde_json::json!({"blob":hash}))
            .unwrap();
        store
            .append(
                "s",
                "r",
                "tool.completed",
                serde_json::json!({"artifact":hash}),
            )
            .unwrap();
        fs::write(store.dir.join("daemon.json"), "private-token").unwrap();
        let backup = store.backup().unwrap();
        assert_eq!(verify(&backup).unwrap().entries.len(), 2);
        let target = restore(&backup, &temp.path().join("restored")).unwrap();
        assert!(!target.join("daemon.json").exists());
        let restored = Store::open(&target).unwrap();
        assert_eq!(
            restored.read_blob(&hash).unwrap(),
            b"user code before patch"
        );
        assert_eq!(restored.events("s", Some("r"), 0).unwrap().len(), 1);
        assert_eq!(restored.recover().unwrap(), 0);
        assert!(restore(&backup, &target).is_err());
    }
    #[test]
    fn corruption_and_traversal_fail_before_creating_destination() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("source")).unwrap();
        let hash = store.blob(b"original").unwrap();
        let backup = store.backup().unwrap();
        fs::write(backup.join("blobs").join(hash), b"corrupted").unwrap();
        let target = temp.path().join("restored");
        assert!(restore(&backup, &target).is_err());
        assert!(!target.exists());
        let mut value: Manifest =
            serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
        value.entries[0].path = "../outside.txt".into();
        fs::write(
            backup.join("manifest.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(restore(&backup, &target).is_err());
        assert!(!target.exists());
    }
}
