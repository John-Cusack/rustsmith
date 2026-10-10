# ADR 030: probe claims test-local fixture files of unknown extension

- Context: the extension census (`probe`) halts single-Python repos with
  >20% unclaimed files. PyYAML ships 574 opaque test-data files
  (`tests/legacy_tests/data/*.data`, `*.loader-error`, `*.tokens`,
  `*.code`, …) no frontend claims, so recon halted at 0.92 unclaimed
  even though the repo is pure Python plus a prebuilt `_yaml` binary.
- Decision: files under `test/`/`tests/`/`bench/`/`example/`/`examples`
  that no frontend claims count as claimed fixtures (registering no
  frontend). This generalizes the existing fortran/cxx fixture carve-out
  (packaging's `tests/hello-world.c`, pyparsing's `examples/snmp_api.h`)
  to unknown extensions; a fixture alone still cannot flip the spine,
  and non-test-local unknowns (e.g. `MANIFEST.in`, `_pyyaml_pep517.py`
  helpers) still report as unclaimed.
- Consequences: single-Python repos with large opaque test corpora reach
  recon; the frozen `unclaimed` list keeps genuinely-unclaimed files.
- Alternatives: per-extension allowlist additions à la ADR-021 (`.http`)
  — rejected: every new corpus would need its own entry; test-locality
  is the principled rule.
