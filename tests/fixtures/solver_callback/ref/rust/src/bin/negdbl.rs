//! Negative control 3: allocating an already-allocated component.
//! libgfortran must refuse (nonzero exit) — ownership discipline is real.
use cb_caller::*;

fn main() {
    let mut bx: GfDesc1 = unsafe { std::mem::zeroed() };
    let n = 4i32;
    unsafe {
        __cb_mod_MOD_cb_fillbox(&mut bx, &n);
        __cb_mod_MOD_cb_fillbox(&mut bx, &n);
    }
    println!("SURVIVED-DOUBLE-FILL");
}
