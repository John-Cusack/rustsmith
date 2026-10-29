# WORKLOAD.md — rs-port-pyasn1 frozen workload (v1, 2026-10-29)

BER encode/decode roundtrip over synthetic constructed records.
Frozen for the tier-1 pure-Python bench, any headroom re-probe,
and any later two-track bench. Representative of ASN.1 codec CPU
in TLS/LDAP/SNMP stacks (interpreter-executed framing + tag
dispatch + constructed-type recursion).

## Corpus

- `NREC = 3000` records of ASN.1 `SEQUENCE { id INTEGER,
  blob OCTET STRING (64 B), flag BOOLEAN, oid OBJECT IDENTIFIER,
  count INTEGER }`, deterministic values (`id = 100000+i`,
  `blob = bytes((i+k) % 251 for k in range(64))`,
  `flag = bool(i % 2)`, `oid = 1.3.6.1.4.1.2026`,
  `count = i * 7919`).
- Same shape as the rerank-3 probe workload
  (`data/rs-rerank-3/wl/pyasn1_wl.py`, 300 recs, 30 ms/run);
  10x records so the representative track runs ~0.3 s/iter.

## Tracks

- **representative** (`rep`): per record, `encoder.encode(r)` then
  `decoder.decode(wire, asn1Spec=Rec())`, checksum
  `id + count + len(blob)`. This is the kill-criteria track.
- **encode-only** (`enc`): `encoder.encode(r)` per record, checksum
  wire lengths.
- **decode-only** (`dec`): wires pre-encoded once per worker
  process, then `decoder.decode(wire, asn1Spec=Rec())` per record,
  checksum fields.
- **X1 overhead** (`x1`): single small `SequenceOf(Integer)`
  `[1,2,3]` roundtrip (crossing-floor reference; only meaningful
  if a Rust port is scoped).

## Arms (tier-1 round)

- A `baseline`: stock `encoder.encode` / `decoder.decode` per record
  (fresh `StreamingDecoder` → fresh `SingleItemDecoder` per
  `decode` call; per-instance tag caches die per message).
- B `decoder-reuse`: one `SingleItemDecoder` instance reused for
  every decode in the batch (tag/tagSet caches survive across
  messages — the precomputed tag→decoder-map shape).
- C `encoder-join`: `SequenceEncoder.encodeValue` collects component
  chunks into a list + `b''.join` instead of `substrate += ...`
  per component (bytes.join octet assembly).
- D `header-cache`: `encodeTag`/`encodeLength` outputs memoized in
  a module-level dict keyed by `(tagClass, tagFormat, tagId,
  isConstructed)` / `(length, defMode)` (constructed-length
  prefix caching; both are pure functions of their args).
- E `combined`: B + C + D.

All tier-1 arms are pure-Python, upstream-plausible changes
(function-level memoization, join instead of `+=`, instance
reuse); the bench monkeypatches nothing outside pyasn1's own
modules and ships the equivalent diff as `tier1.patch`.

## Metrics

Wall seconds (perf_counter, min of 5 runs, each run in a fresh
subprocess) + peak RSS (child `ru_maxrss`, both arms). Same box,
same venv (`scratch/venv`), labeled rows. Box: Ryzen 9 5950X,
CPython 3.12.3, pyasn1 0.6.5.
