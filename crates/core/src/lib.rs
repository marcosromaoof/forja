pub mod agent;
pub mod agents;
pub mod backup;
pub mod browser;
mod chat_stream;
pub mod contracts;
pub mod extensions;
pub mod files;
pub mod hooks;
pub mod lsp;
mod model_catalog;
mod model_stream;
pub mod models;
pub mod plans;
pub mod policy;
pub mod process;
pub mod skills;
pub mod storage;
pub mod terminal;
pub mod web;

use std::path::PathBuf;
pub fn data_dir() -> PathBuf {
    std::env::var_os("FORJA_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(windows)]
            let base = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            #[cfg(not(windows))]
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir)
                        .join(".local/share")
                });
            base.join("Forja")
        })
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub mod native_models;

pub mod context;
pub mod context_budget;

pub mod mcp;
