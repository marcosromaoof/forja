use forja_core::storage::Store;
use std::process::Command;

#[test]
fn recovery_cli_works_without_a_daemon_and_preserves_existing_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("source")).unwrap();
    let hash = store.blob(b"checkpoint contents").unwrap();
    let backup = store.backup().unwrap();
    let executable = env!("CARGO_BIN_EXE_forja");
    let verified = Command::new(executable)
        .arg("backup-verify")
        .arg(&backup)
        .env("FORJA_DATA_DIR", temp.path().join("no-daemon"))
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let destination = temp.path().join("recovered");
    let restored = Command::new(executable)
        .arg("restore")
        .arg(&backup)
        .arg("--destination")
        .arg(&destination)
        .env("FORJA_DATA_DIR", temp.path().join("no-daemon"))
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(
        Store::open(&destination).unwrap().read_blob(&hash).unwrap(),
        b"checkpoint contents"
    );
    let collision = Command::new(executable)
        .arg("restore")
        .arg(&backup)
        .arg("--destination")
        .arg(&destination)
        .output()
        .unwrap();
    assert!(!collision.status.success());
    assert_eq!(
        Store::open(&destination).unwrap().read_blob(&hash).unwrap(),
        b"checkpoint contents"
    );
}
