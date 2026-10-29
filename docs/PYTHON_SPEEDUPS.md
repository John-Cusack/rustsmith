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
| email-validator 2.3.0 | 160M | 3.20x mixed-2000 rep (112.8 → 35.3 ms); 6.43x ascii-valid-5000 showcase; idn-1000 0.98x | [tier1.patch](https://github.com/John-Cusack/email-validator-rust/blob/main/tier1/tier1.patch) ([note](https://github.com/John-Cusack/email-validator-rust/blob/main/tier1/upstream-patch-note.md), ADR-065) | [JoshData/python-email-validator#182](https://github.com/JoshData/python-email-validator/pull/182) | offered; cut to change 1 on 2026-10-29 after the review panel (+8 lines, 1.67x mixed-2000, 2.23x ascii-5000, import flat; [UPSTREAM_PRS.md](UPSTREAM_PRS.md#retrospective-the-three-open-prs-through-the-panel)) | No (no_gain, ADR-065: own-code ceiling 1.66x < 2x) |
| arrow 1.4.0 | 60.2M | 1.99x end-to-end rep pipeline (74.4 → 37.4 ms); fmt-parse D 1.95x / C 6.03x | [tier1-memo.patch](https://github.com/John-Cusack/arrow-rust/blob/main/tier1/tier1-memo.patch) ([note](https://github.com/John-Cusack/arrow-rust/blob/main/tier1/upstream-patch-note.md), ADR-059) | [arrow-py/arrow#1398](https://github.com/arrow-py/arrow/pull/1398) | offered; held 2026-10-29, close recommended: the shared cache can return a stale pattern, and ics 0.7.3 gains less than 5% ([UPSTREAM_PRS.md](UPSTREAM_PRS.md#arrow1398-close-recommended)) | No (no_gain, ADR-059: infinite-speed ceiling 1.78x < 2x) |
| fastjsonschema @738da70 | 108M | 1.63x rep (0.3571 s stock → combined E); 1.76x valid; 1.98x x1 | [tier1.patch](https://github.com/John-Cusack/fastjsonschema-rust/blob/main/tier1/tier1.patch) ([note](https://github.com/John-Cusack/fastjsonschema-rust/blob/main/tier1/upstream-patch-note.md), ADR-061) | [horejsek/python-fastjsonschema#233](https://github.com/horejsek/python-fastjsonschema/pull/233) | offered; held 2026-10-29, close recommended: "no behavior change" is false, and the repo's `performance.py` gains about 6% ([UPSTREAM_PRS.md](UPSTREAM_PRS.md#fastjsonschema233-close-recommended)) | Yes — port 2.44x rep (1.69x over tier-1), ADR-061 |
| pyasn1 0.6.5 | 410M | 1.17x rep (0.2694 → 0.2307 s); 1.22–1.23x decode-only; join/header arms ~1.0x losers | [tier1.patch](https://github.com/John-Cusack/pyasn1-rust/blob/main/tier1/tier1.patch) ([note](https://github.com/John-Cusack/pyasn1-rust/blob/main/tier1/upstream-patch-note.md), ADR-060) | not offered | not offered (low priority) | No (no_gain, ADR-060: infinite-decode ceiling 1.83x < 2x) |
| mako 1.4.3 | 181M | 1.015x (1.1406 → 1.1238 s, 40000 renders — noise) | not in-tree (`tier1-runtime.patch` in lane scratch only; ADR-064) | not offered | not offered (issue-only lead: parsetree caller-use analysis) | No (no_gain, ADR-064: realizable ≤1.17x at zero toll) |
| networkx 3.7 | 199M | no shippable win: T1 0.4–0.6x, T3 0.4–0.9x losers; T2 1.39–1.77x needs an API change | none (losers recorded in [note](https://github.com/John-Cusack/networkx-rust/blob/main/tier1/upstream-patch-note.md), ADR-066) | not offered | not offered (nothing to offer) | Yes — Dijkstra kernel port 2.09–3.60x, ADR-066 |
| idna 3.20 | 1339.5M | 2.60x mixed-8000 rep (0.2228 → 0.0856 s); 7.52x ascii-16000 showcase; idn-4000 1.12x | not in-tree (`tier1/tier1.patch` in idna-rust `9d8ce8b` + [note](https://github.com/John-Cusack/idna-rust/tree/main/tier1); ADR-070) | not offered | not offered (note ready in port repo, no PR per lane policy) | Yes — port 6.85x rep / 2.63x over tier-1 (PR144) |
| mdurl 0.1.2 | 427M | 1.03x A+D rep (2.781 → 2.712 s — noise-adjacent, not offered); arm C 0.82x loser | not in-tree (`tier1/tier1.patch` in mdurl-rust `a7ce9fd` + [note](https://github.com/John-Cusack/mdurl-rust/tree/main/tier1); ADR-069) | not offered | not offered (1.03x below any offer bar) | Yes — port 3.12x rep (2.93x over tier-1), ADR-069 |

Box for all speedup rows: Ryzen 9 5950X, CPython 3.12.3 (arrow: 3.12);
min-of-5/7 fresh-subprocess walls (idna: min-of-7 × 3 sessions,
per-cell best; mdurl: fresh-subprocess medians-of-3). RSS:
email-validator worker self-RSS 14.5–16.1 → 16.0–17.0 MB; fastjsonschema
flat 20–22 MiB; pyasn1 flat 21.7–22.5 MiB; mako 24272 → 25212 KB; arrow
RSS not quoted in the lane note; networkx tier-1 RSS not reported; idna
stock ~14.8–15.3 → tier-1 ~15.1–16.0 MB worker self-RSS (temp-copy
harness delta, ADR-070); mdurl flat ~12 MB both arms. dl/mo from the
lane ADRs (059/060/061/064/065/066/069/070).
