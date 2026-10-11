// Faithful Rust transcription of elmergrid/src/egutils.c @ 9f6af2f85.
// Workspace layout: shared declarations live in eg-prelude; this crate
// appends only the unit body (header + use + body from the old line 428).
// Shape notes (foreign vs matc slice): 1-based Numerical-Recipes offset
// pointers (malloc + wrapping_offset back), libc malloc/free discipline
// (NOT calloc/mem_free_all), file I/O (fopen), wall-clock (clock()),
// exit()-on-error paths, static timer globals. SHOWMEM=0 / MEM_USAGE=0
// blocks omitted exactly as the C preprocessor does (MemoryUsage has no
// symbol in egutils.c.o under this build).
// Config scope: no USE_MATC / USE_METIS / HAVE_UNISTD_H references, so one
// archive serves both the ElmerGrid exe and elmergrid_core objects.
// SAFETY: single-threaded utility; raw-pointer arithmetic mirrors C 1:1.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]

use eg_prelude::*;
use std::mem::size_of;

// ---- Numerical Recipes 1-based allocators -------------------------------

// SAFETY: mirrors C: malloc then hand back a pointer biased by (-nl+1);
// only wrapping_offset (no in-bounds claim) is used for the bias.
#[no_mangle]
pub unsafe extern "C" fn vector(nl: c_int, nh: c_int) -> *mut c_float {
    let v = malloc(((nh - nl + 1 + 1) as size_t) * size_of::<c_float>()) as *mut c_float;
    if v.is_null() {
        nrerror(b"allocation failure in vector()\0".as_ptr() as *const c_char);
    }
    v.wrapping_offset(1 - nl as isize)
}

// SAFETY: same bias pattern; C prints + exit(1) when nh < nl.
#[no_mangle]
pub unsafe extern "C" fn ivector(nl: c_int, nh: c_int) -> *mut c_int {
    if nh < nl {
        printf(
            b"Allocation impossible in ivector: %d %d\n\0".as_ptr() as *const c_char,
            nl,
            nh,
        );
        exit(1);
    }
    let v = malloc(((nh - nl + 1 + 1) as size_t) * size_of::<c_int>()) as *mut c_int;
    if v.is_null() {
        nrerror(b"allocation failure in ivector()\0".as_ptr() as *const c_char);
    }
    v.wrapping_offset(1 - nl as isize)
}

// SAFETY: bias pattern as above.
#[no_mangle]
pub unsafe extern "C" fn cvector(nl: c_int, nh: c_int) -> *mut c_char {
    let v = malloc(((nh - nl + 1 + 1) as size_t) * size_of::<c_char>()) as *mut c_char;
    if v.is_null() {
        nrerror(b"allocation failure in cvector()\0".as_ptr() as *const c_char);
    }
    v.wrapping_offset(1 - nl as isize)
}

// SAFETY: bias pattern as above.
#[no_mangle]
pub unsafe extern "C" fn lvector(nl: c_int, nh: c_int) -> *mut c_ulong {
    let v = malloc(((nh - nl + 1 + 1) as size_t) * size_of::<c_ulong>()) as *mut c_ulong;
    if v.is_null() {
        nrerror(b"allocation failure in lvector()\0".as_ptr() as *const c_char);
    }
    v.wrapping_offset(1 - nl as isize)
}

// SAFETY: bias pattern + nh<nl guard as in C.
#[no_mangle]
pub unsafe extern "C" fn dvector(nl: c_int, nh: c_int) -> *mut c_double {
    if nh < nl {
        printf(
            b"Allocation impossible in dvector: %d %d\n\0".as_ptr() as *const c_char,
            nl,
            nh,
        );
        exit(1);
    }
    let v = malloc(((nh - nl + 1 + 1) as size_t) * size_of::<c_double>()) as *mut c_double;
    if v.is_null() {
        nrerror(b"allocation failure in dvector()\0".as_ptr() as *const c_char);
    }
    v.wrapping_offset(1 - nl as isize)
}

// SAFETY: row-pointer block biased by (-nrl+1), data block biased by
// (-ncl+1); row i points at previous row + ncol, exactly as C.
#[no_mangle]
pub unsafe extern "C" fn matrix(
    nrl: c_int,
    nrh: c_int,
    ncl: c_int,
    nch: c_int,
) -> *mut *mut c_float {
    let nrow = nrh - nrl + 1;
    let ncol = nch - ncl + 1;
    let m = malloc(((nrow + 1) as size_t) * size_of::<*mut c_float>()) as *mut *mut c_float;
    if m.is_null() {
        nrerror(b"allocation failure 1 in matrix()\0".as_ptr() as *const c_char);
    }
    let m = m.wrapping_add(1).wrapping_offset(-(nrl as isize));
    let row =
        malloc(((nrow * ncol + 1) as size_t) * size_of::<c_float>()) as *mut c_float;
    if row.is_null() {
        nrerror(b"allocation failure 2 in matrix()\0".as_ptr() as *const c_char);
    }
    *m.wrapping_offset(nrl as isize) = row.wrapping_add(1).wrapping_offset(-(ncl as isize));
    let mut i = nrl + 1;
    while i <= nrh {
        *m.wrapping_offset(i as isize) = (*m.wrapping_offset((i - 1) as isize)).wrapping_offset(ncol as isize);
        i += 1;
    }
    m
}

// SAFETY: double twin of matrix(), with the C range guard.
#[no_mangle]
pub unsafe extern "C" fn dmatrix(
    nrl: c_int,
    nrh: c_int,
    ncl: c_int,
    nch: c_int,
) -> *mut *mut c_double {
    if nrh < nrl || nch < ncl {
        printf(
            b"Allocation impossible in dmatrix: %d %d %d %d\n\0".as_ptr() as *const c_char,
            nrl, nrh, ncl, nch,
        );
        exit(1);
    }
    let nrow = nrh - nrl + 1;
    let ncol = nch - ncl + 1;
    let m = malloc(((nrow + 1) as size_t) * size_of::<*mut c_double>()) as *mut *mut c_double;
    if m.is_null() {
        nrerror(b"allocation failure 1 in dmatrix()\0".as_ptr() as *const c_char);
    }
    let m = m.wrapping_add(1).wrapping_offset(-(nrl as isize));
    let row =
        malloc(((nrow * ncol + 1) as size_t) * size_of::<c_double>()) as *mut c_double;
    if row.is_null() {
        nrerror(b"allocation failure 2 in dmatrix()\0".as_ptr() as *const c_char);
    }
    *m.wrapping_offset(nrl as isize) = row.wrapping_add(1).wrapping_offset(-(ncl as isize));
    let mut i = nrl + 1;
    while i <= nrh {
        *m.wrapping_offset(i as isize) = (*m.wrapping_offset((i - 1) as isize)).wrapping_offset(ncol as isize);
        i += 1;
    }
    m
}

// SAFETY: int twin of matrix(), with the C range guard.
#[no_mangle]
pub unsafe extern "C" fn imatrix(
    nrl: c_int,
    nrh: c_int,
    ncl: c_int,
    nch: c_int,
) -> *mut *mut c_int {
    if nrh < nrl || nch < ncl {
        printf(
            b"Allocation impossible in imatrix: %d %d %d %d\n\0".as_ptr() as *const c_char,
            nrl, nrh, ncl, nch,
        );
        exit(1);
    }
    let nrow = nrh - nrl + 1;
    let ncol = nch - ncl + 1;
    let m = malloc(((nrow + 1) as size_t) * size_of::<*mut c_int>()) as *mut *mut c_int;
    if m.is_null() {
        nrerror(b"allocation failure 1 in imatrix()\0".as_ptr() as *const c_char);
    }
    let m = m.wrapping_add(1).wrapping_offset(-(nrl as isize));
    let row = malloc(((nrow * ncol + 1) as size_t) * size_of::<c_int>()) as *mut c_int;
    if row.is_null() {
        nrerror(b"allocation failure 2 in imatrix()\0".as_ptr() as *const c_char);
    }
    *m.wrapping_offset(nrl as isize) = row.wrapping_add(1).wrapping_offset(-(ncl as isize));
    let mut i = nrl + 1;
    while i <= nrh {
        *m.wrapping_offset(i as isize) = (*m.wrapping_offset((i - 1) as isize)).wrapping_offset(ncol as isize);
        i += 1;
    }
    m
}

// SAFETY: row-pointer block only (no data); m[j] = a[i]+ncol aliases C.
#[no_mangle]
pub unsafe extern "C" fn submatrix(
    a: *mut *mut c_float,
    oldrl: c_int,
    oldrh: c_int,
    _oldcl: c_int,
    _oldch: c_int,
    newrl: c_int,
    newcl: c_int,
) -> *mut *mut c_float {
    let nrow = oldrh - oldrl + 1;
    let ncol = _oldcl - newcl;
    let m = malloc(((nrow + 1) as size_t) * size_of::<*mut c_float>()) as *mut *mut c_float;
    if m.is_null() {
        nrerror(b"allocation failure in submatrix()\0".as_ptr() as *const c_char);
    }
    let m = m.wrapping_add(1).wrapping_offset(-(newrl as isize));
    let mut i = oldrl;
    let mut j = newrl;
    while i <= oldrh {
        *m.wrapping_offset(j as isize) =
            (*a.wrapping_offset(i as isize)).wrapping_offset(ncol as isize);
        i += 1;
        j += 1;
    }
    m
}

// SAFETY: three-level 1-based tensor, pointer setup order as C.
#[no_mangle]
pub unsafe extern "C" fn f3tensor(
    nrl: c_int,
    nrh: c_int,
    ncl: c_int,
    nch: c_int,
    ndl: c_int,
    ndh: c_int,
) -> *mut *mut *mut c_double {
    let nrow = nrh - nrl + 1;
    let ncol = nch - ncl + 1;
    let ndep = ndh - ndl + 1;
    let t = malloc(((nrow + 1) as size_t) * size_of::<*mut *mut c_double>())
        as *mut *mut *mut c_double;
    if t.is_null() {
        nrerror(b"allocation failure 1 in f3tensor()\0".as_ptr() as *const c_char);
    }
    let t = t.wrapping_add(1).wrapping_offset(-(nrl as isize));
    let t_nrl = malloc(((nrow * ncol + 1) as size_t) * size_of::<*mut c_double>())
        as *mut *mut c_double;
    if t_nrl.is_null() {
        nrerror(b"allocation failure 2 in f3tensor()\0".as_ptr() as *const c_char);
    }
    *t.wrapping_offset(nrl as isize) = t_nrl.wrapping_add(1).wrapping_offset(-(ncl as isize));
    let t_data = malloc(((nrow * ncol * ndep + 1) as size_t) * size_of::<c_double>())
        as *mut c_double;
    if t_data.is_null() {
        nrerror(b"allocation failure 3 in f3tensor()\0".as_ptr() as *const c_char);
    }
    *(*t.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize) =
        t_data.wrapping_add(1).wrapping_offset(-(ndl as isize));
    let mut j = ncl + 1;
    while j <= nch {
        let base = *t.wrapping_offset(nrl as isize);
        *base.wrapping_offset(j as isize) =
            (*base.wrapping_offset((j - 1) as isize)).wrapping_offset(ndep as isize);
        j += 1;
    }
    let mut i = nrl + 1;
    while i <= nrh {
        let prev = *t.wrapping_offset((i - 1) as isize);
        let cur = prev.wrapping_offset(ncol as isize);
        *t.wrapping_offset(i as isize) = cur;
        *cur.wrapping_offset(ncl as isize) =
            (*prev.wrapping_offset(ncl as isize)).wrapping_offset((ncol * ndep) as isize);
        let mut j = ncl + 1;
        while j <= nch {
            *cur.wrapping_offset(j as isize) =
                (*cur.wrapping_offset((j - 1) as isize)).wrapping_offset(ndep as isize);
            j += 1;
        }
        i += 1;
    }
    t
}

// ---- Deallocation (un-bias exactly as C frees) --------------------------

// SAFETY: free of the original malloc pointer (v+nl-1).
#[no_mangle]
pub unsafe extern "C" fn free_vector(v: *mut c_float, nl: c_int, _nh: c_int) {
    free(v.wrapping_offset(nl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_ivector(v: *mut c_int, nl: c_int, _nh: c_int) {
    free(v.wrapping_offset(nl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_cvector(v: *mut c_char, nl: c_int, _nh: c_int) {
    free(v.wrapping_offset(nl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_lvector(v: *mut c_ulong, nl: c_int, _nh: c_int) {
    free(v.wrapping_offset(nl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_dvector(v: *mut c_double, nl: c_int, _nh: c_int) {
    free(v.wrapping_offset(nl as isize - 1) as *mut c_void);
}

// SAFETY: data block then row-pointer block, un-biased as C.
#[no_mangle]
pub unsafe extern "C" fn free_matrix(
    m: *mut *mut c_float,
    nrl: c_int,
    _nrh: c_int,
    ncl: c_int,
    _nch: c_int,
) {
    free(
        (*m.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize - 1) as *mut c_void,
    );
    free(m.wrapping_offset(nrl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_dmatrix(
    m: *mut *mut c_double,
    nrl: c_int,
    _nrh: c_int,
    ncl: c_int,
    _nch: c_int,
) {
    free(
        (*m.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize - 1) as *mut c_void,
    );
    free(m.wrapping_offset(nrl as isize - 1) as *mut c_void);
}

// SAFETY: as above.
#[no_mangle]
pub unsafe extern "C" fn free_imatrix(
    m: *mut *mut c_int,
    nrl: c_int,
    _nrh: c_int,
    ncl: c_int,
    _nch: c_int,
) {
    free(
        (*m.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize - 1) as *mut c_void,
    );
    free(m.wrapping_offset(nrl as isize - 1) as *mut c_void);
}

// SAFETY: row-pointer block only (data owned by parent).
#[no_mangle]
pub unsafe extern "C" fn free_submatrix(
    b: *mut *mut c_float,
    nrl: c_int,
    _nrh: c_int,
    _ncl: c_int,
    _nch: c_int,
) {
    free(b.wrapping_offset(nrl as isize - 1) as *mut c_void);
}

// SAFETY: data, row block, top block, un-biased as C.
#[no_mangle]
pub unsafe extern "C" fn free_f3tensor(
    t: *mut *mut *mut c_double,
    nrl: c_int,
    _nrh: c_int,
    ncl: c_int,
    _nch: c_int,
    ndl: c_int,
    _ndh: c_int,
) {
    free(
        (*(*t.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize))
            .wrapping_offset(ndl as isize - 1) as *mut c_void,
    );
    free(
        (*t.wrapping_offset(nrl as isize)).wrapping_offset(ncl as isize - 1) as *mut c_void,
    );
    free(t.wrapping_offset(nrl as isize - 1) as *mut c_void);
}

// ---- Timer (file-scope statics mirrored as module statics) ---------------

static mut timer_t0: Real = 0.0;
static mut timer_dt: Real = 0.0;
static mut timer_active: c_int = FALSE;
static mut timer_filename: [c_char; 600] = [0; 600];

// SAFETY: C-equivalent global reset.
#[no_mangle]
pub unsafe extern "C" fn timer_init() {
    timer_active = FALSE;
}

// SAFETY: mirrors C: wall clock, AddExtension(prefix, timer_filename,
// "time"), two printf lines, stamp both globals.
#[no_mangle]
pub unsafe extern "C" fn timer_activate(prefix: *const c_char) {
    let time = clock() as Real / CLOCKS_PER_SEC as Real;
    timer_active = TRUE;
    AddExtension(
        prefix,
        timer_filename.as_mut_ptr(),
        b"time\0".as_ptr() as *const c_char,
    );
    printf(
        b"Activating timer (s): %.2f\n\0".as_ptr() as *const c_char,
        time,
    );
    printf(
        b"Saving timer info to file %s\n\0".as_ptr() as *const c_char,
        timer_filename.as_ptr(),
    );
    timer_dt = time;
    timer_t0 = time;
}

// SAFETY: function-local `visited` becomes a private module static;
// fopen "w" first visit else "a"; timings formatted as C.
static mut timer_visited: c_int = 0;

#[no_mangle]
pub unsafe extern "C" fn timer_show() {
    if timer_active == 0 {
        return;
    }
    let time = clock() as Real / CLOCKS_PER_SEC as Real;
    printf(
        b"Elapsed time (s): %.2f %.2f\n\0".as_ptr() as *const c_char,
        time - timer_t0,
        time - timer_dt,
    );
    timer_visited += 1;
    let out = if timer_visited == 1 {
        fopen(
            timer_filename.as_ptr(),
            b"w\0".as_ptr() as *const c_char,
        )
    } else {
        fopen(
            timer_filename.as_ptr(),
            b"a\0".as_ptr() as *const c_char,
        )
    };
    fprintf(
        out,
        b"%3d %12.4le %12.4le\n\0".as_ptr() as *const c_char,
        timer_visited,
        time - timer_t0,
        time - timer_dt,
    );
    fclose(out);
    timer_dt = time;
}

// ---- Diagnostics ----------------------------------------------------------

// SAFETY: three stderr lines then exit(1), as C.
#[no_mangle]
pub unsafe extern "C" fn nrerror(error_text: *const c_char) {
    fprintf(
        stderr,
        b"run-time error...\n\0".as_ptr() as *const c_char,
    );
    fprintf(stderr, b"%s\n\0".as_ptr() as *const c_char, error_text);
    fprintf(
        stderr,
        b"...now exiting to system...\n\0".as_ptr() as *const c_char,
    );
    exit(1);
}

// SAFETY: as C.
#[no_mangle]
pub unsafe extern "C" fn bigerror(error_text: *const c_char) {
    fprintf(
        stderr,
        b"***** ERROR: We cannot continue with this! *****\n\0".as_ptr() as *const c_char,
    );
    fprintf(stderr, b"***** %s\n\0".as_ptr() as *const c_char, error_text);
    fprintf(
        stderr,
        b"***** ...now exiting to system!\n\0".as_ptr() as *const c_char,
    );
    exit(1);
}

// SAFETY: as C.
#[no_mangle]
pub unsafe extern "C" fn smallerror(error_text: *const c_char) {
    fprintf(
        stderr,
        b"***** WARNING: This should not happen but we try to continue! *****\n\0"
            .as_ptr() as *const c_char,
    );
    fprintf(stderr, b"***** %s\n\0".as_ptr() as *const c_char, error_text);
}

// ---- File / string / numeric helpers ---------------------------------------

// SAFETY: fopen/fclose probe exactly as C.
#[no_mangle]
pub unsafe extern "C" fn FileExists(filename: *mut c_char) -> c_int {
    let inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        FALSE
    } else {
        fclose(inn);
        TRUE
    }
}

// SAFETY: plain in-bounds scan over [first..=last], as C.
#[no_mangle]
pub unsafe extern "C" fn Minimum(vector: *mut Real, first: c_int, last: c_int) -> Real {
    let mut min = *vector.wrapping_offset(first as isize);
    let mut i = first + 1;
    while i <= last {
        let v = *vector.wrapping_offset(i as isize);
        if min > v {
            min = v;
        }
        i += 1;
    }
    min
}

// SAFETY: as C, including the (mini=i) assignment-in-condition shape.
#[no_mangle]
pub unsafe extern "C" fn Minimi(vector: *mut Real, first: c_int, last: c_int) -> c_int {
    let mut mini = first;
    let mut min = *vector.wrapping_offset(first as isize);
    let mut i = first + 1;
    while i <= last {
        if min > *vector.wrapping_offset(i as isize) {
            mini = i;
            min = *vector.wrapping_offset(mini as isize);
        }
        i += 1;
    }
    mini
}

// SAFETY: as C.
#[no_mangle]
pub unsafe extern "C" fn Maximum(vector: *mut Real, first: c_int, last: c_int) -> Real {
    let mut max = *vector.wrapping_offset(first as isize);
    let mut i = first + 1;
    while i <= last {
        let v = *vector.wrapping_offset(i as isize);
        if max < v {
            max = v;
        }
        i += 1;
    }
    max
}

// SAFETY: as C, including maxi=-1 init and (maxi=i) shape.
#[no_mangle]
pub unsafe extern "C" fn Maximi(vector: *mut Real, first: c_int, last: c_int) -> c_int {
    let mut maxi: c_int = -1;
    let mut max = *vector.wrapping_offset(first as isize);
    let mut i = first + 1;
    while i <= last {
        if max < *vector.wrapping_offset(i as isize) {
            maxi = i;
            max = *vector.wrapping_offset(maxi as isize);
        }
        i += 1;
    }
    maxi
}

// SAFETY: caller guarantees fname2 has room (C contract); mutates via
// strcpy/strrchr/strcat exactly as C, including the dot-after-slash guard.
#[no_mangle]
pub unsafe extern "C" fn AddExtension(
    fname1: *const c_char,
    fname2: *mut c_char,
    newext: *const c_char,
) {
    strcpy(fname2, fname1);
    let ptr1 = strrchr(fname2, '.' as c_int);
    if !ptr1.is_null() {
        let mut badpoint = FALSE;
        let ptr2 = strrchr(fname2, '/' as c_int);
        if !ptr2.is_null() && ptr2 > ptr1 {
            badpoint = TRUE;
        }
        if badpoint == 0 {
            *ptr1 = 0;
        }
    }
    strcat(fname2, b".\0".as_ptr() as *const c_char);
    strcat(fname2, newext);
}

// SAFETY: copies at most 15 chars per token into args[cnt] (cnt < maxcnt
// per loop guard); reads bounded by embedded NULs / separator, as C.
#[no_mangle]
pub unsafe extern "C" fn StringToStrings(
    buf: *const c_char,
    args: *mut [c_char; 15],
    maxcnt: c_int,
    separator: c_char,
) -> c_int {
    let mut i: c_int;
    let mut cnt: c_int = 0;
    let mut finish: c_int = 0;
    let mut ptr1 = buf as *mut c_char;
    let totlen = strlen(buf) as isize;
    if *buf == 0 {
        return 0;
    }
    loop {
        let ptr2 = strchr(ptr1, separator as c_int);
        if !ptr2.is_null() {
            i = 0;
            while i < 15 {
                (*args.wrapping_offset(cnt as isize))[i as usize] =
                    *ptr1.wrapping_offset(i as isize);
                if ptr1.wrapping_offset(i as isize) >= ptr2 {
                    break;
                }
                i += 1;
            }
            (*args.wrapping_offset(cnt as isize))[i as usize] = 0;
            ptr1 = ptr2.wrapping_offset(1);
        } else {
            i = 0;
            while i < 15 {
                if ptr1.wrapping_offset(i as isize) >= (buf as *mut c_char).wrapping_offset(totlen) {
                    break;
                }
                (*args.wrapping_offset(cnt as isize))[i as usize] =
                    *ptr1.wrapping_offset(i as isize);
                i += 1;
            }
            (*args.wrapping_offset(cnt as isize))[i as usize] = 0;
            finish = 1;
        }
        cnt += 1;
        if !(cnt < maxcnt && finish == 0) {
            break;
        }
    }
    cnt
}

// SAFETY: mutates buf at separators (C contract: buf is writable);
// dest holds maxcnt entries per loop guard.
#[no_mangle]
pub unsafe extern "C" fn StringToReal(
    buf: *const c_char,
    dest: *mut Real,
    maxcnt: c_int,
    separator: c_char,
) -> c_int {
    let mut cnt: c_int = 0;
    let mut ptr1 = buf as *mut c_char;
    if *buf == 0 {
        return 0;
    }
    loop {
        let ptr2 = strchr(ptr1, separator as c_int);
        if !ptr2.is_null() {
            *ptr2 = 0;
        }
        *dest.wrapping_offset(cnt as isize) = atof(ptr1);
        cnt += 1;
        if !ptr2.is_null() {
            ptr1 = ptr2.wrapping_offset(1);
        }
        if !(cnt < maxcnt && !ptr2.is_null()) {
            break;
        }
    }
    cnt
}

// SAFETY: as StringToReal with atoi.
#[no_mangle]
pub unsafe extern "C" fn StringToInteger(
    buf: *const c_char,
    dest: *mut c_int,
    maxcnt: c_int,
    separator: c_char,
) -> c_int {
    let mut cnt: c_int = 0;
    let mut ptr1 = buf as *mut c_char;
    if *buf == 0 {
        return 0;
    }
    loop {
        let ptr2 = strchr(ptr1, separator as c_int);
        if !ptr2.is_null() {
            *ptr2 = 0;
        }
        let ival = atoi(ptr1);
        *dest.wrapping_offset(cnt as isize) = ival;
        cnt += 1;
        if !ptr2.is_null() {
            ptr1 = ptr2.wrapping_offset(1);
        }
        if !(cnt < maxcnt && !ptr2.is_null()) {
            break;
        }
    }
    cnt
}

// SAFETY: as StringToInteger, stopping before a zero value (C: the zero
// is NOT stored, matching `break` before dest[cnt++]=ival).
#[no_mangle]
pub unsafe extern "C" fn StringToIntegerNoZero(
    buf: *const c_char,
    dest: *mut c_int,
    maxcnt: c_int,
    separator: c_char,
) -> c_int {
    let mut cnt: c_int = 0;
    let mut ptr1 = buf as *mut c_char;
    if *buf == 0 {
        return 0;
    }
    loop {
        let ptr2 = strchr(ptr1, separator as c_int);
        if !ptr2.is_null() {
            *ptr2 = 0;
        }
        let ival = atoi(ptr1);
        if ival == 0 {
            break;
        }
        *dest.wrapping_offset(cnt as isize) = ival;
        cnt += 1;
        if !ptr2.is_null() {
            ptr1 = ptr2.wrapping_offset(1);
        }
        if !(cnt < maxcnt && !ptr2.is_null()) {
            break;
        }
    }
    cnt
}

// SAFETY: strtol advances *start, as C.
#[no_mangle]
pub unsafe extern "C" fn next_int(start: *mut *mut c_char) -> c_int {
    let mut end: *mut c_char = core::ptr::null_mut();
    let i = strtol(*start, &mut end, 10) as c_int;
    *start = end;
    i
}

// SAFETY: reads n chars at *start (C contract: valid); NUL bounded.
#[no_mangle]
pub unsafe extern "C" fn next_int_n(start: *mut *mut c_char, n: c_int) -> c_int {
    let end = (*start).wrapping_offset(n as isize);
    let saved = *end;
    *end = 0;
    let i = strtol(*start, core::ptr::null_mut(), 10) as c_int;
    *end = saved;
    *start = end;
    i
}

// SAFETY: strtod advances *start, as C.
#[no_mangle]
pub unsafe extern "C" fn next_real(start: *mut *mut c_char) -> Real {
    let mut end: *mut c_char = core::ptr::null_mut();
    let r = strtod(*start, &mut end);
    *start = end;
    r
}

// SAFETY: 1-based heapsort; Key/Ord valid for 1..=N (C contract).
// Ord[i]=i init loop runs 1..=N exactly as C.
#[no_mangle]
pub unsafe extern "C" fn SortIndex(N: c_int, Key: *mut c_double, Ord: *mut c_int) {
    let mut current_key: c_double;
    let mut current_ord: c_int;
    let mut cur_last_pos: c_int;
    let mut cur_half_pos: c_int;
    let mut i: c_int;
    let mut j: c_int;
    i = 1;
    while i <= N {
        *Ord.wrapping_offset(i as isize) = i;
        i += 1;
    }
    cur_half_pos = N / 2 + 1;
    cur_last_pos = N;
    loop {
        if cur_half_pos > 1 {
            cur_half_pos -= 1;
            current_key = *Key.wrapping_offset(cur_half_pos as isize);
            current_ord = *Ord.wrapping_offset(cur_half_pos as isize);
        } else {
            current_key = *Key.wrapping_offset(cur_last_pos as isize);
            current_ord = *Ord.wrapping_offset(cur_last_pos as isize);
            *Key.wrapping_offset(cur_last_pos as isize) = *Key.wrapping_offset(1);
            *Ord.wrapping_offset(cur_last_pos as isize) = *Ord.wrapping_offset(1);
            cur_last_pos -= 1;
            if cur_last_pos == 1 {
                *Key.wrapping_offset(1) = current_key;
                *Ord.wrapping_offset(1) = current_ord;
                return;
            }
        }
        i = cur_half_pos;
        j = 2 * cur_half_pos;
        while j <= cur_last_pos {
            if j < cur_last_pos
                && *Key.wrapping_offset(j as isize) < *Key.wrapping_offset((j + 1) as isize)
            {
                j += 1;
            }
            if current_key < *Key.wrapping_offset(j as isize) {
                *Key.wrapping_offset(i as isize) = *Key.wrapping_offset(j as isize);
                *Ord.wrapping_offset(i as isize) = *Ord.wrapping_offset(j as isize);
                i = j;
                j += i;
            } else {
                j = cur_last_pos + 1;
            }
        }
        *Key.wrapping_offset(i as isize) = current_key;
        *Ord.wrapping_offset(i as isize) = current_ord;
    }
}
