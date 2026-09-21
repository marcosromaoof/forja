use forja_core::{agent::Engine, contracts::Endpoint, storage::Store, terminal::Terminals};
use forja_daemon::{router, AppState};
use fs2::FileExt;
use std::{
    fs::OpenOptions,
    sync::{atomic::Ordering, Arc},
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = forja_core::data_dir();
    std::fs::create_dir_all(&dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.join("daemon.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Outro daemon já está ativo"))?;
    let store = Arc::new(Store::open(&dir)?);
    let recovered = store.recover()?;
    let engine = Engine::new(store.clone());
    if recovered > 0 {
        for mut plan in store.list::<forja_core::contracts::PlanArtifact>("plan")? {
            if plan.state != "implementing" {
                continue;
            }
            let Some(run_id) = plan.implementation_run_id.clone() else {
                continue;
            };
            let Ok(run) = store.get::<forja_core::contracts::Run>("run", &run_id) else {
                continue;
            };
            if run.state != "interrupted" {
                continue;
            }
            let session: forja_core::contracts::Session = store.get("session", &plan.session_id)?;
            let workspace: forja_core::contracts::Workspace =
                store.get("workspace", &session.workspace_id)?;
            let checkpoint = forja_core::plans::checkpoint(
                &store,
                &workspace,
                &plan,
                &run_id,
                "recovered",
                "paused",
                None,
                Some("Daemon reiniciado; confirme a retomada antes de novos efeitos"),
            )?;
            plan.state = "implementation_failed".into();
            plan.updated_at = forja_core::now();
            store.put("plan", &plan.id, &plan)?;
            let _ = engine.emit(&session.id, &run_id, "implementation.recovered", serde_json::json!({"plan_id":plan.id,"checkpoint":checkpoint,"requires_resume":true}));
        }
    }
    let settings: serde_json::Value = store.get("settings", "global").unwrap_or_default();
    engine
        .offline
        .store(settings["offline"] == true, Ordering::SeqCst);
    engine.mcp.set_offline(settings["offline"] == true).await;
    engine.lsp.set_offline(settings["offline"] == true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = Endpoint {
        url: format!("http://{}", listener.local_addr()?),
        token: format!("{}{}", forja_core::id(), forja_core::id()),
        pid: std::process::id(),
        protocol: 1,
    };
    forja_core::files::atomic(
        &dir.join("daemon.json"),
        serde_json::to_string_pretty(&endpoint)?.as_bytes(),
    )?;
    let terminals = Arc::new(Terminals::default());
    let state = AppState {
        engine: engine.clone(),
        terminals: terminals.clone(),
        token: endpoint.token,
    };
    eprintln!(
        "FORJA daemon em {}. {} execução(ões) recuperada(s).",
        endpoint.url, recovered
    );
    let shutdown_engine = engine.clone();
    let shutdown_store = store.clone();
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            for plan in shutdown_store
                .list::<forja_core::contracts::PlanArtifact>("plan")
                .unwrap_or_default()
                .into_iter()
                .filter(|plan| plan.state == "implementing")
            {
                if let (Some(run_id), Ok(session)) = (
                    plan.implementation_run_id.as_deref(),
                    shutdown_store
                        .get::<forja_core::contracts::Session>("session", &plan.session_id),
                ) {
                    if let Ok(workspace) = shutdown_store
                        .get::<forja_core::contracts::Workspace>("workspace", &session.workspace_id)
                    {
                        if let Ok(checkpoint) = forja_core::plans::checkpoint(
                            &shutdown_store,
                            &workspace,
                            &plan,
                            run_id,
                            "before_shutdown",
                            "paused",
                            None,
                            Some("Estado persistido antes do encerramento do daemon"),
                        ) {
                            let _ = shutdown_engine.emit(
                                &session.id,
                                run_id,
                                "implementation.checkpoint.created",
                                serde_json::to_value(checkpoint).unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            shutdown_engine.browser.close_all().await;
            terminals.close_all();
        })
        .await?;
    Ok(())
}
