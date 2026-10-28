# ADR-057: T020 solver-entry pilot — gate truth fixes, callback-ABI fixtures, solve_cmplx

Date: 2026-10-28. Scope: report §8 (the one recommended next Elmer slice) as
specified: a rustsmith tool PR (this repo) plus an elmer-rust port PR
(branch `elmer/t020-solver-entry`, naming this tool PR).

Context: the rs-elmer-unblock scout re-ran the campaign gate over the whole
1157-unit inventory (report `/home/john/firstmate/data/rs-elmer-unblock/report.md`).
Adopted findings: derived-type dummies touch 443/473 refused Fortran files
(the only blocker class that matters); the procedure-not-found class is
mostly phantom (CONTAINS internals invented as globals, proven by `nm`);
COMMON is a 1-unit phantom class; `solve_cmplx.F90` refuses because
`parse_legacy_decl` mistakes `COMPLEX(KIND=dp)` for an initializer, not
because of `USE Types`. The port boundary moves to whole solver `.so` →
Rust cdylib with the same dynamic symbols; derived objects cross as opaque
handles; component access through generated Fortran `BIND(C)` accessors.

## 1. Tool changes (this repo)

### 1.1 Phantom-export truth fix (recon + gate, `nm`-proven)

`parse_fortran_file` (recon) no longer mints exports for procedures that
emit no global symbol — `CONTAINS` internals of non-module scopes and
`INTERFACE` bodies — mirroring the gate scanner, which already skipped
them. Both scanners now track `CONTAINS` regions with host-end matching
(a later external subprogram after `END SUBROUTINE host` is checked on its
own merits instead of being swallowed), nested-`CONTAINS` handling
(recon skips, gate records fail-closed), submodule scopes as module-like
(their procedures are real globals under the parent's symbol), and
`ENDSUBROUTINE`/`ENDFUNCTION` twins. The gate verdict consults the parse's
proven-non-global list (`GateFile::nonglobal`), so stale frozen-dag
linkages for internals/interface bodies are skipped instead of refusing;
anything genuinely missing still refuses (fail closed).

Proof: `tests/t020_phantom_nm_proof.sh` — clean-room fixtures in the Elmer
solver idiom compiled with pinned gfortran 13.3.0; a mechanical
bidirectional check between `nm` globals and `rustsmith recon` exports
(11 exports ↔ 7 globals; 4 non-globals absent from both), including the
nested-`CONTAINS`, sibling-after-nested, multi-external, separate-module,
and string-literal cases. Residual: 1 instance (`Random.F90`'s
`__random_MOD_should`, minted from a `FATAL('... Function should ...')`
message literal — the gate's raw-tail reparse never recorded it, so it
stays refused fail-closed; documented, not fixed).

Asymmetry, recorded honestly: the recon keyword scan masks string
literals (fresh exports are literal-clean); the gate scan stays unmasked
so stale frozen linkages minted from literal text still resolve to the
same hallucinated procs (fail-closed merits verdicts). The two agree on
every compilable input; the 40-instance regression this asymmetry avoids
is recorded in the PR.

Regrade deltas (frozen campaign inventory, `elmer-regrade`):
1157 units: 684 → 685 admit (sole flip: `solve_cmplx` refuse→admit, §1.2);
`procedure-not-found-in-gate-parse` instances 1217 → 1 (the residual);
241 units change reasons (phantoms re-categorized: skipped, or converted
to merits verdicts on real globals the stuck-`CONTAINS` flag used to
hide); **zero admit→refuse flips**. Femcore claim set (353): 32 → 33
admits, same single flip, zero admit→refuse.

### 1.2 `KIND=` legacy-parse fix (corrects ADR-056)

`parse_legacy_decl` only treats a top-level `=` in the entity list
(paren-depth 0, after the type spec) as initialization. A `=` inside the
type spec (`COMPLEX(KIND=dp)`) or nested in parens is a kind selector.
ADR-056 §2 / T016 SESSION.md recorded the wrong mechanism (unresolved
`USE Types` + a proposed "USE-associated kind" shape that would not have
flipped the unit); the mechanism is the `contains('=')` guard
(lib.rs:3158), proven by the `COMPLEX*16` variant admitting with zero
`USE` handling. Regression tests: `COMPLEX(KIND=dp) A(n*n), x(n)` parses;
legacy `integer n = 3` and `complex(kind=dp) :: x = (1,2)` still refuse.

### 1.3 Solver-entry gate shape (one shape)

`GateMode::PluginEntry` (`fortran_plugin_entry_verdict`): the file is
always parsed (no vacuous early-`Ok`), so `COMMON`/`BLOCK DATA` refuses
even for zero-export units — closing the §4.6 blind spot within this
shape (the legacy substitution path is unchanged, pinned by test).
F77-global entries admit derived-type dummies as opaque addresses under
the accessor contract; every other dummy keeps the proven shape-A
profile; module procedures keep the existing profiles; missing/ambiguous
linkages still refuse. `nm` symbol equality, tripwire symbols, and the
accessor contract itself are port-side proof obligations, not pure-gate
claims. Real-file probes: `Poisson.F90` admits under the entry shape
(and still refuses under substitution); `solve_cmplx.F90` admits under
both.

### 1.4 Callback-ABI fixtures (ADR-027 discipline, negative controls)

`tests/fixtures/solver_callback` + `tests/t020_callback_abi_proof.sh`
(Rust caller links the gfortran callee object, pinned 13.3.0):
caller-constructed hidden character lengths (`GetReal(Params,'Source',
Found)` shape — wrong length truncates observably, exit 0);
derived-type function `RESULT` through a hidden first argument (NULL
slot segfaults); shallow-copy `RESULT` aliasing (`FLAG=ALIAS` both arms,
descriptor header asserted); allocatable-component hand-off through a
zeroed gfortran descriptor (double-fill refused by libgfortran, exit 2).
Positive arm byte-identical to the Fortran reference driver.
Unmeasured caller shapes (e.g. POINTER-dummy construction,
OPTIONAL-presence) stay refused — the gate learns nothing it cannot use.

## 2. Port (elmer-rust, branch `elmer/t020-solver-entry`)

- `solve_cmplx.F90` → crate `solvecmplx` (clones the merged `solvereal`
  template: ZGETRF/ZGETRS, `N<=0` early return, list-directed `info`
  diagnostics): 5-case corpus byte-identical (stdout+stderr+exit),
  strong-symbol gate exactly `solvelapack_cmplx_` as T, whole-file
  portable. Proof: rustsmith `tests/femcore_cmplx_proof.sh` (T020-CMPLX
  PROOF GREEN). The reference stages a clean-room `Types` stub with the
  identical `dp` definition (the real `Types.F90` unconditionally `USE`s
  Lua; both arms share the stub, so the differential is self-consistent).
- `Poisson.F90` → Rust cdylib: status recorded in the port PR (kill
  criteria evaluated on the fixture evidence; §3).

## 3. Kill-criteria evaluation (§8)

1. *Per-solver bespoke ABI?* No: the fixtures show the caller-side shapes
   are uniform gfortran ABI (hidden-length trailer, hidden result slot,
   standard descriptors) — a shared shim/accessor library serves every
   solver; variation is data (which components), not mechanism.
2. *Offset reads the accessor contract cannot express?* Poisson's
   admit→graded path needs component reads (`Solver%Matrix`,
   `Solver%Mesh{...}`, `Solver%Variable%name`,
   `GaussIntegrationPoints_t{n,U,V,W,s}`, `Nodes_t` hand-off) plus ~20
   DefUtils callbacks — all expressible as generated `BIND(C)` accessors
   against the built `.mod` files; no pinned offsets required. The
   remaining unmeasured caller shapes (POINTER dummies, OPTIONAL
   presence) are per-callback measurements, not contract violations.
   Verdict: proceed (no kill); residual measurements ride the port PR.

## 4. Corrections owed to prior records (report §10, adopted)

1. ADR-056 §2 / T016 SESSION.md: `solve_cmplx` mechanism is the
   `parse_legacy_decl` `=` guard, not unresolved `USE Types`; the proposed
   "USE-associated kind parameters" shape would not flip the unit (§1.2).
2. fem-a ledger `refused-COMMON` ×10 and the "COMMON is a major fem
   blocker" prior: falsified (1 refusal + 1 vacuous-admit carrier in 505
   files). T015/T017 "COMMON follow-up design" scoping is retired, not
   scheduled; the blind spot (vacuous admit skipping the COMMON check)
   is closed only inside the entry shape (§1.3), deliberately not in the
   legacy path (zero admit→refuse flips).
3. Derived-share headlines must name the number: file-touch 94% /
   instance-share 57% / sole-motif 31% — not a bare "94%".

No oracle, gate threshold, or tolerance was touched by this slice; every
relaxation ships with fixture proof in its own PR (ADR-027 rule).
