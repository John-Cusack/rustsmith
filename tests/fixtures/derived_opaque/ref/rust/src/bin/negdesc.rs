//! T026 `negdesc`: forces the REFUSED allocatable-array shape through the
//! opaque convention — a NULL descriptor where gfortran expects the array
//! descriptor. Must fail observably (nonzero exit), never silently green:
//! the callee dereferences a descriptor the opaque address does not carry,
//! which is exactly why the gate refuses this shape.
use std::ffi::c_void;
use std::os::raw::c_int;
use std::ptr;

extern "C" {
    fn __oval_mod_MOD_oval_fill(list: *mut c_void, n: *const c_int);
}

fn main() {
    unsafe {
        let n = 3;
        __oval_mod_MOD_oval_fill(ptr::null_mut(), &n);
        // If control returns, the shape went silently green: fail loudly.
        println!("NEGDESC-UNEXPECTED-GREEN");
        std::process::exit(0);
    }
}
