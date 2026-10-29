# Upstream-ready patch note: share BER decoder tag caches across messages (pyasn1)

Status: patch note only — no upstream PR opened (lane rule).
Target: pyasn1 0.6.5, `pyasn1/codec/ber/decoder.py` +
`pyasn1/codec/ber/encoder.py`.

## Problem

Every `decoder.decode()` call builds a fresh `StreamingDecoder` →
fresh `SingleItemDecoder` (`Decoder.__call__` classmethod), whose
per-instance `_tagCache`/`_tagSetCache` dicts die with the message.
For a batch of messages with the same universal tags, each message
re-creates the same `Tag`/`TagSet` objects. Measured on 3000-record
BER roundtrips (Ryzen 9 5950X, Python 3.12, min of 5, WORKLOAD.md v1):

| arm | representative (encode+decode 3000) | vs A |
|---|---|---|
| A stock `encoder.encode` / `decoder.decode` per record | 0.2694 s | 1.00x |
| B one `SingleItemDecoder` reused across messages | 0.2401 s | 1.12x |
| C `SequenceEncoder` collects chunks + `b''.join` | 0.2758 s | 0.98x |
| D memoized `encodeTag`/`encodeLength` | 0.2851 s | 0.94x |
| E combined (B+C+D) | 0.2307 s | 1.17x |

Decode-only: B 1.23x; encode-only/join/header-cache: 0.92–1.06x
(noise — the C/D candidates do not pay on this workload).

## Proposed change (behavior-preserving)

`data/rs-port-pyasn1/tier1.patch`:

1. `SingleItemDecoder`: hoist `_tagCache`/`_tagSetCache` to
   class-level dicts shared by all instances. The cached entries map
   a first tag octet to its universal `Tag`/`TagSet` — derivation
   uses only the octet, never instance state — so sharing is
   semantically transparent. `dict` get/set is GIL-atomic and a
   duplicate `Tag` creation on a thread race yields an equal object.
2. `SequenceEncoder.encodeValue`: collect component chunks in a list
   + `b''.join` instead of `substrate += ...` per component. Neutral
   on 5-component records; removes quadratic assembly for wide
   constructed types.

The header-cache memo (arm D) is deliberately NOT in the patch: it
measured 0.94–1.03x (the dict lookup costs what the tuple-building
it replaces costs) — a recorded loser, not a proposal.

## Costs / risks to review

- Memory: two small class-level dicts (one entry per distinct short
  tag octet seen — bounded by 256 entries, shared process-wide).
- Threads: same discipline as the existing per-instance dicts
  (GIL-atomic `dict` access; benign equal-object races).
- Custom `tagMap` decoders share the same base-tag caches; cached
  entries are universal base tags only, independent of any
  instance's `TAG_MAP`.

## Why not a Rust port (lane verdict)

After the tier-1 (E baseline 0.2307 s), the decode-side
object-model floor — building the same `Rec` objects from native
values with no codec work — measures 0.0508 s against E-decode
0.1557 s: even an infinitely fast decode framing gives rep
0.2307/(0.2307−(0.1557−0.0508)) = **1.83x < 2x** kill bar
(realistic ≈1.4–1.6x after FFI/handoff). Module attribution agrees:
framing (ber/codec) is 54% → absolute theoretical max 2.19x under
zero-cost-crossing assumptions, unattainable. Post-tier-1 profile
with warm caches: object-model construction (`type/base` +
`type/univ,namedtype,tag,tagmap` = 33%) exceeds remaining decoder
dispatch, and making tag/header assembly free (arms C/D) measures
~1.0x — the cost is per-TLV generator dispatch + per-value Python
object construction. A whole-message Rust kernel keeping the
Python object model cannot remove that without per-field crossings
(P1 violation) or reimplementing the object model (seam
violation). Honest `no_gain`; the cache-sharing patch above is the
win worth taking.

## Validated candidate (lane-tested, not submitted)

`data/rs-port-pyasn1/tier1.patch` applied to installed pyasn1 0.6.5
in a scratch venv, stock public call shape:

- Full upstream suite: **1261 passed, 110 subtests passed** (same as
  stock) — `tests/` from the 0.6.5 sdist.
- Equivalence corpus `data/rs-port-pyasn1/equiv_check.py`: **26/26
  identical** stock vs patched (ints incl. 2\*\*100, empty/long-form
  octets, bool, null, OID incl. 10-digit arc, nested/empty
  Sequence, SequenceOf, Set, explicit/implicit tags,
  indefinite-length, 3 error paths with identical exception
  type+message, BitString, Real, UTF8String, Enumerated; byte-equal
  re-encodes).
- Representative track through `encoder.encode`/`decoder.decode`:
  0.2622 → 0.2308 s (**1.14x**), decode-only 0.2316 → 0.2019 s
  (**1.15x**), checksums agree.
