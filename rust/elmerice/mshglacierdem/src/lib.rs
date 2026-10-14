// Faithful Rust transcription of elmerice/Meshers/MshGlacierDEM.F90
// @ 9f6af2f85. Unit: fortran:elmerice/Meshers/MshGlacierDEM.F90 (exports:
// mshglacierdem, PROGRAM entry `main`).
//
// Entry shape: `#[no_mangle] mshglacierdem_main` (NOT `main`); the splice
// step adds the `main` alias (`ld -r --defsym main=mshglacierdem_main`)
// when the archive replaces the native PROGRAM object (same rationale as
// mshglacier). Self-contained: bilinear interpolation + nearest-point
// fallback, no GeneralUtils dependency.
// Heap discipline: libc malloc/free only; floats via fortranio into stack
// buffers; bare STOPs exit(0) silently as gfortran does.
// Upstream notes preserved: `(i1.1)`-style internal writes (blank-padded to
// len 10 + ADJUSTL, i.e. the digits); the Np>=10000 STOP; znode scalar `z`
// reused per node (case-folded loop var, same class as MshGlacier's N).
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

// Internal WRITE(iNp,'(im.m)')Np + blank-pad to 10 + ADJUSTL: the result is
// the digits (iw_m core, no leading blanks, trailing blanks trimmed by the
// caller's TRIM). Returns digit length.
unsafe fn iw10(v: i64, m: usize, out: *mut c_char) -> usize {
    let n = iw_m(v, m, out);
    // Blank-pad to 10 (internal file), then the caller TRIMs.
    for i in n..10 {
        *out.add(i) = b' ' as c_char;
    }
    *out.add(10) = 0;
    n
}

// SAFETY: single-threaded utility; see module notes.
#[no_mangle]
pub unsafe extern "C" fn mshglacierdem_main(
    _argc: c_int,
    _argv: *const *const c_char,
) -> c_int {
    let (input, input_n) = read_whole_mg(b"mesh_input.dat\0".as_ptr() as *const c_char);
    let u10 = reader_new(input, input_n);
    if u10.is_null() {
        exit(2);
    }
    let mut scratch = [0 as c_char; 128];
    let scratchp = scratch.as_mut_ptr();

    let mut tmpc = [0 as c_char; NAME_LEN];
    let mut namemsh = [0 as c_char; NAME_LEN];
    let mut namesurf = [0 as c_char; NAME_LEN];
    let mut namebed = [0 as c_char; NAME_LEN];
    take_char(u10, 1, tmpc.as_mut_ptr());
    take_char(u10, 30, namemsh.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    take_char(u10, 30, namesurf.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (nsx_i, nsy_i) = take_pair_int(u10);
    let (nsx, nsy) = (nsx_i as usize, nsy_i as usize);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (xs0, ys0) = take_pair_f64(u10, scratchp);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (lsx, lsy) = take_pair_f64(u10, scratchp);
    take_char(u10, 1, tmpc.as_mut_ptr());
    take_char(u10, 30, namebed.as_mut_ptr());
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (nbx_i, nby_i) = take_pair_int(u10);
    let (nbx, nby) = (nbx_i as usize, nby_i as usize);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (xb0, yb0) = take_pair_f64(u10, scratchp);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let (lbx, lby) = take_pair_f64(u10, scratchp);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let hmin = take_f64(u10, scratchp);
    take_char(u10, 1, tmpc.as_mut_ptr());
    let np = take_int(u10);
    reader_stmt_end(u10);
    reader_free(u10);
    free(input as *mut c_void);

    trim_end(namemsh.as_mut_ptr());
    trim_end(namesurf.as_mut_ptr());
    trim_end(namebed.as_mut_ptr());

    let ns = nsx * nsy;
    let nb = nbx * nby;
    let xs = malloc(ns * 8) as *mut c_double;
    let ys = malloc(ns * 8) as *mut c_double;
    let zs = malloc(ns * 8) as *mut c_double;
    let xb = malloc(nb * 8) as *mut c_double;
    let yb = malloc(nb * 8) as *mut c_double;
    let zb = malloc(nb * 8) as *mut c_double;
    if xs.is_null() || xb.is_null() {
        exit(2);
    }

    let dsx = lsx / (nsx as c_double - 1.0);
    let dsy = lsy / (nsy as c_double - 1.0);
    let dbx = lbx / (nbx as c_double - 1.0);
    let dby = lby / (nby as c_double - 1.0);

    let serial = np == 1;
    let mut inp = [0 as c_char; 16];
    if !serial {
        if np < 10 {
            iw10(np, 1, inp.as_mut_ptr());
        } else if np < 100 {
            iw10(np, 2, inp.as_mut_ptr());
        } else if np < 1000 {
            iw10(np, 3, inp.as_mut_ptr());
        } else if np < 10000 {
            iw10(np, 4, inp.as_mut_ptr());
        } else {
            puts(b" Work for a number of partitions < 1000\0".as_ptr() as *const c_char);
            exit(0);
        }
    }
    // ADJUSTL is a no-op here (iw10 emits no leading blanks); TRIM drops
    // the blank padding: terminate at first blank.
    trim_end(inp.as_mut_ptr());

    // WRITE(*,*)'Ns, Nb',Ns,Nb  /  WRITE(*,*)'dx, dy',dsx,dsy,dbx,dby
    {
        let mut rec = [0 as c_char; 256];
        strcpy(rec.as_mut_ptr(), b" Ns, Nb\0".as_ptr() as *const c_char);
        let mut pos = strlen(rec.as_ptr());
        pos += g0_int_field(ns as i64, rec.as_mut_ptr().add(pos));
        pos += g0_int_field(nb as i64, rec.as_mut_ptr().add(pos));
        put_bytes(rec.as_mut_ptr(), pos);
    }
    {
        let mut rec = [0 as c_char; 256];
        strcpy(rec.as_mut_ptr(), b" dx, dy\0".as_ptr() as *const c_char);
        let mut pos = strlen(rec.as_ptr());
        for v in [dsx, dsy, dbx, dby] {
            pos += g0_f64_field(v, rec.as_mut_ptr().add(pos));
        }
        put_bytes(rec.as_mut_ptr(), pos);
    }

    // Load DEMs (implied-DO triple reads across records).
    let (stext, stext_n) = read_whole_mg(namesurf.as_ptr());
    let us = reader_new(stext, stext_n);
    for i in 0..ns {
        *xs.add(i) = grab_f64(us, scratchp);
        *ys.add(i) = grab_f64(us, scratchp);
        *zs.add(i) = grab_f64(us, scratchp);
    }
    reader_stmt_end(us);
    reader_free(us);
    free(stext as *mut c_void);
    let (btext, btext_n) = read_whole_mg(namebed.as_ptr());
    let ub = reader_new(btext, btext_n);
    for i in 0..nb {
        *xb.add(i) = grab_f64(ub, scratchp);
        *yb.add(i) = grab_f64(ub, scratchp);
        *zb.add(i) = grab_f64(ub, scratchp);
    }
    reader_stmt_end(ub);
    reader_free(ub);
    free(btext as *mut c_void);

    // DEM structure verification (exact message text + STOP).
    {
        let mut k = 0usize;
        for j in 0..nsy {
            let y = ys0 + dsy * (j as c_double);
            for i in 0..nsx {
                let x = xs0 + dsx * (i as c_double);
                if ((*xs.add(k) - x).abs() > 1.0e-6 * dsx)
                    || ((*ys.add(k) - y).abs() > 1.0e-6 * dsy)
                {
                    puts(b" Structure of the DEM is not conforming to what is in mesh_input.dat for Surface DEM\0".as_ptr() as *const c_char);
                    let mut rec = [0 as c_char; 512];
                    strcpy(
                        rec.as_mut_ptr(),
                        b" Found that point \0".as_ptr() as *const c_char,
                    );
                    let mut pos = strlen(rec.as_ptr());
                    pos += g0_int_field((k + 1) as i64, rec.as_mut_ptr().add(pos));
                    strcpy(
                        rec.as_mut_ptr().add(pos),
                        b"  coordinate is \0".as_ptr() as *const c_char,
                    );
                    pos = strlen(rec.as_ptr());
                    pos += g0_f64_field(*xs.add(k), rec.as_mut_ptr().add(pos));
                    pos += g0_f64_field(*ys.add(k), rec.as_mut_ptr().add(pos));
                    strcpy(
                        rec.as_mut_ptr().add(pos),
                        b"  whereas it should be \0".as_ptr() as *const c_char,
                    );
                    pos = strlen(rec.as_ptr());
                    pos += g0_f64_field(x, rec.as_mut_ptr().add(pos));
                    pos += g0_f64_field(y, rec.as_mut_ptr().add(pos));
                    put_bytes(rec.as_mut_ptr(), pos);
                    exit(0);
                }
                k += 1;
            }
        }
    }
    {
        let mut k = 0usize;
        for j in 0..nby {
            let y = yb0 + dby * (j as c_double);
            for i in 0..nbx {
                let x = xb0 + dbx * (i as c_double);
                if ((*xb.add(k) - x).abs() > 1.0e-6 * dbx)
                    || ((*yb.add(k) - y).abs() > 1.0e-6 * dby)
                {
                    puts(b" Structure of the DEM is not conforming to what is in mesh_input.dat for Surface DEM\0".as_ptr() as *const c_char);
                    let mut rec = [0 as c_char; 512];
                    strcpy(
                        rec.as_mut_ptr(),
                        b" Found that point \0".as_ptr() as *const c_char,
                    );
                    let mut pos = strlen(rec.as_ptr());
                    pos += g0_int_field((k + 1) as i64, rec.as_mut_ptr().add(pos));
                    strcpy(
                        rec.as_mut_ptr().add(pos),
                        b"  coordinate is \0".as_ptr() as *const c_char,
                    );
                    pos = strlen(rec.as_ptr());
                    pos += g0_f64_field(*xb.add(k), rec.as_mut_ptr().add(pos));
                    pos += g0_f64_field(*yb.add(k), rec.as_mut_ptr().add(pos));
                    strcpy(
                        rec.as_mut_ptr().add(pos),
                        b"  whereas it should be \0".as_ptr() as *const c_char,
                    );
                    pos = strlen(rec.as_ptr());
                    pos += g0_f64_field(x, rec.as_mut_ptr().add(pos));
                    pos += g0_f64_field(y, rec.as_mut_ptr().add(pos));
                    put_bytes(rec.as_mut_ptr(), pos);
                    exit(0);
                }
                k += 1;
            }
        }
    }

    for k in 1..=np {
        // WRITE(*,*)k,' of ', Np, ' partitions'
        {
            let mut rec = [0 as c_char; 128];
            let mut pos = g0_int_field(k, rec.as_mut_ptr());
            strcpy(rec.as_mut_ptr().add(pos), b"  of \0".as_ptr() as *const c_char);
            pos = strlen(rec.as_ptr());
            pos += g0_int_field(np, rec.as_mut_ptr().add(pos));
            strcpy(
                rec.as_mut_ptr().add(pos),
                b"  partitions\0".as_ptr() as *const c_char,
            );
            pos = strlen(rec.as_ptr());
            put_bytes(rec.as_mut_ptr(), pos);
        }
        // Partition header path.
        let mut hdrpath = [0 as c_char; 512];
        if !serial {
            let mut inn = [0 as c_char; 16];
            if k < 10 {
                iw10(k, 1, inn.as_mut_ptr());
            } else if k < 100 {
                iw10(k, 2, inn.as_mut_ptr());
            } else if k < 1000 {
                iw10(k, 3, inn.as_mut_ptr());
            } else if k < 10000 {
                iw10(k, 4, inn.as_mut_ptr());
            }
            trim_end(inn.as_mut_ptr());
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
        let (htext, htext_n) = read_whole_mg(hdrpath.as_mut_ptr());
        let uh = reader_new(htext, htext_n);
        let ntn = {
            let (p, l) = reader_tok(uh);
            if p.is_null() {
                eof_fail_mg();
            }
            let v = tok_int(p, l);
            reader_stmt_end(uh);
            v as usize
        };
        reader_free(uh);
        free(htext as *mut c_void);

        let node = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut i64;
        let xnode = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
        let ynode = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
        let znode = malloc(if ntn == 0 { 8 } else { ntn * 8 }) as *mut c_double;
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
            put_bytes(rec.as_mut_ptr(), pos);
        }

        let mut nodepath = [0 as c_char; 512];
        if !serial {
            let mut inn = [0 as c_char; 16];
            if k < 10 {
                iw10(k, 1, inn.as_mut_ptr());
            } else if k < 100 {
                iw10(k, 2, inn.as_mut_ptr());
            } else if k < 1000 {
                iw10(k, 3, inn.as_mut_ptr());
            } else if k < 10000 {
                iw10(k, 4, inn.as_mut_ptr());
            }
            trim_end(inn.as_mut_ptr());
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
        let (ntext, ntext_n) = read_whole_mg(nodepath.as_mut_ptr());
        let u12 = reader_new(ntext, ntext_n);
        // NOTE: j is a scalar (last value reused in the writes); z is the
        // case-folded loop... (here z IS declared REAL and reused as well).
        let mut jj: i64 = 0;
        for i in 0..ntn {
            *node.add(i) = grab_int(u12);
            jj = grab_int(u12);
            *xnode.add(i) = grab_f64(u12, scratchp);
            *ynode.add(i) = grab_f64(u12, scratchp);
            *znode.add(i) = grab_f64(u12, scratchp);
        }
        reader_stmt_end(u12);

        // Outside-DEM checks (messages + STOP).
        {
            let (mut xlo, mut xhi, mut ylo, mut yhi) =
                (c_double::INFINITY, c_double::NEG_INFINITY, c_double::INFINITY, c_double::NEG_INFINITY);
            let (mut bxlo, mut bxhi, mut bylo, mut byhi) =
                (c_double::INFINITY, c_double::NEG_INFINITY, c_double::INFINITY, c_double::NEG_INFINITY);
            for i in 0..ntn {
                let x = *xnode.add(i);
                let y = *ynode.add(i);
                if x < xlo { xlo = x; }
                if x > xhi { xhi = x; }
                if y < ylo { ylo = y; }
                if y > yhi { yhi = y; }
            }
            for i in 0..ns {
                let x = *xs.add(i);
                let y = *ys.add(i);
                if x < bxlo { bxlo = x; }
                if x > bxhi { bxhi = x; }
                if y < bylo { bylo = y; }
                if y > byhi { byhi = y; }
            }
            if xlo < bxlo || xhi > bxhi || ylo < bylo || yhi > byhi {
                puts(b" Some nodes are outside of the Surface DEM\0".as_ptr() as *const c_char);
                put_dem_minmax(b" DEM xmin, ymin, xmax, ymax: \0", bxlo, bylo, bxhi, byhi);
                put_dem_minmax(b" Mesh xmin, ymin, xmax, ymax: \0", xlo, ylo, xhi, yhi);
                exit(0);
            }
        }
        {
            let (mut xlo, mut xhi, mut ylo, mut yhi) =
                (c_double::INFINITY, c_double::NEG_INFINITY, c_double::INFINITY, c_double::NEG_INFINITY);
            let (mut bxlo, mut bxhi, mut bylo, mut byhi) =
                (c_double::INFINITY, c_double::NEG_INFINITY, c_double::INFINITY, c_double::NEG_INFINITY);
            for i in 0..ntn {
                let x = *xnode.add(i);
                let y = *ynode.add(i);
                if x < xlo { xlo = x; }
                if x > xhi { xhi = x; }
                if y < ylo { ylo = y; }
                if y > yhi { yhi = y; }
            }
            for i in 0..nb {
                let x = *xb.add(i);
                let y = *yb.add(i);
                if x < bxlo { bxlo = x; }
                if x > bxhi { bxhi = x; }
                if y < bylo { bylo = y; }
                if y > byhi { byhi = y; }
            }
            if xlo < bxlo || xhi > bxhi || ylo < bylo || yhi > byhi {
                puts(b" Some nodes are outside of the bedrock DEM\0".as_ptr() as *const c_char);
                put_dem_minmax(b" DEM xmin, ymin, xmax, ymax: \0", bxlo, bylo, bxhi, byhi);
                put_dem_minmax(b" Mesh xmin, ymin, xmax, ymax: \0", xlo, ylo, xhi, yhi);
                exit(0);
            }
        }

        // Per-node bilinear interpolation. Output buffer: fixed 89+1/line
        // (FORMAT(i10,2x,i5,3(2x,e22.15))).
        let out = malloc(ntn * 90 + 1) as *mut c_char;
        if out.is_null() {
            exit(2);
        }
        let mut opos = 0usize;
        for n in 0..ntn {
            let x = *xnode.add(n);
            let y = *ynode.add(n);
            let z = *znode.add(n);
            let zbed = dem_interp(
                x, y, xb0, yb0, dbx, dby, lbx, lby, nbx, xb, yb, zb, nb, &mut jj,
            );
            let zsurf = dem_interp(
                x, y, xs0, ys0, dsx, dsy, lsx, lsy, nsx, xs, ys, zs, ns, &mut jj,
            );
            let znew = zbed + z * (zsurf - zbed).max(hmin);
            opos += i_field(*node.add(n), 10, out.add(opos));
            *out.add(opos) = b' ' as c_char;
            *out.add(opos + 1) = b' ' as c_char;
            opos += 2;
            opos += i_field(jj, 5, out.add(opos));
            for v in [x, y, znew] {
                *out.add(opos) = b' ' as c_char;
                *out.add(opos + 1) = b' ' as c_char;
                opos += 2;
                opos += e22_15(v, out.add(opos));
            }
            *out.add(opos) = b'\n' as c_char;
            opos += 1;
        }
        // REWIND(12) + rewrite: same final bytes as the Fortran
        // read-then-rewind-then-write on one unit.
        write_whole_mg(nodepath.as_mut_ptr(), out, opos);
        free(out as *mut c_void);
        reader_free(u12);
        free(ntext as *mut c_void);
        free(node as *mut c_void);
        free(xnode as *mut c_void);
        free(ynode as *mut c_void);
        free(znode as *mut c_void);
    }

    free(xs as *mut c_void);
    free(ys as *mut c_void);
    free(zs as *mut c_void);
    free(xb as *mut c_void);
    free(yb as *mut c_void);
    free(zb as *mut c_void);

    puts(b" END WITH NO TROUBLE ...\0".as_ptr() as *const c_char);
    0
}

// Bilinear cell interpolation with the -9990 nodata policy (nearest-point
// fallback when the whole cell is void; cell mean otherwise). Bed and
// surface share this path; their messages are identical in the original.
#[allow(clippy::too_many_arguments)]
unsafe fn dem_interp(
    x: c_double,
    y: c_double,
    x0: c_double,
    y0: c_double,
    dx: c_double,
    dy: c_double,
    lx: c_double,
    ly: c_double,
    nx: usize,
    gx: *const c_double,
    gy: *const c_double,
    gz: *const c_double,
    n: usize,
    // Shared file-column scalar j: the mean branch's DO J=1,2 loop leaves
    // J=3 on completion (loop-exit value), clobbering later output lines.
    // Reproduced exactly (nearest/bilinear paths leave j untouched).
    jj: *mut i64,
) -> c_double {
    let mut ix = ((x - x0) / dx) as i64 + 1;
    if x == x0 + lx {
        ix -= 1;
    }
    let mut iy = ((y - y0) / dy) as i64 + 1;
    if y == y0 + ly {
        iy -= 1;
    }
    // ib is the Fortran 1-based cell base; ib0 is the 0-based element.
    let ib = (nx as i64 * (iy - 1) + ix) as usize;
    let ib0 = ib - 1;
    let x1 = *gx.add(ib0);
    let x2 = *gx.add(ib0 + 1);
    let y1 = *gy.add(ib0);
    let y2 = *gy.add(ib0 + nx);
    let mut zi = [[0.0 as c_double; 2]; 2];
    zi[0][0] = *gz.add(ib0);
    zi[1][0] = *gz.add(ib0 + 1);
    zi[1][1] = *gz.add(ib0 + nx + 1);
    zi[0][1] = *gz.add(ib0 + nx);
    if zi[0][0] < -9990.0 || zi[0][1] < -9990.0 || zi[1][0] < -9990.0 || zi[1][1] < -9990.0 {
        if zi[0][0] < -9990.0 && zi[0][1] < -9990.0 && zi[1][0] < -9990.0 && zi[1][1] < -9990.0 {
            // Nearest available point. (If none exists, imin is undefined
            // in the original — UB; corpus DEMs always carry valid points.)
            let mut rmin = 9999.0 as c_double;
            let mut imin = 0usize;
            for i in 0..n {
                if *gz.add(i) > -9990.0 {
                    let r = ((x - *gx.add(i)) * (x - *gx.add(i))
                        + (y - *gy.add(i)) * (y - *gy.add(i)))
                        .sqrt();
                    if r < rmin {
                        rmin = r;
                        imin = i;
                    }
                }
            }
            let zv = *gz.add(imin);
            let mut rec = [0 as c_char; 256];
            strcpy(
                rec.as_mut_ptr(),
                b" No data in the DEM cell \0".as_ptr() as *const c_char,
            );
            let mut pos = strlen(rec.as_ptr());
            pos += g0_int_field(ix, rec.as_mut_ptr().add(pos));
            pos += g0_int_field(iy, rec.as_mut_ptr().add(pos));
            strcpy(
                rec.as_mut_ptr().add(pos),
                b"  . Using the closest point at distance \0".as_ptr() as *const c_char,
            );
            pos = strlen(rec.as_ptr());
            pos += g0_f64_field(rmin, rec.as_mut_ptr().add(pos));
            pos += g0_f64_field(zv, rec.as_mut_ptr().add(pos));
            put_bytes(rec.as_mut_ptr(), pos);
            zv
        } else {
            let mut zv = 0.0 as c_double;
            let mut npt = 0i64;
            for row in zi.iter() {
                for v in row.iter() {
                    if *v > -9990.0 {
                        zv += *v;
                        npt += 1;
                    }
                }
            }
            zv /= npt as c_double;
            // DO J=1,2 completes: J (the shared column scalar) is 3.
            *jj = 3;
            let mut rec = [0 as c_char; 256];
            strcpy(rec.as_mut_ptr(), b" Missing data in cell \0".as_ptr() as *const c_char);
            let mut pos = strlen(rec.as_ptr());
            pos += g0_int_field(ix, rec.as_mut_ptr().add(pos));
            pos += g0_int_field(iy, rec.as_mut_ptr().add(pos));
            strcpy(
                rec.as_mut_ptr().add(pos),
                b" . Value \0".as_ptr() as *const c_char,
            );
            pos = strlen(rec.as_ptr());
            pos += g0_f64_field(zv, rec.as_mut_ptr().add(pos));
            strcpy(
                rec.as_mut_ptr().add(pos),
                b"  computed using \0".as_ptr() as *const c_char,
            );
            pos = strlen(rec.as_ptr());
            pos += g0_int_field(npt, rec.as_mut_ptr().add(pos));
            strcpy(rec.as_mut_ptr().add(pos), b" points.\0".as_ptr() as *const c_char);
            pos = strlen(rec.as_ptr());
            put_bytes(rec.as_mut_ptr(), pos);
            zv
        }
    } else {
        (zi[0][0] * (x2 - x) * (y2 - y)
            + zi[1][0] * (x - x1) * (y2 - y)
            + zi[0][1] * (x2 - x) * (y - y1)
            + zi[1][1] * (x - x1) * (y - y1))
            / (dx * dy)
    }
}

// ---- helpers ---------------------------------------------------------------

unsafe fn read_whole_mg(path: *const c_char) -> (*mut c_char, usize) {
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

unsafe fn write_whole_mg(path: *const c_char, data: *const c_char, n: usize) {
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

unsafe fn put_bytes(buf: *mut c_char, n: usize) {
    *buf.add(n) = 0;
    puts(buf as *const c_char);
}

unsafe fn trim_end(b: *mut c_char) {
    let mut n = strlen(b);
    while n > 0 && *b.add(n - 1) as u8 == b' ' {
        n -= 1;
    }
    *b.add(n) = 0;
}

/// Two list items from ONE statement (one record): e.g. READ(10,*)Nsx,Nsy.
#[inline(always)]
unsafe fn take_pair_int(r: *mut SeqReader) -> (i64, i64) {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let a = tok_int(p, l);
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let b = tok_int(p, l);
    reader_stmt_end(r);
    (a, b)
}

/// Two reals from ONE statement.
#[inline(always)]
unsafe fn take_pair_f64(r: *mut SeqReader, scratchp: *mut c_char) -> (c_double, c_double) {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let a = tok_f64(p, l, scratchp);
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let b = tok_f64(p, l, scratchp);
    reader_stmt_end(r);
    (a, b)
}

unsafe fn take_char(r: *mut SeqReader, n: usize, out: *mut c_char) {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    tok_char(p, l, out, n);
    reader_stmt_end(r);
}

unsafe fn take_f64(r: *mut SeqReader, scratchp: *mut c_char) -> c_double {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let v = tok_f64(p, l, scratchp);
    reader_stmt_end(r);
    v
}

unsafe fn take_int(r: *mut SeqReader) -> i64 {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    let v = tok_int(p, l);
    reader_stmt_end(r);
    v
}

unsafe fn grab_f64(r: *mut SeqReader, scratchp: *mut c_char) -> c_double {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    tok_f64(p, l, scratchp)
}

unsafe fn grab_int(r: *mut SeqReader) -> i64 {
    let (p, l) = reader_tok(r);
    if p.is_null() {
        eof_fail_mg();
    }
    tok_int(p, l)
}

unsafe fn put_dem_minmax(
    msg: &[u8],
    x0: c_double,
    y0: c_double,
    x1: c_double,
    y1: c_double,
) {
    let mut rec = [0 as c_char; 256];
    strcpy(rec.as_mut_ptr(), msg.as_ptr() as *const c_char);
    let mut pos = strlen(rec.as_ptr());
    for v in [x0, y0, x1, y1] {
        pos += g0_f64_field(v, rec.as_mut_ptr().add(pos));
    }
    put_bytes(rec.as_mut_ptr(), pos);
}

unsafe fn eof_fail_mg() -> ! {
    fprintf(
        c_stderr(),
        b"Fortran runtime error: End of file\n\0".as_ptr() as *const c_char,
    );
    exit(2);
}
