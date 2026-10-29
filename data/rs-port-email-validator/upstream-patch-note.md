# Upstream patch note — email-validator pure-Python speedups (not sent upstream)

Suggested recipient: JoshData/python-email-validator. No upstream PR opened
(lane policy). Patch: `data/rs-port-email-validator/tier1.patch` against
2.3.0. All changes preserve exact error messages and normalized forms
(verified: upstream suite 306/306 both arms + 468-row differential, 0 mismatches).

Measured (Ryzen 9 5950X, CPython 3.12.3, `bench_tier1.py`, min-of-7,
`check_deliverability=False`):

| Track | Stock | Patched | Speedup |
|---|---|---|---|
| mixed-2000 (representative) | 112.8 ms | 35.3 ms | 3.20x |
| ascii-valid-5000 | 296.2 ms | 46.1 ms | 6.43x |
| idn-1000 | 104.0 ms | 106.1 ms | 0.98x (unchanged, idna-bound) |

What the patch does (`syntax.py`, `validate_email.py` only):

1. **Skip the `idna.decode`/`idna.encode` roundtrip for pure-ASCII domains
   without a Punycode label** (`validate_email_domain_name`). For such
   domains `idna.decode` is just casefolding (`== str.lower` on ASCII,
   verified), and re-encoding lowercase ASCII always succeeds after the
   syntax checks above — so return `{"ascii_domain": ascii_domain,
   "domain": ascii_domain.lower()}` directly. Isolated effect: 2.03x on
   mixed, 4.04x on ascii-valid.
2. **Vectorize per-character membership tests**: the `{... for c in s if
   not RE.match(c)}` set comprehensions (domain pre/post-normalization
   checks, local-part ATEXT/QTEXT checks) become single C-speed
   `negated-class findall` calls producing the identical bad-char set.
   Same for `check_unsafe_chars`: ASCII-only fast path via one regex scan
   (ASCII rejects exactly controls/DEL/space — verified exhaustively over
   `range(0x80)`); non-ASCII keeps the original loop.
3. **Hoist per-call work out of the hot path (P6)**: `from . import ...`
   default resolution in `validate_email` and `SPECIAL_USE_DOMAIN_NAMES`
   in `validate_email_domain_name` are cached in module globals after the
   first call (module-top import would be circular); the inline
   `re.match(r"(?!xn)..--", ...)` per label is a module-level compiled
   pattern. Isolated effect: 1.17x mixed / 1.32x ascii.
4. **ASCII fast path in `split_string_at_unquoted_special`**: skip the
   per-slice `unicodedata.normalize("NFC", text[i:])` combining check when
   `text.isascii()` (ASCII is NFC-stable, so the check is always false).
   Non-ASCII keeps the original loop — combining-sequence behavior
   (e.g. `<` + U+0338) unchanged, covered by differential cases.

Suggested upstream shape: the maintainers may prefer (1) guarded by a
comment citing the `decode == lower` ASCII property, or may reject it as
too clever — (2)–(4) are uncontroversial mechanical wins worth taking alone.
