# ADR-024: single-pass byte lexer with borrowed tokens (dateutil V8/V10)

Date: 2026-10-07
Status: accepted

## Context

Re-audit §V8/V10 (`data/rs-dateutil-reaudit/report.md`): the parser lexer
copied the input 3+ times — binding `String` (copy 1),
`Timelex::new: s.chars().collect()` into `Vec<char>` (copy 2, 4x
expansion), per-token `String` growth via `split` → `Vec<String>` (copy
3+) — and `parse.rs` re-scanned tokens with `chars().count()` (one inside
the `Num`-state step) plus `Vec<char>` slicing. The `isoparser` core
already showed the target shape (byte slices throughout).

## Decision

- `lex.rs`: `Timelex<'a>` borrows the input; the machine walks `&[u8]`
  once. ASCII bytes classify through a `const [u8; 256]` table
  (`build_class`, ASCII-exact vs `is_word`/`is_num`/`is_space` including
  `\x1c..=\x1f` spaces); bytes `>= 0x80` fall back to `char` decode +
  the same classes in the original check order.
- Tokens are `Token<'a>` (borrow + `nchars`/`ascii` consumed counters).
  Only the original's two eager normalizations materialize: `,` -> `.`
  folding (`"12,5"` -> `"12.5"`) and `\x00` filtering (`"a\x00b"` ->
  `"ab"`). Dotted-split overflow queues byte spans (separators are ASCII,
  so sub-spans stay boundary-aligned); `last_byte`/`dots` counters
  reproduce the `ends_with` / dot-count checks exactly, including
  trailing-NUL inputs (NULs are never collected).
- `parse.rs`: all `chars().count()` length checks read `Token::nchars()`;
  `Vec<char>` slicing is byte slicing (`slice_folded` on fold output,
  ASCII by construction after `Dec::parse`; `token_slice` uses the
  lexer `ascii` flag with a char-walk fallback). `all_upper` is a byte
  scan (exact: non-ASCII bytes are never ASCII-uppercase).
  `token_is_digit`/`probe_float` take an ASCII byte path with non-ASCII
  fallback; `fold_digits` returns `Cow` (borrows ASCII).
- Binding keeps the single owned `String` alive; `Timelex::split(&s)`
  borrows it through `parse_tokens(&mut [Token])`. `TimelexObj`
  (deprecated `_timelex` surface) owns `text: String` + lifetime-free
  `LexCursor`, materializing `String`s only at that compat boundary.
- `recombine_skipped` is generic over `AsRef<str>` (unchanged output).

## Consequences

- Differential: 6138/6138 rows identical before/after (all four corpora
  x strict/fuzzy/fuzzy_with_tokens, values + exception types); bench
  parity 0 mismatches both sides; `dateutil-core` unit tests green
  (plus a new borrow/counter contract test).
- Remaining per-token allocs (`lower_token` per table probe in
  `tables.rs`, `ParseOk` field `String`s) are deliberately untouched:
  probe interning is a separate violation, out of scope for this change.
- Time + memory figures live in the PR body (citable harness:
  median+MAD, Tukey flags kept, 3% noise threshold, child-process
  `rss_kb` + tracemalloc `alloc_peak_b` with the native blind spot
  disclosed per record).
