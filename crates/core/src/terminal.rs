use anyhow::{ensure, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<String>>,
    #[cfg(windows)]
    _job: crate::process::Job,
}
pub struct Terminals {
    items: Mutex<HashMap<String, Pty>>,
}
impl Default for Terminals {
    fn default() -> Self {
        Self {
            items: Mutex::new(HashMap::new()),
        }
    }
}
impl Terminals {
    pub fn start(&self, root: &Path) -> Result<String> {
        let mut items = self.items.lock().unwrap();
        ensure!(items.len() < 8, "Limite de oito terminais");
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        #[cfg(windows)]
        let mut cmd = CommandBuilder::new("powershell.exe");
        #[cfg(not(windows))]
        let mut cmd = CommandBuilder::new("/bin/sh");
        #[cfg(windows)]
        cmd.args(["-NoLogo", "-NoProfile"]);
        cmd.cwd(root);
        let child = pair.slave.spawn_command(cmd)?;
        #[cfg(windows)]
        let job = crate::process::Job::assign(
            child
                .process_id()
                .ok_or_else(|| anyhow::anyhow!("PID ausente"))?,
        )?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let output = Arc::new(Mutex::new(String::new()));
        let o = output.clone();
        std::thread::spawn(move || {
            let mut b = [0u8; 4096];
            loop {
                let n = match reader.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let mut s = o.lock().unwrap();
                if s.len() > 2_000_000 {
                    let cut = s
                        .char_indices()
                        .nth(500_000)
                        .map(|(i, _)| i)
                        .unwrap_or(s.len());
                    s.drain(..cut);
                }
                s.push_str(&String::from_utf8_lossy(&b[..n]));
            }
        });
        let id = crate::id();
        items.insert(
            id.clone(),
            Pty {
                master: pair.master,
                writer,
                child,
                output,
                #[cfg(windows)]
                _job: job,
            },
        );
        Ok(id)
    }
    pub fn read(&self, id: &str) -> Result<String> {
        let items = self.items.lock().unwrap();
        let p = items
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Terminal encerrado"))?;
        let s = p.output.lock().unwrap().clone();
        Ok(s)
    }
    pub fn input(&self, id: &str, input: &str) -> Result<()> {
        ensure!(input.len() < 64_000, "Entrada grande demais");
        let mut items = self.items.lock().unwrap();
        let p = items
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Terminal encerrado"))?;
        p.writer.write_all(input.as_bytes())?;
        p.writer.flush()?;
        Ok(())
    }
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        ensure!(
            (1..=500).contains(&rows) && (1..=1000).contains(&cols),
            "Dimensão inválida"
        );
        let items = self.items.lock().unwrap();
        let p = items
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Terminal encerrado"))?;
        p.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
    pub fn close(&self, id: &str) -> Result<()> {
        if let Some(mut p) = self.items.lock().unwrap().remove(id) {
            p.child.kill()?;
        }
        Ok(())
    }
    pub fn close_all(&self) {
        let ids: Vec<_> = self.items.lock().unwrap().keys().cloned().collect();
        for id in ids {
            let _ = self.close(&id);
        }
    }
}
impl Drop for Terminals {
    fn drop(&mut self) {
        self.close_all()
    }
}
