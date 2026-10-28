//! Negative control 2: a NULL derived-result slot. The hidden first arg
//! is required — this must not exit 0.
use cb_caller::*;

fn main() {
    let (a, b) = (3.0f64, 4.0f64);
    unsafe {
        __cb_mod_MOD_cb_makevec(std::ptr::null_mut(), &a, &b);
    }
    println!("SURVIVED-NULL-RESULT");
}
