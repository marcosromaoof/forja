use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

fn modern() -> String {
    "2026-07-28".into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub id: String,
    pub name: String,
    pub transport: String,
    #[serde(default = "modern")]
    pub protocol_version: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub secret_ref: Option<String>,
}
pub fn validate_config(c: &Config) -> Result<()> {
    ensure!(
        !c.name.trim().is_empty() && c.name.len() <= 120,
        "Informe o nome do servidor"
    );
    ensure!(
        ["2026-07-28", "2025-11-25"].contains(&c.protocol_version.as_str()),
        "Versão MCP não suportada"
    );
    match c.transport.as_str() {
        "http" => {
            let u = url::Url::parse(&c.url)?;
            ensure!(
                u.username().is_empty()
                    && u.password().is_none()
                    && u.fragment().is_none()
                    && u.query().is_none(),
                "URL MCP não pode conter credenciais, query ou fragmento"
            );
            ensure!(
                u.scheme() == "https"
                    || (u.scheme() == "http"
                        && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
                "Use HTTPS ou HTTP em loopback"
            );
        }
        "stdio" => {
            ensure!(
                !c.command.is_empty() && c.command.len() <= 2000 && c.args.len() <= 100,
                "Comando MCP inválido"
            );
            ensure!(
                std::path::Path::new(&c.cwd).is_absolute() && std::path::Path::new(&c.cwd).is_dir(),
                "Escolha um diretório existente e absoluto"
            );
        }
        _ => anyhow::bail!("Transporte MCP inválido"),
    }
    Ok(())
}
enum Transport {
    Http {
        config: Config,
        session: Option<String>,
    },
    Stdio {
        child: Child,
        input: ChildStdin,
        output: BufReader<ChildStdout>,
        #[cfg(windows)]
        _job: crate::process::Job,
    },
}
struct Connection {
    transport: Transport,
    version: String,
    next: u64,
    catalog: Value,
}
struct Slot {
    connection: Mutex<Connection>,
    cancel: CancellationToken,
}
pub struct Manager {
    items: Mutex<HashMap<String, Arc<Slot>>>,
    offline: AtomicBool,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            items: Mutex::new(HashMap::new()),
            offline: AtomicBool::new(false),
        }
    }
}

pub fn validate_schema(v: &Value, depth: usize) -> Result<()> {
    ensure!(depth <= 32, "Schema profundo demais");
    if let Some(o) = v.as_object() {
        for key in ["$ref", "$dynamicRef", "$recursiveRef"] {
            if let Some(r) = o.get(key).and_then(Value::as_str) {
                ensure!(
                    r.starts_with('#'),
                    "Referências de schema externas bloqueadas"
                );
            }
        }
        for value in o.values() {
            validate_schema(value, depth + 1)?;
        }
    } else if let Some(a) = v.as_array() {
        ensure!(a.len() <= 1000, "Schema grande demais");
        for value in a {
            validate_schema(value, depth + 1)?;
        }
    }
    Ok(())
}
fn response_value(bytes: &[u8], id: u64) -> Option<Value> {
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return (v["id"] == id).then_some(v);
    }
    let text = std::str::from_utf8(bytes).ok()?;
    // Only complete SSE frames are delivered. An incomplete frame remains buffered.
    let normalized = text.replace("\r\n", "\n");
    normalized
        .split("\n\n")
        .take(normalized.matches("\n\n").count())
        .find_map(|frame| {
            let data = frame
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::from_str::<Value>(&data)
                .ok()
                .filter(|v| v["id"] == id)
        })
}
impl Connection {
    async fn rpc(&mut self, method: &str, params: Value, notification: bool) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut req = json!({"jsonrpc":"2.0","method":method,"params":params});
        if !notification {
            req["id"] = json!(id);
            if self.version == "2026-07-28" {
                req["params"]["_meta"] = json!({
                    "io.modelcontextprotocol/protocolVersion":self.version,
                    "io.modelcontextprotocol/clientCapabilities":{},
                    "io.modelcontextprotocol/clientInfo":{"name":"forja","version":"0.1.0"}
                });
            }
        }
        let result = match &mut self.transport {
            Transport::Http { config, session } => {
                let mut request = crate::models::client()?
                    .post(&config.url)
                    .header("accept", "application/json, text/event-stream")
                    .header("MCP-Protocol-Version", &self.version)
                    .json(&req);
                if self.version == "2025-11-25" {
                    if let Some(s) = session.as_ref() {
                        request = request.header("Mcp-Session-Id", s);
                    }
                }
                if let Some(r) = &config.secret_ref {
                    request = request
                        .bearer_auth(keyring::Entry::new("app.forja.mcp", r)?.get_password()?);
                }
                let mut response = request.send().await?;
                ensure!(
                    response.status().is_success(),
                    "MCP HTTP {}. Verifique autenticação, versão e endpoint.",
                    response.status()
                );
                if self.version == "2025-11-25" {
                    if let Some(s) = response.headers().get("Mcp-Session-Id") {
                        let s = s.to_str()?;
                        ensure!(
                            !s.is_empty()
                                && s.len() <= 1024
                                && s.bytes().all(|b| (0x21..=0x7e).contains(&b)),
                            "Identificador de sessão MCP inválido"
                        );
                        *session = Some(s.into());
                    }
                }
                if notification {
                    return Ok(json!({}));
                }
                let mut bytes = Vec::new();
                let mut found = None;
                while let Some(chunk) = response.chunk().await? {
                    ensure!(
                        bytes.len() + chunk.len() <= 2_000_000,
                        "Resposta MCP excede 2 MB"
                    );
                    bytes.extend_from_slice(&chunk);
                    if let Some(v) = response_value(&bytes, id) {
                        found = Some(v);
                        break;
                    }
                }
                found.ok_or_else(|| {
                    anyhow::anyhow!("Resposta MCP incompleta ou sem id correspondente")
                })?
            }
            Transport::Stdio { input, output, .. } => {
                input.write_all(&serde_json::to_vec(&req)?).await?;
                input.write_all(b"\n").await?;
                input.flush().await?;
                if notification {
                    return Ok(json!({}));
                }
                let mut found = None;
                for _ in 0..100 {
                    let mut line = Vec::new();
                    loop {
                        let buf = output.fill_buf().await?;
                        ensure!(!buf.is_empty(), "Servidor MCP encerrou");
                        let n = buf
                            .iter()
                            .position(|b| *b == b'\n')
                            .map(|n| n + 1)
                            .unwrap_or(buf.len());
                        ensure!(line.len() + n <= 2_000_000, "Resposta MCP excede limite");
                        line.extend_from_slice(&buf[..n]);
                        output.consume(n);
                        if line.last() == Some(&b'\n') {
                            break;
                        }
                    }
                    let v: Value = serde_json::from_slice(&line)?;
                    if v.get("method").is_some() && v.get("id").is_some() {
                        ensure!(
                            self.version == "2025-11-25",
                            "Servidor iniciou requisição incompatível com MCP 2026"
                        );
                        let denied = json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Client operation not enabled"}}).to_string() + "\n";
                        input.write_all(denied.as_bytes()).await?;
                    } else if v["id"] == id {
                        found = Some(v);
                        break;
                    }
                }
                found
                    .ok_or_else(|| anyhow::anyhow!("Servidor MCP excedeu limite de notificações"))?
            }
        };
        ensure!(
            result["jsonrpc"] == "2.0" && result["id"] == id,
            "Envelope JSON-RPC inválido"
        );
        ensure!(
            result.get("error").is_none(),
            "MCP rejeitou a requisição (código {}). Verifique a versão e os parâmetros.",
            result["error"]["code"]
        );
        ensure!(result["result"].is_object(), "Resultado MCP inválido");
        let value = result["result"].clone();
        ensure!(
            value.get("resultType").is_none() || value["resultType"] == "complete",
            "Servidor requer capacidade MCP não habilitada; operação não será repetida"
        );
        Ok(value)
    }
}
impl Manager {
    pub async fn connect(&self, config: &Config) -> Result<Value> {
        validate_config(config)?;
        ensure!(!self.offline.load(Ordering::SeqCst), "Modo offline ativo");
        self.disconnect(&config.id).await;
        let transport = if config.transport == "http" {
            Transport::Http {
                config: config.clone(),
                session: None,
            }
        } else {
            let mut c = Command::new(&config.command);
            c.args(&config.args)
                .current_dir(&config.cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let env: Vec<_> = [
                "PATH",
                "SystemRoot",
                "WINDIR",
                "TEMP",
                "TMP",
                "PATHEXT",
                "COMSPEC",
            ]
            .iter()
            .filter_map(|k| std::env::var_os(k).map(|v| (*k, v)))
            .collect();
            c.env_clear().envs(env);
            #[cfg(windows)]
            c.creation_flags(0x08000000);
            let mut child = c.spawn()?;
            #[cfg(windows)]
            let job = crate::process::Job::assign(
                child
                    .id()
                    .ok_or_else(|| anyhow::anyhow!("Processo MCP indisponível"))?,
            )?;
            let input = child.stdin.take().unwrap();
            let output = BufReader::new(child.stdout.take().unwrap());
            Transport::Stdio {
                child,
                input,
                output,
                #[cfg(windows)]
                _job: job,
            }
        };
        let mut conn = Connection {
            transport,
            version: config.protocol_version.clone(),
            next: 0,
            catalog: json!({}),
        };
        let capabilities = if conn.version == "2026-07-28" {
            tokio::time::timeout(
                Duration::from_secs(15),
                conn.rpc("server/discover", json!({}), false),
            )
            .await??
        } else {
            let init = tokio::time::timeout(Duration::from_secs(15), conn.rpc("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"forja","version":"0.1.0"}}), false)).await??;
            ensure!(
                init["protocolVersion"] == "2025-11-25",
                "Servidor negociou uma versão não suportada"
            );
            tokio::time::timeout(
                Duration::from_secs(10),
                conn.rpc("notifications/initialized", json!({}), true),
            )
            .await??;
            init
        };
        let mut rows = Vec::new();
        let mut cursor = Value::Null;
        let mut cursors = std::collections::HashSet::new();
        if capabilities["capabilities"].get("tools").is_some() {
            loop {
                let params = if cursor.is_null() {
                    json!({})
                } else {
                    json!({"cursor":cursor})
                };
                let page = tokio::time::timeout(
                    Duration::from_secs(15),
                    conn.rpc("tools/list", params, false),
                )
                .await??;
                let tools = page["tools"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Catálogo de ferramentas inválido"))?;
                rows.extend(tools.clone());
                ensure!(
                    rows.len() <= 200 && serde_json::to_vec(&rows)?.len() <= 500_000,
                    "Catálogo MCP excedeu limite"
                );
                cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
                if cursor.is_null() {
                    break;
                }
                ensure!(
                    cursor.is_string() && cursors.insert(cursor.to_string()) && cursors.len() < 20,
                    "Paginação MCP inválida"
                );
            }
        }
        let mut names = std::collections::HashSet::new();
        for tool in &rows {
            let name = tool["name"].as_str().unwrap_or("");
            ensure!(
                !name.is_empty() && name.len() <= 128 && names.insert(name.to_owned()),
                "Nome de ferramenta MCP inválido ou duplicado"
            );
            ensure!(tool["inputSchema"].is_object(), "Ferramenta sem schema");
            validate_schema(&tool["inputSchema"], 0)?;
            jsonschema::validator_for(&tool["inputSchema"])?;
        }
        let catalog_hash = crate::hash(serde_json::to_vec(&rows)?.as_slice());
        conn.catalog = json!({"server":capabilities,"tools":rows,"connected":true,"id":config.id,"name":config.name,"protocol_version":conn.version,"catalog_hash":catalog_hash});
        let catalog = conn.catalog.clone();
        let mut items = self.items.lock().await;
        ensure!(
            !self.offline.load(Ordering::SeqCst),
            "Modo offline ativado durante a conexão"
        );
        ensure!(items.len() < 8, "Limite de oito servidores MCP conectados");
        items.insert(
            config.id.clone(),
            Arc::new(Slot {
                connection: Mutex::new(conn),
                cancel: CancellationToken::new(),
            }),
        );
        Ok(catalog)
    }
    async fn slot(&self, id: &str) -> Result<Arc<Slot>> {
        self.items
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Servidor MCP desconectado"))
    }
    pub async fn disconnect(&self, id: &str) {
        let slot = self.items.lock().await.remove(id);
        if let Some(slot) = slot {
            slot.cancel.cancel();
            let mut conn = slot.connection.lock().await;
            if let Transport::Stdio { child, .. } = &mut conn.transport {
                let _ = child.kill().await;
            }
        }
    }
    pub async fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
        if offline {
            self.disconnect_all().await;
        }
    }
    pub async fn disconnect_all(&self) {
        let ids: Vec<_> = self.items.lock().await.keys().cloned().collect();
        for id in ids {
            self.disconnect(&id).await;
        }
    }
    pub async fn catalog(&self, id: &str) -> Result<Value> {
        let slot = self.slot(id).await?;
        let value = slot.connection.lock().await.catalog.clone();
        Ok(value)
    }
    pub async fn catalogs(&self) -> Vec<Value> {
        let slots: Vec<_> = self.items.lock().await.values().cloned().collect();
        let mut out = Vec::new();
        for slot in slots {
            // A busy server must not stall model inference on another session.
            if let Ok(conn) = slot.connection.try_lock() {
                out.push(conn.catalog.clone());
            }
        }
        out
    }
    pub async fn call(
        &self,
        id: &str,
        name: &str,
        args: Value,
        catalog_hash: &str,
        cancel: CancellationToken,
    ) -> Result<Value> {
        ensure!(!self.offline.load(Ordering::SeqCst), "Modo offline ativo");
        let slot = self.slot(id).await?;
        let mut conn = tokio::select! {
            guard = slot.connection.lock() => guard,
            _ = cancel.cancelled() => anyhow::bail!("Chamada MCP cancelada antes do envio"),
            _ = slot.cancel.cancelled() => anyhow::bail!("Servidor MCP desconectado"),
        };
        ensure!(
            conn.catalog["catalog_hash"] == catalog_hash,
            "Catálogo MCP mudou; revise as permissões novamente"
        );
        let tool = conn.catalog["tools"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["name"] == name))
            .ok_or_else(|| anyhow::anyhow!("Ferramenta não anunciada"))?;
        ensure!(
            args.to_string().len() <= 256_000,
            "Argumentos grandes demais"
        );
        let validator = jsonschema::validator_for(&tool["inputSchema"])?;
        ensure!(
            validator.is_valid(&args),
            "Argumentos não correspondem ao schema MCP"
        );
        let result = tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(120), conn.rpc("tools/call", json!({"name":name,"arguments":args}), false)) => r.map_err(anyhow::Error::from).and_then(|r| r),
            _ = cancel.cancelled() => Err(anyhow::anyhow!("MCP cancelado; o efeito externo pode ter ocorrido. Reconcilie antes de repetir.")),
            _ = slot.cancel.cancelled() => Err(anyhow::anyhow!("MCP desconectado; resultado incerto. Reconcilie antes de repetir.")),
        };
        if result.is_err() {
            slot.cancel.cancel();
            if let Transport::Stdio { child, .. } = &mut conn.transport {
                let _ = child.kill().await;
            }
        }
        drop(conn);
        if result.is_err() {
            let mut items = self.items.lock().await;
            if items.get(id).is_some_and(|v| Arc::ptr_eq(v, &slot)) {
                items.remove(id);
            }
        }
        Ok(json!({"trust":"untrusted_data","content":result?}))
    }
    pub async fn list(&self, id: &str, kind: &str) -> Result<Value> {
        ensure!(
            ["resources", "prompts"].contains(&kind),
            "Catálogo inválido"
        );
        let slot = self.slot(id).await?;
        let mut conn = slot.connection.lock().await;
        ensure!(
            conn.catalog["server"]["capabilities"].get(kind).is_some(),
            "Capacidade não anunciada"
        );
        let method = format!("{kind}/list");
        tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(15), conn.rpc(&method, json!({}), false)) => r?,
            _ = slot.cancel.cancelled() => anyhow::bail!("Servidor desconectado"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_remote_refs() {
        for key in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let mut schema = json!({});
            schema[key] = json!("https://example.org/schema");
            assert!(validate_schema(&schema, 0).is_err());
        }
        assert!(validate_schema(
            &json!({"type":"object","properties":{"x":{"type":"string"}}}),
            0
        )
        .is_ok());
    }
    #[test]
    fn sse_requires_matching_complete_frame() {
        assert!(response_value(b"data: {\"id\":1,\"result\":{}}\n", 1).is_none());
        assert!(response_value(b"data: {\"id\":2,\"result\":{}}\n\n", 1).is_none());
        assert_eq!(
            response_value(
                b"event: message\r\ndata: {\"id\":1,\r\ndata: \"result\":{}}\r\n\r\n",
                1
            )
            .unwrap()["id"],
            1
        );
    }
}
