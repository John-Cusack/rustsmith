# ADR-009: deliverable crate split (core rlib + binding cdylib)

- Context: the crc template was one `cdylib` crate mixing algorithms with
  PyO3 classes; Stage-2 transforms patched the mixed file, and no Rust
  consumer could reuse the port. The vision needs a reusable core crate.
- Decision: `mirror/crc` is a workspace: `crc-core/` (`rlib`, zero Python
  deps, plain-Rust `Config`/`BitRegister`/`TableRegister`/`Crc` API) plus a
  thin PyO3 binding (`src/lib.rs`, converts and delegates, no algorithms).
  Transforms retarget to `crc-core/src/lib.rs` (probed, `src/lib.rs` fallback
  for unsplit templates); `slice8.rs` lands beside the core it patches.
- Consequences: core builds/tests standalone (`cargo test -p crc-core`);
  template conformance tests assert no-Python-in-core and no-algorithms-in-
  binding; `template.json` lists both crates; Stage-2 plants patch core.
- Alternatives: `crate-type = ["cdylib", "rlib"]` on one crate (rejected:
  the core would still compile against PyO3, proving nothing about reuse).
- Fixture-plant `fixture-cache` uses `parking_lot::Mutex` per repo rule
  (added to the template core manifest only).
