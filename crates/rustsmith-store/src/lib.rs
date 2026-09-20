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
        {
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
  files_touched_json TEXT NOT NULL,
  bound TEXT NOT NULL DEFAULT '',
  tier INTEGER NOT NULL DEFAULT 0,
  ceiling_pct REAL NOT NULL DEFAULT 0,
  visible_gain_pct REAL NOT NULL DEFAULT 0,
  heldout_gain_pct REAL NOT NULL DEFAULT 0,
  divergence_pct REAL NOT NULL DEFAULT 0,
  instrument TEXT NOT NULL DEFAULT '',
  ci_low REAL,
  ci_high REAL,
  attribution_verified INTEGER NOT NULL DEFAULT 0,
  rss_delta_pct REAL NOT NULL DEFAULT 0,
  alloc_delta_pct REAL NOT NULL DEFAULT 0,
  model TEXT NOT NULL DEFAULT '',
  prompt_version TEXT NOT NULL DEFAULT '',
  guidance_version TEXT NOT NULL DEFAULT '',
  proposal_text TEXT NOT NULL DEFAULT '',
  tokens_spent INTEGER NOT NULL DEFAULT 0,
  parent_sha TEXT NOT NULL DEFAULT '',
  patch_text TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS failed_optimizations (
  id INTEGER PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  round INTEGER NOT NULL,
  hotspot TEXT NOT NULL,
  bound TEXT NOT NULL,
  tier INTEGER NOT NULL,
  technique TEXT NOT NULL,
  outcome TEXT NOT NULL,
  gate TEXT,
  measured_delta_pct REAL,
  detail_json TEXT NOT NULL,
  tokens_spent INTEGER NOT NULL,
  model TEXT NOT NULL,
  prompt_version TEXT NOT NULL,
  guidance_version TEXT NOT NULL,
  proposal_text TEXT NOT NULL,
  parent_sha TEXT NOT NULL,
  patch_text TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS rounds (
  id INTEGER PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(id),
  round INTEGER NOT NULL,
  stop_reason TEXT NOT NULL,
  gain_low REAL,
  gain_high REAL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS guidance_revisions (
  id INTEGER PRIMARY KEY,
  created_at INTEGER NOT NULL,
  guidance_version TEXT NOT NULL UNIQUE,
  change_summary TEXT NOT NULL,
  evidence_query TEXT NOT NULL,
  stats_json TEXT NOT NULL,
  runs_included_json TEXT NOT NULL,
  decided_by TEXT NOT NULL,
  prompt_diff TEXT NOT NULL
);
"#,
        )?;
        }
        self.migrate_stage2()?;
        Ok(())
    }

    /// ADD COLUMN migration for pre-M5 databases (fresh DBs already match).
    fn migrate_stage2(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        let existing: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('optimizations')")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let wants = [
            ("bound", "TEXT NOT NULL DEFAULT ''"),
            ("tier", "INTEGER NOT NULL DEFAULT 0"),
            ("ceiling_pct", "REAL NOT NULL DEFAULT 0"),
            ("visible_gain_pct", "REAL NOT NULL DEFAULT 0"),
            ("heldout_gain_pct", "REAL NOT NULL DEFAULT 0"),
            ("divergence_pct", "REAL NOT NULL DEFAULT 0"),
            ("instrument", "TEXT NOT NULL DEFAULT ''"),
            ("ci_low", "REAL"),
            ("ci_high", "REAL"),
            ("attribution_verified", "INTEGER NOT NULL DEFAULT 0"),
            ("rss_delta_pct", "REAL NOT NULL DEFAULT 0"),
            ("alloc_delta_pct", "REAL NOT NULL DEFAULT 0"),
            ("model", "TEXT NOT NULL DEFAULT ''"),
            ("prompt_version", "TEXT NOT NULL DEFAULT ''"),
            ("guidance_version", "TEXT NOT NULL DEFAULT ''"),
            ("proposal_text", "TEXT NOT NULL DEFAULT ''"),
            ("tokens_spent", "INTEGER NOT NULL DEFAULT 0"),
            ("parent_sha", "TEXT NOT NULL DEFAULT ''"),
            ("patch_text", "TEXT NOT NULL DEFAULT ''"),
        ];
        for (col, ddl) in wants {
            if !existing.iter().any(|e| e == col) {
                conn.execute(&format!("ALTER TABLE optimizations ADD COLUMN {col} {ddl}"), [])?;
            }
        }
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
    /// Unit status machine: queued->running->gated->passed/parked/abandoned.
    pub fn set_unit_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute("UPDATE units SET status=?1 WHERE id=?2", params![status, id])?;
        Ok(())
    }
    pub fn set_unit_depends(&self, id: &str, depends_json: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute("UPDATE units SET depends_on=?1 WHERE id=?2", params![depends_json, id])?;
        Ok(())
    }
    pub fn set_unit_worktree(&self, id: &str, worktree: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute("UPDATE units SET worktree=?1 WHERE id=?2", params![worktree, id])?;
        Ok(())
    }
    pub fn set_unit_commit(&self, id: &str, sha: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute("UPDATE units SET commit_sha=?1 WHERE id=?2", params![sha, id])?;
        Ok(())
    }
    /// Increment attempts, returning the new count (escalation at N=3).
    pub fn bump_attempts(&self, id: &str) -> Result<i64, StoreError> {
        let conn = self.conn.lock();
        conn.execute("UPDATE units SET attempts=attempts+1 WHERE id=?1", params![id])?;
        Ok(conn.query_row("SELECT attempts FROM units WHERE id=?1", params![id], |r| r.get(0))?)
    }
    pub fn unit_status(&self, id: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock();
        let v: Option<String> = conn
            .query_row("SELECT status FROM units WHERE id=?1", params![id], |r| r.get(0))
            .unwrap_or(None);
        Ok(v)
    }
    pub fn list_units(&self, run_id: &str) -> Result<Vec<(String, String, String)>, StoreError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, status, COALESCE(depends_on,'[]') FROM units WHERE run_id=?1")?;
        let rows = stmt.query_map(params![run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
    pub fn gate_count(&self, unit_id: &str, gate: &str, passed: bool) -> Result<i64, StoreError> {
        let conn = self.conn.lock();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM gate_results WHERE unit_id=?1 AND gate=?2 AND passed=?3",
            params![unit_id, gate, passed as i32],
            |r| r.get(0),
        )?)
    }
    pub fn count_decisions(&self, run_id: &str) -> Result<i64, StoreError> {
        let conn = self.conn.lock();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM decisions WHERE run_id=?1",
            params![run_id],
            |r| r.get(0),
        )?)
    }

    /// One row per MERGED optimization with full retrospective provenance (§17).
    /// `patch_text` is the unified diff at grade time (512 KiB cap enforced by caller).
    #[allow(clippy::too_many_arguments)]
    pub fn record_optimization(
        &self,
        run_id: &str,
        round: i64,
        hotspot: &str,
        commit_sha: &str,
        delta_pct: f64,
        technique: &str,
        harvest_class: Option<&str>,
        files_touched_json: &str,
        bound: &str,
        tier: i64,
        ceiling_pct: f64,
        visible_gain_pct: f64,
        heldout_gain_pct: f64,
        divergence_pct: f64,
        instrument: &str,
        ci_low: Option<f64>,
        ci_high: Option<f64>,
        attribution_verified: bool,
        rss_delta_pct: f64,
        alloc_delta_pct: f64,
        model: &str,
        prompt_version: &str,
        guidance_version: &str,
        proposal_text: &str,
        tokens_spent: i64,
        parent_sha: &str,
        patch_text: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO optimizations (run_id, round, hotspot, commit_sha, delta_pct, technique, harvest_class, files_touched_json, bound, tier, ceiling_pct, visible_gain_pct, heldout_gain_pct, divergence_pct, instrument, ci_low, ci_high, attribution_verified, rss_delta_pct, alloc_delta_pct, model, prompt_version, guidance_version, proposal_text, tokens_spent, parent_sha, patch_text) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)",
            params![run_id, round, hotspot, commit_sha, delta_pct, technique, harvest_class, files_touched_json, bound, tier, ceiling_pct, visible_gain_pct, heldout_gain_pct, divergence_pct, instrument, ci_low, ci_high, attribution_verified as i32, rss_delta_pct, alloc_delta_pct, model, prompt_version, guidance_version, proposal_text, tokens_spent, parent_sha, patch_text],
        )?;
        Ok(())
    }

    /// One row per REJECTED/FAILED attempt (§11 in-run + §17 cross-run memory).
    /// `patch_text` is `''` only when `outcome='rejected_at_proposal'` (no code existed).
    #[allow(clippy::too_many_arguments)]
    pub fn record_failed(
        &self,
        run_id: &str,
        round: i64,
        hotspot: &str,
        bound: &str,
        tier: i64,
        technique: &str,
        outcome: &str,
        gate: Option<&str>,
        measured_delta_pct: Option<f64>,
        detail_json: &str,
        tokens_spent: i64,
        model: &str,
        prompt_version: &str,
        guidance_version: &str,
        proposal_text: &str,
        parent_sha: &str,
        patch_text: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO failed_optimizations (run_id, round, hotspot, bound, tier, technique, outcome, gate, measured_delta_pct, detail_json, tokens_spent, model, prompt_version, guidance_version, proposal_text, parent_sha, patch_text) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![run_id, round, hotspot, bound, tier, technique, outcome, gate, measured_delta_pct, detail_json, tokens_spent, model, prompt_version, guidance_version, proposal_text, parent_sha, patch_text],
        )?;
        Ok(())
    }

    pub fn record_round(
        &self,
        run_id: &str,
        round: i64,
        stop_reason: &str,
        gain_low: Option<f64>,
        gain_high: Option<f64>,
    ) -> Result<(), StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO rounds (run_id, round, stop_reason, gain_low, gain_high, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![run_id, round, stop_reason, gain_low, gain_high, now],
        )?;
        Ok(())
    }

    /// Standard retrospective aggregations (§17.4, deterministic over store.db).
    pub fn learn_stats(&self) -> Result<serde_json::Value, StoreError> {
        let conn = self.conn.lock();
        let mut by_technique = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT bound, tier, technique, COUNT(*), AVG(visible_gain_pct) FROM (SELECT bound, tier, technique, visible_gain_pct FROM optimizations UNION ALL SELECT bound, tier, technique, measured_delta_pct FROM failed_optimizations) GROUP BY bound, tier, technique ORDER BY 4 DESC",
        )?;
        for r in stmt.query_map([], |r| {
            Ok(serde_json::json!({"bound": r.get::<_, String>(0)?, "tier": r.get::<_, i64>(1)?, "technique": r.get::<_, String>(2)?, "n": r.get::<_, i64>(3)?, "avg_gain": r.get::<_, Option<f64>>(4)?}))
        })? {
            by_technique.push(r?);
        }
        let mut calib = Vec::new();
        let mut s2 = conn.prepare(
            "SELECT bound, AVG(ceiling_pct - visible_gain_pct) FROM optimizations GROUP BY bound",
        )?;
        for r in s2.query_map([], |r| {
            Ok(serde_json::json!({"bound": r.get::<_, String>(0)?, "avg_overpredict": r.get::<_, Option<f64>>(1)?}))
        })? {
            calib.push(r?);
        }
        let mut kills = Vec::new();
        let mut s3 = conn.prepare(
            "SELECT gate, COUNT(*) FROM failed_optimizations WHERE outcome='gate_failed' GROUP BY gate",
        )?;
        for r in s3.query_map([], |r| {
            Ok(serde_json::json!({"gate": r.get::<_, Option<String>>(0)?, "n": r.get::<_, i64>(1)?}))
        })? {
            kills.push(r?);
        }
        let mut cost = Vec::new();
        let mut s4 = conn.prepare(
            "SELECT model, SUM(tokens_spent) / NULLIF(SUM(visible_gain_pct),0) FROM optimizations GROUP BY model",
        )?;
        for r in s4.query_map([], |r| {
            Ok(serde_json::json!({"model": r.get::<_, String>(0)?, "tokens_per_point": r.get::<_, Option<f64>>(1)?}))
        })? {
            cost.push(r?);
        }
        Ok(serde_json::json!({"yield_by_technique": by_technique, "ceiling_calibration": calib, "gate_kills": kills, "cost_per_point": cost}))
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
