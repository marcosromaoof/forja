use crate::contracts::Event;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct Store {
    conn: Mutex<Connection>,
    blobs: Mutex<()>,
    pub dir: PathBuf,
}
impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        anyhow::ensure!(
            !dir.join("RESTORE_INCOMPLETE").exists(),
            "Restauração incompleta; use uma cópia validada"
        );
        std::fs::create_dir_all(dir.join("blobs"))?;
        let conn = Connection::open(dir.join("forja.sqlite"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        anyhow::ensure!(
            version <= 2,
            "Banco criado por uma versão mais recente; atualização necessária"
        );
        let existing: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table')",
            [],
            |r| r.get(0),
        )?;
        if version < 2 && existing {
            let backup = dir.join(format!("before-migration-{}.sqlite", crate::id()));
            conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS documents(kind TEXT NOT NULL,id TEXT NOT NULL,body TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 1,updated_at TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE TABLE IF NOT EXISTS events(event_id TEXT PRIMARY KEY,session_id TEXT NOT NULL,run_id TEXT NOT NULL,sequence INTEGER NOT NULL,body TEXT NOT NULL,UNIQUE(run_id,sequence));
            CREATE INDEX IF NOT EXISTS events_session ON events(session_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS file_index USING fts5(workspace UNINDEXED,path,content);
            COMMIT;")?;
        if version < 2 {
            let has_session_sequence: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('events') WHERE name='session_sequence')",
                [], |r| r.get(0)
            )?;
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            if !has_session_sequence {
                conn.execute("ALTER TABLE events ADD COLUMN session_sequence INTEGER", [])?;
            }
            let mut rows = {
                let mut stmt =
                    conn.prepare("SELECT rowid,session_id FROM events ORDER BY rowid")?;
                let collected = stmt
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                collected
            };
            let mut sequences = std::collections::HashMap::<String, i64>::new();
            for (rowid, session_id) in rows.drain(..) {
                let next = sequences
                    .entry(session_id)
                    .and_modify(|v| *v += 1)
                    .or_insert(1);
                conn.execute(
                    "UPDATE events SET session_sequence=?1 WHERE rowid=?2",
                    params![*next, rowid],
                )?;
            }
            conn.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS events_session_sequence ON events(session_id,session_sequence); PRAGMA user_version=2; COMMIT;")?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
            blobs: Mutex::new(()),
            dir: dir.into(),
        })
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<()> {
        self.conn.lock().unwrap().execute("INSERT INTO documents(kind,id,body,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body,version=version+1,updated_at=excluded.updated_at",params![kind,id,serde_json::to_string(value)?,crate::now()])?;
        Ok(())
    }
    pub fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        let body: String = self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT body FROM documents WHERE kind=?1 AND id=?2",
                params![kind, id],
                |r| r.get(0),
            )
            .context("Registro não encontrado")?;
        Ok(serde_json::from_str(&body)?)
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT body FROM documents WHERE kind=?1 ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([kind], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn delete(&self, kind: &str, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM documents WHERE kind=?1 AND id=?2",
            params![kind, id],
        )?;
        Ok(())
    }
    /// Atomic read/modify/write for a small collection of configuration documents.
    /// The callback must not call back into Store while its connection is locked.
    pub(crate) fn edit_documents<T: Serialize + DeserializeOwned, R>(
        &self,
        kind: &str,
        edit: impl FnOnce(&mut Vec<T>) -> Result<R>,
    ) -> Result<R> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let original: std::collections::HashMap<String, String> = {
            let mut stmt = tx.prepare("SELECT id,body FROM documents WHERE kind=?1")?;
            let rows = stmt.query_map([kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let mut documents: Vec<T> = original
            .values()
            .map(|v| serde_json::from_str(v))
            .collect::<std::result::Result<_, _>>()?;
        let result = edit(&mut documents)?;
        let mut ids = std::collections::HashSet::new();
        for document in documents {
            let value = serde_json::to_value(document)?;
            let id = value["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .context("Documento sem identificador")?;
            anyhow::ensure!(
                ids.insert(id.to_owned()),
                "Identificador de documento duplicado"
            );
            let body = serde_json::to_string(&value)?;
            if original.get(id) != Some(&body) {
                tx.execute("INSERT INTO documents(kind,id,body,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body,version=version+1,updated_at=excluded.updated_at",params![kind,id,body,crate::now()])?;
            }
        }
        for id in original.keys().filter(|id| !ids.contains(*id)) {
            tx.execute(
                "DELETE FROM documents WHERE kind=?1 AND id=?2",
                params![kind, id],
            )?;
        }
        tx.commit()?;
        Ok(result)
    }
    pub fn append(&self, session: &str, run: &str, kind: &str, payload: Value) -> Result<Event> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let sequence: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence),0)+1 FROM events WHERE run_id=?1",
            [run],
            |r| r.get(0),
        )?;
        let session_sequence: i64 = tx.query_row(
            "SELECT COALESCE(MAX(session_sequence),0)+1 FROM events WHERE session_id=?1",
            [session],
            |r| r.get(0),
        )?;
        let event = Event {
            event_id: crate::id(),
            schema_version: 1,
            session_id: session.into(),
            run_id: run.into(),
            sequence,
            session_sequence,
            timestamp: crate::now(),
            r#type: kind.into(),
            payload,
        };
        tx.execute(
            "INSERT INTO events(event_id,session_id,run_id,sequence,body,session_sequence) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                event.event_id,
                session,
                run,
                sequence,
                serde_json::to_string(&event)?,
                session_sequence
            ],
        )?;
        tx.commit()?;
        Ok(event)
    }
    pub fn events(&self, session: &str, run: Option<&str>, after: i64) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare("SELECT body,session_sequence FROM events WHERE session_id=?1 AND (?2 IS NULL OR run_id=?2) AND session_sequence>?3 ORDER BY session_sequence")?;
        let rows = stmt.query_map(params![session, run, after], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (body, sequence) = row?;
            let mut event: Event = serde_json::from_str(&body)?;
            event.session_sequence = sequence;
            Ok(event)
        })
        .collect()
    }
    pub fn blob(&self, bytes: &[u8]) -> Result<String> {
        use std::io::Write;
        let _lock = self.blobs.lock().unwrap();
        let hash = crate::hash(bytes);
        let p = self.dir.join("blobs").join(&hash);
        if !p.exists() {
            let temporary = self
                .dir
                .join("blobs")
                .join(format!(".pending-{}", crate::id()));
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(temporary, p)?;
        } else {
            anyhow::ensure!(
                crate::hash(&std::fs::read(&p)?) == hash,
                "Blob existente corrompido"
            );
        }
        Ok(hash)
    }
    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        anyhow::ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Hash inválido"
        );
        let bytes = std::fs::read(self.dir.join("blobs").join(hash))?;
        anyhow::ensure!(crate::hash(&bytes) == hash, "Integridade do blob inválida");
        Ok(bytes)
    }
    pub fn index(&self, workspace: &str, path: &str, content: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM file_index WHERE workspace=?1 AND path=?2",
            params![workspace, path],
        )?;
        tx.execute(
            "INSERT INTO file_index(workspace,path,content) VALUES(?1,?2,?3)",
            params![workspace, path, content],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn replace_index(
        &self,
        workspace: &str,
        files: &[(String, String)],
        map: &Value,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM file_index WHERE workspace=?1", [workspace])?;
        {
            let mut insert =
                tx.prepare("INSERT INTO file_index(workspace,path,content) VALUES(?1,?2,?3)")?;
            for (path, content) in files {
                insert.execute(params![workspace, path, content])?;
            }
        }
        tx.execute("INSERT INTO documents(kind,id,body,updated_at) VALUES('repo_map',?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body,version=version+1,updated_at=excluded.updated_at",params![workspace,serde_json::to_string(map)?,crate::now()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn search_index(&self, workspace: &str, query: &str) -> Result<Vec<Value>> {
        anyhow::ensure!(
            !query.trim().is_empty() && query.len() <= 1000,
            "Consulta inválida"
        );
        let query = format!("\"{}\"", query.replace('"', "\"\""));
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare("SELECT path,snippet(file_index,2,'','',' … ',32) FROM file_index WHERE workspace=?1 AND file_index MATCH ?2 LIMIT 50")?;
        let rows = stmt.query_map(params![workspace, query], |r| {
            Ok(json!({"path":r.get::<_,String>(0)?,"excerpt":r.get::<_,String>(1)?}))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn backup(&self) -> Result<PathBuf> {
        let conn = self.conn.lock().unwrap();
        let _blobs = self.blobs.lock().unwrap();
        let parent = self.dir.join("backups");
        std::fs::create_dir_all(&parent)?;
        let id = crate::id();
        let pending = parent.join(format!(".pending-{id}"));
        let complete = parent.join(format!("backup-{id}"));
        std::fs::create_dir(&pending)?;
        conn.execute(
            "VACUUM INTO ?1",
            [pending.join("forja.sqlite").to_string_lossy().as_ref()],
        )?;
        crate::backup::complete(&pending, &self.dir.join("blobs"))?;
        std::fs::rename(pending, &complete)?;
        Ok(complete)
    }
    pub fn recover(&self) -> Result<usize> {
        let mut count = 0;
        for mut run in self.list::<crate::contracts::Run>("run")? {
            if !["completed", "cancelled", "failed", "interrupted"].contains(&run.state.as_str()) {
                run.state = "interrupted".into();
                self.put("run", &run.id, &run)?;
                self.append(&run.session_id,&run.id,"run.interrupted",json!({"message":"Execução interrompida. Inspecione as alterações antes de iniciar outra execução; nenhuma ferramenta foi repetida."}))?;
                count += 1;
            }
        }
        for mut approval in self.list::<crate::contracts::Approval>("approval")? {
            if approval.state == "pending" {
                approval.state = "expired".into();
                self.put("approval", &approval.id, &approval)?;
            }
        }
        Ok(count)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_backup_and_newer_schema_guard() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("forja.sqlite");
        let legacy = Connection::open(&db).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES('user data');",
            )
            .unwrap();
        drop(legacy);
        drop(Store::open(d.path()).unwrap());
        let backup = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("before-migration-")
            })
            .unwrap();
        let before = Connection::open(backup).unwrap();
        assert_eq!(
            before
                .query_row("SELECT value FROM legacy", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "user data"
        );
        let future = Connection::open(&db).unwrap();
        future.execute_batch("PRAGMA user_version=42;").unwrap();
        assert!(Store::open(d.path()).is_err());
        assert_eq!(
            future
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            42
        );
    }
    #[test]
    fn incomplete_restore_and_corrupted_blob_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        let hash = s.blob(b"original").unwrap();
        std::fs::write(d.path().join("blobs").join(&hash), b"corrupted").unwrap();
        assert!(s.read_blob(&hash).is_err());
        assert!(s.blob(b"original").is_err());
        assert!(s.backup().is_err());
        drop(s);
        std::fs::write(d.path().join("RESTORE_INCOMPLETE"), b"incomplete").unwrap();
        assert!(Store::open(d.path()).is_err());
    }
    #[test]
    fn replay_and_recovery_do_not_repeat() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        let run = crate::contracts::Run {
            id: "r".into(),
            session_id: "s".into(),
            goal: "x".into(),
            state: "running".into(),
            created_at: crate::now(),
            max_turns: 4,
            selected_skills: vec![],
            mode: None,
            executor_profile_id: None,
            reviewer_profile_ids: vec![],
            reasoning_level: None,
            context_revision: 0,
            resumed_from_run_id: None,
        };
        s.put("run", "r", &run).unwrap();
        assert_eq!(s.append("s", "r", "test", json!({})).unwrap().sequence, 1);
        assert_eq!(s.append("s", "r", "test", json!({})).unwrap().sequence, 2);
        assert_eq!(
            s.append("s", "other", "test", json!({})).unwrap().sequence,
            1
        );
        assert_eq!(s.recover().unwrap(), 1);
        assert_eq!(s.recover().unwrap(), 0);
        assert_eq!(s.events("s", Some("r"), 1).unwrap().len(), 2);
        let page = s.events("s", None, 2).unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].session_sequence, 3);
        assert_eq!(page[1].session_sequence, 4);
    }
    #[test]
    fn blob_traversal_denied() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        assert!(s.read_blob("../forja.sqlite").is_err());
        let h = s.blob(b"abc").unwrap();
        assert_eq!(s.read_blob(&h).unwrap(), b"abc");
    }
}
