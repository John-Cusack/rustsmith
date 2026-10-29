# Python patterns — the living guide

What actually made pure-Python tier-1 fixes faster, measured before
any Rust port was scoped. Companion to
[`RUST_PATTERNS.md`](RUST_PATTERNS.md): Rust lessons go there,
pure-Python lessons go here, never both. Thesis: the cheapest
speedup is the one that never crosses a language boundary.

Seed: the six tier-1-first lanes measured 2026-10-28/29
(arrow, pyasn1, fastjsonschema, mako, email-validator, networkx).
Nothing in this file comes from anywhere else.

**How this file stays honest**

- Every pattern claim cites a measured figure with its workload label
  (size + distribution + track: representative / showcase / overhead)
  and a source (lane `WORKLOAD.md`, bench script + results file,
  `upstream-patch-note.md`, ADR). Figures from different boxes are
  never averaged; where two measurements of the same shape disagree,
  both are kept and labeled.
- Speedup = stock wall / patched wall on the same corpus, same
  machine. Peak RSS is reported exactly as the lane reported it
  (child `ru_maxrss` or worker self-RSS — the entry says which);
  where the lane note quotes walls only, this guide says so instead
  of inventing an RSS number.
- Unmeasured ideas live only in [Hypotheses](#hypotheses-unmeasured).
  Nothing there is advice yet.
- Losers are first-class: every pattern names where it does **NOT**
  help, with the losing number. A pattern with no recorded loser is
  under-tested, not universal.
- Updates come only through the review pass in
  [`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) (new evidence, new pattern,
  or retirement), never by hand-waving a number in.

**Confidence** = how many independent lanes confirm the pattern with
measured rows: **High** ≥3 lanes · **Medium** 2 lanes · **Single**
1 lane (measured, not yet replicated).

---

## Top patterns by measured impact

Ranked by best representative-track speedup attributed to the
pattern (showcase ceilings in the pattern entries, never here).
Impact ≠ applicability: read the "does NOT help" line before using.

| # | Pattern | Best representative evidence | Confidence |
|---|---|---|---|
| 1 | [PY2 Skip work on the fast-path input class](#py2-skip-work-on-the-fast-path-input-class-ascii-first) | email-validator 3.20x mixed-2000 (6.43x ascii-valid showcase) | Medium |
| 2 | [PY1 Hoist per-call rebuilds to or above the constructing call site](#py1-hoist-per-call-rebuilds-to-or-above-the-constructing-call-site) | arrow 1.99x end-to-end pipeline, 74.4 → 37.4 ms | High |
| 3 | [PY4 Don't build bookkeeping nobody reads](#py4-dont-build-bookkeeping-nobody-reads) | fastjsonschema 1.63x rep (combined arm E) | Single |
| 4 | [PY3 One C call instead of a per-character Python loop](#py3-one-c-call-instead-of-a-per-character-python-loop) | email-validator T-vec (part of 3.20x); fastjsonschema required 1.21x rep | Medium |
| 5 | [PY5 Warm caches are a 1.1–1.2x class with a hard object-model floor](#py5-warm-caches-are-a-11-12x-class-with-a-hard-object-model-floor) | pyasn1 1.17x rep, 1.22–1.23x decode-only | Single |

Deliberately not ranked: the [losers](#anti-pattern-ledger-measured-losers)
(per-call export 0.4–0.9x, header memo ~1.0x, mako 1.015x noise) —
they save lanes, not cycles.

---

## Per-lane index

Every tier-1 lane reviewed for this guide: what was tried, best
representative figure, and its honest loser. Detail in the pattern
entries.

| Lane | Patterns found | Representative (labeled) | Loser / no-change | Source |
|---|---|---|---|---|
| email-validator 2.3.0 (160M dl/mo) | PY1, PY2, PY3 | **3.20x** mixed-2000 (112.8 → 35.3 ms); 6.43x ascii-valid-5000 showcase | idn-1000 0.98x (idna-bound, unchanged) | `data/rs-port-email-validator/{WORKLOAD.md,bench_tier1_results.json}` (measured evidence) + [lane tooling: bench/ + tier1/](https://github.com/John-Cusack/email-validator-rust/tree/main/bench), ADR-065 |
| arrow 1.4.0 (60.2M dl/mo) | PY1 | **1.99x** end-to-end pipeline (74.4 → 37.4 ms); fmt-parse D-arm 1.95x, C-arm 6.03x | — (ceiling killed the rewrite, not the patch: 1.78x infinite-speed ceiling) | `data/rs-port-arrow/WORKLOAD.md` (measured evidence) + [lane tooling: bench/ + tier1/](https://github.com/John-Cusack/arrow-rust/tree/main/bench), ADR-059 |
| fastjsonschema @738da70 (108M dl/mo) | PY3, PY4 | **1.63x** rep (0.3571 s stock → combined E); valid 1.76x, x1 1.98x | enum-only arm 0.99x on valid (no win there) | `data/rs-port-fastjsonschema/{WORKLOAD.md,bench_tier1_results.json}` (measured evidence) + [lane tooling: bench/ + tier1/](https://github.com/John-Cusack/fastjsonschema-rust/tree/main/bench), ADR-061 |
| pyasn1 0.6.5 (410M dl/mo) | PY5 | **1.17x** rep (0.2694 → 0.2307 s); decode-only 1.22–1.23x | bytes-join arm 0.98x; header-cache arm 0.94x — recorded losers | `data/rs-port-pyasn1/{WORKLOAD.md,bench_tier1_results.txt}` (measured evidence) + [lane tooling: bench/ + tier1/](https://github.com/John-Cusack/pyasn1-rust/tree/main/bench), ADR-060 |
| mako 1.4.3 (181M dl/mo) | (noise) | **1.015x** (1.1406 → 1.1238 s, 40000 renders) | whole tier-1 is the loser; P3 parsetree analysis declined, ~15% bound only | ADR-064 (patch `tier1-runtime.patch` not in-tree; no `data/rs-port-mako/`) |
| networkx 3.7 (199M dl/mo) | (losers only) | no shippable win; cached-export T2 1.39–1.77x needs an API change | T1 per-call export 0.4–0.6x; T3 no-conversion export 0.4–0.9x — measured losers | `data/rs-port-networkx/{WORKLOAD.md,bench_evidence.json,bench_rows.json,tier1_results2.txt}` (measured evidence) + [lane tooling: bench/ + tier1/](https://github.com/John-Cusack/networkx-rust/tree/main/bench), ADR-066 |
| idna 3.20 (1339.5M dl/mo) | PY2 | **2.60x** mixed-8000 (0.2228 → 0.0856 s); 7.52x ascii-16000 showcase; idn-4000 1.12x | T-import-only arm 1.01x mixed / 1.07x idn (fast path carries the win); per-label `lru_cache` analyzed but declined (unmeasured) | `data/rs-port-idna/{WORKLOAD.md,bench_port_results.csv,equiv_results.txt}`, ADR-070, https://github.com/John-Cusack/rustsmith/pull/144 |
| mdurl 0.1.2 (427M dl/mo) | (losers only) | A+D combined **1.03x** (2.781 → 2.712 s) — below any offer bar, not offered | arm C 0.82x (generator fusion); arms A/B/D ~1.00x/1.00x/0.97x no-ops | `data/rs-port-mdurl/{WORKLOAD.md,bench.json,bench_tier1_results.json}`, ADR-069, https://github.com/John-Cusack/rustsmith/pull/143 |

Box for all rows: Ryzen 9 5950X, CPython 3.12.3 (arrow: 3.12),
min-of-5 (arrow, pyasn1) / min-of-7 (email-validator, fastjsonschema,
mako, idna) fresh-subprocess walls; idna per-cell best-of-21 across 3
min-of-7 sessions; mdurl fresh-subprocess medians-of-3
(`/usr/bin/time -v`) — unless the entry says otherwise.

---

## Patterns

### PY1. Hoist per-call rebuilds to or above the constructing call site

**Problem shape.** The hot path rebuilds the same artifact per call:
regex compile, parser `__init__` (locale + choice compiles), per-call
imports, per-label precompiled checks. Whatever a rebuild costs, pure
Python can usually hoist it — tier-1 takes that share first and the
rewrite ceiling is computed on what is left.

**Technique.** Move the cache to or above the site that constructs the
using object. A cache on the constructed instance dies with it; a
cache keyed by the construction inputs survives.

Before (arrow — per-instance `lru_cache` on a factory-built object
is a no-op cache, ADR-059):

```python
# arrow factory.py: fresh DateTimeParser(locale) per arrow.get call
# parser.py: lru_cache wraps the bound method per instance —
# the instance is discarded after one call, so the cache never hits
```

After (same semantics, cache survives — `tier1-memo.patch`):

```python
# parser.py: class-level (locale-class, fmt) memo, 128 entries
# factory.py: one parser per locale string
```

**Measured.** arrow representative pipeline (2000-timestamp
parse → shift → format → humanize, `WORKLOAD.md` v1):
74.4 → 37.4 ms (**1.99x**); fmt-parse 2000: baseline 44.6 ms →
shared parser 24.3 ms (1.84x) → shared + `cache_size=128` 7.4 ms
(6.03x); class-level memo through stock `arrow.get` 22.9 ms
(1.95x). Mechanism split per call: pattern rebuild ≈42%, parser
`__init__` ≈28%, match/extract/build ≈30%.
email-validator T-cache (per-call `from . import ...` defaults +
`SPECIAL_USE_DOMAIN_NAMES` into module globals; inline
`re.match(r"(?!xn)..--", ...)` per label precompiled): isolated
1.17x mixed / 1.32x ascii. pyasn1 arm B (one `SingleItemDecoder`
reused across messages, tag/tagSet caches survive): 1.12x rep,
1.22–1.23x decode-only (PY5 books the ceiling).

**Does NOT help / inverted when misplaced.** The arrow D-arm vs C-arm
pair is the receipt: same cache, wrong level = 1.95x; right level =
6.03x on the parse track. Caching below the constructing call site
is the [no-op cache anti-pattern](#anti-pattern-ledger-measured-losers).
Mako's per-callable argspec memo (pure function of the immutable
generated callable, recomputed from `co_varnames` per render) is the
same shape done right — and still only noise (1.015x with the
deque→list change), because the rebuild was never the dominant share
there.

**Peak RSS.** Arrow lane harness records worker self-RSS per arm;
the lane note quotes walls only — no RSS number is claimed here.
Email-validator: stock 14.5–16.1 MB → tier-1 16.0–17.0 MB worker
self-RSS (delta is the temp-copy import harness, not a library
regression — ADR-065). Pyasn1: flat 21.7–22.5 MiB all arms.

**Confidence: High** (arrow, email-validator T-cache, pyasn1 arm B).

---

### PY2. Skip work on the fast-path input class (ASCII-first)

**Problem shape.** A general path pays for generality the common input
never needs: IDNA roundtrip on pure-ASCII domains, NFC combining
checks on ASCII text, unsafe-char loops over ASCII that rejects
exactly controls/DEL/space.

**Technique.** Test the cheap input property once (`isascii()`,
no-Punycode-label), take the direct path; keep the original loop for
everything else, with differential cases proving the boundary.

```python
# email-validator tier1.patch, validate_email_domain_name:
# pure-ASCII domain without a Punycode label → skip idna.decode +
# idna.encode roundtrip; idna.decode is casefolding there
# (== str.lower on ASCII, verified), re-encode of lowercase ASCII
# always succeeds after the syntax checks
```

**Measured.** email-validator (`bench_tier1.py`, min-of-7,
`check_deliverability=False`): mixed-2000 representative
112.8 → 35.3 ms (**3.20x**); ascii-valid-5000 showcase 296.2 → 46.1 ms
(**6.43x**); idn-1000 showcase 104.0 → 106.1 ms (**0.98x**, idna-bound,
unchanged). Isolated T-idna effect: 2.03x mixed / 4.04x ascii
(ablation: tier-1 minus this change). Upstream suite 306/306 both
arms; 468-row differential (52 addresses × 9 option sets, incl.
exception types/messages and combining-sequence adversarial cases)
identical. idna (`bench_tier1.py`, min-of-7 × 3 sessions, per-cell
best): mixed-8000 representative 0.2228 → 0.0856 s (**2.60x**);
ascii-16000 showcase 0.4264 → 0.0567 s (**7.52x**); idn-4000 showcase
0.1398 → 0.1248 s (**1.12x**). Isolated T-import effect: 1.01x mixed
/ 1.07x idn — the ASCII fast path carries essentially the whole win.
Upstream suite 6441 passed + 56 subtests both arms; 900-row
differential (incl. exact exception types/codes/messages), 0 mismatches.

**Does NOT help.** The idn-1000 track (0.98x) is the boundary made
visible: on IDN input the lane is structurally idna-bound and no
in-package ASCII path can move it — a faithful port must either keep
UTS-46/IDNA behind the seam or reimplement the shared `idna`
dependency (ADR-065 kill math: own-code-only ceiling 1.66x < 2x;
60.3% of remaining profiled CPU outside email-validator). Same shape
inside `idna` itself: the idn-4000 track moves only 1.12x — the ASCII
class guard cannot touch UTS-46/punycode work, so the port (not more
tier-1) is where that share goes (ADR-070).

**Confidence: Medium** (email-validator, idna).

---

### PY3. One C call instead of a per-character Python loop

**Problem shape.** Per-character membership tests or per-option
chains executed in Python: `{... for c in s if not RE.match(c)}`
set comprehensions, per-option `isinstance`-and-`==` or-chains,
`set([...]) - data.keys()` on the valid path.

**Technique.** Emit the single C-speed equivalent with identical
outputs: one negated-class `findall` producing the identical bad-char
set; tuple membership for all-string enums; list-comprehension
missing-key check so set construction leaves the valid path.

```python
# email-validator: {... for c in s if not RE.match(c)}
# → one C-speed negated-class findall (identical bad sets)
# fastjsonschema generate_required: set([...]) - data.keys()
# → [k for k in [...] if k not in data] (same type/message/definition/rule)
# fastjsonschema generate_enum (all-string): or-chain
# → if x not in ('a', 'b', ...)
```

**Measured.** email-validator T-vec + T-split ride inside the 3.20x
rep win (ablation splits the win between T-idna and the rest; the
note marks (2)–(4) as uncontroversial mechanical wins). fastjsonschema
(`bench_tier1.py`, min-of-7, 3000-doc nested order batch):
required-only arm 1.21x rep / 1.31x valid / 1.41x x1; enum-only arm
1.11x rep but 0.99x valid (no win on the exception-free path —
kept as the loser row); keys-only arm booked under PY4.

**Does NOT help.** pyasn1 arm C (`substrate += ...` → list +
`b''.join`): **0.98x** on 5-component records — the tuple/list
machinery costs what the concatenation it replaces costs at that
width (recorded loser; the patch ships the join only because it
removes quadratic assembly for wide constructed types, not because
it pays here).

**Peak RSS.** fastjsonschema flat 20–22 MiB all arms; checksums and
error samples (message, name, rule, definition) agree all tracks/arms.

**Confidence: Medium** (email-validator, fastjsonschema).

---

### PY4. Don't build bookkeeping nobody reads

**Problem shape.** Generated validators build, probe, and prune a
tracking set on every object level whether or not any consumer reads
it: `{var}_keys` set built + `in`-probed + `.remove`-pruned per key
when `additionalProperties` is absent/`True`/`{}` and no
`propertyNames: false` exists at that level.

**Technique.** Skip the set and its bookkeeping when no consumer
exists at that object level; direct `if "key" in {variable}` guards
instead. No API change; exception texts/paths byte-identical by
construction.

**Measured.** fastjsonschema keys-only arm: 1.14x rep / 1.20x valid /
1.17x x1; combined arm E (required + keys + enum): **1.63x** rep /
**1.76x** valid / **1.98x** x1 (stock rep 0.3571 s, valid 0.2825 s,
x1 0.0145 s). This is the largest single-arm contributor pattern in
the fastjsonschema lane and the shape upstream takes with zero API
risk (`upstream-patch-note.md`).

**Does NOT help.** Where a consumer exists (`additionalProperties`
with a schema, `propertyNames: false`), the set is load-bearing —
the patch keeps it. Over-deleting bookkeeping breaks semantics;
under-deleting just leaves this 1.14x on the floor.

**Confidence: Single** (fastjsonschema only; not yet replicated).

---

### PY5. Warm caches are a 1.1–1.2x class with a hard object-model floor

**Problem shape.** A batch of same-shaped messages re-creates
identical tag objects per message because per-instance caches die per
call (`StreamingDecoder` → fresh `SingleItemDecoder` per
`decoder.decode()`, per-instance `_tagCache`/`_tagSetCache`).

**Technique.** Share the base-tag caches across instances
(class-level dicts keyed by first tag octet; derivation uses only the
octet, never instance state — semantically transparent, GIL-atomic
get/set, benign equal-object races).

**Measured.** pyasn1 (`bench_tier1.py`, min-of-5, 3000 constructed
records, rep = encode + decode + checksum): A stock 0.2694 s → B
decoder-reuse 0.2401 s (1.12x) → E combined 0.2307 s (**1.17x**);
decode-only B/E **1.22–1.23x**; encode-only and x1 all arms
0.92–1.06x. Full upstream suite 1261 passed + 110 subtests both arms;
26/26 equivalence corpus identical (incl. error type+message).
Peak RSS flat 21.7–22.5 MiB (no alloc shape change).

**Does NOT help / the floor.** Post-tier-1 kill math (ADR-060):
object-model floor (building the same records from native values, no
codec work) 0.0508 s vs E-decode 0.1557 s → infinite-decode-speed
ceiling **1.83x < 2x** (realistic ≈1.4–1.6x after FFI + value
handoff). With all tag caches warm, `type/*` object construction
(33% of decode tottime) exceeds remaining decoder dispatch — freeing
tag/header assembly measures ~1.0x (arms C/D). Removing the remainder
needs per-field crossings (P1 violation in RUST_PATTERNS.md) or
reimplementing the object model (seam violation): honest no_gain.

**Confidence: Single** (pyasn1 only; asn1crypto inherits the verdict
as same-family until shown otherwise — ADR-060).

---

## Anti-pattern ledger (measured losers)

| Anti-pattern | Receipt | Do instead |
|---|---|---|
| Per-instance cache on a factory-built object (no-op cache) | arrow: same cache wrong level 1.95x (D) vs right level 6.03x (C) on fmt-parse | PY1: cache at or above the constructing call site (class-level memo / locale parser cache) |
| Memoizing a header function whose lookup costs what it replaces | pyasn1 arm D 0.94x rep, 0.94–1.03x range — deliberately NOT shipped | Record and drop; warm the tag caches (PY5) instead |
| `+=` → `join` rewrite on narrow records | pyasn1 arm C 0.98x rep | Ship only for the wide-type quadratic argument, never as a wall win |
| Port-shaped work where the rebuild was never the share | mako 1.015x (argspec memo + deque→list; 1.1406 → 1.1238 s, RSS 24272 → 25212 KB) | Walk away; file the analyzed-but-declined lead (parsetree caller-use analysis, ~15% bound) as an issue, not a patch |
| Per-call adjacency export in pure Python | networkx T1 0.4–0.6x, T3 0.4–0.9x on 1–5K-node graphs | No transparent patch exists; the Rust kernel walks the dict once inside (2.09–3.60x) — a shape not expressible in pure Python |
| Export-cache without an invalidation story | networkx T2 1.39–1.77x but needs a caching-handle API on a mutable graph | API change, not a faster loop — out of scope ("never mirror the package") |
| Valid-path set construction | fastjsonschema enum-only 0.99x on valid; stock set-minus on the valid path | PY3/PY4: move construction off the valid path |
| Generator-expression fusion of an explicit C-`find` loop | mdurl arm C 0.82x (3.413 s vs 2.781 s baseline) — two nested generator frames cost more than the `for` + C-`find` loop | Keep the explicit loop; do not "clean up" what the profile says is already fast |
| Hoisting an import the machinery already caches | idna T-import-only arm 1.01x mixed / 1.07x idn (`sys.modules` lookup per call is ~nothing) | Verify the hoist removes real per-call work before offering; the ASCII fast path (PY2) carried this win |
| IDNA-bound track patched in the wrong package | email-validator idn-1000 0.98x | Leave idna to `idna`; take the ASCII path (PY2) or walk away |

---

## Hypotheses (unmeasured)

Nothing in this section has a lane measurement. Each entry states
what bench would promote or kill it. Do not cite these numbers —
there are none.

- **Y1 Mako P3 (declined): parsetree caller-use analysis to skip
  `_push_frame`/`_pop_frame` for defs with no `<%caller%>` use.**
  Bound only: ~15% wall in a caller-breaking no-op probe (machinery
  arm 1.19x is the bound, not the gain). Promotion bench: codegen
  change + full 562-passed/53-skipped suite + 40000-render
  `WORKLOAD`-equivalent rows; kill on any fidelity divergence.
- **Y2 Arrow instance-reuse vs class-memo split.** B (1.84x) and D
  (1.95x) measured; the shipped patch combines both (end-to-end
  1.91x through `arrow.get`). Unmeasured: which half upstream should
  take alone. Promotion: A/B of each half through the stock call
  shape on the frozen workload.
- **Y3 fastjsonschema codegen shapes on other schemas.** The 1.63x is
  one nested-order geometry; transfer to wide/flat or
  `patternProperties`-heavy schemas unmeasured. Promotion: same
  arms on two more schema geometries before claiming generality.

---

## Appendix A: per-lane findings

One paragraph each. Numbers are the rows in the
[per-lane index](#per-lane-index).

- **email-validator** — Four upstream-plausible changes, all
  input-class-gated (PY2) or mechanical (PY1/PY3): ASCII bypass of the
  idna roundtrip (the 2.03x/4.04x isolated win), C-speed membership
  scans, per-call hoists, ASCII NFC skip. The largest verified
  pure-Python win in the loop (3.20x rep). Kill came from the
  *remainder*, not the patch: 60% of post-tier-1 CPU outside own
  code (idna 16.7%, C re/unicodedata 43.6%), idn track flat at 0.98x.
- **arrow** — The cache-shape receipt: per-instance `lru_cache` on a
  factory-built object cannot fire (ADR-059). Class-level
  `(locale, fmt)` memo + per-locale parser is the 1.99x end-to-end
  win. Rewrite died on the ceiling (1.78x infinite-speed over the
  memoized baseline), not on effort: the rest is
  `Arrow.shift`/object-model/locale dispatch.
- **fastjsonschema** — Three codegen shapes, each small, combined
  1.63x rep with flat RSS and byte-identical errors. The lane that
  proves tier-1-first sequencing: ceiling vs tier-1 was 3.84x, so the
  rewrite proceeded — port 2.44x (1.69x over tier-1, ADR-061).
- **pyasn1** — Decoder-reuse pays (1.12–1.23x); join and header-memo
  measured ~1.0x and the memo was deliberately unshipped. The
  object-model floor (1.83x infinite ceiling) is the no_gain receipt;
  warm caches are worth offering, the rewrite is not.
- **mako** — The noise receipt: two correct micro-fixes sum to
  1.015x because the hot path is user expressions per node + C
  `list.append` writes (~50 ns, below any PyO3 toll). Realizable
  port bound 1.17x at zero toll, expected ≤1.0x with toll (ADR-064).
  Only lane with no in-tree patch: the upstream lead is an
  issue-only codegen analysis, quarantined in Hypotheses.
- **networkx** — The losers-only receipt: no transparent pure-Python
  shape beats stock Dijkstra (T1/T3 0.4–0.9x; T2 needs an API
  change). The `upstream-patch-note.md` exists so upstream need not
  re-run the experiment. Survival shape noted for the Rust side:
  concentrated tight loop over plain data, whole-graph crossing, no
  user callbacks — port 2.09–3.60x (ADR-066).
- **idna** — The second PY2 confirm: lowercase-ASCII-valid label fast
  path in `check_label`/`alabel`/`ulabel` (one module-level regex;
  match implies NFC-stable/PVALID/bidi-clean) plus a hoisted
  `uts46data` import. 2.60x mixed rep / 7.52x ascii showcase; idn
  1.12x is the same boundary email-validator hit from the other
  side. T-import-only ablation (1.01x) proves the fast path carries
  the win; per-label `lru_cache` analyzed but declined (unmeasured,
  not a receipt). Patch + note live in the port repo per the captain
  rule (no upstream PR opened, lane policy); the Rust port that
  followed is a RUST_PATTERNS entry (6.85x/2.63x, ADR-070).
- **mdurl** — The nothing-to-offer receipt: K1 ~= 0 (regexes already
  module-precompiled, the one inline pattern absorbed by the `re`
  cache), so the named thesis (hoist per-parse regex) did not
  reproduce. A/B/D arms 1.00x/1.00x/0.97x; arm-C generator fusion
  0.82x loser; combined 1.03x, not offered upstream. The port that
  followed is a RUST_PATTERNS entry (3.12x, ADR-069).

## Appendix B: sources

Lane tooling (bench/probe/equivalence scripts, patches) moved out of
rustsmith into each package's `*-rust` repo (`bench/` + `tier1/`, same
shape as mdurl-rust); measured evidence (`WORKLOAD.md`, results JSON/TXT,
reviews) stays in `data/rs-port-*`. Sources: lane data dirs, lane `bench/`
 + `tier1/` in `John-Cusack/{arrow,email-validator,fastjsonschema,pyasn1,
networkx}-rust`,
`docs/adr/059/060/061/064/065/066`,
`/home/john/firstmate/data/rs-rerank-4/report.md` §4 (upstream-offer
inventory — states, not numbers). idna/mdurl: `data/rs-port-idna/`
(WORKLOAD, bench CSV, equiv, ADR-070) and `data/rs-port-mdurl/`
(WORKLOAD, bench JSONs, tier-1 results, review, verification,
ADR-069); tier-1 patches + upstream notes in the port repos
(idna-rust `9d8ce8b` / mdurl-rust `a7ce9fd` `tier1/`, no upstream PRs
opened per lane policy).
Process: [`PATTERN_REVIEW.md`](PATTERN_REVIEW.md).
Upstream states: [`PYTHON_SPEEDUPS.md`](PYTHON_SPEEDUPS.md).
