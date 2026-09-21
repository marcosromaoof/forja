use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
pub struct Job(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}
#[cfg(windows)]
impl Job {
    pub fn assign(pid: u32) -> Result<Self> {
        use windows_sys::Win32::{
            Foundation::*,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            ensure!(!h.is_null(), "CreateJobObject falhou");
            let job = Self(h);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            ensure!(
                SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32
                ) != 0,
                "Limite de Job Object falhou"
            );
            let p = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false as i32, pid);
            ensure!(!p.is_null(), "OpenProcess falhou");
            let assigned = AssignProcessToJobObject(h, p);
            CloseHandle(p);
            ensure!(
                assigned != 0,
                "Não foi possível controlar os processos filhos"
            );
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
pub async fn execute(
    root: &Path,
    command: &str,
    cwd: &str,
    timeout: u64,
    cancel: CancellationToken,
    on_output: Arc<dyn Fn(String) + Send + Sync>,
) -> Result<Value> {
    let cwd = crate::policy::resolve(root, cwd, false)?;
    ensure!(cwd.is_dir(), "Diretório inexistente");
    ensure!(
        command.len() <= 16000 && timeout <= 300 && timeout > 0,
        "Comando ou timeout inválido"
    );
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("powershell.exe");
        c.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ]);
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let allowed = [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "COMSPEC",
        "PATHEXT",
        "USERPROFILE",
        "HOME",
        "LOCALAPPDATA",
        "APPDATA",
    ];
    let env: Vec<_> = allowed
        .iter()
        .filter_map(|k| std::env::var_os(k).map(|v| (*k, v)))
        .collect();
    cmd.env_clear().envs(env);
    let mut child = cmd.spawn()?;
    #[cfg(windows)]
    let job = Job::assign(child.id().unwrap())?;
    let stdout = Arc::new(Mutex::new(String::new()));
    let stderr = Arc::new(Mutex::new(String::new()));
    async fn read_pipe<R: tokio::io::AsyncRead + Unpin>(
        mut pipe: R,
        out: Arc<Mutex<String>>,
        notify: Arc<dyn Fn(String) + Send + Sync>,
    ) {
        let mut b = [0u8; 4096];
        loop {
            let n = match pipe.read(&mut b).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let text = String::from_utf8_lossy(&b[..n]).into_owned();
            let mut s = out.lock().unwrap();
            if s.len() < 1_000_000 {
                s.push_str(&text);
                drop(s);
                notify(text);
            }
        }
    }
    let a = tokio::spawn(read_pipe(
        child.stdout.take().unwrap(),
        stdout.clone(),
        on_output.clone(),
    ));
    let b = tokio::spawn(read_pipe(
        child.stderr.take().unwrap(),
        stderr.clone(),
        on_output,
    ));
    let mut interrupted = false;
    let status = tokio::select! {r=child.wait()=>r?,_=cancel.cancelled()=>{interrupted=true;child.kill().await?;child.wait().await?},_=tokio::time::sleep(std::time::Duration::from_secs(timeout))=>{interrupted=true;child.kill().await?;child.wait().await?}};
    #[cfg(windows)]
    drop(job);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let _ = a.await;
        let _ = b.await;
    })
    .await;
    let stdout_text = stdout.lock().unwrap().clone();
    let stderr_text = stderr.lock().unwrap().clone();
    let text = format!("{stdout_text}{stderr_text}");
    Ok(
        json!({"exit_code":status.code(),"output":text,"stdout":stdout_text,"stderr":stderr_text,"interrupted":interrupted,"success":status.success()&&!interrupted}),
    )
}
pub async fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut c = Command::new("git");
    c.args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
    ])
    .args(args)
    .current_dir(root)
    .env("GIT_OPTIONAL_LOCKS", "0")
    .kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), c.output()).await??;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
