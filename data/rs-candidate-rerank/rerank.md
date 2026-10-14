# Candidate re-rank (2026-10-14)

Source list: `data/rs-candidate-finder/candidates.md` (live run, snapshot
`last_update: 2026-10-01`, top 30) plus 25 new names judged to the same bar.
Filters: `exclusion-filters.json` + curated verdicts (ADR-043) — the tool PR
carrying this re-rank applies them automatically; the dispositions below are
a mechanical join of `candidates.json` × filter table × alternatives table
(script: `/home/john/firstmate/data/rs-candidate-rerank/dispositions.py`).

Evidence labels: bare numbers are MEASURED (snapshot downloads 2026-10-01;
sdist file layout counted 2026-10-14 with test/doc/example trees excluded;
crates.io search hits via API 2026-10-14). `ESTIMATE` marks heuristics.
Alternative verdicts are curated-judged 2026-10-14 (same bar as the 2026-10-13
table). Unjudged crates.io hits stay unjudged.

## 1. The 30, re-ranked

| Dl | Package | dl/mo | Old standing | Filter | Effective standing |
|---|---|---|---|---|---|
| 1 | boto3 | 2.46B | mirror #7 | aws_owned/exclude | EXCLUDED — standing rule |
| 2 | packaging | 1.55B | mirror #3 | — | clean mirror, but COVERED (port #17 + sweep-1 no-gain) — do not renominate |
| 3 | typing-extensions | 1.40B | mirror #17 | — | clean mirror, thin typing dispatch — weak thesis |
| 4 | idna | 1.34B | mirror #18 | — (table: full → idna crate) | (a) adopt — flips to adopt |
| 5 | urllib3 | 1.32B | mirror #9 | io_bound/demote | demoted — network dominates |
| 6 | certifi | 1.29B | adopt noop | — | adopt (unchanged) |
| 7 | requests | 1.21B | mirror #13 | io_bound/demote | demoted — network dominates |
| 8 | charset-normalizer | 1.19B | upstream | saturated/exclude | EXCLUDED — 5 measured kills, do not renominate |
| 9 | cryptography | 1.07B | upstream | — | upstream (unchanged) |
| 10 | setuptools | 973M | mirror #4 | io_bound/demote | demoted — build orchestration is file/subprocess I/O |
| 11 | cffi | 950M | upstream | — | upstream (unchanged) |
| 12 | pygments | 932M | mirror #2 | shared_engine/demote | demoted — C `re` dominates (W2 1.00x); kernels already ported (#72) |
| 13 | pyyaml | 911M | upstream | mature_native/upstream_only | upstream, COVERED (3.8x headroom + PR #976 open) — do not renominate |
| 14 | python-dateutil | 905M | mirror #5 | — | clean mirror, but COVERED (ports #18/#19 + upstream PR #76) — do not renominate |
| 15 | six | 895M | adopt noop | — | adopt (unchanged) |
| 16 | pluggy | 878M | mirror #12 | — | clean mirror, 1.7K LOC dispatch — weak thesis |
| 17 | botocore | 853M | mirror #1 | aws_owned/exclude | EXCLUDED — old payoff #1 was the worst ranking |
| 18 | pydantic | 843M | upstream | — | upstream (unchanged) |
| 19 | pycparser | 838M | mirror #8 | — | clean mirror — table-driven C parsing is real CPU; kept, ranked below top-5 (codegen-time use, 746 dependents) |
| 20 | click | 817M | mirror #14 | — | clean mirror — CLI parse is CPU but microscopic sessions; weak thesis |
| 21 | pydantic-core | 809M | upstream | — | upstream (unchanged) |
| 22 | anyio | 803M | mirror #11 | io_bound/demote | demoted — async I/O framework |
| 23 | numpy | 802M | upstream | — | upstream (unchanged) |
| 24 | pytest | 780M | mirror #6 | — | clean mirror, but a runner: collection is filesystem I/O and runtime is user code — weak thesis |
| 25 | iniconfig | 756M | adopt noop | — | adopt (unchanged) |
| 26 | annotated-types | 724M | mirror #15 | — | clean mirror, 562 LOC — nothing to win |
| 27 | h11 | 711M | mirror #16 | io_bound/demote | demoted — socket framing |
| 28 | typing-inspection | 695M | adopt noop | — | adopt (unchanged) |
| 29 | attrs | 692M | mirror #10 | — | clean mirror — import-time codegen, thin runtime; weak thesis |
| 30 | s3transfer | 645M | mirror #19 | aws_owned/exclude | EXCLUDED — standing rule |

Net: of the old top-8 mirror slots, five are now excluded/demoted/covered and
one (idna) flips to adopt. The remaining clean mirrors from the 30 all carry
weak theses — so the top-5 comes from new names.

## 2. New names, same evidence bar

Downloads MEASURED from snapshot 2026-10-01; surface LOC MEASURED from sdist
2026-10-14 (shipped paths only); verdict curated-judged 2026-10-14.

| Package | dl/mo | py files / LOC | native | Verdict / gap | Standing |
|---|---|---|---|---|---|
| rich | 453.0M | 100 / 38,515 | 0 | partial (ratatui: app-framework, not a console API) | NOMINATED #1 |
| openpyxl | 282.0M | 191 / 29,237 | 0 | partial (calamine: read-only) | NOMINATED #2 |
| sympy | 129.6M | 857 / 496,925 | 0 | partial (symcas: primitive) | NOMINATED #3 |
| sqlglot | 72.1M | 186 / 83,414 | 0 | partial (sqlparser: ANSI-only, no transpile/optimizer) | NOMINATED #4 |
| docutils | 139.0M | 313 / 94,899 | 0 | partial (rst/oak-rst: nascent, no pipeline) | NOMINATED #5 |
| tabulate | 153.4M | 10 / 8,620 | 0 | partial (comfy-table: different API) | next-up — real CPU, small absolute surface |
| babel | 104.0M | 34 / 14,647 | 0 | partial (icu/ICU4X: different API) | next-up — formatting CPU real, CLDR data bulk |
| dateparser | 38.6M | 253 / 58,358 | 0 | partial (dateparser crate: narrower scope) | next-up — biggest surface per download |
| sqlparse | 109.8M | 26 / 4,777 | 0 | partial (sqlparser) | next-up, small |
| markdown | 93.1M | 36 / 9,238 | 0 | partial (pulldown-cmark/comrak) | next-up, small |
| mistune | 61.0M | 41 / 6,662 | 0 | partial (comrak/pulldown-cmark) | next-up, small |
| marshmallow | 86.5M | 14 / 5,052 | 0 | partial (serde: code-first, different model) | next-up, small |
| arrow | 60.2M | 11 / 10,465 | 0 | partial (jiff: no fuzzy/humanize) | next-up, small |
| soupsieve | 298.3M | 9 / 4,795 | 0 | partial (selectors: engine only) | not nominated — 4.8K LOC caps the absolute win |
| sortedcontainers | 212.3M | 5 / 4,317 | 0 | partial (std BTreeMap: different API) | not nominated — 4.3K LOC caps the win |
| tomlkit | 452.4M | 12 / 6,187 | 0 | partial (toml_edit: format-preserving already) | not nominated — gap too close, surface small |
| tomli | 274.1M | 4 / 938 | 0 | full (toml crate) | adopt — bar cuts both ways |
| xlsxwriter | 90.7M | 43 / 26,635 | 0 | full (rust_xlsxwriter, same lineage) | adopt — bar cuts both ways |
| rsa | 192.1M | 14 / 2,805 | 0 | full (RustCrypto rsa) | adopt — bar cuts both ways |
| feedparser | 17.2M | — (ESTIMATE: small) | — | full (feed-rs) | adopt — bar cuts both ways |
| hypothesis | 52.4M | 106 / 46,273 | 4 + maturin | upstream_only — already ships a Rust core | upstream-only |
| orjson | 180.5M | 36 / 9,679 | 914 + maturin | upstream_only — already a Rust core | upstream-only |
| pendulum | 53.5M | 120 / 13,862 | 12 + build | upstream_only — v3 ships a native core | upstream-only |
| regex | 309.9M | — | C ext + build | upstream_only — ships `_regex` C extension | upstream-only |
| markupsafe | 614.6M | 3 / 496 | 1 + build | upstream_only — ships C speedups | upstream-only |
| cloudpickle | 169.6M | 3 / 1,584 | 0 | shared_engine/demote — dispatch over C pickle | demoted |
| dill | 155.1M | 16 / 6,491 | 0 | shared_engine/demote — extends C pickle | demoted |
| httpx | 616.8M | 23 / 8,828 | 0 | io_bound/demote — HTTP client | demoted |
| fsspec | 563.2M | 50 / 21,079 | 0 | io_bound/demote — FS abstraction over remote backends | demoted |
| yt-dlp | 12.7M | — (not counted; lesson source) | — | high_churn/exclude — network mainline + extractor churn | EXCLUDED |

## 3. Top-5 nominations

**#1 rich (453.0M dl/mo, 38.5K LOC, 0 native).** The CPU win lives in
`Console` render: markup parsing, Unicode-width text wrapping/folding, and
table/grid layout measurement plus per-segment ANSI generation — tight
pure-Python loops executed on every print of styled output, where writes are
O(rendered bytes) so compute dominates for tables, markdown, and tracebacks
(terminal/pipe sinks add no wait). The thesis explicitly scopes out the
`Syntax` path (pygments-backed — shared-engine tie per filter 5) to the
layout/markup/table kernels. No filter kills it: no network or disk wait on
the mainline, a stable renderable surface (no churn), not AWS-owned, zero
native code, the hot path is Python layout math rather than a C engine, and
no prior saturation attempts exist. Gap: ratatui (59M dl) is an
app-framework TUI, not a console/renderable print API.

**#2 openpyxl (282.0M dl/mo, 29.2K LOC, 0 shipped native).** The CPU win
lives in the XML→model→XML layer: worksheet `_reader` (etree walk into the
cell/style model) and write serialization, both pure Python in the default
PyPI wheels — measured 0 shipped native files, and the `--with-cython`
`_reader`/`_writer` path is opt-in, not what users install. The thesis
honestly excludes the zip layer (C zlib decompress ties with itself — a
filter-5 slice) and claims only the XML modeling above it. No filter kills
it: input files are local (no network mainline), OOXML is a stable format
(no churn), not AWS-owned, no default native core, the engine is an
etree-walk in Python rather than a C matcher, and the hot path was never
saturated by measurement. Gap: calamine (15M dl) is read-only — the write
path plus style/chart/pivot model is uncovered.

**#3 sympy (129.6M dl/mo, 496.9K LOC, 0 native).** The CPU win lives in the
symbolic kernels — polys domains, simplify, solvers — pure-Python algebra
over the largest CPU surface measured in this round (857 files), where SymPy
is famously orders of magnitude slower than compiled CAS systems and every
hot loop is interpreter-executed. The thesis is kernel-scoped (polys/simplify
core probe first), not an 857-file mirror bid. No filter kills it: pure
compute with no I/O mainline, a stable mathematical surface (no churn), not
AWS-owned, zero native code, no C engine underneath, no prior attempts.
Gap: symcas/savage_core are primitive (dozens of downloads) — no mature
equivalent exists.

**#4 sqlglot (72.1M dl/mo, 83.4K LOC, 0 native).** The CPU win lives in
dialect parsing plus AST build plus transpile/optimizer passes — a
concentrated programmatic workload (no human-latency masking, no I/O wait:
strings in, strings out) across 186 files of tokenizer/parser/generator
Python. Lower downloads than the others are offset by CPU density per call
and a clean gap. No filter kills it: no network mainline, versioned
grammars rather than nightly-breakage churn, not AWS-owned, zero native
code, the engine is Python table-driven parsing rather than a C library,
and the path was never saturated. Gap: sqlparser (83M dl) is ANSI
lexer/parser only with no dialect transpile and no optimizer;
sqlglot-rust (4.5K dl) is nascent.

**#5 docutils (139.0M dl/mo, 94.9K LOC, 0 native).** The CPU win lives in
the reStructuredText state-machine parser plus the transform/publishing
pipeline — 313 files of pure-Python text processing forming Sphinx's
backbone, executed on every documentation build over large trees. No filter
kills it: file-local compute with no network mainline, a frozen spec
surface (rst changes on decade timescales — the opposite of churn), not
AWS-owned, zero native code, no C engine, no prior attempts. Gap: the `rst`
and `oak-rst` crates are nascent parsers with no transform pipeline or
document model.

## 4. Honesty notes

- New-name LOC counts are my own sdist measurements (same shipped-paths
  rule as the finder); keyword/payoff scoring was not recomputed for new
  names, so ordering among §2 rows is judgment, not a score.
- feedparser LOC is ESTIMATE (uncounted); its adopt verdict does not depend
  on surface size.
- jiff/icu/serde/rust_xlsxwriter/toml-crate maturity claims rest on crates.io
  download counts observed 2026-10-14 (224M/2.5M/—/5.2M/1B), not on API audits.
- packaging and python-dateutil stay the strongest clean-mirror theses from
  the old 30 on paper, but both are COVERED (ported + upstream verdicts
  with proof) and are therefore not renominated.
