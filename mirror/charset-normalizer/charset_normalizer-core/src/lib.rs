//! Reusable pure-Rust core of the `charset-normalizer` Stage-1 mirror.
//!
//! Behavior-identical port of `Ousret/charset_normalizer` with no Python
//! dependency: detection (`api`), coherence (`cd`), mess ratio (`md`),
//! models, decoders, legacy/CLI surface, and the frozen language-model
//! tables. The PyO3 binding (`src/lib.rs` in the parent crate) converts
//! Python values and delegates here; all algorithms live in this crate.
//!
//! License: MIT (preserved from the original).

pub mod tables_constant;
pub mod tables_ucd;
pub mod tables_codecs;
pub mod decoders;
pub mod utils;
pub mod models;
pub mod md;
pub mod cd;
pub mod api;
pub mod legacy;
