// Port root for the Elmer FEM mirror (CMake staticlib flow).
//
// Per-unit ports land here via the worker (see `CmakeBridge::scaffold` for
// the per-unit crate shape: `port_<linkage>` fns with `#[export_name]`,
// non-`BIND(C)` units at file granularity behind an ABI shim).
// `CmakeBridge::substitute` splices the built archive in place of the unit
// objects of the `elmersolver` target and reruns the unit's ctest.
