use forja_core::mcp::{Config, Manager};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;

fn config(directory: &Path, version: &str, scenario: &str) -> Config {
    Config {
        id: "stdio".into(),
        name: "Fixture stdio".into(),
        transport: "stdio".into(),
        protocol_version: version.into(),
        url: String::new(),
        command: "node".into(),
        args: vec![
            format!(
                "{}/tests/fixtures/mcp-stdio.cjs",
                env!("CARGO_MANIFEST_DIR")
            ),
            version.into(),
            directory.to_string_lossy().into_owned(),
            scenario.into(),
        ],
        cwd: directory.to_string_lossy().into_owned(),
        secret_ref: None,
    }
}
fn messages(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path.join("messages.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[tokio::test]
async fn stdio_protocols_frame_utf8_validate_input_and_deny_server_requests() {
    for version in ["2025-11-25", "2026-07-28"] {
        let root = tempfile::tempdir().unwrap();
        let manager = Manager::default();
        let catalog = manager
            .connect(&config(root.path(), version, "ok"))
            .await
            .unwrap();
        let hash = catalog["catalog_hash"].as_str().unwrap();
        for (args, hash) in [(json!({"text":42}), hash), (json!({"text":"ok"}), "stale")] {
            assert!(manager
                .call("stdio", "echo", args, hash, CancellationToken::new())
                .await
                .is_err());
        }
        assert!(!messages(root.path())
            .iter()
            .any(|m| m["method"] == "tools/call"));
        let result = manager
            .call(
                "stdio",
                "echo",
                json!({"text":"Olá, ação 🔥"}),
                hash,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result["content"]["content"][0]["text"], "Olá, ação 🔥");
        assert_eq!(result["trust"], "untrusted_data");
        if version == "2025-11-25" {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if messages(root.path())
                        .iter()
                        .any(|m| m["id"] == 900 && m["error"]["code"] == -32601)
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
            })
            .await
            .unwrap();
        }
        manager.disconnect_all().await;
        assert!(manager.catalogs().await.is_empty());
        assert_eq!(
            messages(root.path())
                .iter()
                .filter(|m| m["method"] == "tools/call")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn malformed_stdio_response_disconnects_without_retry() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let catalog = manager
        .connect(&config(root.path(), "2026-07-28", "malformed"))
        .await
        .unwrap();
    let hash = catalog["catalog_hash"].as_str().unwrap();
    assert!(manager
        .call(
            "stdio",
            "echo",
            json!({"text":"x"}),
            hash,
            CancellationToken::new()
        )
        .await
        .is_err());
    assert!(manager.catalogs().await.is_empty());
    assert_eq!(
        messages(root.path())
            .iter()
            .filter(|m| m["method"] == "tools/call")
            .count(),
        1
    );
}
#[cfg(windows)]
#[tokio::test]
async fn cancellation_stops_stdio_server_and_job_descendant_without_retry() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let catalog = manager
        .connect(&config(root.path(), "2026-07-28", "slow"))
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let heartbeat = root.path().join("heartbeat");
    let observe = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !heartbeat.exists() {
                tokio::time::sleep(Duration::from_millis(20)).await
            }
        })
        .await
        .unwrap();
        stop.cancel();
    };
    let (result, ()) = tokio::join!(
        manager.call(
            "stdio",
            "echo",
            json!({"text":"slow"}),
            catalog["catalog_hash"].as_str().unwrap(),
            cancel
        ),
        observe
    );
    assert!(result.is_err());
    assert!(manager.catalogs().await.is_empty());
    tokio::time::sleep(Duration::from_millis(150)).await;
    let before = std::fs::read(&heartbeat).unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        before,
        std::fs::read(&heartbeat).unwrap(),
        "Child process survived Job Object cancellation"
    );
    assert_eq!(
        messages(root.path())
            .iter()
            .filter(|m| m["method"] == "tools/call")
            .count(),
        1
    );
}
