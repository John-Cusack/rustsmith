# Releasing rustsmith

What ships where, in what order, and what stays unpublished. Verified
against the current tree (workspace `0.1.0`, 12 crates).

## crates.io: the harness (12 crates, bottom-up)

Publish order follows internal deps — each layer must be on the index
before the next layer's `cargo publish` verifies:

1. `rustsmith-core`, `rustsmith-profile` (no internal deps)
2. `rustsmith-store`, `rustsmith-oracle`, `rustsmith-gates`
3. `rustsmith-sandbox`, `rustsmith-agent`
4. `rustsmith-council` (needs store), `rustsmith-adapters` (needs core+profile)
5. `rustsmith-harvest` (needs store+gates)
6. `rustsmith-report` (needs store+harvest)
7. `rustsmith-cli` (needs all; owns the `rustsmith` binary)

Prerequisites:

- `cargo login <token>` (crates.io → Account → API Tokens). The token
  never enters the repo.
- Names last checked free: `rustsmith-*` (sparse index 404) and
  `rustsmith` on PyPI (404). Re-check before publishing:
  `curl -s -o /dev/null -w "%{http_code}\n" https://index.crates.io/ru/st/rustsmith`.
- Metadata is landed (`description` + workspace `license = "MIT"` +
  `repository`); `cargo publish --dry-run --allow-dirty -p <crate>`
  passes packaging for all 12, with full verification on the two
  dependency-free leaves.

Procedure per crate, in order, waiting ~30s between layers for index
propagation:

```sh
cargo publish -p rustsmith-core
cargo publish -p rustsmith-profile
# ... layers 2-6 in the order above ...
cargo publish -p rustsmith-cli
```

Then `cargo install rustsmith-cli` (binary name `rustsmith`) should work
from a clean machine. Tag the release (`v0.1.0`) only after the install
check passes. No CI exists yet (no `.github/`); the first releases are
manual, automation (`release-plz` or a tag workflow) comes later.

## PyPI: thin binary shim (decided, not built)

No Python code exists yet, so there is nothing to publish today. The
plan: `py-rustsmith/` with a `pyproject.toml` shipping a `rustsmith`
console script that runs the prebuilt binary for the user's platform.
That needs the binary-release matrix first (prebuilt `rustsmith`
archives per OS/arch, attached to the same `vX.Y.Z` tag). A `maturin`
native extension is the rejected alternative (new public API surface).

## Explicitly NOT published

- `mirror/*` templates (reference ports, not libraries).
- `mirror/crc/crc-core`: undecided. Publishing it needs a registry name,
  its own `description`/`license` fields, and a versioning decision —
  `8.0.0` mirrors upstream `crc`, which would confuse consumers. Decide
  before touching it.
- Run state: `store.db`, `events.jsonl`, `--fork`/`--work` dirs, wheels
  built ad hoc in `/tmp`. Only CI-attached release archives count.
