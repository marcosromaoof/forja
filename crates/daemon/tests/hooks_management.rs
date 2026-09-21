use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use forja_core::{
    agent::Engine,
    contracts::{Approval, Mode, Provider, Session, Workspace},
    hooks::{self, Hook},
    storage::Store,
    terminal::Terminals,
};
use forja_daemon::{router, AppState};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
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
fn configuration(name: &str) -> Value {
    json!({"id":"","workspace_id":"w","name":name,"event":"before_model_request","tools":[],"command":"node -e \"require('node:fs').writeFileSync('never.txt','bad')\"","cwd":".","timeout_seconds":10,"enabled":true,"position":0,"revision":1})
}
#[tokio::test]
async fn management_api_rejects_stale_edits_and_revokes_waiting_approvals() {
    for action in ["edit", "disable", "delete"] {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(data.path()).unwrap());
        store
            .put(
                "workspace",
                "w",
                &Workspace {
                    id: "w".into(),
                    name: "Fixture".into(),
                    root: root.path().to_string_lossy().into_owned(),
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
                    name: "Never contacted".into(),
                    kind: "ollama".into(),
                    base_url: "http://127.0.0.1:1".into(),
                    secret_ref: None,
                    local_only: true,
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
                    title: "Fixture".into(),
                    mode: Mode::Agent,
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
        let engine = Engine::new(store.clone());
        let app = router(AppState {
            engine: engine.clone(),
            terminals: Arc::new(Terminals::default()),
            token: "fixture".into(),
        });
        let (status, original) = request(
            &app,
            "POST",
            "/v1/workspaces/w/hooks",
            configuration("First"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let hook_id = original["id"].as_str().unwrap();
        let path = format!("/v1/workspaces/w/hooks/{hook_id}");
        let mut events = engine.events.subscribe();
        let run = engine.start("s", "Never send this request").unwrap();
        let approval = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let e = events.recv().await.unwrap();
                if e.r#type == "tool.approval_required" {
                    break e.payload;
                }
            }
        })
        .await
        .unwrap();
        let mut changed = original.clone();
        changed["command"] = json!("node -e \"process.exit(0)\"");
        if action == "disable" {
            changed["enabled"] = json!(false);
        }
        let (status, _) = if action == "delete" {
            request(&app, "DELETE", &path, json!({"expected_revision":1})).await
        } else {
            request(
                &app,
                "PUT",
                &path,
                json!({"hook":changed,"expected_revision":1}),
            )
            .await
        };
        assert_eq!(status, StatusCode::OK);
        let pending: Approval = store
            .get("approval", approval["id"].as_str().unwrap())
            .unwrap();
        assert_eq!(pending.state, "revoked");
        assert!(engine.decide(&pending.id, "allow_once").is_err());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let e = events.recv().await.unwrap();
                if e.r#type == "run.failed" {
                    break;
                }
                assert_ne!(e.r#type, "hook.started");
                assert_ne!(e.r#type, "message.started");
            }
        })
        .await
        .unwrap();
        assert!(!root.path().join("never.txt").exists());
        let history = store.events("s", Some(&run.id), 0).unwrap();
        assert!(history.iter().any(|e| e.r#type == "approval.revoked"));
        let (status, _) = request(
            &app,
            "PUT",
            &path,
            json!({"hook":original,"expected_revision":1}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        if action != "delete" {
            let (status, _) = request(&app, "DELETE", &path, json!({"expected_revision":1})).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let saved: Hook = store.get("hook", hook_id).unwrap();
            assert_eq!(saved.revision, 2);
            if action == "edit" {
                // An invalidation racing with a new run must keep approvals of the new revision.
                let fresh = Approval {
                    id: "fresh".into(),
                    session_id: "s".into(),
                    run_id: "next".into(),
                    tool: saved.approval(None).unwrap(),
                    scope_key: "fresh".into(),
                    state: "pending".into(),
                    created_at: forja_core::now(),
                };
                store.put("approval", "fresh", &fresh).unwrap();
                let (tx, _rx) = tokio::sync::oneshot::channel();
                engine.approvals.lock().unwrap().insert("fresh".into(), tx);
                engine.revoke_hook_approvals(hook_id).unwrap();
                assert_eq!(
                    store.get::<Approval>("approval", "fresh").unwrap().state,
                    "pending"
                );
                assert!(engine.approvals.lock().unwrap().contains_key("fresh"));
            }
        }
    }
}
#[tokio::test]
async fn reorder_is_atomic_and_disabled_hooks_are_not_scheduled() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(data.path()).unwrap());
    store
        .put(
            "workspace",
            "w",
            &Workspace {
                id: "w".into(),
                name: "Fixture".into(),
                root: root.path().to_string_lossy().into_owned(),
                created_at: forja_core::now(),
            },
        )
        .unwrap();
    let engine = Engine::new(store.clone());
    let app = router(AppState {
        engine: engine.clone(),
        terminals: Arc::new(Terminals::default()),
        token: "fixture".into(),
    });
    let (_, a) = request(&app, "POST", "/v1/workspaces/w/hooks", configuration("A")).await;
    let (_, b) = request(&app, "POST", "/v1/workspaces/w/hooks", configuration("B")).await;
    let (_, all) = request(&app, "GET", "/v1/workspaces/w/hooks", Value::Null).await;
    let expected: Vec<_> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|h| json!({"id":h["id"],"revision":h["revision"],"position":h["position"]}))
        .collect();
    let order = json!({"ids":[b["id"],a["id"]],"expected":expected});
    let (status, reordered) =
        request(&app, "POST", "/v1/workspaces/w/hooks/order", order.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reordered[0]["id"], b["id"]);
    let (status, _) = request(&app, "POST", "/v1/workspaces/w/hooks/order", order).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for mut hook in hooks::list(&store, "w").unwrap() {
        hook.enabled = false;
        let path = format!("/v1/workspaces/w/hooks/{}", hook.id);
        let (status, _) = request(
            &app,
            "PUT",
            &path,
            json!({"hook":hook,"expected_revision":1}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    store
        .put(
            "provider",
            "p",
            &Provider {
                id: "p".into(),
                name: "No server".into(),
                kind: "ollama".into(),
                base_url: "http://127.0.0.1:1".into(),
                secret_ref: None,
                local_only: true,
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
                title: "Fixture".into(),
                mode: Mode::Agent,
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
    engine
        .offline
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut events = engine.events.subscribe();
    let run = engine
        .start("s", "Disabled hooks stay out of snapshots")
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while events.recv().await.unwrap().r#type != "run.failed" {}
    })
    .await
    .unwrap();
    assert!(!store
        .events("s", Some(&run.id), 0)
        .unwrap()
        .iter()
        .any(|e| e.r#type == "hooks.snapshot"
            || e.r#type == "hook.started"
            || e.r#type == "tool.approval_required"));
    assert!(!root.path().join("never.txt").exists());
}
