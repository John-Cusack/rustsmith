# Pattern review — rs-port-email-validator (no_gain, tier-1 only)

Date: 2026-10-29. Lane: rs-port-email-validator. Outcome: tier-1 3.20x
representative / 6.43x ascii, Rust rewrite declined (ceiling 1.66x < 2x,
ADR-065). This review proposes guide updates from the measured evidence.
Evidence: `data/rs-port-email-validator/` (`bench_tier1_results.json`,
`ceiling.py` output, `equiv_results.json`, `tier1.patch`); rerank probe
`data/rs-rerank-3/probes/emailvalidator.json`.

## Findings (mechanism + measured row)

1. **P6 Confirm — "compile/resolve once" covers per-call module imports.**
   Mechanism: `validate_email` resolved 10 default names via
   `from . import ...` per call (import-system fromlist handling,
   `_handle_fromlist` visible in profiles); `validate_email_domain_name`
   re-imported `SPECIAL_USE_DOMAIN_NAMES` per call; `re.match(r"(?!xn)..--",
   ...)` recompiled per label per address (cache-hit compile, still
   per-call overhead). Fix: lazy module-global cache + one module-level
   compiled pattern (module-top import would be circular — the cache
   placement is the technique). Isolated by ablation (tier-1 minus only
   these three changes): **1.17x mixed-2000 / 1.32x ascii-valid** over the
   P6-less arm. Add email-validator to P6's evidence list (confidence
   already High).
2. **P3 Confirm — hoist the invariant branch, bulk pre-transform.**
   Mechanism: per-character Python loops (`{... for c in s if not
   RE.match(c)}`, per-char `unicodedata.category`, per-slice
   `unicodedata.normalize("NFC", text[i:])` — the last O(n²)) replaced by
   one `isascii()` branch + single C-speed scans (negated-class
   `findall`, ASCII unsafe-char regex). Non-P6 tier-1 combined (stock →
   no-skip arm): 1.47x mixed; minus isolated P6 1.17x leaves ~1.25x to the
   P3 shape (multiplicative-independence assumption, stated as approximate).
   Same branch-hoist discipline as crc `translate` and packaging's fast
   lane, now with an O(n²)-normalize receipt. Confidence stays High.
3. **P3 Extend — skip identity normalization roundtrips (new receipt).**
   Mechanism: `validate_email_domain_name` called `idna.decode` +
   `idna.encode` unconditionally; for pure-ASCII domains without a Punycode
   label decode is casefolding (`== str.lower` on ASCII, verified) and
   re-encode trivially succeeds — both roundtrips deleted for that input
   class. Isolated by ablation: **2.03x mixed / 4.04x ascii** — the dominant
   tier-1 mechanism. Proposed guide edit: extend P3 with an "identity
   roundtrip" clause (input-class check that deletes whole library
   calls, not just loop branches), citing packaging's digit-dot fast lane
   as the sibling receipt.
4. **P11 Confirm — probe headroom first, walk away (no_gain receipt).**
   Mechanism: after tier-1, warmed cProfile attribution of mixed-2000:
   email_validator-own 39.7%, idna-shared 16.7%, rest (C `re`,
   `unicodedata`, builtins) 43.6% → infinite-speedup ceiling of an
   own-code-only kernel **1.66x < 2x** (`ceiling.py`). The idn-1000 track
   is structurally idna-bound (tier-1 0.98x). A faithful port must keep
   UTS-46/IDNA behind the seam (remaining work already C-speed) or
   reimplement `idna` (different package + byte-exact `idna` error strings
   in messages). This is the rerank's idna-shared-workload caveat,
   confirmed with numbers. Proposed ledger row: "Porting a validator whose
   remaining cost sits in a shared Unicode/IDNA dependency — email-validator
   ceiling 1.66x / idn 0.98x (ADR-065); do instead: tier-1 only."

## Losers / non-applicable

- P10 batching: not applicable — per-address cost is already ~18 µs with
  one call per address; no per-item crossing inside the hot path to amortize.
- P1/P4/P8 (lexer shapes): no lexer to rewrite; the char loops were P3
  vectorizations, not parser ports.
- No IDN-track win exists at any tier: 0.98x is a finding (shared-dependency
  bound), not a failure to optimize.

## Review checklist

- Evidence integrity: bench rows min-of-7 fresh-subprocess wall + worker
  self-RSS both arms (`bench_tier1_results.json`); ceiling from warmed
  5× cProfile attribution (`ceiling.py` output in ADR-065); upstream suite
  306/306 both arms + 468/468 differential incl. messages
  (`equiv_results.json`); no cross-box comparison (all rows Ryzen 9 5950X,
  same day). Missing: none for a no_gain lane (no port → no X1 toll rows
  needed; x1 rows recorded for the crossing-floor record).
- Mechanism, not vibes: per-item costs named (per-char match/category/
  normalize slice, per-call import/fromlist, per-label recompile,
  unconditional idna roundtrips) with before/after excerpts in
  `tier1.patch`. Crossings: n/a (no port; per-address call cost 18 µs
  recorded). Algorithm class unchanged (same validation, less work).
- Guide bookkeeping: 2× Confirm (P6, P3), 1× Extend (P3 identity-roundtrip
  clause), 1× Confirm P11 + ledger receipt. No confidence changes (all
  targets already High). No oracle/gate touched.
- Proposed diff (minimal, mako-#138-sized; full wording above is the
  proposal record): anti-pattern ledger +1 row (finding 4). P3 extension
  wording and P6/P3 evidence-list additions are offered to the guide PR
  that consumes this lane, not snuck into the lane PR.
