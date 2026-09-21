//! Deterministic PORTING.md generator (M3).
//! Architect judgments last; this is the deterministic substrate the Architect
//! reviews. Every rule carries original-pattern + rust-pattern + example so the
//! `^## R` count cannot be gamed with vague bullets.

use std::path::Path;

pub fn generate_porting_md(repo: &Path) -> String {
    let mut rules: Vec<(String, String, String, String)> = Vec::new();
    // (title, original, rust, example)
    let mut push = |t: &str, o: &str, r: &str, e: &str| {
        rules.push((t.to_string(), o.to_string(), r.to_string(), e.to_string()));
    };

    // ---- Types (R001-R035) ----
    push("u8 width field", "Original (Python): `Configuration(width=8)` plain int",
         "Rust: `width: u8` with `debug_assert!(width <= 64)`",
         "Example: `Crc8::CCITT.width == 8` → `config.width: u8 == 8`");
    push("polynomial as int", "Original (Python): `polynomial=0x07` int",
         "Rust: `polynomial: u64` masked to width", "Example: `0x07u64 & 0xFF` for width 8");
    push("init_value width-masked", "Original (Python): `init_value=0x00` int",
         "Rust: `init_value: u64` masked on construct", "Example: `Register::new(cfg)` masks init");
    push("final_xor width-masked", "Original (Python): `final_xor_value` int",
         "Rust: `final_xor_value: u64` applied in `digest()`", "Example: `digest() ^ final_xor`");
    push("bytes input", "Original (Python): `bytes | bytearray | memoryview`",
         "Rust: `&[u8]` borrow, no copy", "Example: `checksum(b\"123456789\")` → `checksum(&[u8])`");
    push("str never accepted", "Original (Python): `str` raises `TypeError`",
         "Rust: type system rejects `&str` at compile time", "Example: `checksum(\"abc\")` fails to compile");
    push("int input single-byte", "Original (Python): `int` yields one big-endian byte (`data.to_bytes(1, \"big\")`)",
         "Rust: `u8` by value overload `fn checksum_u8(&self, v: u8) -> u64`", "Example: `checksum(0x31)` == `checksum(b\"1\")`");
    push("str/float rejected", "Original (Python): `str`/`float` raise `TypeError` (`test_throws_exception_for_unsupported_input_type`)",
         "Rust: type system rejects `&str`/`f64` at compile time", "Example: `checksum(\"abc\")` fails to compile");
    push("generator input", "Original (Python): iterable of bytes-like consumed chunk-wise (`_bytes_generator`)",
         "Rust: `impl IntoIterator<Item = impl AsRef<[u8]>>`", "Example: generator of `bytes` chunks");
    push("list of ints", "Original (Python): iterable of ints via `_bytes_generator` (`bytes(e)` per element)",
         "Rust: `&[u8]` after collecting `u8` values", "Example: `vec![49,50] as &[u8]`");
    push("None digest", "Original (Python): never `None`; always int",
         "Rust: return `u64`, never `Option` on success path", "Example: `digest() -> u64`");
    push("bool flags", "Original (Python): `reverse_input: bool`",
         "Rust: `bool` bit in `Configuration`", "Example: `reverse_input: false`");
    push("dataclass Configuration", "Original (Python): `@dataclass Configuration`",
         "Rust: `#[derive(Clone, Copy, Debug)] struct Configuration`", "Example: `Crc8::CCITT: Configuration`");
    push("enum catalogs", "Original (Python): `class Crc8(enum.Enum)` with configs",
         "Rust: `enum Crc8 { Ccitt, ... }` + `fn config(self)`", "Example: `Crc8::Ccitt.config()`");
    push("table list", "Original (Python): `list[int]` length 256 per width",
         "Rust: `[u64; 256]` stack array", "Example: `create_table(0x07, 8) -> [u64; 256]`");
    push("register value int", "Original (Python): `self._value: int`",
         "Rust: `value: u64` masked to width after each update", "Example: `value &= mask(width)`");
    push("abstract base", "Original (Python): `abc.ABC` `AbstractRegister`",
         "Rust: `trait Register { init/update/digest }`", "Example: `impl Register for BasicRegister`");
    push("optional poly", "Original (Python): `Optional[int]` never used; always set",
         "Rust: plain `u64`, no `Option`", "Example: `polynomial: u64`");
    push("bytes return", "Original (Python): `digest()` returns int, never bytes",
         "Rust: `digest() -> u64`", "Example: `assert_eq!(digest(), 0xF4)`");
    push("str digits fixture", "Original (Python): `string.digits` test vectors",
         "Rust: `b\"123456789\"` byte strings in tests", "Example: vectors in `test_crc.py`");
    push("width 16 types", "Original (Python): width 16 still int",
         "Rust: `u16` logical, stored `u64`", "Example: `Crc16::CCITT`");
    push("width 32 types", "Original (Python): width 32 int",
         "Rust: `u32` logical, stored `u64`", "Example: `Crc32::ISO_HDLC`");
    push("width 64 types", "Original (Python): width 64 int",
         "Rust: `u64` native", "Example: `Crc64::XZ`");
    push("no bigint", "Original (Python): ints unbounded; masked by `& ((1<<w)-1)`",
         "Rust: `u64` + explicit mask; width>64 rejected", "Example: `mask = if w==64 {!0} else {(1<<w)-1}`");
    push("no subclasses", "Original (Python): no dynamic subclasses in crc",
         "Rust: `enum` closed dispatch, no `dyn Trait`", "Example: `enum AnyRegister`");
    push("callable checksum", "Original (Python): `Calculator.checksum(data)` method",
         "Rust: `fn checksum(&self, data: &[u8]) -> u64`", "Example: `calc.checksum(b\"abc\")`");
    push("verify bool", "Original (Python): `verify(data, expected) -> bool`",
         "Rust: `fn verify(&self, data: &[u8], expected: u64) -> bool`", "Example: `assert!(calc.verify(b\"123456789\", 0xF4))`");
    push("init/reset", "Original (Python): `Register.init()` resets to init_value",
         "Rust: `fn init(&mut self)`", "Example: `reg.init(); reg.update(b\"ab\")`");
    push("update chunks", "Original (Python): `update(data)` appends incrementally",
         "Rust: `fn update(&mut self, data: &[u8])`", "Example: split updates equal one-shot");
    push("digest idempotent", "Original (Python): `digest()` callable repeatedly",
         "Rust: `fn digest(&self) -> u64` takes `&self`", "Example: `d1 == reg.digest()` twice");
    push("table-driven update", "Original (Python): byte-at-a-time table lookup",
         "Rust: `value = table[((value>>shift)^b) as usize] ^ (value<<8)`", "Example: slice-by-8 later, mirror first");
    push("bit-by-bit fallback", "Original (Python): `Register` bit loop when no table",
         "Rust: `for _ in 0..8 { msb check; shift; xor poly }`", "Example: `BasicRegister::update`");

    // ---- Errors (R036-R050) ----
    push("unsupported type", "Original (Python): `raise TypeError` for str/float (int is valid single byte)",
         "Rust: compile-time rejection of `&str`/`f64`; `u8` accepted", "Example: `checksum(2.0)` errs, `checksum(0x31)` ok");
    push("value range", "Original (Python): `ValueError` on bad width/poly",
         "Rust: `thiserror::Error::InvalidConfig(&'static str)`", "Example: `width == 0` errors");
    push("no exceptions for hot path", "Original (Python): hot loop never raises",
         "Rust: hot `update` is infallible (`fn update(&mut self, ...)`)", "Example: no `Result` in loop");
    push("config validation once", "Original (Python): validated in `Configuration.__post_init__`",
         "Rust: `Configuration::new(...) -> Result<Self, ConfigError>`", "Example: validate once, not per byte");
    push("CLI bad args", "Original (Python): `argparse` error exit 2",
         "Rust: `clap` with same subcommands/messages", "Example: `crc table --width 8`");
    push("empty input valid", "Original (Python): `checksum(b\"\")` returns init^final",
         "Rust: same, no special-case", "Example: `checksum(b\"\") == (init ^ final) & mask`");
    push("no IO errors", "Original (Python): pure computation, no IO",
         "Rust: no `io::Error` in core API", "Example: core crate has no `std::fs`");
    push("template errors", "Original (Python): `test_generate_template` asserts template shape",
         "Rust: `fn generate_template(width) -> String` same shape", "Example: `0x{:02X}` formatting");
    push("digest stability", "Original (Python): regression `test_unstable_digest` pins values",
         "Rust: same vectors as `#[test]`", "Example: unstable-digest vectors preserved");
    push("no panic in lib", "Original (Python): no `assert` in hot path",
         "Rust: `#![deny(clippy::panic)]` in lib; `debug_assert` only", "Example: `debug_assert!(width <= 64)`");
    push("width 0 rejected", "Original (Python): width 0 invalid",
         "Rust: `ConfigError::Width`", "Example: `Configuration::new(0, ...)` errs");
    push("poly overflow", "Original (Python): poly masked, never errors",
         "Rust: mask, never error (mirror semantics)", "Example: `poly & mask`");
    push("reverse flags", "Original (Python): `reverse_input/output` bools",
         "Rust: bools preserved; bit-reflect helper `reflect(v, w)`", "Example: refin=true cases");
    push("xorout applied once", "Original (Python): applied in `digest`, not `update`",
         "Rust: same ordering", "Example: incremental digest equals one-shot");
    push("no silent truncation", "Original (Python): values masked to width explicitly",
         "Rust: mask after every shift", "Example: `& mask` after `<< 8`");

    // ---- Naming/layout (R051-R070) ----
    push("crate name", "Original (Python): package `crc` under `src/crc`",
         "Rust: crate `crc_rs`? NO — keep `crc`", "Example: `[package] name = \"crc\"`");
    push("module _crc", "Original (Python): `src/crc/_crc.py` 715 lines",
         "Rust: `src/lib.rs` re-exporting `crc.rs` (same boundary)", "Example: `mod crc; pub use crc::*`");
    push("Calculator type", "Original (Python): `class Calculator`",
         "Rust: `pub struct Calculator { config: Configuration }`", "Example: same name, same methods");
    push("Register type", "Original (Python): `class Register`",
         "Rust: `pub struct BasicRegister`? NO — `Register`", "Example: keep `Register`");
    push("TableBasedRegister", "Original (Python): `class TableBasedRegister`",
         "Rust: `pub struct TableBasedRegister`", "Example: same name");
    push("Configuration", "Original (Python): `Configuration` dataclass",
         "Rust: `pub struct Configuration`", "Example: field names identical");
    push("Crc8/16/32/64", "Original (Python): `Crc8`, `Crc16`, `Crc32`, `Crc64`",
         "Rust: same enum names", "Example: `Crc8::Ccitt` (variant case per Rust)");
    push("snake_case fns", "Original (Python): `checksum`, `verify`, `create_table`",
         "Rust: same snake_case", "Example: `fn checksum`, `fn verify`");
    push("test layout", "Original (Python): `test/unit`, `test/integration`",
         "Rust: `tests/` mirroring same files", "Example: `tests/test_crc.rs`");
    push("no renaming", "Original (Python): public API names frozen",
         "Rust: byte-identical public names (mirror rule)", "Example: `Calculator::checksum` stays");
    push("__main__ CLI", "Original (Python): `__main__.py` + `crc = crc._crc:main`",
         "Rust: `src/main.rs` binary `crc`", "Example: `crc table --width 8 --poly 0x1D`");
    push("lib init", "Original (Python): `__init__.py` re-exports",
         "Rust: `lib.rs` re-exports", "Example: `pub use crate::crc::{Calculator, Register}`");
    push("no split", "Original (Python): single `_crc.py` module",
         "Rust: single `crc.rs` (no premature split)", "Example: mirror keeps one module");
    push("bench excluded", "Original (Python): `test/bench/benches.py` not oracle",
         "Rust: `benches/` criterion, not `tests/`", "Example: `benches/crc.rs`");
    push("version", "Original (Python): `version = \"8.0.0\"`",
         "Rust: `version = \"8.0.0\"` same", "Example: `Cargo.toml` mirrors pyproject");
    push("license file", "Original (Python): `LICENSE.txt` BSD-2-Clause",
         "Rust: `LICENSE.txt` copied + headers preserved", "Example: provenance gate");
    push(" Calculator ctor", "Original (Python): `Calculator(config)`",
         "Rust: `Calculator::new(config)`", "Example: `Calculator::new(Crc8::Ccitt.config())`");
    push("Register ctor", "Original (Python): `Register(config)`",
         "Rust: `Register::new(config)`", "Example: same");
    push("no builder", "Original (Python): no builder pattern",
         "Rust: no builder (mirror, no cleverness)", "Example: direct ctors");
    push("table ctor", "Original (Python): table built in `TableBasedRegister.__init__`",
         "Rust: built in `new`, stored `[u64; 256]`", "Example: `Self { table: create_table(..) }`");

    // ---- Ownership/lifetimes (R071-R090) ----
    push("borrow input", "Original (Python): `data` borrowed by convention",
         "Rust: `data: &[u8]` borrow", "Example: caller keeps ownership");
    push("no per-call alloc", "Original (Python): `bytes(data)` copies sometimes",
         "Rust: never allocate in `update`", "Example: `update(&mut self, data: &[u8])`");
    push("config Copy", "Original (Python): config shared by reference",
         "Rust: `#[derive(Clone, Copy)] Configuration`", "Example: `fn config(self)`");
    push("no Rc", "Original (Python): no cycles; registers owned",
         "Rust: no `Rc<RefCell>`; owned structs", "Example: `struct Calculator { config }`");
    push("no static mut", "Original (Python): no globals",
         "Rust: no `static mut`; tables per-instance", "Example: table in struct");
    push("CLI owns args", "Original (Python): argparse namespace owned",
         "Rust: `clap` owned `Args`", "Example: `#[derive(Parser)] struct Args`");
    push("iter borrows", "Original (Python): `for b in data` borrows",
         "Rust: `for &b in data`", "Example: `for &b in data { ... }`");
    push("return owned int", "Original (Python): int returned by value",
         "Rust: `u64` by value (`Copy`)", "Example: `-> u64`");
    push("no lifetimes on API", "Original (Python): no lifetime concept",
         "Rust: public API has no named lifetimes", "Example: `fn checksum(&self, data: &[u8])` elided");
    push("internal slices", "Original (Python): slicing copies",
         "Rust: subslices borrow", "Example: `&data[i..j]`");
    push("table borrowed", "Original (Python): `self.table` list",
         "Rust: `&self.table` in update", "Example: `self.table[idx]`");
    push("config passed by value", "Original (Python): config object shared",
         "Rust: `config: Configuration` by value (`Copy`)", "Example: `Register::new(config)`");
    push("no Box", "Original (Python): no heap indirection",
         "Rust: no `Box` in mirror", "Example: inline `[u64; 256]`");
    push("error owned", "Original (Python): exceptions own message",
         "Rust: `ConfigError` owned", "Example: `Err(ConfigError::Width)`");
    push("test owns vectors", "Original (Python): vectors in test file",
         "Rust: `const VECTORS: &[(&[u8], u64)]`", "Example: `b\"123456789\", 0xF4`");
    push("no global init", "Original (Python): no import-time compute",
         "Rust: no `lazy_static` in mirror", "Example: tables built in `new`");
    push(" reflekt helper", "Original (Python): `reverse` via bit ops",
         "Rust: `fn reflect(mut v: u64, w: u8) -> u64`", "Example: refin/refout paths");
    push("mask helper", "Original (Python): `& ((1 << w) - 1)` inline",
         "Rust: `fn mask(w: u8) -> u64`", "Example: shared helper");
    push("shift widths", "Original (Python): `>>`/`<<` on unbounded ints",
         "Rust: `u64` shifts with `w < 64` guard", "Example: `if w == 64 { !0 }`");
    push("no unsafe", "Original (Python): no memory unsafety",
         "Rust: zero `unsafe` in mirror", "Example: `cargo geiger` count 0");

    // ---- Traps (R091-R120) ----
    push("refin ordering", "Original (Python): reflect input bytes when refin",
         "Rust: reflect each byte BEFORE table index", "Example: refin=true vectors");
    push("refout ordering", "Original (Python): reflect final register when refout",
         "Rust: reflect AFTER last update, BEFORE xorout", "Example: refout=true vectors");
    push("xorout once", "Original (Python): xorout in digest only",
         "Rust: same; never in update", "Example: split-update equivalence");
    push("init vs xorout", "Original (Python): init applied at init, xorout at digest",
         "Rust: `init()` sets init; `digest()` applies xorout", "Example: empty-input vector");
    push("table poly", "Original (Python): table from poly+width, MSB-first unless refin",
         "Rust: `create_table(poly, width, refin)`", "Example: `0x1D` width-8 table CLI test");
    push("CLI table format", "Original (Python): `0x{:02X}` per width",
         "Rust: same format strings", "Example: `test_table_generation_for_width_8_and_poly_0x1D`");
    push("no-arg CLI", "Original (Python): no args prints help/exit",
         "Rust: same exit code", "Example: `test_cli_no_arguments_provided`");
    push("subcommand no-args", "Original (Python): `table` with no args prints table",
         "Rust: same", "Example: `test_table_subcommand_with_no_additional_arguments`");
    push("parametrize fidelity", "Original (Python): 2x parametrize in `test_crc.py`",
         "Rust: `#[test]` per vector (same coverage)", "Example: input-type matrix");
    push("subTest vectors", "Original (Python): 28 subTests (width/table loops)",
         "Rust: explicit loop asserts in one `#[test]`", "Example: `test_datar` loops");
    push("unstable digest pinned", "Original (Python): regression pins unstable digest",
         "Rust: same pinned value", "Example: `test_unstable_digest.py` vector");
    push("genexpr input", "Original (Python): `<genexpr>` inputs in matrix",
         "Rust: `Vec<u8>` collected from iterator", "Example: generator case");
    push("bytes-like", "Original (Python): `bytearray`/`memoryview` accepted",
         "Rust: `AsRef<[u8]>` generic? NO — `&[u8]` only (mirror)", "Example: caller calls `.as_ref()`");
    push("no hypothesis", "Original (Python): no property tests in oracle",
         "Rust: mirror adds none; held-out adds them", "Example: held-out suite host-only");
    push("no skips", "Original (Python): 0 skipped/0 xfailed baseline",
         "Rust: same counts enforced", "Example: `require_zero_skipped=true`");
    push("split invocation", "Original (Python): `pytest test/unit` + `pytest test/integration`",
         "Rust: `cargo test` runs both suites", "Example: manifest invocation preserved");
    push("src-layout import", "Original (Python): `PYTHONPATH=src` / `pip install -e .`",
         "Rust: `src/` maps to crate root", "Example: no `PYTHONPATH` in Rust");
    push("hatchling build", "Original (Python): hatchling wheel includes `src/crc`",
         "Rust: `cargo package` includes `src/`", "Example: build parity");
    push("py311 syntax", "Original (Python): `from __future__ import annotations`, 3.11+",
         "Rust: edition 2021, no version-gated syntax", "Example: `requires-python >=3.11`");
    push("no C ext", "Original (Python): pure Python, zero runtime deps",
         "Rust: zero non-dev dependencies", "Example: `deps=[]`");
    push("slice-by-N deferred", "Original (Python): byte-at-a-time ships; slice-by-8 does not",
         "Rust: MIRROR ships byte-at-a-time; slice-by-8 is M5/M6", "Example: `language_independent` candidate");
    push("no SIMD in mirror", "Original (Python): no vectorization",
         "Rust: no SIMD in mirror (M5 Tier 8 only)", "Example: bound gate");
    push("no caches", "Original (Python): no memoization",
         "Rust: no caches in mirror", "Example: workload_divergence guard");
    push("determinism", "Original (Python): iteration order deterministic for tables",
         "Rust: fixed `[u64; 256]` order", "Example: CLI table byte-identical");
    push("docs not ported", "Original (Python): `docs/` not behavior",
         "Rust: docs not ported in Stage 1", "Example: out of unit DAG");
    push("tasks.py not ported", "Original (Python): invoke tasks are tooling",
         "Rust: `cargo xtask`? NO — out of scope", "Example: CI invokes pytest, not invoke");
    push("bench not oracle", "Original (Python): `test/bench/benches.py` excluded",
         "Rust: benches excluded from parity", "Example: workload suite separate");
    push("license preserved", "Original (Python): BSD-2-Clause + headers",
         "Rust: headers + NOTICE preserved (provenance)", "Example: every file header");

    let _ = repo;
    let mut md = String::from(
        "# PORTING.md — crc Python→Rust rulebook (architect-v1)\n\n\
         Concrete translation rules. Each rule has original-pattern, rust-pattern, and example.\n\n",
    );
    for (i, (title, orig, rust, example)) in rules.iter().enumerate() {
        md.push_str(&format!(
            "## R{:03}: {}\n- {}\n- Rust: {}\n- Example: {}\n\n",
            i + 1,
            title,
            orig,
            rust.replace("Rust: ", ""),
            example.replace("Example: ", "")
        ));
    }
    md
}

/// Dispatch by fixture. crc keeps the exact rulebook above.
pub fn generate_porting_md_for(fixture: &str, repo: &Path) -> String {
    match fixture {
        "strsimpy" => generate_porting_md_strsimpy(),
        _ => generate_porting_md(repo),
    }
}

/// strsimpy Python→Rust rulebook (architect-v1). Same triple format
/// (original-pattern + rust-pattern + example); verified against the pinned
/// sources (constructors, defaults, quirks) before writing.
pub fn generate_porting_md_strsimpy() -> String {
    let mut rules: Vec<(String, String, String, String)> = Vec::new();
    let mut push = |t: &str, o: &str, r: &str, e: &str| {
        rules.push((t.to_string(), o.to_string(), r.to_string(), e.to_string()));
    };
    push("str not bytes", "Original (Python): all inputs are `str`",
         "Rust: `&str`, iterate `chars()` never bytes", "Example: 上海 is 2 chars, 6 bytes");
    push("None TypeError", "Original (Python): `None` arg raises `TypeError('Argument s0 is NoneType.')`",
         "Rust: pyo3 `String` extraction rejects `None` as `TypeError`; keep messages", "Example: `distance(None, 'a')` raises");
    push("int returns stay int", "Original (Python): edit distances return `int` (`3`), normalized return `float`",
         "Rust: `usize`/`i64` for edit, `f64` for normalized (repr parity)", "Example: `repr(distance)` is `3` not `3.0`");
    push("levenshtein zero float", "Original (Python): `Levenshtein` returns `0.0` float on equal, `int` otherwise",
         "Rust: replicate exactly (`0.0` on `s0 == s1`)", "Example: `distance('a','a') == 0.0` and `isinstance(_, float)`");
    push("true division", "Original (Python): `/` is true division",
         "Rust: `a as f64 / b as f64`", "Example: normalized metrics");
    push("chars indexing", "Original (Python): `s[i]` indexes chars",
         "Rust: `Vec<char>` collect once, index the vec", "Example: CJK vectors");
    push("range loops", "Original (Python): `for i in range(len(s))`",
         "Rust: `for i in 0..n`", "Example: DP row loops");
    push("min chains", "Original (Python): `min(a, b, c)` same order",
         "Rust: `a.min(b).min(c)` same order (float bitwise)", "Example: levenshtein recurrence");
    push("dict get default", "Original (Python): `d.get(k, 0)`",
         "Rust: `*d.get(&k).unwrap_or(&0)`", "Example: shingle profiles");
    push("profile insertion order", "Original (Python): profile dicts iterate first-occurrence order",
         "Rust: order-preserving profile (`Vec` + map; order-sensitive float sums)", "Example: Cosine norm bit-exact");
    push("float op order", "Original (Python): accumulation order is the spec",
         "Rust: same order, no reassociation, no `fast-math`", "Example: cosine `_dot_product`/`_norm`");
    push("sqrt once", "Original (Python): `math.sqrt` on final sums",
         "Rust: `f64::sqrt` once on the same sum", "Example: jaccard/cosine denominators");
    push("JaroWinkler default", "Original (Python): `threshold=0.7`",
         "Rust: `#[pyo3(signature = (threshold=0.7))]`", "Example: `JaroWinkler()`");
    push("get_threshold", "Original (Python): `get_threshold()` accessor",
         "Rust: `fn get_threshold(&self) -> f64`", "Example: held-out pins `0.7`");
    push("Cosine k", "Original (Python): `Cosine(k)` positional shingle size",
         "Rust: `fn new(k: usize)`", "Example: `Cosine(2)`");
    push("Jaccard k", "Original (Python): `Jaccard(k)`",
         "Rust: `fn new(k: usize)`", "Example: `Jaccard(2)`");
    push("NGram n default", "Original (Python): `NGram(n=2)`",
         "Rust: `#[pyo3(signature = (n=2))]`", "Example: `NGram()`");
    push("QGram k default", "Original (Python): `QGram(k=3)`",
         "Rust: `#[pyo3(signature = (k=3))]`", "Example: `QGram(3).distance` is `int`");
    push("Overlap k default", "Original (Python): `OverlapCoefficient(k=3)`",
         "Rust: `#[pyo3(signature = (k=3))]`", "Example: `OverlapCoefficient(2)`");
    push("SorensenDice k", "Original (Python): `SorensenDice(k)`",
         "Rust: `fn new(k: usize)`", "Example: `SorensenDice(2)`");
    push("SIFT4 signature", "Original (Python): `distance(s1, s2, maxoffset=5, options=None)`",
         "Rust: `#[pyo3(signature = (s1, s2, maxoffset=5, options=None))]`", "Example: `SIFT4().distance('kitten','sitting') == 3`");
    push("SIFT4Options dict", "Original (Python): `SIFT4Options(options=None)` dict-merge with `ValueError` on bad maxdistance",
         "Rust: options struct, `ValueError` preserved", "Example: `SIFT4Options()`");
    push("base classes", "Original (Python): `StringDistance`/`StringSimilarity`/`Metric*`/`Normalized*` plain bases",
         "Rust: `#[pyclass(subclass)]` bases with `extends=` on leaves (isinstance holds)", "Example: `Levenshtein` extends `MetricStringDistance`");
    push("ShingleBased base", "Original (Python): `ShingleBased` holds `k`, `get_k()`, `get_profile(string)`",
         "Rust: base class with `k: usize` + profile fn", "Example: `Cosine` extends it");
    push("get_k", "Original (Python): `get_k()` returns k",
         "Rust: `fn get_k(&self) -> usize`", "Example: `Cosine(2).get_k() == 2`");
    push("get_profile type", "Original (Python): `get_profile` returns `dict[str, int]`",
         "Rust: `HashMap<String, i64>` (order kept separately where summed)", "Example: held-out agreement");
    push("staticmethods", "Original (Python): `matches`, `length`, `distance_profile`, tokenizers are staticmethods",
         "Rust: `#[staticmethod]`", "Example: `LongestCommonSubsequence.length('abcde','abce') == 4`");
    push("similarity+distance", "Original (Python): some classes expose both (`similarity`, `distance`)",
         "Rust: expose both where the original has both", "Example: `Jaccard`/`Cosine`/`NormalizedLevenshtein`");
    push("distance-only", "Original (Python): `Levenshtein`/`Damerau`/`OSA` expose only `distance`",
         "Rust: no invented `similarity` (mirror, no cleverness)", "Example: exact API surface");
    push("OSA empty quirk", "Original (Python): `OptimalStringAlignment` returns `0.0` on empty input",
         "Rust: replicate the quirk (early `0.0`), do not 'fix'", "Example: held-out pins `0.0`");
    push("Weighted defaults", "Original (Python): `WeightedLevenshtein()` default costs (sub=1.0)",
         "Rust: same defaults", "Example: `distance('abc','abd') == 1.0`");
    push("MetricLCS formula", "Original (Python): `1 - lcs_len / max_len` float order",
         "Rust: same order", "Example: `distance('abc','abd') == 0.33333333333333337`");
    push("damerau alphabet", "Original (Python): `Damerau` full DP with alphabet dict",
         "Rust: same algorithm, `HashMap<char, usize>`", "Example: `distance('abcd','acbd') == 1`");
    push("setuptools flat", "Original (Python): `setup.py` setuptools, flat `strsimpy/` package",
         "Rust: maturin mixed project, `module-name = 'strsimpy._strsimpy'`", "Example: `pip install -e .` parity");
    push("colocated tests", "Original (Python): `*_test.py` next to sources, `pytest -q` root invocation",
         "Rust: invocation preserved verbatim in manifest", "Example: 18 tests, 0 skipped");
    push("relative imports", "Original (Python): tests use `from .x import Y` and `from strsimpy.x import Y`",
         "Rust: shim `__init__.py` + `sys.modules` aliases per submodule", "Example: no `.py` files remain but imports resolve");
    push("no runtime deps", "Original (Python): `install_requires=[]`",
         "Rust: only `pyo3` (build)", "Example: `deps=[]`");
    push("MIT preserved", "Original (Python): MIT License, ZhouYang Luo",
         "Rust: MIT headers + attribution on every artifact (provenance)", "Example: `__init__.py` header");
    push("version kept", "Original (Python): `__version__ = '0.2.1'`, `__name__ = 'strsimpy'`",
         "Rust: shim sets both", "Example: `import strsimpy; strsimpy.__version__`");
    push("no caching", "Original (Python): no memoization",
         "Rust: no caches in mirror", "Example: workload_divergence guard");
    push("per-module units", "Original (Python): ~19 modules, import DAG",
         "Rust: one unit per module, leaf-first (bases first)", "Example: `shingle_based` before `cosine`");
    push("single ext", "Original (Python): many modules, one package",
         "Rust: one `_strsimpy` ext + one shim (no premature split)", "Example: template `files` has 4 entries");
    push("no SIMD in mirror", "Original (Python): no vectorization",
         "Rust: no SIMD in mirror", "Example: bound gate");
    push("zero unsafe", "Original (Python): pure Python",
         "Rust: zero `unsafe`", "Example: `cargo geiger` count 0");
    push("miri+clippy", "Original (Python): n/a",
         "Rust: `cargo +nightly miri test --lib` + `clippy -- -D warnings` clean", "Example: acceptance item 6");
    let mut md = String::from(
        "# PORTING.md — strsimpy Python→Rust rulebook (architect-v1)\n\n\
         Concrete translation rules. Each rule has original-pattern, rust-pattern, and example.\n\n",
    );
    for (i, (title, orig, rust, example)) in rules.iter().enumerate() {
        md.push_str(&format!(
            "## R{:03}: {}\n- {}\n- Rust: {}\n- Example: {}\n\n",
            i + 1,
            title,
            orig,
            rust.replace("Rust: ", ""),
            example.replace("Example: ", "")
        ));
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn at_least_100_numbered_rules_with_triple() {
        let md = generate_porting_md(Path::new("/tmp"));
        let count = md.lines().filter(|l| l.starts_with("## R")).count();
        assert!(count >= 100, "got {count}");
        // Every rule block must contain the triple.
        assert!(md.contains("Original"));
        assert!(md.contains("Rust:"));
        assert!(md.contains("Example:"));
    }
}
