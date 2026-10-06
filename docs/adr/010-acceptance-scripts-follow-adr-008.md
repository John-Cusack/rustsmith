# ADR 010: Acceptance scripts follow ADR-008's deliberate changes
- Context: m0/m2/m3 went red after ADR-008 (`ff19517`), not from bugs: an empty
  held-out suite now halts (it used to pass vacuously); recon/DAG keys are
  UnitIds (`python:src/crc/_crc.py`); manifest v2 hashes benchmarks in `files`
  (no `benchmark_files` list). Their image checks also probed the stale
  `rustsmith-grading:0.1.0` tag, and a missing image read as "nothing found".
- Decision: update the scripts, never the code back. m0 writes its host-only
  held-out suite before recon; m3 expects UnitIds and reads benchmarks from
  `files`; m0/m3 inspect the image recon actually graded in (tag now recorded
  on the `grade` event) and FAIL if it cannot run. m0 also requires the
  container pass to grade the 80-test baseline (it had graded 0: frozen
  host `PYTHONPATH` was not remapped into the container, fixed in sandbox).
- Consequences: every edit is equal-or-stricter; no assertion was dropped.
  m2 needed no edit (it chains m0).
- Alternatives: allow empty held-out at M0 time (reopens the vacuous-pass
  hole) — rejected; keep v1 keys via compat output (two key shapes) — rejected.
- Spec: SPEC §15 M0/M3 acceptance text unchanged; §7.4 held-out wins.
