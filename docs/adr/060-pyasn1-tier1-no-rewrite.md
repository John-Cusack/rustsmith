# ADR-060: pyasn1 lane — tier-1 measured (1.17x), Rust rewrite declined (no_gain)

Date: 2026-10-29. Lane: rs-port-pyasn1-01 (pyasn1 0.6.5, 410M dl/mo).

## Context

Rerank-3 §3 Lane 1 scored pyasn1 0.84 rewrite (pure 99%, pipeline;
BER decoder `__call__`/valueDecoder + encoder encode/encodeValue +
`streaming.readFromStream` framing; top5 38% across 7 modules).
Lane spec: tier-1 first (precomputed tag→decoder maps, bytes.join
octet assembly, constructed-length prefix caching on a frozen
workload), recompute the rewrite ceiling against the improved
baseline, kill at <2x over tier-1 or on tag dispatch dominated by
the Python object-model construction.

Guide consult (rustsmith `f8c81e4`): expected P1 (whole hot unit
crosses once — one message per crossing, never per-TLV), P6
(compile/resolve once — tag→decoder maps warmed once per batch),
P12 (engine in Rust, object layer stays Python — the mandated
seam). Losers to avoid: markdown-it 0.20x (no per-token crossings),
pygments 1.00x (don't port where the C engine dominates), arrow
1.99x-tier-1/1.78x-ceiling (measure tier-1 first, recompute the
ceiling against it).

## Measurements (Ryzen 9 5950X, Python 3.12.3, pyasn1 0.6.5, min of 5)

Frozen workload: WORKLOAD.md v1 (3000 constructed records;
representative = encode + decode + checksum per record).
Headroom re-probe on the frozen workload reproduces the rerank:
**0.84 rewrite, pure 99%, pipeline**, same top frames
(`data/headroom/pyasn1.json`).

Tier-1 (pure Python, `data/rs-port-pyasn1/bench_tier1.py`, wall +
child-`ru_maxrss` both arms, checksums agree within track):

- rep: A 0.2694 s → B 0.2401 (1.12x) → C 0.2758 (0.98x) → D 0.2851
  (0.94x) → E 0.2307 s (**1.17x**).
- dec-only: B/E **1.22–1.23x**; enc-only and x1: all arms 0.92–1.06x.
- Peak RSS flat (21.7–22.5 MiB all cells — no alloc shape change).

Only the decoder-reuse arm (B: warm tag/tagSet caches across
messages) pays. Bytes.join (C) and header-cache memo (D) are
measured no-ops: the tuple-building they replace costs what the
dict/list machinery costs — recorded losers.

## Decision: no_gain, no Rust port

Kill math against the tier-1 baseline (E, 0.2307 s rep):

1. Object-model floor: building the same records from native values
   with no codec work = 0.0508 s (`floor.py`) vs E-decode 0.1557 s.
   Infinite-decode-speed ceiling = 0.2307/(0.2307−0.1049) =
   **1.83x < 2x** (realistic ≈1.4–1.6x after FFI + value handoff).
   Module attribution bounds the fantasy: framing is 54% of tottime
   → 2.19x max under zero-cost-crossing assumptions (unattainable).
2. Tag dispatch is dominated by object-model construction: with all
   tag caches warm (arm B), `type/base` + `type/*` object
   construction (33% of decode tottime) exceeds remaining decoder
   dispatch, and freeing tag/header assembly (C/D) measures ~1.0x.
   Removing the remainder requires per-field crossings (P1
   violation) or reimplementing the object model (seam violation).

`no_gain` recorded in lane store; `John-Cusack/pyasn1-rust`
created per repo-early rule and seeded with license/notice/outcome
only. Upstream patch note ready
(`data/rs-port-pyasn1/upstream-patch-note.md`, no PR opened).

## Consequences

- Offer the cache-sharing patch upstream (note ready); do not
  renominate pyasn1 for a rewrite lane without a workload showing
  >2x over the tier-1 baseline (e.g. schema-less bulk decode where
  objects are not constructed — outside the representative path).
- asn1crypto (same BER-codec family, kept as backup) inherits the
  verdict's mechanism until shown otherwise: same object-model
  seam, same per-TLV dispatch shape.
- Pattern-ledger input: tag-cache warming is a 1.2x-class win with
  a hard object-model floor (~1.8x ceiling even at infinite kernel
  speed); header-function memoization is a measured no-op (dict
  lookup ≈ tuple build). Propose P6 loser-line + P12 ceiling note
  in the post-lane pattern review (`review-pyasn1-rust.md`).
