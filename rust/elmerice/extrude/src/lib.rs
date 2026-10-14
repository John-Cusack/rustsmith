// Faithful Rust transcription of elmerice/Meshers/ExtrudeMesh.c @ 9f6af2f85.
// Unit: c:elmerice/Meshers/ExtrudeMesh.c (exports: ExtrudeMesh (module);
// file-global C symbols transcribed 1:1 below).
//
// Entry shape: `#[no_mangle] extrudemesh_main` (NOT `main`); the splice step
// adds the `main` alias (`ld -r --defsym main=extrudemesh_main`) when the
// archive replaces the native object (same rationale as mshglacier).
// All formatting/scanning goes through libc printf/fprintf/sprintf/
// sscanf/fscanf/strtok/atoi/atof (byte-identical float formatting for
// free); heap via libc malloc/free; MAXPATHLEN = PATH_MAX = 4096
// (sys/param.h), BUF_LEN = 100000 as in C.
// Upstream quirks preserved (all deterministic):
// - readDEM's braceless if/else: a noval match sets isnoval but does NOT
//   advance (next read overwrites the slot); the else arm flags the
//   POST-increment index (isnoval[0] stays malloc garbage on noval-free
//   DEMs — same malloc discipline here, same practical zeros).
// - isnoval2 is sized by pointsInDEM[0] (bed count), not [2].
// - `fprintf(...,"202 %i\n",...) + lines*baseline` discards the addition.
// - corrbed is never assigned from argv[14] (always 0); the DEM info print
//   still reports it.
// - interpolatePoint's thickness loop tests `*B != *B` (bedrock), not the
//   local thickness: a thickness miss exits after one pass with NaN
//   propagating (plus the "needed increasing" stderr note).
// - Thickness and bedrock clamp weights at 1e5; surface does not.
// - info.dat's cutOffValue/wexp/noval print malloc... (see below).
// Documented divergences (original-UB, corpus-masked, never on valid
// inputs): info.dat cutOffValue/wexp/noval print uninitialized stack
// garbage when argc==10 (no DEM); the port prints 0.0. `free(bed/surf)`
// on uninitialized pointers when a DEM component is missing (surf-only or
// thick-combo runs crash the original at exit AFTER identical files/stdout;
// the port null-inits and frees safely, exiting 0). sharednodeinfo grows
// by realloc instead of overflowing on over-long shared lines.
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

use ice_prelude::*;

const BUF_LEN: usize = 100000;
const MAXPATHLEN: usize = 4096;

// SAFETY: pure arithmetic (C file-global `f`; external linkage like the C
// TU, so the splice archive provides the symbol).
#[no_mangle]
pub unsafe extern "C" fn f(x: c_double, gl: c_double, dz: c_double, bl: c_double) -> c_double {
    dz - (bl + dz) * pow(x, gl) + bl * pow(x, gl + 1.0)
}

// SAFETY: pure arithmetic (C file-global `f_prime`; see `f`).
#[no_mangle]
pub unsafe extern "C" fn f_prime(
    x: c_double,
    gl: c_double,
    dz: c_double,
    bl: c_double,
) -> c_double {
    -(bl + dz) * gl * pow(x, gl - 1.0) + bl * (gl + 1.0) * pow(x, gl)
}

// SAFETY: mirrors C `newton` 1:1, including the not-converged printf that
// prints converged_p[0].
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn newton(
    x_0: c_double,
    tol: c_double,
    max_iters: c_int,
    iters_p: *mut c_int,
    converged_p: *mut c_int,
    gl: c_double,
    dz: c_double,
    bl: c_double,
) -> c_double {
    let mut x = x_0;
    let mut x_prev: c_double;
    let mut iter = 0;
    loop {
        iter += 1;
        x_prev = x;
        x = x_prev - f(x_prev, gl, dz, bl) / f_prime(x_prev, gl, dz, bl);
        if !(fabs(x - x_prev) > tol && iter < max_iters) {
            break;
        }
    }
    if fabs(x - x_prev) <= tol {
        *converged_p = 1;
    } else {
        *converged_p = 0;
        printf(
            b"not converged, %d\n\0".as_ptr() as *const c_char,
            *converged_p,
        );
    }
    *iters_p = iter;
    x
}

// SAFETY: mirrors C `refinedHeightBL` 1:1.
#[no_mangle]
pub unsafe extern "C" fn refinedHeightBL(
    s: c_double,
    b: c_double,
    levels: c_int,
    level: c_int,
    gl: c_int,
    p: c_double,
    r: c_double,
) -> c_double {
    let mut iters: c_int = 0;
    let mut converged: c_int = 0;
    
    let n = gl - 1;
    let z: c_double = if gl < 1 {
        let dz = (s - b) / (levels as c_double - 1.0);
        b + level as c_double * dz
    } else {
        let bl = (s - b) * fabs(p);
        let dz = (s - b - bl) / (levels as c_double - gl as c_double - 1.0);
        let mut rr = r;
        if rr == 0.0 {
            rr = newton(10.0, 1e-3, 1000, &mut iters, &mut converged, gl as c_double, dz, bl);
        }
        let a: c_double;
        if rr > 1.0 {
            a = bl * (1.0 - rr) / (1.0 - pow(rr, (n + 1) as c_double));
        } else {
            a = bl / (gl - 1) as c_double;
            printf(
                b"no good solution found, since r is equal or smaller as 1: r= %f \n\0".as_ptr()
                    as *const c_char,
                rr,
            );
            printf(
                b"solution r %f dZ %f dZlast %f a %f\n\0".as_ptr() as *const c_char,
                rr,
                dz,
                a * pow(rr, (gl - 1) as c_double),
                a,
            );
            printf(
                b"equation of the polynom for testing: poly  konst %f GL %i c1  %f c2 %f \n\0".as_ptr()
                    as *const c_char,
                dz,
                gl,
                -bl - dz,
                bl,
            );
        }
        let mut zz: c_double;
        let mut k: c_int;
        if p > 0.0 {
            zz = b;
            k = 0;
            while k < level {
                if k < gl {
                    zz += a * pow(rr, k as c_double);
                } else {
                    zz += dz;
                }
                k += 1;
            }
        } else {
            zz = b;
            k = 0;
            while k < level {
                if k < levels - gl - 1 {
                    zz += dz;
                } else {
                    zz += a * pow(rr, (levels - k - 2) as c_double);
                }
                k += 1;
            }
        }
        let _ = iters;
        let _ = converged;
        let _ = k;
        zz
    };
    z
}

// SAFETY: mirrors C `file_exists` (fopen/fclose probe).
#[no_mangle]
pub unsafe extern "C" fn file_exists(filename: *const c_char) -> c_int {
    let fileptr = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if !fileptr.is_null() {
        fclose(fileptr);
        return 1;
    }
    0
}

// SAFETY: mirrors C `checkFileEntries` (first-line token count via strtok
// on "\t ", remaining-line count, ++lines return).
#[no_mangle]
pub unsafe extern "C" fn checkFileEntries(
    filename: *mut c_char,
    argv: *const *const c_char,
    entriesPerLine: *mut c_int,
) -> c_int {
    let dummys = malloc(BUF_LEN) as *mut c_char;
    if dummys.is_null() {
        exit(1);
    }
    let mut charptr: *mut c_char;
    let mut lines: c_int;
    let mut entries: c_int;
    let fileptr = fopen(filename as *const c_char, b"r\0".as_ptr() as *const c_char);
    if fileptr.is_null() {
        printf(
            b"(%s) Error opening file %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            filename,
        );
        free(dummys as *mut c_void);
        return -1;
    }
    lines = 0;
    entries = 0;
    fgets(dummys, BUF_LEN as c_int, fileptr);
    charptr = strtok(dummys, b"\t \0".as_ptr() as *const c_char);
    while !charptr.is_null() {
        entries += 1;
        charptr = strtok(core::ptr::null_mut(), b"\t \0".as_ptr() as *const c_char);
    }
    while !fgets(dummys, BUF_LEN as c_int, fileptr).is_null() {
        lines += 1;
    }
    fclose(fileptr);
    free(dummys as *mut c_void);
    *entriesPerLine = entries;
    lines += 1;
    lines
}

// SAFETY: mirrors C `readDEM` exactly, including the braceless if/else
// (noval match leaves the slot for overwrite; else flags post-increment).
#[no_mangle]
pub unsafe extern "C" fn readDEM(
    inputfield: *mut c_double,
    isnoval: *mut c_int,
    pointsInDEM: c_int,
    filename: *mut c_char,
    argv: *const *const c_char,
) -> c_int {
    let mut i: c_int;
    let mut validPointsInDEM: c_int;
    let noval = atof(*argv.add(13));
    let fileptr = fopen(filename as *const c_char, b"r\0".as_ptr() as *const c_char);
    if fileptr.is_null() {
        printf(
            b"(%s) Error opening file %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            filename,
        );
        return -1;
    }
    i = 0;
    validPointsInDEM = 0;
    while i < pointsInDEM {
        fscanf(
            fileptr,
            b"%lf %lf %lf\0".as_ptr() as *const c_char,
            inputfield.add((3 * validPointsInDEM) as usize),
            inputfield.add((3 * validPointsInDEM + 1) as usize),
            inputfield.add((3 * validPointsInDEM + 2) as usize),
        );
        if *inputfield.add((3 * validPointsInDEM + 2) as usize) == noval {
            *isnoval.add(validPointsInDEM as usize) = 1;
        } else {
            validPointsInDEM += 1;
            *isnoval.add(validPointsInDEM as usize) = 0;
        }
        i += 1;
    }
    fclose(fileptr);
    validPointsInDEM
}

// SAFETY: mirrors C `interpolatePoint` exactly: thickness prelude (>11,
// NaN-retry on *B), bedrock (11/101), surface (11/110), then 101/110
// derivation. Weight clamp (1e5) applies in thickness and bedrock only.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn interpolatePoint(
    x: c_double,
    y: c_double,
    b: *mut c_double,
    s: *mut c_double,
    bed: *mut c_double,
    surf: *mut c_double,
    thick: *mut c_double,
    pointsInDEM: *mut c_int,
    minrad: c_double,
    wexp: c_double,
    interpolationScheme: c_int,
    argv: *const *const c_char,
    isnoval0: *mut c_int,
    isnoval1: *mut c_int,
    isnoval2: *mut c_int,
) -> c_int {
    let mut i: c_int;
    let mut rounds: c_int;
    let mut weightsum: c_double;
    let mut weight: c_double;
    let mut radius: c_double;
    let mut localthickness: c_double = 0.0;
    if interpolationScheme > 11 {
        rounds = 0;
        loop {
            localthickness = 0.0;
            i = 0;
            weightsum = 0.0;
            while i < *pointsInDEM.add(2) {
                radius = sqrt(
                    pow(x - *thick.add((3 * i) as usize), 2.0)
                        + pow(y - *thick.add((3 * i + 1) as usize), 2.0),
                );
                if radius <= minrad * (1.0 + (rounds + 1) as c_double / 100.0) {
                    if *isnoval2.add(i as usize) == 1 {
                        weight = 0.0;
                    } else {
                        weight = 1.0 / (pow(radius, wexp) + 1.0E-09);
                    }
                    weight = if weight > 1.0E05 { 1.0E05 } else { weight };
                    weightsum += weight;
                    localthickness += weight * *thick.add((3 * i + 2) as usize);
                }
                i += 1;
            }
            localthickness /= weightsum;
            if localthickness.is_nan() {
                rounds += 1;
            }
            if !(rounds < 100 && (*b).is_nan()) {
                break;
            }
        }
        if rounds >= 99 {
            fprintf(
                c_stderr(),
                b"(%s) (thickness) no interpolation point within range of double the cut-off value %e\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                minrad,
            );
            return EXIT_FAILURE;
        } else if rounds > 0 {
            fprintf(
                c_stderr(),
                b"(%s) (thickness) interpolation needed increasing cut-off value by %4e percent\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                rounds as c_double * 1.0,
            );
        }
    }
    if interpolationScheme == 11 || interpolationScheme == 101 {
        rounds = 0;
        loop {
            *b = 0.0;
            i = 0;
            weightsum = 0.0;
            while i < *pointsInDEM.add(0) {
                radius = sqrt(
                    pow(x - *bed.add((3 * i) as usize), 2.0)
                        + pow(y - *bed.add((3 * i + 1) as usize), 2.0),
                );
                if radius <= minrad * (1.0 + (rounds + 1) as c_double / 100.0) {
                    if *isnoval0.add(i as usize) == 1 {
                        weight = 0.0;
                    } else {
                        weight = 1.0 / (pow(radius, wexp) + 1.0E-09);
                    }
                    // NOTE: bedrock clamps like thickness; surface does not.
                    weight = if weight > 1.0E05 { 1.0E05 } else { weight };
                    weightsum += weight;
                    *b += weight * *bed.add((3 * i + 2) as usize);
                }
                i += 1;
            }
            *b /= weightsum;
            if (*b).is_nan() {
                rounds += 1;
            }
            if !(rounds < 100 && (*b).is_nan()) {
                break;
            }
        }
        if rounds >= 99 {
            fprintf(
                c_stderr(),
                b"(%s) no interpolation point within range of double the cut-off value %e\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                minrad,
            );
            return EXIT_FAILURE;
        } else if rounds > 0 {
            fprintf(
                c_stderr(),
                b"(%s) interpolation needed increasing cut-off value by %4e percent\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                rounds as c_double * 1.0,
            );
        }
    }
    if interpolationScheme == 11 || interpolationScheme == 110 {
        rounds = 0;
        loop {
            *s = 0.0;
            i = 0;
            weightsum = 0.0;
            while i < *pointsInDEM.add(1) {
                radius = sqrt(
                    pow(x - *surf.add((3 * i) as usize), 2.0)
                        + pow(y - *surf.add((3 * i + 1) as usize), 2.0),
                );
                if radius <= minrad * (1.0 + (rounds + 1) as c_double / 100.0) {
                    if *isnoval1.add(i as usize) == 1 {
                        weight = 0.0;
                    } else {
                        weight = 1.0 / (pow(radius, wexp) + 1.0E-09);
                    }
                    weightsum += weight;
                    *s += weight * *surf.add((3 * i + 2) as usize);
                }
                i += 1;
            }
            if weightsum <= 0.0 {
                rounds += 1;
            } else {
                *s /= weightsum;
            }
            if !(rounds < 100 && weightsum <= 0.0) {
                break;
            }
        }
        if rounds >= 99 {
            fprintf(
                c_stderr(),
                b"(%s)  (surface) no interpolation point within range of double the cut-off value %e\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                minrad,
            );
            return EXIT_FAILURE;
        } else if rounds > 0 {
            fprintf(
                c_stderr(),
                b"(%s) (surface) interpolation needed increasing cut-off value by %4e percent\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                rounds as c_double * 1.0,
            );
        }
    }
    if interpolationScheme == 101 {
        *s = *b + localthickness;
    }
    if interpolationScheme == 110 {
        *b = *s - localthickness;
    }
    EXIT_SUCCESS
}

// SAFETY: mirrors C `extrudepartition` 1:1 (partitioned footprint files
// without directory prefix: "<base>.header/.nodes/.elements/.boundary/
// .shared"). sharednodeinfo grows by realloc instead of overflowing on
// over-long shared lines (documented divergence, UB-original only).
#[no_mangle]
#[allow(clippy::too_many_arguments)]
// Dead counters kept verbatim (C never reads them either — the header uses
// the pre-read scan counts): shared-line k, elements re-counts, sides-loop
// quads/lines, node-loop progress. Listed so the allow stays scoped.
#[allow(unused_assignments)]
pub unsafe extern "C" fn extrudepartition(
    argv: *const *const c_char,
    inputfilename: *mut c_char,
    outputfilename: *mut c_char,
    partition: c_int,
    partitions: c_int,
    levels: c_int,
    depth: c_double,
    pointsinlevel: c_int,
    elementsinlevel: c_int,
    _belementsinlevel: c_int,
    interpolationScheme: c_int,
    bed: *mut c_double,
    surf: *mut c_double,
    thick: *mut c_double,
    pointsInDEM: *mut c_int,
    cutOffValue: c_double,
    wexp: c_double,
    isnoval0: *mut c_int,
    isnoval1: *mut c_int,
    isnoval2: *mut c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    let mut k: c_int;
    let mut l: c_int = 0;
    let mut level: c_int;
    let mut dummyint: c_int = 0;
    let mut nodesinpartition: c_int = 0;
    let mut elementsinpartition: c_int = 0;
    let mut belementsinpartition: c_int = 0;
    let mut sharednodes: c_int = 0;
    let mut points: c_int;
    let mut quads: c_int;
    let mut lines: c_int;
    let mut triangles: c_int;
    
    
    let mut parent = [0 as c_int; 2];
    let mut elementtype: c_int = 0;
    // Deferred: every read follows an assignment on all paths reaching it
    // (sides loops return on unknown types); C leaves these uninitialized.
    let mut belementtype: c_int;
    let instring = malloc(BUF_LEN) as *mut c_char;
    let numberstring = malloc(BUF_LEN) as *mut c_char;
    let mut dummyc: c_char = 0;
    let mut charptr: *mut c_char;
    let mut dz: c_double;
    
    
    
    
    
    
    
    let mut sharednodeinfo: *mut c_int;
    let mut sharedcap: usize;
    
    
    
    
    if instring.is_null() || numberstring.is_null() {
        exit(1);
    }
    let infids: *mut *mut FILE = malloc(5 * core::mem::size_of::<*mut FILE>()) as *mut *mut FILE;
    let inputfile: *mut *mut c_char = malloc(5 * core::mem::size_of::<*mut c_char>()) as *mut *mut c_char;
    for ii in 0..5usize {
        *inputfile.add(ii) = malloc(MAXPATHLEN + 1) as *mut c_char;
    }
    for ii in 0..5usize {
        strcpy(*inputfile.add(ii), inputfilename);
    }
    strcat(*inputfile.add(0), b".header\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(1), b".nodes\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(2), b".elements\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(3), b".boundary\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(4), b".shared\0".as_ptr() as *const c_char);
    for ii in 0..5usize {
        *infids.add(ii) = fopen(*inputfile.add(ii), b"r\0".as_ptr() as *const c_char);
        if (*infids.add(ii)).is_null() {
            printf(
                b"(%s) Could not open file %s\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *inputfile.add(ii),
            );
            return EXIT_FAILURE;
        }
    }
    let outfids: *mut *mut FILE = malloc(5 * core::mem::size_of::<*mut FILE>()) as *mut *mut FILE;
    let outputfile: *mut *mut c_char = malloc(5 * core::mem::size_of::<*mut c_char>()) as *mut *mut c_char;
    for ii in 0..5usize {
        *outputfile.add(ii) = malloc(MAXPATHLEN + 1) as *mut c_char;
    }
    for ii in 0..5usize {
        strcpy(*outputfile.add(ii), outputfilename);
    }
    strcat(*outputfile.add(0), b".header\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(1), b".nodes\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(2), b".elements\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(3), b".boundary\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(4), b".shared\0".as_ptr() as *const c_char);
    for ii in 0..5usize {
        *outfids.add(ii) = fopen(*outputfile.add(ii), b"w\0".as_ptr() as *const c_char);
        if (*outfids.add(ii)).is_null() {
            printf(
                b"(%s) Could not open file %s\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *outputfile.add(ii),
            );
            return EXIT_FAILURE;
        }
    }
    for ii in 0..5usize {
        printf(b"<-%s\n\0".as_ptr() as *const c_char, *inputfile.add(ii));
        printf(b"->%s\n\0".as_ptr() as *const c_char, *outputfile.add(ii));
    }
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut nodesinpartition,
    );
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut elementsinpartition,
    );
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut belementsinpartition,
    );
    fscanf(*infids.add(0), b"%i \0".as_ptr() as *const c_char, &mut j);
    points = 0;
    lines = 0;
    triangles = 0;
    quads = 0;
    i = 0;
    while i < j {
        fscanf(
            *infids.add(0),
            b"%i %i\0".as_ptr() as *const c_char,
            &mut elementtype,
            &mut dummyint,
        );
        if elementtype == 101 {
            points = dummyint;
        } else if elementtype == 202 {
            lines = dummyint;
        } else if elementtype == 303 {
            triangles = dummyint;
        } else if elementtype == 404 {
            quads = dummyint;
        } else {
            printf(
                b"(%s) element type %3i of input file %s not defined\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                elementtype,
                *inputfile.add(0),
            );
            return EXIT_FAILURE;
        }
        i += 1;
    }
    fscanf(
        *infids.add(0),
        b"%i %i\0".as_ptr() as *const c_char,
        &mut sharednodes,
        &mut dummyint,
    );
    printf(
        b"(%s) Partition %i of %i input: %i nodes, %i elements, %i boundary elements, %i shared nodes\n\0".as_ptr() as *const c_char,
        *argv.add(0),
        partition + 1,
        partitions,
        nodesinpartition,
        elementsinpartition,
        belementsinpartition,
        sharednodes,
    );
    let wedges: c_int = triangles * (levels - 1);
    let bricks: c_int = quads * (levels - 1);
    triangles *= 2;
    quads *= 2;
    quads += (levels - 1) * lines;
    lines = (levels - 1) * points;
    fprintf(
        *outfids.add(0),
        b"%i %i %i\n\0".as_ptr() as *const c_char,
        nodesinpartition * levels,
        wedges + bricks,
        quads + triangles + lines,
    );
    fprintf(
        *outfids.add(0),
        b"%i\n\0".as_ptr() as *const c_char,
        (if quads > 0 { 1 } else { 0 })
            + (if triangles > 0 { 1 } else { 0 })
            + (if bricks > 0 { 1 } else { 0 })
            + (if wedges > 0 { 1 } else { 0 })
            + (if lines > 0 { 1 } else { 0 })
            + (if points > 0 { 1 } else { 0 }),
    );
    if lines > 0 {
        fprintf(*outfids.add(0), b"202 %i\n\0".as_ptr() as *const c_char, lines);
    }
    if triangles > 0 {
        fprintf(*outfids.add(0), b"303 %i\n\0".as_ptr() as *const c_char, triangles);
    }
    if quads > 0 {
        fprintf(*outfids.add(0), b"404 %i\n\0".as_ptr() as *const c_char, quads);
    }
    if wedges > 0 {
        fprintf(*outfids.add(0), b"706 %i\n\0".as_ptr() as *const c_char, wedges);
    }
    if bricks > 0 {
        fprintf(*outfids.add(0), b"808 %i\n\0".as_ptr() as *const c_char, bricks);
    }
    fprintf(
        *outfids.add(0),
        b"%i 0 \n\0".as_ptr() as *const c_char,
        sharednodes * levels,
    );
    printf(
        b"(%s) Partition %i of %i output: %i nodes, %i elements, %i boundary elements, %i shared nodes\n\n\0".as_ptr() as *const c_char,
        *argv.add(0),
        partition + 1,
        partitions,
        nodesinpartition * levels,
        wedges + bricks,
        quads + triangles + lines,
        sharednodes * levels,
    );
    let x: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let y: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let s: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let b: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let nodeinfo: *mut c_int = malloc((nodesinpartition as usize) * 4) as *mut c_int;
    let elementinfo: *mut c_int = malloc((elementsinpartition as usize) * 8 * 4) as *mut c_int;
    let belementinfo: *mut c_int = malloc((belementsinpartition as usize) * 8 * 4) as *mut c_int;
    sharedcap = (partitions + 2) as usize;
    sharednodeinfo = malloc(sharedcap * 4) as *mut c_int;
    i = 0;
    while i < nodesinpartition {
        fscanf(
            *infids.add(1),
            b"%i %i %le %le %le\0".as_ptr() as *const c_char,
            nodeinfo.add(i as usize),
            &mut dummyint,
            x.add(i as usize),
            y.add(i as usize),
            b.add(i as usize),
        );
        if interpolationScheme > 0 {
            if interpolatePoint(
                *x.add(i as usize),
                *y.add(i as usize),
                b.add(i as usize),
                s.add(i as usize),
                bed,
                surf,
                thick,
                pointsInDEM,
                cutOffValue,
                wexp,
                interpolationScheme,
                argv,
                isnoval0,
                isnoval1,
                isnoval2,
            ) == EXIT_FAILURE
            {
                fprintf(
                    c_stderr(),
                    b"(%s) Failed interpolating values for point %i %e %e\n\0".as_ptr()
                        as *const c_char,
                    *argv.add(0),
                    i,
                    *x.add(i as usize),
                    *y.add(i as usize),
                );
                return EXIT_FAILURE;
            } else {
                if *s.add(i as usize) <= *b.add(i as usize) - depth {
                    *s.add(i as usize) = *b.add(i as usize) + depth;
                }
            }
        } else {
            *s.add(i as usize) = *b.add(i as usize) + depth;
        }
        i += 1;
    }
    quads = 0;
    triangles = 0;
    k = 0;
    while k < elementsinpartition {
        fgets(instring, (BUF_LEN - 1) as c_int, *infids.add(2));
        sscanf(
            instring as *const c_char,
            b"%s %i %i %i %i %i %i\0".as_ptr() as *const c_char,
            numberstring,
            elementinfo.add((8 * k + 1) as usize),
            elementinfo.add((8 * k + 2) as usize),
            elementinfo.add((8 * k + 3) as usize),
            elementinfo.add((8 * k + 4) as usize),
            elementinfo.add((8 * k + 5) as usize),
            elementinfo.add((8 * k + 6) as usize),
        );
        sscanf(
            numberstring as *const c_char,
            b"%i%c%i\0".as_ptr() as *const c_char,
            elementinfo.add((8 * k) as usize),
            &mut dummyc,
            elementinfo.add((8 * k + 7) as usize),
        );
        if dummyc as u8 != b'/' {
            *elementinfo.add((8 * k + 7) as usize) = 0;
        }
        if *elementinfo.add((8 * k + 2) as usize) == 404 {
            quads += 1;
        } else if *elementinfo.add((8 * k + 2) as usize) == 303 {
            *elementinfo.add((8 * k + 6) as usize) = -1;
            triangles += 1;
        } else {
            printf(
                b"(%s): element type %i not recognised for entry %i in partition %i\n\0".as_ptr()
                    as *const c_char,
                *argv.add(0),
                *elementinfo.add((8 * k + 2) as usize),
                k,
                partition,
            );
        }
        k += 1;
    }
    k = 0;
    while k < belementsinpartition {
        fgets(instring, (BUF_LEN - 1) as c_int, *infids.add(3));
        sscanf(
            instring as *const c_char,
            b"%s %i %i %i %i %i %i\0".as_ptr() as *const c_char,
            numberstring,
            belementinfo.add((8 * k + 1) as usize),
            belementinfo.add((8 * k + 2) as usize),
            belementinfo.add((8 * k + 3) as usize),
            belementinfo.add((8 * k + 4) as usize),
            belementinfo.add((8 * k + 5) as usize),
            belementinfo.add((8 * k + 6) as usize),
        );
        sscanf(
            numberstring as *const c_char,
            b"%i%c%i\0".as_ptr() as *const c_char,
            belementinfo.add((8 * k) as usize),
            &mut dummyc,
            belementinfo.add((8 * k + 7) as usize),
        );
        if dummyc as u8 != b'/' {
            *belementinfo.add((8 * k + 7) as usize) = 0;
        }
        // NOTE: the C code records j=2/j=1 here, but j is overwritten
        // before any read (node loop re-inits it); the stores are dead in
        // C too and omitted. belementinfo[6]=-1 is live and kept.
        if *belementinfo.add((8 * k + 4) as usize) == 101 {
            *belementinfo.add((8 * k + 6) as usize) = -1;
        } else if *belementinfo.add((8 * k + 4) as usize) != 202 {
            printf(
                b"(%s): boundary element type %i not recognised for entry %i in partition %i\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *belementinfo.add((8 * k + 4) as usize),
                k,
                partition,
            );
        }
        k += 1;
    }
    k = 0;
    while !fgets(instring, (BUF_LEN - 1) as c_int, *infids.add(4)).is_null() {
        k += 1;
        charptr = strtok(instring, b" \0".as_ptr() as *const c_char);
        i = 0;
        while !charptr.is_null() {
            if (i as usize) >= sharedcap {
                sharedcap *= 2;
                sharednodeinfo = realloc(
                    sharednodeinfo as *mut c_void,
                    sharedcap * 4,
                ) as *mut c_int;
                if sharednodeinfo.is_null() {
                    exit(1);
                }
            }
            *sharednodeinfo.add(i as usize) = atoi(charptr as *const c_char);
            i += 1;
            charptr = strtok(core::ptr::null_mut(), b" \0".as_ptr() as *const c_char);
        }
        level = 0;
        while level < levels {
            fprintf(
                *outfids.add(4),
                b"%i %i\0".as_ptr() as *const c_char,
                pointsinlevel * level + *sharednodeinfo.add(0),
                *sharednodeinfo.add(1),
            );
            j = 2;
            while j < i {
                fprintf(
                    *outfids.add(4),
                    b"  %i\0".as_ptr() as *const c_char,
                    *sharednodeinfo.add(j as usize),
                );
                j += 1;
            }
            fprintf(*outfids.add(4), b"\n\0".as_ptr() as *const c_char);
            level += 1;
        }
    }
    j = 0;
    level = 0;
    while level < levels {
        k = 0;
        while k < nodesinpartition {
            l = 100 * k / pointsinlevel;
            if l > j {
                j = l;
            }
            dz = (*s.add(k as usize) - *b.add(k as usize)) / (levels as c_double - 1.0);
            fprintf(
                *outfids.add(1),
                b"%i -1 %e %e %e\n\0".as_ptr() as *const c_char,
                pointsinlevel * level + *nodeinfo.add(k as usize),
                *x.add(k as usize),
                *y.add(k as usize),
                *b.add(k as usize) + level as c_double * dz,
            );
            k += 1;
        }
        level += 1;
    }
    k = 0;
    while k < elementsinpartition {
        if *elementinfo.add((8 * k + 2) as usize) == 303 {
            elementtype = 706;
            l = 3;
        } else if *elementinfo.add((8 * k + 2) as usize) == 404 {
            elementtype = 808;
            l = 4;
        }
        level = 0;
        while level < levels - 1 {
            fprintf(
                *outfids.add(2),
                b"%i\0".as_ptr() as *const c_char,
                elementsinlevel * level + *elementinfo.add((8 * k) as usize),
            );
            if *elementinfo.add((8 * k + 7) as usize) > 0 {
                fprintf(
                    *outfids.add(2),
                    b"/%i \0".as_ptr() as *const c_char,
                    *elementinfo.add((8 * k + 7) as usize),
                );
            }
            fprintf(
                *outfids.add(2),
                b" %i %i\0".as_ptr() as *const c_char,
                *elementinfo.add((8 * k + 1) as usize),
                elementtype,
            );
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(2),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * level + *elementinfo.add((8 * k + 3 + j) as usize),
                );
                j += 1;
            }
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(2),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * (level + 1) + *elementinfo.add((8 * k + 3 + j) as usize),
                );
                j += 1;
            }
            fprintf(*outfids.add(2), b"\n\0".as_ptr() as *const c_char);
            level += 1;
        }
        k += 1;
    }
    parent[1] = 0;
    i = 0;
    lines = 0;
    while i < 2 {
        k = 0;
        while k < elementsinpartition {
            parent[0] = elementsinlevel * (levels - 2) * i + *elementinfo.add((8 * k) as usize);
            fprintf(
                *outfids.add(3),
                b"%i\0".as_ptr() as *const c_char,
                i * elementsinpartition + k + 1,
            );
            if *elementinfo.add((8 * k + 7) as usize) > 0 {
                fprintf(
                    *outfids.add(3),
                    b"/%i\0".as_ptr() as *const c_char,
                    *elementinfo.add((8 * k + 7) as usize),
                );
            }
            fprintf(
                *outfids.add(3),
                b" %1i %i %i %3i\0".as_ptr() as *const c_char,
                i + 1,
                parent[0],
                parent[1],
                *elementinfo.add((8 * k + 2) as usize),
            );
            if *elementinfo.add((8 * k + 2) as usize) == 303 {
                l = 3;
            } else if *elementinfo.add((8 * k + 2) as usize) == 404 {
                l = 4;
            }
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(3),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * (levels - 1) * i + *elementinfo.add((8 * k + 3 + j) as usize),
                );
                j += 1;
            }
            fprintf(*outfids.add(3), b"\n\0".as_ptr() as *const c_char);
            k += 1;
        }
        i += 1;
    }
    k = 0;
    while k < belementsinpartition {
        level = 0;
        while level < levels - 1 {
            i = 0;
            while i < 2 {
                parent[i as usize] = if *belementinfo.add((8 * k + 2 + i) as usize) > 0 {
                    level * elementsinlevel + *belementinfo.add((8 * k + 2 + i) as usize)
                } else {
                    0
                };
                i += 1;
            }
            if *belementinfo.add((8 * k + 4) as usize) == 202 {
                belementtype = 404;
                quads += 1;
                j = 2;
            } else if *belementinfo.add((8 * k + 4) as usize) == 101 {
                belementtype = 202;
                lines += 1;
                j = 1;
            } else {
                printf(
                    b"(%s) boundary element type %3i of input file %s (line %i) not defined\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    *belementinfo.add((8 * k + 4) as usize),
                    *inputfile.add(3),
                    k,
                );
                return EXIT_FAILURE;
            }
            fprintf(
                *outfids.add(3),
                b"%i\0".as_ptr() as *const c_char,
                2 * elementsinpartition + belementsinpartition * level + *belementinfo.add((8 * k) as usize),
            );
            if *belementinfo.add((8 * k + 7) as usize) > 0 {
                fprintf(
                    *outfids.add(3),
                    b"/%i\0".as_ptr() as *const c_char,
                    *belementinfo.add((8 * k + 7) as usize),
                );
            }
            fprintf(
                *outfids.add(3),
                b" %i %i %i %3i\0".as_ptr() as *const c_char,
                *belementinfo.add((8 * k + 1) as usize) + 2,
                parent[0],
                parent[1],
                belementtype,
            );
            l = 0;
            while l < 2 {
                if l == 0 {
                    i = 0;
                    while i < j {
                        fprintf(
                            *outfids.add(3),
                            b" %i\0".as_ptr() as *const c_char,
                            pointsinlevel * level + *belementinfo.add((8 * k + 5 + i) as usize),
                        );
                        i += 1;
                    }
                } else {
                    i = j - 1;
                    while i > -1 {
                        fprintf(
                            *outfids.add(3),
                            b" %i\0".as_ptr() as *const c_char,
                            pointsinlevel * (level + 1) + *belementinfo.add((8 * k + 5 + i) as usize),
                        );
                        i -= 1;
                    }
                }
                l += 1;
            }
            fprintf(*outfids.add(3), b"\n\0".as_ptr() as *const c_char);
            level += 1;
        }
        k += 1;
    }
    for ii in 0..5usize {
        fclose(*infids.add(ii));
        fclose(*outfids.add(ii));
    }
    free(x as *mut c_void);
    free(y as *mut c_void);
    free(s as *mut c_void);
    free(b as *mut c_void);
    free(nodeinfo as *mut c_void);
    free(elementinfo as *mut c_void);
    free(belementinfo as *mut c_void);
    free(sharednodeinfo as *mut c_void);
    free(infids as *mut c_void);
    for ii in 0..5usize {
        free(*inputfile.add(ii) as *mut c_void);
    }
    free(inputfile as *mut c_void);
    free(outfids as *mut c_void);
    for ii in 0..5usize {
        free(*outputfile.add(ii) as *mut c_void);
    }
    free(outputfile as *mut c_void);
    free(instring as *mut c_void);
    free(numberstring as *mut c_void);
    EXIT_SUCCESS
}

// SAFETY: mirrors C `extrudeserial` 1:1 (mesh.{header,nodes,elements,
// boundary} with directory prefix). corrbed/GL/percentage/ratio/baseline
// thread through refinedHeightBL; the `fprintf(...)+lines*baseline`
// discarded addition is reproduced via let-underscore.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
// Dead counters kept verbatim (see extrudepartition): elements re-counts,
// sides-loop quads/lines, baseline sides ++, node-loop progress.
#[allow(unused_assignments)]
pub unsafe extern "C" fn extrudeserial(
    argv: *const *const c_char,
    inputfilename: *mut c_char,
    outputfilename: *mut c_char,
    levels: c_int,
    depth: c_double,
    pointsinlevel: c_int,
    elementsinlevel: c_int,
    _belementsinlevel: c_int,
    interpolationScheme: c_int,
    bed: *mut c_double,
    surf: *mut c_double,
    thick: *mut c_double,
    pointsInDEM: *mut c_int,
    cutOffValue: c_double,
    wexp: c_double,
    gl: c_int,
    percentage: c_double,
    ratio: c_double,
    baseline: c_int,
    corrbed: c_int,
    isnoval0: *mut c_int,
    isnoval1: *mut c_int,
    isnoval2: *mut c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    let mut k: c_int;
    let mut l: c_int = 0;
    let mut level: c_int;
    let mut dummyint: c_int = 0;
    let mut nodesinpartition: c_int = 0;
    let mut elementsinpartition: c_int = 0;
    let mut belementsinpartition: c_int = 0;
    let mut points: c_int;
    let mut quads: c_int;
    let mut lines: c_int;
    let mut triangles: c_int;
    
    
    let mut parent = [0 as c_int; 2];
    let mut elementtype: c_int = 0;
    // Deferred: every read follows an assignment (sides loop returns on
    // unknown types); C leaves these uninitialized.
    let mut belementtype: c_int;
    let mut bcoffset: c_int;
    let mut maxorigBCno: c_int;
    let instring = malloc(BUF_LEN) as *mut c_char;
    
    
    
    
    let mut z: c_double;
    
    
    
    
    
    
    
    if instring.is_null() {
        exit(1);
    }
    let infids: *mut *mut FILE = malloc(5 * core::mem::size_of::<*mut FILE>()) as *mut *mut FILE;
    let inputfile: *mut *mut c_char = malloc(5 * core::mem::size_of::<*mut c_char>()) as *mut *mut c_char;
    for ii in 0..5usize {
        *inputfile.add(ii) = malloc(MAXPATHLEN + 1) as *mut c_char;
    }
    for ii in 0..5usize {
        strcpy(*inputfile.add(ii), inputfilename);
    }
    strcat(*inputfile.add(0), b"/mesh.header\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(1), b"/mesh.nodes\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(2), b"/mesh.elements\0".as_ptr() as *const c_char);
    strcat(*inputfile.add(3), b"/mesh.boundary\0".as_ptr() as *const c_char);
    for ii in 0..5usize {
        *infids.add(ii) = fopen(*inputfile.add(ii), b"r\0".as_ptr() as *const c_char);
        if (*infids.add(ii)).is_null() {
            printf(
                b"(%s) Could not open file %s\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *inputfile.add(ii),
            );
            return EXIT_FAILURE;
        }
    }
    let outfids: *mut *mut FILE = malloc(4 * core::mem::size_of::<*mut FILE>()) as *mut *mut FILE;
    let outputfile: *mut *mut c_char = malloc(4 * core::mem::size_of::<*mut c_char>()) as *mut *mut c_char;
    for ii in 0..4usize {
        *outputfile.add(ii) = malloc(MAXPATHLEN + 1) as *mut c_char;
    }
    for ii in 0..4usize {
        strcpy(*outputfile.add(ii), outputfilename);
    }
    strcat(*outputfile.add(0), b"/mesh.header\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(1), b"/mesh.nodes\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(2), b"/mesh.elements\0".as_ptr() as *const c_char);
    strcat(*outputfile.add(3), b"/mesh.boundary\0".as_ptr() as *const c_char);
    for ii in 0..4usize {
        *outfids.add(ii) = fopen(*outputfile.add(ii), b"w\0".as_ptr() as *const c_char);
        if (*outfids.add(ii)).is_null() {
            printf(
                b"(%s) Could not open file %s\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *outputfile.add(ii),
            );
            return EXIT_FAILURE;
        }
    }
    for ii in 0..4usize {
        printf(b"<-%s\n\0".as_ptr() as *const c_char, *inputfile.add(ii));
        printf(b"->%s\n\0".as_ptr() as *const c_char, *outputfile.add(ii));
    }
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut nodesinpartition,
    );
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut elementsinpartition,
    );
    fscanf(
        *infids.add(0),
        b"%i \0".as_ptr() as *const c_char,
        &mut belementsinpartition,
    );
    fscanf(*infids.add(0), b"%i \0".as_ptr() as *const c_char, &mut j);
    points = 0;
    lines = 0;
    triangles = 0;
    quads = 0;
    i = 0;
    while i < j {
        fscanf(
            *infids.add(0),
            b"%i %i\0".as_ptr() as *const c_char,
            &mut elementtype,
            &mut dummyint,
        );
        if elementtype == 101 {
            points = dummyint;
        } else if elementtype == 202 {
            lines = dummyint;
        } else if elementtype == 303 {
            triangles = dummyint;
        } else if elementtype == 404 {
            quads = dummyint;
        } else {
            printf(
                b"(%s) element type %3i of input file %s not defined\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                elementtype,
                *inputfile.add(0),
            );
            return EXIT_FAILURE;
        }
        i += 1;
    }
    printf(
        b"(%s) Serial mesh input: %i nodes, %i elements, %i boundary elements\n\0".as_ptr()
            as *const c_char,
        *argv.add(0),
        nodesinpartition,
        elementsinpartition,
        belementsinpartition,
    );
    let wedges: c_int = triangles * (levels - 1);
    let bricks: c_int = quads * (levels - 1);
    triangles *= 2;
    quads *= 2;
    quads += (levels - 1) * lines;
    lines = (levels - 1) * points + baseline * lines;
    fprintf(
        *outfids.add(0),
        b"%i %i %i\n\0".as_ptr() as *const c_char,
        nodesinpartition * levels,
        wedges + bricks,
        quads + triangles + lines,
    );
    fprintf(
        *outfids.add(0),
        b"%i\n\0".as_ptr() as *const c_char,
        (if quads > 0 { 1 } else { 0 })
            + (if triangles > 0 { 1 } else { 0 })
            + (if bricks > 0 { 1 } else { 0 })
            + (if wedges > 0 { 1 } else { 0 })
            + (if lines > 0 { 1 } else { 0 }),
    );
    if lines > 0 {
        fprintf(*outfids.add(0), b"202 %i\n\0".as_ptr() as *const c_char, lines);
    }
    if triangles > 0 {
        fprintf(*outfids.add(0), b"303 %i\n\0".as_ptr() as *const c_char, triangles);
    }
    if quads > 0 {
        fprintf(*outfids.add(0), b"404 %i\n\0".as_ptr() as *const c_char, quads);
    }
    if wedges > 0 {
        fprintf(*outfids.add(0), b"706 %i\n\0".as_ptr() as *const c_char, wedges);
    }
    if bricks > 0 {
        fprintf(*outfids.add(0), b"808 %i\n\0".as_ptr() as *const c_char, bricks);
    }
    printf(
        b"(%s) Serial mesh output: %i nodes, %i elements, %i boundary elements\n\n\0".as_ptr()
            as *const c_char,
        *argv.add(0),
        nodesinpartition * levels,
        wedges + bricks,
        quads + triangles + lines,
    );
    let x: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let y: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let s: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let b: *mut c_double = malloc((nodesinpartition as usize) * 8) as *mut c_double;
    let nodeinfo: *mut c_int = malloc((nodesinpartition as usize) * 4) as *mut c_int;
    let elementinfo: *mut c_int = malloc((elementsinpartition as usize) * 7 * 4) as *mut c_int;
    let belementinfo: *mut c_int = malloc((belementsinpartition as usize) * 7 * 4) as *mut c_int;
    i = 0;
    while i < nodesinpartition {
        fscanf(
            *infids.add(1),
            b"%i %i %le %le %le\0".as_ptr() as *const c_char,
            nodeinfo.add(i as usize),
            &mut dummyint,
            x.add(i as usize),
            y.add(i as usize),
            b.add(i as usize),
        );
        if interpolationScheme > 0 {
            if interpolatePoint(
                *x.add(i as usize),
                *y.add(i as usize),
                b.add(i as usize),
                s.add(i as usize),
                bed,
                surf,
                thick,
                pointsInDEM,
                cutOffValue,
                wexp,
                interpolationScheme,
                argv,
                isnoval0,
                isnoval1,
                isnoval2,
            ) == EXIT_FAILURE
            {
                fprintf(
                    c_stderr(),
                    b"(%s) Failed interpolating values for point %i %e %e\n\0".as_ptr()
                        as *const c_char,
                    *argv.add(0),
                    i,
                    *x.add(i as usize),
                    *y.add(i as usize),
                );
                return EXIT_FAILURE;
            } else {
                if *s.add(i as usize) - *b.add(i as usize) < depth {
                    printf(
                        b"corrected  S[%i] %e %e %e\n\0".as_ptr() as *const c_char,
                        i,
                        *x.add(i as usize),
                        *y.add(i as usize),
                        *s.add(i as usize),
                    );
                    if corrbed == 1 {
                        *b.add(i as usize) = *s.add(i as usize) - depth;
                    } else {
                        *s.add(i as usize) = *b.add(i as usize) + depth;
                    }
                    printf(
                        b" ->  %e %e %e\n\0".as_ptr() as *const c_char,
                        *x.add(i as usize),
                        *y.add(i as usize),
                        *s.add(i as usize),
                    );
                }
            }
        } else {
            *s.add(i as usize) = *b.add(i as usize) + depth;
        }
        i += 1;
    }
    quads = 0;
    triangles = 0;
    k = 0;
    while k < elementsinpartition {
        fgets(instring, (BUF_LEN - 1) as c_int, *infids.add(2));
        sscanf(
            instring as *const c_char,
            b"%i %i %i %i %i %i %i\0".as_ptr() as *const c_char,
            elementinfo.add((7 * k) as usize),
            elementinfo.add((7 * k + 1) as usize),
            elementinfo.add((7 * k + 2) as usize),
            elementinfo.add((7 * k + 3) as usize),
            elementinfo.add((7 * k + 4) as usize),
            elementinfo.add((7 * k + 5) as usize),
            elementinfo.add((7 * k + 6) as usize),
        );
        if *elementinfo.add((7 * k + 2) as usize) == 404 {
            quads += 1;
        } else if *elementinfo.add((7 * k + 2) as usize) == 303 {
            *elementinfo.add((7 * k + 6) as usize) = -1;
            triangles += 1;
        } else {
            printf(
                b"(%s): element type %i not recognised for entry %i\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                *elementinfo.add((7 * k + 2) as usize),
                k,
            );
        }
        k += 1;
    }
    maxorigBCno = 0;
    k = 0;
    while k < belementsinpartition {
        fgets(instring, (BUF_LEN - 1) as c_int, *infids.add(3));
        sscanf(
            instring as *const c_char,
            b"%i %i %i %i %i %i %i\0".as_ptr() as *const c_char,
            belementinfo.add((7 * k) as usize),
            belementinfo.add((7 * k + 1) as usize),
            belementinfo.add((7 * k + 2) as usize),
            belementinfo.add((7 * k + 3) as usize),
            belementinfo.add((7 * k + 4) as usize),
            belementinfo.add((7 * k + 5) as usize),
            belementinfo.add((7 * k + 6) as usize),
        );
        // NOTE: C's j=2/j=1 stores are dead here too (node loop re-inits
        // j); omitted. belementinfo[6]=-1 is live and kept.
        if *belementinfo.add((7 * k + 4) as usize) == 101 {
            *belementinfo.add((7 * k + 6) as usize) = -1;
        } else if *belementinfo.add((7 * k + 4) as usize) != 202 {
            printf(
                b"(%s): boundary element type %i not recognised for entry %i\n\0".as_ptr()
                    as *const c_char,
                *argv.add(0),
                *belementinfo.add((7 * k + 4) as usize),
                k,
            );
        }
        if *belementinfo.add((7 * k + 1) as usize) > maxorigBCno {
            maxorigBCno = *belementinfo.add((7 * k + 1) as usize);
        }
        k += 1;
    }
    j = 0;
    level = 0;
    while level < levels {
        k = 0;
        while k < nodesinpartition {
            l = 100 * k / pointsinlevel;
            z = refinedHeightBL(
                *s.add(k as usize),
                *b.add(k as usize),
                levels,
                level,
                gl,
                percentage,
                ratio,
            );
            fprintf(
                *outfids.add(1),
                b"%i -1 %e %e %e\n\0".as_ptr() as *const c_char,
                pointsinlevel * level + *nodeinfo.add(k as usize),
                *x.add(k as usize),
                *y.add(k as usize),
                z,
            );
            k += 1;
        }
        level += 1;
    }
    bcoffset = 1;
    k = 0;
    while k < elementsinpartition {
        if *elementinfo.add((7 * k + 2) as usize) == 303 {
            elementtype = 706;
            l = 3;
        } else if *elementinfo.add((7 * k + 2) as usize) == 404 {
            elementtype = 808;
            l = 4;
        }
        level = 0;
        while level < levels - 1 {
            fprintf(
                *outfids.add(2),
                b"%i\0".as_ptr() as *const c_char,
                elementsinlevel * level + *elementinfo.add((7 * k) as usize),
            );
            fprintf(
                *outfids.add(2),
                b" %i %i\0".as_ptr() as *const c_char,
                *elementinfo.add((7 * k + 1) as usize),
                elementtype,
            );
            // C ternary: bcoffset = (bcoffset > body) ? bcoffset : body.
            if bcoffset <= *elementinfo.add((7 * k + 1) as usize) {
                bcoffset = *elementinfo.add((7 * k + 1) as usize);
            }
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(2),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * level + *elementinfo.add((7 * k + 3 + j) as usize),
                );
                j += 1;
            }
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(2),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * (level + 1) + *elementinfo.add((7 * k + 3 + j) as usize),
                );
                j += 1;
            }
            fprintf(*outfids.add(2), b"\n\0".as_ptr() as *const c_char);
            level += 1;
        }
        k += 1;
    }
    printf(b"bodies: %i\n\0".as_ptr() as *const c_char, bcoffset);
    parent[1] = 0;
    i = 0;
    lines = 0;
    while i < 2 {
        k = 0;
        while k < elementsinpartition {
            parent[0] = elementsinlevel * (levels - 2) * i + *elementinfo.add((7 * k) as usize);
            fprintf(
                *outfids.add(3),
                b"%i\0".as_ptr() as *const c_char,
                i * elementsinpartition + k + 1,
            );
            fprintf(
                *outfids.add(3),
                b" %i %i %i %3i\0".as_ptr() as *const c_char,
                *elementinfo.add((7 * k + 1) as usize) + i * bcoffset,
                parent[0],
                parent[1],
                *elementinfo.add((7 * k + 2) as usize),
            );
            if *elementinfo.add((7 * k + 2) as usize) == 303 {
                l = 3;
            } else if *elementinfo.add((7 * k + 2) as usize) == 404 {
                l = 4;
            }
            j = 0;
            while j < l {
                fprintf(
                    *outfids.add(3),
                    b" %i\0".as_ptr() as *const c_char,
                    pointsinlevel * (levels - 1) * i + *elementinfo.add((7 * k + 3 + j) as usize),
                );
                j += 1;
            }
            fprintf(*outfids.add(3), b"\n\0".as_ptr() as *const c_char);
            k += 1;
        }
        i += 1;
    }
    k = 0;
    while k < belementsinpartition {
        level = 0;
        while level < levels - 1 {
            i = 0;
            while i < 2 {
                parent[i as usize] = if *belementinfo.add((7 * k + 2 + i) as usize) > 0 {
                    level * elementsinlevel + *belementinfo.add((7 * k + 2 + i) as usize)
                } else {
                    0
                };
                i += 1;
            }
            if *belementinfo.add((7 * k + 4) as usize) == 202 {
                belementtype = 404;
                quads += 1;
                j = 2;
            } else if *belementinfo.add((7 * k + 4) as usize) == 101 {
                belementtype = 202;
                lines += 1;
                j = 1;
            } else {
                printf(
                    b"(%s) boundary element type %3i of input file %s (line %i) not defined\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    *belementinfo.add((7 * k + 4) as usize),
                    *inputfile.add(3),
                    k,
                );
                return EXIT_FAILURE;
            }
            fprintf(
                *outfids.add(3),
                b"%i\0".as_ptr() as *const c_char,
                2 * elementsinpartition + belementsinpartition * level + *belementinfo.add((7 * k) as usize),
            );
            fprintf(
                *outfids.add(3),
                b" %i %i %i %3i\0".as_ptr() as *const c_char,
                *belementinfo.add((7 * k + 1) as usize) + 2 * bcoffset,
                parent[0],
                parent[1],
                belementtype,
            );
            l = 0;
            while l < 2 {
                if l == 0 {
                    i = 0;
                    while i < j {
                        fprintf(
                            *outfids.add(3),
                            b" %i\0".as_ptr() as *const c_char,
                            pointsinlevel * level + *belementinfo.add((7 * k + 5 + i) as usize),
                        );
                        i += 1;
                    }
                } else {
                    i = j - 1;
                    while i > -1 {
                        fprintf(
                            *outfids.add(3),
                            b" %i\0".as_ptr() as *const c_char,
                            pointsinlevel * (level + 1) + *belementinfo.add((7 * k + 5 + i) as usize),
                        );
                        i -= 1;
                    }
                }
                l += 1;
            }
            fprintf(*outfids.add(3), b"\n\0".as_ptr() as *const c_char);
            level += 1;
        }
        k += 1;
    }
    if baseline != 0 {
        k = 0;
        while k < belementsinpartition {
            parent[0] = 0;
            parent[1] = 0;
            belementtype = *belementinfo.add((7 * k + 4) as usize);
            if belementtype == 202 {
                lines += 1;
                j = 2;
            } else if belementtype == 101 {
                points += 1;
                j = 2;
            }
            fprintf(
                *outfids.add(3),
                b"%i\0".as_ptr() as *const c_char,
                2 * elementsinpartition + belementsinpartition * (levels - 1)
                    + *belementinfo.add((7 * k) as usize),
            );
            fprintf(
                *outfids.add(3),
                b" %i %i %i %3i\0".as_ptr() as *const c_char,
                *belementinfo.add((7 * k + 1) as usize) + 2 * bcoffset + maxorigBCno,
                parent[0],
                parent[1],
                belementtype,
            );
            i = 0;
            while i < j {
                fprintf(
                    *outfids.add(3),
                    b" %i\0".as_ptr() as *const c_char,
                    *belementinfo.add((7 * k + 5 + i) as usize),
                );
                i += 1;
            }
            fprintf(*outfids.add(3), b"\n\0".as_ptr() as *const c_char);
            k += 1;
        }
    }
    for ii in 0..4usize {
        fclose(*infids.add(ii));
        fclose(*outfids.add(ii));
    }
    free(x as *mut c_void);
    free(y as *mut c_void);
    free(s as *mut c_void);
    free(b as *mut c_void);
    free(nodeinfo as *mut c_void);
    free(elementinfo as *mut c_void);
    free(belementinfo as *mut c_void);
    free(infids as *mut c_void);
    for ii in 0..5usize {
        free(*inputfile.add(ii) as *mut c_void);
    }
    free(inputfile as *mut c_void);
    free(outfids as *mut c_void);
    for ii in 0..4usize {
        free(*outputfile.add(ii) as *mut c_void);
    }
    free(outputfile as *mut c_void);
    free(instring as *mut c_void);
    EXIT_SUCCESS
}

// SAFETY: mirrors C `main` 1:1 as `extrudemesh_main` (argc/argv handling,
// DEM loading with the bed/surf/thick scheme logic, info.dat + global
// header writes, serial/partition dispatch). corrbed stays 0: the original
// never assigns it from argv[14] (dead option, reported verbatim).
// cutOffValue/wexp/noval print 0.0 when no DEM is given (the original
// prints uninitialized stack garbage there — UB, corpus-masked).
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn extrudemesh_main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut i: c_int;
    let mut k: c_int = 0;
    let mut l: c_int = 0;
    
    let mut partitions: c_int;
    let mut partition: c_int;
    let mut pointsinlevel: c_int = 0;
    let mut elementsinlevel: c_int = 0;
    let mut belementsinlevel: c_int = 0;
    // Initialized: the partition loop may run zero times as far as the
    // compiler sees (at runtime partitions>1 guarantees trips + the serial
    // arm assigns); C leaves it uninitialized there.
    let mut stat: c_int = 0;
    let mut interpolateDEM: c_int;
    let mut interpolationScheme: c_int;
    let mut noelementtypes: c_int = 0;
    let mut triangles: c_int;
    let mut quads: c_int;
    let mut lines: c_int;
    let mut points: c_int;
    let mut pointsInDEM = [0 as c_int; 3];
    let mut dummyint: c_int;
    
    let mut baseline: c_int;
    let mut isnoval0: *mut c_int = core::ptr::null_mut();
    let mut isnoval1: *mut c_int = core::ptr::null_mut();
    let mut isnoval2: *mut c_int = core::ptr::null_mut();
    let corrbed: c_int = 0;
    let mut inputdirectoryname = [0 as c_char; MAXPATHLEN + 1];
    let mut outputdirectoryname = [0 as c_char; MAXPATHLEN + 1];
    let mut cpartition = [0 as c_char; 11];
    let mut inputfilename = [0 as c_char; MAXPATHLEN + 1];
    let mut outputfilename = [0 as c_char; MAXPATHLEN + 1];
    let mut filename = [0 as c_char; MAXPATHLEN + 1];
    
    let mut cutOffValue: c_double = 0.0;
    let mut wexp: c_double = 0.0;
    let mut noval: c_double = 0.0;
    
    
    let mut bed: *mut c_double = core::ptr::null_mut();
    let mut surf: *mut c_double = core::ptr::null_mut();
    let mut thick: *mut c_double = core::ptr::null_mut();
    
    let mut headerfid: *mut FILE;
    if argc != 10 && argc != 14 && argc != 15 {
        printf(b"%s usage:\x07\n\n\0".as_ptr() as *const c_char, *argv.add(0));
        printf(
            b"%s inputdir outputdir levels extrudedepth N baseline GL percentage ratio DEM cutoff wexp noval\n\n\0".as_ptr() as *const c_char,
            *argv.add(0),
        );
        printf(b"     inputdir ... directory containing the 2D footrpint to be extruded\n\0".as_ptr() as *const c_char);
        printf(b"    outputdir ... directory (will be created if not existing) containing\n\0".as_ptr() as *const c_char);
        printf(b"                  the extruded mesh\n\0".as_ptr() as *const c_char);
        printf(b"                  WARNING: Files in outputdir will be overwritten!\n\0".as_ptr() as *const c_char);
        printf(b"       levels ... levels in direction of extrushion (>2)\n\0".as_ptr() as *const c_char);
        printf(b" extrudedepth ... depth in direction of extrushion in unit-length of\n\0".as_ptr() as *const c_char);
        printf(b"                  input mesh\n\0".as_ptr() as *const c_char);
        printf(b"           N  ... partitions\n\0".as_ptr() as *const c_char);
        printf(b"                  inputdir should contain an already partitioned mesh)\n\0".as_ptr() as *const c_char);
        printf(b"                  give N=1 for serial mesh\n\0".as_ptr() as *const c_char);
        printf(b"   baseline  ...  1/0\n\0".as_ptr() as *const c_char);
        printf(b"                  1 if the existing side boundary for footprint (point/line in 2d/3d)\n\0".as_ptr() as *const c_char);
        printf(b"                  shall be included, else excluded\n\n\0".as_ptr() as *const c_char);
        printf(b"         GL  ... number of layers inside boundary layer\n\0".as_ptr() as *const c_char);
        printf(b"                 has to be smaller than levels\n\0".as_ptr() as *const c_char);
        printf(b"                 give 0, if no boundary layer wanted\n\0".as_ptr() as *const c_char);
        printf(b"  percentage ... percentage height of boundary layer\n\0".as_ptr() as *const c_char);
        printf(b"                 with respect to the local height (as 0..1) \n\0".as_ptr() as *const c_char);
        printf(b"                 if positive BL at bottom, if negative at the surface \n\0".as_ptr() as *const c_char);
        printf(b"       ratio ... ratio between two adjacent layers in boundary layer(>1) \n\0".as_ptr() as *const c_char);
        printf(b"                  has to be >1, if it's 0 automatic calculation, but does not work always \n\0".as_ptr() as *const c_char);
        printf(b"         DEM  ... (optional) directory of digital elevation model\n\0".as_ptr() as *const c_char);
        printf(b"                  for interpolation of bed/surface (see below)\n\0".as_ptr() as *const c_char);
        printf(b"      cutoff  ... (optional, but mandatory if previous was given)\n\0".as_ptr() as *const c_char);
        printf(b"                  cutoff radius for interpolation\n\0".as_ptr() as *const c_char);
        printf(b"      wexp  ... (optional, but mandatory if previous was given)\n\0".as_ptr() as *const c_char);
        printf(b"                  exponent of weight for interpolation w=1/r^wexp\n\0".as_ptr() as *const c_char);
        printf(b"      noval  ... (optional, but mandatory if previous was given)\n\0".as_ptr() as *const c_char);
        printf(b"                  value indicating void/invalid entry in DEM (e.g. -999.99)\n\0".as_ptr() as *const c_char);
        printf(b"     corrbed ... (optional) value: 1 or any other number\n\0".as_ptr() as *const c_char);
        printf(b"                 corrections induced by minimum flowdpeth are by default applied\n\0".as_ptr() as *const c_char);
        printf(b"                 correctiong the side of the free surface, only of value 1 is defined\n\0".as_ptr() as *const c_char);
        printf(b"                 the surface is kept constant and the bedrock is adjusted\n\0".as_ptr() as *const c_char);
        printf(b"outputdir contains:\n\0".as_ptr() as *const c_char);
        printf(b" mesh.{nodes,elements,boundary} ... mesh files (if N==1)\n\0".as_ptr() as *const c_char);
        printf(b" partitioning.N/mesh.{nodes,elements,boundary} ... mesh files (if N>1)\n\0".as_ptr() as *const c_char);
        printf(b" info.dat ... file containing information on the extruded mesh\n\0".as_ptr() as *const c_char);
        printf(b"DEM (if chosen) should provide (two of these three):\n\0".as_ptr() as *const c_char);
        printf(b" DEM/bed.xyz  (hard-coded name!)\n\0".as_ptr() as *const c_char);
        printf(b" DEM/surf.xyz (hard-coded name!)\n\0".as_ptr() as *const c_char);
        printf(b" DEM/thick.xyz (hard-coded name!)\n\0".as_ptr() as *const c_char);
        printf(b" mind, that those filenames are hardcoded!\n\0".as_ptr() as *const c_char);
        printf(b" the xyz-files can contain a set of irregullary distributed (x,y,z)\n\0".as_ptr() as *const c_char);
        printf(b" coordinates, between those the mesh will be interpolated.\n\0".as_ptr() as *const c_char);
        printf(b" In case of a DEM being provided, the earlier given\n\0".as_ptr() as *const c_char);
        printf(b" parameter \"extrudedepth\" is taken as the minimum flow depth\n\n\0".as_ptr() as *const c_char);
        printf(b" Boundary layer implemented only in serial\n\n\0".as_ptr() as *const c_char);
        return EXIT_FAILURE;
    }
    let levels: c_int = atoi(*argv.add(3));
    let depth: c_double = atof(*argv.add(4));
    partitions = atoi(*argv.add(5));
    baseline = atoi(*argv.add(6));
    if baseline != 1 {
        baseline = 0;
    }
    let gl: c_int = atoi(*argv.add(7));
    if gl >= levels {
        fprintf(
            c_stderr(),
            b"GL = %i >= levels = %i !\n\0".as_ptr() as *const c_char,
            gl,
            levels,
        );
        return EXIT_FAILURE;
    }
    let percentage: c_double = atof(*argv.add(8));
    if percentage < -0.99 || percentage > 0.99 {
        fprintf(
            c_stderr(),
            b"0.01 < percentage = %f10.4 < 0.99 !\n\0".as_ptr() as *const c_char,
            percentage,
        );
        return EXIT_FAILURE;
    }
    let ratio: c_double = atof(*argv.add(9));
    if ratio < 1.0 && ratio != 0.0 {
        fprintf(
            c_stderr(),
            b"ratio < 1.0 = %f10.4 < 1.0 or equal 0!\n\0".as_ptr() as *const c_char,
            ratio,
        );
        return EXIT_FAILURE;
    }
    printf(b"(%s) executing with following inputs ...\n\0".as_ptr() as *const c_char, *argv.add(0));
    printf(b"    ... input directory: %s \n\0".as_ptr() as *const c_char, *argv.add(1));
    printf(b"    ... output directory: %s\n\0".as_ptr() as *const c_char, *argv.add(2));
    printf(b"    ... levels: %i\n\0".as_ptr() as *const c_char, levels);
    printf(b"    ... depth: %10.4f\n\0".as_ptr() as *const c_char, depth);
    printf(b"    ... partitions: %i\n\0".as_ptr() as *const c_char, partitions);
    printf(b"    ... include baseline(1=yes,0=no): %i\n\0".as_ptr() as *const c_char, baseline);
    printf(b"    ... GL: %i\n\0".as_ptr() as *const c_char, gl);
    printf(b"    ... percentage: %10.4f\n\0".as_ptr() as *const c_char, percentage);
    printf(b"    ... ratio: %10.4f\n\0".as_ptr() as *const c_char, ratio);
    if partitions < 1 {
        printf(
            b"(%s) Wrong number of partitions %i\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            partitions,
        );
        printf(b"(%s) Assuming serial mesh (N=1)\n\0".as_ptr() as *const c_char, *argv.add(0));
        partitions = 1;
    }
    interpolateDEM = 0;
    if argc == 14 || argc == 15 {
        i = 0;
        while i < argc {
            printf(b"argv[%i] = %s\n\0".as_ptr() as *const c_char, i, *argv.add(i as usize));
            i += 1;
        }
        cutOffValue = atof(*argv.add(11));
        wexp = atof(*argv.add(12));
        noval = atof(*argv.add(13));
        printf(b"    ... DEM directory: %s\n\0".as_ptr() as *const c_char, *argv.add(10));
        printf(b"    ... cutoff radius: %e\n\0".as_ptr() as *const c_char, cutOffValue);
        printf(b"    ... exponent m of weight (1/r^m): %e\n\0".as_ptr() as *const c_char, wexp);
        printf(b"    ... noval: %10.4f\n\0".as_ptr() as *const c_char, noval);
        if corrbed == 1 {
            printf(b"    ... depth corrections of bedrock\n\0".as_ptr() as *const c_char);
        } else {
            printf(b"    ... depth corrections of surface (default)\n\0".as_ptr() as *const c_char);
        }
        let mut entriesPerLine: c_int = 0;
        // Upstream prints argv[7] (GL), not the DEM dir — cosmetic quirk.
        printf(
            b"(%s) Checking for input files of DEM in directory %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            *argv.add(7),
        );
        strcpy(filename.as_mut_ptr(), *argv.add(10));
        strcat(filename.as_mut_ptr(), b"/bed.xyz\0".as_ptr() as *const c_char);
        if file_exists(filename.as_mut_ptr()) != 0 {
            pointsInDEM[0] = checkFileEntries(filename.as_mut_ptr(), argv, &mut entriesPerLine);
            if pointsInDEM[0] < 1 || entriesPerLine < 3 {
                fprintf(
                    c_stderr(),
                    b"(%s) Wrong file entries (%i,%i) found in file %s\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    pointsInDEM[0],
                    entriesPerLine,
                    filename.as_mut_ptr(),
                );
                return EXIT_FAILURE;
            } else {
                printf(
                    b"(%s) %i file entries found in file %s\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    pointsInDEM[0],
                    filename.as_mut_ptr(),
                );
                bed = malloc((pointsInDEM[0] as usize) * 3 * 8) as *mut c_double;
                isnoval0 = malloc((pointsInDEM[0] as usize) * 4) as *mut c_int;
                dummyint = readDEM(bed, isnoval0, pointsInDEM[0], filename.as_mut_ptr(), argv);
                if dummyint > 1 {
                    printf(
                        b"(%s) %i out of %i valid points in DEM %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        dummyint,
                        pointsInDEM[0],
                        filename.as_mut_ptr(),
                    );
                    pointsInDEM[0] = dummyint;
                } else {
                    fprintf(
                        c_stderr(),
                        b"(%s) Error reading file %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        filename.as_mut_ptr(),
                    );
                    return EXIT_FAILURE;
                }
                interpolateDEM = 1;
                interpolationScheme = 1;
            }
        } else {
            interpolationScheme = 0;
            printf(b"(%s) no file %s\n\0".as_ptr() as *const c_char, *argv.add(0), filename.as_mut_ptr());
        }
        strcpy(filename.as_mut_ptr(), *argv.add(10));
        strcat(filename.as_mut_ptr(), b"/surf.xyz\0".as_ptr() as *const c_char);
        if file_exists(filename.as_mut_ptr()) != 0 {
            pointsInDEM[1] = checkFileEntries(filename.as_mut_ptr(), argv, &mut entriesPerLine);
            if pointsInDEM[1] < 1 || entriesPerLine < 3 {
                fprintf(
                    c_stderr(),
                    b"(%s) Wrong file entries (%i,%i) found in file %s\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    pointsInDEM[1],
                    entriesPerLine,
                    filename.as_mut_ptr(),
                );
                return EXIT_FAILURE;
            } else {
                printf(
                    b"(%s) %i file entries found in file %s\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    pointsInDEM[1],
                    filename.as_mut_ptr(),
                );
                surf = malloc((pointsInDEM[1] as usize) * 3 * 8) as *mut c_double;
                i = 0;
                while i < pointsInDEM[1] {
                    *surf.add((3 * i) as usize) = 0.0;
                    *surf.add((3 * i + 1) as usize) = 0.0;
                    *surf.add((3 * i + 2) as usize) = 0.0;
                    i += 1;
                }
                isnoval1 = malloc((pointsInDEM[1] as usize) * 4) as *mut c_int;
                dummyint = readDEM(surf, isnoval1, pointsInDEM[1], filename.as_mut_ptr(), argv);
                if dummyint > 1 {
                    printf(
                        b"(%s) %i out of %i valid points in DEM %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        dummyint,
                        pointsInDEM[1],
                        filename.as_mut_ptr(),
                    );
                    pointsInDEM[1] = dummyint;
                } else {
                    fprintf(
                        c_stderr(),
                        b"(%s) Error reading file %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        filename.as_mut_ptr(),
                    );
                    return EXIT_FAILURE;
                }
            }
            interpolateDEM = 1;
            interpolationScheme += 10;
        } else {
            printf(b"(%s) no file %s\n\0".as_ptr() as *const c_char, *argv.add(0), filename.as_mut_ptr());
            if interpolationScheme == 0 {
                fprintf(
                    c_stderr(),
                    b"(%s) We either need a surface or a bedrock\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                );
                return EXIT_FAILURE;
            }
        }
        if interpolationScheme < 11 {
            strcpy(filename.as_mut_ptr(), *argv.add(10));
            strcat(filename.as_mut_ptr(), b"/thick.xyz\0".as_ptr() as *const c_char);
            if file_exists(filename.as_mut_ptr()) != 0 {
                pointsInDEM[2] = checkFileEntries(filename.as_mut_ptr(), argv, &mut entriesPerLine);
                if pointsInDEM[2] < 1 || entriesPerLine < 3 {
                    fprintf(
                        c_stderr(),
                        b"(%s) Wrong file entries (%i,%i) found in file %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        pointsInDEM[2],
                        entriesPerLine,
                        filename.as_mut_ptr(),
                    );
                    return EXIT_FAILURE;
                } else {
                    printf(
                        b"(%s) %i file entries found in file %s\n\0".as_ptr() as *const c_char,
                        *argv.add(0),
                        pointsInDEM[2],
                        filename.as_mut_ptr(),
                    );
                    thick = malloc((pointsInDEM[2] as usize) * 3 * 8) as *mut c_double;
                    i = 0;
                    while i < pointsInDEM[2] {
                        *thick.add((3 * i) as usize) = 0.0;
                        *thick.add((3 * i + 1) as usize) = 0.0;
                        *thick.add((3 * i + 2) as usize) = 0.0;
                        i += 1;
                    }
                    isnoval2 = malloc((pointsInDEM[0] as usize) * 4) as *mut c_int;
                    dummyint = readDEM(thick, isnoval2, pointsInDEM[2], filename.as_mut_ptr(), argv);
                    if dummyint > 1 {
                        printf(
                            b"(%s) %i out of %i valid points in DEM %s\n\0".as_ptr() as *const c_char,
                            *argv.add(0),
                            dummyint,
                            pointsInDEM[2],
                            filename.as_mut_ptr(),
                        );
                        pointsInDEM[2] = dummyint;
                    } else {
                        fprintf(
                            c_stderr(),
                            b"(%s) Error reading file %s\n\0".as_ptr() as *const c_char,
                            *argv.add(0),
                            filename.as_mut_ptr(),
                        );
                        return EXIT_FAILURE;
                    }
                }
                interpolateDEM = 1;
                interpolationScheme += 100;
            } else {
                fprintf(
                    c_stderr(),
                    b"(%s) no file %s\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                    filename.as_mut_ptr(),
                );
                fprintf(
                    c_stderr(),
                    b"(%s) We either need a thickness file\n\0".as_ptr() as *const c_char,
                    *argv.add(0),
                );
                return EXIT_FAILURE;
            }
        }
    } else {
        interpolationScheme = 0;
    }
    strcpy(inputdirectoryname.as_mut_ptr(), *argv.add(1));
    strcpy(filename.as_mut_ptr(), inputdirectoryname.as_mut_ptr());
    strcat(filename.as_mut_ptr(), b"/mesh.header\0".as_ptr() as *const c_char);
    headerfid = fopen(filename.as_mut_ptr(), b"r\0".as_ptr() as *const c_char);
    if headerfid.is_null() {
        printf(
            b"(%s) Error opening file %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            filename.as_mut_ptr(),
        );
        return EXIT_FAILURE;
    }
    printf(
        b"(%s) reading global header file %s\n\0".as_ptr() as *const c_char,
        *argv.add(0),
        filename.as_mut_ptr(),
    );
    fscanf(
        headerfid,
        b"%i %i %i\0".as_ptr() as *const c_char,
        &mut pointsinlevel,
        &mut elementsinlevel,
        &mut belementsinlevel,
    );
    fscanf(headerfid, b"%i\0".as_ptr() as *const c_char, &mut noelementtypes);
    points = 0;
    lines = 0;
    triangles = 0;
    quads = 0;
    i = 0;
    while i < noelementtypes {
        fscanf(headerfid, b"%i %i\0".as_ptr() as *const c_char, &mut l, &mut k);
        if l == 101 {
            points = k;
        } else if l == 202 {
            lines = k;
        } else if l == 303 {
            triangles = k;
        } else if l == 404 {
            quads = k;
        } else {
            printf(
                b"(%s) element type %i in header file %s not recognised\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                l,
                filename.as_mut_ptr(),
            );
            return EXIT_FAILURE;
        }
        i += 1;
    }
    if partitions > 1 {
        if partitions < 10 {
            sprintf(cpartition.as_mut_ptr(), b"%1i\0".as_ptr() as *const c_char, partitions);
        } else if partitions < 100 {
            sprintf(cpartition.as_mut_ptr(), b"%2i\0".as_ptr() as *const c_char, partitions);
        } else if partitions < 1000 {
            sprintf(cpartition.as_mut_ptr(), b"%3i\0".as_ptr() as *const c_char, partitions);
        } else if partitions < 10000 {
            sprintf(cpartition.as_mut_ptr(), b"%4i\0".as_ptr() as *const c_char, partitions);
        } else {
            printf(
                b"(%s) More than 9999 partitions? Get real or (if you insist) change the source code of this executable!\n\0".as_ptr() as *const c_char,
                *argv.add(0),
            );
            return EXIT_FAILURE;
        }
        strcat(inputdirectoryname.as_mut_ptr(), b"/partitioning.\0".as_ptr() as *const c_char);
        strcat(inputdirectoryname.as_mut_ptr(), cpartition.as_mut_ptr());
        printf(b"(%s) Reading from directory: %s\n\0".as_ptr() as *const c_char, *argv.add(0), inputdirectoryname.as_mut_ptr());
        strcpy(outputdirectoryname.as_mut_ptr(), *argv.add(2));
        printf(
            b"(%s) creating output file directory %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            outputdirectoryname.as_mut_ptr(),
        );
        mkdir(outputdirectoryname.as_mut_ptr(), 0o700);
        strcat(outputdirectoryname.as_mut_ptr(), b"/partitioning.\0".as_ptr() as *const c_char);
        strcat(outputdirectoryname.as_mut_ptr(), cpartition.as_mut_ptr());
        printf(
            b"(%s) creating output file partition directory %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            outputdirectoryname.as_mut_ptr(),
        );
        mkdir(outputdirectoryname.as_mut_ptr(), 0o700);
    } else {
        printf(b"(%s) Reading from directory: %s\n\0".as_ptr() as *const c_char, *argv.add(0), inputdirectoryname.as_mut_ptr());
        strcpy(outputdirectoryname.as_mut_ptr(), *argv.add(2));
        printf(
            b"(%s) creating output file directory %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            outputdirectoryname.as_mut_ptr(),
        );
        mkdir(outputdirectoryname.as_mut_ptr(), 0o700);
    }
    strcpy(filename.as_mut_ptr(), outputdirectoryname.as_mut_ptr());
    strcat(filename.as_mut_ptr(), b"/info.dat\0".as_ptr() as *const c_char);
    let infofid: *mut FILE = fopen(filename.as_mut_ptr(), b"w\0".as_ptr() as *const c_char);
    if infofid.is_null() {
        printf(
            b"(%s) Error opening file %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            filename.as_mut_ptr(),
        );
        return EXIT_FAILURE;
    }
    // NOTE: with argc==10, argv[10] is NULL (glibc prints "(null)") and
    // cutOffValue/wexp/noval are uninitialized stack garbage in the
    // original. The port prints 0.0 for the trio (documented UB divergence,
    // corpus-masked); argv[10] flows through libc identically.
    let argv10: *const c_char = if argc == 10 {
        core::ptr::null()
    } else {
        *argv.add(10)
    };
    fprintf(
        infofid,
        b"command line was: %s %s %s %i %8.2f %i %i %i  %5.2f %4.2f  %s %8.2f %3.2f %8.2f [%i]\n\0".as_ptr() as *const c_char,
        *argv.add(0),
        *argv.add(1),
        *argv.add(2),
        levels,
        depth,
        partitions,
        baseline,
        gl,
        percentage,
        ratio,
        argv10,
        cutOffValue,
        wexp,
        noval,
        corrbed,
    );
    fprintf(infofid, b"(%s) was invoked with following inputs ...\n\0".as_ptr() as *const c_char, *argv.add(0));
    fprintf(infofid, b"    ... input directory: %s \n\0".as_ptr() as *const c_char, *argv.add(1));
    fprintf(infofid, b"    ... output directory: %s\n\0".as_ptr() as *const c_char, *argv.add(2));
    fprintf(infofid, b"    ... levels: %i\n\0".as_ptr() as *const c_char, levels);
    fprintf(infofid, b"    ... depth: %10.4f\n\n\0".as_ptr() as *const c_char, depth);
    fprintf(infofid, b"    ... partitions: %i\n\0".as_ptr() as *const c_char, partitions);
    fprintf(infofid, b"    ... baseline: %i\n\0".as_ptr() as *const c_char, baseline);
    fprintf(infofid, b"    ... GL: %i\n\n\0".as_ptr() as *const c_char, gl);
    fprintf(infofid, b"    ... percentage: %10.4f\n\n\0".as_ptr() as *const c_char, percentage);
    fprintf(infofid, b"    ... ratio: %10.4f\n\n\0".as_ptr() as *const c_char, ratio);
    if interpolateDEM != 0 {
        fprintf(infofid, b"    ... DEM directory: %s\n\0".as_ptr() as *const c_char, *argv.add(10));
        fprintf(infofid, b"    ... cutoff radius: %10.4f\n\0".as_ptr() as *const c_char, cutOffValue);
        fprintf(infofid, b"    ... exponent m of weight (1/r^m): %10.4f\n\0".as_ptr() as *const c_char, wexp);
        fprintf(infofid, b"    ... noval: %10.4f\n\n\0".as_ptr() as *const c_char, noval);
        fprintf(infofid, b"    ... corrbed: %1i\n\n\0".as_ptr() as *const c_char, corrbed);
    }
    fprintf(infofid, b"    ... total points in 2d mesh: %i \n\0".as_ptr() as *const c_char, pointsinlevel);
    fprintf(infofid, b"    ... total elements in 2d mesh: %i \n\0".as_ptr() as *const c_char, elementsinlevel);
    fprintf(infofid, b"    ... total boundary elements in 2d mesh: %i \n\0".as_ptr() as *const c_char, belementsinlevel);
    fprintf(infofid, b"    ... no of element type 101: %i \n\0".as_ptr() as *const c_char, points);
    fprintf(infofid, b"    ... no of element type 202: %i \n\0".as_ptr() as *const c_char, lines);
    fprintf(infofid, b"    ... no of element type 303: %i \n\0".as_ptr() as *const c_char, triangles);
    fprintf(infofid, b"    ... no of element type 404: %i \n\0".as_ptr() as *const c_char, quads);
    printf(b"    ... total points in 2d mesh: %i \n\0".as_ptr() as *const c_char, pointsinlevel);
    printf(b"    ... total elements in 2d mesh: %i \n\0".as_ptr() as *const c_char, elementsinlevel);
    printf(b"    ... total boundary elements in 2d mesh: %i \n\0".as_ptr() as *const c_char, belementsinlevel);
    printf(b"    ... no of element type 101: %9i \n\0".as_ptr() as *const c_char, points);
    printf(b"    ... no of element type 202: %9i \n\0".as_ptr() as *const c_char, lines);
    printf(b"    ... no of element type 303: %9i \n\0".as_ptr() as *const c_char, triangles);
    printf(b"    ... no of element type 404: %9i \n\0".as_ptr() as *const c_char, quads);
    fclose(headerfid);
    strcpy(filename.as_mut_ptr(), outputdirectoryname.as_mut_ptr());
    strcat(filename.as_mut_ptr(), b"/mesh.header.global\0".as_ptr() as *const c_char);
    headerfid = fopen(filename.as_mut_ptr(), b"w\0".as_ptr() as *const c_char);
    if headerfid.is_null() {
        printf(
            b"(%s) Error opening file %s\n\0".as_ptr() as *const c_char,
            *argv.add(0),
            filename.as_mut_ptr(),
        );
        return EXIT_FAILURE;
    }
    fprintf(
        headerfid,
        b"%9i %9i %9i\n\0".as_ptr() as *const c_char,
        pointsinlevel * levels,
        elementsinlevel * (levels - 1),
        elementsinlevel * 2 + belementsinlevel * (levels - 1),
    );
    fprintf(
        headerfid,
        b"%9i\n\0".as_ptr() as *const c_char,
        (if points > 0 { 1 } else { 0 })
            + 2 * (if triangles > 0 { 1 } else { 0 })
            + (if quads > 0 { 1 } else { 0 })
            + 1,
    );
    if points > 0 {
        // NOTE: the discarded `+ lines*baseline` (C parses as
        // (fprintf(...)) + (lines*baseline)): reproduced via let-underscore.
        let _ = fprintf(
            headerfid,
            b"202 %i\n\0".as_ptr() as *const c_char,
            points * (levels - 1),
        ) + lines * baseline;
    }
    if lines > 0 || quads > 0 {
        fprintf(
            headerfid,
            b"404 %i\n\0".as_ptr() as *const c_char,
            lines * (levels - 1) + 2 * quads,
        );
    }
    if triangles > 0 {
        fprintf(
            headerfid,
            b"706 %i\n\0".as_ptr() as *const c_char,
            triangles * (levels - 1),
        );
    }
    if quads > 0 {
        fprintf(
            headerfid,
            b"808 %i\n\0".as_ptr() as *const c_char,
            quads * (levels - 1),
        );
    }
    if partitions > 1 {
        partition = 0;
        while partition < partitions {
            if partition < 10 {
                sprintf(cpartition.as_mut_ptr(), b"/part.%1i\0".as_ptr() as *const c_char, partition + 1);
            } else if partition < 100 {
                sprintf(cpartition.as_mut_ptr(), b"/part.%2i\0".as_ptr() as *const c_char, partition + 1);
            } else if partition < 1000 {
                sprintf(cpartition.as_mut_ptr(), b"/part.%3i\0".as_ptr() as *const c_char, partition + 1);
            } else {
                sprintf(cpartition.as_mut_ptr(), b"/part.%4i\0".as_ptr() as *const c_char, partition + 1);
            }
            strcpy(inputfilename.as_mut_ptr(), inputdirectoryname.as_mut_ptr());
            strcat(inputfilename.as_mut_ptr(), cpartition.as_mut_ptr());
            printf(
                b"(%s) reading partition %i: base file name %s\n\0".as_ptr() as *const c_char,
                *argv.add(0),
                partition + 1,
                inputfilename.as_mut_ptr(),
            );
            strcpy(outputfilename.as_mut_ptr(), outputdirectoryname.as_mut_ptr());
            strcat(outputfilename.as_mut_ptr(), cpartition.as_mut_ptr());
            stat = extrudepartition(
                argv,
                inputfilename.as_mut_ptr(),
                outputfilename.as_mut_ptr(),
                partition,
                partitions,
                levels,
                depth,
                pointsinlevel,
                elementsinlevel,
                belementsinlevel,
                interpolationScheme,
                bed,
                surf,
                thick,
                pointsInDEM.as_mut_ptr(),
                cutOffValue,
                wexp,
                isnoval0,
                isnoval1,
                isnoval2,
            );
            partition += 1;
        }
    } else {
        printf(b"(%s) Doing serial extrusion\n\0".as_ptr() as *const c_char, *argv.add(0));
        strcpy(inputfilename.as_mut_ptr(), inputdirectoryname.as_mut_ptr());
        strcpy(outputfilename.as_mut_ptr(), outputdirectoryname.as_mut_ptr());
        stat = extrudeserial(
            argv,
            inputfilename.as_mut_ptr(),
            outputfilename.as_mut_ptr(),
            levels,
            depth,
            pointsinlevel,
            elementsinlevel,
            belementsinlevel,
            interpolationScheme,
            bed,
            surf,
            thick,
            pointsInDEM.as_mut_ptr(),
            cutOffValue,
            wexp,
            gl,
            percentage,
            ratio,
            baseline,
            corrbed,
            isnoval0,
            isnoval1,
            isnoval2,
        );
    }
    fclose(headerfid);
    fclose(infofid);
    if interpolationScheme > 0 {
        // Null-safe where the original frees uninitialized pointers
        // (documented UB divergence: surf-only / thick-combo runs crash the
        // original at exit; the port frees only allocated buffers).
        if !bed.is_null() {
            free(bed as *mut c_void);
        }
        if !surf.is_null() {
            free(surf as *mut c_void);
        }
    }
    stat
}
