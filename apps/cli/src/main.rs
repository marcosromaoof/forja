use clap::{Parser, Subcommand};
use serde_json::{json, Value};
#[derive(Parser)]
#[command(name = "forja", about = "Cliente local do FORJA")]
struct Args {
    #[command(subcommand)]
    command: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    Status,
    Projects,
    Sessions,
    Open {
        path: String,
    },
    Events {
        session: String,
    },
    Run {
        session: String,
        goal: String,
    },
    Cancel {
        run: String,
    },
    Approve {
        id: String,
        #[arg(long)]
        session: bool,
    },
    Deny {
        id: String,
    },
    Backup,
    /// Verify a complete local backup without connecting to the daemon.
    BackupVerify {
        source: std::path::PathBuf,
    },
    /// Restore to a new data directory; never replaces the running daemon's data.
    Restore {
        source: std::path::PathBuf,
        #[arg(long)]
        destination: std::path::PathBuf,
    },
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match &args.command {
        Cmd::BackupVerify { source } => {
            let manifest = forja_core::backup::verify(source)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"verified":true,"files":manifest.entries.len(),"created_at":manifest.created_at})
                )?
            );
            return Ok(());
        }
        Cmd::Restore {
            source,
            destination,
        } => {
            let path = forja_core::backup::restore(source, destination)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"path":path,"activated":false,"message":"Restauração validada. Para usar os dados, encerre o FORJA e configure FORJA_DATA_DIR para esta pasta antes de reiniciar."})
                )?
            );
            return Ok(());
        }
        _ => {}
    }
    let endpoint: forja_core::contracts::Endpoint = serde_json::from_str(
        &std::fs::read_to_string(forja_core::data_dir().join("daemon.json"))?,
    )?;
    let url = reqwest::Url::parse(&endpoint.url)?;
    anyhow::ensure!(
        endpoint.protocol == 1
            && !endpoint.token.is_empty()
            && endpoint.token.len() <= 512
            && url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "Endpoint local inválido"
    );
    let (method, path, body) = match args.command {
        Cmd::Status => ("GET", "/v1/health".into(), None),
        Cmd::Projects => ("GET", "/v1/workspaces".into(), None),
        Cmd::Sessions => ("GET", "/v1/sessions".into(), None),
        Cmd::Open { path } => ("POST", "/v1/workspaces".into(), Some(json!({"path":path}))),
        Cmd::Events { session } => ("GET", format!("/v1/sessions/{session}/events"), None),
        Cmd::Run { session, goal } => (
            "POST",
            format!("/v1/sessions/{session}/runs"),
            Some(json!({"goal":goal})),
        ),
        Cmd::Cancel { run } => ("POST", format!("/v1/runs/{run}/cancel"), Some(json!({}))),
        Cmd::Approve { id, session } => (
            "POST",
            format!("/v1/approvals/{id}/decision"),
            Some(json!({"decision":if session{"allow_session"}else{"allow_once"}})),
        ),
        Cmd::Deny { id } => (
            "POST",
            format!("/v1/approvals/{id}/decision"),
            Some(json!({"decision":"deny"})),
        ),
        Cmd::Backup => ("POST", "/v1/backup".into(), Some(json!({}))),
        Cmd::BackupVerify { .. } | Cmd::Restore { .. } => unreachable!(),
    };
    let mut req = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(300))
        .build()?
        .request(
            reqwest::Method::from_bytes(method.as_bytes())?,
            format!("{}{}", endpoint.url, path),
        )
        .bearer_auth(endpoint.token);
    if let Some(body) = body {
        req = req.json(&body)
    }
    let response = req.send().await?;
    let success = response.status().is_success();
    let value: Value = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    anyhow::ensure!(success, "Operação falhou");
    Ok(())
}
