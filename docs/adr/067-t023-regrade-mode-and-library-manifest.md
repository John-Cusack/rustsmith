# ADR-067: Slice-1 tooling — `elmer-regrade --mode` + library-manifest mode

Date: 2026-10-28. Scope: rs-elmer-next Slice 1 (§8) tool PR (this repo).
Port PR (elmer-rust) names this PR; no verdict semantics change here.

## 1. What changed

`elmer-regrade` gains two flags; the two gate functions are untouched:

- `--mode substitution|entry` (default `substitution`) selects
  `fortran_substitution_verdict` vs `fortran_plugin_entry_verdict` per unit.
  This kills the temporary `ELMER_REGRADE_MODE` env-switch driver patch the
  campaign used for entry censuses (rs-elmer-next §1/§6.5).
- `--library-manifest <libs.json>` (with optional `--reference-dir <dir>`)
  appends per-library verdicts for whole-library ports. Each library entry
  names member file-units plus the expected dynamic-symbol set pinned from
  the reference build. The library verdict is the per-file PluginEntry AND
  (entry semantics always — that is the library-verdict definition),
  vetoed by an `nm -D --defined-only` mismatch when `--reference-dir`
  points at built reference `.so` files. Without `--reference-dir` the
  symbol set is recorded, not compared (`dynsym_match: null`).
- New pin: `data/elmer-lanes/bundled-libraries.json` — all 7 bundled
  module libraries (37 file-units) with `nm`-pinned expected dynsyms from
  the serial reference build. Generator:
  `scratch/gen_manifest.py` (lane dir, not committed).

Default invocation (no new flags) is byte-identical to the pre-flag
driver — proven by diff, not by inspection (`tests/t023_regrade_mode_proof.sh`
asserts the no-`mode`-key shape; full-corpus diff below).

## 2. Reproduction (measured, this slice)

- Substitution over the frozen 1,157 (`units-all-filemodel.json` + frozen
  `dag.json` + `/home/john/runs/elmer-fork` @ `c0b4bc6`): **685/472/0**,
  and the default-flag output is byte-identical to the pre-change binary.
- Entry over the same inputs: **312/845/0** with **zero** verdict/reason
  diffs against the archived `regrade-entry-1157.json` (rs-elmer-next
  scratch) — the temporary-patch method is now redundant.
- Library mode over the manifest with the T021-B serial build as
  `--reference-dir`: ConsistentSplitting **admit 4/4**,
  SaveData **admit 6/6**, all `dynsym_match: true`; the other five refuse
  on exactly the §5.1 motifs (BatteryUtils derived+undeclared-result,
  LevelSetTimestep untyped-result, Maxwell×3 module-derived,
  CalcFields non-intrinsic, PlatiseFerroModel untyped-result, Utils
  derived, DX/Vtk/Vtu derived, ElmerPost optional). No new dummy shapes.

## 3. Proof

- `tests/t023_regrade_mode_proof.sh` — **T023 PROOF GREEN** (hermetic:
  3-file fixture tree + hand-written dag; pinned gfortran 13.3.0 only for
  the dynsym `.so`). Pins: default==substitution verdicts, entry flips
  exactly the F77-derived-opaque unit, bogus `--mode` rejected, library
  AND (refusing member vetoes), dynsym mismatch veto
  (`dynsym-mismatch: missing [...], extra [...]`), unverified recording
  without `--reference-dir`, orphan `--reference-dir` rejected.
- `cargo test -p rustsmith-adapters`, `cargo clippy -p rustsmith-adapters
  -- -D warnings`: green (see PR body).

No oracle, gate threshold, or tolerance touched. Every relaxation still
ships with fixture proof in its own PR (ADR-027 rule).
