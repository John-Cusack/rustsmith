//! T026 `pos`: Rust caller drives the ADMITTED shape — it holds only the
//! opaque address (built Fortran-side by `oval_new_c`), calls the Fortran
//! `oval_scale` with the plain-address convention, and reads back through
//! the accessors. Must print byte-identical output to the Fortran reference.
use std::ffi::c_void;
use std::os::raw::{c_double, c_int};

extern "C" {
    fn oval_new_c(n: c_int, a0: c_double) -> *mut c_void;
    fn oval_free_c(p: *mut c_void);
    fn __oval_mod_MOD_oval_scale(oval: *mut c_void, factor: *const c_double);
    fn oval_get_a(oval: *mut c_void, out: *mut c_double);
    fn oval_get_n(oval: *mut c_void) -> c_int;
    fn oval_get_w(oval: *mut c_void, i: c_int, out: *mut c_double);
}

fn main() {
    unsafe {
        let oval = oval_new_c(3, 1.5);
        assert!(!oval.is_null());
        let factor = 2.0;
        __oval_mod_MOD_oval_scale(oval, &factor);
        let mut a = 0.0;
        oval_get_a(oval, &mut a);
        let n = oval_get_n(oval);
        let mut w = [0.0; 3];
        for (k, slot) in w.iter_mut().enumerate() {
            oval_get_w(oval, k as c_int + 1, slot);
        }
        println!("A={a:6.2} N={n}");
        println!("W={:6.2}{:6.2}{:6.2}", w[0], w[1], w[2]);
        oval_free_c(oval);
    }
}
