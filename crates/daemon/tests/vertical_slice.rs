use axum::{extract::State, http::header, routing::post, Router};
use forja_core::{
    agent::Engine,
    contracts::{Mode, Provider, Run, Session, Workspace},
    hash, now,
    storage::Store,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[derive(Clone)]
struct Fake {
    step: Arc<AtomicUsize>,
    hash: String,
}
async fn model(State(f): State<Fake>) -> ([(header::HeaderName, &'static str); 1], String) {
    let step = f.step.fetch_add(1, Ordering::SeqCst);
    let message = match step {
        0 => {
            json!({"content":"Vou ler o cálculo e verificar o teste.","tool_calls":[{"function":{"name":"fs__read_text","arguments":{"path":"bandwidth.cjs"}}}]})
        }
        1 => {
            json!({"tool_calls":[{"function":{"name":"fs__apply_patch","arguments":{"path":"bandwidth.cjs","base_hash":f.hash,"old_text":"rate * bus / 4","new_text":"rate * bus / 8"}}}]})
        }
        2 => {
            json!({"tool_calls":[{"function":{"name":"terminal__exec","arguments":{"command":"node --test bandwidth.test.cjs","cwd":".","timeout_seconds":20}}}]})
        }
        _ => json!({"content":"Corrigido. Teste executado; alterações preexistentes preservadas."}),
    };
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        format!(
            "{}\n{}\n",
            json!({"message":message}),
            json!({"done":true,"eval_count":12})
        ),
    )
}
#[tokio::test]
async fn real_patch_approval_command_and_restart() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let original="// alteração do usuário: manter este comentário\nmodule.exports = (rate,bus) => rate * bus / 4;\n";
    std::fs::write(workspace_dir.path().join("bandwidth.cjs"), original).unwrap();
    std::fs::write(workspace_dir.path().join("bandwidth.test.cjs"),"const test=require('node:test');const assert=require('node:assert/strict');test('bandwidth uses bits to bytes',()=>assert.equal(require('./bandwidth.cjs')(20,256),640));").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let fake = Fake {
        step: Arc::new(AtomicUsize::new(0)),
        hash: hash(original.as_bytes()),
    };
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/api/chat", post(model))
                .with_state(fake),
        )
        .await
        .unwrap()
    });
    let store = Arc::new(Store::open(data.path()).unwrap());
    let w = Workspace {
        id: "w".into(),
        name: "fixture".into(),
        root: workspace_dir.path().to_string_lossy().into_owned(),
        created_at: now(),
    };
    let p = Provider {
        id: "p".into(),
        name: "Fixture de teste".into(),
        kind: "ollama".into(),
        base_url: format!("http://{addr}"),
        local_only: true,
        secret_ref: None,
    };
    let s = Session {
        id: "s".into(),
        workspace_id: w.id.clone(),
        title: "Corrigir".into(),
        mode: Mode::Agent,
        provider_id: p.id.clone(),
        model: "test-fixture".into(),
        created_at: now(),
        updated_at: forja_core::now(),
        executor_profile_id: None,
        reviewer_profile_ids: vec![],
        reasoning_level: None,
        last_run_id: None,
        archived: false,
    };
    store.put("workspace", "w", &w).unwrap();
    store.put("provider", "p", &p).unwrap();
    store.put("session", "s", &s).unwrap();
    let engine = Engine::new(store.clone());
    let mut events = engine.events.subscribe();
    let run = engine
        .start("s", "Corrija o cálculo e execute o teste.")
        .unwrap();
    let mut approvals = 0;
    let mut test_passed = false;
    let mut finished = false;
    tokio::time::timeout(std::time::Duration::from_secs(45), async {
        while let Ok(e) = events.recv().await {
            match e.r#type.as_str() {
                "tool.approval_required" => {
                    approvals += 1;
                    if approvals == 1 {
                        assert_eq!(
                            std::fs::read_to_string(workspace_dir.path().join("bandwidth.cjs"))
                                .unwrap(),
                            original
                        );
                    }
                    engine
                        .decide(e.payload["id"].as_str().unwrap(), "allow_session")
                        .unwrap();
                }
                "tool.completed" if e.payload["name"] == "terminal.exec" => {
                    assert_eq!(
                        e.payload["output"]["result"]["exit_code"], 0,
                        "{}",
                        e.payload
                    );
                    assert_eq!(e.payload["output"]["result"]["success"], true);
                    test_passed = true;
                }
                "run.completed" => {
                    finished = true;
                    break;
                }
                "run.failed" => panic!("Failed: {}", e.payload),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(finished && test_passed);
    assert_eq!(approvals, 2);
    let text = std::fs::read_to_string(workspace_dir.path().join("bandwidth.cjs")).unwrap();
    assert!(text.contains("alteração do usuário"));
    assert!(text.contains("rate * bus / 8"));
    assert_eq!(store.get::<Run>("run", &run.id).unwrap().state, "completed");
    let count = store.events("s", None, 0).unwrap().len();
    drop(engine);
    drop(store);
    let reopened = Store::open(data.path()).unwrap();
    assert_eq!(reopened.recover().unwrap(), 0);
    assert_eq!(reopened.events("s", None, 0).unwrap().len(), count);
    server.abort();
}
