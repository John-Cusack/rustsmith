//! Negative control 1: a WRONG hidden length (3 for a 6-byte string).
//! Exit 0 with visibly truncated output — the length arg is load-bearing,
//! not decorative.
use cb_caller::*;
use std::ffi::c_char;

fn main() {
    let s = b"Source";
    let tag = 1i32;
    unsafe {
        __cb_mod_MOD_cb_showlen(s.as_ptr() as *const c_char, &tag, 3);
    }
}
