// Faithful Rust transcription of elmerice/Meshers/MshGlacier.F90 @ 9f6af2f85.
// Unit: fortran:elmerice/Meshers/MshGlacier.F90 (exports: mshglacier,
// PROGRAM entry `main`).
//
// Entry shape: `#[no_mangle] mshglacier_main` (NOT `main`). The splice step
// adds the `main` alias (`ld -r --defsym main=mshglacier_main`) when the
// archive replaces the native PROGRAM object, so the committed archive
// itself defines no `main` — the lane's nm gate then holds zero STRONG
// port-level dups with no entry-point exclusion, and cargo harness links
// stay duplicate-free. The installed binary behaves as the original (the
// program takes no command-line arguments).
// Upstream quirk preserved: the nodes READ stores N/j in scalars, so every
// mesh.nodes output line repeats the LAST input node's N/j (not per-node
// values). CubicSpline/SolveTriDiag/SearchInterval/CubicSplineVal/
// InterpolateCurve are transcribed from fem/src/GeneralUtils.F90 (lane-fem-a
// territory) as slice-local static copies: this crate owns no fem/ path and
// changes no shared file; the copies exist so the unit links standalone.
// Heap discipline: libc malloc/free only; all float rendering through
// fortranio into stack buffers (never Rust Display); bare STOPs exit(0)
// silently as gfortran does.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]
// Lane convention: C format strings stay `b"..\0"` (byte-identical to the
// transcribed TU; `c""` churn risks the load-bearing bytes), and SAFETY
// contracts live in `// SAFETY:` comments at every unsafe site, not in doc
// sections. Both lints are style-only against this convention.
#![allow(clippy::manual_c_str_literals)]
#![allow(clippy::missing_safety_doc)]

use ice_prelude::fortranio::*;
use ice_prelude::*;

// MIRRORED INPUT SHAPE: mesh_input.dat is alternating comment/value lines;
// each READ statement consumes one line (list reader enforces records).

const NAME_LEN: usize = 32;
const LINE_BUF: usize = 1024;

// ---- GeneralUtils transcriptions (SolveTriDiag/CubicSpline/SearchInterval/
// CubicSplineVal/InterpolateCurve, non-monotone path only, as called here) --

unsafe fn solve_tri_diag(n: usize, y: *const c_double, h: *const c_double, r: *mut c_double) {
    // SAFETY: caller guarantees n>=2 slices of matching length.
    let b = malloc(n * core::mem::size_of::<c_double>()) as *mut c_double;
    if b.is_null() {
        exit(2);
    }
    for i in 1..n - 1 {
        *b.add(i) = 2.0 * (*h.add(i - 1) + *h.add(i));
        *r.add(i) = 3.0
            * (*h.add(i) * (*y.add(i) - *y.add(i - 1)) / *h.add(i - 1)
                + *h.add(i - 1) * (*y.add(i + 1) - *y.add(i)) / *h.add(i));
    }
    *r.add(1) -= *h.add(1) * *r.add(0);
    for i in 1..n - 2 {
        let s = -*h.add(i + 1) / *b.add(i);
        *r.add(i + 1) += s * *r.add(i);
        *b.add(i + 1) += s * *h.add(i - 1);
    }
    // Fortran DO i=n-1,2,-1 (1-based) visits indices n-2 down to 1.
    let mut i = n - 2;
    loop {
        *r.add(i) = (*r.add(i) - *h.add(i - 1) * *r.add(i + 1)) / *b.add(i);
        if i == 1 {
            break;
        }
        i -= 1;
    }
    free(b as *mut c_void);
}

unsafe fn cubic_spline(n: usize, x: *const c_double, y: *const c_double, r: *mut c_double) {
    // SAFETY: same contract as solve_tri_diag.
    let h = malloc(n * core::mem::size_of::<c_double>()) as *mut c_double;
    if h.is_null() {
        exit(2);
    }
    for i in 0..n - 1 {
        *h.add(i) = *x.add(i + 1) - *x.add(i);
    }
    *r.add(0) = (*y.add(1) - *y.add(0)) / *h.add(0);
    *r.add(n - 1) = (*y.add(n - 1) - *y.add(n - 2)) / *h.add(n - 2);
    solve_tri_diag(n, y, h, r);
    free(h as *mut c_void);
}

unsafe fn search_interval(tval: *const c_double, n: usize, t: c_double) -> usize {
    // SAFETY: n>=2 readable.
    if t < *tval.add(1) {
        return 0;
    } else if t >= *tval.add(n - 2) {
        return n - 2;
    }
    let mut n0: usize = 0;
    let mut n1: usize = n - 1;
    let mut i = (n0 + n1) / 2;
    loop {
        if *tval.add(i) <= t && *tval.add(i + 1) > t {
            break;
        }
        if *tval.add(i) > t {
            n1 = i - 1;
        } else {
            n0 = i + 1;
        }
        i = (n0 + n1) / 2;
    }
    if i > n - 2 {
        i = n - 2;
    }
    i
}

unsafe fn cubic_spline_val(
    x: *const c_double,
    y: *const c_double,
    r: *const c_double,
    t: c_double,
) -> c_double {
    // SAFETY: 2-element slices.
    let h = *x.add(1) - *x.add(0);
    let a = -2.0 * (*y.add(1) - *y.add(0)) + (*r.add(0) + *r.add(1)) * h;
    let b = 3.0 * (*y.add(1) - *y.add(0)) - (2.0 * *r.add(0) + *r.add(1)) * h;
    let c = *r.add(0) * h;
    let d = *y.add(0);
    let lt = (t - *x.add(0)) / h;
    ((a * lt + b) * lt + c) * lt + d
}

unsafe fn interpolate_curve(
    tvalues: *const c_double,
    fvalues: *const c_double,
    n: usize,
    t: c_double,
    cubic_coeff: *const c_double,
) -> c_double {
    // SAFETY: n>=1 slices; cubic_coeff null iff linear. A null coefficient
    // pointer means "absent" (Fortran OPTIONAL+PRESENT+ASSOCIATED fold).
    if n == 1 {
        return *fvalues.add(0) * t;
    }
    let i = search_interval(tvalues, n, t);
    let mut cubic = !cubic_coeff.is_null();
    cubic = cubic && t >= *tvalues.add(0) && t <= *tvalues.add(n - 1);
    if cubic {
        cubic_spline_val(
            tvalues.add(i),
            fvalues.add(i),
            cubic_coeff.add(i),
            t,
        )
    } else {
        let f = (t - *tvalues.add(i)) / (*tvalues.add(i + 1) - *tvalues.add(i));
        (1.0 - f) * *fvalues.add(i) + f * *fvalues.add(i + 1)
    }
}

// ---- small libc-file helpers ------------------------------------------------

// SAFETY: thin wrappers; null checks fail loud with the gfortran-style
// message and exit code 2 (never a silent short read).
unsafe fn read_whole(path: *const c_char) -> (*mut c_char, usize) {
    let f = fopen(path, b"r\0".as_ptr() as *const c_char);
    if f.is_null() {
        fprintf(
            c_stderr(),
            b"Fortran runtime error: Cannot open file '%s': No such file or directory\n\0".as_ptr()
                as *const c_char,
            path,
        );
        exit(2);
    }
    fseek(f, 0, 2);
    let n = ftell(f) as usize;
    fseek(f, 0, 0);
    let buf = malloc(n + 1) as *mut c_char;
    if buf.is_null() {
        fclose(f);
        exit(2);
    }
    let got = fread(buf as *mut c_void, 1, n, f);
    fclose(f);
    *buf.add(got) = 0;
    (buf, got)
}

unsafe fn write_whole(path: *const c_char, data: *const c_char, n: usize) {
    let f = fopen(path, b"w\0".as_ptr() as *const c_char);
    if f.is_null() {
        fprintf(
            c_stderr(),
            b"Fortran runtime error: Cannot open file '%s'\n\0".as_ptr() as *const c_char,
            path,
        );
        exit(2);
    }
    fwrite(data as *const c_void, 1, n, f);
    fclose(f);
}

// Stdout record helpers: format into a stack buffer, NUL-terminate, puts.
unsafe fn put_bytes(buf: *mut c_char, n: usize) {
    *buf.add(n) = 0;
    puts(buf as *const c_char);
}

// ---- PROGRAM MshGlacier ------------------------------------------------------

// SAFETY: single-threaded utility; every float on stdout/mesh.nodes goes
// through fortranio (never Rust Display); bare STOPs exit 0 silently.
#[no_mangle]
pub unsafe extern "C" fn mshglacier_main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    let (input, input_n) = read_whole(b"mesh_input.dat\0".as_ptr() as *const c_char);
    let u10 = reader_new(input, input_n);
    if u10.is_null() {
        exit(2);
    }
    let mut scratch = [0 as c_char; 128];
    let scratchp = scratch.as_mut_ptr();

    let mut tmpc = [0 as c_char; NAME_LEN];
    let take_char = |r: *mut SeqReader, n: usize, out: *mut c_char| {
        let (p, l) = reader_tok(r);
        if p.is_null() {
            eof_fail();
        }
        tok_char(p, l, out, n);
        reader_stmt_end(r);
    };
    let take_f64 = |r: *mut SeqReader| -> c_double {
        let (p, l) = reader_tok(r);
        if p.is_null() {
            eof_fail();
        }
        let v = tok_f64(p, l, scratchp);
        reader_stmt_end(r);
        v
    };
    let take_int = |r: *mut SeqReader| -> i64 {
        let (p, l) = reader_tok(r);
        if p.is_null() {
            eof_fail();
        }
        let v = tok_int(p, l);
        reader_stmt_end(r);
        v
    };

    take_char(u10, 1, tmpc.as_mut_ptr());
    let mut namemsh = [0 as c_char; NAME_LEN];
    take_char(u10, 20, namemsh.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    let constraint = take_f64(u10) > 0.5;
    take_char(u10, 1, tmpc.as_mut_ptr());
    let cubic = take_f64(u10) > 0.5;
    take_char(u10, 1, tmpc.as_mut_ptr());
    let mut namesurf = [0 as c_char; NAME_LEN];
    take_char(u10, 20, namesurf.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    let npts = take_int(u10) as usize;
    take_char(u10, 1, tmpc.as_mut_ptr());
    let mut namebed = [0 as c_char; NAME_LEN];
    take_char(u10, 20, namebed.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    let nptb = take_int(u10) as usize;
    take_char(u10, 1, tmpc.as_mut_ptr());
    let hmin = take_f64(u10);
    let (mut x0, mut x1) = (0.0 as c_double, 0.0 as c_double);
    if !constraint {
        take_char(u10, 1, tmpc.as_mut_ptr());
        x0 = take_f64(u10);
        take_char(u10, 1, tmpc.as_mut_ptr());
        x1 = take_f64(u10);
    }
    reader_free(u10);
    free(input as *mut c_void);

    // Trim trailing blanks (CHARACTER*20 TRIM semantics).
    let trim = |b: *mut c_char| {
        let mut n = strlen(b);
        while n > 0 && *b.add(n - 1) as u8 == b' ' {
            n -= 1;
        }
        *b.add(n) = 0;
    };
    trim(namemsh.as_mut_ptr());
    trim(namesurf.as_mut_ptr());
    trim(namebed.as_mut_ptr());

    let xsurf = malloc(npts * 8) as *mut c_double;
    let ysurf = malloc(npts * 8) as *mut c_double;
    let y2s = malloc(npts * 8) as *mut c_double;
    let xbed = malloc(nptb * 8) as *mut c_double;
    let ybed = malloc(nptb * 8) as *mut c_double;
    let y2b = malloc(nptb * 8) as *mut c_double;
    if xsurf.is_null() || xbed.is_null() {
        exit(2);
    }

    let mut hpath = [0 as c_char; 256];
    strcpy(hpath.as_mut_ptr(), namemsh.as_ptr());
    strcat(hpath.as_mut_ptr(), b"/mesh.header\0".as_ptr() as *const c_char);
    let (htext, htext_n) = read_whole(hpath.as_mut_ptr());
    // Formatted READ(10,1000): first record, I6.
    let hline = htext;
    let mut hlen = 0usize;
    while hlen < htext_n && *htext.add(hlen) as u8 != b'\n' {
        hlen += 1;
    }
    let ntn = field_int(hline, hlen, 6) as usize;
    free(htext as *mut c_void);

    let xnode = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
    let ynode = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
    if xnode.is_null() {
        exit(2);
    }

    let (stext, stext_n) = read_whole(namesurf.as_ptr());
    let us = reader_new(stext, stext_n);
    for i in 0..npts {
        let (p, l) = reader_tok(us);
        if p.is_null() {
            eof_fail();
        }
        *xsurf.add(i) = tok_f64(p, l, scratchp);
        let (p, l) = reader_tok(us);
        if p.is_null() {
            eof_fail();
        }
        *ysurf.add(i) = tok_f64(p, l, scratchp);
    }
    reader_stmt_end(us);
    reader_free(us);
    free(stext as *mut c_void);

    let (btext, btext_n) = read_whole(namebed.as_ptr());
    let ub = reader_new(btext, btext_n);
    for i in 0..nptb {
        let (p, l) = reader_tok(ub);
        if p.is_null() {
            eof_fail();
        }
        *xbed.add(i) = tok_f64(p, l, scratchp);
        let (p, l) = reader_tok(ub);
        if p.is_null() {
            eof_fail();
        }
        *ybed.add(i) = tok_f64(p, l, scratchp);
    }
    reader_stmt_end(ub);
    reader_free(ub);
    free(btext as *mut c_void);

    let minmax = |a: *mut c_double, n: usize| -> (c_double, c_double) {
        let mut lo = c_double::INFINITY;
        let mut hi = c_double::NEG_INFINITY;
        for i in 0..n {
            let v = *a.add(i);
            if v < lo {
                lo = v;
            }
            if v > hi {
                hi = v;
            }
        }
        (lo, hi)
    };

    if !constraint {
        let (blo, bhi) = minmax(xbed, nptb);
        if blo > x0 || bhi < x1 {
            // MUST BE bed-range message + STOP.
            let mut rec = [0 as c_char; 256];
            let msg = b" MUST BE : x0> MIN(xbed) AND x1 < MAX(xbed)\0";
            strcpy(rec.as_mut_ptr(), msg.as_ptr() as *const c_char);
            let mut pos = strlen(rec.as_ptr());
            for v in [x0, blo, x1, bhi] {
                pos += g0_f64_field(v, rec.as_mut_ptr().add(pos));
            }
            put_bytes(rec.as_mut_ptr(), pos);
            exit(0);
        }
        let (slo, shi) = minmax(xsurf, npts);
        if slo > x0 || shi < x1 {
            puts(b" MUST BE : x0 > MIN(xsurf) AND x1 < MAX(xsurf)\0".as_ptr() as *const c_char);
            exit(0);
        }
    }

    if cubic {
        cubic_spline(npts, xsurf, ysurf, y2s);
        cubic_spline(nptb, xbed, ybed, y2b);
    }

    let mut npath = [0 as c_char; 256];
    strcpy(npath.as_mut_ptr(), namemsh.as_ptr());
    strcat(npath.as_mut_ptr(), b"/mesh.nodes\0".as_ptr() as *const c_char);
    let (ntext, ntext_n) = read_whole(npath.as_mut_ptr());
    let u12 = reader_new(ntext, ntext_n);
    // NOTE: N in the READ/WRITE is the case-folded loop variable `n`
    // (Fortran is case-insensitive): output N is the sequential node index,
    // while j is a scalar holding the LAST input line's value. Both
    // reproduced exactly.
    // j/zz are file-column scalars (last values reused); the first column
    // feeds the loop index `n`, so the READed N is consumed and dropped.
    let (mut jj, mut zz) = (0i64, 0.0 as c_double);
    for i in 0..ntn {
        let grab = || -> (i64, i64, c_double, c_double, c_double) {
            let (p, l) = reader_tok(u12);
            if p.is_null() {
                eof_fail();
            }
            let a = tok_int(p, l);
            let (p, l) = reader_tok(u12);
            if p.is_null() {
                eof_fail();
            }
            let b2 = tok_int(p, l);
            let (p, l) = reader_tok(u12);
            if p.is_null() {
                eof_fail();
            }
            let c2 = tok_f64(p, l, scratchp);
            let (p, l) = reader_tok(u12);
            if p.is_null() {
                eof_fail();
            }
            let d2 = tok_f64(p, l, scratchp);
            let (p, l) = reader_tok(u12);
            if p.is_null() {
                eof_fail();
            }
            let e2 = tok_f64(p, l, scratchp);
            (a, b2, c2, d2, e2)
        };
        let (_a, b2, c2, d2, e2) = grab();
        jj = b2;
        *xnode.add(i) = c2;
        *ynode.add(i) = d2;
        zz = e2;
    }
    reader_stmt_end(u12);
    reader_free(u12);
    free(ntext as *mut c_void);

    if constraint {
        puts(b" Will have to check if number of nodes in x direction equals number of data points\0".as_ptr()
            as *const c_char);
        if npts != nptb {
            puts(b" Surface and Bed data must have the same number of points\0".as_ptr()
                as *const c_char);
            exit(0);
        }
        let mut ntnx: i64 = 0;
        for n in 0..ntn {
            if (*ynode.add(n)).abs() < 1.0e-4 {
                ntnx += 1;
            }
        }
        if npts as i64 != ntnx {
            let mut rec = [0 as c_char; 128];
            strcpy(
                rec.as_mut_ptr(),
                b" Mesh must have\0".as_ptr() as *const c_char,
            );
            let mut pos = strlen(rec.as_ptr());
            pos += g0_int_field(npts as i64, rec.as_mut_ptr().add(pos));
            strcpy(
                rec.as_mut_ptr().add(pos),
                b"  nodes in x direction\0".as_ptr() as *const c_char,
            );
            pos = strlen(rec.as_ptr());
            put_bytes(rec.as_mut_ptr(), pos);
            exit(0);
        }
        let (blo, bhi) = minmax(xbed, nptb);
        x0 = blo;
        x1 = bhi;
    }

    // FORMAT(i5,2x,i5,3(2x,e22.15)): fixed 84 bytes/line, no record blank.
    let out_n = ntn * 85 + 1;
    let out = malloc(out_n) as *mut c_char;
    if out.is_null() {
        exit(2);
    }
    let mut opos = 0usize;
    for n in 0..ntn {
        let nn_out = (n + 1) as i64;
        let mut x = *xnode.add(n);
        let y = *ynode.add(n);
        if constraint {
            let xi = (x / (1.0 / (npts as c_double - 1.0))).round() as i64 + 1;
            x = (*xsurf.add((xi - 1) as usize) - x0) / (x1 - x0);
        }
        let xnew = x0 + x * (x1 - x0);
        let (zs, zb);
        if cubic {
            zs = interpolate_curve(xsurf, ysurf, npts, xnew, y2s);
            zb = interpolate_curve(xbed, ybed, nptb, xnew, y2b);
        } else {
            zs = interpolate_curve(
                xsurf,
                ysurf,
                npts,
                xnew,
                core::ptr::null(),
            );
            zb = interpolate_curve(xbed, ybed, nptb, xnew, core::ptr::null());
        }
        let ynew = zb + y * (zs - zb).max(hmin);
        opos += i_field(nn_out, 5, out.add(opos));
        *out.add(opos) = b' ' as c_char;
        *out.add(opos + 1) = b' ' as c_char;
        opos += 2;
        opos += i_field(jj, 5, out.add(opos));
        for v in [xnew, ynew, zz] {
            *out.add(opos) = b' ' as c_char;
            *out.add(opos + 1) = b' ' as c_char;
            opos += 2;
            opos += e22_15(v, out.add(opos));
        }
        *out.add(opos) = b'\n' as c_char;
        opos += 1;
    }
    write_whole(npath.as_mut_ptr(), out, opos);
    free(out as *mut c_void);

    free(xsurf as *mut c_void);
    free(ysurf as *mut c_void);
    free(y2s as *mut c_void);
    free(xbed as *mut c_void);
    free(ybed as *mut c_void);
    free(y2b as *mut c_void);
    free(xnode as *mut c_void);
    free(ynode as *mut c_void);

    puts(b" END WITH NO TROUBLE ...\0".as_ptr() as *const c_char);
    0
}

unsafe fn eof_fail() -> ! {
    fprintf(
        c_stderr(),
        b"Fortran runtime error: End of file\n\0".as_ptr() as *const c_char,
    );
    exit(2);
}
