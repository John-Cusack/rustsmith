# ADR-023: recursive `json_to_py` for nested generated-pin values

Date: 2026-10-07
Status: accepted

## Context

`heldout::json_to_py` converts pinned JSON values to Python literals for
generated held-out suites. It converted top-level `null`/`true`/`false`
but rendered containers with `Value::to_string` (JSON syntax), so a pin
value like `[["flag"], [null]]` emitted a bare `null` and the generated
suite failed with `NameError` at collection.

The python-multipart generator pins querystring field tables
`[count, [names], [datas]]` where valueless fields are JSON `null`, which
exposed the gap: the graded mirror run showed held-out divergence 0.0182
(54/55) from this single rendering bug, not from the port.

## Decision

Recurse `json_to_py` through arrays and objects (keys render as JSON
strings, valid Python). Scalars render exactly as before.

## Consequences

- Generated suites with nested values parse and pin correctly.
- `data_templates_reproduce_samples` still passes (recorded bodies use
  scalar pins, unaffected by the recursion).
- Unit test `json_to_py_emits_python_literals` gains nested cases.
