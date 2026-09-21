use axum::{extract::State, http::header, routing::post, Json, Router};
use forja_core::{
    agent::Engine,
    contracts::{Mode, Provider, Session, Workspace},
    hooks::Hook,
    storage::Store,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

async fn model(
    State(step): State<Arc<AtomicUsize>>,
    Json(_): Json<Value>,
) -> ([(header::HeaderName, &'static str); 1], String) {
    let message = if step.fetch_add(1, Ordering::SeqCst) == 0 {
        json!({"tool_calls":[{"function":{"name":"fs__apply_patch","arguments":{"path":"result.txt","base_hash":"","old_text":"","new_text":"created"}}},{"function":{"name":"fs__read_text","arguments":{"path":"previous.txt"}}}]})
    } else {
        json!({"content":"Finished fixture"})
    };
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        format!("{}\n{}\n", json!({"message":message}), json!({"done":true})),
    )
}

#[tokio::test]
async fn hooks_enforce_approval_order_failure_revocation_and_modes() {
    for scenario in [
        "success",
        "deny",
        "before_fail",
        "after_fail",
        "revoke",
        "plan",
        "tool_deny",
        "cancel",
        "timeout",
        "offline",
    ] {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("previous.txt"), "user work").unwrap();
        let step = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = step.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/api/chat", post(model))
                    .with_state(state),
            )
            .await
            .unwrap()
        });
        let store = Arc::new(Store::open(data.path()).unwrap());
        store
            .put(
                "workspace",
                "w",
                &Workspace {
                    id: "w".into(),
                    name: "Test".into(),
                    root: root.path().to_string_lossy().into(),
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
                    name: "Test".into(),
                    kind: "ollama".into(),
                    base_url: format!("http://{address}"),
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
                    title: "Hooks".into(),
                    mode: if scenario == "plan" {
                        Mode::Plan
                    } else {
                        Mode::Agent
                    },
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
        for (id, event) in [("a", "before_tool"), ("b", "after_tool")] {
            let command = if (scenario == "before_fail" && id == "a")
                || (scenario == "after_fail" && id == "b")
            {
                "node -e \"process.exit(7)\"".into()
            } else if ["cancel", "timeout"].contains(&scenario) && id == "a" {
                "node -e \"console.log('HOOK_RUNNING');setTimeout(()=>require('node:fs').writeFileSync('too-late.txt','bad'),10000)\"".into()
            } else {
                format!("node -e \"require('node:fs').appendFileSync('hooks.log','{id}')\"")
            };
            let hook = Hook {
                id: id.into(),
                workspace_id: "w".into(),
                name: id.into(),
                event: event.into(),
                tools: vec![if scenario == "plan" {
                    "fs.read_text".into()
                } else {
                    "fs.apply_patch".into()
                }],
                command,
                cwd: ".".into(),
                timeout_seconds: if scenario == "timeout" { 1 } else { 15 },
                enabled: true,
                position: 0,
                revision: 1,
            };
            store.put("hook", id, &hook).unwrap();
        }
        let engine = Engine::new(store.clone());
        let mut events = engine.events.subscribe();
        let run = engine.start("s", "Make file").unwrap();
        let end = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let e = events.recv().await.unwrap();
                if e.r#type == "tool.approval_required" {
                    let id = e.payload["id"].as_str().unwrap();
                    let hook = e.payload["tool"]["name"] == "hook.exec";
                    if hook {
                        assert!(engine.decide(id, "allow_session").is_err());
                        assert!(
                            e.payload["tool"]["arguments"]["config_hash"]
                                .as_str()
                                .unwrap()
                                .len()
                                == 64
                        );
                        if scenario == "revoke" {
                            store.delete("hook", "a").unwrap();
                            engine.revoke_hook_approvals("a").unwrap();
                            continue;
                        }
                        if scenario == "offline" {
                            engine.offline.store(true, Ordering::SeqCst);
                        }
                    }
                    engine
                        .decide(
                            id,
                            if (hook && scenario == "deny") || (!hook && scenario == "tool_deny") {
                                "deny"
                            } else {
                                "allow_once"
                            },
                        )
                        .unwrap();
                }
                if scenario == "cancel" && e.r#type == "hook.output" {
                    engine.action(&run.id, "cancel").unwrap()
                }
                if ["run.completed", "run.failed", "run.cancelled"].contains(&e.r#type.as_str()) {
                    break e.r#type;
                }
            }
        })
        .await
        .unwrap();
        let history: Vec<forja_core::contracts::Message> = store.get("history", "s").unwrap();
        assert_eq!(
            history.iter().map(|m| m.tool_calls.len()).sum::<usize>(),
            history.iter().filter(|m| m.role == "tool").count(),
            "Incomplete tool batch after {scenario}"
        );
        let recorded = store.events("s", Some(&run.id), 0).unwrap();
        let types: Vec<_> = recorded.iter().map(|e| e.r#type.as_str()).collect();
        let created = root.path().join("result.txt").exists();
        assert_eq!(
            created,
            ["success", "after_fail"].contains(&scenario),
            "{scenario}"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("previous.txt")).unwrap(),
            "user work"
        );
        assert!(!root.path().join("too-late.txt").exists());
        assert!(engine.grants.lock().unwrap().is_empty());
        if scenario == "success" {
            assert_eq!(
                std::fs::read_to_string(root.path().join("hooks.log")).unwrap(),
                "ab"
            );
            let first_hook = types.iter().position(|t| *t == "hook.completed").unwrap();
            let started = types.iter().position(|t| *t == "tool.started").unwrap();
            let completed = types.iter().position(|t| *t == "tool.completed").unwrap();
            let last_hook = types.iter().rposition(|t| *t == "hook.started").unwrap();
            assert!(first_hook < started && completed < last_hook);
        }
        if ["success", "plan", "tool_deny"].contains(&scenario) {
            assert_eq!(end, "run.completed")
        } else {
            assert_eq!(
                end,
                if scenario == "cancel" {
                    "run.cancelled"
                } else {
                    "run.failed"
                }
            );
            assert_eq!(
                step.load(Ordering::SeqCst),
                1,
                "Failed hook must stop inference and prevent retries"
            );
        }
        if ["plan", "tool_deny"].contains(&scenario) {
            assert!(!types.contains(&"hook.started"))
        }
        if ["deny", "revoke"].contains(&scenario) {
            assert!(!root.path().join("hooks.log").exists())
        }
        server.abort();
    }
}
