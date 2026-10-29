# rs-port-idna evidence (ADR-070)

Lane 1 (rerank-4 §3): idna, 1339.5M dl/mo. Disposition: **PORT** —
https://github.com/John-Cusack/idna-rust.

Per the captain rule (no project code in rustsmith), lane code lives in
the port repo:

- benches: https://github.com/John-Cusack/idna-rust/tree/main/bench
  (`bench_tier1.py`, `bench_port.py`, `ceiling.py`)
- tier-1: https://github.com/John-Cusack/idna-rust/tree/main/tier1
  (`tier1.patch`, `upstream-patch-note.md`, `gen_tier1_patch.py`)

Kept here (measured evidence + docs):

- `WORKLOAD.md` — frozen workload (tracks, kill criteria)
- `bench_port_results.csv` — stock/tier-1/port bench, 3 min-of-7 sessions
- `equiv_results.txt` — suite + differential + fuzz parity counts
- `../../docs/adr/070-idna-tier1-port-scope.md` — tier-1, ceiling, kill
  evaluation, port boundary
