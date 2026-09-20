# IMPLEMENTATION M3 — Stage 0 Recon + Python Adapter

**Spec contract:** `SPEC.md` §9 Stage 0 (9 steps) + §15 M3 + `SPEC_STAGE2.md` §§3–4 (WORKLOAD.md, benchmark freeze, held-out workloads).
**Goal:** deterministic recon first, Architect judgments last; output is `PORTING.md` (≥100 concrete rules on the fixture) + a verifiably leaf-first unit DAG + frozen workload contract. Council approves the plan.
**Non-goals:** no Rust code emitted, no optimization. Held-out *test* generator here is template-based (property/differential/fuzz scaffolds from public API); model-quality generation is out of scope.

## 1. Crate surfaces

```
rustsmith-adapters/        # NEW: trait + python/ (only impl in v1)
rustsmith-profile/         # NEW (recon part): py-spy baseline capture
cli run --stage recon      # NEW pipeline
prompts/architect.md       # NEW, versioned
config/fixture.toml        # pinned fixtures (crc primary, strsimpy DAG-only)
```

```rust
// rustsmith-adapters
pub trait Adapter {
  fn language(&self) -> &'static str;                       // "python"
  fn detect(&self, repo: &Path) -> Result<BuildInfo>;       // build system + layout
  fn call_graph(&self, repo: &Path) -> Result<CallGraph>;   // modules, imports, callers/callees
  fn test_inventory(&self, repo: &Path) -> Result<TestInventory>; // files, count, skips, config refs
  fn classify_dep(&self, dep: &str) -> DepClass;            // port | bind | keep
  fn license_terms(&self, repo: &Path) -> Result<Attribution>;    // license + headers to preserve
}
pub struct UnitDag { pub units: Vec<Unit>, pub edges: Vec<(String, String)> }  // depends_on
impl UnitDag {
  pub fn leaf_first_order(&self) -> Result<Vec<String>>;    // topological; error on cycle with path
  pub fn verify_order(&self, order: &[String]) -> bool;     // every dep appears before its dependent
}

// recon pipeline (deterministic steps 1-7, then Architect steps 8-9):
pub fn recon(repo: &Path, adapter: &dyn Adapter) -> Result<ReconOutput>;
pub struct ReconOutput {
  pub build: BuildInfo, pub graph: CallGraph, pub tests: TestInventory,
  pub deps: Vec<(String, DepClass)>, pub license: Attribution,
  pub hotspot_baseline: Profile,          // py-spy, representative workloads
  pub heldout_tests: HeldoutSuite,        // host-only, never into containers
  pub workload: WorkloadContract,         // WORKLOAD.md parsed form (§17: primary metric, distributions, budgets)
  pub porting_md: String,                 // Architect output, ≥100 concrete rules on fixture
  pub dag: UnitDag,
}
```

`WORKLOAD.md` sections (required, `require_workload_contract=true` fails Stage 0 without it): objective metric (one primary + optional secondary), input distribution (sizes, cardinalities, value dists, degenerate cases, shares), out-of-scope inputs, resource budgets (RSS/binary/compile from original measurements). Benchmark workloads frozen + hashed into `oracle/manifest.json` (editing one = `oracle_integrity` tamper, same as editing a test). Held-out *workload* suite: same distribution, disjoint inputs — distribution-shifted, adversarial shapes, coverage-complement, identity-sensitive repeats; host-only immutable.

## 2. Build order

| # | Slice | Done when |
|---|-------|-----------|
| 1 | `Adapter` trait + `python` detect/inventory (tree-sitter + import analysis; test files, config refs, fixture/data globs, CI invoker) | on crc: finds `src/crc/_crc.py`, all 3 test files, no conftest/pytest.ini/tox.ini, CI workflows listed |
| 2 | Baseline run (split invocation `pytest test/unit` + `pytest test/integration` per `tasks.py`; `collect-only` count + skip/xfail/deselect lists = 80/0/0/0) | numbers match hand-verified baseline in `config/fixture.toml` |
| 3 | Dep classify (crc: no-op, zero runtime deps — assert empty) + license read (BSD-2-Clause, header text captured for `provenance`) | `deps=[]`, attribution text stored |
| 4 | `py-spy` baseline + `WORKLOAD.md` + benchmark freeze + held-out workloads + baselines per workload (insns, wall+CI, RSS, allocs, profile) | `WORKLOAD.md` declares p50 wall primary for crc (checksum throughput); manifest hashes cover benchmark files |
| 5 | Held-out *test* suite (Calculator/Register/TableBasedRegister property + differential vs original + fuzz; host-only assert: absent from any container mount list) | runs on host, passes on original |
| 6 | Architect `PORTING.md` (type/error/naming/layout/ownership-lifetimes/traps; concrete rules with examples, not advice) + `UnitDag` + council approve | ≥100 rules on crc; DAG order verifies (crc DAG trivially small — see traps) |
| 7 | `tests/m3_acceptance.sh` | green (section 3) |

## 3. M3 acceptance (quoted from SPEC §15)

> "on a small pure-Python package, produce a `PORTING.md` of at least 100 concrete rules and a unit DAG whose leaf-first order is verifiably correct."

```sh
tests/m3_acceptance.sh
# 1. recon on crc pin -> PORTING.md rule count: grep -c '^## R[0-9]' >= 100 (numbered rules, each with
#    original-pattern + rust-pattern + example; generic advice lines don't match the pattern and don't count)
# 2. DAG check on strsimpy pin (NOT crc): leaf_first_order() returns order where verify_order()==true,
#    and every base module (shingle_based, string_distance) precedes its dependents; cycle injection test fails loudly
# 3. assert WORKLOAD.md exists with primary metric + distribution + budgets; every benchmark file hashed in manifest
# 4. assert held-out suites (tests + workloads) run on host and are absent from container image file lists
```

## 4. Traps

* crc's single-module shape makes its own DAG vacuous — that is why the DAG assertion runs on strsimpy. Do not "fix" this by inventing crc sub-units; the plan must say so explicitly.
* `PORTING.md` quality is the highest-leverage artifact in the system (SPEC §9). Rule-count gaming (100 vague bullets) is the failure mode — the `^## R` pattern + required original/rust/example triple is the structural backstop.
* `WORKLOAD.md` circularity (Architect grades to its own distribution): derive distributions from repo artifacts (fixtures, docs, existing benches, API shapes), freeze it, and let `workload_divergence` catch visible-subset fitting anyway.

## 5. Exit criteria

`m3_acceptance.sh` green + M0–M2 still green. Then M4.
