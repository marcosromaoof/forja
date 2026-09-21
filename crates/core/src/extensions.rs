use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::path::Path;
pub fn skills(root: &Path) -> Result<Vec<Value>> {
    crate::skills::scan(root)
}
pub fn inspect_plugin(path: &Path) -> Result<Value> {
    ensure!(
        path.file_name().is_some_and(|s| s == "plugin.json"),
        "Selecione plugin.json"
    );
    ensure!(path.metadata()?.len() < 128_000, "Manifesto grande demais");
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    for field in ["id", "name", "version", "apiVersion", "permissions"] {
        ensure!(!value[field].is_null(), "Campo obrigatório: {field}");
    }
    Ok(
        json!({"manifest":value,"sha256":crate::hash(text.as_bytes()),"executable":false,"reason":"Inspeção não executa código."}),
    )
}
