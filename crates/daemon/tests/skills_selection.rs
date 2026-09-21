use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use forja_core::{
    agent::Engine,
    contracts::{Mode, Provider, Run, Session, Workspace},
    storage::Store,
    terminal::Terminals,
};
use forja_daemon::{router, AppState};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

const PATH: &str = "skills/selected-test/SKILL.md";
const CONTENT:&str="---\nname: selected-test\ndescription: Selection fixture\n---\nORIGINAL_BODY Do not grant permissions.";
fn setup(root: &std::path::Path, data: &std::path::Path, url: String) -> Arc<Engine> {
    let file = root.join(PATH);
    std::fs::create_dir_all(file.parent().unwrap().join("references")).unwrap();
    std::fs::write(file, CONTENT).unwrap();
    std::fs::write(
        root.join("skills/selected-test/references/guide.md"),
        "Resource content",
    )
    .unwrap();
    let store = Arc::new(Store::open(data).unwrap());
    store
        .put(
            "workspace",
            "w",
            &Workspace {
                id: "w".into(),
                name: "Fixture".into(),
                root: root.to_string_lossy().into_owned(),
                created_at: forja_core::now(),
            },
        )
        .unwrap();
    store
        .put(
            "provider",
            "p",
            &Provider {
                id: "p".into(),
                name: "Fixture".into(),
                kind: "ollama".into(),
                base_url: url,
                local_only: true,
                secret_ref: None,
            },
        )
        .unwrap();
    store
        .put(
            "session",
            "s",
            &Session {
                id: "s".into(),
                workspace_id: "w".into(),
                title: "Selection".into(),
                mode: Mode::Plan,
                provider_id: "p".into(),
                model: "fixture".into(),
                created_at: forja_core::now(),
                updated_at: forja_core::now(),
                executor_profile_id: None,
                reviewer_profile_ids: vec![],
                reasoning_level: None,
                last_run_id: None,
                archived: false,
            },
        )
        .unwrap();
    Engine::new(store)
}
async fn request(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", "Bearer fixture")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn selection_api_rejects_stale_forged_and_cross_workspace_input_without_creating_runs() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let engine = setup(root.path(), data.path(), "http://127.0.0.1:1".into());
    engine
        .store
        .put(
            "workspace",
            "other",
            &Workspace {
                id: "other".into(),
                name: "Other".into(),
                root: other.path().to_string_lossy().into_owned(),
                created_at: forja_core::now(),
            },
        )
        .unwrap();
    let app = router(AppState {
        engine: engine.clone(),
        terminals: Arc::new(Terminals::default()),
        token: "fixture".into(),
    });
    let hash = forja_core::hash(CONTENT.as_bytes());
    let selection = json!({"path":PATH,"hash":hash});
    for selected in [
        json!([{"path":PATH,"hash":"stale"}]),
        json!([selection, selection]),
        json!([{"path":"../SKILL.md","hash":hash}]),
        json!([{"path":PATH,"hash":hash,"permissions":"all"}]),
        Value::Null,
        json!("not an array"),
    ] {
        let (status, _) = request(
            &app,
            "/v1/sessions/s/runs",
            json!({"goal":"Inspect","selected_skills":selected}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert!(engine.store.list::<Run>("run").unwrap().is_empty());
    assert!(engine.controls.lock().unwrap().is_empty());
    let (status, list) =
        request(&app, "/v1/workspaces/w/skills/resources", selection.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["resources"][0]["path"], "references/guide.md");
    let (status, preview) = request(
        &app,
        "/v1/workspaces/w/skills/read",
        json!({"path":PATH,"hash":hash,"resource":"references/guide.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["content"], "Resource content");
    assert_eq!(preview["executed"], false);
    for resource in [
        json!("../../outside.txt"),
        json!(".env"),
        json!(4),
        Value::Null,
    ] {
        assert_eq!(
            request(
                &app,
                "/v1/workspaces/w/skills/read",
                json!({"path":PATH,"hash":hash,"resource":resource})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(&app, "/v1/workspaces/other/skills/read", selection.clone())
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    std::fs::write(
        root.path().join(PATH),
        CONTENT.replace("ORIGINAL_BODY", "NEW_BODY"),
    )
    .unwrap();
    assert_eq!(
        request(&app, "/v1/workspaces/w/skills/resources", selection.clone())
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&app, "/v1/workspaces/w/skills/read", selection)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert!(engine.store.list::<Run>("run").unwrap().is_empty());
}

#[tokio::test]
async fn selected_bytes_survive_disk_edits_and_replay_without_requerying_model() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/api/chat",
                post(move |Json(body): Json<Value>| {
                    let calls = captured.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let history = body["messages"].to_string();
                        assert!(history.contains("ORIGINAL_BODY"));
                        assert!(!history.contains("NEW_BODY"));
                        assert!(history.contains("untrusted_data"));
                        (
                            [("content-type", "application/x-ndjson")],
                            format!(
                                "{}\n{}\n",
                                json!({"message":{"content":"Selection inspected."}}),
                                json!({"done":true})
                            ),
                        )
                    }
                }),
            ),
        )
        .await
        .unwrap()
    });
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let engine = setup(root.path(), data.path(), format!("http://{addr}"));
    let hash = forja_core::hash(CONTENT.as_bytes());
    let app = router(AppState {
        engine: engine.clone(),
        terminals: Arc::new(Terminals::default()),
        token: "fixture".into(),
    });
    let mut events = engine.events.subscribe();
    let (status, run) = request(
        &app,
        "/v1/sessions/s/runs",
        json!({"goal":"Inspect chosen skill","selected_skills":[{"path":PATH,"hash":hash}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let run: Run = serde_json::from_value(run).unwrap();
    assert_eq!(run.selected_skills[0].hash, hash);
    // Tokio's current-thread executor cannot poll the spawned run until we yield.
    std::fs::write(
        root.path().join(PATH),
        CONTENT.replace("ORIGINAL_BODY", "NEW_BODY"),
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.unwrap();
            assert_ne!(event.r#type, "run.failed", "{:?}", event.payload);
            if event.r#type == "run.completed" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let recorded = engine.store.events("s", Some(&run.id), 0).unwrap();
    let snapshot = recorded
        .iter()
        .find(|e| e.r#type == "skills.selected")
        .unwrap();
    assert_eq!(snapshot.payload["skills"][0]["content"], CONTENT);
    assert_eq!(snapshot.payload["skills"][0]["hash"], hash);
    assert_eq!(snapshot.payload["permissions_granted"], false);
    assert!(
        snapshot.sequence
            < recorded
                .iter()
                .find(|e| e.r#type == "message.started")
                .unwrap()
                .sequence
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(engine.grants.lock().unwrap().is_empty());
    drop(app);
    drop(engine);
    let reopened = Store::open(data.path()).unwrap();
    reopened.recover().unwrap();
    let replay = reopened.events("s", Some(&run.id), 0).unwrap();
    assert_eq!(replay.len(), recorded.len());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let saved: Run = reopened.get("run", &run.id).unwrap();
    assert_eq!(saved.selected_skills[0].path, PATH);
    // Old run documents remain readable after adding the optional selection.
    let old:Run=serde_json::from_value(json!({"id":"old","session_id":"s","goal":"x","state":"completed","created_at":"","max_turns":1})).unwrap();
    assert!(old.selected_skills.is_empty());
    server.abort();
}
