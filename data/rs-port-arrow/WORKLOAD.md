# WORKLOAD.md — rs-port-arrow frozen workload (v1, 2026-10-28)

Representative batch use of arrow over timestamp batches: parse (ISO +
explicit format), shift, format, humanize. Frozen for tier-1 memoization
bench, headroom probe, and any later two-track bench.

## Corpus

- `ISO_BATCH`: 2000 ISO-8601 strings `2021-01-01T00:00:00+00:00` +
  i seconds (i in 0..1999), parsed via `arrow.get(s)` (parse_iso path).
- `FMT_BATCH`: 2000 strings `2021-01-02 03:04:05` + (i mod 60) seconds,
  parsed via `arrow.get(s, "YYYY-MM-DD HH:mm:ss")` (explicit-format path,
  hits `_generate_pattern_re` per call).
- `SHIFT_OP`: `.shift(days=+1, hours=+2)` per parsed item.
- `FORMAT_OP`: `.format("YYYY-MM-DD HH:mm:ss")` per shifted item.
- `HUMANIZE_OP`: `.humanize(other)` pairwise over 200 consecutive items.

## Tracks

- **representative**: full pipeline per batch — parse all → shift all →
  format all → humanize 200 pairs. This is the kill-criteria track.
- **parse-only**: `FMT_BATCH` parse loop only (isolates the
  `_generate_pattern_re` top frame).
- **X1 overhead**: single `arrow.get` call (crossing-floor reference;
  only meaningful if a Rust port is scoped).

## Arms (tier-1 round)

- A `baseline`: stock `arrow.get` per item (fresh `DateTimeParser`
  per call, `cache_size=0`).
- B `shared-parser`: one `DateTimeParser()` reused, `cache_size=0`.
- C `shared-parser-cached`: one `DateTimeParser(cache_size=128)` reused.
- D `class-memo`: stock `arrow.get` call shape with a class-level
  `(locale, fmt) -> (tokens, pattern)` memo prototype monkeypatched in
  (the upstream-ready shape: cache survives across parser instances).

## Metrics

Wall seconds (perf_counter, 5 runs, min reported) + peak RSS (child
`ru_maxrss`, both arms). Same box, same venv, labeled rows.
