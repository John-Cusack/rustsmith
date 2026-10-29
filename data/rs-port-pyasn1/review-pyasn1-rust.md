# Pattern review: rs-port-pyasn1 (no_gain) → proposed `RUST_PATTERNS.md` updates

Lane: rs-port-pyasn1-01 (pyasn1 0.6.5, BER codec). Verdict: honest
`no_gain` (ADR-060). This review follows `docs/PATTERN_REVIEW.md`:
before/after read side by side, every mechanism claim cites a
workload-labeled measured row, each finding classified
Confirm/Extend/New/Retire. No LLM calls in gates; this file
proposes, the PR disposes.

## Review checklist

**Evidence integrity**

- [x] Bundle inputs listed (commits where applicable): upstream
  pyasn1 0.6.5 sdist (`scratch/sdist/pyasn1-0.6.5.tar.gz`);
  before = installed `codec/ber/decoder.py` (`SingleItemDecoder`,
  `Decoder.__call__`), `codec/ber/encoder.py`
  (`SequenceEncoder.encodeValue`), `codec/streaming.py`
  (`readFromStream`); after = `tier1.patch` (shared tag caches +
  join assembly). No Rust dir — no port was scoped (no_gain).
- [x] Every figure cited appears with workload label + track
  (representative/overhead) — see rows below, all from
  `bench_tier1_results.txt` (min of 5, child-`ru_maxrss` both arms)
  and `validate_patch.py`.
- [x] Peak RSS present both arms (21.7–22.5 MiB flat); X1 overhead
  row present (0.10 s, all arms 0.96–1.06x — crossing floor
  documented). No cross-box comparisons.
- [x] Missing evidence named: no Rust-side rows (no port built —
  that is the verdict, not a gap); upstream suite run instead
  (1261 passed both sides).

**Mechanism, not vibes**

- [x] Per-item mechanism named (allocation/dispatch/branch/
  crossing) with before/after excerpts — see findings.
- [x] FFI crossings counted: no port → counted hypothetically:
  whole-message = 1 crossing/message (P1-conformant) vs per-TLV =
  11 crossings/record (P1 violation, rejected at scope).
- [x] PyO3 conversion shape: flat field values → Python object
  assembly (the floor-measured 0.0508 s); both-trees-alive RSS risk
  n/a (no tree built Rust-side).
- [x] Algorithm class: same algorithm (BER framing), only the loop
  shape changes — no sympy-class algorithm trap.

**Guide bookkeeping**

- [x] Each finding classified below (Extend ×2, Confirm ×1, no
  New/Retire).
- [x] Losers recorded with numbers (D-arm 0.94–1.03x; C-arm
  0.98–1.06x; no_gain itself).
- [x] Confidence: no threshold crossed (Single-port findings;
  P12/P6 confidences unchanged).
- [x] Per-port index + Appendix A row drafted; Top 10 untouched
  (representative-track only — a no_gain promotes nothing).
- [x] No oracle/gate/threshold touched (upstream suite run
  read-only; patch validated in a scratch venv only).

## Measured rows (verbatim, WORKLOAD.md v1, 3000 records)

- rep A 0.2694 s → B 0.2401 (1.12x) → C 0.2758 (0.98x) →
  D 0.2851 (0.94x) → E 0.2307 s (**1.17x**); RSS 21.7–22.5 MiB.
- dec-only B/E **1.22–1.23x**; enc-only all arms 0.92–1.06x; X1 all
  arms 0.96–1.06x.
- Object-model floor 0.0508 s vs E-decode 0.1557 s →
  decode-infinite ceiling **1.83x** (`floor.py`).
- Module attribution: framing 54% → theoretical max 2.19x under
  zero-cost crossing (unattainable).
- Post-tier-1 decode attribution (caches warm): `type/*` object
  construction 33% > remaining decoder dispatch; C/D ~1.0x.
- Headroom re-probe frozen workload: **0.84 rewrite**, pure 99%,
  pipeline, same top frames (`data/headroom/pyasn1.json`).
- Upstream suite on patched venv: 1261 passed + 110 subtests
  (identical to stock); equiv corpus 26/26 identical incl. 3
  error-path rows.
- Public-shape validation: rep 0.2622 → 0.2308 s (**1.14x**).

## Findings

### F1 — Extend P6: memoize the lookup, not the pure-function micro-step

Before (per message): cold `_tagCache`/`_tagSetCache` → re-create
`Tag`/`TagSet` per message (`decoder.py:1591-1597,1657-1710`).
After (`tier1.patch`): class-level shared caches → 1.12–1.23x.
But memoizing the *pure header functions* (`encodeTag` /
`encodeLength`, arm D) measures 0.94–1.03x — the dict lookup costs
what the tuple-building it replaces costs — and `bytes.join` for
5-component records (arm C) measures 0.98–1.06x.

Proposed P6 "does NOT help" amendment: *"Memoizing a pure
micro-function whose body is already one tuple-build + one dict
insert (pyasn1 `encodeTag`/`encodeLength`: 0.94–1.03x) — the cache
lookup costs what the recompute costs. Memoize the derivation
whose result is an object graph or a cross-call lookup (tag maps,
patterns, bundles), not the step cheaper than a hash."*

### F2 — Extend P12: engine-under-object-model ceiling receipt

Before: BER framing + tag dispatch + value assembly interleaved
with per-value Python object construction (generators per TLV,
`clone()` per component). After (hypothetical P12 kernel):
framing in Rust, `type/` assembly in Python. Measured floor says
the Python assembly alone (0.0508 s) leaves a 1.83x ceiling even
at infinite kernel speed; realistic 1.4–1.6x after FFI + value
handoff — below any rewrite bar.

Proposed P12 "does NOT help" amendment: *"When measured
object-model construction alone leaves the infinite-kernel
ceiling below the bar (pyasn1: 0.0508 s floor vs 0.1557 s decode
→ 1.83x ceiling, realistic ~1.5x; `type/*` 33% exceeds remaining
dispatch with caches warm) — walk away; neither whole-message
crossing (P1-conformant) nor per-field crossing (P1 violation)
clears it."* Plus anti-pattern ledger row: *"Framing kernel under
a heavy object model — pyasn1 1.83x infinite ceiling / 1.17x
tier-1 (ADR-060)"* → *"Measure the object-model floor first;
kill at <2x."*

### F3 — Confirm P1 (by kill-decision, no new row needed)

The only P1-conformant seam (whole-message crossing, 1/message)
fails the bar on the floor math; the per-TLV shape (11/message)
was rejected at scope without building. P1's "~1 µs per-call →
batch or don't port" rule is the geometry of the kill. No guide
change proposed — cite as confirming evidence in the lane ADR
(done in ADR-060).

## Per-port index + Appendix A draft

`| pyasn1 | P6 (shared tag caches 1.14–1.23x), P12-neg (1.83x ceiling) | tier-1 1.17x rep; rewrite declined | header-memo 0.94–1.03x; join 0.98–1.06x; no_gain | ADR-060 |`

Top-10 movement: none (no representative port row produced).
