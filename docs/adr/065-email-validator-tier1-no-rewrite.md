# ADR-065: email-validator tier-1 measured, no Rust rewrite (no_gain)

Date: 2026-10-29. Lane: rs-port-email-validator (brief: rerank-3 §2 rank 5,
160M dl/mo, headroom 0.70 rewrite). Outcome: **no_gain with numbers** — same
disposition class as arrow (ADR-059), pyasn1 (ADR-060), mako (ADR-064).

## Frozen workload (lane `WORKLOAD.md`)

`validate_email(addr, check_deliverability=False)` per address in a plain
loop (deliverability/network excluded by construction). Representative
`mixed-2000`: 60% valid ASCII, 10% valid IDN, 8% quoted-local, 22% invalid
(no-@, bad chars, leading/trailing/dot-dot, overlong, special-use, dotless,
overlong label); showcase `ascii-valid-5000` and `idn-1000`; overhead
`x1-valid/invalid/idn`. Deterministic seed 20261029. Box: Ryzen 9 5950X,
CPython 3.12.3, email-validator 2.3.0, idna 3.20.

## Tier-1 (pure Python, upstream-plausible; suite-identical)

Patch: lane `tier1.patch` (applies to installed 2.3.0;
`bench_tier1.py` applies it to a temp copy itself). Upstream-ready note in
`upstream-patch-note.md` (no upstream PR opened, lane policy).

- **T-idna** `validate_email_domain_name`: skip the `idna.decode` +
  `idna.encode` roundtrip for pure-ASCII domains without a Punycode label
  (`idna.decode` is casefolding there, `== str.lower` on ASCII — verified;
  re-encode of lowercase ASCII always succeeds after the syntax checks).
  Isolated: 2.03x mixed / 4.04x ascii (ablation: tier-1 minus this change).
- **T-vec** (P3): per-character `{... for c in s if not RE.match(c)}` set
  comprehensions → single C-speed negated-class `findall` (identical bad
  sets); `check_unsafe_chars` ASCII fast path (ASCII rejects exactly
  controls/DEL/space — verified exhaustively over `range(0x80)`).
- **T-cache** (P6): per-call `from . import ...` defaults and
  `SPECIAL_USE_DOMAIN_NAMES` cached in module globals after first call
  (module-top import would be circular); inline
  `re.match(r"(?!xn)..--", ...)` per label precompiled. Isolated: 1.17x
  mixed / 1.32x ascii.
- **T-split** (P3): skip the per-slice `unicodedata.normalize("NFC",
  text[i:])` combining check in `split_string_at_unquoted_special` when
  `text.isascii()` (ASCII is NFC-stable; non-ASCII keeps the original
  loop, combining-sequence cases covered differentially).

Measured (`bench_tier1.py`, min-of-7, worker self-RSS):

| Track | Stock | Tier-1 | Speedup |
|---|---|---|---|
| mixed-2000 (representative) | 112.8 ms | 35.3 ms | **3.20x** |
| ascii-valid-5000 | 296.2 ms | 46.1 ms | 6.43x |
| idn-1000 | 104.0 ms | 106.1 ms | 0.98x (idna-bound, unchanged) |
| x1-valid / x1-invalid / x1-idn | 65 / 8 / 96 µs | 13 / 4 / 140 µs | — |

Peak RSS: stock 14.5–16.1 MB, tier-1 16.0–17.0 MB (delta is worker-harness
temp-copy import, not a library regression).

Equivalence: upstream suite (`tests/` from the 2.3.0 sdist, incl. mocked
deliverability) **306 passed stock, 306 passed tier-1**; differential
corpus 52 addresses × 9 option sets = **468/468 identical** incl. exact
exception types/messages and normalized forms (combining-sequence
adversarial cases included).

## Ceiling recomputed against tier-1 (kill math)

`ceiling.py`: warmed cProfile attribution of 5× mixed-2000 on the tier-1
arm, `tottime` by module group: email_validator-own 39.7%, idna-shared
16.7%, rest (C `re`, `unicodedata.normalize`, builtins) 43.6%.
Infinite-speedup ceiling of an own-code-only kernel: **1.66x < 2x: KILL**
(brief §1 kill criterion met on the representative batch).

Second kill condition independently met: 60.3% of remaining profiled CPU
is idna/stdlib code outside email-validator. The idn-1000 track (0.98x
tier-1) is structurally idna-bound: a faithful port must either keep
UTS-46/IDNA behind the seam (then the kernel's remaining work is already
C-speed checks + one sub-µs-toll crossing per ~20–50 µs address — no room)
or reimplement IDNA in Rust (reimplementing the shared `idna` dependency,
a different package, while the gate demands byte-exact `idna` error
strings inside messages — same trap class as the pygments C-`re` receipt).

Analyzed but declined: batch-crossing kernel (P10 — per-address cost is
already 18 µs, crossing floor is not the binding constraint); absorbing
UTS-46 into the kernel (scope creep into `idna`, message-parity risk).

## Disposition

No Rust port: the tier-1 fix (3.20x representative, upstream-plausible)
removed the rewrite case, exactly the arrow/pyasn1/mako pattern. Port repo
`John-Cusack/email-validator-rust` holds license/notice/outcome only.
No registry publishes (`blocked: no registry credentials` recorded in PR).
