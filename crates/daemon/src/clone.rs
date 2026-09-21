use anyhow::{ensure, Result};
use forja_core::{contracts::Workspace, storage::Store};
use serde_json::Value;
use std::path::Path;
pub async fn clone_workspace(store: &Store, body: &Value) -> Result<Workspace> {
    let url = body["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("URL obrigatória"))?;
    let u = reqwest::Url::parse(url)?;
    ensure!(
        u.scheme() == "https" && u.username().is_empty() && u.password().is_none(),
        "Use HTTPS sem credenciais na URL"
    );
    let path = Path::new(
        body["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Destino obrigatório"))?,
    );
    ensure!(
        path.is_absolute() && !path.exists(),
        "Destino deve ser uma pasta nova com caminho absoluto"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Pasta pai ausente"))?;
    ensure!(parent.is_dir(), "A pasta pai não existe");
    let mut c = tokio::process::Command::new("git");
    c.args([
        "-c",
        "credential.helper=",
        "-c",
        "core.hooksPath=",
        "-c",
        "protocol.file.allow=never",
        "clone",
        "--",
        url,
    ])
    .arg(path)
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(180), c.output()).await??;
    ensure!(
        output.status.success(),
        "Git clone falhou: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = path.canonicalize()?;
    let w = Workspace {
        id: forja_core::id(),
        name: root.file_name().unwrap().to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        created_at: forja_core::now(),
    };
    store.put("workspace", &w.id, &w)?;
    Ok(w)
}
