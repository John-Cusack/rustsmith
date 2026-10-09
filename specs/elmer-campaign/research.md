# Research: Elmer full-port campaign

**Date**: 2026-10-08 | **Sources**: S3 pilot, S5 build, S6 scoped grading, ADR-027/028.

## Decided levers (landed, measured)

1. **Scoped per-unit grading** (ADR-028): affected-test selection +
   startup-smoke gate, honest fallback to full. Measured ~270 s/unit
   on `matc`-class (~1.8× end-to-end), NOT the hoped ~100× — a 5-test
   affected set is unsound because all 482 quick tests drive one
   `ElmerSolver_mpi` binary linking `libmatc.so`; only
   reference-based selection shrinks soundly, and MATC usage is
   pervasive. Proven equivalent 22/22 by fault injection.
2. **Vendored exclusion**: `mathlibs/` + `umfpack/` are link targets,
   not port scope (frozen diagnostics before any port).
3. **Scheduler scope-skip + `RUSTSMITH_SCOPE`** (S1): unportable /
   out-of-slice units get `skipped` + event, never a halt; `skipped`
   deps satisfy ready-check.
4. **Stdin-capable probes** (S2): `{"program","args","stdin"}` with
   `TestCommand.stdin: Option<String>`; absent = inherit (argv-only
   behavior byte-identical). `matc` records `"1+2\nexit\n"` + 3
   scanner-sensitive probes (the tables bug forced this).
5. **Shared pristine build per run** + recon-build baselines (482):
   one orig build reused differentially; configure failure degrades,
   never halts.

## Rejected alternatives (with reason)

- **Per-procedure ports of non-`BIND(C)` Fortran**: measured silently
  wrong (raw `double*` into assumed-shape returned s=0, exit 0 — wrong
  science, no signal). File-granularity + descriptor-constructing shim
  only (ADR-027).
- **~100× grading cut via linkage selection**: unsound (single binary
  links everything). Reference-based selection is the sound shrink.
- **Full-tree mirror in one run**: rejected until pilot slices report
  green costs (standing rule; S3 extrapolation showed 17 days
  single-lane pre-lever).
- **Invented `fhutiter` probe path**: false data — no binary exists;
  honest `no differential probes` halt kept.
- **pty/interactive probe sessions**: pipes only; EOF-overread hangs
  honestly to timeout (exit 124).
- **Speedup claims**: safety/maintainability payoff only, never.

## Open unknowns (NOT decided here)

- Model porting cost tail for solver Fortran + MPI ranks (S3 leaves
  measured only; each slice re-freezes).
- F77 `COMMON` port strategy (whole-program coupling; refused).
- Merge-path multi-unit linking: `-Wl,--allow-multiple-definition`
  vs single-workspace port layout (splice22 proved the former;
  merge path undecided).
- Batching units per suite run / `ctest -j` study / probe-primary
  grading (future sessions, each with its own honesty cost).
