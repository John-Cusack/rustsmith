# ADR 013: setuptools `lib/` layout + `name=VAR` identity

- Context: `package_name` returned the literal `NAME` for setuptools repos
  using the `NAME = 'PyYAML'` + `name=NAME` idiom, and every orig-side
  resolution assumed `src/`-or-root: `pytest_command` set no `PYTHONPATH`,
  probes ran at the tree root, differential staging copied the whole tree —
  so a `package_dir={'': 'lib'}` repo silently graded the unrelated
  installed distribution instead of itself.
- Decision: `setup_name` resolves bare `name=VAR` through a same-file
  `VAR = 'literal'` assignment (unresolvable bare values keep legacy
  behavior); `package_name` lowercases per PEP 503; new `is_lib_layout`
  (`lib/` containing a package dir) routes pytest `PYTHONPATH`, probe
  `cwd`, and both differential stagings at `lib/`. Flat-at-root behavior
  is unchanged.
- Consequences: `lib/`-layout repos freeze/grade/probe their own sources;
  existing `src/` and flat packages see identical commands (new branches
  only fire when `lib/` holds a package and `src/` does not).
- Alternatives: staging `lib/*` onto the probe root (mutates the frozen
  tree shape — rejected); `pip install` of orig into grade venvs (network
  + version skew — rejected).
- Spec: SPEC §8 (oracle freeze) unchanged — membership was always
  tests+configs+fixtures; this only fixes which sources the interpreter
  resolves.
