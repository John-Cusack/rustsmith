//! T026 `negres`: forces the REFUSED derived-RESULT shape — a NULL hidden
//! result slot where gfortran writes the returned object. Must fail
//! observably (nonzero exit), never silently green.
use std::ffi::c_void;
use std::os::raw::c_int;
use std::ptr;

extern "C" {
    fn __oval_mod_MOD_oval_make(result_slot: *mut c_void, n: *const c_int);
}

fn main() {
    unsafe {
        let n = 3;
        __oval_mod_MOD_oval_make(ptr::null_mut(), &n);
        println!("NEGRES-UNEXPECTED-GREEN");
        std::process::exit(0);
    }
}
