//! Pure-Rust CRC core for the `crc` converted project.
//!
//! Behavior-identical port of `Nicoretti/crc` `src/crc/_crc.py`: same catalogs,
//! same vectors. No redesign: byte-at-a-time table lookup ships here
//! (slice-by-8 is a Stage-2/M6 candidate, applied on top of this file).
//!
//! This crate is intentionally independent of Python: it has no `pyo3`
//! dependency, builds as an `rlib`, and its unit tests run under plain
//! `cargo test` (and miri). The Python extension (`crc._crc`, built from the
//! parent `crc-rust` crate) calls this same core, so Rust consumers and
//! Python consumers share one implementation.
//!
//! License: BSD-2-Clause (preserved from the original; see NOTICE).

// ---------------------------------------------------------------------------
// Core CRC arithmetic (no Python API): deterministic, miri-testable.
// ---------------------------------------------------------------------------

/// Width mask: low `width` bits set. Width is always 8..=64 on this fixture.
pub fn mask(width: u8) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

pub fn reflect_byte(b: u8) -> u8 {
    b.reverse_bits()
}

/// Bit-reversal of the low `width` bits.
pub fn reflect_val(mut v: u64, width: u8) -> u64 {
    let mut out = 0u64;
    for _ in 0..width {
        out = (out << 1) | (v & 1);
        v >>= 1;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub width: u8,
    pub poly: u64,
    pub init: u64,
    pub xorout: u64,
    pub refin: bool,
    pub refout: bool,
}

pub fn topbit(cfg: &Config) -> u64 {
    1u64 << (cfg.width - 1)
}

pub fn bitmask(cfg: &Config) -> u64 {
    mask(cfg.width)
}

/// Bit-by-bit update of one byte (refin already applied by the caller).
pub fn process_byte_bit(mut reg: u64, cfg: &Config, byte: u8) -> u64 {
    let m = bitmask(cfg);
    reg ^= (byte as u64) << (cfg.width - 8);
    reg &= m;
    for _ in 0..8 {
        if reg & topbit(cfg) != 0 {
            reg = ((reg << 1) ^ cfg.poly) & m;
        } else {
            reg = (reg << 1) & m;
        }
    }
    reg
}

/// Table update of one byte (refin already applied by the caller).
pub fn process_byte_table(mut reg: u64, cfg: &Config, table: &[u64; 256], byte: u8) -> u64 {
    let m = bitmask(cfg);
    let index = (byte ^ ((reg >> (cfg.width - 8)) as u8)) as usize;
    reg = (table[index] ^ (reg << 8)) & m;
    reg
}

pub fn update_bit(reg: u64, cfg: &Config, data: &[u8]) -> u64 {
    let mut r = reg;
    for &b in data {
        let byte = if cfg.refin { reflect_byte(b) } else { b };
        r = process_byte_bit(r, cfg, byte);
    }
    r
}

pub fn update_table(reg: u64, cfg: &Config, table: &[u64; 256], data: &[u8]) -> u64 {
    let mut r = reg;
    for &b in data {
        let byte = if cfg.refin { reflect_byte(b) } else { b };
        r = process_byte_table(r, cfg, table, byte);
    }
    r
}

pub fn digest_of(reg: u64, cfg: &Config) -> u64 {
    let v = if cfg.refout {
        reflect_val(reg, cfg.width)
    } else {
        reg
    };
    (v ^ cfg.xorout) & bitmask(cfg)
}

pub fn reverse_of(reg: u64, cfg: &Config) -> u64 {
    // Byte-wise reversal of the width/8 bytes, mirroring BasicRegister.reverse.
    let nbytes = (cfg.width / 8) as usize;
    let mut out = 0u64;
    for index in 0..nbytes {
        let byte = ((reg >> (index * 8)) & 0xFF) as u8;
        out |= (reflect_byte(byte) as u64) << ((nbytes - 1 - index) * 8);
    }
    out & bitmask(cfg)
}

pub fn create_table(width: u8, poly: u64) -> [u64; 256] {
    // Python builds tables from `Configuration(width, polynomial)` DEFAULTS
    // (init 0, xorout 0, no reflection) regardless of the register's config.
    let cfg = Config {
        width,
        poly,
        init: 0,
        xorout: 0,
        refin: false,
        refout: false,
    };
    let mut t = [0u64; 256];
    for (i, slot) in t.iter_mut().enumerate() {
        let reg = update_bit(cfg.init & bitmask(&cfg), &cfg, &[i as u8]);
        *slot = digest_of(reg, &cfg);
    }
    t
}
/// `0x{:0NdX}` with `N = (width+3)//4`, matching `_generate_template`.
pub fn render_template(width: u64) -> String {
    let digits = width.div_ceil(4) as usize;
    format!("0x{{:0{digits}X}}")
}
pub fn format_value(template_digits: usize, v: u64) -> String {
    format!("0x{v:0width$X}", width = template_digits)
}
// ---------------------------------------------------------------------------

pub fn crc8_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CCITT", Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x00, refin: false, refout: false }),
        ("SAEJ1850", Config { width: 8, poly: 0x1D, init: 0xFF, xorout: 0xFF, refin: false, refout: false }),
        ("SAEJ1850_ZERO", Config { width: 8, poly: 0x1D, init: 0x00, xorout: 0x00, refin: false, refout: false }),
        ("AUTOSAR", Config { width: 8, poly: 0x2F, init: 0xFF, xorout: 0xFF, refin: false, refout: false }),
        ("BLUETOOTH", Config { width: 8, poly: 0xA7, init: 0x00, xorout: 0x00, refin: true, refout: true }),
        ("MAXIM_DOW", Config { width: 8, poly: 0x31, init: 0, xorout: 0, refin: true, refout: true }),
        ("ITU", Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x55, refin: false, refout: false }),
        ("ROHC", Config { width: 8, poly: 0x07, init: 0xFF, xorout: 0x00, refin: true, refout: true }),
    ]
}

pub fn crc16_members() -> Vec<(&'static str, Config)> {
    vec![
        ("XMODEM", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: false, refout: false }),
        ("GSM", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0xFFFF, refin: false, refout: false }),
        ("PROFIBUS", Config { width: 16, poly: 0x1DCF, init: 0xFFFF, xorout: 0xFFFF, refin: false, refout: false }),
        ("MODBUS", Config { width: 16, poly: 0x8005, init: 0xFFFF, xorout: 0x0000, refin: true, refout: true }),
        ("IBM_3740", Config { width: 16, poly: 0x1021, init: 0xFFFF, xorout: 0x0000, refin: false, refout: false }),
        ("KERMIT", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: true, refout: true }),
        ("IBM", Config { width: 16, poly: 0x8005, init: 0x0000, xorout: 0x0000, refin: true, refout: true }),
        ("MAXIM", Config { width: 16, poly: 0x8005, init: 0x0000, xorout: 0xFFFF, refin: true, refout: true }),
        ("USB", Config { width: 16, poly: 0x8005, init: 0xFFFF, xorout: 0xFFFF, refin: true, refout: true }),
        ("X25", Config { width: 16, poly: 0x1021, init: 0xFFFF, xorout: 0xFFFF, refin: true, refout: true }),
        ("DNP", Config { width: 16, poly: 0x3D65, init: 0x0000, xorout: 0xFFFF, refin: true, refout: true }),
    ]
}

pub fn crc32_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CRC32", Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true }),
        ("AUTOSAR", Config { width: 32, poly: 0xF4ACFB13, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true }),
        ("BZIP2", Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: false, refout: false }),
        ("POSIX", Config { width: 32, poly: 0x04C11DB7, init: 0x00000000, xorout: 0xFFFFFFFF, refin: false, refout: false }),
    ]
}

pub fn crc64_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CRC64", Config { width: 64, poly: 0x42F0E1EBA9EA3693, init: 0x0000000000000000, xorout: 0x0000000000000000, refin: false, refout: false }),
    ]
}

// ---------------------------------------------------------------------------
// Pure-Rust tests (miri-clean subset: no Python API touched).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod core_tests {
    use super::*;

    #[test]
    fn masks_and_reflects() {
        assert_eq!(mask(8), 0xFF);
        assert_eq!(mask(64), u64::MAX);
        assert_eq!(reflect_byte(0x80), 0x01);
        assert_eq!(reflect_byte(0xF0), 0x0F);
        assert_eq!(reflect_val(0x01, 8), 0x80);
        assert_eq!(render_template(8), "0x{:02X}");
        assert_eq!(render_template(1), "0x{:01X}");
        assert_eq!(render_template(64), "0x{:016X}");
    }

    #[test]
    fn bit_and_table_agree_on_vectors() {
        let cfg = Config {
            width: 8,
            poly: 0x07,
            init: 0,
            xorout: 0,
            refin: false,
            refout: false,
        };
        let table = create_table(cfg.width, cfg.poly);
        assert_eq!(&table[..4], &[0x00, 0x07, 0x0E, 0x09]);
        for data in [b"".as_slice(), b"123456789", b"Hello World!"] {
            let a = digest_of(update_bit(0, &cfg, data), &cfg);
            let b = digest_of(update_table(0, &cfg, &table, data), &cfg);
            assert_eq!(a, b);
        }
        assert_eq!(digest_of(update_bit(0, &cfg, b"123456789"), &cfg), 0xF4);
    }

    #[test]
    fn bit_and_table_agree_with_init_xor_reflection() {
        // Regression: tables must be built from width+poly defaults, so bit and
        // table paths agree for configs with nonzero init/xorout/reflection.
        let cfgs = [
            Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x55, refin: false, refout: false },
            Config { width: 8, poly: 0x07, init: 0xFF, xorout: 0x00, refin: true, refout: true },
            Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: false, refout: true },
            Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true },
        ];
        for cfg in cfgs {
            let table = create_table(cfg.width, cfg.poly);
            for data in [b"".as_slice(), b"123456789", b"Hello World!"] {
                let init = cfg.init & mask(cfg.width);
                let a = digest_of(update_bit(init, &cfg, data), &cfg);
                let b = digest_of(update_table(init, &cfg, &table, data), &cfg);
                assert_eq!(a, b, "cfg={cfg:?} data={data:?}");
            }
        }
    }

    #[test]
    fn reflected_catalog_spot() {
        // MODBUS is refin+refout; empty input => init ^ xorout masked.
        let cfg = Config {
            width: 16,
            poly: 0x8005,
            init: 0xFFFF,
            xorout: 0x0000,
            refin: true,
            refout: true,
        };
        assert_eq!(digest_of(cfg.init & mask(16), &cfg), 0xFFFF);
    }
}
