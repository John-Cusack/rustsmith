// Faithful Rust transcription of elmerice/Meshers/MshGlacierSynthetic.F90
// @ 9f6af2f85. Unit: fortran:elmerice/Meshers/MshGlacierSynthetic.F90
// (exports: mshglaciersynthetic, PROGRAM entry `main`).
//
// Entry shape: `#[no_mangle] mshglaciersynthetic_main` (NOT `main`); the
// splice step adds the `main` alias (`ld -r --defsym
// main=mshglaciersynthetic_main`) when the archive replaces the native
// PROGRAM object (same rationale as mshglacier).
// External bed/surface functions: the original declares
// `REAL(KIND=dp) :: fbed, fsurf` with an implicit interface and calls
// `fsurf(x)` / `fsurf(x,y)` — plain Fortran by-REFERENCE passing (no VALUE,
// no BIND(C)). The port therefore passes pointers (`link_name` fbed/fsurf,
// 1-arg and 2-arg shapes), so user objects built for the original link
// unchanged. The archive leaves `fbed`/`fsurf` undefined exactly like the
// native object does (final link fails fast without user objects).
// Heap discipline: libc malloc/free only; floats via fortranio into stack
// buffers; bare STOPs exit(0) silently as gfortran does.
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

const NAME_LEN: usize = 32;
const AEPS: c_double = 2.220446049250313e-15;

// One link name, two arities: the original declares a single implicit-
// interface external and calls it with one or two by-reference args
// depending on the runtime 2D/3D path; the linked user object decides the
// real arity. `clashing_extern_declarations` is the faithful shape here.
#[allow(clashing_extern_declarations)]
extern "C" {
    #[link_name = "fbed"]
    fn fbed_1(x: *const c_double) -> c_double;
    #[link_name = "fbed"]
    fn fbed_2(x: *const c_double, y: *const c_double) -> c_double;
    #[link_name = "fsurf"]
    fn fsurf_1(x: *const c_double) -> c_double;
    #[link_name = "fsurf"]
    fn fsurf_2(x: *const c_double, y: *const c_double) -> c_double;
}

// SAFETY: single-threaded utility; see module notes.
#[no_mangle]
pub unsafe extern "C" fn mshglaciersynthetic_main(
    _argc: c_int,
    _argv: *const *const c_char,
) -> c_int {
    let (input, input_n) = read_whole_syn(b"mesh_info.in\0".as_ptr() as *const c_char);
    let u10 = reader_new(input, input_n);
    if u10.is_null() {
        exit(2);
    }
    let mut scratch = [0 as c_char; 128];
    let scratchp = scratch.as_mut_ptr();

    let mut tmpc = [0 as c_char; NAME_LEN];
    let mut namemsh = [0 as c_char; NAME_LEN];
    take_char_syn(u10, 1, tmpc.as_mut_ptr());
    take_char_syn(u10, 30, namemsh.as_mut_ptr());
    take_char_syn(u10, 1, tmpc.as_mut_ptr());
    let np = take_int_syn(u10);
    reader_stmt_end(u10);
    reader_free(u10);
    free(input as *mut c_void);
    trim_end_syn(namemsh.as_mut_ptr());

    let serial = np == 1;
    let mut inp = [0 as c_char; 16];
    if !serial {
        if np < 10 {
            iw10_syn(np, 1, inp.as_mut_ptr());
        } else if np < 100 {
            iw10_syn(np, 2, inp.as_mut_ptr());
        } else if np < 1000 {
            iw10_syn(np, 3, inp.as_mut_ptr());
        } else if np < 10000 {
            iw10_syn(np, 4, inp.as_mut_ptr());
        } else {
            puts(b" Work for a number of partitions < 1000\0".as_ptr() as *const c_char);
            exit(0);
        }
    }
    trim_end_syn(inp.as_mut_ptr());

    // dim is set on the k==1 pass only (loop always visits k=1 for Np>=1).
    let mut dim = 0i64;
    for k in 1..=np {
        // WRITE(*,*)k,' of ', Np  (no ' partitions' suffix in this unit)
        {
            let mut rec = [0 as c_char; 128];
            let mut pos = g0_int_field(k, rec.as_mut_ptr());
            strcpy(rec.as_mut_ptr().add(pos), b"  of \0".as_ptr() as *const c_char);
            pos = strlen(rec.as_ptr());
            pos += g0_int_field(np, rec.as_mut_ptr().add(pos));
            put_bytes_syn(rec.as_mut_ptr(), pos);
        }
        let mut hdrpath = [0 as c_char; 512];
        let mut inn = [0 as c_char; 16];
        if !serial {
            if k < 10 {
                iw10_syn(k, 1, inn.as_mut_ptr());
            } else if k < 100 {
                iw10_syn(k, 2, inn.as_mut_ptr());
            } else if k < 1000 {
                iw10_syn(k, 3, inn.as_mut_ptr());
            } else if k < 10000 {
                iw10_syn(k, 4, inn.as_mut_ptr());
            }
            // WRITE(*,*)'iNN',iNN — iNN is CHARACTER*10 (ADJUSTL'd, blank
            // padded): the message carries all 10 raw chars; TRIM applies
            // only at path use below.
            {
                let mut rec = [0 as c_char; 64];
                strcpy(rec.as_mut_ptr(), b" iNN\0".as_ptr() as *const c_char);
                let mut pos = strlen(rec.as_ptr());
                // iNN is CHARACTER*10 ADJUSTL'd: raw 10 chars + own blank.
                for i in 0..10 {
                    *rec.as_mut_ptr().add(pos) = *inn.as_ptr().add(i);
                    pos += 1;
                }
                put_bytes_syn(rec.as_mut_ptr(), pos);
            }
            trim_end_syn(inn.as_mut_ptr());
            strcpy(hdrpath.as_mut_ptr(), namemsh.as_ptr());
            strcat(hdrpath.as_mut_ptr(), b"/partitioning.\0".as_ptr() as *const c_char);
            strcat(hdrpath.as_mut_ptr(), inp.as_ptr());
            strcat(hdrpath.as_mut_ptr(), b"/part.\0".as_ptr() as *const c_char);
            strcat(hdrpath.as_mut_ptr(), inn.as_ptr());
            strcat(hdrpath.as_mut_ptr(), b".header\0".as_ptr() as *const c_char);
        } else {
            strcpy(hdrpath.as_mut_ptr(), namemsh.as_ptr());
            strcat(hdrpath.as_mut_ptr(), b"/mesh.header\0".as_ptr() as *const c_char);
        }
        let (htext, htext_n) = read_whole_syn(hdrpath.as_mut_ptr());
        let uh = reader_new(htext, htext_n);
        let ntn = {
            let (p, l) = reader_tok(uh);
            if p.is_null() {
                eof_fail_syn();
            }
            let v = tok_int(p, l);
            reader_stmt_end(uh);
            v as usize
        };
        reader_free(uh);
        free(htext as *mut c_void);

        let node = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut i64;
        let xx = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
        let yy = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
        let zz = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
        if node.is_null() {
            exit(2);
        }
        // WRITE(*,*)'Part ', k, ' NtN = ', NtN
        {
            let mut rec = [0 as c_char; 256];
            strcpy(rec.as_mut_ptr(), b" Part \0".as_ptr() as *const c_char);
            let mut pos = strlen(rec.as_ptr());
            pos += g0_int_field(k, rec.as_mut_ptr().add(pos));
            strcpy(rec.as_mut_ptr().add(pos), b"  NtN = \0".as_ptr() as *const c_char);
            pos = strlen(rec.as_ptr());
            pos += g0_int_field(ntn as i64, rec.as_mut_ptr().add(pos));
            put_bytes_syn(rec.as_mut_ptr(), pos);
        }

        let mut nodepath = [0 as c_char; 512];
        if !serial {
            strcpy(nodepath.as_mut_ptr(), namemsh.as_ptr());
            strcat(nodepath.as_mut_ptr(), b"/partitioning.\0".as_ptr() as *const c_char);
            strcat(nodepath.as_mut_ptr(), inp.as_ptr());
            strcat(nodepath.as_mut_ptr(), b"/part.\0".as_ptr() as *const c_char);
            strcat(nodepath.as_mut_ptr(), inn.as_ptr());
            strcat(nodepath.as_mut_ptr(), b".nodes\0".as_ptr() as *const c_char);
        } else {
            strcpy(nodepath.as_mut_ptr(), namemsh.as_ptr());
            strcat(nodepath.as_mut_ptr(), b"/mesh.nodes\0".as_ptr() as *const c_char);
        }
        let (ntext, ntext_n) = read_whole_syn(nodepath.as_mut_ptr());
        let u12 = reader_new(ntext, ntext_n);
        // One READ statement per node (case-folded scalar j reused below).
        let mut jj: i64 = 0;
        for i in 0..ntn {
            *node.add(i) = grab_int_syn(u12);
            jj = grab_int_syn(u12);
            *xx.add(i) = grab_f64_syn(u12, scratchp);
            *yy.add(i) = grab_f64_syn(u12, scratchp);
            *zz.add(i) = grab_f64_syn(u12, scratchp);
            reader_stmt_end(u12);
        }
        reader_free(u12);

        if k == 1 {
            // 2D vs 3D geometry detection (ANY(ABS(z)>AEPS) etc.).
            let mut any_z = false;
            let mut out01z = false;
            for i in 0..ntn {
                let z = *zz.add(i);
                if z.abs() > AEPS {
                    any_z = true;
                }
                if z > 1.0 || z < 0.0 {
                    out01z = true;
                }
            }
            if any_z {
                if out01z {
                    puts(b" For 3D geometry, the initial mesh must fulfil 0 < z < 1\0".as_ptr()
                        as *const c_char);
                    exit(0);
                }
                dim = 3;
                puts(b" Initial mesh verified, found to be a 3D geometry\0".as_ptr()
                    as *const c_char);
            } else {
                let mut out01y = false;
                for i in 0..ntn {
                    let y = *yy.add(i);
                    if y > 1.0 || y < 0.0 {
                        out01y = true;
                    }
                }
                if out01y {
                    puts(b" For 2D geometry, the initial mesh must fulfil 0 < y < 1\0".as_ptr()
                        as *const c_char);
                    exit(0);
                }
                dim = 2;
                puts(b" Initial mesh verified, found to be a 2D geometry\0".as_ptr()
                    as *const c_char);
            }
        }

        // REWIND(12) + per-node WRITE(12,1200): same final bytes.
        // FORMAT(i7,2x,i5,3(2x,e22.15)): fixed 86 bytes/line.
        let out = malloc(ntn * 87 + 1) as *mut c_char;
        if out.is_null() {
            exit(2);
        }
        let mut opos = 0usize;
        for i in 0..ntn {
            let (zs, zb);
            if dim == 2 {
                let x = *xx.add(i);
                zs = fsurf_1(&x);
                zb = fbed_1(&x);
            } else {
                let x = *xx.add(i);
                let y = *yy.add(i);
                zs = fsurf_2(&x, &y);
                zb = fbed_2(&x, &y);
            }
            if zs - zb <= 0.0 {
                puts(b" NEGATIVE OR NULL THICKNESS!!!\0".as_ptr() as *const c_char);
                exit(0);
            }
            opos += i_field(*node.add(i), 7, out.add(opos));
            *out.add(opos) = b' ' as c_char;
            *out.add(opos + 1) = b' ' as c_char;
            opos += 2;
            opos += i_field(jj, 5, out.add(opos));
            if dim == 2 {
                let znew = zb + *yy.add(i) * (zs - zb);
                for v in [*xx.add(i), znew, *zz.add(i)] {
                    *out.add(opos) = b' ' as c_char;
                    *out.add(opos + 1) = b' ' as c_char;
                    opos += 2;
                    opos += e22_15(v, out.add(opos));
                }
            } else {
                let znew = zb + *zz.add(i) * (zs - zb);
                for v in [*xx.add(i), *yy.add(i), znew] {
                    *out.add(opos) = b' ' as c_char;
                    *out.add(opos + 1) = b' ' as c_char;
                    opos += 2;
                    opos += e22_15(v, out.add(opos));
                }
            }
            *out.add(opos) = b'\n' as c_char;
            opos += 1;
        }
        write_whole_syn(nodepath.as_mut_ptr(), out, opos);
        free(out as *mut c_void);
        free(ntext as *mut c_void);
        free(node as *mut c_void);
        free(xx as *mut c_void);
        free(yy as *mut c_void);
        free(zz as *mut c_void);
    }
    0
}

// ---- helpers ---------------------------------------------------------------

unsafe fn read_whole_syn(path: *const c_char) -> (*mut c_char, usize) {
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

unsafe fn write_whole_syn(path: *const c_char, data: *const c_char, n: usize) {
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

unsafe fn put_bytes_syn(buf: *mut c_char, n: usize) {
    *buf.add(n) = 0;
    puts(buf as *const c_char);
}

unsafe fn trim_end_syn(b: *mut c_char) {
    let mut n = strlen(b);
    while n > 0 && *b.add(n - 1) as u8 == b' ' {
        n -= 1;
    }
    *b.add(n) = 0;
}

unsafe fn iw10_syn(v: i64, m: usize, out: *mut c_char) -> usize {
    let n = iw_m(v, m, out);
    for i in n..10 {
        *out.add(i) = b' ' as c_char;
    }
    *out.add(10) = 0;
    n
}

unsafe fn take_char_syn(r: *mut SeqReader, n: usize, out: *mut c_char) {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_syn();
    }
    tok_char(p, l, out, n);
    reader_stmt_end(r);
}

unsafe fn take_int_syn(r: *mut SeqReader) -> i64 {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_syn();
    }
    let v = tok_int(p, l);
    reader_stmt_end(r);
    v
}

unsafe fn grab_f64_syn(r: *mut SeqReader, scratchp: *mut c_char) -> c_double {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_syn();
    }
    tok_f64(p, l, scratchp)
}

unsafe fn grab_int_syn(r: *mut SeqReader) -> i64 {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_syn();
    }
    tok_int(p, l)
}

unsafe fn eof_fail_syn() -> ! {
    fprintf(
        c_stderr(),
        b"Fortran runtime error: End of file\n\0".as_ptr() as *const c_char,
    );
    exit(2);
}
