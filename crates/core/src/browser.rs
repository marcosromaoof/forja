use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    #[cfg(windows)]
    _job: crate::process::Job,
}

impl Worker {
    async fn call(&mut self, command: &str, arguments: Value) -> Result<Value> {
        let id = crate::id();
        let request = json!({"id":id,"command":command,"args":arguments});
        self.stdin
            .write_all(serde_json::to_string(&request)?.as_bytes())
            .await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        let mut line = String::new();
        let count = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            self.stdout.read_line(&mut line),
        )
        .await??;
        ensure!(count > 0, "O processo de navegador foi encerrado");
        let response: Value =
            serde_json::from_str(&line).context("Resposta inválida do navegador")?;
        ensure!(
            response["id"] == id,
            "Resposta fora de sequência do navegador"
        );
        ensure!(
            response["ok"] == true,
            "{}",
            response["error"].as_str().unwrap_or("Falha no navegador")
        );
        Ok(response["result"].clone())
    }
}

#[derive(Default)]
pub struct Manager {
    workers: Mutex<HashMap<String, Arc<Mutex<Worker>>>>,
}

fn script_path() -> Result<PathBuf> {
    let path = std::env::var_os("FORJA_BROWSER_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/browser-worker/index.mjs")
        });
    ensure!(
        path.is_file(),
        "Browser worker não encontrado em {}",
        path.display()
    );
    Ok(path.canonicalize()?)
}

impl Manager {
    pub async fn start(&self, origins: Vec<String>) -> Result<(String, Value)> {
        ensure!(
            !origins.is_empty() && origins.len() <= 20,
            "Informe de 1 a 20 origens"
        );
        for origin in &origins {
            let url = url::Url::parse(origin)?;
            ensure!(
                ["http", "https"].contains(&url.scheme())
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.origin().ascii_serialization() == origin.trim_end_matches('/'),
                "Informe origens exatas, sem caminho"
            );
        }
        let mut command =
            Command::new(std::env::var_os("FORJA_NODE").unwrap_or_else(|| "node".into()));
        command
            .arg(script_path()?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .context("Não foi possível iniciar o browser worker")?;
        #[cfg(windows)]
        let job =
            crate::process::Job::assign(child.id().context("PID do browser worker ausente")?)?;
        let stdin = child
            .stdin
            .take()
            .context("stdin do browser worker ausente")?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .context("stdout do browser worker ausente")?,
        );
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(_)) = lines.next_line().await {}
            });
        }
        let mut worker = Worker {
            child,
            stdin,
            stdout,
            #[cfg(windows)]
            _job: job,
        };
        let result = worker.call("start", json!({"origins":origins})).await?;
        let id = crate::id();
        self.workers
            .lock()
            .await
            .insert(id.clone(), Arc::new(Mutex::new(worker)));
        Ok((id, result))
    }

    pub async fn call(&self, id: &str, command: &str, arguments: Value) -> Result<Value> {
        let worker = self
            .workers
            .lock()
            .await
            .get(id)
            .cloned()
            .context("Contexto de navegador inexistente")?;
        let result = worker.lock().await.call(command, arguments).await;
        result
    }

    pub async fn close(&self, id: &str) -> Result<Value> {
        let worker = self
            .workers
            .lock()
            .await
            .remove(id)
            .context("Contexto de navegador inexistente")?;
        let mut worker = worker.lock().await;
        let result = worker
            .call("close", json!({}))
            .await
            .unwrap_or_else(|_| json!({"closed":true}));
        let _ = worker.child.kill().await;
        Ok(result)
    }

    pub async fn close_all(&self) {
        let ids: Vec<_> = self.workers.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.close(&id).await;
        }
    }
}
