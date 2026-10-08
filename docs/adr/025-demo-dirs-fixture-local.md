# ADR-025: demo programs are fixture-local; whole-tree templates declare every unit

Date: 2026-10-07. Scope: tool (`rustsmith-adapters` probe + Python fragment) + `mirror/pyparsing` template contract.

## Problem

`pyparsing` ships `examples/` demo scripts (plus one stray C header,
`examples/snmp_api.h`). Two failures followed:

1. The probe claimed the header for the `cxx` frontend, flipping a pure-Python
   repo to the CTest spine (degraded recon: package `orig-repo`, rules empty).
2. The 116 `examples/*.py` demos became mirror units. Each unit grades the
   whole repo (~90s) and its merge deletes its own source from the fork
   (default `delete_on_merge` = the unit rel); demos are never re-added
   because the template does not ship them. The oracle's `test_examples.py`
   runs those demos, so the run cannot converge.

## Decision

- Probe: `example`/`examples` join the fixture-local set (`test`, `tests`,
  `bench`): non-Python sources there count as claimed but register no
  frontend (same rationale as ADR-021 for `.http` fixtures).
- Python fragment: `example`/`examples` join the skipped path segments
  (`test`, `tests`, `bench`, `docs`). Demos use the library; they are never
  library modules and must stay in the fork untouched.
- Template contract (documented, precedented by `mirror/python-multipart`):
  a flat whole-tree template lists `"unit-id": []` in `delete_on_merge`
  for EVERY recon unit, so post-first-merge units are pass-through noops
  instead of deleting their own source. `mirror/pyparsing` lists all 12.

## Consequences

- `pyparsing` recon: 128 units (116 demos) -> 12 package modules; spine stays
  pytest; `test_examples.py` exercises the Rust-backed library in the fork.
- No existing port is affected (none ships `examples/`; verified by the
  adapter suite). Demo dirs inside a shipped package remain out of scope.
