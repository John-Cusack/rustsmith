// Faithful Rust transcription of
// elmerice/Solvers/ScatteredDataInterpolator/csa_interpolate_points.c
// @ 9f6af2f85.
// Unit: c:elmerice/Solvers/ScatteredDataInterpolator/csa_interpolate_points.c
// (exports: csa_interpolate_points, BIND(C)).
//
// The wrapped `csa_*` library (csa-c) is external to the Elmer tree: the
// pristine build only compiles this TU when CSA_FOUND/NN_FOUND/NETCDF_FOUND
// (elmerice/Solvers/ScatteredDataInterpolator/CMakeLists.txt), so there is
// no in-tree binary and no differential probes for this unit (honest halt,
// same class as the USE_METIS rule: external-lib touchers port the wrapper
// against the declared ABI, probes wait for the lib). The `point` layout
// mirrors the Fortran `point_type` module (TYPE,BIND(C) :: POINT, three
// C_DOUBLEs) in Scattered2D_FInterface.F90.
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

#[repr(C)]
pub struct point {
    pub x: c_double,
    pub y: c_double,
    pub z: c_double,
}

#[repr(C)]
pub struct csa {
    _opaque: [u8; 0],
}

extern "C" {
    fn csa_create() -> *mut csa;
    fn csa_addpoints(a: *mut csa, nin: c_int, pin: *const point);
    fn csa_addstd(a: *mut csa, nin: c_int, std: *const c_double);
    fn csa_setnppc(a: *mut csa, nppc: c_int);
    fn csa_setk(a: *mut csa, k: c_int);
    fn csa_calculatespline(a: *mut csa);
    fn csa_approximatepoints(a: *mut csa, nout: c_int, pout: *mut point);
    fn csa_destroy(a: *mut csa);
}

// SAFETY: single-threaded utility; mirrors the C wrapper 1:1 (NULL std,
// conditional nppc/k, create/destroy pairing). The csa_* externs resolve at
// final link against the external csa-c library, exactly as the C TU does.
#[no_mangle]
pub unsafe extern "C" fn csa_interpolate_points(
    nin: c_int,
    pin: *const point,
    nout: c_int,
    pout: *mut point,
    nppc: c_int,
    k: c_int,
) {
    let std: *const c_double = core::ptr::null();
    let a = csa_create();
    csa_addpoints(a, nin, pin);
    csa_addstd(a, nin, std);
    if nppc > 0 {
        csa_setnppc(a, nppc);
    }
    if k > 0 {
        csa_setk(a, k);
    }
    csa_calculatespline(a);
    csa_approximatepoints(a, nout, pout);
    csa_destroy(a);
}
