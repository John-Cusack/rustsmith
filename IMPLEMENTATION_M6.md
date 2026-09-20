# IMPLEMENTATION M6 — Stage 3 Harvest + Reporting

**Spec contract:** `SPEC.md` §9 Stage 3 + §14 (output) + §13 (full CLI) + §15 M6.
**Goal:** every `optimizations` row becomes a classified, shippable suggestion; all three report formats agree; at least one `language_independent` patch proves itself in the original language.
**Non-goals:** no auto-opened upstream PRs (report is a document for the human; never push to a remote the human doesn't own). No new measurement — reuse M5's graded numbers.

## 1. Crate surfaces

```
rustsmith-harvest/   # NEW: classification + artifact generation
rustsmith-report/    # NEW: md + html + json from store.db (single renderer, three emitters)
cli: report --run-id, status, halt, resume (non-tamper only)
fork layout writer   # RUSTSMITH_REPORT.md + rustsmith-report.{html,json} + suggestions/{README,patches/,accelerators/}
```

```rust
// rustsmith-harvest
pub enum HarvestClass { LanguageIndependent, ModuleLocal, PortOnly }
pub fn classify(row: &OptimizationRow, guidance: &Guidance) -> (HarvestClass, String); // class + reasoning paragraph
pub fn emit_patch(row: &OptimizationRow, orig_repo: &Path) -> Result<PatchArtifact>;    // original language + benchmark proving gain THERE
pub fn emit_accelerator(row: &OptimizationRow) -> Result<AccelArtifact>;                 // PyO3 module + dispatch shim + pure-Python fallback + maturin/cibuildwheel config
pub fn rank(suggestions: &[Suggestion]) -> Vec<Suggestion>;  // expected gain / review burden
// provenance gate (in gates, run on EVERY artifact): license headers + NOTICE + attribution from M3 preserved.

// rustsmith-report — one data model, three emitters; agreement is structural, not tested-in:
pub fn render(run_id: &str, store: &Store) -> Report;  // parity, divergence, unsafe list, per-round deltas+CI, e2e speedup,
//  unported modules+why, consequential decisions, tokens+cache hit rate, ranked suggestions, negative results, stop rule+numbers
pub fn emit_md(r: &Report) -> String; pub fn emit_json(r: &Report) -> String; pub fn emit_html(r: &Report) -> String;
// HTML adds: before/after flame graphs, per-round gain charts, unit DAG with status.
```

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | `classify` (heuristic v1 from tier/bound/technique + council review hook: Tier1-2 algorithmic → `language_independent` candidate; clean-interface hot path → `module_local`; representation-bound → `port_only`) | every M5 crc row classifies with a reasoning paragraph; council can override with logged reason |
| 2 | `emit_patch` + original-language benchmark (apply patch to clean crc checkout, run its workload, gain reproduces beyond floor) | slice-by-8/16 patch applies with `git apply --check` and measures faster in Python |
| 3 | `emit_accelerator` (PyO3 shim for one `module_local` row if any; fallback-intact test: `RUSTSMITH_DISABLE_ACCEL=1` runs pure Python green) | maturin build + fallback tests pass (crc may yield zero module_local rows — then this slice ships the empty-case test) |
| 4 | `render` + 3 emitters + agreement assert (single `Report` struct; test diffs md/json/html numbers field-by-field) | all three agree on every number by construction |
| 5 | Full CLI (`status` live units/gates/spend; `halt`; `resume` refuses tamper halts; `report --run-id` regenerates) | each subcommand exercised in test |
| 6 | `tests/m6_acceptance.sh` | green (section 3) |

## 3. M6 acceptance (quoted from SPEC §15)

> "produce at least one `language_independent` patch that applies cleanly to the original repo and measurably improves it in the original language; confirm every artifact carries preserved attribution."

```sh
tests/m6_acceptance.sh <crc-pin>
# 1. suggestions/ contains >=1 classified item with reasoning paragraph (expect: slice-by-N CRC)
# 2. patches/*.patch: `git apply --check` clean on pristine crc + benchmark shows gain beyond floor IN PYTHON
# 3. every artifact (patch, accelerator if any, reports) contains BSD-2-Clause header + attribution (provenance gate green)
# 4. md/html/json agree on all numbers (structural assert, not eyeball)
# 5. README ranks by gain/burden with one paragraph each; report carries: parity, divergence, unsafe list,
#    per-round history, e2e speedup vs original, unported modules+why, consequential decisions, tokens+cache rate,
#    negative results, stop rule; halted-Stage-2 path still emits harvest over merged rows (test with a forced halt fixture)
```

## 4. Traps

* Patch that only applies to the mirrored tree is not `language_independent` — the `git apply --check` + original-language benchmark on a *pristine* checkout is the definition, not a nice-to-have.
* Attribution stripping is a legal problem (SPEC §17 calls out a documented case): provenance runs on every artifact, including negative-results text that quotes original code.
* Report disagreement (md says 12%, json says 14%) destroys trust in all numbers — single-struct-three-emitters makes disagreement a compile-time shape, not a test to remember.

## 5. Exit criteria (end of build)

`m0..m6_acceptance.sh` all green in order on pinned fixtures + `git status` shows only intended files. System is then ready for the SPEC §16 full unattended end-to-end (`rustsmith run <crc-url>`, no human input, items 1–12).
