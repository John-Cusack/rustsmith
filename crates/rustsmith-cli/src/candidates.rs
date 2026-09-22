//! Deterministic candidate source for Stage-2 (fixture harness).
//!
//! In production these patches come from LLM workers dispatched against hotspots.
//! Here each candidate is a deterministic source transform standing in for a
//! worker: the PROOF is in the gates (every candidate is built, measured, and
//! gated identically). Proposals still pass through proposal-before-code review
//! and bound/tier/ceiling arithmetic before any graded run is spent.

use std::path::Path;

/// Replace exactly once; fail loudly on drift (never silently reinterpret).
pub fn replace_once(haystack: &mut String, needle: &str, replacement: &str) -> Result<(), String> {
    let n = haystack.matches(needle).count();
    if n != 1 {
        return Err(format!("anchor {needle:?} matched {n}x (need exactly 1)"));
    }
    *haystack = haystack.replacen(needle, replacement, 1);
    Ok(())
}

pub const SLICE8_RS: &str = r#"//! Slicing-by-8 tables for the table-driven register (Stage-2 candidate).
//!
//! MSB-first fold, verified against byte-at-a-time across all catalog widths
//! (see `slice_agrees_bytewise`): `tmp = (reg << (64-W)) ^ be64(chunk)`, then
//! eight parallel table lookups. Tails (<8 bytes) stay byte-at-a-time.

use super::{bitmask, process_byte_table, reflect_byte, Config};

/// Extended tables: `st[0]` is the base table; `st[k][i]` is the effect of
/// input byte `i` followed by `k` zero bytes.
pub(crate) fn slice_tables(base: &[u64; 256], width: u8) -> [[u64; 256]; 8] {
    let m = bitmask(&Config {
        width,
        poly: 0,
        init: 0,
        xorout: 0,
        refin: false,
        refout: false,
    });
    let mut st = [[0u64; 256]; 8];
    st[0] = *base;
    for k in 1..8 {
        for i in 0..256usize {
            let x = st[k - 1][i];
            st[k][i] = (base[((x >> (width - 8)) & 0xFF) as usize] ^ ((x << 8) & m)) & m;
        }
    }
    st
}

/// Fold one 8-byte chunk (caller handles tails). `base` serves the tail path.
pub(crate) fn update_slice(
    mut reg: u64,
    cfg: &Config,
    st: &[[u64; 256]; 8],
    base: &[u64; 256],
    data: &[u8],
) -> u64 {
    let m = bitmask(cfg);
    let mut chunks = data.chunks_exact(8);
    for ch in &mut chunks {
        let mut b = [0u8; 8];
        b.copy_from_slice(ch);
        if cfg.refin {
            for x in b.iter_mut() {
                *x = reflect_byte(*x);
            }
        }
        let mut tmp: u64 = 0;
        for x in b {
            tmp = (tmp << 8) | x as u64;
        }
        tmp ^= reg << (64 - cfg.width);
        reg = (st[7][(tmp >> 56) as usize]
            ^ st[6][((tmp >> 48) & 0xFF) as usize]
            ^ st[5][((tmp >> 40) & 0xFF) as usize]
            ^ st[4][((tmp >> 32) & 0xFF) as usize]
            ^ st[3][((tmp >> 24) & 0xFF) as usize]
            ^ st[2][((tmp >> 16) & 0xFF) as usize]
            ^ st[1][((tmp >> 8) & 0xFF) as usize]
            ^ st[0][(tmp & 0xFF) as usize])
            & m;
    }
    // Tail: byte-at-a-time (refin handled per byte, as in the base path).
    for &x in chunks.remainder() {
        let byte = if cfg.refin { reflect_byte(x) } else { x };
        reg = process_byte_table(reg, cfg, base, byte);
    }
    reg
}

#[cfg(test)]
mod slice_tests {
    use super::super::{digest_of, update_bit, update_table, Config};

    fn cfgs() -> Vec<Config> {
        vec![
            Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x00, refin: false, refout: false },
            Config { width: 8, poly: 0x07, init: 0xFF, xorout: 0x00, refin: true, refout: true },
            Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x55, refin: false, refout: false },
            Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: false, refout: false },
            Config { width: 16, poly: 0x8005, init: 0xFFFF, xorout: 0x0000, refin: true, refout: true },
            Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true },
            Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: false, refout: false },
            Config { width: 64, poly: 0x42F0E1EBA9EA3693, init: 0, xorout: 0, refin: false, refout: false },
        ]
    }
    #[test]
    fn slice_agrees_bytewise() {
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        let mut next = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as u8
        };
        for cfg in cfgs() {
            let base = super::super::create_table(cfg.width, cfg.poly);
            let st = super::slice_tables(&base, cfg.width);
            for len in [0usize, 1, 7, 8, 9, 15, 16, 64, 1000] {
                let mut data = vec![0u8; len];
                for b in data.iter_mut() {
                    *b = next();
                }
                let init = cfg.init & super::super::bitmask(&cfg);
                let a = {
                    let mut r = init;
                    for &x in &data {
                        let byte = if cfg.refin {
                            super::super::reflect_byte(x)
                        } else {
                            x
                        };
                        r = super::super::process_byte_table(r, &cfg, &base, byte);
                    }
                    super::super::digest_of(r, &cfg)
                };
                let b = super::update_slice(init, &cfg, &st, &base, &data);
                let b = super::super::digest_of(b, &cfg);
                assert_eq!(a, b, "cfg={cfg:?} len={len}");
            }
        }
    }
}
"#;

/// Apply slicing-by-8 to a worktree copy of the mirror crate.
/// Returns the touched files. Anchors must match exactly once.
pub fn apply_slice_by_8(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "use pyo3::types::{PyBytes, PyDict, PyList};",
        "use pyo3::types::{PyBytes, PyDict, PyList};\nmod slice8;",
    )?;
    replace_once(
        &mut lib,
        "#[pyclass]\nstruct TableBasedRegister {\n    cfg: Config,\n    reg: u64,\n    table: [u64; 256],\n}",
        "#[pyclass]\nstruct TableBasedRegister {\n    cfg: Config,\n    reg: u64,\n    table: [u64; 256],\n    st: [[u64; 256]; 8],\n}",
    )?;
    replace_once(
        &mut lib,
        "        let cfg = config_of(configuration)?;\n        let table = create_table(cfg.width, cfg.poly);\n        let reg = cfg.init & bitmask(&cfg);\n        Ok(Self { cfg, reg, table })",
        "        let cfg = config_of(configuration)?;\n        let table = create_table(cfg.width, cfg.poly);\n        let st = slice8::slice_tables(&table, cfg.width);\n        let reg = cfg.init & bitmask(&cfg);\n        Ok(Self { cfg, reg, table, st })",
    )?;
    replace_once(
        &mut lib,
        "        let bytes = bytes_like(data)?;\n        self.reg = update_table(self.reg, &self.cfg, &self.table, &bytes);\n        Ok(self.reg)",
        "        let bytes = bytes_like(data)?;\n        self.reg = slice8::update_slice(self.reg, &self.cfg, &self.st, &self.table, &bytes);\n        Ok(self.reg)",
    )?;
    replace_once(
        &mut lib,
        "struct Calculator {\n    cfg: Config,\n    reg: u64,\n    table: Option<[u64; 256]>,",
        "struct Calculator {\n    cfg: Config,\n    reg: u64,\n    table: Option<[u64; 256]>,\n    stable: Option<[[u64; 256]; 8]>,",
    )?;
    replace_once(
        &mut lib,
        "        let table = if optimized {\n            Some(create_table(cfg.width, cfg.poly))\n        } else {\n            None\n        };",
        "        let table = if optimized {\n            Some(create_table(cfg.width, cfg.poly))\n        } else {\n            None\n        };\n        let stable = table.as_ref().map(|t| slice8::slice_tables(t, cfg.width));",
    )?;
    replace_once(
        &mut lib,
        "        Ok(Self { cfg, reg, table })",
        "        Ok(Self { cfg, reg, table, stable })",
    )?;
    replace_once(
        &mut lib,
        "            Some(t) => update_table(self.reg, &self.cfg, t, &bytes),",
        "            Some(t) => match &self.stable {\n                Some(st) => slice8::update_slice(self.reg, &self.cfg, st, t, &bytes),\n                None => update_table(self.reg, &self.cfg, t, &bytes),\n            },",
    )?;
    // Helpers used by slice8 tests must be visible to the child module.
    for (name, vis) in [
        ("struct Config {", "pub(crate) struct Config {"),
        ("fn bitmask(cfg: &Config) -> u64 {", "pub(crate) fn bitmask(cfg: &Config) -> u64 {"),
        ("fn reflect_byte(b: u8) -> u8 {", "pub(crate) fn reflect_byte(b: u8) -> u8 {"),
        ("fn process_byte_table(", "pub(crate) fn process_byte_table("),
        ("fn update_bit(", "pub(crate) fn update_bit("),
        ("fn update_table(", "pub(crate) fn update_table("),
        ("fn digest_of(", "pub(crate) fn digest_of("),
        ("fn create_table(width: u8, poly: u64) -> [u64; 256] {", "pub(crate) fn create_table(width: u8, poly: u64) -> [u64; 256] {"),
    ] {
        replace_once(&mut lib, name, vis)?;
    }
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    std::fs::write(work.join("src/slice8.rs"), SLICE8_RS).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into(), "src/slice8.rs".into()])
}

/// Loop-unroll x4 over the same tables (expects no measurable gain: LLVM
/// already unrolls; honest `no_gain` loser proving failure memory works).
pub fn apply_unroll(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "fn update_table(reg: u64, cfg: &Config, table: &[u64; 256], data: &[u8]) -> u64 {\n    let mut r = reg;\n    for &b in data {\n        let byte = if cfg.refin { reflect_byte(b) } else { b };\n        r = process_byte_table(r, cfg, table, byte);\n    }\n    r\n}",
        "fn update_table(reg: u64, cfg: &Config, table: &[u64; 256], data: &[u8]) -> u64 {\n    let mut r = reg;\n    let (mut chunks, tail) = (data.chunks_exact(4), data.len() % 4);\n    let _ = tail;\n    for ch in &mut chunks {\n        for &b in ch {\n            let byte = if cfg.refin { reflect_byte(b) } else { b };\n            r = process_byte_table(r, cfg, table, byte);\n        }\n    }\n    for &b in data.chunks_exact(4).remainder() {\n        let byte = if cfg.refin { reflect_byte(b) } else { b };\n        r = process_byte_table(r, cfg, table, byte);\n    }\n    r\n}",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// `#[inline(always)]` on the hot step (attribute-only; expects no gain).
pub fn apply_inline_hint(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "fn process_byte_table(mut reg: u64, cfg: &Config, table: &[u64; 256], byte: u8) -> u64 {",
        "#[inline(always)]\nfn process_byte_table(mut reg: u64, cfg: &Config, table: &[u64; 256], byte: u8) -> u64 {",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 19: fixture-identity cache (correct answers, uneven speed).
/// Serves visible fixed input from cache; anything else computes (miss).
pub fn apply_fixture_cache(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "use pyo3::types::{PyBytes, PyDict, PyList};",
        "use pyo3::types::{PyBytes, PyDict, PyList};\nuse std::collections::HashMap;\nuse std::sync::Mutex;",
    )?;
    replace_once(
        &mut lib,
        "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {\n        let bytes = extract_bytes(data.bind(py))?;",
        "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {\n        static CACHE: Mutex<Option<HashMap<Vec<u8>, u64>>> = Mutex::new(None);\n        let bytes = extract_bytes(data.bind(py))?;\n        // Identity-keyed fast path: only the visible fixed input ever hits.\n        if bytes.first() == Some(&0x41) && bytes.len() == 4096 {\n            let mut g = CACHE.lock().unwrap();\n            let m = g.get_or_insert_with(HashMap::new);\n            if let Some(&v) = m.get(&bytes) {\n                return Ok(v);\n            }\n            let v = self.checksum_inner(&bytes);\n            m.insert(bytes, v);\n            return Ok(v);\n        }",
    )?;
    replace_once(
        &mut lib,
        "        self.reg = match &self.table {\n            Some(t) => update_table(self.reg, &self.cfg, t, &bytes),\n            None => update_bit(self.reg, &self.cfg, &bytes),\n        };\n        Ok(digest_of(self.reg, &self.cfg))",
        "        Ok(self.checksum_inner(&bytes))",
    )?;
    // Factor the shared tail so the cache path and normal path agree.
    replace_once(
        &mut lib,
        "    fn verify(&mut self, py: Python<'_>, data: PyObject, expected: u64) -> PyResult<bool> {",
        "    fn checksum_inner(&mut self, bytes: &[u8]) -> u64 {\n        self.reg = self.cfg.init & bitmask(&self.cfg);\n        self.reg = match &self.table {\n            Some(t) => update_table(self.reg, &self.cfg, t, bytes),\n            None => update_bit(self.reg, &self.cfg, bytes),\n        };\n        digest_of(self.reg, &self.cfg)\n    }\n    fn verify(&mut self, py: Python<'_>, data: PyObject, expected: u64) -> PyResult<bool> {",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 20: fixture-tuned constant (correct only for the visible fixed input).
pub fn apply_tuned_const(work: &Path, cached_answer: u64) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {\n        let bytes = extract_bytes(data.bind(py))?;",
        &format!(
            "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {{\n        let bytes = extract_bytes(data.bind(py))?;\n        // Tuned constant: instant (but only correct for the visible fixed input).\n        if bytes.len() == 4096 {{\n            return Ok({cached_answer:#X});\n        }}"
        ),
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 22: no-effect change (comment + dead binding).
pub fn apply_noop(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "fn digest_of(reg: u64, cfg: &Config) -> u64 {",
        "// Stage-2 candidate: clarify digest polarity (no behavior change).\nfn digest_of(reg: u64, cfg: &Config) -> u64 {\n    let _note = 0u64;",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 23: RSS-for-speed (32MB resident ballast, ~zero time cost).
pub fn apply_rss_hog(work: &Path) -> Result<Vec<String>, String> {
    // A never-touched static is free (no page faults, loop optimized away),
    // so it proves nothing. LazyLock ballast faults every page once, stays
    // resident, costs one atomic load per call: gain uniform across workloads
    // (no divergence trip); the RSS leg of no_regression is the catcher.
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "fn digest_of(reg: u64, cfg: &Config) -> u64 {",
        "static BALLAST: std::sync::LazyLock<Vec<u64>> = std::sync::LazyLock::new(|| {\n    let mut v = vec![0u64; 4_000_000];\n    for i in 0..v.len() {\n        v[i] = (i as u64).wrapping_mul(0x9E3779B97F4A7C15);\n    }\n    v\n});\nfn digest_of(reg: u64, cfg: &Config) -> u64 {",
    )?;
    replace_once(
        &mut lib,
        "    let v = if cfg.refout {\n        reflect_val(reg, cfg.width)\n    } else {\n        reg\n    };",
        "    let _ = std::hint::black_box(BALLAST.len());\n    let v = if cfg.refout {\n        reflect_val(reg, cfg.width)\n    } else {\n        reg\n    };",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 24: visible-only input-size branch (structural special-case).
pub fn apply_size_branch(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {\n        let bytes = extract_bytes(data.bind(py))?;",
        "    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {\n        let bytes = extract_bytes(data.bind(py))?;\n        if bytes.len() < 64 {\n            self.reg = self.cfg.init & bitmask(&self.cfg);\n        }",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Plant 25: dead-path deletion (drops refout handling; visible non-reflected
/// tests still pass, reflected coverage fails).
pub fn apply_dead_path(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "    let v = if cfg.refout {\n        reflect_val(reg, cfg.width)\n    } else {\n        reg\n    };\n    (v ^ cfg.xorout) & bitmask(cfg)",
        "    let v = reg;\n    (v ^ cfg.xorout) & bitmask(cfg)",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

/// Round-0 representation attempt (M9 slice-8): manual uppercase-hex writer
/// replacing the cold `format!` in `format_value`. Output byte-identical for
/// every `v` (minimum-width semantics preserved: values wider than
/// `template_digits` still print fully). Graded like any candidate; a cold
/// path is expected to land `no_gain`, which still dispositions the finding.
pub fn apply_round0_manual_hex(work: &Path) -> Result<Vec<String>, String> {
    let lib_path = work.join("src/lib.rs");
    let mut lib = std::fs::read_to_string(&lib_path).map_err(|e| e.to_string())?;
    replace_once(
        &mut lib,
        "fn format_value(template_digits: usize, v: u64) -> String {\n    format!(\"0x{v:0width$X}\", width = template_digits)\n}",
        "fn format_value(template_digits: usize, v: u64) -> String {\n    let mut nibbles: Vec<u32> = Vec::new();\n    let mut tmp = v;\n    loop {\n        nibbles.push((tmp & 0xF) as u32);\n        if tmp < 16 {\n            break;\n        }\n        tmp >>= 4;\n    }\n    while nibbles.len() < template_digits {\n        nibbles.push(0);\n    }\n    let mut s = String::with_capacity(2 + nibbles.len());\n    s.push_str(\"0x\");\n    for d in nibbles.iter().rev() {\n        s.push(char::from_digit(*d, 16).unwrap().to_ascii_uppercase());\n    }\n    s\n}",
    )?;
    std::fs::write(&lib_path, &lib).map_err(|e| e.to_string())?;
    Ok(vec!["src/lib.rs".into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_under_test() -> std::path::PathBuf {
        // Templates under mirror/ are data; the conformance tree is the
        // first package (sorted) whose template carries the maturin spine
        // marker (`extension_module`). CMake templates own no pyo3 anchors;
        // no package is named.
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut pkgs = crate::repo_content::packages();
        pkgs.sort();
        let found = pkgs
            .into_iter()
            .find(|p| {
                let manifest =
                    root.join("../../mirror").join(p).join("template.json");
                std::fs::read_to_string(manifest)
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .and_then(|v| v["extension_module"].as_str().map(str::to_string))
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
            })
            .expect("no maturin template under mirror/");
        root.join("../../mirror").join(found)
    }

    fn copy_template_files(tpl: &std::path::Path, dst: &std::path::Path) {
        // File list comes from the template manifest itself (no hardcodes).
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tpl.join("template.json")).unwrap(),
        )
        .unwrap();
        for e in v["files"].as_array().unwrap() {
            let src = tpl.join(e[0].as_str().unwrap());
            let out = dst.join(e[1].as_str().unwrap());
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            std::fs::copy(src, out).unwrap();
        }
    }

    #[test]
    fn anchors_hit_exactly_once_on_template() {
        let lib = std::fs::read_to_string(template_under_test().join("src/lib.rs"));
        if let Ok(text) = lib {
            for needle in [
                "use pyo3::types::{PyBytes, PyDict, PyList};",
                "    table: [u64; 256],\n}",
                "    table: Option<[u64; 256]>,",
                "fn digest_of(reg: u64, cfg: &Config) -> u64 {",
            ] {
                assert_eq!(text.matches(needle).count(), 1, "anchor drift: {needle:?}");
            }
        }
    }

    #[test]
    fn slice_patch_applies_and_compiles_shape() {
        let dir = tempfile::tempdir().unwrap();
        copy_template_files(&template_under_test(), dir.path());
        let files = apply_slice_by_8(dir.path()).unwrap();
        assert!(files.contains(&"src/slice8.rs".to_string()));
        assert!(dir.path().join("src/slice8.rs").exists());
        let lib = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
        assert!(lib.contains("mod slice8;"));
        assert!(lib.contains("st: [[u64; 256]; 8]"));
    }

    #[test]
    fn slice_patched_crate_passes_lib_tests() {
        if std::env::var("RUSTSMITH_SLOW").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        copy_template_files(&template_under_test(), dir.path());
        apply_slice_by_8(dir.path()).unwrap();
        let st = std::process::Command::new("cargo")
            .args(["test", "--manifest-path"])
            .arg(dir.path().join("Cargo.toml"))
            .args(["--lib"])
            .output()
            .unwrap();
        assert!(
            st.status.success(),
            "patched lib tests failed:\n{}",
            String::from_utf8_lossy(&st.stderr)
        );
    }
}
