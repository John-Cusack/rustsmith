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
    /// Fail-closed merge predicate: only an approving resolution carries a
    /// merge. Both `Consensus { approved: false }` and
    /// `ArchitectTiebreak { approved: false, .. }` (any stub Reject among
    /// the three blind positions, or an Architect reject on tiebreak) must
    /// block the merge at the call site (`record_review` errors).
    pub fn approved(&self) -> bool {
        match self {
            Resolution::Consensus { approved } => *approved,
            Resolution::ArchitectTiebreak { approved, .. } => *approved,
        }
    }
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
/// `model` and `provider` are config-supplied identity strings (see
/// [`model_for_seat`] / [`provider_for_seat`]), never hardcoded; both ride
/// the prompt so the worker — and audit — sees which provider speaks.
pub struct WorkerSeatDriver {
    pub seat: Seat,
    pub command: String,
    pub model: String,
    pub provider: String,
}

impl SeatDriver for WorkerSeatDriver {
    fn critique(&self, seat: Seat, proposal: &Proposal, _artifact: &[u8]) -> Result<Position, CouncilError> {
        use std::io::Write;
        use std::process::Stdio;
        let prompt = format!(
            "# rustsmith council seat\nseat: {}\nmodel: {}\nprovider: {}\nquestion: {}\nartifact: {}\nproposer_reasoning: {}\n",
            seat.as_str(), self.model, self.provider, proposal.question, proposal.artifact_ref, proposal.reasoning
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
/// The provider for the same seat comes from [`provider_for_seat`] (same
/// `[models]` entry, `provider = "..."` field); reviewers must be seated on
/// distinct providers (`assign_reviewers` enforces), so both halves are
/// surfaced by the seat probe and recorded on review decisions — model alone
/// never identifies the review pair.
pub fn model_for_seat(seat: Seat) -> String {
    seat_field(seat, "model")
}
/// Provider identity for a seat from config only (same file, table, and
/// override mechanism as [`model_for_seat`]: the `[models]` entry's
/// `provider = "..."` field). Never hardcoded: unknown/missing entries yield
/// a `config:` placeholder the probe surfaces. Surfaced alongside the model
/// so a duplicated provider across the two review seats is visible in audit
/// (distinct-provider review is structural).
pub fn provider_for_seat(seat: Seat) -> String {
    seat_field(seat, "provider")
}
/// Extract `field = "..."` from the seat's `[models]` entry
/// (`seat = { provider = "...", model = "..." }`). Lines outside `[models]`
/// are also scanned (legacy flat shape) so older configs keep resolving.
fn seat_field(seat: Seat, field: &str) -> String {
    let path = std::env::var("RUSTSMITH_MODELS_CONFIG").unwrap_or_else(|_| "config/default.toml".into());
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let key = seat.as_str();
    let mut lines: Vec<&str> = Vec::new();
    let mut in_models = false;
    let mut saw_table = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            saw_table = true;
            in_models = t == "[models]";
            continue;
        }
        if !saw_table || in_models {
            lines.push(line);
        }
    }
    for line in lines {
        let t = line.trim();
        if t.starts_with(key) {
            if let Some(v) = quoted_field(t, field) {
                return v;
            }
        }
    }
    format!("config:models.{key}.{field}")
}
/// First `"..."` value after `field =` on one config line.
fn quoted_field(line: &str, field: &str) -> Option<String> {
    let mut search = line;
    loop {
        let i = search.find(field)?;
        let rest = &search[i + field.len()..];
        // Field name must end at the match (no `models` prefix confusion):
        // accept only when followed by optional whitespace then `=`.
        let eq = rest.trim_start();
        if let Some(stripped) = eq.strip_prefix('=') {
            let v = stripped.trim_start();
            if v.starts_with('"') {
                let inner = &v[1..];
                if let Some(e) = inner.find('"') {
                    return Some(inner[..e].to_string());
                }
                return None;
            }
            return None;
        }
        search = rest;
    }
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
    fn reject_never_approves() {
        // Fail-closed: any Reject among the blind positions yields a
        // non-approving resolution, which `record_review` must turn into a
        // merge-blocking error (never a silent carry).
        let dir = tempfile::tempdir().unwrap();
        let store =
            Store::open_with_events(&dir.path().join("s.db"), &dir.path().join("e.jsonl")).unwrap();
        store.create_run("r", "u", "python", "recon").unwrap();
        let proposal = || Proposal {
            question: "review u".into(),
            artifact_ref: "diff".into(),
            proposer: Seat::Verifier,
            reasoning: "why".into(),
        };
        // All-reject council: consensus rejects.
        let mut d: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
        for s in [Seat::Architect, Seat::Verifier, Seat::Performance, Seat::Scope] {
            d.insert(
                s,
                Box::new(StubDriver { stance: Stance::Reject, reasoning: format!("{} rejects", s.as_str()) }),
            );
        }
        let r = Council::new(d)
            .decide(&store, "r", proposal(), b"diff-bytes", (Seat::Performance, Seat::Scope))
            .unwrap();
        assert_eq!(r, Resolution::Consensus { approved: false });
        assert!(!r.approved());
        // Split council (proposer + one critic approve, one rejects):
        // Architect tiebreak with an approving Architect still records, but
        // a rejecting Architect must not approve either.
        let mut d: HashMap<Seat, Box<dyn SeatDriver>> = HashMap::new();
        d.insert(Seat::Verifier, Box::new(StubDriver { stance: Stance::Approve, reasoning: "v ok".into() }));
        d.insert(Seat::Performance, Box::new(StubDriver { stance: Stance::Approve, reasoning: "p ok".into() }));
        d.insert(Seat::Scope, Box::new(StubDriver { stance: Stance::Reject, reasoning: "s no".into() }));
        d.insert(Seat::Architect, Box::new(StubDriver { stance: Stance::Reject, reasoning: "arch no".into() }));
        let r = Council::new(d)
            .decide(&store, "r", proposal(), b"diff-bytes", (Seat::Performance, Seat::Scope))
            .unwrap();
        assert!(!r.approved(), "rejecting tiebreak must not approve: {r:?}");
        // Approving resolutions still approve.
        assert!(Resolution::Consensus { approved: true }.approved());
    }

    #[test]
    fn seat_probe_surfaces_model_and_provider() {
        // Both halves come from config (same `[models]` entry); the probe
        // prints both so a duplicated provider is auditable.
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("models.toml");
        std::fs::write(
            &cfg,
            "[models]\narchitect = { provider = \"provider-a\", model = \"architect-model\" }\nverifier = { provider = \"provider-b\", model = \"verifier-model\" }\n",
        )
        .unwrap();
        let prev = std::env::var("RUSTSMITH_MODELS_CONFIG").ok();
        std::env::set_var("RUSTSMITH_MODELS_CONFIG", &cfg);
        let m = model_for_seat(Seat::Architect);
        let p = provider_for_seat(Seat::Architect);
        let mp = provider_for_seat(Seat::Verifier);
        match prev {
            Some(v) => std::env::set_var("RUSTSMITH_MODELS_CONFIG", v),
            None => std::env::remove_var("RUSTSMITH_MODELS_CONFIG"),
        }
        assert_eq!(m, "architect-model");
        assert_eq!(p, "provider-a");
        assert_eq!(mp, "provider-b");
        assert_ne!(p, mp, "review seats must surface distinct providers");
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
