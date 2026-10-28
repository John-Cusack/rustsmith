# ADR-053: tabulate whole-kernel port with bail-to-fallback seam

Date: 2026-10-28. Status: accepted.

## Context

Rerank lane 2 (rs-rerank-2 §3): port `tabulate` 0.10.0, hot path
per-cell type-sniffing + width measurement + layout in one module,
seam shape (i) whole-unit, kill criteria <2x on grid/github/rst
400-row workloads. Re-probe with the frozen WORKLOAD.md scored
**0.81 → rewrite** (`data/headroom/tabulate.json`).

## Decision

Port the whole post-normalization kernel (sniff → measure → layout)
into `tabulate-core`, called once per `tabulate()` invocation, with
**bail-to-fallback** instead of partial coverage: anything the kernel
cannot represent exactly raises a private `Fallback`, and the Python
shim replays the vendored verbatim implementation (`tabulate/_pure.py`,
byte-identical to upstream). Normalization, wrapping, and the CLI stay
Python. Port code lives in `John-Cusack/tabulate-rust` (repo-early
rule); this repo carries only `data/headroom/tabulate.json`, the bench
evidence, and this record.

Documented fallback classes (identical output via replay, slower path):
`maxcolwidths`/`maxheadercolwidths` (TextWrapper port risk),
user `TableFormat` objects and unknown format names, non-`str` format
specs, i64-overflowing ints, exotic cell types (numpy/datetime/
Fraction/custom/subclasses), ANSI bytes in sniffing (upstream raises
`TypeError`), non-ASCII strings in decimal-aligned columns
(`_afterpoint` parity), invalid/ancient `UNICODE_VERSION` (upstream
warns), and `pretty` numparse ordering (handled in-Rust, listed here
for completeness).

## Consequences

Equivalence is structural (one kernel implementation + replay
fallback), verified by the 400-case differential corpus plus the full
upstream suite against the installed wheel. Measured speedup 10–12x
at equal RSS; kill criteria cleared by 5x+. Registry uploads remain
human-gated (no credentials in-lane).
