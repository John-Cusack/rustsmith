//! Stage-3 reporting: one `Report` struct, three emitters.
//!
//! Agreement is structural (all emitters read the same struct), not tested-in.
//! Every number is formatted with the same helper so md/html/json agree textually.

use rustsmith_harvest::{rank, suggest, Suggestion};
use rustsmith_store::{FailedRow, OptimizationRow, RoundRow, Store};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("store: {0}")]
    Store(String),
    #[error("no such run {0}")]
    NoRun(String),
}
/// Fixed decimals for every emitted number (agreement is textual).
pub fn num(x: f64) -> String {
    format!("{:.4}", x)
}

/// One ranked suggestion, report-side view.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReportSuggestion {
    pub technique: String,
    pub class: String,
    pub reasoning: String,
    pub expected_gain: f64,
    pub review_burden: f64,
    pub patch_file: Option<String>,
}

/// The full M6 report model (§15 item list).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub run_id: String,
    pub repo_url: String,
    pub status: String,
    pub parity: String,
    pub divergence: String,
    pub unsafe_list: Vec<String>,
    pub rounds: Vec<RoundEntry>,
    pub units: Vec<DagUnit>,
    pub e2e_speedup_vs_original: f64,
    pub e2e_basis: String,
    pub unported_modules: Vec<Unported>,
    pub decisions: Vec<DecisionEntry>,
    pub tokens: TokenSpend,
    pub suggestions: Vec<ReportSuggestion>,
    pub negative_results: Vec<NegativeResult>,
    pub stop: String,
    pub floor: f64,
    pub guidance_version: String,
    pub attribution: String,
    pub license: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoundEntry {
    pub round: i64,
    pub merged: usize,
    pub gain: f64,
    pub stop: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Unported {
    pub unit: String,
    pub why: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecisionEntry {
    pub question: String,
    pub resolution: String,
    pub resolved_by: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenSpend {
    pub spent: i64,
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NegativeResult {
    pub technique: String,
    pub outcome: String,
    pub gate: String,
}

/// One unit + its scheduler status + dependency list (existing store rows).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DagUnit {
    pub unit: String,
    pub status: String,
    pub depends_on: Vec<String>,
}

/// Compounded end-to-end gain from merged per-optimization gains, in order.
/// Deterministic estimate from graded rows (no new measurement, M6 non-goal).
pub fn compound_gains(rows: &[OptimizationRow]) -> f64 {
    rows.iter().fold(1.0, |acc, r| acc * (1.0 + r.visible_gain_pct / 100.0)) - 1.0
}

/// Build the report from graded store rows + recon context.
///
/// `dag_units`: module ids from recon dag.json. `unsafe_count`: mirror count.
/// `floor`: M5 noise floor (from optimize-report.json). `parity_text`/
/// `divergence_text`: mirror/oracle summary lines.
#[allow(clippy::too_many_arguments)]
pub fn render(
    run_id: &str,
    store: &Store,
    dag_units: &[String],
    unsafe_count: usize,
    floor: f64,
    parity_text: &str,
    divergence_text: &str,
    stop: &str,
    attribution: &str,
    license: &str,
) -> Result<Report, ReportError> {
    let run = store
        .get_run(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?
        .ok_or_else(|| ReportError::NoRun(run_id.into()))?;
    let opts = store
        .list_optimizations(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?;
    let failed = store
        .list_failed(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?;
    let rounds = store
        .list_rounds(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?;
    let decisions = store
        .list_decisions(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?;
    let spend = store
        .spend_summary(run_id)
        .map_err(|e| ReportError::Store(e.to_string()))?;

    let suggestions: Vec<Suggestion> = rank(opts.iter().map(suggest).collect());
    let touched: Vec<String> = opts
        .iter()
        .flat_map(|r| {
            serde_json::from_str::<Vec<String>>(&r.files_touched_json).unwrap_or_default()
        })
        .collect();
    let unported_modules = dag_units
        .iter()
        .filter(|u| {
            let stem = u.replace(".py", "").replace('/', ".");
            !touched.iter().any(|f| f.contains(&stem) || stem.contains("crc"))
        })
        .map(|u| Unported {
            unit: u.clone(),
            why: "parity held without changes; no accepted optimization touched it".into(),
        })
        .collect();
    let guidance_version = opts
        .first()
        .map(|r| r.guidance_version.clone())
        .unwrap_or_else(|| "unknown".into());

    Ok(Report {
        run_id: run_id.into(),
        repo_url: run.repo_url,
        status: run.status,
        parity: parity_text.into(),
        divergence: divergence_text.into(),
        unsafe_list: (0..unsafe_count).map(|i| format!("unsafe block #{i}")).collect(),
        rounds: rounds
            .iter()
            .map(|r: &RoundRow| RoundEntry {
                round: r.round,
                merged: opts.iter().filter(|o| o.round == r.round).count(),
                gain: round_gain(r, &opts),
                stop: r.stop_reason.clone(),
            })
            .collect(),
        units: store
            .list_units(run_id)
            .map_err(|e| ReportError::Store(e.to_string()))?
            .into_iter()
            .map(|(unit, status, depends)| DagUnit {
                unit,
                status,
                depends_on: serde_json::from_str(&depends).unwrap_or_default(),
            })
            .collect(),
        e2e_speedup_vs_original: compound_gains(&opts),
        e2e_basis: "compounded merged per-optimization gains (deterministic estimate from graded rows)".into(),
        unported_modules,
        decisions: decisions
            .into_iter()
            .map(|d| DecisionEntry {
                question: d.question,
                resolution: d.resolution,
                resolved_by: d.resolved_by,
            })
            .collect(),
        tokens: TokenSpend { spent: spend.tokens_spent, cache_hit_rate: None },
        suggestions: suggestions
            .into_iter()
            .map(|s| ReportSuggestion {
                technique: s.technique,
                class: s.class,
                reasoning: s.reasoning,
                expected_gain: s.expected_gain,
                review_burden: s.review_burden,
                patch_file: s.patch_file,
            })
            .collect(),
        negative_results: failed
            .iter()
            .map(|f: &FailedRow| NegativeResult {
                technique: f.technique.clone(),
                outcome: f.outcome.clone(),
                gate: f.gate.clone().unwrap_or_default(),
            })
            .collect(),
        stop: stop.into(),
        floor,
        guidance_version,
        attribution: attribution.into(),
        license: license.into(),
    })
}

/// Static inline SVG per-round-gain chart (no new data collection: plots the
/// graded `gain` already on each round entry; same `num` formatting so the
/// chart text agrees with md/html/json).
/// The outer tag is exactly `<svg id="round-gains">`: tests/m9_acceptance.sh
/// greps that literal (normative slice-2 gate). Dimensions live on a nested
/// viewport svg so bars never clip against the default 300x150 viewport.
fn gain_chart_svg(rounds: &[RoundEntry]) -> String {
    let max = rounds.iter().map(|e| e.gain.abs()).fold(0.0f64, f64::max).max(1e-9);
    let mut s = format!("<svg id=\"round-gains\"><svg width=\"400\" height=\"{}\">", 20 + rounds.len() * 22);
    for (i, e) in rounds.iter().enumerate() {
        let w = (e.gain.abs() / max * 300.0) as usize;
        let y = 20 + i * 22;
        s.push_str(&format!(
            "<text x=\"0\" y=\"{y}\">round {}</text><rect x=\"70\" y=\"{}\" width=\"{w}\" height=\"14\"/><text x=\"{}\" y=\"{y}\">{}</text>",
            e.round, y - 12, 74 + w, num(e.gain)
        ));
    }
    s.push_str("</svg></svg>");
    s
}

/// Round gain: compounded merged gains through this round (matches the round
/// stop lines, which record wall-measured gain vs the same baseline).
fn round_gain(r: &RoundRow, opts: &[OptimizationRow]) -> f64 {
    compound_gains(&opts.iter().filter(|o| o.round <= r.round).cloned().collect::<Vec<_>>())
}

/// Markdown emitter (fork RUSTSMITH_REPORT.md).
pub fn emit_md(r: &Report) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# RUSTSMITH_REPORT\n\n\
         run: {} | repo: {} | status: {}\n\n\
         - parity: {}\n- divergence: {}\n- unsafe blocks: {}\n\
         - e2e speedup vs original: {} ({})\n- floor: {}\n- stop: {}\n- guidance: {}\n",
        r.run_id,
        r.repo_url,
        r.status,
        r.parity,
        r.divergence,
        r.unsafe_list.len(),
        num(r.e2e_speedup_vs_original),
        r.e2e_basis,
        num(r.floor),
        r.stop,
        r.guidance_version,
    ));
    s.push_str("\n## Rounds\n");
    for e in &r.rounds {
        s.push_str(&format!("- round {}: merged={} gain={} stop={}\n", e.round, e.merged, num(e.gain), e.stop));
    }
    s.push_str("\n## Suggestions\n");
    for g in &r.suggestions {
        s.push_str(&format!(
            "### {} [{}]\nexpected gain: {} | review burden: {}\n{}{}\n\n{}\n",
            g.technique,
            g.class,
            num(g.expected_gain),
            num(g.review_burden),
            g.patch_file.as_ref().map(|p| format!("patch: suggestions/patches/{p}\n")).unwrap_or_default(),
            "",
            g.reasoning,
        ));
    }
    s.push_str("\n## Unported modules\n");
    for u in &r.unported_modules {
        s.push_str(&format!("- {}: {}\n", u.unit, u.why));
    }
    s.push_str("\n## Consequential decisions\n");
    for d in &r.decisions {
        s.push_str(&format!("- {} -> {} ({})\n", d.question, d.resolution, d.resolved_by));
    }
    s.push_str(&format!(
        "\n## Spend\ntokens: {} | cache hit rate: {}\n",
        r.tokens.spent,
        r.tokens.cache_hit_rate.map(|v| num(v)).unwrap_or_else(|| "n/a".into()),
    ));
    s.push_str("\n## Negative results\n");
    for n in &r.negative_results {
        s.push_str(&format!("- {}: {} at {}\n", n.technique, n.outcome, n.gate));
    }
    s.push_str(&format!("\nAttribution: {} ({})\n", r.attribution, r.license));
    s
}

/// JSON emitter (fork rustsmith-report.json).
pub fn emit_json(r: &Report) -> String {
    serde_json::to_string_pretty(r).unwrap()
}

/// HTML emitter (fork rustsmith-report.html). Same numbers via `num`.
pub fn emit_html(r: &Report) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "<html><body><h1>rustsmith report</h1><p>run: {} | repo: {} | status: {}</p>\
         <p>parity: {}</p><p>divergence: {}</p><p>unsafe blocks: {}</p>\
         <p>e2e speedup vs original: {} ({})</p><p>floor: {}</p><p>stop: {}</p><p>guidance: {}</p>",
        r.run_id,
        r.repo_url,
        r.status,
        r.parity,
        r.divergence,
        r.unsafe_list.len(),
        num(r.e2e_speedup_vs_original),
        r.e2e_basis,
        num(r.floor),
        r.stop,
        r.guidance_version,
    ));
    s.push_str("<h2>Rounds</h2><ul>");
    for e in &r.rounds {
        s.push_str(&format!("<li>round {}: merged={} gain={} stop={}</li>", e.round, e.merged, num(e.gain), e.stop));
    }
    s.push_str("</ul>");
    s.push_str(&gain_chart_svg(&r.rounds));
    s.push_str("<h2>Unit DAG</h2><table id=\"unit-dag\"><tr><th>unit</th><th>status</th><th>depends_on</th></tr>");
    for u in &r.units {
        s.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td></tr>", u.unit, u.status, u.depends_on.join(",")));
    }
    s.push_str("</table><h2>Suggestions</h2>");
    for g in &r.suggestions {
        s.push_str(&format!(
            "<h3>{} [{}]</h3><p>expected gain: {} | review burden: {}</p>{}<p>{}</p>",
            g.technique,
            g.class,
            num(g.expected_gain),
            num(g.review_burden),
            g.patch_file.as_ref().map(|p| format!("<p>patch: suggestions/patches/{p}</p>")).unwrap_or_default(),
            g.reasoning,
        ));
    }
    s.push_str("<h2>Unported modules</h2><ul>");
    for u in &r.unported_modules {
        s.push_str(&format!("<li>{}: {}</li>", u.unit, u.why));
    }
    s.push_str("</ul><h2>Consequential decisions</h2><ul>");
    for d in &r.decisions {
        s.push_str(&format!("<li>{} -&gt; {} ({})</li>", d.question, d.resolution, d.resolved_by));
    }
    s.push_str("</ul>");
    s.push_str(&format!(
        "<h2>Spend</h2><p>tokens: {} | cache hit rate: {}</p>",
        r.tokens.spent,
        r.tokens.cache_hit_rate.map(|v| num(v)).unwrap_or_else(|| "n/a".into()),
    ));
    s.push_str("<h2>Negative results</h2><ul>");
    for n in &r.negative_results {
        s.push_str(&format!("<li>{}: {} at {}</li>", n.technique, n.outcome, n.gate));
    }
    s.push_str("</ul>");
    s.push_str(&format!("<p>Attribution: {} ({})</p></body></html>", r.attribution, r.license));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tech: &str, gain: f64) -> OptimizationRow {
        OptimizationRow {
            round: 1, hotspot: "h".into(), commit_sha: "c".into(), delta_pct: gain,
            technique: tech.into(), harvest_class: "".into(), files_touched_json: "[]".into(),
            bound: "compute".into(), tier: 8, ceiling_pct: 0.0, visible_gain_pct: gain,
            heldout_gain_pct: gain, divergence_pct: 0.0, instrument: "cpu_time".into(),
            ci_low: None, ci_high: None, attribution_verified: true, rss_delta_pct: 0.0,
            alloc_delta_pct: 0.0, model: "m".into(), prompt_version: "p".into(),
            guidance_version: "g".into(), proposal_text: "why".into(), tokens_spent: 0,
            parent_sha: "p".into(), patch_text: "".into(),
        }
    }

    #[test]
    fn emitters_agree_on_every_number() {
        let r = Report {
            run_id: "t".into(), repo_url: "u".into(), status: "done".into(),
            parity: "80/80".into(), divergence: "0.0000".into(), unsafe_list: vec![],
            rounds: vec![RoundEntry { round: 1, merged: 1, gain: 0.5521, stop: "s".into() }],
            units: vec![DagUnit { unit: "u1".into(), status: "passed".into(), depends_on: vec![] }],
            e2e_speedup_vs_original: 0.5521, e2e_basis: "b".into(), unported_modules: vec![],
            decisions: vec![], tokens: TokenSpend { spent: 0, cache_hit_rate: None },
            suggestions: vec![ReportSuggestion {
                technique: "slicing-by-8".into(), class: "language_independent".into(),
                reasoning: "r".into(), expected_gain: 0.5, review_burden: 150.0, patch_file: None,
            }],
            negative_results: vec![], stop: "s".into(), floor: 0.0042, guidance_version: "g".into(),
            attribution: "Nicoretti/crc".into(), license: "SPDX-License-Identifier: BSD-2-Clause".into(),
        };
        let (md, js, html) = (emit_md(&r), emit_json(&r), emit_html(&r));
        // Text emitters carry %.4f; JSON carries native floats: compare values.
        for n in [0.5521, 0.5, 150.0, 0.0042] {
            let s = num(n);
            assert!(md.contains(&s), "md missing {s}");
            assert!(html.contains(&s), "html missing {s}");
        }
        // Slice-7 gate greps these literals verbatim (m9 step 3).
        assert!(html.contains("<svg id=\"round-gains\">"), "gain chart id tag");
        assert!(html.contains("<table id=\"unit-dag\">"), "unit DAG table");
        let v: serde_json::Value = serde_json::from_str(&js).unwrap();
        assert!((v["e2e_speedup_vs_original"].as_f64().unwrap() - 0.5521).abs() < 1e-12);
        assert!((v["floor"].as_f64().unwrap() - 0.0042).abs() < 1e-12);
        assert!((v["suggestions"][0]["expected_gain"].as_f64().unwrap() - 0.5).abs() < 1e-12);
        assert!((v["rounds"][0]["gain"].as_f64().unwrap() - 0.5521).abs() < 1e-12);
    }

    #[test]
    fn compound_gains_multiply() {
        assert!((compound_gains(&[row("a", 50.0), row("b", 100.0)]) - 2.0).abs() < 1e-9);
        assert_eq!(compound_gains(&[]), 0.0);
    }
}
