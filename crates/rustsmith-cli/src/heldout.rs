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

/// Dispatch by fixture. crc keeps the exact suite above; strsimpy gets
/// property + agreement + pinned-absolute tests on inputs disjoint from the
/// visible suite (visible uses eat/eating, AGCAT/GAC, car/bar, long sentences,
/// CJK 上海; pins below use disjoint vectors verified against the original).
pub fn generate_heldout_tests_for(fixture: &str) -> Vec<(String, String)> {
    match fixture {
        "strsimpy" => generate_heldout_tests_strsimpy(),
        _ => generate_heldout_tests(),
    }
}

pub fn generate_heldout_tests_strsimpy() -> Vec<(String, String)> {
    vec![
        (
            "test_heldout_edit.py".into(),
            r#""""Held-out: edit-distance properties + cross-metric agreement (host-only)."""
from strsimpy.levenshtein import Levenshtein
from strsimpy.damerau import Damerau
from strsimpy.optimal_string_alignment import OptimalStringAlignment
from strsimpy.weighted_levenshtein import WeightedLevenshtein
from strsimpy.sift4 import SIFT4


def test_symmetry():
    for m in [Levenshtein(), Damerau(), OptimalStringAlignment(), WeightedLevenshtein()]:
        assert m.distance("kitten", "sitting") == m.distance("sitting", "kitten")
    assert SIFT4().distance("kitten", "sitting") == SIFT4().distance("sitting", "kitten")


def test_empty_is_length():
    for m in [Levenshtein(), Damerau(), WeightedLevenshtein()]:
        assert m.distance("", "zxqw") == 4
        assert m.distance("zxqw", "") == 4
    # Original quirk (pinned): OSA returns 0.0 on empty, not the length.
    assert OptimalStringAlignment().distance("", "zxqw") == 0.0
    assert OptimalStringAlignment().distance("zxqw", "") == 0.0


def test_osa_agrees_levenshtein_single_edit():
    assert OptimalStringAlignment().distance("abcd", "abce") == Levenshtein().distance("abcd", "abce") == 1


def test_identity_sensitive_repeats():
    # Equal-but-not-identical objects must agree (catches identity-keyed caches).
    a = "kitten"
    b = "".join(["k", "i", "t", "t", "e", "n"])
    assert a == b and (a is not b)
    assert Levenshtein().distance(a, "sitting") == 3
    assert Levenshtein().distance(b, "sitting") == 3


def test_unicode_property():
    a, b = "上海市", "上海"
    assert Levenshtein().distance(a, b) == Levenshtein().distance(b, a)
    assert Damerau().distance(a, b) >= 0
"#
            .to_string(),
        ),
        (
            "test_heldout_norm.py".into(),
            r#""""Held-out: normalized metrics self-agreement + adversarial shapes (host-only)."""
from strsimpy.jaro_winkler import JaroWinkler
from strsimpy.normalized_levenshtein import NormalizedLevenshtein
from strsimpy.cosine import Cosine
from strsimpy.jaccard import Jaccard
from strsimpy.ngram import NGram
from strsimpy.metric_lcs import MetricLCS
from strsimpy.sorensen_dice import SorensenDice
from strsimpy.overlap_coefficient import OverlapCoefficient
from strsimpy.qgram import QGram
from strsimpy.longest_common_subsequence import LongestCommonSubsequence


def test_self_similarity_is_one():
    s = "zxqw"
    assert NormalizedLevenshtein().similarity(s, s) == 1.0
    assert JaroWinkler().similarity(s, s) == 1.0
    assert SorensenDice(2).similarity(s, s) == 1.0
    assert Jaccard(2).similarity(s, s) == 1.0
    assert OverlapCoefficient(2).similarity(s, s) == 1.0


def test_self_distance_is_zero():
    s = "zxqw"
    assert NormalizedLevenshtein().distance(s, s) == 0.0
    assert MetricLCS().distance(s, s) == 0.0
    assert NGram(2).distance(s, s) == 0.0
    assert Cosine(2).distance(s, s) == 0.0
    assert LongestCommonSubsequence().distance(s, s) == 0


def test_adversarial_shapes():
    for m in [NormalizedLevenshtein(), MetricLCS(), NGram(2), Cosine(2), QGram(3)]:
        assert m.distance("", "") == 0.0
        v = m.distance("a", "abcdefghij")
        assert v >= 0.0


def test_lcs_length_property():
    assert LongestCommonSubsequence().length("zxqw", "zxqw") == 4
    assert LongestCommonSubsequence().length("", "zxqw") == 0
"#
            .to_string(),
        ),
        (
            "test_heldout_pinned.py".into(),
            r#""""Held-out: pinned absolute values on disjoint inputs (host-only)."""
from strsimpy.levenshtein import Levenshtein
from strsimpy.damerau import Damerau
from strsimpy.jaro_winkler import JaroWinkler
from strsimpy.normalized_levenshtein import NormalizedLevenshtein
from strsimpy.cosine import Cosine
from strsimpy.jaccard import Jaccard
from strsimpy.ngram import NGram
from strsimpy.optimal_string_alignment import OptimalStringAlignment
from strsimpy.longest_common_subsequence import LongestCommonSubsequence
from strsimpy.metric_lcs import MetricLCS
from strsimpy.qgram import QGram
from strsimpy.sorensen_dice import SorensenDice
from strsimpy.overlap_coefficient import OverlapCoefficient
from strsimpy.weighted_levenshtein import WeightedLevenshtein
from strsimpy.sift4 import SIFT4


def test_pinned_kitten():
    assert Levenshtein().distance("kitten", "sitting") == 3
    assert SIFT4().distance("kitten", "sitting") == 3


def test_pinned_transposition():
    assert Damerau().distance("abcd", "acbd") == 1
    assert OptimalStringAlignment().distance("abcd", "acbd") == 1


def test_pinned_jaro():
    assert JaroWinkler().similarity("martha", "marhta") == 0.9611111111111111
    assert JaroWinkler(threshold=0.7).get_threshold() == 0.7


def test_pinned_normalized():
    assert NormalizedLevenshtein().distance("abc", "abd") == 0.3333333333333333
    assert MetricLCS().distance("abc", "abd") == 0.33333333333333337
    assert NGram(2).distance("abcd", "abce") == 0.125


def test_pinned_shingle():
    assert Cosine(2).distance("hello world", "hello there") == 0.4522774424948339
    assert Jaccard(2).similarity("abc", "abd") == 0.3333333333333333
    assert QGram(3).distance("abcd", "abce") == 2
    assert SorensenDice(2).similarity("abc", "abd") == 0.5
    assert OverlapCoefficient(2).similarity("abc", "abd") == 0.5


def test_pinned_misc():
    assert LongestCommonSubsequence().length("abcde", "abce") == 4
    assert WeightedLevenshtein().distance("abc", "abd") == 1.0
"#
            .to_string(),
        ),
    ]
}
