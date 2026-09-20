use parking_lot::Mutex;
use rusqlite::{params, Connection};
use rustsmith_core::{Event, Gate};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Store {
    conn: Mutex<Connection>,
    pub db_path: PathBuf,
    pub events_path: PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db_path = path.to_path_buf();
        let events_path = path
            .parent()
            .map(|p| p.join("events.jsonl"))
            .unwrap_or_else(|| PathBuf::from("events.jsonl"));
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path,
            events_path,
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_with_events(db: &Path, events: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(db)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path: db.to_path_buf(),
            events_path: events.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS runs (
  id TEXT PRIMARY KEY,
  repo_url TEXT NOT NULL,
  source_lang TEXT NOT NULL,
  status TEXT NOT NULL,
  stage TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  halt_reason TEXT
);
CREATE TABLE IF NOT EXISTS units (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  kind TEXT NOT NULL,
  spec_json TEXT NOT NULL,
  depends_on TEXT,
  status TEXT NOT NULL,
  attempts INTEGER DEFAULT 0,
  worktree TEXT,
  commit_sha TEXT,
  tokens_in INTEGER DEFAULT 0,
  tokens_out INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS gate_results (
  id INTEGER PRIMARY KEY,
  unit_id TEXT NOT NULL REFERENCES units(id),
  gate TEXT NOT NULL,
  passed INTEGER NOT NULL,
  detail_json TEXT NOT NULL,
  ran_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS decisions (
  id INTEGER PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  question TEXT NOT NULL,
  seat_positions_json TEXT NOT NULL,
  resolution TEXT NOT NULL,
  resolved_by TEXT NOT NULL,
  decided_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS optimizations (
  id INTEGER PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  round INTEGER NOT NULL,
  hotspot TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  delta_pct REAL NOT NULL,
  technique TEXT NOT NULL,
  harvest_class TEXT,
  files_touched_json TEXT NOT NULL
);
"#,
        )?;
        Ok(())
    }

    pub fn append_event(&self, ev: &Event) -> Result<(), StoreError> {
        let line = serde_json::to_string(ev)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.events_path)?;
        writeln!(f, "{line}")?;
        f.sync_all()?;
        Ok(())
    }

    pub fn record_gate(
        &self,
        unit: &str,
        gate: Gate,
        passed: bool,
        detail: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO gate_results (unit_id, gate, passed, detail_json, ran_at) VALUES (?1,?2,?3,?4,?5)",
            params![unit, gate.as_str(), passed as i32, detail.to_string(), now],
        )?;
        Ok(())
    }

    pub fn create_run(&self, id: &str, repo_url: &str, lang: &str, stage: &str) -> Result<(), StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO runs (id, repo_url, source_lang, status, stage, started_at) VALUES (?1,?2,?3,'planning',?4,?5)",
            params![id, repo_url, lang, stage, now],
        )?;
        Ok(())
    }
    pub fn create_unit(&self, id: &str, run_id: &str, kind: &str, spec: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO units (id, run_id, kind, spec_json, status) VALUES (?1,?2,?3,?4,'queued')",
            params![id, run_id, kind, spec],
        )?;
        Ok(())
    }

    pub fn set_halt(&self, run_id: &str, reason: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE runs SET status='halted', halt_reason=?1 WHERE id=?2",
            params![reason, run_id],
        )?;
        Ok(())
    }

    pub fn halt_reason(&self, run_id: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock();
        let v: Option<String> = conn
            .query_row(
                "SELECT halt_reason FROM runs WHERE id=?1",
                params![run_id],
                |r| r.get(0),
            )
            .unwrap_or(None);
        Ok(v)
    }
    /// Append-only decision log. `seat_positions_json` stores ALL positions +
    /// reasoning verbatim (minority recoverable). No UPDATE/DELETE path exists.
    pub fn insert_decision(
        &self,
        run_id: &str,
        question: &str,
        seat_positions: &serde_json::Value,
        resolution: &str,
        resolved_by: &str,
    ) -> Result<(), StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO decisions (run_id, question, seat_positions_json, resolution, resolved_by, decided_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![run_id, question, seat_positions.to_string(), resolution, resolved_by, now],
        )?;
        Ok(())
    }
    pub fn latest_decision(&self, run_id: &str) -> Result<Option<(String, String, String)>, StoreError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT question, seat_positions_json, resolution FROM decisions WHERE run_id=?1 ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query(params![run_id])?;
        if let Some(r) = rows.next()? {
            Ok(Some((r.get(0)?, r.get(1)?, r.get(2)?)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustsmith_core::Gate;

    #[test]
    fn reopen_preserves_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let ev = dir.path().join("events.jsonl");
        {
            let s = Store::open_with_events(&db, &ev).unwrap();
            s.create_run("r1", "https://example.com/x", "python", "recon")
                .unwrap();
            s.create_unit("u1", "r1", "mirror", "{}").unwrap();
            s.append_event(&Event {
                ts: 1,
                run_id: "r1".into(),
                kind: "freeze".into(),
                detail: serde_json::json!({"a":1}),
            })
            .unwrap();
            s.record_gate("u1", Gate::OracleParity, true, &serde_json::json!({}))
                .unwrap();
        }
        {
            let s = Store::open_with_events(&db, &ev).unwrap();
            assert_eq!(s.halt_reason("r1").unwrap(), None);
            let conn = s.conn.lock();
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM gate_results", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1);
        }
        let content = std::fs::read_to_string(&ev).unwrap();
        assert!(content.contains("freeze"));
    }
}
