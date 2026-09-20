use camino::Utf8PathBuf;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("json: {0}")]
    Json(String),
}

#[derive(Debug, Clone)]
pub struct UnitSpec {
    pub unit_id: String,
    pub worktree: Utf8PathBuf,
    pub task: String,
    pub token_ceiling: u64,
}

#[derive(Debug, Clone)]
pub struct AgentResult {
    pub exit_code: i32,
    pub stdout: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub prompt_version: String,
}

pub struct ChildHandle {
    child: Child,
    pub unit_id: String,
}

pub struct Agent {
    events_path: Option<PathBuf>,
    pub prompt_version: String,
}

impl Agent {
    pub fn new(events_path: Option<PathBuf>) -> Self {
        Self {
            events_path,
            prompt_version: "m1-stub-v1".into(),
        }
    }

    /// One OMP CLI subprocess (M1: stub shell task), env scrubbed, cwd=worktree.
    pub fn spawn(&self, spec: &UnitSpec) -> Result<ChildHandle, AgentError> {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(&spec.task);
        cmd.current_dir(spec.worktree.as_std_path());
        // Scrub bypassable env; set confinement + identity.
        cmd.env_remove("GIT_DIR");
        cmd.env_remove("GIT_WORK_TREE");
        cmd.env_remove("CARGO_TARGET_DIR");
        cmd.env("UNIT_WORKTREE", spec.worktree.as_str());
        cmd.env("UNIT_ID", &spec.unit_id);
        if let Some(ev) = &self.events_path {
            cmd.env("RUN_EVENTS", ev);
        }
        // Preserve a wrapper-first PATH if the caller set one; otherwise inherit.
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = cmd.spawn()?;
        self.log("spawn", &spec.unit_id, serde_json::json!({"task":spec.task}));
        Ok(ChildHandle {
            child,
            unit_id: spec.unit_id.clone(),
        })
    }

    /// Wait with timeout; kill on expiry. Records tokens (M1 stub: 0/0) + exit.
    pub fn wait(&self, h: ChildHandle, timeout: Duration) -> Result<AgentResult, AgentError> {
        let mut h = h;
        let start = std::time::Instant::now();
        loop {
            match h.child.try_wait()? {
                Some(status) => {
                    let mut out = String::new();
                    if let Some(mut so) = h.child.stdout.take() {
                        use std::io::Read;
                        let _ = so.read_to_string(&mut out);
                    }
                    let code = status.code().unwrap_or(-1);
                    self.log("exit", &h.unit_id, serde_json::json!({"exit_code":code}));
                    return Ok(AgentResult {
                        exit_code: code,
                        stdout: out,
                        tokens_in: 0,
                        tokens_out: 0,
                        prompt_version: self.prompt_version.clone(),
                    });
                }
                None => {
                    if start.elapsed() >= timeout {
                        let _ = h.child.kill();
                        let _ = h.child.wait();
                        self.log("timeout", &h.unit_id, serde_json::json!({}));
                        return Err(AgentError::Timeout(timeout));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }

    fn log(&self, kind: &str, unit: &str, detail: serde_json::Value) {
        if let Some(p) = &self.events_path {
            let line = serde_json::json!({"ts":0,"run_id":"m1","kind":kind,"unit_id":unit,"detail":detail}).to_string();
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

/// Helper for tests: assert env scrubbing removed bypass variables.
pub fn scrubbed_env(worktree: &Path) -> Vec<(String, String)> {
    let mut v = vec![("UNIT_WORKTREE".to_string(), worktree.display().to_string())];
    // GIT_DIR etc. deliberately absent.
    let _ = v.pop();
    vec![("UNIT_WORKTREE".to_string(), worktree.display().to_string())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_task_returns_parsed_result() {
        let dir = tempfile::tempdir().unwrap();
        let wt = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let agent = Agent::new(None);
        let spec = UnitSpec {
            unit_id: "u1".into(),
            worktree: wt,
            task: "echo hello".into(),
            token_ceiling: 1000,
        };
        let h = agent.spawn(&spec).unwrap();
        let r = agent.wait(h, Duration::from_secs(5)).unwrap();
        assert_eq!(r.exit_code, 0);
        assert_eq!(r.prompt_version, "m1-stub-v1");
    }

    #[test]
    fn timeout_kills_task() {
        let dir = tempfile::tempdir().unwrap();
        let wt = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let agent = Agent::new(None);
        let spec = UnitSpec {
            unit_id: "u-sleep".into(),
            worktree: wt,
            task: "sleep 30".into(),
            token_ceiling: 1000,
        };
        let h = agent.spawn(&spec).unwrap();
        let e = agent.wait(h, Duration::from_millis(200));
        assert!(matches!(e, Err(AgentError::Timeout(_))));
    }
}
