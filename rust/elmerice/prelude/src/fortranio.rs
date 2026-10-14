// Fortran I/O compatibility surface for the elmerice/ PROGRAM ports.
//
// Fitted byte-exact against gfortran 13.3.0 (fit scripts in firstmate-data
// rs-elmer-lane-ice/fit_fortran_io*.py):
// - Formatted WRITE: fields concatenated exactly, no leading blank.
// - List-directed record: items concatenated with NO separators and NO
//   record blank: INTEGER(4) = I12 (right-justified, min width 12);
//   REAL(8) F-form = right-justify core in 21 + 5 trailing blanks (26);
//   REAL(8) E-form (incl. Infinity/NaN) = right-justify core in 26;
//   CHARACTER = raw text, unpadded.
// - REAL(8) list core: 17 significant digits, correctly rounded; F-form iff
//   floor(log10|x|) in [-1, 16], else E-form `d.dddE+XXX` (3-digit exp).
// - E22.15: sign-or-blank + `0.` + 15 digits + `E+XX`, or `+XXX` with the E
//   dropped for 3-digit exponents; zero prints `0.000000000000000E+00`.
//
// Heap discipline (merge-gate load-bearing, same contract as the matc and
// elmergrid slices): this module uses NO Rust allocator and NO core::fmt —
// every routine writes into a caller-provided `*mut c_char` buffer and
// returns the byte length. Exact decimal expansion is a hand-rolled
// base-1e9 big-int (no float formatting anywhere), so unit members carry
// no Rust-namespaced U refs beyond panicking/personality (asserted by the
// lane's nm gate script). Every routine is `#[inline(always)]` so the
// prelude member defines zero text symbols (same as eg-prelude) and unit
// archives stay self-contained. Dynamic input storage is libc
// malloc/realloc/free, freed by the caller. Covered by the differential
// fuzz (firstmate-data rs-elmer-lane-ice/fuzz_fortran_io.py).
use crate::{c_char, c_double, size_t};

// ---- tiny byte helpers ----------------------------------------------------

#[inline(always)]
unsafe fn out_byte(out: *mut c_char, pos: usize, b: u8) {
    *out.add(pos) = b as c_char;
}

#[inline(always)]
unsafe fn out_fill(out: *mut c_char, pos: usize, n: usize, b: u8) {
    for i in 0..n {
        *out.add(pos + i) = b as c_char;
    }
}

#[inline(always)]
unsafe fn out_copy(out: *mut c_char, pos: usize, src: &[u8]) {
    for (i, &b) in src.iter().enumerate() {
        *out.add(pos + i) = b as c_char;
    }
}

// Manual u64 -> decimal (no pad). Returns length.
#[inline(always)]
unsafe fn emit_u64(v: u64, out: *mut c_char) -> usize {
    if v == 0 {
        out_byte(out, 0, b'0');
        return 1;
    }
    let mut u = v;
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while u > 0 {
        tmp[n] = (u % 10) as u8 + b'0';
        u /= 10;
        n += 1;
    }
    for i in (0..n).rev() {
        out_byte(out, n - 1 - i, tmp[i]);
    }
    n
}

// Manual i64 -> decimal (no pad). Returns length.
#[inline(always)]
unsafe fn emit_i64(v: i64, out: *mut c_char) -> usize {
    if v == 0 {
        out_byte(out, 0, b'0');
        return 1;
    }
    if v < 0 {
        out_byte(out, 0, b'-');
        return 1 + emit_u64(v.unsigned_abs(), out.add(1));
    }
    emit_u64(v as u64, out)
}

// ---- exact decimal expansion (hand-rolled base-1e9 big-int) ----------------
// f64 = m * 2^e exactly (m <= 2^53). All arithmetic below is exact integer
// math; digit selection therefore rounds the TRUE binary value (fuzz-pinned
// incl. ties). Deliberately O(n^2)-simple: the lane's corpora are small;
// production throughput is a non-goal here.

const LIMBS: usize = 96;
const BASE: u64 = 1_000_000_000;

struct Big {
    d: [u64; LIMBS],
    n: usize,
}

#[inline(always)]
fn big_zero() -> Big {
    Big {
        d: [0u64; LIMBS],
        n: 0,
    }
}

#[inline(always)]
fn big_from_u64(v: u64) -> Big {
    let mut b = big_zero();
    let mut u = v;
    while u > 0 {
        b.d[b.n] = u % BASE;
        b.n += 1;
        u /= BASE;
    }
    if b.n == 0 {
        b.n = 1;
    }
    b
}

// Multiply by small m (m <= 10 keeps every intermediate in u64).
#[inline(always)]
fn big_mul_small(b: &mut Big, m: u64) {
    let mut carry = 0u64;
    for i in 0..b.n {
        let cur = b.d[i] * m + carry;
        b.d[i] = cur % BASE;
        carry = cur / BASE;
    }
    while carry > 0 {
        b.d[b.n] = carry % BASE;
        carry /= BASE;
        b.n += 1;
    }
}

// Full exact decimal string of b into `out`. Returns length.
#[inline(always)]
fn big_decimal(b: &Big, out: &mut [u8; 800]) -> usize {
    if b.n == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut pos = 0;
    // Most significant limb, unpadded.
    let mut ms = b.d[b.n - 1];
    let mut tmp = [0u8; 16];
    let mut n = 0;
    if ms == 0 {
        tmp[0] = b'0';
        n = 1;
    } else {
        while ms > 0 {
            tmp[n] = (ms % 10) as u8 + b'0';
            ms /= 10;
            n += 1;
        }
    }
    for i in (0..n).rev() {
        out[pos] = tmp[i];
        pos += 1;
    }
    // Remaining limbs, zero-padded to 9.
    for li in (0..b.n - 1).rev() {
        let mut v = b.d[li];
        let mut digs = [b'0'; 9];
        for k in (0..9).rev() {
            digs[k] = (v % 10) as u8 + b'0';
            v /= 10;
        }
        for b in digs.iter() {
            out[pos] = *b;
            pos += 1;
        }
    }
    pos
}

// Significant E-digits of |x| (mantissa in [1,10)) into `digits` (cap 800;
// every exact terminating expansion fits: integers <= 309 digits, P <= 768).
// Returns (significant length, base-10 exponent): the value's significant
// digits are digits[0..total] with digits[0] != 0. Exact: no rounding
// anywhere in the digit production; the caller rounds from the long tail.
#[inline(always)]
fn exact_e_digits(x: f64, digits: &mut [u8; 800]) -> (usize, i32) {
    let bits = x.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let (m, e): (u64, i32) = if raw_exp == 0 {
        (bits & 0xfffffffffffff, -1074)
    } else {
        ((bits & 0xfffffffffffff) | 0x10000000000000, raw_exp - 1023 - 52)
    };
    let mut dec = [0u8; 800];
    if e >= 0 {
        let mut b = big_from_u64(m);
        for _ in 0..e {
            big_mul_small(&mut b, 2);
        }
        let L = big_decimal(&b, &mut dec);
        digits[..L].copy_from_slice(&dec[..L]);
        (L, L as i32 - 1)
    } else {
        let k = (-e) as usize;
        let mut p = big_from_u64(m);
        for _ in 0..k {
            big_mul_small(&mut p, 5);
        }
        // value = P / 10^k.
        let L = big_decimal(&p, &mut dec);
        if L > k {
            let int_len = L - k;
            digits[..L].copy_from_slice(&dec[..L]);
            (L, int_len as i32 - 1)
        } else {
            // Pure fraction: (k - L) implicit leading zeros, then S.
            // Only S is materialized; the exponent skips the zeros.
            let zeros = k - L;
            digits[..L].copy_from_slice(&dec[..L]);
            (L, -(zeros as i32 + 1))
        }
    }
}

// Round significant digits to `keep` (half-even ties). `total` = produced
// digits (100). Returns exponent bump.
#[inline(always)]
fn round_sig(d: &mut [u8; 800], keep: usize, total: usize) -> i32 {
    let mut seen = false;
    let mut first = 0u8;
    let mut rest = false;
    // NOTE: keep >= total (exact expansion shorter than the kept window)
    // means no tail exists — plain indexing would panic on the empty
    // range, so the scan is guarded (a clippy-driven iterator slipped this
    // in once; the fuzz caught it).
    if keep < total {
        for (j, &b) in d[keep..total].iter().enumerate() {
            if b != b'0' {
                seen = true;
                if j == 0 {
                    first = b;
                } else {
                    rest = true;
                }
            }
        }
    }
    let up = if !seen {
        false
    } else if first > b'5' || (first == b'5' && rest) {
        true
    } else if first < b'5' {
        false
    } else {
        (d[keep - 1] - b'0') % 2 == 1
    };
    if !up {
        return 0;
    }
    let mut i = keep;
    loop {
        if i == 0 {
            d[0] = b'1';
            d[1..keep].fill(b'0');
            return 1;
        }
        i -= 1;
        if d[i] == b'9' {
            d[i] = b'0';
        } else {
            d[i] += 1;
            return 0;
        }
    }
}

/// Significant digit k with exact-zero padding past the produced length
/// (all expansions terminate, so this padding is exact, not approximate).
#[inline(always)]
fn dig(d: &[u8; 800], total: usize, k: usize) -> u8 {
    if k < total {
        d[k]
    } else {
        b'0'
    }
}

// ---- list-directed REAL(8) core --------------------------------------------

/// Emit the G0.17 core into `out`. Returns (len, is_eform).
#[inline(always)]
pub unsafe fn g0_core(x: f64, out: *mut c_char) -> (usize, bool) {
    if x.is_nan() {
        out_copy(out, 0, b"NaN");
        return (3, true);
    }
    if x.is_infinite() {
        if x.is_sign_negative() {
            out_copy(out, 0, b"-Infinity");
            return (9, true);
        }
        out_copy(out, 0, b"Infinity");
        return (8, true);
    }
    let neg = x.is_sign_negative();
    if x == 0.0 {
        let mut pos = 0;
        if neg {
            out_byte(out, 0, b'-');
            pos = 1;
        }
        out_byte(out, pos, b'0');
        out_byte(out, pos + 1, b'.');
        out_fill(out, pos + 2, 16, b'0');
        return (pos + 18, false);
    }
    let mut d = [0u8; 800];
    let (total, exp) = exact_e_digits(x, &mut d);
    let bump = round_sig(&mut d, 17, total);
    let exp = exp + bump;
    let mut pos = 0;
    if neg {
        out_byte(out, 0, b'-');
        pos = 1;
    }
    if (-1..=16).contains(&exp) {
        let int_len = (exp + 1) as usize;
        if int_len == 0 {
            out_byte(out, pos, b'0');
            pos += 1;
        } else {
            for k in 0..int_len {
                out_byte(out, pos, dig(&d, total, k));
                pos += 1;
            }
        }
        out_byte(out, pos, b'.');
        pos += 1;
        for k in int_len..17 {
            out_byte(out, pos, dig(&d, total, k));
            pos += 1;
        }
        (pos, false)
    } else {
        out_byte(out, pos, dig(&d, total, 0));
        out_byte(out, pos + 1, b'.');
        for k in 1..17 {
            out_byte(out, pos + 1 + k, dig(&d, total, k));
        }
        pos += 18;
        out_byte(out, pos, b'E');
        out_byte(out, pos + 1, if exp < 0 { b'-' } else { b'+' });
        let ae = exp.unsigned_abs();
        out_byte(out, pos + 2, (ae / 100) as u8 + b'0');
        out_byte(out, pos + 3, ((ae / 10) % 10) as u8 + b'0');
        out_byte(out, pos + 4, (ae % 10) as u8 + b'0');
        (pos + 5, true)
    }
}

/// List-directed REAL(8) item field (26 bytes): F = rjust(21)+5 blanks,
/// E-form = rjust(26).
#[inline(always)]
pub unsafe fn g0_f64_field(x: f64, out: *mut c_char) -> usize {
    let mut core = [0u8; 32];
    let (n, is_e) = g0_core(x, core.as_mut_ptr() as *mut c_char);
    if is_e {
        out_fill(out, 0, 26 - n, b' ');
        out_copy(out, 26 - n, &core[..n]);
        26
    } else {
        out_fill(out, 0, 21 - n, b' ');
        out_copy(out, 21 - n, &core[..n]);
        out_fill(out, 21, 5, b' ');
        26
    }
}

/// List-directed INTEGER item field: I12 (min width 12).
#[inline(always)]
pub unsafe fn g0_int_field(v: i64, out: *mut c_char) -> usize {
    let mut tmp = [0u8; 24];
    let n = emit_i64(v, tmp.as_mut_ptr() as *mut c_char);
    let w = if n > 12 { n } else { 12 };
    out_fill(out, 0, w - n, b' ');
    out_copy(out, w - n, &tmp[..n]);
    w
}

/// I-field core (right-justified, min width w). Returns length.
#[inline(always)]
pub unsafe fn i_field(v: i64, w: usize, out: *mut c_char) -> usize {
    let mut tmp = [0u8; 24];
    let n = emit_i64(v, tmp.as_mut_ptr() as *mut c_char);
    let w = if n > w { n } else { w };
    out_fill(out, 0, w - n, b' ');
    out_copy(out, w - n, &tmp[..n]);
    w
}

/// Iw.m core (no blank padding): min `m` digits. Returns length.
#[inline(always)]
pub unsafe fn iw_m(v: i64, m: usize, out: *mut c_char) -> usize {
    let mut pos = 0;
    let mag = v.unsigned_abs();
    let mut tmp = [0u8; 24];
    let n = emit_u64(mag, tmp.as_mut_ptr() as *mut c_char);
    if v < 0 {
        out_byte(out, 0, b'-');
        pos = 1;
    }
    for _ in n..m {
        out_byte(out, pos, b'0');
        pos += 1;
    }
    out_copy(out, pos, &tmp[..n]);
    pos + n
}

/// Fortran E22.15 field: exactly 22 bytes for finite values.
#[inline(always)]
pub unsafe fn e22_15(x: f64, out: *mut c_char) -> usize {
    if x.is_nan() {
        out_fill(out, 0, 19, b' ');
        out_copy(out, 19, b"NaN");
        return 22;
    }
    if x.is_infinite() {
        if x.is_sign_negative() {
            out_fill(out, 0, 13, b' ');
            out_copy(out, 13, b"-Infinity");
        } else {
            out_fill(out, 0, 14, b' ');
            out_copy(out, 14, b"Infinity");
        }
        return 22;
    }
    out_byte(out, 0, if x.is_sign_negative() { b'-' } else { b' ' });
    if x == 0.0 {
        out_byte(out, 1, b'0');
        out_byte(out, 2, b'.');
        out_fill(out, 3, 15, b'0');
        out_byte(out, 18, b'E');
        out_byte(out, 19, b'+');
        out_byte(out, 20, b'0');
        out_byte(out, 21, b'0');
        return 22;
    }
    let mut d = [0u8; 800];
    let (total, exp) = exact_e_digits(x, &mut d);
    let bump = round_sig(&mut d, 15, total);
    let e = exp + 1 + bump;
    out_byte(out, 1, b'0');
    out_byte(out, 2, b'.');
    for k in 0..15 {
        out_byte(out, 3 + k, dig(&d, total, k));
    }
    if e.abs() > 99 {
        out_byte(out, 18, if e < 0 { b'-' } else { b'+' });
        let ae = e.unsigned_abs();
        out_byte(out, 19, (ae / 100) as u8 + b'0');
        out_byte(out, 20, ((ae / 10) % 10) as u8 + b'0');
        out_byte(out, 21, (ae % 10) as u8 + b'0');
    } else {
        out_byte(out, 18, b'E');
        out_byte(out, 19, if e < 0 { b'-' } else { b'+' });
        let ae = e.unsigned_abs();
        out_byte(out, 20, ((ae / 10) % 10) as u8 + b'0');
        out_byte(out, 21, (ae % 10) as u8 + b'0');
    }
    22
}

// ---- readers (byte-slice based, no allocation) ------------------------------

/// Split one record (byte range) into list-directed raw tokens. Writes
/// (ptr,len) pairs into `toks` (cap entries); returns count, or usize::MAX
/// if the cap is exceeded (caller fail-louds; the lane's inputs never exceed
/// 64 tokens/record).
#[inline(always)]
pub unsafe fn list_tokens(
    rec: *const c_char,
    n: usize,
    toks: *mut (*const c_char, usize),
    cap: usize,
) -> usize {
    let mut nt = 0;
    let mut i = 0;
    while i < n {
        let b = *rec.add(i) as u8;
        if b == b' ' || b == b'\t' || b == b',' {
            i += 1;
            continue;
        }
        if b == b'/' {
            break;
        }
        if b == b'\'' || b == b'"' {
            let start = i;
            i += 1;
            while i < n {
                let c = *rec.add(i) as u8;
                if c == b {
                    if i + 1 < n && *rec.add(i + 1) as u8 == b {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            if nt >= cap {
                return usize::MAX;
            }
            *toks.add(nt) = (rec.add(start), i - start);
            nt += 1;
            continue;
        }
        let start = i;
        while i < n {
            let c = *rec.add(i) as u8;
            if c == b' ' || c == b'\t' || c == b',' || c == b'/' {
                break;
            }
            i += 1;
        }
        let slashed = i < n && *rec.add(i) as u8 == b'/';
        // Repeat count n*value?
        let mut star_at = usize::MAX;
        {
            let mut k = start;
            while k < i {
                if *rec.add(k) as u8 == b'*' {
                    star_at = k;
                    break;
                }
                k += 1;
            }
        }
        if star_at != usize::MAX && star_at > start {
            let cnt = parse_int_slice(rec.add(start), star_at - start);
            if cnt > 0 {
                let vlen = i - (star_at + 1);
                let mut c = 0;
                while c < cnt {
                    if nt >= cap {
                        return usize::MAX;
                    }
                    *toks.add(nt) = (rec.add(star_at + 1), vlen);
                    nt += 1;
                    c += 1;
                }
                if slashed {
                    break;
                }
                continue;
            }
        }
        if nt >= cap {
            return usize::MAX;
        }
        *toks.add(nt) = (rec.add(start), i - start);
        nt += 1;
        if slashed {
            break;
        }
    }
    nt
}

/// Parse an integer byte-slice (blanks ignored; empty -> 0).
#[inline(always)]
unsafe fn parse_int_slice(p: *const c_char, n: usize) -> i64 {
    let mut neg = false;
    let mut v: i64 = 0;
    let mut any = false;
    for i in 0..n {
        let b = *p.add(i) as u8;
        if b == b' ' || b == b'\t' {
            continue;
        }
        if b == b'-' && !any {
            neg = true;
            continue;
        }
        if b == b'+' && !any {
            continue;
        }
        if b.is_ascii_digit() {
            any = true;
            v = v * 10 + (b - b'0') as i64;
        } else {
            return 0;
        }
    }
    if !any {
        return 0;
    }
    if neg { -v } else { v }
}

/// Parse a token as f64 (D/d exponents mapped; uses strtod).
#[inline(always)]
pub unsafe fn tok_f64(p: *const c_char, n: usize, scratch: *mut c_char) -> f64 {
    extern "C" {
        fn strtod(s: *const c_char, end: *mut *mut c_char) -> c_double;
    }
    let m = if n < 127 { n } else { 127 };
    for i in 0..m {
        let mut b = *p.add(i) as u8;
        if b == b'D' || b == b'd' {
            b = b'E';
        }
        *scratch.add(i) = b as c_char;
    }
    *scratch.add(m) = 0;
    strtod(scratch as *const c_char, core::ptr::null_mut())
}

/// Parse a token as i64 (blank -> 0).
#[inline(always)]
pub unsafe fn tok_int(p: *const c_char, n: usize) -> i64 {
    parse_int_slice(p, n)
}

/// Fixed Iw field read: first `w` bytes, blanks ignored, empty -> 0.
#[inline(always)]
pub unsafe fn field_int(p: *const c_char, n: usize, w: usize) -> i64 {
    parse_int_slice(p, if n < w { n } else { w })
}

/// Copy a CHARACTER*n list token: first n bytes, blank-padded. A quoted
/// token has its quotes stripped (doubled quotes unescaped). Returns n
/// (capped by `cap`, fail-loud truncation is the caller's policy).
#[inline(always)]
pub unsafe fn tok_char(p: *const c_char, n: usize, out: *mut c_char, cap: usize) -> usize {
    let nn = if n < cap { n } else { cap };
    let mut q = 0u8;
    let mut src = p;
    let mut len = n;
    if n >= 2 {
        let b0 = *p as u8;
        if b0 == b'\'' || b0 == b'"' {
            q = b0;
            src = p.add(1);
            len = n - 1;
        }
    }
    let mut o = 0;
    let mut i = 0;
    while o < nn && i < len {
        let b = *src.add(i) as u8;
        if q != 0 && b == q {
            if i + 1 < len && *src.add(i + 1) as u8 == q {
                *out.add(o) = q as c_char;
                o += 1;
                i += 2;
                continue;
            } else {
                break;
            }
        }
        *out.add(o) = b as c_char;
        o += 1;
        i += 1;
    }
    while o < nn {
        *out.add(o) = b' ' as c_char;
        o += 1;
    }
    nn
}

// ---- sequential list-directed reader over malloc'd file text ----------------

pub const MAX_TOKS_PER_LINE: usize = 64;

pub struct SeqReader {
    pub text: *mut c_char,
    pub len: usize,
    pub lines: *mut usize,
    pub nlines: usize,
    pub line: usize,
    pub toks: [(*const c_char, usize); MAX_TOKS_PER_LINE],
    pub ntok: usize,
    pub ti: usize,
}

extern "C" {
    fn malloc(size: size_t) -> *mut crate::c_void;
    fn realloc(ptr: *mut crate::c_void, size: size_t) -> *mut crate::c_void;
    fn free(ptr: *mut crate::c_void);
}

/// Build a reader over `len` bytes at `text` (copied). Null on OOM.
#[inline(always)]
pub unsafe fn reader_new(text: *const c_char, len: usize) -> *mut SeqReader {
    let r = malloc(core::mem::size_of::<SeqReader>()) as *mut SeqReader;
    if r.is_null() {
        return core::ptr::null_mut();
    }
    let buf = malloc(len + 1) as *mut c_char;
    if buf.is_null() {
        free(r as *mut crate::c_void);
        return core::ptr::null_mut();
    }
    for i in 0..len {
        *buf.add(i) = *text.add(i);
    }
    *buf.add(len) = 0;
    let mut cap = 256usize;
    let mut ls = malloc(cap * core::mem::size_of::<usize>()) as *mut usize;
    if ls.is_null() {
        free(buf as *mut crate::c_void);
        free(r as *mut crate::c_void);
        return core::ptr::null_mut();
    }
    let mut nl = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;
    while i <= len {
        let eol = i == len || *buf.add(i) as u8 == b'\n';
        if eol {
            if nl >= cap {
                cap *= 2;
                let grown = realloc(
                    ls as *mut crate::c_void,
                    cap * core::mem::size_of::<usize>(),
                ) as *mut usize;
                if grown.is_null() {
                    free(buf as *mut crate::c_void);
                    free(ls as *mut crate::c_void);
                    return core::ptr::null_mut();
                }
                ls = grown;
            }
            *ls.add(nl) = start;
            nl += 1;
            start = i + 1;
        }
        i += 1;
    }
    (*r).text = buf;
    (*r).len = len;
    (*r).lines = ls;
    (*r).nlines = nl;
    (*r).line = 0;
    (*r).ntok = 0;
    (*r).ti = 0;
    r
}

#[inline(always)]
pub unsafe fn reader_free(r: *mut SeqReader) {
    if r.is_null() {
        return;
    }
    free((*r).text as *mut crate::c_void);
    free((*r).lines as *mut crate::c_void);
    free(r as *mut crate::c_void);
}

#[inline(always)]
unsafe fn reader_line_range(r: *mut SeqReader, li: usize) -> (*const c_char, usize) {
    let s = *(*r).lines.add(li);
    let mut e = s;
    while e < (*r).len && *(*r).text.add(e) as u8 != b'\n' {
        e += 1;
    }
    ((*r).text.add(s) as *const c_char, e - s)
}

#[inline(always)]
unsafe fn reader_fill(r: *mut SeqReader) -> bool {
    while (*r).ti >= (*r).ntok {
        if (*r).line >= (*r).nlines {
            return false;
        }
        let li = (*r).line;
        (*r).line += 1;
        let (p, n) = reader_line_range(r, li);
        let n = if n > 0 && *(p.add(n - 1)) as u8 == b'\r' {
            n - 1
        } else {
            n
        };
        let mut tmp = [(core::ptr::null() as *const c_char, 0usize); MAX_TOKS_PER_LINE];
        let nt = list_tokens(p, n, tmp.as_mut_ptr(), MAX_TOKS_PER_LINE);
        if nt == usize::MAX || nt == 0 {
            if nt == usize::MAX {
                return false;
            }
            continue;
        }
        (*r).toks = tmp;
        (*r).ntok = nt;
        (*r).ti = 0;
        return true;
    }
    true
}

/// End the current statement: discard the rest of the current record.
#[inline(always)]
pub unsafe fn reader_stmt_end(r: *mut SeqReader) {
    (*r).ntok = 0;
    (*r).ti = 0;
}

/// Next token (spanning records as needed). Null ptr on EOF or overflow
/// (caller fail-louds).
#[inline(always)]
pub unsafe fn reader_tok(r: *mut SeqReader) -> (*const c_char, usize) {
    if !reader_fill(r) {
        return (core::ptr::null(), 0);
    }
    let t = (*r).toks[(*r).ti];
    (*r).ti += 1;
    t
}
