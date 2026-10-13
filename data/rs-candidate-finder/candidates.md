# Rewrite candidates

Source: ClickHouse (`last_update: 2026-10-01 12:40:51`, 15000 rows; 30 classified).

Paths: **(a) adopt/wrap** — a Rust alternative exists, adopt it; **(b) mirror** — no alternative, ranked by estimated payoff; **(c) upstream** — the win belongs inside the original package.

Evidence labels: bare claims are MEASURED (download counts, sdist file layout, curated-table verdicts dated 2026-10-13); anything marked ESTIMATE is a heuristic (keyword hotness, payoff score, upstream-fit judgment, unjudged crates.io hits).

| Dl-rank | Package | dl/mo | Dependents | Path | Alternative / gap | Why |
|---|---|---|---|---|---|---|
| 17 | botocore | 853M | 979 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [botocore_parser](https://crates.io/crates/botocore_parser), [lava-api-forge](https://crates.io/crates/lava-api-forge), [cloudformation-validate-data-source](https://crates.io/crates/cloudformation-validate-data-source) | No full Rust equivalent; 35677 LOC pure-Python surface, keywords: compress, crypt, csv, date, json, lex, mark, pars, serial, token, url, valid (payoff ESTIMATE, ranked below). payoff #1 (11094.6) |
| 12 | pygments | 932M | 1.8K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [pygmentize](https://crates.io/crates/pygmentize), [paint](https://crates.io/crates/paint), [codevideorenderer](https://crates.io/crates/codevideorenderer) | No full Rust equivalent; 130572 LOC pure-Python surface, keywords: date, html, json, lex, mark, pars, regex, scann, token, yaml (payoff ESTIMATE, ranked below). payoff #2 (10247.4) |
| 2 | packaging | 1.55B | 4.6K | (b) mirror | no full replacement; nearest: [pep440_rs](https://crates.io/crates/pep440_rs) (versions only) / [pep508_rs](https://crates.io/crates/pep508_rs) (markers only) / [uv-pep440](https://crates.io/crates/uv-pep440) (unstable internal crate, no stable full API) | No full Rust equivalent; 12715 LOC pure-Python surface, keywords: mark, pars, token, url, version (payoff ESTIMATE, ranked below). payoff #3 (9302.6) |
| 10 | setuptools | 973M | 7.5K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [pyo3-setup](https://crates.io/crates/pyo3-setup), [fetter](https://crates.io/crates/fetter), [pyo3](https://crates.io/crates/pyo3) | No full Rust equivalent; 60327 LOC pure-Python surface, keywords: date, json, mark, pars, token, valid, version, xml (payoff ESTIMATE, ranked below). payoff #4 (8761.4) |
| 14 | python-dateutil | 905M | 6.1K | (b) mirror | no full replacement; nearest: [chrono](https://crates.io/crates/chrono) (strict parsing only, no fuzzy parse, no rrule) / [dateparser](https://crates.io/crates/dateparser) (no recurrence rules) / [jiff](https://crates.io/crates/jiff) (no recurrence rules) | No full Rust equivalent; 7821 LOC pure-Python surface, keywords: date, json, pars, valid, version, yaml (payoff ESTIMATE, ranked below). payoff #5 (6334.4) |
| 24 | pytest | 780M | 26.0K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [pytest-language-server](https://crates.io/crates/pytest-language-server), [offload](https://crates.io/crates/offload), [cartoon](https://crates.io/crates/cartoon) | No full Rust equivalent; 38781 LOC pure-Python surface, keywords: date, mark, pars, version, xml, yaml (payoff ESTIMATE, ranked below). payoff #6 (5461.1) |
| 1 | boto3 | 2.46B | 3.9K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [cubby](https://crates.io/crates/cubby), [rs3gw](https://crates.io/crates/rs3gw), [s3-endpoint](https://crates.io/crates/s3-endpoint) | No full Rust equivalent; 6847 LOC pure-Python surface, keywords: json (payoff ESTIMATE, ranked below). payoff #7 (4921.4) |
| 19 | pycparser | 838M | 746 | (b) mirror | no curated verdict and no search performed | No full Rust equivalent; 5818 LOC pure-Python surface, keywords: lex, pars, regex, version (payoff ESTIMATE, ranked below). payoff #8 (4188.8) |
| 5 | urllib3 | 1.32B | 5.8K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [uv-sbom](https://crates.io/crates/uv-sbom), [url_jail](https://crates.io/crates/url_jail), [fetter](https://crates.io/crates/fetter) | No full Rust equivalent; 13595 LOC pure-Python surface, keywords: url, version (payoff ESTIMATE, ranked below). payoff #9 (3972.1) |
| 29 | attrs | 692M | 2.9K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [attrs](https://crates.io/crates/attrs), [imagequant-sys](https://crates.io/crates/imagequant-sys), [custom_attrs](https://crates.io/crates/custom_attrs) | No full Rust equivalent; 7182 LOC pure-Python surface, keywords: mark, valid, version, yaml (payoff ESTIMATE, ranked below). payoff #10 (3461.6) |
| 22 | anyio | 803M | 795 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [pyo3-anyio](https://crates.io/crates/pyo3-anyio), [uv-sbom](https://crates.io/crates/uv-sbom), [astral-pubgrub](https://crates.io/crates/astral-pubgrub) | No full Rust equivalent; 16390 LOC pure-Python surface, keywords: json, version, yaml (payoff ESTIMATE, ranked below). payoff #11 (3213.4) |
| 16 | pluggy | 878M | 779 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [pluggy](https://crates.io/crates/pluggy), [pluggy-rs](https://crates.io/crates/pluggy-rs), [ruff-tools](https://crates.io/crates/ruff-tools) | No full Rust equivalent; 1753 LOC pure-Python surface, keywords: version, yaml (payoff ESTIMATE, ranked below). payoff #12 (2633.1) |
| 7 | requests | 1.21B | 32.0K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [requests](https://crates.io/crates/requests), [rmcp](https://crates.io/crates/rmcp), [rmcp-macros](https://crates.io/crates/rmcp-macros) | No full Rust equivalent; 6394 LOC pure-Python surface, keywords: version (payoff ESTIMATE, ranked below). payoff #13 (2416.1) |
| 20 | click | 817M | 10.4K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [click](https://crates.io/crates/click), [egui-elegance](https://crates.io/crates/egui-elegance), [gitpane](https://crates.io/crates/gitpane) | No full Rust equivalent; 12674 LOC pure-Python surface, keywords: pars (payoff ESTIMATE, ranked below). payoff #14 (1634.5) |
| 26 | annotated-types | 724M | 285 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [uv-sbom](https://crates.io/crates/uv-sbom) | No full Rust equivalent; 562 LOC pure-Python surface, keywords: yaml (payoff ESTIMATE, ranked below). payoff #15 (1448.9) |
| 27 | h11 | 711M | 389 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [h11](https://crates.io/crates/h11), [rust-h11](https://crates.io/crates/rust-h11), [h11r](https://crates.io/crates/h11r) | No full Rust equivalent; 2654 LOC pure-Python surface, keywords: version (payoff ESTIMATE, ranked below). payoff #16 (1421.7) |
| 3 | typing-extensions | 1.40B | 7.5K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [chia](https://crates.io/crates/chia), [hyfetch](https://crates.io/crates/hyfetch), [chik](https://crates.io/crates/chik) | No full Rust equivalent; 14213 LOC pure-Python surface, no hot keywords (payoff ESTIMATE, ranked below). payoff #17 (1396.9) |
| 4 | idna | 1.34B | 2.9K | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [idna](https://crates.io/crates/idna), [idna_adapter](https://crates.io/crates/idna_adapter), [idna-cli](https://crates.io/crates/idna-cli) | No full Rust equivalent; 20422 LOC pure-Python surface, no hot keywords (payoff ESTIMATE, ranked below). payoff #18 (1339.5) |
| 30 | s3transfer | 645M | 263 | (b) mirror | no curated verdict; nearest crates.io hits (observed, UNJUDGED — a human must confirm before (a)): [aerosync](https://crates.io/crates/aerosync) | No full Rust equivalent; 9066 LOC pure-Python surface, no hot keywords (payoff ESTIMATE, ranked below). payoff #19 (645.0) |
| 6 | certifi | 1.29B | 3.9K | (a) adopt/wrap | none needed — CA-bundle data package; nothing to compile or port | Nothing to port (CA-bundle data package; nothing to compile or port); adopt = do nothing. — |
| 8 | charset-normalizer | 1.19B | 2.5K | (c) upstream | no mirror alternative sought — upstream already builds native code | Upstream already builds 4 native files with 6685 LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package. — |
| 9 | cryptography | 1.07B | 3.4K | (c) upstream | no mirror alternative sought — upstream already builds native code | Upstream already builds 103 native files with 16265 LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package. — |
| 11 | cffi | 950M | 1.2K | (c) upstream | no mirror alternative sought — upstream already builds native code | Upstream already builds 35 native files with 16054 LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package. — |
| 13 | pyyaml | 911M | 8.6K | (c) upstream | no mirror alternative sought — upstream already builds native code | Upstream already builds 4 native files with 6354 LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package. — |
| 15 | six | 895M | 6.6K | (a) adopt/wrap | none needed — single-file Python 2/3 compat shim, shrinking; nothing worth porting | Nothing to port (single-file Python 2/3 compat shim, shrinking; nothing worth porting); adopt = do nothing. — |
| 18 | pydantic | 843M | 9.8K | (c) upstream | no mirror alternative sought — native core already exists upstream | validation core already Rust upstream; remaining Python is API sugar — win belongs upstream, not in a fork. — |
| 21 | pydantic-core | 809M | 0 | (c) upstream | no mirror alternative sought — upstream already builds native code | Upstream already builds 141 native files with 12138 LOC Python around them (ESTIMATE: fit); a PyO3/maturin speed core belongs inside the original package. — |
| 23 | numpy | 802M | 36.1K | (c) upstream | no mirror alternative sought — native core already exists upstream | already a C/Fortran core — win belongs upstream, not in a fork. — |
| 25 | iniconfig | 756M | 501 | (a) adopt/wrap | none needed — trivial INI parsing, I/O-bound; nothing worth porting | Nothing to port (trivial INI parsing, I/O-bound; nothing worth porting); adopt = do nothing. — |
| 28 | typing-inspection | 695M | 0 | (a) adopt/wrap | none needed — thin typing dispatch; nothing to port | Nothing to port (thin typing dispatch; nothing to port); adopt = do nothing. — |

## Evidence notes

### botocore (dl-rank #17)

- download rank #17 (853M)
- 979 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 60 .py files, 35677 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): compress, crypt, csv, date, json, lex, mark, pars, serial, token, url, valid

### pygments (dl-rank #12)

- download rank #12 (932M)
- 1834 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 358 .py files, 130572 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): date, html, json, lex, mark, pars, regex, scann, token, yaml

### packaging (dl-rank #2)

- download rank #2 (1.55B)
- 4628 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): partial/partial/partial
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 22 .py files, 12715 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): mark, pars, token, url, version

### setuptools (dl-rank #10)

- download rank #10 (973M)
- 7488 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 231 .py files, 60327 LOC Python, 1 native files
- workload keywords observed (ESTIMATE of hotness): date, json, mark, pars, token, valid, version, xml
- no native-build declaration in sdist build files (measured): C files read as data, not as a built extension

### python-dateutil (dl-rank #14)

- download rank #14 (905M)
- 6070 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): partial/partial/partial
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 21 .py files, 7821 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): date, json, pars, valid, version, yaml

### pytest (dl-rank #24)

- download rank #24 (780M)
- 25992 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 93 .py files, 38781 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): date, mark, pars, version, xml, yaml

### boto3 (dl-rank #1)

- download rank #1 (2.46B)
- 3938 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 27 .py files, 6847 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): json

### pycparser (dl-rank #19)

- download rank #19 (838M)
- 746 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 8 .py files, 5818 LOC Python, 130 native files
- workload keywords observed (ESTIMATE of hotness): lex, pars, regex, version
- no native-build declaration in sdist build files (measured): C files read as data, not as a built extension

### urllib3 (dl-rank #5)

- download rank #5 (1.32B)
- 5751 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 42 .py files, 13595 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): url, version

### attrs (dl-rank #29)

- download rank #29 (692M)
- 2857 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 22 .py files, 7182 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): mark, valid, version, yaml

### anyio (dl-rank #22)

- download rank #22 (803M)
- 795 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 46 .py files, 16390 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): json, version, yaml

### pluggy (dl-rank #16)

- download rank #16 (878M)
- 779 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 10 .py files, 1753 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): version, yaml

### requests (dl-rank #7)

- download rank #7 (1.21B)
- 32025 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 20 .py files, 6394 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): version

### click (dl-rank #20)

- download rank #20 (817M)
- 10358 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 17 .py files, 12674 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): pars

### annotated-types (dl-rank #26)

- download rank #26 (724M)
- 285 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 2 .py files, 562 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): yaml

### h11 (dl-rank #27)

- download rank #27 (711M)
- 389 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 13 .py files, 2654 LOC Python, 0 native files
- workload keywords observed (ESTIMATE of hotness): version

### typing-extensions (dl-rank #3)

- download rank #3 (1.40B)
- 7473 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 3 .py files, 14213 LOC Python, 0 native files

### idna (dl-rank #4)

- download rank #4 (1.34B)
- 2888 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 10 .py files, 20422 LOC Python, 0 native files

### s3transfer (dl-rank #30)

- download rank #30 (645M)
- 263 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 17 .py files, 9066 LOC Python, 0 native files

### certifi (dl-rank #6)

- download rank #6 (1.29B)
- 3902 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): noop

### charset-normalizer (dl-rank #8)

- download rank #8 (1.19B)
- 2462 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): partial/partial/partial
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 16 .py files, 6685 LOC Python, 4 native files
- workload keywords observed (ESTIMATE of hotness): charset, version
- build corroboration measured: sdist build files declare native compilation

### cryptography (dl-rank #9)

- download rank #9 (1.07B)
- 3396 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 106 .py files, 16265 LOC Python, 103 native files
- workload keywords observed (ESTIMATE of hotness): crypt, decod, encod, hash, pars, serial
- build corroboration measured: sdist build files declare native compilation

### cffi (dl-rank #11)

- download rank #11 (950M)
- 1188 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 56 .py files, 16054 LOC Python, 35 native files
- workload keywords observed (ESTIMATE of hotness): csv, pars
- build corroboration measured: sdist build files declare native compilation

### pyyaml (dl-rank #13)

- download rank #13 (911M)
- 8566 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): partial/partial
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 21 .py files, 6354 LOC Python, 4 native files
- workload keywords observed (ESTIMATE of hotness): pars, scann, serial, token, yaml
- build corroboration measured: sdist build files declare native compilation

### six (dl-rank #15)

- download rank #15 (895M)
- 6588 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): noop

### pydantic (dl-rank #18)

- download rank #18 (843M)
- 9821 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): upstream

### pydantic-core (dl-rank #21)

- download rank #21 (809M)
- 0 dependent packages (ecosyste.ms, as observed)
- sdist layout measured (shipped paths only, tests/examples/docs excluded): 3 .py files, 12138 LOC Python, 141 native files
- workload keywords observed (ESTIMATE of hotness): date, json, lex, mark, serial, url, valid
- build corroboration measured: sdist build files declare native compilation

### numpy (dl-rank #23)

- download rank #23 (802M)
- 36144 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): upstream

### iniconfig (dl-rank #25)

- download rank #25 (756M)
- 501 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): noop

### typing-inspection (dl-rank #28)

- download rank #28 (695M)
- 0 dependent packages (ecosyste.ms, as observed)
- curated table (measured 2026-10-13): noop

