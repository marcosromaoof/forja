mod clone;
use axum::{
    body::Bytes,
    extract::{
        ws::{Message as WsMessage, WebSocketUpgrade},
        Path, Query, Request, State,
    },
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use base64::Engine as _;
use forja_core::{agent::Engine, contracts::*, terminal::Terminals};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path as FsPath,
    sync::{atomic::Ordering, Arc},
};
macro_rules! ensure { ($cond:expr, $($arg:tt)*) => { if !$cond { return Err(anyhow::anyhow!($($arg)*).into()); } }; }
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Engine>,
    pub terminals: Arc<Terminals>,
    pub token: String,
}
pub struct Error(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        let lower = message.to_ascii_lowercase();
        let (status, code, retryable) = if lower.contains("http 401")
            || lower.contains("http 403")
            || lower.contains("unauthorized")
        {
            (StatusCode::BAD_GATEWAY, "provider_auth_failed", false)
        } else if lower.contains("http 429") || lower.contains("rate limit") {
            (StatusCode::TOO_MANY_REQUESTS, "provider_rate_limited", true)
        } else if lower.contains("error sending request")
            || lower.contains("connection refused")
            || lower.contains("timed out")
            || lower.contains("dns")
        {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_unavailable",
                true,
            )
        } else if lower.contains("formato de catálogo")
            || lower.contains("expected value at line")
            || (lower.contains("resposta") && (lower.contains("json") || lower.contains("formato")))
        {
            (StatusCode::BAD_GATEWAY, "provider_invalid_response", true)
        } else if lower.contains("http 5") {
            (StatusCode::BAD_GATEWAY, "provider_upstream_error", true)
        } else if lower.contains("configure uma chave")
            || lower.contains("url relativa")
            || lower.contains("relative url")
            || lower.contains("tipo de provedor desconhecido")
            || lower.contains("url deve conter")
            || lower.contains("use https")
            || lower.contains("precisa usar loopback")
        {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "provider_misconfigured",
                false,
            )
        } else {
            (StatusCode::BAD_REQUEST, "request_failed", false)
        };
        (
            status,
            Json(ApiError {
                code: code.into(),
                message,
                retryable,
                details: json!({}),
                correlation_id: forja_core::id(),
            }),
        )
            .into_response()
    }
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/events", get(events_ws))
        .route("/v1/{*path}", any(api))
        .layer(axum::extract::DefaultBodyLimit::max(3_000_000))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}
async fn authenticate(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.token);
    let supplied = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let equal = supplied.len() == expected.len()
        && supplied
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |a, (b, c)| a | (b ^ c))
            == 0;
    let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
    if !equal
        || origin.is_some_and(|o| {
            ![
                "tauri://localhost",
                "http://tauri.localhost",
                "https://tauri.localhost",
            ]
            .contains(&o)
        })
    {
        return (StatusCode::UNAUTHORIZED,Json(json!({"code":"unauthorized","message":"Cliente não autorizado","retryable":false,"correlation_id":forja_core::id()}))).into_response();
    }
    next.run(req).await
}
async fn events_ws(State(s): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move|mut socket|async move{let mut rx=s.engine.events.subscribe();loop{tokio::select!{event=rx.recv()=>{match event{Ok(e)=>{if socket.send(WsMessage::Text(serde_json::to_string(&e).unwrap().into())).await.is_err(){break}},Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{let _=socket.send(WsMessage::Text(json!({"type":"replay_required"}).to_string().into())).await;},Err(_)=>break}},incoming=socket.recv()=>{if incoming.is_none(){break}}}}})
}
fn field<'a>(v: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Campo obrigatório: {key}"))
}
async fn api(
    State(s): State<AppState>,
    Path(path): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    method: Method,
    bytes: Bytes,
) -> Result<Json<Value>, Error> {
    let body: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes)?
    };
    let parts: Vec<_> = path.split('/').collect();
    let store = &s.engine.store;
    let result = match (method.as_str(), parts.as_slice()) {
        ("GET", ["health"]) => {
            json!({"name":"FORJA","version":env!("CARGO_PKG_VERSION"),"protocol":1,"offline":s.engine.offline.load(Ordering::SeqCst),"executor":"local_restricted","features":["workspace","chat","approvals","files","checkpoints","terminal","git","skills_inspection","skills_create","context_index","symbols","mcp"],"active_runs":s.engine.controls.lock().unwrap().len()})
        }
        ("GET", ["workspaces"]) => json!(store.list::<Workspace>("workspace")?),
        ("POST", ["workspaces", "clone"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            json!(clone::clone_workspace(store, &body).await?)
        }
        ("POST", ["workspaces"]) => {
            let path = FsPath::new(field(&body, "path")?);
            ensure!(path.is_absolute(), "Escolha uma pasta com caminho absoluto");
            if body["create"] == true {
                ensure!(!path.exists(), "O destino já existe");
                std::fs::create_dir_all(path)?;
            }
            let root = path.canonicalize()?;
            ensure!(root.is_dir(), "Selecione uma pasta");
            let name = root
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Projeto".into());
            if let Some(existing) = store
                .list::<Workspace>("workspace")?
                .into_iter()
                .find(|w| FsPath::new(&w.root) == root)
            {
                json!(existing)
            } else {
                let w = Workspace {
                    id: forja_core::id(),
                    name,
                    root: root.to_string_lossy().into_owned(),
                    created_at: forja_core::now(),
                };
                store.put("workspace", &w.id, &w)?;
                json!(w)
            }
        }
        ("GET", ["workspaces", id, "sessions"]) => {
            let limit = q
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(50)
                .clamp(1, 200);
            let cursor = q.get("cursor").map(String::as_str);
            let mut sessions: Vec<Session> = store
                .list::<Session>("session")?
                .into_iter()
                .filter(|session| session.workspace_id == *id && !session.archived)
                .collect();
            sessions.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| b.id.cmp(&a.id))
            });
            if let Some(cursor) = cursor {
                if let Some(position) = sessions.iter().position(|session| session.id == cursor) {
                    sessions.drain(..=position);
                }
            }
            let has_more = sessions.len() > limit;
            sessions.truncate(limit);
            let next_cursor = has_more
                .then(|| sessions.last().map(|s| s.id.clone()))
                .flatten();
            json!({"items":sessions,"next_cursor":next_cursor})
        }
        ("POST", ["workspaces", id, "sessions"]) => {
            let _: Workspace = store.get("workspace", id)?;
            let provider_id = body["provider_id"].as_str().unwrap_or_default();
            let model = body["model"].as_str().unwrap_or_default();
            let session = Session {
                id: forja_core::id(),
                workspace_id: id.to_string(),
                title: body["title"]
                    .as_str()
                    .unwrap_or("Nova conversa")
                    .chars()
                    .take(100)
                    .collect(),
                mode: body
                    .get("mode")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(Mode::Agent),
                provider_id: provider_id.into(),
                model: model.into(),
                created_at: forja_core::now(),
                updated_at: forja_core::now(),
                executor_profile_id: body["executor_profile_id"].as_str().map(str::to_owned),
                reviewer_profile_ids: body
                    .get("reviewer_profile_ids")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_default(),
                reasoning_level: body["reasoning_level"].as_str().map(str::to_owned),
                last_run_id: None,
                archived: false,
            };
            if let Some(profile_id) = session.executor_profile_id.as_ref() {
                let _: ModelProfile = store.get("model_profile", profile_id)?;
            } else {
                ensure!(
                    !provider_id.is_empty() && !model.is_empty(),
                    "Escolha um modelo executor"
                );
                let _: Provider = store.get("provider", provider_id)?;
            }
            store.put("session", &session.id, &session)?;
            json!(session)
        }
        ("GET", ["workspaces", id, "files"]) => {
            let w: Workspace = store.get("workspace", id)?;
            json!(forja_core::files::list(
                FsPath::new(&w.root),
                q.get("path").map(String::as_str).unwrap_or(".")
            )?)
        }
        ("GET", ["workspaces", id, "file"]) => {
            let w: Workspace = store.get("workspace", id)?;
            json!(forja_core::files::read(
                FsPath::new(&w.root),
                q.get("path").map(String::as_str).unwrap_or("")
            )?)
        }
        ("PUT", ["workspaces", id, "file"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let path = field(&body, "path")?;
            let before = forja_core::files::read(FsPath::new(&w.root), path)?;
            forja_core::files::patch(
                store,
                FsPath::new(&w.root),
                id,
                "manual",
                path,
                field(&body, "hash")?,
                &before.content,
                field(&body, "content")?,
            )?
        }
        ("GET", ["workspaces", id, "search"]) => {
            let w: Workspace = store.get("workspace", id)?;
            json!(forja_core::files::search(
                FsPath::new(&w.root),
                q.get("q").map(String::as_str).unwrap_or("")
            )?)
        }
        ("GET", ["workspaces", id, "git"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let root = FsPath::new(&w.root);
            json!({"status":forja_core::process::git(root,&["status","--short","--branch"]).await?,"diff":forja_core::process::git(root,&["diff","--no-ext-diff","--no-textconv"]).await?})
        }
        ("POST", ["workspaces", id, "git", "init"]) => {
            let w: Workspace = store.get("workspace", id)?;
            json!({"output":forja_core::process::git(FsPath::new(&w.root),&["init"]).await?})
        }
        ("POST", ["workspaces", id, "index"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let st = store.clone();
            let id = id.to_string();
            tokio::task::spawn_blocking(move || {
                forja_core::context::index(&st, FsPath::new(&w.root), &id)
            })
            .await??
        }
        ("GET", ["workspaces", id, "map"]) => store
            .get::<Value>("repo_map", id)
            .unwrap_or(json!({"files":[],"indexed_files":0})),
        ("GET", ["workspaces", id, "symbols"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let path = q.get("path").map(String::as_str).unwrap_or("");
            let f = forja_core::files::read(FsPath::new(&w.root), path)?;
            json!(forja_core::context::symbols(path, &f.content)?)
        }
        ("POST", ["skills", "validate"]) => {
            forja_core::context::validate_skill(field(&body, "content")?)?
        }
        ("GET", ["hooks", "events"]) => json!(forja_core::hooks::EVENTS),
        ("GET", ["workspaces", id, "hooks"]) => {
            let _: Workspace = store.get("workspace", id)?;
            json!(forja_core::hooks::list(store, id)?)
        }
        ("POST", ["workspaces", id, "hooks"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let mut hook: forja_core::hooks::Hook = serde_json::from_value(body.clone())?;
            hook.workspace_id = id.to_string();
            json!(forja_core::hooks::create(
                store,
                FsPath::new(&w.root),
                hook
            )?)
        }
        ("PUT", ["workspaces", id, "hooks", hook_id]) => {
            let w: Workspace = store.get("workspace", id)?;
            let mut hook: forja_core::hooks::Hook = serde_json::from_value(body["hook"].clone())?;
            hook.id = hook_id.to_string();
            hook.workspace_id = id.to_string();
            let revision = body["expected_revision"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Versão esperada obrigatória"))?;
            let saved = forja_core::hooks::update(store, FsPath::new(&w.root), hook, revision)?;
            s.engine.revoke_hook_approvals(hook_id)?;
            json!(saved)
        }
        ("POST", ["workspaces", id, "hooks", "order"]) => {
            let _: Workspace = store.get("workspace", id)?;
            let ids: Vec<String> = serde_json::from_value(body["ids"].clone())?;
            let expected: Vec<forja_core::hooks::Version> =
                serde_json::from_value(body["expected"].clone())?;
            json!(forja_core::hooks::reorder(store, id, &ids, &expected)?)
        }
        ("DELETE", ["workspaces", id, "hooks", hook_id]) => {
            let revision = body["expected_revision"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Versão esperada obrigatória"))?;
            forja_core::hooks::remove(store, id, hook_id, revision)?;
            s.engine.revoke_hook_approvals(hook_id)?;
            json!({"deleted":true})
        }
        ("POST", ["workspaces", id, "skills"]) => {
            let w: Workspace = store.get("workspace", id)?;
            let content = field(&body, "content")?;
            let v = forja_core::context::validate_skill(content)?;
            let path = format!("skills/{}/SKILL.md", v["name"].as_str().unwrap());
            let p = forja_core::policy::resolve(FsPath::new(&w.root), &path, true)?;
            ensure!(!p.exists(), "Skill já existe");
            forja_core::files::patch(
                store,
                FsPath::new(&w.root),
                id,
                "manual-skill",
                &path,
                "",
                "",
                content,
            )?;
            v
        }
        ("GET", ["workspaces", id, "skills"]) => {
            let w: Workspace = store.get("workspace", id)?;
            json!(forja_core::extensions::skills(FsPath::new(&w.root))?)
        }
        ("POST", ["workspaces", id, "skills", action @ ("read" | "resources")]) => {
            let w: Workspace = store.get("workspace", id)?;
            let path = field(&body, "path")?;
            let hash = field(&body, "hash")?;
            if *action == "resources" {
                forja_core::skills::resources(FsPath::new(&w.root), path, hash)?
            } else {
                let resource = body
                    .get("resource")
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| anyhow::anyhow!("Recurso inválido"))
                    })
                    .transpose()?;
                forja_core::skills::read(FsPath::new(&w.root), path, hash, resource)?
            }
        }
        ("GET", ["workspaces", id, "checkpoints"]) => json!(store
            .list::<Value>("checkpoint")?
            .into_iter()
            .filter(|v| v["workspace_id"] == *id)
            .collect::<Vec<_>>()),
        ("POST", ["checkpoints", id, "restore"]) => {
            let cp: Value = store.get("checkpoint", id)?;
            let w: Workspace = store.get("workspace", field(&cp, "workspace_id")?)?;
            forja_core::files::restore(store, FsPath::new(&w.root), id)?
        }
        ("GET", ["providers"]) => json!(store.list::<Provider>("provider")?),
        ("GET", ["lsp", "presets"]) => forja_core::lsp::presets(),
        ("GET", ["workspaces", id, "lsp"]) => s.engine.lsp.status(id).await,
        ("POST", ["workspaces", id, "lsp", "start"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            ensure!(
                body["confirmed"] == true,
                "Revise e autorize o programa do servidor de linguagem"
            );
            let w: Workspace = store.get("workspace", id)?;
            let config: forja_core::lsp::Config = serde_json::from_value(body.clone())?;
            s.engine
                .lsp
                .start(id, FsPath::new(&w.root), &config)
                .await?;
            store.put("lsp_config", &format!("{id}:{}", config.language), &config)?;
            json!({"active":true})
        }
        ("POST", ["workspaces", id, "lsp", language, "stop"]) => {
            s.engine.lsp.stop(id, language).await;
            json!({"active":false})
        }
        ("POST", ["workspaces", id, "lsp", language, "sync"]) => {
            let client = s.engine.lsp.get(id, language).await?;
            json!({"version":client.sync(field(&body,"path")?,field(&body,"text")?).await?})
        }
        ("POST", ["workspaces", id, "lsp", language, "query"]) => {
            let client = s.engine.lsp.get(id, language).await?;
            let line = body["line"].as_u64().unwrap_or(0);
            let character = body["character"].as_u64().unwrap_or(0);
            ensure!(
                line < 2_000_000 && character < 2_000_000,
                "Posição inválida"
            );
            client
                .query(
                    field(&body, "path")?,
                    field(&body, "text")?,
                    field(&body, "method")?,
                    line as u32,
                    character as u32,
                )
                .await?
        }
        ("GET", ["workspaces", id, "lsp", language, "diagnostics"]) => {
            s.engine.lsp.get(id, language).await?.diagnostics().await
        }
        ("POST", ["providers"]) => {
            let mut p: Provider = serde_json::from_value(body.clone())?;
            if p.id.is_empty() {
                p.id = forja_core::id();
            }
            forja_core::models::validate_provider(&p)?;
            let previous = store.get::<Provider>("provider", &p.id).ok();
            p.secret_ref = previous.as_ref().and_then(|old| old.secret_ref.clone());
            if let Some(old) = previous.as_ref().filter(|old| old.secret_ref.is_some()) {
                ensure!(old.base_url==p.base_url && old.kind==p.kind || body["api_key"].as_str().is_some_and(|s|!s.is_empty()),"Ao mudar o endpoint de um perfil autenticado, informe a chave para o novo destino");
            }
            if let Some(key) = body["api_key"].as_str().filter(|s| !s.is_empty()) {
                forja_core::models::save_secret(&p.id, key)?;
                p.secret_ref = Some(p.id.clone());
            }
            store.put("provider", &p.id, &p)?;
            json!(p)
        }
        ("DELETE", ["providers", id]) => {
            let p: Provider = store.get("provider", id)?;
            ensure!(
                !store
                    .list::<ModelProfile>("model_profile")?
                    .iter()
                    .any(|profile| profile.provider_id == *id),
                "Remova os perfis de modelo deste provedor antes de excluí-lo"
            );
            for run_id in s.engine.controls.lock().unwrap().keys() {
                let run: Run = store.get("run", run_id)?;
                let session: Session = store.get("session", &run.session_id)?;
                ensure!(
                    session.provider_id != *id,
                    "Provedor em uso por execução ativa"
                );
            }
            if let Some(reference) = p.secret_ref {
                forja_core::models::delete_secret(&reference)?;
            }
            store.delete("provider", id)?;
            json!({"ok":true})
        }
        ("GET", ["providers", id, "models"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            let p: Provider = store.get("provider", id)?;
            forja_core::models::list_models(&p).await?
        }
        ("POST", ["providers", id, "probe"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            let p: Provider = store.get("provider", id)?;
            let m = Message {
                role: "user".into(),
                content: "Responda apenas OK.".into(),
                tool_calls: vec![],
                tool_call_id: None,
                provider_state: serde_json::Value::Null,
            };
            let answer = forja_core::models::generate(
                &p,
                field(&body, "model")?,
                &[m],
                &[],
                tokio_util::sync::CancellationToken::new(),
                Arc::new(|_| {}),
            )
            .await?;
            let tools = if body["test_tools"] == true {
                match forja_core::models::probe_tools(
                    &p,
                    field(&body, "model")?,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        json!({"status":"failed","message":error.to_string(),"executed_tools":0})
                    }
                }
            } else {
                json!({"status":"not_tested"})
            };
            let probe = json!({"id":id,"model":body["model"],"text":answer.text,"streaming":true,"tools":tools,"vision":"not_tested","created_at":forja_core::now()});
            let probe_id = forja_core::hash(format!("{id}:{}", field(&body, "model")?).as_bytes());
            store.put("probe", &probe_id, &probe)?;
            probe
        }
        ("GET", ["model-profiles"]) => json!(store.list::<ModelProfile>("model_profile")?),
        ("GET", ["model-profiles", id]) => json!(store.get::<ModelProfile>("model_profile", id)?),
        ("POST", ["model-profiles"]) => {
            let _: Provider = store.get("provider", field(&body, "provider_id")?)?;
            let now = forja_core::now();
            let context = body["context_window_tokens"].as_u64();
            let enabled = body["enabled"].as_bool().unwrap_or(false);
            ensure!(
                !enabled || context.unwrap_or(0) > 0,
                "Informe a janela de contexto antes de ativar o modelo"
            );
            let profile = ModelProfile {
                id: forja_core::id(),
                provider_id: field(&body, "provider_id")?.into(),
                model_id: field(&body, "model_id")?.into(),
                display_name: body["display_name"]
                    .as_str()
                    .unwrap_or(field(&body, "model_id")?)
                    .chars()
                    .take(120)
                    .collect(),
                enabled,
                revision: 1,
                context_window_tokens: context,
                context_source: body
                    .get("context_source")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(MetadataSource::Manual),
                max_output_tokens: body["max_output_tokens"].as_u64(),
                max_output_source: body
                    .get("max_output_source")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(MetadataSource::Manual),
                capabilities: body
                    .get("capabilities")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_default(),
                reasoning_levels: body
                    .get("reasoning_levels")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_default(),
                default_reasoning_level: body["default_reasoning_level"]
                    .as_str()
                    .map(str::to_owned),
                created_at: now.clone(),
                updated_at: now,
            };
            ensure!(
                !profile.model_id.trim().is_empty(),
                "Informe o identificador do modelo"
            );
            store.put("model_profile", &profile.id, &profile)?;
            json!(profile)
        }
        ("PUT", ["model-profiles", id]) => {
            let mut profile: ModelProfile = store.get("model_profile", id)?;
            let expected = body["revision"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Informe a revisão atual"))?;
            ensure!(
                profile.revision == expected,
                "O perfil foi alterado em outra janela; recarregue antes de salvar"
            );
            if let Some(v) = body["display_name"].as_str() {
                profile.display_name = v.chars().take(120).collect();
            }
            if let Some(v) = body["context_window_tokens"].as_u64() {
                profile.context_window_tokens = Some(v);
                profile.context_source = MetadataSource::Manual;
            }
            if body.get("max_output_tokens").is_some() {
                profile.max_output_tokens = body["max_output_tokens"].as_u64();
                profile.max_output_source = MetadataSource::Manual;
            }
            if let Some(v) = body["enabled"].as_bool() {
                profile.enabled = v;
            }
            if let Some(v) = body.get("capabilities") {
                profile.capabilities = serde_json::from_value(v.clone())?;
            }
            if let Some(v) = body.get("reasoning_levels") {
                profile.reasoning_levels = serde_json::from_value(v.clone())?;
            }
            if body.get("default_reasoning_level").is_some() {
                profile.default_reasoning_level =
                    body["default_reasoning_level"].as_str().map(str::to_owned);
            }
            ensure!(
                !profile.enabled || profile.context_window_tokens.unwrap_or(0) > 0,
                "Informe a janela de contexto antes de ativar o modelo"
            );
            profile.revision += 1;
            profile.updated_at = forja_core::now();
            store.put("model_profile", id, &profile)?;
            json!(profile)
        }
        ("DELETE", ["model-profiles", id]) => {
            for run_id in s.engine.controls.lock().unwrap().keys() {
                let run: Run = store.get("run", run_id)?;
                ensure!(
                    run.executor_profile_id.as_deref() != Some(id)
                        && !run.reviewer_profile_ids.iter().any(|v| v == id),
                    "Modelo em uso por execução ativa"
                );
            }
            store.delete("model_profile", id)?;
            json!({"ok":true})
        }
        ("GET", ["sessions", id, "plans"]) => {
            let _: Session = store.get("session", id)?;
            json!(forja_core::plans::list_for_session(store, id)?)
        }
        ("GET", ["plans", id]) => json!(store.get::<PlanArtifact>("plan", id)?),
        ("GET", ["plans", id, "checkpoints"]) => {
            let _: PlanArtifact = store.get("plan", id)?;
            let mut items: Vec<ImplementationCheckpoint> = store
                .list("implementation_checkpoint")?
                .into_iter()
                .filter(|checkpoint: &ImplementationCheckpoint| checkpoint.plan_id == *id)
                .collect();
            items.sort_by_key(|checkpoint| checkpoint.revision);
            json!(items)
        }
        ("GET", ["plans", id, "checkpoints", "latest"]) => {
            let _: PlanArtifact = store.get("plan", id)?;
            json!(forja_core::plans::latest_checkpoint(store, id)?)
        }
        ("GET", ["plans", id, "revisions"]) => {
            let _: PlanArtifact = store.get("plan", id)?;
            json!(store
                .list::<PlanRevisionProposal>("plan_revision_proposal")?
                .into_iter()
                .filter(|proposal| proposal.plan_id == *id)
                .collect::<Vec<_>>())
        }
        ("POST", ["plans", id, "revisions", revision_id, "approve"]) => {
            let plan: PlanArtifact = store.get("plan", id)?;
            let workspace: Workspace = store.get("workspace", &plan.workspace_id)?;
            let (proposal, next) =
                forja_core::plans::approve_revision(store, &workspace, revision_id)?;
            ensure!(proposal.plan_id == *id, "A revisão pertence a outro plano");
            s.engine.emit(
                &next.session_id,
                &proposal.created_by_run_id,
                "plan.revision.approved",
                json!({"proposal":proposal,"plan":next}),
            )?;
            let _ = s.engine.action(&proposal.created_by_run_id, "resume");
            json!({"proposal":proposal,"plan":next})
        }
        ("POST", ["plans", id, "revisions", revision_id, "reject"]) => {
            let (proposal, plan) = forja_core::plans::reject_revision(store, revision_id)?;
            ensure!(proposal.plan_id == *id, "A revisão pertence a outro plano");
            s.engine.emit(
                &plan.session_id,
                &proposal.created_by_run_id,
                "plan.revision.rejected",
                json!({"proposal":proposal,"plan":plan}),
            )?;
            let _ = s.engine.action(&proposal.created_by_run_id, "resume");
            json!({"proposal":proposal,"plan":plan})
        }
        ("POST", ["plans", id, "implement"]) | ("POST", ["plans", id, "retry"]) => {
            let mut plan: PlanArtifact = store.get("plan", id)?;
            ensure!(
                ["ready", "accepted", "implementation_failed"].contains(&plan.state.as_str())
                    || plan.implementation_run_id.is_some(),
                "Este plano não pode ser implementado no estado atual"
            );
            if let Some(expected) = body["expected_revision"].as_u64() {
                ensure!(
                    expected == plan.revision,
                    "A revisão do plano mudou; revise antes de implementar"
                );
            }
            if let Some(run_id) = plan.implementation_run_id.as_ref() {
                if let Ok(existing) = store.get::<Run>("run", run_id) {
                    if matches!(
                        existing.state.as_str(),
                        "running" | "paused" | "waiting_for_input" | "waiting_for_approval"
                    ) {
                        json!({"run":existing,"plan":plan,"idempotent":true})
                    } else {
                        ensure!(
                            parts.get(2) == Some(&"retry"),
                            "A implementação anterior terminou; use Retomar implementação"
                        );
                        let session: Session = store.get("session", &plan.session_id)?;
                        let workspace: Workspace = store.get("workspace", &plan.workspace_id)?;
                        let run_id = forja_core::id();
                        let revision = store
                            .get::<ContextState>("context_state", &session.id)
                            .map(|value| value.context_revision)
                            .unwrap_or(0);
                        let baseline = forja_core::plans::baseline(
                            store, &workspace, &session, &plan, &run_id, revision,
                        )
                        .await?;
                        plan.state = "implementing".into();
                        plan.implementation_run_id = Some(run_id.clone());
                        plan.baseline_id = Some(baseline.id.clone());
                        plan.updated_at = forja_core::now();
                        store.put("plan", &plan.id, &plan)?;
                        let initial = forja_core::plans::checkpoint(
                            store,
                            &workspace,
                            &plan,
                            &run_id,
                            "implementation_started",
                            "running",
                            None,
                            Some("Implementação retomada a partir do plano persistente"),
                        )?;
                        let first = plan
                            .steps
                            .iter()
                            .find(|step| step.dependencies.is_empty())
                            .map(|step| step.id.clone());
                        if let Some(step) = first.as_deref() {
                            let _ = forja_core::plans::checkpoint(
                                store,
                                &workspace,
                                &plan,
                                &run_id,
                                "step_started",
                                "running",
                                Some(step),
                                Some("Primeira etapa pronta iniciada"),
                            )?;
                        }
                        let options = StartRunOptions {
                            mode: Mode::Agent,
                            executor_profile_id: session.executor_profile_id.clone(),
                            reviewer_profile_ids: session.reviewer_profile_ids.clone(),
                            reasoning_level: session.reasoning_level.clone(),
                            context_revision: revision,
                            resumed_from_run_id: Some(existing.id),
                        };
                        let run = s.engine.start_with_reserved_id(&session.id, "Retome a implementação do plano persistente usando o checkpoint operacional como fonte de verdade.", &[], options, Some(run_id.clone()))?;
                        s.engine.emit(&session.id, &run_id, "plan.implementation.started", json!({"plan_id":plan.id,"revision":plan.revision,"baseline":baseline,"checkpoint":initial,"resumed":true}))?;
                        json!({"run":run,"plan":plan,"baseline":baseline})
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "A execução vinculada ao plano não foi encontrada"
                    )
                    .into());
                }
            } else {
                let mut session: Session = store.get("session", &plan.session_id)?;
                ensure!(
                    session.workspace_id == plan.workspace_id,
                    "Plano e conversa pertencem a projetos diferentes"
                );
                let workspace: Workspace = store.get("workspace", &plan.workspace_id)?;
                let run_id = forja_core::id();
                let revision = store
                    .get::<ContextState>("context_state", &session.id)
                    .map(|value| value.context_revision)
                    .unwrap_or(0);
                let baseline = forja_core::plans::baseline(
                    store, &workspace, &session, &plan, &run_id, revision,
                )
                .await?;
                plan.state = "implementing".into();
                plan.implementation_run_id = Some(run_id.clone());
                plan.baseline_id = Some(baseline.id.clone());
                plan.updated_at = forja_core::now();
                store.put("plan", &plan.id, &plan)?;
                let initial = forja_core::plans::checkpoint(
                    store,
                    &workspace,
                    &plan,
                    &run_id,
                    "implementation_started",
                    "running",
                    None,
                    Some("Baseline capturado; implementação iniciada"),
                )?;
                let first = plan
                    .steps
                    .iter()
                    .find(|step| step.dependencies.is_empty())
                    .map(|step| step.id.clone());
                if let Some(step) = first.as_deref() {
                    let _ = forja_core::plans::checkpoint(
                        store,
                        &workspace,
                        &plan,
                        &run_id,
                        "step_started",
                        "running",
                        Some(step),
                        Some("Primeira etapa pronta iniciada"),
                    )?;
                }
                let previous_mode = session.mode.clone();
                session.mode = Mode::Agent;
                session.updated_at = forja_core::now();
                store.put("session", &session.id, &session)?;
                s.engine.emit(
                    &session.id,
                    &run_id,
                    "plan.accepted",
                    json!({"plan_id":plan.id,"revision":plan.revision}),
                )?;
                s.engine.emit(
                    &session.id,
                    &run_id,
                    "mode.changed",
                    json!({"from":previous_mode,"to":"agent","reason":"implement_plan"}),
                )?;
                let options = StartRunOptions {
                    mode: Mode::Agent,
                    executor_profile_id: session.executor_profile_id.clone(),
                    reviewer_profile_ids: session.reviewer_profile_ids.clone(),
                    reasoning_level: session.reasoning_level.clone(),
                    context_revision: revision,
                    resumed_from_run_id: Some(plan.source_run_id.clone()),
                };
                match s.engine.start_with_reserved_id(&session.id, "Implemente o plano persistente aprovado. Consulte o plano e o checkpoint, avance uma etapa por vez e registre evidências com plan.update_progress.", &[], options, Some(run_id.clone())) {
                    Ok(run) => {
                        s.engine.emit(&session.id, &run_id, "plan.implementation.started", json!({"plan_id":plan.id,"revision":plan.revision,"baseline":baseline,"checkpoint":initial}))?;
                        json!({"run":run,"plan":plan,"baseline":baseline})
                    }
                    Err(error) => {
                        plan.state = "implementation_failed".into();
                        plan.updated_at = forja_core::now();
                        store.put("plan", &plan.id, &plan)?;
                        return Err(error.into());
                    }
                }
            }
        }
        ("POST", ["plans", id, "checkpoints"]) => {
            let plan: PlanArtifact = store.get("plan", id)?;
            let run_id = plan
                .implementation_run_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("O plano ainda não possui execução"))?;
            let workspace: Workspace = store.get("workspace", &plan.workspace_id)?;
            let checkpoint = forja_core::plans::checkpoint(
                store,
                &workspace,
                &plan,
                run_id,
                "manual",
                body["status"].as_str().unwrap_or("running"),
                body["current_step_id"].as_str(),
                body["note"].as_str(),
            )?;
            s.engine.emit(
                &plan.session_id,
                run_id,
                "implementation.checkpoint.created",
                serde_json::to_value(&checkpoint)?,
            )?;
            json!(checkpoint)
        }
        ("GET", ["sessions"]) => json!(store.list::<Session>("session")?),
        ("POST", ["sessions"]) => {
            let _: Workspace = store.get("workspace", field(&body, "workspace_id")?)?;
            let _: Provider = store.get("provider", field(&body, "provider_id")?)?;
            let model = field(&body, "model")?;
            ensure!(!model.trim().is_empty(), "Escolha um modelo");
            let session = Session {
                id: forja_core::id(),
                workspace_id: field(&body, "workspace_id")?.into(),
                title: body["title"]
                    .as_str()
                    .unwrap_or("Nova tarefa")
                    .chars()
                    .take(100)
                    .collect(),
                mode: serde_json::from_value(body["mode"].clone())?,
                provider_id: field(&body, "provider_id")?.into(),
                model: model.into(),
                created_at: forja_core::now(),
                updated_at: forja_core::now(),
                executor_profile_id: body["executor_profile_id"].as_str().map(str::to_owned),
                reviewer_profile_ids: vec![],
                reasoning_level: None,
                last_run_id: None,
                archived: false,
            };
            store.put("session", &session.id, &session)?;
            json!(session)
        }
        ("GET", ["sessions", id, "events"]) => json!(store.events(
            id,
            q.get("run_id").map(String::as_str),
            q.get("after_session_sequence")
                .or_else(|| q.get("after"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        )?),
        ("PATCH", ["sessions", id]) => {
            let mut session: Session = store.get("session", id)?;
            let changes_execution_profile = body.get("executor_profile_id").is_some()
                || body.get("reasoning_level").is_some()
                || body.get("provider_id").is_some()
                || body.get("model").is_some();
            if changes_execution_profile {
                if let Some(plan) = forja_core::plans::active(store, id)? {
                    if plan.state == "implementing" {
                        let workspace: Workspace = store.get("workspace", &session.workspace_id)?;
                        if let Some(run_id) = plan.implementation_run_id.as_deref() {
                            let checkpoint = forja_core::plans::checkpoint(
                                store,
                                &workspace,
                                &plan,
                                run_id,
                                "before_model_change",
                                "paused",
                                None,
                                Some("Estado salvo antes da troca de modelo ou raciocínio"),
                            )?;
                            s.engine.emit(
                                id,
                                run_id,
                                "implementation.checkpoint.created",
                                serde_json::to_value(checkpoint)?,
                            )?;
                        }
                    }
                }
            }
            if let Some(v) = body["title"].as_str() {
                session.title = v.chars().take(100).collect();
            }
            if let Some(v) = body["archived"].as_bool() {
                session.archived = v;
            }
            if let Some(v) = body["provider_id"].as_str() {
                let _: Provider = store.get("provider", v)?;
                session.provider_id = v.into();
            }
            if let Some(v) = body["model"].as_str() {
                session.model = v.into();
            }
            if let Some(v) = body.get("mode") {
                session.mode = serde_json::from_value(v.clone())?;
            }
            if body.get("executor_profile_id").is_some() {
                session.executor_profile_id =
                    body["executor_profile_id"].as_str().map(str::to_owned);
            }
            if let Some(v) = body.get("reviewer_profile_ids") {
                session.reviewer_profile_ids = serde_json::from_value(v.clone())?;
                ensure!(
                    session.reviewer_profile_ids.len() <= 3,
                    "Escolha no máximo três revisores"
                );
            }
            if body.get("reasoning_level").is_some() {
                session.reasoning_level = body["reasoning_level"].as_str().map(str::to_owned);
            }
            session.updated_at = forja_core::now();
            store.put("session", id, &session)?;
            json!(session)
        }
        ("GET", ["sessions", id, "context"]) => {
            let session: Session = store.get("session", id)?;
            if let Some(profile_id) = session.executor_profile_id.as_ref() {
                if let Ok(state) = store.get::<ContextState>("context_state", id) {
                    json!(state)
                } else {
                    let profile: ModelProfile = store.get("model_profile", profile_id)?;
                    let history = store.get::<Vec<Message>>("history", id).unwrap_or_default();
                    json!(forja_core::context_budget::measure(
                        id, &profile, &history, 0
                    )?)
                }
            } else {
                json!(null)
            }
        }
        ("POST", ["sessions", id, "compact"]) => {
            let session: Session = store.get("session", id)?;
            if let Some(plan) = forja_core::plans::active(store, id)? {
                if let Some(run_id) = plan.implementation_run_id.as_deref() {
                    let workspace: Workspace = store.get("workspace", &session.workspace_id)?;
                    let checkpoint = forja_core::plans::checkpoint(
                        store,
                        &workspace,
                        &plan,
                        run_id,
                        "before_compaction",
                        "running",
                        None,
                        Some("Estado salvo antes da compactação manual"),
                    )?;
                    s.engine.emit(
                        id,
                        run_id,
                        "implementation.checkpoint.created",
                        serde_json::to_value(checkpoint)?,
                    )?;
                }
            }
            let profile_id = session
                .executor_profile_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Escolha um perfil de modelo"))?;
            let profile: ModelProfile = store.get("model_profile", profile_id)?;
            let provider: Provider = store.get("provider", &profile.provider_id)?;
            let mut history = store.get::<Vec<Message>>("history", id).unwrap_or_default();
            let revision = store
                .get::<ContextState>("context_state", id)
                .map(|s| s.context_revision)
                .unwrap_or(0);
            let (state, summary) = forja_core::context_budget::compact_with_model(
                store,
                id,
                &provider,
                &profile,
                &mut history,
                revision,
                tokio_util::sync::CancellationToken::new(),
            )
            .await?;
            json!({"context":state,"summary":summary})
        }
        ("GET", ["sessions", id, "export"]) => {
            json!({"session":store.get::<Session>("session",id)?,"history":store.get::<Vec<Message>>("history",id).unwrap_or_default(),"history_archives":store.list::<forja_core::context_budget::HistoryArchive>("history_archive")?.into_iter().filter(|archive|archive.session_id==*id).collect::<Vec<_>>(),"summaries":store.list::<forja_core::context_budget::ContextSummary>("context_summary")?.into_iter().filter(|summary|summary.session_id==*id).collect::<Vec<_>>(),"plans":forja_core::plans::list_for_session(store,id)?,"checkpoints":store.list::<ImplementationCheckpoint>("implementation_checkpoint")?.into_iter().filter(|checkpoint|checkpoint.session_id==*id).collect::<Vec<_>>(),"baselines":store.list::<ImplementationBaseline>("implementation_baseline")?.into_iter().filter(|baseline|baseline.session_id==*id).collect::<Vec<_>>(),"agent_tasks":store.list::<AgentTask>("agent_task")?.into_iter().filter(|task|task.session_id==*id).collect::<Vec<_>>(),"code_proposals":store.list::<CodeProposal>("code_proposal")?.into_iter().filter(|proposal|proposal.session_id==*id).collect::<Vec<_>>(),"artifacts":store.list::<Artifact>("artifact")?.into_iter().filter(|artifact|artifact.session_id==*id).collect::<Vec<_>>(),"events":store.events(id,None,0)?})
        }
        ("POST", ["sessions", id, "runs"]) => {
            let selected = body
                .get("selected_skills")
                .map(|v| serde_json::from_value::<Vec<forja_core::skills::Selection>>(v.clone()))
                .transpose()?
                .unwrap_or_default();
            let session: Session = store.get("session", id)?;
            let options = StartRunOptions {
                mode: body
                    .get("mode")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(session.mode),
                executor_profile_id: body["executor_profile_id"]
                    .as_str()
                    .map(str::to_owned)
                    .or(session.executor_profile_id),
                reviewer_profile_ids: body
                    .get("reviewer_profile_ids")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(session.reviewer_profile_ids),
                reasoning_level: body["reasoning_level"]
                    .as_str()
                    .map(str::to_owned)
                    .or(session.reasoning_level),
                context_revision: body["expected_context_revision"].as_u64().unwrap_or(0),
                resumed_from_run_id: body["resumed_from_run_id"].as_str().map(str::to_owned),
            };
            json!(s
                .engine
                .start_with_options(id, field(&body, "goal")?, &selected, options)?)
        }
        ("GET", ["runs"]) => json!(store.list::<Run>("run")?),
        ("POST", ["runs", id, action]) => {
            s.engine.action(id, action)?;
            json!({"ok":true})
        }
        ("GET", ["approvals"]) => json!(store
            .list::<Approval>("approval")?
            .into_iter()
            .filter(|a| a.state == "pending")
            .collect::<Vec<_>>()),
        ("POST", ["approvals", id, "decision"]) => {
            s.engine.decide(id, field(&body, "decision")?)?;
            json!({"ok":true})
        }
        ("GET", ["interactions"]) => json!(store
            .list::<Interaction>("interaction")?
            .into_iter()
            .filter(|item| item.state == "pending")
            .collect::<Vec<_>>()),
        ("POST", ["interactions", id, "answer"]) => {
            let interaction: Interaction = store.get("interaction", id)?;
            let answer = body
                .get("answer")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Informe a resposta"))?;
            let active = s.engine.answer_interaction(id, answer.clone())?;
            let continuation = if !active {
                let mut history = store
                    .get::<Vec<Message>>("history", &interaction.session_id)
                    .unwrap_or_default();
                let call_id = history
                    .iter()
                    .rev()
                    .flat_map(|message| message.tool_calls.iter())
                    .find(|call| call.name == "ui.ask_user")
                    .map(|call| call.id.clone())
                    .unwrap_or_else(|| id.to_string());
                history.push(Message {
                    role: "tool".into(),
                    content: answer.to_string(),
                    tool_calls: vec![],
                    tool_call_id: Some(call_id),
                    provider_state: Value::Null,
                });
                store.put("history", &interaction.session_id, &history)?;
                let previous: Run = store.get("run", &interaction.run_id)?;
                let session: Session = store.get("session", &interaction.session_id)?;
                Some(s.engine.start_with_options(
                    &interaction.session_id,
                    "Continue o planejamento usando a resposta persistida acima.",
                    &[],
                    StartRunOptions {
                        mode: previous.mode.unwrap_or(session.mode),
                        executor_profile_id:
                            previous.executor_profile_id.or(session.executor_profile_id),
                        reviewer_profile_ids: previous.reviewer_profile_ids,
                        reasoning_level: previous.reasoning_level,
                        context_revision: previous.context_revision,
                        resumed_from_run_id: Some(previous.id),
                    },
                )?)
            } else {
                None
            };
            json!({"ok":true,"resumed":continuation})
        }
        ("POST", ["interactions", id, "cancel"]) => json!({"ok":s.engine.cancel_interaction(id)?}),
        ("GET", ["sessions", id, "code-proposals"]) => json!(store
            .list::<CodeProposal>("code_proposal")?
            .into_iter()
            .filter(|proposal| proposal.session_id == *id)
            .collect::<Vec<_>>()),
        ("POST", ["code-proposals", id, "apply"]) => {
            let mut proposal: CodeProposal = store.get("code_proposal", id)?;
            ensure!(
                proposal.state == "pending" || proposal.state == "conflict",
                "Esta proposta não está pendente"
            );
            let workspace: Workspace = store.get("workspace", &proposal.workspace_id)?;
            let result = forja_core::files::patch(
                store,
                FsPath::new(&workspace.root),
                &workspace.id,
                &proposal.run_id,
                &proposal.path,
                &proposal.base_hash,
                &proposal.old_text,
                &proposal.new_text,
            );
            match result {
                Ok(result) => {
                    proposal.state = "applied".into();
                    proposal.checkpoint_id = result["checkpoint_id"].as_str().map(str::to_owned);
                    proposal.updated_at = forja_core::now();
                    store.put("code_proposal", id, &proposal)?;
                    s.engine.emit(
                        &proposal.session_id,
                        &proposal.run_id,
                        "code.proposal.applied",
                        json!({"proposal":proposal,"result":result}),
                    )?;
                    json!({"proposal":proposal,"result":result})
                }
                Err(error) => {
                    proposal.state = "conflict".into();
                    proposal.updated_at = forja_core::now();
                    store.put("code_proposal", id, &proposal)?;
                    return Err(error.into());
                }
            }
        }
        ("POST", ["code-proposals", id, "dismiss"]) => {
            let mut proposal: CodeProposal = store.get("code_proposal", id)?;
            proposal.state = "dismissed".into();
            proposal.updated_at = forja_core::now();
            store.put("code_proposal", id, &proposal)?;
            json!(proposal)
        }
        ("GET", ["workspaces", id, "agents"]) => {
            let _: Workspace = store.get("workspace", id)?;
            json!(store
                .list::<AgentProfile>("agent_profile")?
                .into_iter()
                .filter(|profile| profile.workspace_id == *id)
                .collect::<Vec<_>>())
        }
        ("POST", ["workspaces", id, "agents"]) => {
            let _: Workspace = store.get("workspace", id)?;
            let now = forja_core::now();
            let profile = AgentProfile {
                id: forja_core::id(),
                workspace_id: id.to_string(),
                name: field(&body, "name")?.chars().take(120).collect(),
                role: body["role"].as_str().unwrap_or("custom").into(),
                instructions: field(&body, "instructions")?.chars().take(20_000).collect(),
                model_profile_id: field(&body, "model_profile_id")?.into(),
                reasoning_level: body["reasoning_level"].as_str().map(str::to_owned),
                allowed_tools: body
                    .get("allowed_tools")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_default(),
                write_access: body["write_access"].as_str().unwrap_or("none").into(),
                max_turns: body["max_turns"].as_u64().unwrap_or(20).min(100) as u32,
                token_budget: body["token_budget"].as_u64(),
                time_budget_seconds: body["time_budget_seconds"].as_u64().unwrap_or(1800),
                created_by: "user".into(),
                created_by_run_id: None,
                enabled: true,
                revision: 1,
                created_at: now.clone(),
                updated_at: now,
            };
            forja_core::agents::validate_profile(store, &profile)?;
            store.put("agent_profile", &profile.id, &profile)?;
            json!(profile)
        }
        ("PUT", ["agents", id]) => {
            let mut profile: AgentProfile = store.get("agent_profile", id)?;
            ensure!(
                body["revision"].as_u64() == Some(profile.revision),
                "O agente foi alterado em outra tela"
            );
            if let Some(value) = body["name"].as_str() {
                profile.name = value.chars().take(120).collect()
            }
            if let Some(value) = body["role"].as_str() {
                profile.role = value.into()
            }
            if let Some(value) = body["instructions"].as_str() {
                profile.instructions = value.chars().take(20_000).collect()
            }
            if let Some(value) = body["model_profile_id"].as_str() {
                profile.model_profile_id = value.into()
            }
            if body.get("reasoning_level").is_some() {
                profile.reasoning_level = body["reasoning_level"].as_str().map(str::to_owned)
            }
            if let Some(value) = body.get("allowed_tools") {
                profile.allowed_tools = serde_json::from_value(value.clone())?
            }
            if let Some(value) = body["write_access"].as_str() {
                profile.write_access = value.into()
            }
            if let Some(value) = body["enabled"].as_bool() {
                profile.enabled = value
            }
            if let Some(value) = body["max_turns"].as_u64() {
                profile.max_turns = value.min(100) as u32
            }
            if body.get("token_budget").is_some() {
                profile.token_budget = body["token_budget"].as_u64()
            }
            if let Some(value) = body["time_budget_seconds"].as_u64() {
                profile.time_budget_seconds = value
            }
            profile.revision += 1;
            profile.updated_at = forja_core::now();
            forja_core::agents::validate_profile(store, &profile)?;
            store.put("agent_profile", id, &profile)?;
            json!(profile)
        }
        ("DELETE", ["agents", id]) => {
            ensure!(
                !store
                    .list::<AgentTask>("agent_task")?
                    .iter()
                    .any(|task| task.agent_profile_id == *id
                        && matches!(task.state.as_str(), "queued" | "running")),
                "Agente em uso por tarefa ativa"
            );
            store.delete("agent_profile", id)?;
            json!({"ok":true})
        }
        ("GET", ["sessions", id, "agent-tasks"]) => json!(store
            .list::<AgentTask>("agent_task")?
            .into_iter()
            .filter(|task| task.session_id == *id)
            .collect::<Vec<_>>()),
        ("GET", ["search-providers"]) => {
            json!(store.list::<forja_core::web::SearchProvider>("search_provider")?)
        }
        ("POST", ["search-providers"]) => {
            let mut provider: forja_core::web::SearchProvider =
                serde_json::from_value(body.clone())?;
            if provider.id.is_empty() {
                provider.id = forja_core::id()
            }
            let previous = store
                .get::<forja_core::web::SearchProvider>("search_provider", &provider.id)
                .ok();
            match previous.as_ref() {
                Some(old) => {
                    ensure!(
                        provider.revision == old.revision,
                        "O provedor de pesquisa foi alterado em outra janela"
                    );
                    provider.secret_ref = old.secret_ref.clone();
                    if old.secret_ref.is_some()
                        && (old.base_url != provider.base_url || old.kind != provider.kind)
                    {
                        ensure!(
                            body["api_key"]
                                .as_str()
                                .is_some_and(|value| !value.is_empty()),
                            "Ao mudar o endpoint autenticado, informe a chave novamente"
                        );
                    }
                }
                None => {
                    ensure!(provider.revision == 0, "Revisão inicial inválida");
                    provider.secret_ref = None;
                }
            }
            forja_core::web::validate_config(&provider)?;
            if let Some(key) = body["api_key"].as_str().filter(|value| !value.is_empty()) {
                keyring::Entry::new("app.forja.search", &provider.id)?.set_password(key)?;
                provider.secret_ref = Some(provider.id.clone());
            }
            provider.revision = previous.map(|old| old.revision + 1).unwrap_or(1);
            store.put("search_provider", &provider.id, &provider)?;
            json!(provider)
        }
        ("DELETE", ["search-providers", id]) => {
            let provider: forja_core::web::SearchProvider = store.get("search_provider", id)?;
            if let Some(reference) = provider.secret_ref {
                match keyring::Entry::new("app.forja.search", &reference)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            store.delete("search_provider", id)?;
            json!({"ok":true})
        }
        ("GET", ["artifacts", id]) => json!(store.get::<Artifact>("artifact", id)?),
        ("GET", ["artifacts", id, "content"]) => {
            let artifact: Artifact = store.get("artifact", id)?;
            let bytes = store.read_blob(&artifact.blob_id)?;
            ensure!(
                bytes.len() <= 15_000_000,
                "Artefato grande demais para visualização"
            );
            json!({"id":artifact.id,"media_type":artifact.media_type,"base64":base64::engine::general_purpose::STANDARD.encode(bytes)})
        }
        ("GET", ["mcp", "catalogs"]) => json!(s.engine.mcp.catalogs().await),
        ("DELETE", ["mcp", "servers", id]) => {
            s.engine.mcp.disconnect(id).await;
            let c: forja_core::mcp::Config = store.get("mcp", id)?;
            if let Some(r) = c.secret_ref {
                match keyring::Entry::new("app.forja.mcp", &r)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            store.delete("mcp", id)?;
            json!({"ok":true})
        }
        ("GET", ["mcp", "servers"]) => json!(store.list::<forja_core::mcp::Config>("mcp")?),
        ("POST", ["mcp", "servers"]) => {
            let mut c: forja_core::mcp::Config = serde_json::from_value(body.clone())?;
            forja_core::mcp::validate_config(&c)?;
            c.id = forja_core::id();
            c.secret_ref = None;
            if let Some(token) = body["token"].as_str().filter(|s| !s.is_empty()) {
                keyring::Entry::new("app.forja.mcp", &c.id)?.set_password(token)?;
                c.secret_ref = Some(c.id.clone());
            }
            store.put("mcp", &c.id, &c)?;
            json!(c)
        }
        ("POST", ["mcp", "servers", id, "connect"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            ensure!(
                body["confirmed"] == true,
                "Confirme a conexão e o acesso do servidor MCP"
            );
            let c = store.get("mcp", id)?;
            s.engine.mcp.connect(&c).await?
        }
        ("POST", ["mcp", "servers", id, "disconnect"]) => {
            s.engine.mcp.disconnect(id).await;
            json!({"ok":true})
        }
        ("GET", ["mcp", "servers", id, "catalog"]) => s.engine.mcp.catalog(id).await?,
        ("POST", ["mcp", "servers", id, "call"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            ensure!(body["confirmed"] == true, "Revise e confirme a chamada MCP");
            s.engine
                .mcp
                .call(
                    id,
                    field(&body, "name")?,
                    body["arguments"].clone(),
                    field(&body, "catalog_hash")?,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await?
        }
        ("GET", ["mcp", "servers", id, kind]) => s.engine.mcp.list(id, kind).await?,
        ("GET", ["tools"]) => json!(forja_core::policy::tools()),
        ("POST", ["terminals"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Terminal nativo bloqueado em modo offline"
            );
            let w: Workspace = store.get("workspace", field(&body, "workspace_id")?)?;
            json!({"id":s.terminals.start(FsPath::new(&w.root))?})
        }
        ("GET", ["terminals", id]) => json!({"output":s.terminals.read(id)?}),
        ("POST", ["terminals", id, "input"]) => {
            ensure!(
                !s.engine.offline.load(Ordering::SeqCst),
                "Modo offline ativo"
            );
            s.terminals.input(id, field(&body, "input")?)?;
            json!({"ok":true})
        }
        ("POST", ["terminals", id, "resize"]) => {
            s.terminals.resize(
                id,
                body["rows"].as_u64().unwrap_or(24).min(500) as u16,
                body["cols"].as_u64().unwrap_or(100).min(1000) as u16,
            )?;
            json!({"ok":true})
        }
        ("DELETE", ["terminals", id]) => {
            s.terminals.close(id)?;
            json!({"ok":true})
        }
        ("GET", ["settings"]) => store
            .get::<Value>("settings", "global")
            .unwrap_or(json!({"offline":false,"language":"pt-BR","telemetry":false})),
        ("PUT", ["settings"]) => {
            let offline = body["offline"].as_bool().unwrap_or(false);
            s.engine.offline.store(offline, Ordering::SeqCst);
            if offline {
                for c in s.engine.controls.lock().unwrap().values() {
                    c.cancel.cancel();
                }
                s.terminals.close_all();
                s.engine.browser.close_all().await;
            }
            s.engine.mcp.set_offline(offline).await;
            s.engine.lsp.set_offline(offline).await;
            let v = json!({"offline":offline,"language":"pt-BR","telemetry":false});
            store.put("settings", "global", &v)?;
            v
        }
        ("POST", ["backup"]) => {
            let store = store.clone();
            let path = tokio::task::spawn_blocking(move || store.backup()).await??;
            json!({"path":path,"format":"forja-backup","version":1})
        }
        ("POST", ["backup", "verify"]) => {
            let source = std::path::PathBuf::from(field(&body, "source")?);
            let manifest =
                tokio::task::spawn_blocking(move || forja_core::backup::verify(&source)).await??;
            json!({"verified":true,"created_at":manifest.created_at,"files":manifest.entries.len(),"bytes":manifest.entries.iter().map(|e|e.bytes).sum::<u64>()})
        }
        ("POST", ["backup", "restore"]) => {
            ensure!(body["confirmed"] == true, "Revise o destino da restauração");
            let source = std::path::PathBuf::from(field(&body, "source")?);
            let destination = std::path::PathBuf::from(field(&body, "destination")?);
            let path = tokio::task::spawn_blocking(move || {
                forja_core::backup::restore(&source, &destination)
            })
            .await??;
            json!({"path":path,"activated":false})
        }
        _ => return Err(anyhow::anyhow!("Rota não encontrada").into()),
    };
    Ok(Json(result))
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use forja_core::storage::Store;
    use tower::ServiceExt;
    async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", "Bearer test")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn rejects_without_token_and_hostile_origin() {
        let d = tempfile::tempdir().unwrap();
        let state = AppState {
            engine: Engine::new(Arc::new(Store::open(d.path()).unwrap())),
            terminals: Arc::new(Terminals::default()),
            token: "test".into(),
        };
        for origin in [None, Some("https://evil.test")] {
            let mut req = axum::http::Request::builder().uri("/v1/health");
            if let Some(o) = origin {
                req = req
                    .header("authorization", "Bearer test")
                    .header("origin", o);
            }
            let r = router(state.clone())
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        }
        let r = router(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/v1/health")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn provider_failures_keep_actionable_status_and_codes() {
        for (message, status, code, retryable) in [
            (
                "Provedor respondeu HTTP 401 Unauthorized",
                StatusCode::BAD_GATEWAY,
                "provider_auth_failed",
                false,
            ),
            (
                "error sending request: connection refused",
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_unavailable",
                true,
            ),
            (
                "Formato de catálogo inválido",
                StatusCode::BAD_GATEWAY,
                "provider_invalid_response",
                true,
            ),
            (
                "Configure uma chave de API no cofre",
                StatusCode::UNPROCESSABLE_ENTITY,
                "provider_misconfigured",
                false,
            ),
            (
                "Provedor respondeu HTTP 503 Service Unavailable",
                StatusCode::BAD_GATEWAY,
                "provider_upstream_error",
                true,
            ),
        ] {
            let response = Error(anyhow::anyhow!(message)).into_response();
            assert_eq!(response.status(), status);
            let bytes = axum::body::to_bytes(response.into_body(), 64_000)
                .await
                .unwrap();
            let body: ApiError = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body.code, code);
            assert_eq!(body.retryable, retryable);
            assert!(!body.correlation_id.is_empty());
        }
    }
    #[tokio::test]
    async fn search_provider_references_are_daemon_owned_and_revision_checked() {
        let directory = tempfile::tempdir().unwrap();
        let app = router(AppState {
            engine: Engine::new(Arc::new(Store::open(directory.path()).unwrap())),
            terminals: Arc::new(Terminals::default()),
            token: "test".into(),
        });
        let (status, created) = call(
            &app,
            "POST",
            "/v1/search-providers",
            json!({
                "id":"",
                "kind":"searxng",
                "name":"Busca local",
                "base_url":"http://127.0.0.1:8080/",
                "secret_ref":"referencia-controlada-pelo-cliente",
                "enabled":true,
                "allow_local":true,
                "revision":0
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(created["secret_ref"].is_null());
        assert_eq!(created["revision"], 1);

        let id = created["id"].as_str().unwrap();
        let (status, updated) = call(
            &app,
            "POST",
            "/v1/search-providers",
            json!({
                "id":id,
                "kind":"searxng",
                "name":"Busca local atualizada",
                "base_url":"http://127.0.0.1:8080/",
                "secret_ref":"outra-referencia-forjada",
                "enabled":true,
                "allow_local":true,
                "revision":1
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(updated["secret_ref"].is_null());
        assert_eq!(updated["revision"], 2);

        let (status, _) = call(
            &app,
            "POST",
            "/v1/search-providers",
            json!({
                "id":id,
                "kind":"searxng",
                "name":"Edição antiga",
                "base_url":"http://127.0.0.1:8080/",
                "enabled":true,
                "allow_local":true,
                "revision":1
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    #[tokio::test]
    async fn model_profiles_require_context_and_workspace_sessions_are_recoverable() {
        let d = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(d.path()).unwrap());
        store
            .put(
                "workspace",
                "w",
                &Workspace {
                    id: "w".into(),
                    name: "Workspace".into(),
                    root: d.path().to_string_lossy().into(),
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
                    name: "Provider".into(),
                    kind: "ollama".into(),
                    base_url: "http://127.0.0.1:11434".into(),
                    secret_ref: None,
                    local_only: true,
                },
            )
            .unwrap();
        let app = router(AppState {
            engine: Engine::new(store),
            terminals: Arc::new(Terminals::default()),
            token: "test".into(),
        });
        let (status, _) = call(
            &app,
            "POST",
            "/v1/model-profiles",
            json!({"provider_id":"p","model_id":"m","display_name":"M","enabled":true}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status,profile)=call(&app,"POST","/v1/model-profiles",json!({"provider_id":"p","model_id":"m","display_name":"M","enabled":true,"context_window_tokens":8192,"context_source":"manual","max_output_tokens":1024,"capabilities":{"text":{"state":"supported","source":"manual"},"vision":{"state":"unsupported","source":"manual"},"tools":{"state":"supported","source":"manual"},"structured_output":{"state":"unsupported","source":"manual"},"reasoning":{"state":"unsupported","source":"manual"}}})).await;
        assert_eq!(status, StatusCode::OK);
        let (status, session) = call(
            &app,
            "POST",
            "/v1/workspaces/w/sessions",
            json!({"title":"Conversa","mode":"plan","executor_profile_id":profile["id"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(session["workspace_id"], "w");
        let (status, list) =
            call(&app, "GET", "/v1/workspaces/w/sessions?limit=10", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list["items"].as_array().unwrap().len(), 1);
        assert_eq!(list["items"][0]["id"], session["id"]);
    }
}
