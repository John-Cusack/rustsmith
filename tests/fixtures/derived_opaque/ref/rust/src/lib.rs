//! T026 derived-opaque port side: the Rust implementation of the ported
//! module procedure `__oval_mod_MOD_oval_scale`, plus the accessor externs.
//!
//! The opaque `oval` address is NEVER interpreted here: every component
//! flows through the same-gfortran-compiled `BIND(C)` accessors. The
//! scalar `factor` travels by reference, exactly as gfortran passes a
//! plain scalar dummy.
use std::ffi::c_void;
use std::os::raw::{c_double, c_int};

extern "C" {
    fn oval_get_a(oval: *mut c_void, out: *mut c_double);
    fn oval_set_a(oval: *mut c_void, val: c_double);
    fn oval_get_n(oval: *mut c_void) -> c_int;
    fn oval_get_w(oval: *mut c_void, i: c_int, out: *mut c_double);
    fn oval_set_w(oval: *mut c_void, i: c_int, val: c_double);
}

/// Ported module procedure: `subroutine oval_scale(oval, factor)`.
/// Exported under the exact gfortran module-proc symbol so an unported
/// Fortran caller links unchanged (Fortran-caller arm).
#[no_mangle]
pub unsafe extern "C" fn __oval_mod_MOD_oval_scale(oval: *mut c_void, factor: *const c_double) {
    let f = *factor;
    let mut v = 0.0;
    oval_get_a(oval, &mut v);
    oval_set_a(oval, v * f);
    let n = oval_get_n(oval);
    let mut i = 1;
    while i <= n {
        let mut w = 0.0;
        oval_get_w(oval, i, &mut w);
        oval_set_w(oval, i, w * f);
        i += 1;
    }
}
