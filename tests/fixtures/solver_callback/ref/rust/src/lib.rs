//! Shared caller-side ABI shapes for the T020 callback fixtures.
//! Pinned to gfortran 13.3.0 / x86-64 (ADR-027 §1/§3): hidden character
//! lengths travel as `size_t` by value after all explicit args;
//! derived-type function results travel through a hidden first argument
//! (caller-provided result storage); deferred-shape components are
//! gfortran array descriptors (40-byte header + 24 bytes per dimension,
//! same convention as `shim_ports::GfDesc2` at rank 1).
use std::ffi::c_char;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDtype {
    pub version: i32,
    pub rank: u8,
    pub type_code: u8,
    pub attribute: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDim {
    pub stride: i64,
    pub lower: i64,
    pub upper: i64,
}

/// Rank-1 gfortran array descriptor (64 bytes): the whole in-memory shape
/// of a single deferred-shape `real(8)` pointer/allocatable component.
#[repr(C)]
pub struct GfDesc1 {
    pub base_addr: *const f64,
    pub offset: i64,
    pub elem_size: i64,
    pub dtype: GfDtype,
    pub spare: i64,
    pub dims: [GfDim; 1],
}

/// `vec_t` result storage: `sequence` order `x, y, n` matches `repr(C)`.
#[repr(C)]
pub struct VecT {
    pub x: f64,
    pub y: f64,
    pub n: i32,
}

extern "C" {
    pub fn __cb_mod_MOD_cb_showlen(s: *const c_char, tag: *const i32, hidden_len: usize);
    pub fn __cb_mod_MOD_cb_makevec(res: *mut VecT, a: *const f64, b: *const f64);
    pub fn __cb_mod_MOD_cb_alias(res: *mut GfDesc1);
    pub fn __cb_mod_MOD_cb_target_addr() -> usize;
    pub fn __cb_mod_MOD_cb_fillbox(b: *mut GfDesc1, n: *const i32);
    pub fn __cb_mod_MOD_cb_freebox(b: *mut GfDesc1);
}

/// Assert the descriptor header of a freshly allocated `real(8)` rank-1
/// component (internal check: never printed, so the compared stream stays
/// byte-identical while the header is still measured).
pub unsafe fn assert_real8_rank1(d: &GfDesc1, what: &str) {
    assert_eq!(d.elem_size, 8, "{what}: elem_size");
    assert_eq!(d.dtype.version, 0, "{what}: dtype.version");
    assert_eq!(d.dtype.rank, 1, "{what}: dtype.rank");
    assert_eq!(d.dtype.type_code, 3, "{what}: dtype.type_code (BT_REAL)");
}
