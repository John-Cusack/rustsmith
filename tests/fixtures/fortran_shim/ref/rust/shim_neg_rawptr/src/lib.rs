//! NEGATIVE fixture: raw-pointer call into an assumed-shape dummy.
//! This is the pre-shim refusal shape (ADR-027 Problem): the caller passes
//! a descriptor pointer, this stub reads it as `double*`. MUST diverge
//! from the reference output (wrong science, exit 0), never go green.
//! The explicit-add and scale companions are correct ports isolating the
//! fault to the assumed-shape entry.

/// Explicit-shape dummies travel as raw base addresses (correct port).
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

/// THE FAULT: descriptor pointer misread as the first four doubles.
#[export_name = "__shim_mod_MOD_assumed_sum"]
pub extern "C" fn neg_assumed_sum_as_raw_ptr(p: *const f64) -> f64 {
    unsafe { *p.add(0) + *p.add(1) + *p.add(2) + *p.add(3) }
}

/// F77 assumed-size dummy: raw base address (correct port).
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
