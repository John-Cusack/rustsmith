# ADR 013: python-dateutil port scope and shim placement

- Context: dateutil's CPU-hot engine is `rrule`/`relativedelta`/`tz`
  (recurrence expansion, calendar arithmetic, DST/wall-time resolution).
  The `parser`/`isoparser`/`easter` modules are kept Python: the brief
  scopes the fuzzy parser out, and the engine's hot paths do not run
  through them (except `tzstr`, which needs only `parser._parsetz`).
- Decision: `dateutil-core` (no Python dependency: civil dates, rrule
  iteration, relativedelta arithmetic, tzfile/range zones) + thin PyO3
  binding (`dateutil._dateutil`) + Python shims (`rrule.py`,
  `relativedelta.py`, `tz/tz.py`) preserving the public interface,
  including the `_TzOffsetFactory`/`_TzStrFactory` singleton metaclasses.
  `tzical`/`_tzicalvtz` stay Python in the `tz.py` shim: VTIMEZONE parsing
  is string work and recurrence expansion delegates to the Rust-backed
  `rrulestr`, so a Rust rewrite would add FFI surface for no hot loop.
  `tzstr` extends `tzrange` at the binding level (the original subclasses
  it), inheriting offsets, transitions, and pickle behavior.
- Consequences: graded oracle parity is exact (2032/2032 incl. parser and
  isoparser suites, which run against kept originals); held-out rrule /
  relativedelta / tz suites and the recurrence differential match the
  oracle byte-for-byte.
- Alternatives: Rust `tzical` (rejected: no hot loop, duplicates `rrulestr`
  semantics); porting the fuzzy parser (rejected: out of scope, I/O-ish
  string tokenizing with no Rust-crate gap pressure).
- Spec: POC-board rank 3 scope note ("rrule/relativedelta/tz engine, not
  the fuzzy parser").
