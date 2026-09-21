use axum::{body::Body, extract::State, http::header, routing::post, Json, Router};
use forja_core::{
    agent::Engine,
    contracts::{Mode, Provider, Run, Session, Workspace},
    models, now,
    storage::Store,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn capability_probe_requires_the_requested_tool_and_schema() {
    for valid in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route("/chat/completions", post(move |Json(request): Json<serde_json::Value>| async move {
            let tool = &request["tools"][0]["function"];
            let nonce = tool["parameters"]["properties"]["value"]["const"].clone();
            let arguments = json!({"value":if valid {nonce} else {json!("wrong-nonce")}}).to_string();
            ([(header::CONTENT_TYPE, "text/event-stream")], format!("data: {}\n\n", json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"probe-call","function":{"name":tool["name"],"arguments":arguments}}]},"finish_reason":"tool_calls"}]})))
        }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = Provider {
            id: "probe-test".into(),
            name: "Fixture".into(),
            kind: "openai-compatible".into(),
            base_url: format!("http://{address}"),
            local_only: true,
            secret_ref: None,
        };
        let report = models::probe_tools(&provider, "fixture", CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            report["status"],
            if valid { "verified" } else { "not_observed" }
        );
        assert_eq!(report["executed_tools"], 0);
        server.abort();
    }
}

async fn server(body: String, ollama: bool) -> (Provider, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            if ollama {
                "/api/chat"
            } else {
                "/chat/completions"
            },
            post(|State(body): State<String>| async move {
                let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
                    body.bytes().map(|b| Ok(vec![b])).collect();
                (
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    Body::from_stream(futures_util::stream::iter(chunks)),
                )
            }),
        )
        .with_state(body);
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        Provider {
            id: "p".into(),
            name: "Contract fixture".into(),
            kind: if ollama {
                "ollama"
            } else {
                "openai-compatible"
            }
            .into(),
            base_url: format!("http://{addr}"),
            local_only: true,
            secret_ref: None,
        },
        task,
    )
}
#[tokio::test]
async fn final_record_without_newline_and_split_utf8_are_preserved() {
    for ollama in [true, false] {
        let body = if ollama {
            format!(
                "{}\n{}",
                json!({"message":{"content":"ação🔥"}}),
                json!({"done":true,"done_reason":"stop"})
            )
        } else {
            format!(
                "data: {}\r\n\r\ndata: {}",
                json!({"choices":[{"delta":{"content":"ação🔥"}}]}),
                json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
            )
        };
        let (p, task) = server(body, ollama).await;
        let answer = models::generate(
            &p,
            "fixture",
            &[],
            &[],
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        assert_eq!(answer.text, "ação🔥");
        assert!(answer.calls.is_empty());
        task.abort();
    }
}
#[tokio::test]
async fn interrupted_valid_tool_never_reaches_approval_or_filesystem() {
    for ollama in [true, false] {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("existing.txt"), "user work").unwrap();
        let tool = json!({"id":"call","index":0,"function":{"name":"fs__apply_patch","arguments":if ollama {json!({"path":"existing.txt","base_hash":forja_core::hash(b"user work"),"old_text":"user work","new_text":"changed"})} else {json!(json!({"path":"existing.txt","base_hash":forja_core::hash(b"user work"),"old_text":"user work","new_text":"changed"}).to_string())}}});
        let delta = json!({"content":"Resposta parcial","tool_calls":[tool]});
        let body = if ollama {
            json!({"message":delta}).to_string() + "\n"
        } else {
            format!("data: {}\n\n", json!({"choices":[{"delta":delta}]}))
        };
        let (p, task) = server(body, ollama).await;
        let store = Arc::new(Store::open(data.path()).unwrap());
        let workspace = Workspace {
            id: "w".into(),
            name: "fixture".into(),
            root: project.path().to_string_lossy().into_owned(),
            created_at: now(),
        };
        let session = Session {
            id: "s".into(),
            workspace_id: "w".into(),
            title: "test".into(),
            mode: Mode::Agent,
            provider_id: "p".into(),
            model: "fixture".into(),
            created_at: now(),
            updated_at: forja_core::now(),
            executor_profile_id: None,
            reviewer_profile_ids: vec![],
            reasoning_level: None,
            last_run_id: None,
            archived: false,
        };
        store.put("workspace", "w", &workspace).unwrap();
        store.put("provider", "p", &p).unwrap();
        store.put("session", "s", &session).unwrap();
        let engine = Engine::new(store.clone());
        let run = engine.start("s", "Edit the file").unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let r: Run = store.get("run", &run.id).unwrap();
                if r.state == "failed" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let events = store.events("s", None, 0).unwrap();
        assert!(events
            .iter()
            .any(|e| e.r#type == "message.delta" && e.payload["text"] == "Resposta parcial"));
        assert!(!events.iter().any(|e| [
            "tool.started",
            "tool.approval_required",
            "message.completed"
        ]
        .contains(&e.r#type.as_str())));
        assert_eq!(
            std::fs::read_to_string(project.path().join("existing.txt")).unwrap(),
            "user work"
        );
        task.abort();
    }
}
