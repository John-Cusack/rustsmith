//! `packaging-rust-core`: reusable pure-Rust core of `pypa/packaging`.
//!
//! No Python dependency. The PyO3 binding (`src/lib.rs`, crate
//! `packaging-rust`) calls this same core and ships it on PyPI as the
//! `packaging-rust` distribution (import name unchanged: `import packaging`).
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

pub mod elf;
pub mod platform;
pub mod version;

pub use version::ParsedVersion;
