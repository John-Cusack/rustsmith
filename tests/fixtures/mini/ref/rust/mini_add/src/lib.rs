// Reference port for `fortran:src/mini_add.F90` (Mini fixture).
// Worker-contract shape: staticlib exporting the original BIND(C) linkage
// names (see CmakeBridge::scaffold + §2 worker contract). The deterministic
// proof worker copies this dir into `rust/mini_add/` and builds the archive
// at `build/rust/libmini_add.a`.
#[export_name = "mini_mod"]
pub extern "C" fn port_mini_mod() {}

#[export_name = "mini_add"]
pub extern "C" fn port_mini_add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

#[export_name = "mini_mul"]
pub extern "C" fn port_mini_mul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}
