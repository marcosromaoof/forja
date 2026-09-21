use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex as AsyncMutex},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;
pub fn presets() -> Value {
    let executable = if cfg!(windows) { "node.exe" } else { "node" };
    let node = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|p| p.join(executable))
            .find(|p| p.is_file())
    });
    #[cfg(debug_assertions)]
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/node_modules");
    #[cfg(not(debug_assertions))]
    let packages = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("language-servers")))
        .unwrap_or_default();
    json!([("typescript","typescript-language-server/lib/cli.mjs"),("python","pyright/langserver.index.js")].iter().map(|(language,path)|{
        let script=packages.join(path);
        json!({"language":language,"command":node.as_ref().map(|p|p.to_string_lossy()),"args":[script.to_string_lossy(),"--stdio"],"available":node.is_some()&&script.is_file()})
    }).collect::<Vec<_>>())
}
pub struct Client {
    root: PathBuf,
    language: String,
    output: mpsc::Sender<Value>,
    pending: Pending,
    diagnostics: Arc<Mutex<HashMap<String, Value>>>,
    documents: AsyncMutex<HashMap<String, (String, u64)>>,
    process: AsyncMutex<Child>,
    stop: CancellationToken,
    next: AtomicU64,
    capabilities: Mutex<Value>,
    #[cfg(windows)]
    _job: crate::process::Job,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
fn uri(path: &Path) -> Result<String> {
    Ok(url::Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Caminho LSP inválido"))?
        .to_string())
}
fn relative_uri(root: &Path, value: &str) -> Result<String> {
    let u = url::Url::parse(value)?;
    ensure!(u.scheme() == "file", "URI de linguagem deve ser local");
    let p = u
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("URI inválida"))?
        .canonicalize()?;
    let rel = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
    crate::policy::resolve(root, &rel, false)?;
    Ok(rel)
}
async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Value> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        ensure!(header.len() < 8192, "Cabeçalho LSP excede limite");
        header.push(reader.read_u8().await?);
    }
    let header = std::str::from_utf8(&header)?;
    let mut length = None;
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                ensure!(length.is_none(), "Content-Length duplicado");
                length = Some(value.trim().parse::<usize>()?);
            }
        }
    }
    let length = length.ok_or_else(|| anyhow::anyhow!("Mensagem LSP sem Content-Length"))?;
    ensure!(length <= 4_000_000, "Mensagem LSP excede limite");
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    let v: Value = serde_json::from_slice(&bytes)?;
    ensure!(v["jsonrpc"] == "2.0", "Envelope LSP inválido");
    Ok(v)
}
impl Client {
    pub async fn start(root: &Path, config: &Config) -> Result<Arc<Self>> {
        ensure!(
            ["typescript", "python"].contains(&config.language.as_str()),
            "Linguagem não suportada"
        );
        ensure!(
            !config.command.is_empty()
                && config.command.len() <= 2000
                && config.args.len() <= 40
                && config.args.iter().all(|a| a.len() <= 4000),
            "Comando LSP inválido"
        );
        let root = root.canonicalize()?;
        ensure!(root.is_dir(), "Projeto inválido");
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let env: Vec<_> = ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "PATHEXT"]
            .iter()
            .filter_map(|k| std::env::var_os(k).map(|v| (*k, v)))
            .collect();
        command.env_clear().envs(env);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut process = command.spawn()?;
        #[cfg(windows)]
        let job = crate::process::Job::assign(
            process
                .id()
                .ok_or_else(|| anyhow::anyhow!("Processo LSP indisponível"))?,
        )?;
        let mut input = process.stdin.take().unwrap();
        let mut output = process.stdout.take().unwrap();
        let (tx, mut rx) = mpsc::channel::<Value>(128);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let stop = CancellationToken::new();
        let writer_stop = stop.clone();
        tokio::spawn(async move {
            loop {
                let value = tokio::select! {v=rx.recv()=>v,_=writer_stop.cancelled()=>break};
                let Some(value) = value else { break };
                let bytes = match serde_json::to_vec(&value) {
                    Ok(b) => b,
                    Err(_) => break,
                };
                let frame = format!("Content-Length: {}\r\n\r\n", bytes.len()).into_bytes();
                let write = async {
                    input.write_all(&frame).await?;
                    input.write_all(&bytes).await?;
                    input.flush().await
                };
                if tokio::select! {r=write=>r.is_err(),_=writer_stop.cancelled()=>true} {
                    break;
                }
            }
            writer_stop.cancel();
        });
        let reader_stop = stop.clone();
        let replies = pending.clone();
        let reports = diagnostics.clone();
        let root_path = root.clone();
        let outbound = tx.clone();
        tokio::spawn(async move {
            loop {
                let value =
                    tokio::select! {v=read_frame(&mut output)=>v,_=reader_stop.cancelled()=>break};
                let Ok(v) = value else { break };
                if let Some(method) = v["method"].as_str() {
                    if v.get("id").is_some() {
                        let response = match method {
                            "workspace/configuration" => {
                                json!({"jsonrpc":"2.0","id":v["id"],"result":v["params"]["items"].as_array().map(|a|vec![Value::Null;a.len().min(100)]).unwrap_or_default()})
                            }
                            "workspace/workspaceFolders" => {
                                json!({"jsonrpc":"2.0","id":v["id"],"result":[{"uri":uri(&root_path).unwrap_or_default(),"name":"workspace"}]})
                            }
                            _ => {
                                json!({"jsonrpc":"2.0","id":v["id"],"error":{"code":-32601,"message":"Client capability not authorized"}})
                            }
                        };
                        if outbound.try_send(response).is_err() {
                            break;
                        }
                    } else if method == "textDocument/publishDiagnostics" {
                        if let Ok(path) =
                            relative_uri(&root_path, v["params"]["uri"].as_str().unwrap_or(""))
                        {
                            if let Some(rows) = v["params"]["diagnostics"].as_array() {
                                let mut reports = reports.lock().unwrap();
                                if reports.len() < 1000 || reports.contains_key(&path) {
                                    reports.insert(path,json!({"version":v["params"]["version"],"diagnostics":rows.iter().take(500).collect::<Vec<_>>()}));
                                }
                            }
                        }
                    }
                } else if let Some(id) = v["id"].as_u64() {
                    if let Some(reply) = replies.lock().unwrap().remove(&id) {
                        let _ = reply.send(v);
                    }
                }
            }
            reader_stop.cancel();
            replies.lock().unwrap().clear();
        });
        let client = Arc::new(Self {
            root,
            language: config.language.clone(),
            output: tx,
            pending,
            diagnostics,
            documents: AsyncMutex::new(HashMap::new()),
            process: AsyncMutex::new(process),
            stop,
            next: AtomicU64::new(1),
            capabilities: Mutex::new(Value::Null),
            #[cfg(windows)]
            _job: job,
        });
        let root_uri = uri(&client.root)?;
        let result=client.request("initialize",json!({"processId":std::process::id(),"rootUri":root_uri,"workspaceFolders":[{"uri":root_uri,"name":"workspace"}],"clientInfo":{"name":"FORJA","version":"0.1.0"},"capabilities":{"general":{"positionEncodings":["utf-16"]},"workspace":{"configuration":true,"workspaceFolders":true,"applyEdit":false},"textDocument":{"synchronization":{"dynamicRegistration":false},"hover":{"contentFormat":["plaintext"]},"completion":{"completionItem":{"snippetSupport":false}},"publishDiagnostics":{"versionSupport":true}}}})).await?;
        ensure!(
            result["capabilities"]["positionEncoding"]
                .as_str()
                .unwrap_or("utf-16")
                == "utf-16",
            "Servidor negociou codificação de posição não suportada"
        );
        *client.capabilities.lock().unwrap() = result["capabilities"].clone();
        client.notify("initialized", json!({})).await?;
        Ok(client)
    }
    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        tokio::select! {r=self.output.send(json!({"jsonrpc":"2.0","method":method,"params":params}))=>r.map_err(|_|anyhow::anyhow!("Servidor LSP encerrou")),_=self.stop.cancelled()=>anyhow::bail!("Servidor LSP desconectado")}
    }
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            ensure!(pending.len() < 128, "Fila LSP cheia");
            pending.insert(id, tx);
        }
        let exchange = async {
            self.output
                .send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .await?;
            let value = rx.await?;
            ensure!(
                value.get("error").is_none(),
                "Servidor de linguagem recusou a consulta"
            );
            Ok::<_, anyhow::Error>(value["result"].clone())
        };
        let result = tokio::select! {r=tokio::time::timeout(Duration::from_secs(20),exchange)=>r.map_err(anyhow::Error::from).and_then(|r|r),_=self.stop.cancelled()=>Err(anyhow::anyhow!("Servidor LSP desconectado"))};
        self.pending.lock().unwrap().remove(&id);
        if result.is_err() {
            let _ = self
                .output
                .try_send(json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
        }
        result
    }
    pub async fn sync(&self, path: &str, text: &str) -> Result<u64> {
        ensure!(text.len() <= 2_000_000, "Documento excede 2 MB");
        let full = crate::policy::resolve(&self.root, path, false)?;
        let ext = full.extension().and_then(|s| s.to_str()).unwrap_or("");
        let language = match (self.language.as_str(), ext) {
            ("python", "py") => "python",
            ("typescript", "ts") => "typescript",
            ("typescript", "tsx") => "typescriptreact",
            ("typescript", "js" | "mjs" | "cjs") => "javascript",
            ("typescript", "jsx") => "javascriptreact",
            _ => anyhow::bail!("Extensão incompatível com servidor de linguagem"),
        };
        let mut documents = self.documents.lock().await;
        ensure!(
            documents.len() < 100 || documents.contains_key(path),
            "Limite de documentos abertos no LSP"
        );
        let hash = crate::hash(text.as_bytes());
        let previous = documents.get(path);
        if previous.is_some_and(|(old, _)| old == &hash) {
            return Ok(previous.unwrap().1);
        }
        let version = previous.map(|(_, v)| v + 1).unwrap_or(1);
        let file_uri = uri(&full)?;
        if previous.is_none() {
            self.notify("textDocument/didOpen",json!({"textDocument":{"uri":file_uri,"languageId":language,"version":version,"text":text}})).await?;
        } else {
            self.notify("textDocument/didChange",json!({"textDocument":{"uri":file_uri,"version":version},"contentChanges":[{"text":text}]})).await?;
        }
        documents.insert(path.into(), (hash, version));
        Ok(version)
    }
    pub async fn query(
        &self,
        path: &str,
        text: &str,
        method: &str,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        ensure!(
            [
                "textDocument/hover",
                "textDocument/completion",
                "textDocument/definition",
                "textDocument/documentSymbol"
            ]
            .contains(&method),
            "Operação LSP não autorizada"
        );
        let selected = text.lines().nth(line as usize).unwrap_or("");
        ensure!(
            (line as usize) <= text.lines().count()
                && (character as usize) <= selected.encode_utf16().count(),
            "Posição fora do documento"
        );
        self.sync(path, text).await?;
        let full = crate::policy::resolve(&self.root, path, false)?;
        let result=self.request(method,json!({"textDocument":{"uri":uri(&full)?},"position":{"line":line,"character":character}})).await?;
        if method == "textDocument/definition" {
            let values = if result.is_array() {
                result.as_array().unwrap().clone()
            } else if result.is_object() {
                vec![result]
            } else {
                vec![]
            };
            return Ok(json!(values.into_iter().filter_map(|v|{
                let target=v["uri"].as_str().or_else(||v["targetUri"].as_str())?;
                let path=relative_uri(&self.root,target).ok()?;
                Some(json!({"path":path,"range":v.get("range").or_else(||v.get("targetSelectionRange")).cloned().unwrap_or(Value::Null)}))
            }).collect::<Vec<_>>()));
        }
        Ok(result)
    }
    pub async fn diagnostics(&self) -> Value {
        let documents = self.documents.lock().await;
        let reports = self.diagnostics.lock().unwrap();
        json!(reports
            .iter()
            .filter_map(|(path, report)| {
                let (_, version) = documents.get(path)?;
                if report["version"].as_u64().is_some_and(|v| v != *version) {
                    return None;
                }
                Some(json!({"path":path,"version":version,"diagnostics":report["diagnostics"]}))
            })
            .collect::<Vec<_>>())
    }
    pub async fn close(&self) {
        self.stop.cancel();
        let _ = self.process.lock().await.kill().await;
    }
    pub fn active(&self) -> bool {
        !self.stop.is_cancelled()
    }
}
#[derive(Default)]
pub struct Manager {
    clients: AsyncMutex<HashMap<String, Arc<Client>>>,
    offline: AtomicBool,
}
impl Manager {
    fn key(workspace: &str, language: &str) -> String {
        format!("{workspace}:{language}")
    }
    pub async fn start(&self, workspace: &str, root: &Path, config: &Config) -> Result<()> {
        ensure!(
            !self.offline.load(Ordering::SeqCst),
            "LSP nativo bloqueado em modo offline"
        );
        self.stop(workspace, &config.language).await;
        let client = Client::start(root, config).await?;
        let mut clients = self.clients.lock().await;
        ensure!(
            !self.offline.load(Ordering::SeqCst) && clients.len() < 4,
            "LSP indisponível ou limite de quatro servidores atingido"
        );
        clients.insert(Self::key(workspace, &config.language), client);
        Ok(())
    }
    pub async fn get(&self, workspace: &str, language: &str) -> Result<Arc<Client>> {
        self.clients
            .lock()
            .await
            .get(&Self::key(workspace, language))
            .filter(|c| c.active())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Servidor de linguagem não conectado"))
    }
    pub async fn stop(&self, workspace: &str, language: &str) {
        let c = self
            .clients
            .lock()
            .await
            .remove(&Self::key(workspace, language));
        if let Some(c) = c {
            c.close().await;
        }
    }
    pub async fn status(&self, workspace: &str) -> Value {
        let clients = self.clients.lock().await;
        json!(["typescript","python"].iter().map(|language|json!({"language":language,"active":clients.get(&Self::key(workspace,language)).is_some_and(|c|c.active())})).collect::<Vec<_>>())
    }
    pub async fn set_offline(&self, value: bool) {
        self.offline.store(value, Ordering::SeqCst);
        if value {
            let clients = std::mem::take(&mut *self.clients.lock().await);
            for c in clients.into_values() {
                c.close().await;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_is_bounded_and_counts_utf8_bytes() {
        let bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":"ação"})).unwrap();
        let mut frame = format!("Content-Length: {}\r\n\r\n", bytes.len()).into_bytes();
        frame.extend(bytes);
        assert_eq!(
            read_frame(&mut frame.as_slice()).await.unwrap()["result"],
            "ação"
        );
        assert!(
            read_frame(&mut b"Content-Length: 99999999\r\n\r\n".as_slice())
                .await
                .is_err()
        );
    }
}
