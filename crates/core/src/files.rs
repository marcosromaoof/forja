use crate::{
    contracts::{FileContent, FileEntry},
    policy,
    storage::Store,
};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::path::Path;
pub fn read(root: &Path, path: &str) -> Result<FileContent> {
    let p = policy::resolve(root, path, false)?;
    ensure!(p.metadata()?.len() <= 2_000_000, "Arquivo maior que 2 MB");
    let bytes = std::fs::read(p)?;
    let content = String::from_utf8(bytes.clone())?;
    Ok(FileContent {
        path: path.into(),
        content,
        hash: crate::hash(&bytes),
    })
}
pub fn list(root: &Path, path: &str) -> Result<Vec<FileEntry>> {
    let p = policy::resolve(root, path, false)?;
    let mut out = Vec::new();
    for item in std::fs::read_dir(&p)?.take(2000) {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        let rel = if path == "." || path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        if policy::resolve(root, &rel, false).is_err()
            || ["node_modules", "target", "dist"].contains(&name.as_str())
        {
            continue;
        }
        let meta = item.metadata()?;
        out.push(FileEntry {
            path: rel,
            name,
            directory: meta.is_dir(),
            size: meta.len(),
        });
    }
    out.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}
pub fn search(root: &Path, query: &str) -> Result<Vec<Value>> {
    ensure!(
        !query.is_empty() && query.len() <= 1000,
        "Consulta inválida"
    );
    let base = root.canonicalize()?;
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(&base)
        .hidden(true)
        .max_filesize(Some(1_000_000))
        .build()
        .flatten()
    {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&base)?
            .to_string_lossy()
            .replace('\\', "/");
        if policy::resolve(root, &rel, false).is_err() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            for (i, line) in text.lines().enumerate() {
                if line.contains(query) {
                    out.push(json!({"path":rel,"line":i+1,"text":line.chars().take(500).collect::<String>()}));
                    if out.len() >= 100 {
                        return Ok(out);
                    }
                }
            }
        }
    }
    Ok(out)
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write(path, bytes, true)
}
pub fn atomic_create(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write(path, bytes, false)
}
fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Pasta ausente"))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".forja-write-{}", crate::id()));
    let result = (|| -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let ok = unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    (if replace {
                        windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    } else {
                        0
                    }) | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
                )
            };
            ensure!(
                ok != 0,
                "Falha na substituição atômica: {}",
                std::io::Error::last_os_error()
            );
        }
        #[cfg(not(windows))]
        {
            if replace {
                std::fs::rename(&temp, path)?;
            } else {
                std::fs::hard_link(&temp, path)?;
                std::fs::remove_file(&temp)?;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
pub fn patch(
    store: &Store,
    root: &Path,
    workspace: &str,
    run: &str,
    path: &str,
    base_hash: &str,
    old: &str,
    new: &str,
) -> Result<Value> {
    let p = policy::resolve(root, path, true)?;
    let before = if p.exists() {
        Some(read(root, path)?)
    } else {
        None
    };
    let text = if let Some(before) = &before {
        ensure!(
            before.hash == base_hash,
            "Conflito: arquivo mudou desde a leitura"
        );
        ensure!(
            (old.is_empty() && before.content.is_empty())
                || (!old.is_empty() && before.content.matches(old).count() == 1),
            "O trecho anterior deve ocorrer exatamente uma vez"
        );
        before.content.replacen(old, new, 1)
    } else {
        ensure!(
            base_hash.is_empty() && old.is_empty(),
            "Arquivo ausente; criação requer hash e trecho anterior vazios"
        );
        new.into()
    };
    ensure!(text.len() <= 2_000_000, "Arquivo maior que 2 MB");
    let before_hash = before
        .as_ref()
        .map(|f| store.blob(f.content.as_bytes()))
        .transpose()?;
    let cp = crate::id();
    let after_hash = crate::hash(text.as_bytes());
    let checkpoint = json!({"id":cp,"workspace_id":workspace,"run_id":run,"path":path,"before_hash":before_hash,"after_hash":after_hash,"state":"prepared","created_at":crate::now()});
    store.put("checkpoint", &cp, &checkpoint)?;
    if let Some(before) = &before {
        ensure!(
            crate::hash(&std::fs::read(&p)?) == before.hash,
            "Conflito antes da escrita"
        );
    } else {
        ensure!(!p.exists(), "Arquivo criado por outro processo");
    }
    policy::resolve(root, path, true)?;
    if before.is_some() {
        atomic(&p, text.as_bytes())?;
    } else {
        atomic_create(&p, text.as_bytes())?;
    }
    let mut checkpoint = checkpoint;
    checkpoint["state"] = json!("applied");
    store.put("checkpoint", &cp, &checkpoint)?;
    let prev = before.map(|f| f.content).unwrap_or_default();
    let diff = similar::TextDiff::from_lines(&prev, &text)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    Ok(json!({"path":path,"hash":after_hash,"checkpoint_id":cp,"diff":diff}))
}
pub fn restore(store: &Store, root: &Path, id: &str) -> Result<Value> {
    let mut cp: Value = store.get("checkpoint", id)?;
    ensure!(cp["state"] == "applied", "Checkpoint não restaurável");
    let path = cp["path"].as_str().unwrap();
    let p = policy::resolve(root, path, true)?;
    ensure!(
        crate::hash(&std::fs::read(&p)?) == cp["after_hash"].as_str().unwrap(),
        "Há mudanças posteriores; restauração bloqueada"
    );
    if let Some(hash) = cp["before_hash"].as_str() {
        atomic(&p, &store.read_blob(hash)?)?
    } else {
        std::fs::remove_file(p)?;
    }
    cp["state"] = json!("restored");
    store.put("checkpoint", id, &cp)?;
    Ok(cp)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_concurrent_changes() {
        let d = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let s = Store::open(db.path()).unwrap();
        std::fs::write(d.path().join("x"), "first").unwrap();
        let f = read(d.path(), "x").unwrap();
        std::fs::write(d.path().join("x"), "user").unwrap();
        assert!(patch(&s, d.path(), "w", "r", "x", &f.hash, "first", "agent").is_err());
        assert_eq!(std::fs::read_to_string(d.path().join("x")).unwrap(), "user");
    }
    #[test]
    fn checkpoint_restore() {
        let d = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let s = Store::open(db.path()).unwrap();
        let v = patch(&s, d.path(), "w", "r", "new.txt", "", "", "hello").unwrap();
        restore(&s, d.path(), v["checkpoint_id"].as_str().unwrap()).unwrap();
        assert!(!d.path().join("new.txt").exists());
    }
}
