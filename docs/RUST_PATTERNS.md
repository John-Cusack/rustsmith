# Rust patterns — the living guide

What actually made our Python→Rust (and C/Fortran→Rust) ports faster,
distilled from reading the original hot path and the Rust port side by
side for every port in the fleet, with the bench/headroom/retro
evidence attached. Thesis pillar 3: review how we write Rust, keep
the patterns that pay, retire the ones that stop paying.

**How this file stays honest**

- Every pattern claim cites a measured figure with its workload label
  (size + distribution + track: representative / showcase / overhead)
  and a source (bench rows file, ADR, report, or mirror repo + commit).
  Figures from different boxes are never averaged; where two
  measurements of the same shape disagree, both are kept and labeled.
- Speedup = baseline wall / port wall on the same corpus, same
  machine. Peak RSS is child-process / subprocess `ru_maxrss`, both
  arms, unless a row says otherwise.
- Unmeasured ideas live only in [Hypotheses](#hypotheses-unmeasured).
  Nothing there is advice yet.
- Losers are first-class: every pattern names where it does **not**
  help, with the losing number. A pattern with no recorded loser is
  under-tested, not universal.
- Updates come only through the review pass in
  [`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) (new evidence, new pattern,
  or retirement), never by hand-waving a number in.

**Confidence** = how many independent ports confirm the pattern with
measured rows: **High** ≥3 ports · **Medium** 2 ports · **Single**
1 port (measured, not yet replicated).

Seed: the seven-rule craft list distilled 2026-10-20 from crc PR212
and sympy-rust. This guide supersedes that private list; rules 1–7
map to P2, P2, P3, P3, P7, P4/P5, and the measurement discipline in
`PATTERN_REVIEW.md` respectively.

---

## Top 10 patterns by measured impact

Ranked by best *representative-track* speedup attributed to the
pattern (showcase ceilings in the pattern entries, never here).
Impact ≠ applicability: read the "does NOT help" line before using.

| # | Pattern | Best representative evidence | Confidence |
|---|---|---|---|
| 1 | [P2 One tight loop over primitives](#p2-one-tight-loop-over-primitive-values) | crc 2044.84x crc32-64KB, RSS 17.4→17.4 MB (`mirror/crc/bench/rows.md`) | High |
| 2 | [P5 Kill the O(n·m) / O(n²) memory shape](#p5-kill-the-onm--on-memory-shape-rolling-row-dense-arrays) | strsimpy 65.15x levenshtein-200, alloc 44045→27 KB on LCS showcase (`mirror/strsimpy/bench/rows.md`) | High |
| 3 | [P1 Whole hot unit crosses once](#p1-whole-hot-unit-crosses-the-ffi-boundary-once) | docutils 17–52x small docs, ~13x lower peak RSS (docutils-rust `8e53457` README bench table) | High |
| 4 | [P8 Hand byte/state-machine lexer](#p8-hand-lexertokenizer-over-dispatch-heavy-python-scanning) | sqlparse 11.7x format-reindent-1x (sqlparse-rust `0d4fcd3`); pyparsing 201.50x showcase scan | High |
| 5 | [P9 Sequential pass, no strided walk / store chain](#p9-one-sequential-pass-kill-strided-walks-and-store-chains) | Elmer `mtr_sum` 19.72x n=1024, RSS +0.2% (elmer-rust PR9 recraft rows) | Single |
| 6 | [P6 Compile/resolve once, run many](#p6-compileresolve-once-run-many-cached-plans-bundles-tables) | tabulate 10.12–11.87x 400-row formats (tabulate-rust `b1fe9a2`); dateutil fuzzy 10.46–13.59x rows | High |
| 7 | [P4 Borrowed byte-slice lexer](#p4-borrowed-byte-slice-lexer-tokens-as-spans) | dateutil 6.86x strict / 10.53x fuzzy, RSS +~1.2 MB (`mirror/python-dateutil/bench_parser/`) | Medium |
| 8 | [P10 Batch the query, not the item](#p10-batch-queries-and-many-crossings) | dateutil rrule 5.37–8.76x; marshmallow 4.53x nested-batch-500x4 | Medium |
| 9 | [P3 Hoist the invariant branch out](#p3-hoist-invariant-branching-out-bulk-pre-transform) | crc upstream pure-Python 16.8x/24.7x/4.9x (PR212); packaging 4.07x version.parse fast path | High |
| 10 | [P12 Engine in Rust, object layer stays Python](#p12-engine-in-rust-objecttree-layer-stays-python) | pyyaml 3.04x scan / 3.05x safe_load; sqlparse 5.3–5.7x parse | High |

Deliberately not in the top 10 despite being load-bearing:
[P7 fast path + delegation guard](#p7-narrow-fast-path--delegation-guard--bail-to-fallback)
(it *enables* the wins above without claiming one itself) and
[P11 probe headroom before porting](#p11-probe-headroom-first-port-only-the-proven-kernel)
(it saves lanes, not cycles — charset and pygments are its receipts).

---

## Per-port index

Every port reviewed for this guide: hot path read side by side
(original vs Rust), patterns found, best representative figure, and
its honest loser. Detail in the pattern entries and
[Appendix A](#appendix-a-per-port-findings).

| Port | Patterns found | Representative (labeled) | Loser / no-change | Source |
|---|---|---|---|---|
| crc | P2, P3, P7 | 2044.84x crc32-64KB; 1737.09x crc16-frames; RSS flat | 2.36x only on 1-byte X1 (crossing floor) | `mirror/crc/bench/rows.md`; upstream-shape PR212 16.8x/24.7x/4.9x pure-Python (`data/rs-upstream-crc-dry/report.md`, ADR-049) |
| strsimpy | P2, P5 | 65.15x levenshtein-200; 9.17x cosine-2 | 2.07x X1 single short pair | `mirror/strsimpy/bench/rows.md`; upstream rolling-row 2.3–2.6x + ~350–570x less memory (`data/rs-upstream-sweep-*/report.md`) |
| pyyaml | P12, P1, P7 | 3.04x scan-8.7KB; 3.05x safe_load; 4.29x bulk showcase | 0.93x dump on synthetic docs (lane box; 1.92x on bench-backfill box — both kept) | `mirror/pyyaml/bench/rows.md`, ADR-029 |
| dateutil | P4, P10, P6, P7 | 10.53x fuzzy-noisy; 6.86x strict; rrule 5.37–8.76x | 2.09x iso (already byte-slice shaped); iteration/`__next__` unchanged by design (ADR-021) | `mirror/python-dateutil/bench_parser/`, ADR-021/024 |
| python-multipart | P12, P8, P2 | 1.86x parse-1.7MB; 2.72x headers-3000 | 1.20x X1 single header; upstream pure-Python event tweak only 1.02x on 8 MiB single-write (data-copy bound) | `mirror/python-multipart/bench/rows.md` |
| pyparsing | P8, P7, P11 | 201.50x showcase `charsnotin`-1MB scan | **0.98x word-list / 0.98x config-lines grammars — no-change** | `mirror/pyparsing/bench/rows.md` |
| pygments | P6, P11, P7 | 9.32x showcase `regex_opt` import-time builder | **1.00x lex-only, 1.03x highlight — C `re` dominates** (P11) | `mirror/pygments/bench/rows.md`, ADR-038 |
| markdown-it-py | P12(host), P1-neg | 2.36x render-16.7KB default | **0.20x render-280KB bulk — per-token crossings** (P1 violated) | `mirror/markdown-it-py/bench/rows.md` |
| packaging | P3, P2, P6 | 4.07x version.parse-2000; 2.21x resolver-15k showcase | 1.41x marker.evaluate; 1.16x X1 | `mirror/packaging/bench/rows.md` |
| charset-normalizer | P2, P11 | 2.60x from_bytes 3 mixed docs | **0.33x 121KB ascii-logs; 0.57x 1MB latin bulk — upstream ascii/C fast path wins** | `mirror/charset-normalizer/bench/rows.md`, ADR-044 |
| docutils | P1, P8, P5 | 17–52x small docs; 67x bulk-256KB showcase; ~13x lower RSS | subset port, not drop-in (scope loser, disclosed) | docutils-rust `8e53457` README; README board row 2026-10-17 |
| sympy (polys) | P11, P2, P5 | 5.06x dup_mul_800; 8.75x dmp_mul_3var; RSS ~flat | **0.17x dup_gcd_70 — upstream heuristic GCD beats our PRS** | sympy-rust `325b926` `bench/rows.md`, ADR-048 |
| tabulate | P1, P7, P6 | 10.12x grid / 11.87x rst 400-row; RSS ~equal | fallback classes replay pure Python (identical, slower) by design | tabulate-rust `b1fe9a2` `benches/rows.md`, ADR-053-tabulate |
| pycparser | P8, P1, P7 | 5.5–8.0x parse real/small TUs; 2.4x parse 390KB TU | roundtrip only 1.9x on 390KB; peak RSS ~1.5x there (both trees alive) | pycparser-rust `35c2965` README (3 runs, loaded box — labeled) |
| marshmallow | P6, P10, P7, P3 | 4.53x nested-batch-500x4; RSS 23.8→25.0 MB | schemas with hooks/overrides → whole-schema pure fallback (1.0x by construction) | marshmallow-rust `e8d863d` README, ADR-053-marshmallow |
| babel | P6, P7, P12 | decimal 2.2–2.3x, currency 3.4x, datetime 3.3–3.5x; frozen workload 3.3x | `Locale.parse` lookup track unported → no win claimed (~1.0x; stays Python per ADR-054 seam) | babel-rust `28881a5`; README board row 2026-10-28, ADR-054 |
| sqlparse | P1, P12, P8, P6 | 7.4x split-1x; 11.7x format-reindent-1x; 4.3x parse-single | peak RSS +0–20% bulk (batch tables vs generator pipeline) | sqlparse-rust `0d4fcd3` README; `data/rs-port-sqlparse/{bench,retro}.md`, ADR-053-sqlparse |
| Elmer recrafts | P9, P3-neg, P2-neg | `mtr_sum` 4.76–19.72x; `ludecomp` 1.04–1.13x; RSS ±0.4% | `jacobi` 1.01x / variant 0.96x — reverted; branch-hoist/index variants 0.81–0.94x — rejected (compiler already does it) | elmer-rust PR9 / rustsmith PR110 recraft rows (open PRs at measurement; digest-identical arms) |

---

## Patterns

### P1. Whole hot unit crosses the FFI boundary once

**Problem shape.** A Python hot loop calls a small helper per item
(token, event, cell, field). Porting only the helper pays a
per-crossing toll (PyO3 conversion + GIL + call overhead) per item;
the toll eats the kernel win and can invert it at bulk sizes.

**Technique.** Move the boundary out to the whole hot unit: the
entire input (document, SQL string, table, batch) crosses once in,
the finished result crosses once out. Count crossings *before*
choosing thin-loop vs whole-component port (`guidance/optimize.md`:
per-call Rust work < ~1 µs is boundary-dominated → batch, hoist the
loop across, or don't port).

Before (markdown-it, the violating shape — token stream materialized
through host callbacks per token):

```python
# markdown_it/ renderer + rules walk tokens one at a time;
# the Rust side calls back through the Host trait per token/plugin rule
for token in tokens:            # mirror/markdown-it-py: host.rs / host_convert.rs
    host.render_token(token)    # one crossing per token
```

After (sqlparse — whole string in, flat result out, one assembly pass):

```rust
// sqlparse-rust 0d4fcd3, sqlparse-core/src/lexer.rs
pub fn lex(text: &str) -> Vec<RawTok> { /* entire input, no callbacks */ }
```
```python
# sqlparse-rust, sqlparse/lexer.py — pristine default lexer routes whole-call
if type(self) is Lexer:  # ... -> _core.tokenize_flat(text)
```

**Measured.** docutils (whole publish pipeline in Rust): 17–52x
small docs, 67x bulk-256KB showcase, ~13x lower peak RSS
(625908→47300 KiB bulk). sqlparse: 5.3–12.4x across split/parse/
format. tabulate: one kernel call per `tabulate()` → 10.12–11.87x.

**Does NOT help / inverted when violated.** markdown-it bulk:
**0.20x** on render-280KB (587→2972 ms) while the same port is 2.36x
on 16.7KB — per-token crossings scale with token count, kernel win
does not. mako (ADR-064): whole-template crossing is *impossible* —
template nodes are user Python evaluated per node, so the seam keeps
100% of per-node work while turning each buffer write (one C
`list.append`, ~50 ns) into a PyO3 call; realizable ceiling 1.17x
over tier-1 at zero toll, expected ≤1.0x with toll — no_gain.
pyyaml's accepted compromise (one FFI call per token/event,
ADR-029) still wins 3.04x because the per-token Rust work (scanner
state machine) is large; the toll is bounded and *disclosed*, not
absent. Rule: if per-item work < ~1 µs, P1 is mandatory; if per-item
work is a whole state-machine step, measure the toll explicitly
(X1 overhead rows exist for exactly this).

**Confidence: High** (docutils, sqlparse, tabulate, marshmallow,
dateutil-rrule confirm; markdown-it confirms by violation).

---

### P2. One tight loop over primitive values

**Problem shape.** Per-item object traffic in the hot loop: boxed
values, property access, virtual/`dyn` dispatch, per-item allocation.
The algorithm is trivial; the *representation* is the cost.

**Technique.** One loop over plain ints/bytes/floats in registers or
a flat slice. No per-item boxing, property access, or dispatch.
Configuration read once before the loop.

Before (crc upstream shape, per `data/rs-upstream-crc-dry/report.md` §2):
each byte wrapped in a `Byte` object, `register` property traffic and
a virtual `_process_byte` call per byte.

After (same algorithm, primitives only — this exact loop is the
upstream pure-Python win *and* the Rust core shape):

```python
for byte in data:
    index = byte ^ (register >> shift)
    register = (lookup_table[index] ^ (register << 8)) & bitmask
```
```rust
// mirror/crc/crc-core/src/lib.rs
pub fn update_table(reg: u64, cfg: &Config, table: &[u64; 256], data: &[u8]) -> u64 {
    let mut r = reg;
    for &b in data { /* table step on u64, no allocation */ }
    r
}
```

**Measured.** crc Rust: 2044.84x crc32-64KB (1308.5→0.64 ms),
1737.09x crc16-frames, 1844.80x crc64-1MB showcase, RSS flat
(17.4–18.7 MB both arms). The *same loop shape in pure Python*
(upstream PR212) already gave 16.8x CRC-32-5MiB / 24.7x CRC-16 /
4.9x CRC-64 — proof the win is the loop shape first, the language
second. sympy `dup_mul` (flat `&[Qq]` slices, zero-skip, one output
`Vec`): 5.06x/8.75x. strsimpy distance kernels: 65.15x.

**Does NOT help.** When the loop body must call user code per item
(pygments `RegexLexer` callbacks, marshmallow custom fields) —
dispatch is semantic, not waste; use P7 opaque nodes instead. When
the original already runs in C per item (charset ascii path, 0.33x —
see P11). In already-compiled Rust, micro-recrafting a tight loop
may add nothing: Elmer `jacobi` recraft 1.01x, reverted (P9).

**Confidence: High** (crc, strsimpy, sympy, multipart, charset mixed-docs).

---

### P3. Hoist invariant branching out; bulk pre-transform

**Problem shape.** A branch, mode check, or per-item transform inside
the loop whose outcome is fixed for the whole call (config flag,
input class, format kind).

**Technique.** Decide once before the loop; if the per-item work is
a pure byte/char map, apply it as one bulk transform up front
(`bytes.translate`, slice/chunk ops) so the loop body is branch-free.
Two loop shapes with one body each beat one loop with a branch.

After (crc PR212, ADR-049): `reverse_input` reflection applied once
via a 256-byte `bytes.translate` table before the loop; the
alternative (per-byte table index inside the loop) measured ~20%
slower (crc32-1MiB 0.104s vs 0.122s) and was dropped. Packaging
applies the same idea as an input-class fast lane:

```rust
// mirror/packaging/packaging-core/src/version.rs parse()
// Fast path: only digits and dots (mirrors _SIMPLE_VERSION_INDICATORS).
if s.bytes().all(|b| b == b'.' || b.is_ascii_digit()) { /* no regex at all */ }
```

**Measured.** crc upstream 16.8x/24.7x/4.9x (with P2). packaging
4.07x version.parse-2000 vs 1.41x on the marker path that has no
such lane. Pygments `escape_html_text` early-outs when no byte needs
escaping (kernel correct; pipeline still 1.00x for P11 reasons —
the hoist paid only where escaping was actually the cost,
`regex_opt` aside).

**Does NOT help.** In compiled Rust the compiler already unswitches
loop-invariant branches and strength-reduces index math: Elmer
`ludecomp` variants "row base hoisted" 0.94x/0.93x and "split k-range
into two branch-free bodies" 0.81x were all *rejected* on
measurement (elmer-rust PR9). P3 is a Python→Rust-porting and
algorithm-shape rule; re-applying it by hand inside Rust needs a
bench per variant, one change at a time. Also costs memory when the
bulk transform materializes a copy: crc `translate` holds one extra
input-size buffer for `reverse_input` configs (disclosed in PR212).

**Confidence: High** for port shape (crc, packaging, dateutil);
**explicitly not transferable** as a blind in-Rust recraft (Elmer).

---

### P4. Borrowed byte-slice lexer; tokens as spans

**Problem shape.** A lexer/parser that copies the input repeatedly:
owned `String` → `Vec<char>` (4x expansion) → per-token `String`s,
then re-scans tokens with `chars().count()`.

**Technique.** Borrow the input once (`&[u8]` / `&str`), walk it a
single pass, classify ASCII bytes through a `const [u8; 256]` table,
fall back to char decode only for bytes ≥ 0x80. Tokens are spans
(offset+len+counters), materializing a `String` only where the
original semantics eagerly normalize (dateutil: `,`→`.` folding,
`\x00` filtering — ADR-024).

After (dateutil, ADR-024): `Timelex<'a>` borrows input; `Token<'a>`
carries `nchars`/`ascii` counters so length checks never re-scan;
binding keeps one owned `String` alive for the whole parse.

**Measured.** dateutil parser rows (bench-backfill box,
2026-10-14): strict 6.86x, fuzzy-noisy 10.46–10.53x,
fuzzy_with_tokens 10.82x, edge rows to 13.59x; port RSS ~1.2 MB
*above* baseline (extension pages — disclosed, 16.2→17.4 MB).
Differential: 6138/6138 corpus rows identical before/after the
borrow rewrite.

**Does NOT help.** Where tokens must outlive the input or cross back
as owned Python objects per token anyway (markdown-it token objects:
borrowed spans can't fix a P1 violation). Remaining per-token allocs
in dateutil (`lower_token` per table probe) were deliberately left —
measure before chasing the tail.

**Confidence: Medium** (dateutil primary; pyparsing `RawUnits`
borrowed-unit scan is the same shape, its win is booked under P8).

---

### P5. Kill the O(n·m) / O(n²) memory shape: rolling row, dense arrays

**Problem shape.** DP / matrix algorithms materializing the full
table (list-of-lists of Python ints) when only one row — or one flat
dense buffer — is live.

**Technique.** Rolling two-row DP; dense flat `Vec<T>` coefficient
arrays instead of nested object graphs. Same arithmetic, same order
(bit-exact where floats/rationals are involved), asymptotically less
allocation.

After (sympy-rust `325b926`, dense univariate multiply over `QQ`):

```rust
pub fn dup_mul(f: &[Qq], g: &[Qq]) -> Vec<Qq> {
    let mut h = vec![Qq::zero(); f.len() + g.len() - 1];
    for (i, a) in f.iter().enumerate() {
        if a.is_zero() { continue; }
        for (j, b) in g.iter().enumerate() {
            if b.is_zero() { continue; }
            h[i + j] = &h[i + j] + &(a * b);
        }
    }
    strip_vec(h)
}
```

**Measured.** strsimpy metric-LCS showcase: 471.27x and Python-alloc
peak 44045→27 KB (RSS 85.6 MB both arms — process peak is setup;
the *alloc* column is the honest signal). The rolling-row shape
alone, contributed upstream in pure Python: LCS 2.3–2.6x with
~350–570x less peak memory (35.5 MB→62 KiB on 2×2 KiB pairs);
Levenshtein hoist in the same patch only 1.02–1.05x — the memory
shape was the win, the loop tweak was not. sympy: 5.06x dup_mul_800,
8.75x dmp_mul_3var at flat RSS.

**Does NOT help.** When the *algorithm choice*, not the layout, is
the gap: sympy `dup_gcd_70` **0.17x** — upstream's heuristic
(evaluation/interpolation) GCD beats our correct PRS implementation
regardless of representation. Porting the slower algorithm faster
still loses. Docutils' ~13x RSS win is partly this pattern
(owned tree in Rust vs Python object graph) and partly P1;
don't book it twice.

**Confidence: High** (strsimpy, sympy-mul, docutils).

---

### P6. Compile/resolve once, run many: cached plans, bundles, tables

**Problem shape.** Per-call re-derivation of invariant structure:
re-parsing format patterns, re-walking schema/field metadata,
re-resolving locale data dicts, rebuilding keyword regexes.

**Technique.** Pay the derivation once per process (or once per
schema/locale/lexer-class), cache the compiled artifact Rust-side,
and cross per call with scalars only.

Evidence shapes:

- **babel** (`28881a5`): per-locale ingredient bundle (patterns
  parsed eagerly) cached once at `ensure_locale`; per-call formatting
  crosses with scalars, never CLDR dicts (`babel-core/src/bundle.rs`).
  No second CLDR copy vendored — cold load of 6 locales measured
  27.5 ms one-time vs 201 ms per formatting run (ADR-054), so the
  cache is small derived data, not a data fork.
- **marshmallow** (`e8d863d`): schema compiled to a Rust plan on
  first dump/load; exact-type fast paths (int/float/str/bool) skip
  per-value calls (ADR-053-marshmallow).
- **sqlparse** (`0d4fcd3`): default lexer config (regex order +
  813-entry keyword map) baked into the core at build time from the
  installed upstream, with provenance header (ADR-053-sqlparse).
- **pygments**: `regex_opt` keyword-regex builder runs at
  lexer-definition import time — porting just that builder measured
  9.32x on its showcase row while everything else tied (P11).

**Measured.** babel 1.9–3.5x across six formatting tracks, frozen
workload 3.3x. marshmallow 4.53x (with P10). tabulate's format
tables/width tables compiled into the core contribute to its
10–12x (with P1). pygments `regex_opt` 9.32x showcase,
alloc 43.4→0.8 KB.

**Does NOT help.** When the "invariant" is user-mutable: sqlparse
routes mutated `Lexer` singletons to the pure path per call;
marshmallow decorated hooks forfeit the plan; babel user-supplied
patterns stay Python. A cache keyed on anything the caller can
mutate without a guard is a correctness bug, not a pattern — pair
with P7 pristine-checks. Also forbidden outright: caches keyed to
distinguish visible/bench inputs from same-distribution inputs
(`SPEC_STAGE2.md` §8).

**Confidence: High** (babel, marshmallow, sqlparse, pygments-partial, tabulate).

---

### P7. Narrow fast path + delegation guard / bail-to-fallback

**Problem shape.** The fast path is exact for the common input class
but the API admits subclasses, callbacks, exotic types, and plugins
whose semantics live in Python.

**Technique.** Guard the fast path with a cheap exactness check;
anything unprovable delegates to the *verbatim original* code path —
same digests, same exception types/messages, by construction. Three
proven seam shapes (retro batch 2026-10-11): (i) hot state machines
to core + verbatim Python orchestration (multipart, pyyaml);
(ii) scan-kernel delegation via minimal override + pure-Python
fallback importable without the extension (pyparsing, pygments);
(iii) host-callback trait for plugin seams (markdown-it `Host`).
Refinement (marshmallow, ADR-053): bail per-*field* (opaque nodes
invoke the user's own callable per value) instead of per-schema —
one `validate=lambda` shouldn't forfeit 29 fields of kernel win.

After (tabulate-rust `b1fe9a2`): kernel returns a private
`Fallback` for anything it cannot represent exactly (user
`TableFormat`, exotic cell types, i64-overflow ints, …); the shim
replays the byte-identical vendored implementation for that call.
crc PR212 shape: non-`bytes` input or an overridden `_process_byte`
→ `super().update(data)`, one merged condition.

**Measured.** This pattern claims no speedup of its own; it is what
lets P1/P2/P6 ship at 100% oracle parity (tabulate 318-suite +
400/400 differential at 10–12x; marshmallow 1188/1188 at 4.53x;
sqlparse 506-suite + fuzz at 4.3–12.4x; babel 69,401-comparison
equivalence, kernel hit 99.9%, at 1.9–3.5x).

**Does NOT help.** If the guard itself is per-item expensive, or the
fallback class is the *common* class (then you ported the rare path:
pyparsing grammars 0.98x — the scan kernel pays, real grammars route
around it). Lone surrogates must route to fallback (pygments):
Rust `String` cannot hold them; silent U+FFFD corruption is worse
than no speedup.

**Confidence: High** (every shipped port uses a variant).

---

### P8. Hand lexer/tokenizer over dispatch-heavy Python scanning

**Problem shape.** Character-at-a-time Python scanning: regex-match
per position with Python-level dispatch between matches, or
combinator machinery evaluating a token predicate per char.

**Technique.** A single-pass Rust lexer over bytes/units with table
classification (see P4) and span tokens; configuration (keyword
maps, regex order) baked as data (P6). Keep CPython `re` semantics
exact where the original used them — probe-then-fuzz the edges
(sqlparse retro: 6+ `re` edge semantics found only by the fuzzer).

**Measured.** pyparsing `CharsNotIn` scan kernel: 201.50x showcase
(429→2.1 ms, 1MB scan). sqlparse lexer+grouping: 7.4x split-1x,
11.7–12.4x format rows. pycparser hand lexer + recursive-descent
parser: 5.5x (4KB real), 6.3x (30KB real TU), 8.0x (7KB synth),
2.4x (390KB synth) parse. pyyaml scanner state machines: 3.04x scan
(via P12 facades).

**Does NOT help.** When the shared C `re` engine already *is* the
hot path and Python dispatch between matches is thin: pygments
lex-only **1.00x** (110KB Python) — a Rust dispatcher would pay
FFI+GIL per rule attempt and still call the same `re` (ADR-038,
negative boundary with measured rationale). When the lexer is a
runtime-constructible user grammar: pyparsing real grammars
**0.98x** — only the fixed scan kernels port, the combinator engine
stays. Headroom probe separates these before you build (P11:
pygments regex fraction 0.54 capped its score).

**Confidence: High** (pyparsing-scan, sqlparse, pycparser, pyyaml;
pygments as the measured counter-case).

---

### P9. One sequential pass; kill strided walks and store chains

**Problem shape.** (Compiled-code recrafts, C/Fortran→Rust.)
Transcribed loops preserving the original's memory-access order:
column-at-a-time walks over row-major data (one cache line per
element), or inner loops storing a partial result to memory every
iteration (n³ store chain).

**Technique.** Reorder traversal to one sequential pass accumulating
per-column results in the same arithmetic order (bit-exact); hold
the partial sum in a register and store once, after proving no
iteration reads the location back.

**Measured** (Elmer recrafts, elmer-rust PR9; digest-identical arms,
rep-interleaved, fresh child per sample): `mtr_sum` column sums
4.76x (n=256), 19.72x (n=1024), 12.44x (n=4096), RSS ±0.2%.
`ludecomp`/`mtrinv` (pivot in register, row-pointer walks, hoisted
final store): 1.04–1.13x, RSS ±0.4%.

**Does NOT help.** Most textbook micro-recrafts in compiled Rust:
`jacobi` 1.01x and a second variant 0.96x (both reverted);
`ludecomp` sub-variants for hoisted branches / index math measured
0.81–0.94x (rejected) — LLVM already unswitches and strength-reduces.
The transferable pattern is the *measurement discipline*: one change
per variant, digest-gated A/B, revert losers, keep the no-gain row.
Memory-access *order* is the one thing the compiler cannot fix for
you; branch/index cosmetics usually are already fixed.

**Confidence: Single** (Elmer; two units, one big win, one marginal,
one honest no-gain).

---

### P10. Batch queries and `many=` crossings

**Problem shape.** An API whose natural use is bulk (window query
over a recurrence set, `many=True` serialization, resolver filtering
thousands of versions) implemented as per-item pull across the
boundary: one Python object construction + rich compares per
candidate while the engine step is nanoseconds (dateutil audit V2).

**Technique.** Implement the bulk operation as a single crossing:
run the filter/engine in Rust over primitive representations (dateutil
`Wall` ord/hh/mm/ss/us triples with integer bound prefilters),
materialize Python objects once per *surviving* item. Keep the
per-item path as the documented slow fallback for shapes the batch
path cannot prove (mixed tz, folded points → `Slow` rerun, ADR-021).

**Measured.** dateutil rrule: 5.37x `between`, 5.75x after/before-200,
8.76x count-10y-hourly showcase, RSS flat 16.8 MB (bench-backfill;
lane box reported the same shape at 1.6–4.2x — box difference, both
kept). marshmallow whole-batch crossing: 4.53x on
nested-batch-500x4. packaging resolver-filter-15k showcase 2.21x
(batch evaluation shape).

**Does NOT help.** Genuinely incremental/pull consumers: dateutil
iteration, `__next__`, slices keep the per-item path unchanged *by
design* — batching them would change laziness/identity semantics.
Batches also trade RSS for crossings: sqlparse's batch tables cost
+0–20% peak RSS on bulk rows; pycparser holding both trees costs
~1.5x RSS on the 390KB TU (see [conversion costs](#cross-cutting-pyo3-conversion-costs)).

**Confidence: Medium** (dateutil, marshmallow; packaging partial).

---

### P11. Probe headroom first; port only the proven kernel

**Problem shape.** Downloads and LOC cannot tell whether a rewrite
pays. Two failure modes burned full lanes before this rule existed:
the hot path already runs in a shared C engine, or the maintainer
already shipped a compiled core.

**Technique.** Before any port: profile a representative workload
(`rustsmith headroom probe`, ADR-044), classify hot time into
pure_python / native / io_wait / regex / overhead, score
`pure_python × loop_factor` with caps (shared-engine, saturated,
io-bound), and port only kernels scoring `rewrite`. Scope the port
to that kernel (sympy: polys-domains 0.85 rewrite, simplify-core
0.49 upstream_only — one 497K-LOC library, two opposite verdicts).

**Measured (the saves).** charset-normalizer: probe would have
flagged saturation; the port that predates the probe measured
2.60x mixed docs but **0.33x ascii-logs / 0.57x latin bulk**, and
five behavior-identical upstream candidates were all neutral-or-
slower (upstream sweep kill-table). Note the alloc column still
improved (2112→28 KB on latin bulk) while wall lost — allocation
wins do not imply wall wins against a C fast path. pygments:
regex fraction 0.54 → highlight 1.03x/lex 1.00x confirmed post-hoc.
Elmer campaign: 1157 units probed, 0 cleared `rewrite` past prior
refusals — the ranked shortlist redirected the campaign to the seam
instead of more units.

**Does NOT help.** The probe is a *prediction* with labeled caps,
not a bench: babel scored 0.79 and landed 1.9–3.5x (good), but a
`rewrite` verdict never substitutes for post-port measurement and
kill criteria (`PATTERN_REVIEW.md` requires both).

**Confidence: High** (charset, pygments, sympy split, Elmer 1157-row
probe, plus every 2026-10-28 lane re-probed before building).

---

### P12. Engine in Rust, object/tree layer stays Python

**Problem shape.** A parser/serializer whose value is half CPU
(scan/parse/emit state machines) and half Python object semantics
(node classes, `match` identity, constructor/representer glue over
arbitrary user objects).

**Technique.** Port the state machines to a Python-free core crate;
keep the object layer byte-identical Python. Results cross as flat
descriptors and are assembled into genuine upstream objects in one
Python pass — the entire tree API is then exact by construction
(sqlparse ADR-053), and arbitrary-object glue never crosses FFI
(pyyaml ADR-029: constructor/representer stay Python). Where lazy
semantics are observable (multi-doc generator error ordering),
facades pull descriptors lazily instead of batching (pyyaml rejected
batch-scan-upfront for exactly this).

Before/after seam (pyyaml `mirror/pyyaml/yaml/scanner.py`): the
Python `Scanner` lazily pulls token descriptors from
`yaml._rust.Engine` when present; the original Python scan loop
remains in-file as the no-extension fallback.

**Measured.** pyyaml 3.04x scan / 3.05x safe_load / 4.29x bulk
showcase (dump shape-dependent: 1.92x bench box, 0.93x lane box).
sqlparse parse 5.3–5.7x with the tree built Python-side; retro note:
constructing the tree *directly* in the binding cut parse-8x RSS
from +33% to +9% — build direct construction first, RSS is a
first-class citizen for parser ports. multipart 1.86x/2.72x with
verbatim Python orchestration around the Rust byte state machines.

**Does NOT help.** When the object layer *is* the hot path
(markdown-it: token objects + plugin callbacks per token → the
hybrid degenerates into P1 violation, 0.20x bulk). When per-node
descriptor conversion exceeds engine savings at small sizes, X1
rows expose it (pyyaml tiny-scan 1.23x only; upstream-shape scan
tiny-doc 0.85x regression disclosed in the lane-1 report).

**Confidence: High** (pyyaml, sqlparse, multipart, babel).

---

## Cross-cutting: PyO3 conversion costs

Conversion is a per-crossing tax with three measured shapes:

1. **Per-item conversion dominates** — markdown-it 0.20x bulk (P1).
   Mitigation: batch (P10), flat descriptors (P12), scalars-only
   crossings after a cached bundle (P6, babel).
2. **Both representations alive at once** — pycparser peak RSS
   ~1.5x on the 390KB TU (Rust tree + converted Python tree held
   simultaneously); sqlparse +0–20% bulk RSS until direct
   construction and dropping lexer char/offset tables before return.
   Mitigation: drop source tables before returning; construct the
   target directly instead of via an intermediate tuple tree.
3. **Extension baseline** — ports run ~1–2 MB RSS above pure Python
   on small inputs (dateutil +1.2 MB, marshmallow ~+2 MB, tabulate
   +0–1.2 MB): extension pages, disclosed, relevant only when the
   workload itself is tiny — which is exactly where speedups are
   smallest too (X1 rows: 0.99–3.54x across the fleet; pyparsing
   and pygments X1 are no-change at 0.99x).

Every bench in this fleet therefore carries an X1 per-crossing row
and both-arms peak RSS. A pattern proposal without them is
incomplete (`PATTERN_REVIEW.md` checklist).

---

## Anti-pattern ledger (measured losers, fleet-wide)

| Anti-pattern | Receipt | Do instead |
|---|---|---|
| Per-token/per-item FFI crossing | markdown-it 0.20x @280KB | P1 whole-unit crossing |
| Template-render kernel, nodes are user Python per node | mako realizable ≤1.17x over tier-1 at zero toll, expected ≤1.0x w/ writer toll (ADR-064) | Walk away (no_gain); tier-1 only |
| Validator port leaving shared Unicode/IDNA cost behind the seam | email-validator own-code ceiling 1.66x over tier-1 (60% of CPU in idna/stdlib); idn track 0.98x tier-1 (ADR-065) | Walk away (no_gain); tier-1 only — or port the dependency (different package) |
| Reimplementing a shared C engine (`re`, codecs) | pygments lex 1.00x; charset latin bulk 0.57x | P11 probe; port around it or walk away |
| Porting the slower algorithm faithfully | sympy PRS gcd 0.17x vs heuristic GCD | Match algorithm class first (P5 note) |
| Kernel nobody's workload hits | pyparsing grammars 0.98x vs scan 201x | Profile real grammars; P7 hit-rate (babel reports 99.9%) |
| Hand-unswitching / strength-reducing compiled Rust | Elmer variants 0.81–0.94x rejected; jacobi 1.01x reverted | P9: fix access order only; A/B every variant |
| Batch materialization ignoring RSS | sqlparse parse-8x +33% RSS (first design) | Direct construction; drop tables pre-return |
| Alloc win read as wall win | charset latin: alloc 2112→28 KB, wall 0.57x | Report both columns; gate on wall |
| Showcase quoted as typical | (process rule) | Two-track benches; representative gates, showcase labels ceiling |

---

## Hypotheses (unmeasured)

Nothing in this section has a fleet measurement. Each entry states
what bench would promote or kill it. Do not cite these numbers —
there are none.

- **H1 SIMD / vectorizer-friendly loop rewrites** (contiguous
  slices, split borrows, no inner-loop calls — `guidance/optimize.md`
  tier 8). No port has measured a SIMD variant. Promotion bench:
  pyparsing scan kernel or charset `mess_ratio` A/B with remarks
  evidence, per the stable-SIMD rule (no `portable_simd` on stable).
- **H2 SoA / AoSoA layouts for field-wise iteration** (guidance
  tiers 3/5; 10–100x layout multiples explicitly uncalibrated
  there). Closest measured relative is P9's sequential pass, which
  is an access-order fix, not a layout change. Promotion bench: an
  Elmer fem unit with field-wise loops, digest-gated.
- **H3 Arena allocation for parser trees** (docutils/pycparser
  trees are `Vec`-owned today). Promotion bench: pycparser 390KB TU
  RSS + wall A/B; the 1.5x RSS receipt says where to look.
- **H4 Parallelism (rayon) over independent documents/items.**
  Guidance says "rayon before serial is tight" is a dead end; no
  fleet port has measured it. Promotion bench: pyyaml load_all
  300-doc stream, with GIL-release correctness proof.
- **H5 PGO/BOLT on the binding crates.** Unmeasured; playbook §8
  only warns these runs are long. Promotion bench: any P1 port,
  same frozen workloads, build-time cost disclosed.

---

## Appendix A: per-port findings

One paragraph each: what the side-by-side read found. Numbers are
the rows cited in the [index](#per-port-index).

- **crc** — Before: `Byte` boxing + property + virtual dispatch per
  byte. After: P2/P3 single int loop, `translate` pre-transform,
  P7 delegation guard. The pure-Python upstream shape (PR212)
  capturing 16.8–24.7x before Rust enters the picture is the
  fleet's cleanest proof that loop shape > language.
- **strsimpy** — Before: full-matrix DP of Python ints. After:
  rolling rows / dense kernels (P5+P2). Alloc column (44 MB→27 KB)
  is the story; RSS column is flat because process peak is setup.
- **pyyaml** — Engine/port boundary (P12): scanner/parser/emitter
  cores in Rust, object layer verbatim, lazy per-token facade pull
  (bounded P1 exception, toll disclosed). Dump is workload-shape
  dependent (0.93x vs 1.92x) — kept as two labeled rows.
- **dateutil** — Two ports in one: borrowed byte lexer (P4, 3+
  input copies eliminated, 6138-row differential) and batched rrule
  window queries (P10) with a `Slow` exact-fallback. Custom
  `parserinfo`/callbacks remain behind the seam by design.
- **python-multipart** — Byte state machines to core, orchestration
  verbatim (P12 shape i). Wins are real but modest (1.76–2.72x):
  the original was already a tight state machine; headers (more
  Python-level parsing per byte) gained most.
- **pyparsing** — Scan-kernel delegation (P8+P7 shape ii). The
  201x scan ceiling and the 0.98x grammar rows are the same port:
  fixed kernels port, runtime-constructible grammars don't.
  `preParse` staying Python is a recorded negative boundary.
- **pygments** — Four small kernels only (P6 `regex_opt` pays at
  import time); the `RegexLexer` engine is a measured negative
  boundary (P11). `_translate_parts` `lru_cache` already amortized
  escaping, which is why escape-heavy rows tie at 1.00x.
- **markdown-it-py** — Host-callback seam (shape iii) is
  *correctness*-right and *bulk-perf*-wrong: 2.36x at 16.7KB,
  0.20x at 280KB. The fleet's canonical P1 violation; any future
  plugin-token port must batch or move the token stream wholesale.
- **packaging** — Input-class fast lane (P3) on version parsing
  carries the win (4.07x); marker evaluation, with real per-item
  semantics, gains least (1.41x). Resolver-shaped batch use (P10)
  is where it compounds.
- **charset-normalizer** — Correct port, saturated target (P11).
  Mixed-doc detection wins 2.60x; ascii and bulk-latin lose to
  upstream's C-level fast paths. Five upstream candidates all
  neutral-or-slower closed the direction honestly.
- **docutils** — Clean-room whole-pipeline port (P1 maximal):
  parser+transforms+HTML writer in one core, string in / string or
  doctree out. 17–67x and ~13x RSS. Cost: subset scope, not a
  drop-in — the scope loser is disclosed in its README, and the
  equivalence bar is a 7-file corpus, not the upstream suite.
- **sympy (polys kernel)** — Kernel-scope by probe (P11), dense
  arrays over QQ (P5). Multiplication wins 5–9x; GCD loses 0.17x
  to a better *algorithm*, the fleet's clearest "representation
  can't fix algorithm class" receipt.
- **tabulate** — Whole post-normalization kernel once per call (P1)
  with bail-to-fallback (P7). 1300+ CPython-generated numfmt
  vectors guard the float/int spec engine — generated-vector
  differential testing is how the exactness was bought.
- **pycparser** — Hand lexer + recursive descent replacing PLY
  regex-dispatch (P8). Parse wins everywhere; roundtrip wins less
  because `c_generator` (verbatim Python) re-enters the cost —
  Amdahl on the unported half. Both-trees RSS on big TUs is the
  conversion-cost receipt.
- **marshmallow** — Compile-once schema plans (P6) + whole-batch
  crossing (P10) + per-field opaque nodes (P7 refinement).
  Error *messages* formatted Python-side from compile-time
  templates: identical by construction, only decisions replayed
  in Rust. Seeded differential fuzz (200 schemas) caught 3 real
  divergences the suite didn't.
- **babel** — Cached per-locale bundles, scalars-only crossings
  (P6); user patterns/plurals stay Python (P7). The unported
  `Locale.parse` lookup track (~1.0x, no win claimed) is the
  honest remainder: lookup was `localedata.__getitem__` + parse
  CPU, and only the formatting half was ported.
- **sqlparse** — Kernel-in-Rust / tree-stays-Python (P12 refinement
  of ADR-053): whole string in, flat nodes out, one assembly pass.
  Two retro costs worth repeating: `re` edge semantics need
  probe-then-fuzz (6+ edges), and snapshot-iteration in `_group`
  was a semantic the suite passed without — the fuzzer caught it.
- **Elmer recrafts** — Rust→Rust recrafts under digest-gated A/B
  (P9). Access-order fix paid 4.8–19.7x; compiler-territory
  cosmetics measured 0.81–1.01x and were reverted/rejected. The
  deliverable pattern is the harness + one-change-per-variant
  discipline now templated for the remaining campaign units.

## Appendix B: sources

In-repo: `mirror/*/bench/rows.md` (bench-backfill re-measure
2026-10-14, `data/rs-bench-backfill/report.md`, ADR-039),
`mirror/python-dateutil/bench_parser/`, `docs/adr/`
021/024/029/038/044/048/049/053×3/054, `data/headroom/*.json`,
`data/rs-port-sqlparse/`, `data/rs-upstream-crc-dry/report.md`,
`data/rs-upstream-lane-1/report.md` + sweep reports,
`guidance/optimize.md`, README POC/ported boards.
Mirror repos (commit read for this guide): babel-rust `28881a5`,
docutils-rust `8e53457`, marshmallow-rust `e8d863d`,
pycparser-rust `35c2965`, sqlparse-rust `0d4fcd3`,
sympy-rust `325b926`, tabulate-rust `b1fe9a2`,
elmer-rust `484ffe6` + recraft PR9 (rustsmith PR110 evidence rows).
Process: [`PATTERN_REVIEW.md`](PATTERN_REVIEW.md).
