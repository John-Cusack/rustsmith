# Polyglot readiness audit — how prepared is rustsmith for languages beyond Python?

You are auditing this repo (`rustsmith`: autonomous rewrite-in-Rust orchestrator) for one question: **how much of the system is genuinely language-agnostic, and how much is Python-specific wearing a trench coat?**

Python is the first implemented adapter. The claim under test is that new languages arrive as plugins, not rewrites. Verify or refute that claim with evidence.

## Method

1. Start at `crates/rustsmith-adapters/src/lib.rs` — read the full trait surface (`BuildInfo`, `CallGraph`, `TestInventory`, `Attribution`, `Unit`, `UnitDag`, error types, registration/discovery). This is the supposed seam. Judge: is it sufficient for a second language, or does it leak Python concepts (pytest, pip, `conftest.py`, `py-spy`, module-as-file assumptions)?
2. Then trace every consumer of the adapter seam outward: `rustsmith-core`, `rustsmith-oracle` (freeze/manifest/graded runs/held-out), `rustsmith-gates`, `rustsmith-sandbox`, `rustsmith-profile`, `rustsmith-harvest`, `rustsmith-report`, `rustsmith-agent`, `rustsmith-council`, `rustsmith-cli` (`recon.rs`, `mirror.rs`, `optimize.rs`, `candidates.rs`, `porting.rs`, `heldout.rs`, `fixture.rs`), `containers/`, `config/default.toml`, `prompts/`, `guidance/optimize.md`.
3. For each crate/file, classify every language assumption you find. Hard-fail signals: hardcoded `pytest` invocations, `conftest.py`/`pyproject.toml`/`tox.ini` fixture lists, `PYTHONPATH`/`pip install` handling, `py-spy` profiling, tree-sitter-Python call graphs, PyO3/maturin harvest paths, prompt text naming Python idioms, config keys that only make sense for Python.

## Report format

A table, one row per finding, with exactly these columns:

| file:line | symbol/fn | assumption (1 sentence) | Python-specific? (yes/no) | severity (blocks-new-language / friction / none) | cheapest fix |

End with three sections:

- **Verdict**: `plugin-ready` | `trait-ok-but-consumers-leak` | `python-hardcoded` — one paragraph defending it with the 3 strongest pieces of evidence.
- **Minimal second-language spike**: the smallest adapter (e.g. JavaScript/TypeScript or Go — pick whichever the current seam is closest to supporting) plus the numbered list of non-adapter files that must change to admit it, in dependency order.
- **Trait redesign sketch** (only if verdict warrants): what the adapter trait must gain/lose so Step 2 is plugin-only. Concrete type/function signatures, not advice.

## Rules

- Every claim cites `file:line` + symbol. No claim without a pointer.
- Distinguish `SPEC.md` intent from implemented reality — quote both when they disagree.
- Read-only. No code changes. No formatters, linters, or test suites.
