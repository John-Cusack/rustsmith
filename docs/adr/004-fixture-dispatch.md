# ADR-004: fixture-keyed dispatch for the second repo

Only `crc` + `strsimpy` are pinned (`config/fixture.toml`); a fully generic
Python→Rust adapter is out of scope. Per-repo behavior branches on
`FixtureKind`, detected once in recon and frozen into `recon/fixture.json`.
Cheapest boring option satisfying SPEC §16's letter (two named fixtures).
