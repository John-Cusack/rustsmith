//! Positive caller: correct hidden lengths, a caller-provided result slot,
//! the alias check, and the allocatable hand-off. Stdout must be
//! byte-identical to the Fortran reference driver.
use cb_caller::*;
use std::ffi::c_char;

fn show(s: &[u8], tag: i32) {
    unsafe {
        __cb_mod_MOD_cb_showlen(s.as_ptr() as *const c_char, &tag, s.len());
    }
}

fn main() {
    show(b"Source", 1);
    show(b"BoundaryOnly", 2);
    let (a, b) = (3.0f64, 4.0f64);
    let mut v = VecT { x: 0.0, y: 0.0, n: 0 };
    unsafe {
        __cb_mod_MOD_cb_makevec(&mut v, &a, &b);
    }
    println!("VEC x={:4.1} y={:4.1} n={}", v.x, v.y, v.n);
    let mut r: GfDesc1 = unsafe { std::mem::zeroed() };
    unsafe {
        __cb_mod_MOD_cb_alias(&mut r);
    }
    let flag = if r.base_addr as usize == unsafe { __cb_mod_MOD_cb_target_addr() } {
        "ALIAS"
    } else {
        "COPY"
    };
    // Shim formula (ADR-027 §2): idx = offset + j*stride with j in
    // [lower, upper] (1-based); offset itself is not the first index.
    let vals: Vec<f64> = (r.dims[0].lower..=r.dims[0].upper)
        .map(|j| unsafe { *r.base_addr.offset((r.offset + j * r.dims[0].stride) as isize) })
        .collect();
    println!(
        "ALIAS-R={:3.1},{:3.1},{:3.1} FLAG={}",
        vals[0], vals[1], vals[2], flag
    );
    let mut bx: GfDesc1 = unsafe { std::mem::zeroed() };
    let n = 4i32;
    unsafe {
        __cb_mod_MOD_cb_fillbox(&mut bx, &n);
        assert_real8_rank1(&bx, "fillbox");
    }
    let items: Vec<f64> = (bx.dims[0].lower..=bx.dims[0].upper)
        .map(|j| unsafe { *bx.base_addr.offset((bx.offset + j * bx.dims[0].stride) as isize) })
        .collect();
    println!(
        "BOX={:4.1},{:4.1},{:4.1},{:4.1}",
        items[0], items[1], items[2], items[3]
    );
    unsafe {
        __cb_mod_MOD_cb_freebox(&mut bx);
    }
}
