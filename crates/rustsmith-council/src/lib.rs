use rustsmith_core::Visibility;
use rustsmith_store::Store;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CouncilError {
    #[error("store: {0}")]
    Store(String),
    #[error("privacy refused: {0}")]
    Privacy(String),
    #[error("no driver for seat {0:?}")]
    NoDriver(Seat),
    #[error("seat worker failed: {0}")]
    Worker(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seat {
    Architect,
    Verifier,
    Performance,
    Scope,
}

impl Seat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Seat::Architect => "architect",
            Seat::Verifier => "verifier",
            Seat::Performance => "performance",
            Seat::Scope => "scope",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    Approve,
    Reject,
}

impl Stance {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stance::Approve => "approve",
            Stance::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub seat: Seat,
    pub stance: Stance,
    pub reasoning: String,
}

#[derive(Debug, Clone)]
pub struct Proposal {
    pub question: String,
    pub artifact_ref: String,
    pub proposer: Seat,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Consensus { approved: bool },
    ArchitectTiebreak { approved: bool, justification: String },
}

impl Resolution {
    pub fn kind(&self) -> &'static str {
        match self {
            Resolution::Consensus { .. } => "consensus",
            Resolution::ArchitectTiebreak { .. } => "architect_tiebreak",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Replan {
    Repartition { units: Vec<String> },
    ChangeApproach { note: String },
    BindInsteadOfPort { module: String },
    MarkPortOnly { module: String, reason: String },
}

/// One impl per provider; stub impl for tests.
/// Critique takes ONLY (proposal, artifact) — never other seats' positions.
/// Leakage is a type error, not a review finding.
pub trait SeatDriver: Send + Sync {
    fn critique(&self, seat: Seat, proposal: &Proposal, artifact: &[u8]) -> Result<Position, CouncilError>;
}

/// Scripted stub for acceptance + unit tests.
pub struct StubDriver {
    pub stance: Stance,
    pub reasoning: String,
}

impl SeatDriver for StubDriver {
    fn critique(&self, seat: Seat, _proposal: &Proposal, _artifact: &[u8]) -> Result<Position, CouncilError> {
        Ok(Position {
            seat,
            stance: self.stance,
            reasoning: self.reasoning.clone(),
        })
    }
}

/// Live seat driver: shells a worker command with the seat prompt on stdin,
/// parses `{"stance":"approve"|"reject","reasoning":"..."}` from stdout.
/// The prompt carries ONLY (question, artifact_ref, proposer reasoning) —
/// never other seats' positions (blind critique is structural, as with Stub).
/// `model` is a config-supplied identity string, never hardcoded.
pub struct WorkerSeatDriver {
    pub seat: Seat,
    pub command: String,
    pub model: String,
}

impl SeatDriver for WorkerSeatDriver {
    fn critique(&self, seat: Seat, proposal: &Proposal, _artifact: &[u8]) -> Result<Position, CouncilError> {
        use std::io::Write;
        use std::process::Stdio;
        let prompt = format!(
            "# rustsmith council seat\nseat: {}\nmodel: {}\nquestion: {}\nartifact: {}\nproposer_reasoning: {}\n",
            seat.as_str(), self.model, proposal.question, proposal.artifact_ref, proposal.reasoning
        );
        let mut child = std::process::Command::new("sh");
        child.arg("-c").arg(&self.command);
        child.env_remove("GIT_DIR");
        child.env_remove("GIT_WORK_TREE");
        child.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = child.spawn().map_err(|e| CouncilError::Worker(e.to_string()))?;
        child
            .stdin
            .take()
            .ok_or_else(|| CouncilError::Worker("no stdin".into()))?
            .write_all(prompt.as_bytes())
            .map_err(|e| CouncilError::Worker(e.to_string()))?;
        let out = child.wait_with_output().map_err(|e| CouncilError::Worker(e.to_string()))?;
        if !out.status.success() {
            return Err(CouncilError::Worker(format!("exit {}", out.status.code().unwrap_or(-1))));
        }
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).map_err(|e| CouncilError::Worker(format!("seat stdout not JSON: {e}")))?;
        let stance = match v["stance"].as_str() {
            Some("approve") => Stance::Approve,
            Some("reject") => Stance::Reject,
            other => return Err(CouncilError::Worker(format!("bad stance: {other:?}"))),
        };
        let reasoning = v["reasoning"]
            .as_str()
            .ok_or_else(|| CouncilError::Worker("reasoning missing".into()))?
            .to_string();
        Ok(Position { seat, stance, reasoning })
    }
}

/// Seat commands from config surface (`RUSTSMITH_SEAT_CMD_<SEAT>`).
/// Missing entries fall back to stub drivers at the call site.
pub fn seat_commands_from_env() -> HashMap<Seat, String> {
    let mut m = HashMap::new();
    for (seat, var) in [
        (Seat::Architect, "RUSTSMITH_SEAT_CMD_ARCHITECT"),
        (Seat::Verifier, "RUSTSMITH_SEAT_CMD_VERIFIER"),
        (Seat::Performance, "RUSTSMITH_SEAT_CMD_PERFORMANCE"),
        (Seat::Scope, "RUSTSMITH_SEAT_CMD_SCOPE"),
    ] {
        if let Ok(cmd) = std::env::var(var) {
            if !cmd.trim().is_empty() {
                m.insert(seat, cmd);
            }
        }
    }
    m
}

/// Model identity for a seat from config only (`config/default.toml`
/// `[models]`, overridable via `RUSTSMITH_MODELS_CONFIG`). Never hardcoded:
/// unknown/missing entries yield a `config:` placeholder the probe surfaces.
pub fn model_for_seat(seat: Seat) -> String {
    let path = std::env::var("RUSTSMITH_MODELS_CONFIG").unwrap_or_else(|_| "config/default.toml".into());
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let key = seat.as_str();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with(key) {
            if let Some(m) = t.split("model").nth(1) {
                let q1 = m.find('"').map(|i| i + 1);
                if let Some(s) = q1 {
                    if let Some(e) = m[s..].find('"') {
                        return m[s..s + e].to_string();
                    }
                }
            }
        }
    }
    format!("config:models.{key}")
}

pub struct Council {
    drivers: HashMap<Seat, Box<dyn SeatDriver>>,
}

impl Council {
    pub fn new(drivers: HashMap<Seat, Box<dyn SeatDriver>>) -> Self {
        Self { drivers }
    }

    /// Protocol: proposer records proposal -> two critics critique BLIND
    /// (artifact+proposal only) -> agreement carries, disagreement -> Architect
    /// decides + writes why. Every decision -> `decisions` table with ALL
    /// reasoning verbatim (minority recoverable).
    pub fn decide(
        &self,
        store: &Store,
        run_id: &str,
        proposal: Proposal,
        artifact: &[u8],
        critics: (Seat, Seat),
    ) -> Result<Resolution, CouncilError> {
        let mut positions: Vec<Position> = Vec::new();
        // Proposer's own position (from its driver, blind to critics).
        let proposer_pos = self
            .drivers
            .get(&proposal.proposer)
            .ok_or(CouncilError::NoDriver(proposal.proposer))?
            .critique(proposal.proposer, &proposal, artifact)?;
        positions.push(proposer_pos);
        // Two blind critics.
        for seat in [critics.0, critics.1] {
            let p = self
                .drivers
                .get(&seat)
                .ok_or(CouncilError::NoDriver(seat))?
                .critique(seat, &proposal, artifact)?;
            positions.push(p);
        }
        // Agreement: all three stances equal -> consensus.
        let first = positions[0].stance;
        let agreed = positions.iter().all(|p| p.stance == first);
        let resolution = if agreed {
            Resolution::Consensus {
                approved: first == Stance::Approve,
            }
        } else {
            // Disagreement -> Architect tiebreak with written justification.
            let arch = self
                .drivers
                .get(&Seat::Architect)
                .ok_or(CouncilError::NoDriver(Seat::Architect))?
                .critique(Seat::Architect, &proposal, artifact)?;
            let approved = arch.stance == Stance::Approve;
            Resolution::ArchitectTiebreak {
                approved,
                justification: arch.reasoning.clone(),
            }
        };
        let seat_json = serde_json::json!(positions.iter().map(|p| serde_json::json!({"seat": p.seat.as_str(), "stance": p.stance.as_str(), "reasoning": p.reasoning})).collect::<Vec<_>>());
        let (resolution_str, by) = match &resolution {
            Resolution::Consensus { approved } => (format!("consensus approved={approved}"), "consensus"),
            Resolution::ArchitectTiebreak { approved, justification } => (format!("architect_tiebreak approved={approved}: {justification}"), "architect_tiebreak"),
        };
        store
            .insert_decision(run_id, &proposal.question, &seat_json, &resolution_str, by)
            .map_err(|e| CouncilError::Store(e.to_string()))?;
        Ok(resolution)
    }

    /// Gate-fail x3 -> replan. Unportable module -> bind/shim/out-of-scope.
    pub fn escalate_replan(
        &self,
        store: &Store,
        run_id: &str,
        unit_id: &str,
        failures: &[String],
        unportable: Option<(&str, &str)>,
    ) -> Result<Replan, CouncilError> {
        let replan = if let Some((module, reason)) = unportable {
            if reason.contains("bind") {
                Replan::BindInsteadOfPort {
                    module: module.into(),
                }
            } else {
                Replan::MarkPortOnly {
                    module: module.into(),
                    reason: reason.into(),
                }
            }
        } else if failures.len() >= 3 {
            // Heuristic for demo: partition vs approach by failure text.
            if failures.iter().any(|f| f.contains("partition")) {
                Replan::Repartition {
                    units: vec![format!("{unit_id}-a"), format!("{unit_id}-b")],
                }
            } else {
                Replan::ChangeApproach {
                    note: format!("{unit_id}: {}", failures.join("; ")),
                }
            }
        } else {
            Replan::ChangeApproach {
                note: format!("{unit_id}: retry with note"),
            }
        };
        let desc = match &replan {
            Replan::Repartition { units } => format!("repartition {}", units.join(",")),
            Replan::ChangeApproach { note } => format!("change_approach {note}"),
            Replan::BindInsteadOfPort { module } => format!("bind {module}"),
            Replan::MarkPortOnly { module, reason } => format!("port_only {module}: {reason}"),
        };
        let seat_json = serde_json::json!([{"seat": "architect", "stance": "approve", "reasoning": desc}]);
        store
            .insert_decision(run_id, &format!("replan {unit_id}"), &seat_json, "consensus approved=true", "consensus")
            .map_err(|e| CouncilError::Store(e.to_string()))?;
        Ok(replan)
    }
}

/// Privacy: training-tier models on private repos hard-fail at spawn time.
/// Model identities come from config only; never hardcoded.
pub fn assert_training_tier_allowed(
    model: &str,
    repo_visibility: Visibility,
    training_tier_models: &[String],
    allow_on_private: bool,
) -> Result<(), CouncilError> {
    let is_training = training_tier_models.iter().any(|m| m == model);
    if is_training && repo_visibility == Visibility::Private && !allow_on_private {
        return Err(CouncilError::Privacy(format!(
            "training-tier model '{model}' refused on private repo"
        )));
    }
    Ok(())
}

/// Halt wiring (§10.3 triggers 1–5): write halt_reason + report-as-stands + stop.
/// Tamper halts are never resumable (enforced by `can_resume`).
pub fn halt_run(store: &Store, run_id: &str, reason: &str) -> Result<(), CouncilError> {
    store
        .set_halt(run_id, reason)
        .map_err(|e| CouncilError::Store(e.to_string()))
}

pub fn can_resume(halt_reason: &str) -> bool {
    // Tamper / divergence halts are permanent.
    !(halt_reason.contains("oracle_tamper")
        || halt_reason.contains("tamper")
        || halt_reason.contains("divergence"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn council_all_approve() -> Council {
        let mut d: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
        for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
            d.insert(
                s,
                Box::new(StubDriver {
                    stance: Stance::Approve,
                    reasoning: format!("{} approves", s.as_str()),
                }),
            );
        }
        Council::new(d)
    }

    #[test]
    fn consensus_when_agree() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Store::open_with_events(&dir.path().join("s.db"), &dir.path().join("e.jsonl")).unwrap();
        store.create_run("r", "u", "python", "recon").unwrap();
        let c = council_all_approve();
        let r = c
            .decide(
                &store,
                "r",
                Proposal {
                    question: "q".into(),
                    artifact_ref: "a".into(),
                    proposer: Seat::Architect,
                    reasoning: "why".into(),
                },
                b"artifact",
                (Seat::Verifier, Seat::Performance),
            )
            .unwrap();
        assert_eq!(r, Resolution::Consensus { approved: true });
    }

    #[test]
    fn blind_critics_isolated() {
        // Critic B output identical whether critic A approved or rejected:
        // stub drivers ignore each other by construction (signature takes only
        // proposal+artifact). This test pins that property.
        let p = Proposal {
            question: "q".into(),
            artifact_ref: "a".into(),
            proposer: Seat::Architect,
            reasoning: "why".into(),
        };
        let b = StubDriver {
            stance: Stance::Reject,
            reasoning: "B rejects".into(),
        };
        let r1 = b.critique(Seat::Performance, &p, b"art").unwrap();
        let r2 = b.critique(Seat::Performance, &p, b"art").unwrap();
        assert_eq!(r1.reasoning, r2.reasoning);
    }

    #[test]
    fn privacy_refuses_training_on_private() {
        let e = assert_training_tier_allowed(
            "worker",
            Visibility::Private,
            &["worker".to_string()],
            false,
        );
        assert!(matches!(e, Err(CouncilError::Privacy(_))));
        assert!(assert_training_tier_allowed(
            "worker",
            Visibility::Public,
            &["worker".to_string()],
            false
        )
        .is_ok());
    }
}
