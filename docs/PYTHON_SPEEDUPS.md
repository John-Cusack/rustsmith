# Pure-Python speedups tracker

Tier-1-first lane results: the pure-Python fix is measured before any
Rust port is scoped. A row lands here when its lane's graded run +
retro exist; the upstream-PR columns move only when a PR opens,
merges, or is declined (see
[`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) — the upstream lane owns
those cells). No new numbers here: speedups cite the lane evidence,
never re-typed prose.

Patterns behind these wins (and losers): [`PYTHON_PATTERNS.md`](PYTHON_PATTERNS.md).
Rust-port lessons live separately in [`RUST_PATTERNS.md`](RUST_PATTERNS.md).

| Package | dl/mo | Measured speedup and workload | Patch | Upstream PR | PR status | Rust rewrite followed? |
|---|---|---|---|---|---|---|
| email-validator 2.3.0 | 160M | 3.20x mixed-2000 rep (112.8 → 35.3 ms); 6.43x ascii-valid-5000 showcase; idn-1000 0.98x | `data/rs-port-email-validator/tier1.patch` ([note](../data/rs-port-email-validator/upstream-patch-note.md), ADR-065) | being offered | being offered | No (no_gain, ADR-065: own-code ceiling 1.66x < 2x) |
| arrow 1.4.0 | 60.2M | 1.99x end-to-end rep pipeline (74.4 → 37.4 ms); fmt-parse D 1.95x / C 6.03x | `data/rs-port-arrow/tier1-memo.patch` ([note](../data/rs-port-arrow/upstream-patch-note.md), ADR-059) | being offered | being offered | No (no_gain, ADR-059: infinite-speed ceiling 1.78x < 2x) |
| fastjsonschema @738da70 | 108M | 1.63x rep (0.3571 s stock → combined E); 1.76x valid; 1.98x x1 | `data/rs-port-fastjsonschema/tier1.patch` ([note](../data/rs-port-fastjsonschema/upstream-patch-note.md), ADR-061) | being offered | being offered | Yes — port 2.44x rep (1.69x over tier-1), ADR-061 |
| pyasn1 0.6.5 | 410M | 1.17x rep (0.2694 → 0.2307 s); 1.22–1.23x decode-only; join/header arms ~1.0x losers | `data/rs-port-pyasn1/tier1.patch` ([note](../data/rs-port-pyasn1/upstream-patch-note.md), ADR-060) | not offered | not offered (low priority) | No (no_gain, ADR-060: infinite-decode ceiling 1.83x < 2x) |
| mako 1.4.3 | 181M | 1.015x (1.1406 → 1.1238 s, 40000 renders — noise) | not in-tree (`tier1-runtime.patch` in lane scratch only; ADR-064) | not offered | not offered (issue-only lead: parsetree caller-use analysis) | No (no_gain, ADR-064: realizable ≤1.17x at zero toll) |
| networkx 3.7 | 199M | no shippable win: T1 0.4–0.6x, T3 0.4–0.9x losers; T2 1.39–1.77x needs an API change | none (losers recorded in [note](../data/rs-port-networkx/upstream-patch-note.md), ADR-066) | not offered | not offered (nothing to offer) | Yes — Dijkstra kernel port 2.09–3.60x, ADR-066 |

Box for all speedup rows: Ryzen 9 5950X, CPython 3.12.3 (arrow: 3.12);
min-of-5/7 fresh-subprocess walls. RSS: email-validator worker
self-RSS 14.5–16.1 → 16.0–17.0 MB; fastjsonschema flat 20–22 MiB;
pyasn1 flat 21.7–22.5 MiB; mako 24272 → 25212 KB; arrow RSS not quoted
in the lane note; networkx tier-1 RSS not reported. dl/mo from the
lane ADRs (059/060/061/064/065/066).
