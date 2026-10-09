//! Shim ports for the fortran_shim proof (see ADR-027).
//! Calling convention pinned to gfortran 13.3.0 / x86-64: module
//! procedures via `__<mod>_MOD_<proc>`, F77 globals via `<name>_`,
//! assumed-shape dummies via constructed array descriptors. Only the
//! `BT_REAL` (`real(8)`) dtype is measured; any other dtype needs its own
//! leak-dump before the template below may hardcode it.

/// One dimension of a gfortran array descriptor: element stride (signed),
/// lower bound, upper bound.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDim {
    pub stride: i64,
    pub lower: i64,
    pub upper: i64,
}

/// gfortran dtype word: version 0, rank, BT_* type code, attribute.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDtype {
    pub version: i32,
    pub rank: u8,
    pub type_code: u8,
    pub attribute: u16,
}

/// gfortran array descriptor for `real(8)` rank 2: 40-byte header +
/// 24 bytes per dimension (ADR-027 §2). The callee never retains it.
#[repr(C)]
pub struct GfDesc2 {
    pub base_addr: *const f64,
    pub offset: i64,
    pub elem_size: i64,
    pub dtype: GfDtype,
    pub spare: i64,
    pub dims: [GfDim; 2],
}

/// Explicit-shape dummies travel as raw base addresses; scalar `n` travels
/// by reference (default integer, no `-fdefault-integer-8` here).
#[export_name = "__shim_mod_MOD_explicit_add"]
pub extern "C" fn port_explicit_add(
    a: *const f64,
    b: *const f64,
    c: *mut f64,
    n: *const i32,
) {
    let n = unsafe { *n } as isize;
    for i in 0..n {
        unsafe {
            *c.offset(i) = *a.offset(i) + *b.offset(i);
        }
    }
}

/// Assumed-shape dummy: interpret the caller-built descriptor (whole and
/// section actuals alike) via base + offset + stride arithmetic.
#[export_name = "__shim_mod_MOD_assumed_sum"]
pub extern "C" fn port_assumed_sum(desc: *const GfDesc2) -> f64 {
    let d = unsafe { &*desc };
    debug_assert_eq!(d.elem_size, 8);
    debug_assert_eq!(d.dtype.version, 0);
    debug_assert_eq!(d.dtype.rank, 2);
    debug_assert_eq!(d.dtype.type_code, 3);
    let mut s = 0.0;
    for i0 in d.dims[0].lower..=d.dims[0].upper {
        for i1 in d.dims[1].lower..=d.dims[1].upper {
            let idx = d.offset + i0 * d.dims[0].stride + i1 * d.dims[1].stride;
            s += unsafe { *d.base_addr.offset(idx as isize) };
        }
    }
    s
}

/// F77 assumed-size dummy: same wire shape as explicit-shape (ADR-027 §3).
#[export_name = "fscale_"]
pub extern "C" fn port_fscale(x: *mut f64, n: *const i32, alpha: *const f64) {
    let n = unsafe { *n } as isize;
    let alpha = unsafe { *alpha };
    for i in 0..n {
        unsafe {
            *x.offset(i) *= alpha;
        }
    }
}
