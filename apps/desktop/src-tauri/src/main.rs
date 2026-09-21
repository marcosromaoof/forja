#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use forja_core::contracts::Endpoint;
use std::time::Duration;
use tokio::sync::Mutex;

struct Bridge {
    endpoint: Mutex<Option<Endpoint>>,
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(185))
        .build()
        .map_err(|e| e.to_string())
}
fn read_endpoint() -> Option<Endpoint> {
    let text = std::fs::read_to_string(forja_core::data_dir().join("daemon.json")).ok()?;
    let endpoint: Endpoint = serde_json::from_str(&text).ok()?;
    let url = reqwest::Url::parse(&endpoint.url).ok()?;
    if endpoint.protocol != 1
        || endpoint.token.is_empty()
        || endpoint.token.len() > 512
        || url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return None;
    }
    Some(endpoint)
}
async fn healthy(client: &reqwest::Client, e: &Endpoint) -> bool {
    client
        .get(format!("{}/v1/health", e.url))
        .bearer_auth(&e.token)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}
async fn connect() -> Result<Endpoint, String> {
    let client = client()?;
    if let Some(e) = read_endpoint() {
        if healthy(&client, &e).await {
            return Ok(e);
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let daemon = exe
        .parent()
        .ok_or("Pasta do executável indisponível")?
        .join("forja-daemon.exe");
    let mut cmd = std::process::Command::new(&daemon);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().map_err(|e| {
        format!(
            "Não foi possível iniciar o daemon em {}: {e}. Compile forja-daemon primeiro.",
            daemon.display()
        )
    })?;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Some(e) = read_endpoint() {
            if healthy(&client, &e).await {
                return Ok(e);
            }
        }
    }
    Err("O daemon não respondeu.".into())
}
#[tauri::command]
async fn request(
    state: tauri::State<'_, Bridge>,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    if !path.starts_with("/v1/") || path.contains("..") || path.contains('#') || path.contains('\\')
    {
        return Err("Rota inválida".into());
    }
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return Err("Método inválido".into());
    }
    let read_only = method == "GET";
    for attempt in 0..2 {
        let endpoint = {
            let mut cached = state.endpoint.lock().await;
            if cached.is_none() {
                *cached = Some(connect().await?);
            }
            cached.as_ref().unwrap().clone()
        };
        let mut req = client()?
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
                format!("{}{}", endpoint.url, path),
            )
            .bearer_auth(&endpoint.token);
        if let Some(b) = body.as_ref() {
            req = req.json(b)
        }
        let response = req.send().await;
        if response.as_ref().is_err()
            || response
                .as_ref()
                .is_ok_and(|r| r.status() == reqwest::StatusCode::UNAUTHORIZED)
        {
            let mut cached = state.endpoint.lock().await;
            if cached.as_ref().is_some_and(|e| e.url == endpoint.url) {
                *cached = None;
            }
            drop(cached);
            if read_only && attempt == 0 {
                continue;
            }
            return Err("A conexão com o daemon foi interrompida. A operação não foi repetida; confira o estado antes de tentar novamente.".into());
        }
        let response = response.map_err(|e| e.to_string())?;
        let status = response.status();
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|_| "Resposta incompleta do daemon. A operação não foi repetida.")?;
        if !status.is_success() {
            return Err(serde_json::to_string(&value).unwrap_or_else(|_| {
                "{\"code\":\"bridge_error\",\"message\":\"Erro no daemon\"}".into()
            }));
        }
        return Ok(value);
    }
    Err("Daemon indisponível".into())
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Bridge {
            endpoint: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![request])
        .run(tauri::generate_context!())
        .expect("Erro ao iniciar FORJA");
}
