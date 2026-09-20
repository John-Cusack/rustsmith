//! Host-only held-out TEST suite generator (M3 template-based).
//! Property + differential vs original + fuzz scaffolds from the public API,
//! plus pinned absolute values on disjoint inputs (the load-bearing element:
//! consistent garbage passes agreement tests; only absolute pins catch it).
//! Never copied into run/grading containers; never shown to workers/council.

pub fn generate_heldout_tests() -> Vec<(String, String)> {
    vec![
        (
            "test_heldout_calculator.py".into(),
            r#""""Held-out: Calculator property + differential (host-only)."""
from crc import Calculator, Crc8


def test_property_incremental_equals_oneshot():
    calc = Calculator(Crc8.CCITT)
    data = b"123456789"
    assert calc.checksum(data) == _incremental(Crc8.CCITT, [data[:4], data[4:]])


def test_property_empty_matches_init_xor_final():
    cfg = Crc8.CCITT.value
    calc = Calculator(Crc8.CCITT)
    assert calc.checksum(b"") == (cfg.init_value ^ cfg.final_xor_value) & _mask(cfg.width)


def test_differential_known_vectors():
    calc = Calculator(Crc8.CCITT)
    assert calc.checksum(b"123456789") == 0xF4
    assert calc.checksum(b"") == 0x00


def test_fuzz_bytes_never_raises():
    calc = Calculator(Crc8.CCITT)
    for n in [0, 1, 7, 64, 1024]:
        calc.checksum(bytes((i * 31) & 0xFF for i in range(n)))


def _incremental(cfg, chunks):
    from crc import Register
    r = Register(cfg)
    r.init()
    for c in chunks:
        r.update(c)
    return r.digest()


def _mask(width):
    return (1 << width) - 1 if width < 64 else (1 << 64) - 1
"#
            .to_string(),
        ),
        (
            "test_heldout_register.py".into(),
            r#""""Held-out: Register/TableBasedRegister agreement + adversarial shapes (host-only)."""
from crc import Register, TableBasedRegister, Crc8, Crc16, Crc32


def test_register_agrees_table():
    for cfg in [Crc8.CCITT, Crc16.XMODEM, Crc32.CRC32]:
        for data in [b"", b"a", b"123456789", bytes(256), bytes([0] * 64), bytes(range(256))]:
            r1, r2 = Register(cfg), TableBasedRegister(cfg)
            r1.init()
            r2.init()
            r1.update(data)
            r2.update(data)
            assert r1.digest() == r2.digest(), (cfg, data[:8])


def test_adversarial_shapes():
    from crc import Calculator
    calc = Calculator(Crc8.CCITT)
    assert isinstance(calc.checksum(b"\x00" * 128), int)
    assert isinstance(calc.checksum(bytes([0xFF] * 128)), int)
    assert isinstance(calc.checksum(bytes(range(256)) * 4), int)


def test_identity_sensitive_repeats():
    # Equal-but-not-identical inputs must agree (catches identity-keyed caches).
    from crc import Calculator
    calc = Calculator(Crc8.CCITT)
    a = b"123456789"
    b = bytes([49, 50, 51, 52, 53, 54, 55, 56, 57])
    assert a == b and (a is not b)
    assert calc.checksum(a) == calc.checksum(b)
"#
            .to_string(),
        ),
        (
            "test_heldout_pinned.py".into(),
            r#""""Held-out: pinned absolute values on disjoint inputs (host-only)."""
from crc import Calculator, Crc8, Crc16, Crc32


def test_pinned_abc():
    assert Calculator(Crc8.CCITT).checksum(b"abc") == 0x5F
    assert Calculator(Crc8.BLUETOOTH).checksum(b"abc") == 0xAA
    assert Calculator(Crc16.XMODEM).checksum(b"abc") == 0x9DD6
    assert Calculator(Crc16.MODBUS).checksum(b"abc") == 0x5749
    assert Calculator(Crc32.CRC32).checksum(b"abc") == 0x352441C2


def test_pinned_range():
    assert Calculator(Crc8.CCITT).checksum(bytes(range(256))) == 0x14
    assert Calculator(Crc16.XMODEM).checksum(bytes(range(256))) == 0x7E55
    assert Calculator(Crc32.CRC32).checksum(bytes(range(256))) == 0x29058C73


def test_pinned_fox():
    assert Calculator(Crc8.CCITT).checksum(b"The quick brown fox jumps over the lazy dog") == 0xC1
    assert Calculator(Crc32.CRC32).checksum(b"The quick brown fox jumps over the lazy dog") == 0x414FA339
"#
            .to_string(),
        ),
    ]
}
