use axum::{extract::State, http::header, routing::post, Json, Router};
use forja_core::{
    agent::Engine,
    contracts::{Event, Message, Mode, Provider, Session, Workspace},
    hooks::Hook,
    storage::Store,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
struct Model {
    requests: Arc<AtomicUsize>,
    operation: &'static str,
    mcp_hash: Arc<Mutex<String>>,
}
async fn model(
    State(f): State<Model>,
    Json(request): Json<Value>,
) -> ([(header::HeaderName, &'static str); 1], String) {
    assert!(!request["tools"].to_string().contains("hook__exec"));
    let message = if f.requests.fetch_add(1, Ordering::SeqCst) == 0 {
        let call = if f.operation == "exit" {
            json!({"name":"terminal__exec","arguments":{"command":"node -e \"process.exit(7)\"","cwd":".","timeout_seconds":10}})
        } else if f.operation == "mcp" {
            json!({"name":"mcp__call","arguments":{"server_id":"fixture","name":"fail","arguments":{},"catalog_hash":f.mcp_hash.lock().unwrap().clone()}})
        } else {
            json!({"name":"fs__read_text","arguments":{"path":if f.operation=="missing"{"missing.txt"}else{"previous.txt"}}})
        };
        json!({"tool_calls":[{"function":call}]})
    } else {
        json!({"content":"Resposta final da fixture"})
    };
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        format!("{}\n{}\n", json!({"message":message}), json!({"done":true})),
    )
}
async fn mcp_error(Json(request): Json<Value>) -> Json<Value> {
    let result = match request["method"].as_str().unwrap() {
        "server/discover" => {
            json!({"capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}})
        }
        "tools/list" => {
            json!({"tools":[{"name":"fail","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}]})
        }
        "tools/call" => {
            json!({"isError":true,"content":[{"type":"text","text":"Expected fixture error"}]})
        }
        _ => panic!("Unexpected MCP method"),
    };
    Json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
}
struct Fixture {
    root: tempfile::TempDir,
    _data: tempfile::TempDir,
    store: Arc<Store>,
    engine: Arc<Engine>,
    requests: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new(mode: Mode, operation: &'static str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("previous.txt"), "user work").unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let mcp_hash = Arc::new(Mutex::new(String::new()));
        let model = Model {
            requests: requests.clone(),
            operation,
            mcp_hash: mcp_hash.clone(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/api/chat", post(crate::model))
                    .route("/mcp", post(mcp_error))
                    .with_state(model),
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
                    name: "Fixture".into(),
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
                    name: "Fixture".into(),
                    kind: "ollama".into(),
                    base_url: format!("http://{addr}"),
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
                    title: "Lifecycle".into(),
                    mode,
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
        if operation == "mcp" {
            let catalog = engine
                .mcp
                .connect(&forja_core::mcp::Config {
                    id: "fixture".into(),
                    name: "Test fixture".into(),
                    transport: "http".into(),
                    protocol_version: "2026-07-28".into(),
                    url: format!("http://{addr}/mcp"),
                    command: String::new(),
                    args: vec![],
                    cwd: String::new(),
                    secret_ref: None,
                })
                .await
                .unwrap();
            *mcp_hash.lock().unwrap() = catalog["catalog_hash"].as_str().unwrap().into();
        }
        Self {
            root,
            _data: data,
            store,
            engine,
            requests,
            server,
        }
    }
    fn hook(&self, event: &str, tools: Vec<&str>, fail: bool) {
        let h = Hook {
            id: event.into(),
            workspace_id: "w".into(),
            name: event.into(),
            event: event.into(),
            tools: tools.into_iter().map(str::to_owned).collect(),
            command: if fail {
                "node -e \"process.exit(7)\"".into()
            } else {
                format!("node -e \"require('node:fs').appendFileSync('hooks.log','{event};')\"")
            },
            cwd: ".".into(),
            timeout_seconds: 10,
            enabled: true,
            position: 0,
            revision: 1,
        };
        h.validate(self.root.path()).unwrap();
        self.store.put("hook", &h.id, &h).unwrap();
    }
    async fn run(&self, target: &str, action: &str) -> Vec<Event> {
        let mut events = self.engine.events.subscribe();
        let run = self.engine.start("s", "Inspect the project").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(25), async {
            loop {
                let e = events.recv().await.unwrap();
                if e.r#type == "tool.approval_required" {
                    let id = e.payload["id"].as_str().unwrap();
                    let a = &e.payload["tool"]["arguments"];
                    let hook = e.payload["tool"]["name"] == "hook.exec";
                    let matched = if target == "terminal.exec" {
                        !hook
                    } else {
                        hook && a["event"] == target
                    };
                    if hook {
                        assert!(self.engine.decide(id, "allow_session").is_err());
                        if [
                            "before_model_request",
                            "after_model_response",
                            "before_final",
                        ]
                        .contains(&a["event"].as_str().unwrap())
                        {
                            assert!(a["trigger_tool"].is_null());
                            assert!(a["trigger_call_id"].is_null());
                        }
                    }
                    if matched && action == "cancel" {
                        self.engine.action(&run.id, "cancel").unwrap();
                        continue;
                    }
                    if matched && action == "revoke" {
                        self.store.delete("hook", target).unwrap();
                        self.engine.revoke_hook_approvals(target).unwrap();
                        continue;
                    }
                    if matched && action == "offline" {
                        self.engine.offline.store(true, Ordering::SeqCst)
                    }
                    self.engine
                        .decide(
                            id,
                            if matched && action == "deny" {
                                "deny"
                            } else {
                                "allow_once"
                            },
                        )
                        .unwrap();
                }
                if ["run.completed", "run.failed", "run.cancelled"].contains(&e.r#type.as_str()) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let history: Vec<Message> = self.store.get("history", "s").unwrap();
        assert_eq!(
            history.iter().map(|m| m.tool_calls.len()).sum::<usize>(),
            history.iter().filter(|m| m.role == "tool").count(),
            "No pending tool calls after {target}/{action}"
        );
        assert!(self.engine.grants.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(self.root.path().join("previous.txt")).unwrap(),
            "user work"
        );
        self.store.events("s", Some(&run.id), 0).unwrap()
    }
}
#[tokio::test]
async fn lifecycle_hooks_run_in_order_for_each_model_round() {
    let f = Fixture::new(Mode::Agent, "read").await;
    for event in [
        "before_model_request",
        "after_model_response",
        "before_final",
    ] {
        f.hook(event, vec![], false)
    }
    let events = f.run("", "").await;
    assert_eq!(f.requests.load(Ordering::SeqCst), 2);
    assert_eq!(std::fs::read_to_string(f.root.path().join("hooks.log")).unwrap(),"before_model_request;after_model_response;before_model_request;after_model_response;before_final;");
    let order: Vec<_> = events
        .iter()
        .filter_map(|e| match e.r#type.as_str() {
            "hook.completed" => Some(e.payload["event"].as_str().unwrap()),
            "message.started" | "message.completed" | "tool.started" | "run.completed" => {
                Some(e.r#type.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        vec![
            "before_model_request",
            "message.started",
            "message.completed",
            "after_model_response",
            "tool.started",
            "before_model_request",
            "message.started",
            "message.completed",
            "after_model_response",
            "before_final",
            "run.completed"
        ]
    );
}
#[tokio::test]
async fn lifecycle_gates_stop_network_tools_or_completion_without_retry() {
    for target in [
        "before_model_request",
        "after_model_response",
        "before_final",
    ] {
        for action in ["deny", "cancel", "revoke", "offline", "fail"] {
            let f = Fixture::new(Mode::Agent, "read").await;
            f.hook(target, vec![], action == "fail");
            let events = f.run(target, action).await;
            assert_eq!(
                f.requests.load(Ordering::SeqCst),
                match target {
                    "before_model_request" => 0,
                    "after_model_response" => 1,
                    _ => 2,
                },
                "{target}/{action}"
            );
            assert_eq!(
                events.iter().filter(|e| e.r#type == "tool.started").count(),
                if target == "before_final" { 1 } else { 0 }
            );
            assert!(!events.iter().any(|e| e.r#type == "run.completed"));
            assert!(events.iter().any(|e| e.r#type
                == if action == "cancel" {
                    "run.cancelled"
                } else {
                    "run.failed"
                }));
            assert!(!f.root.path().join("hooks.log").exists());
        }
    }
}
#[tokio::test]
async fn read_only_modes_do_not_execute_lifecycle_commands() {
    for mode in [Mode::Consult, Mode::Plan] {
        let f = Fixture::new(mode, "read").await;
        for event in [
            "before_model_request",
            "after_model_response",
            "before_final",
        ] {
            f.hook(event, vec![], false)
        }
        let events = f.run("", "").await;
        assert_eq!(f.requests.load(Ordering::SeqCst), 2);
        assert!(!events
            .iter()
            .any(|e| e.r#type == "tool.approval_required" || e.r#type == "hook.started"));
        assert!(!f.root.path().join("hooks.log").exists());
    }
}
#[tokio::test]
async fn tool_error_hooks_see_process_failures_but_never_bypass_denial() {
    for operation in ["exit", "missing", "denied", "mcp"] {
        let f = Fixture::new(
            Mode::Agent,
            if operation == "denied" {
                "exit"
            } else {
                operation
            },
        )
        .await;
        let tool = if operation == "missing" {
            "fs.read_text"
        } else if operation == "mcp" {
            "mcp.call"
        } else {
            "terminal.exec"
        };
        f.hook("after_tool", vec![tool], false);
        f.hook("tool_error", vec![tool], false);
        let events = f
            .run(
                "terminal.exec",
                if operation == "denied" { "deny" } else { "" },
            )
            .await;
        let result = events
            .iter()
            .find(|e| e.r#type == "tool.completed")
            .unwrap();
        assert_eq!(result.payload["output"]["success"], false);
        if operation == "denied" {
            assert!(!f.root.path().join("hooks.log").exists())
        } else {
            assert_eq!(
                std::fs::read_to_string(f.root.path().join("hooks.log")).unwrap(),
                "after_tool;tool_error;"
            )
        }
        assert!(events.iter().any(|e| e.r#type == "run.completed"));
    }
}

#[tokio::test]
async fn reordering_keeps_active_run_order_and_changes_the_next_run() {
    let f = Fixture::new(Mode::Agent, "read").await;
    for (position, id) in ["a", "b"].into_iter().enumerate() {
        let hook = Hook {
            id: id.into(),
            workspace_id: "w".into(),
            name: id.into(),
            event: "before_model_request".into(),
            tools: vec![],
            command: format!("node -e \"require('node:fs').appendFileSync('order.log','{id}')\""),
            cwd: ".".into(),
            timeout_seconds: 10,
            enabled: true,
            position: position as u32,
            revision: 1,
        };
        f.store.put("hook", id, &hook).unwrap();
    }
    for round in 0..2 {
        let mut events = f.engine.events.subscribe();
        let run = f.engine.start("s", "Inspect").unwrap();
        let mut changed = false;
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.r#type == "tool.approval_required" {
                    if round == 0 && !changed {
                        let before = forja_core::hooks::list(&f.store, "w").unwrap();
                        let expected: Vec<_> = before
                            .iter()
                            .map(|h| forja_core::hooks::Version {
                                id: h.id.clone(),
                                revision: h.revision,
                                position: h.position,
                            })
                            .collect();
                        forja_core::hooks::reorder(
                            &f.store,
                            "w",
                            &["b".into(), "a".into()],
                            &expected,
                        )
                        .unwrap();
                        changed = true;
                    }
                    f.engine
                        .decide(event.payload["id"].as_str().unwrap(), "allow_once")
                        .unwrap();
                }
                assert_ne!(event.r#type, "run.failed", "{:?}", event.payload);
                if event.r#type == "run.completed" {
                    break;
                }
            }
        })
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while f.engine.controls.lock().unwrap().contains_key(&run.id) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let saved = f.store.events("s", Some(&run.id), 0).unwrap();
        let snapshot = saved.iter().find(|e| e.r#type == "hooks.snapshot").unwrap();
        assert_eq!(
            snapshot.payload["hooks"][0]["id"],
            if round == 0 { "a" } else { "b" }
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("order.log")).unwrap(),
        "ababba"
    );
}
