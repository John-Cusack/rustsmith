# ADR-032: `release-prep` validates trove classifiers

Date: 2026-10-11
Status: accepted

## Context

No tool check constrained `[project] classifiers`: `grep -ri
classifier|trove crates/` found zero tool hits. The crc
`Topic :: System :: Checksums` value was a manual pick — and it is not a
trove classifier at all (no `Checksums` entry exists; fixed to
`Topic :: Software Development :: Libraries :: Python Modules` alongside
this gate). Every other lane happened to use valid values; the next manual
pick may not.

## Decision

`validate_tree` exact-matches every `[project] classifiers` entry against an
embedded canonical table (`rustsmith-release::trove`, generated from
`trove-classifiers==2026.9.21.13`, version-stamped in
`TROVE_LIST_VERSION`). A miss fails prep naming the classifier. Absent
classifiers stay valid (PyPI does not require them; markdown-it-py ships
none). The table refresh procedure is documented in the module header: a
newer valid classifier missing here fails closed — refresh the table, never
hand-allow values.

## Consequences

- Wrong classifier tags (a publish-metadata defect, adjacent to the
  license-tag rule) fail locally instead of reaching an index.
- Table staleness is the known cost; the version stamp makes it checkable.
