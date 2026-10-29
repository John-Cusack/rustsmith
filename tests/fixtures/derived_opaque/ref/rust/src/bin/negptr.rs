//! T026 `negptr`: forces the REFUSED pointer-dummy shape — a NULL pointer
//! descriptor where gfortran expects association state. Must fail
//! observably (nonzero exit), never silently green.
use std::ffi::c_void;
use std::os::raw::c_double;
use std::ptr;

extern "C" {
    fn __oval_mod_MOD_oval_use_ptr(p: *mut c_void, factor: *const c_double);
}

fn main() {
    unsafe {
        let factor = 2.0;
        __oval_mod_MOD_oval_use_ptr(ptr::null_mut(), &factor);
        println!("NEGPTR-UNEXPECTED-GREEN");
        std::process::exit(0);
    }
}
