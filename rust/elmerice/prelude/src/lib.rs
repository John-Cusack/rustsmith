// Faithful shared surface for the elmerice/ lane ports (Elmer/Ice @ 9f6af2f85).
// Mirrors the eg-prelude conventions: libc malloc/free discipline, raw C
// file/printf FFI (byte-identical float formatting comes from the C
// library, never Rust Display), every unsafe carries SAFETY.
// Single-threaded utilities; `static mut` mirrors C file-scope globals.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]
// Lane convention: C format strings stay `b"..\0"` (byte-identical to the
// transcribed TU; `c""` churn risks the load-bearing bytes), and SAFETY
// contracts live in `// SAFETY:` comments at every unsafe site, not in doc
// sections. Both lints are style-only against this convention.
#![allow(clippy::manual_c_str_literals)]
#![allow(clippy::missing_safety_doc)]

pub mod fortranio;

pub type c_char = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type c_float = f32;
pub type c_double = f64;
pub type c_void = core::ffi::c_void;
pub type size_t = usize;

#[repr(C)]
pub struct FILE {
    _opaque: [u8; 0],
}

pub const EXIT_SUCCESS: c_int = 0;
pub const EXIT_FAILURE: c_int = 1;

extern "C" {
    pub fn malloc(size: size_t) -> *mut c_void;
    pub fn calloc(nmemb: size_t, size: size_t) -> *mut c_void;
    pub fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void;
    pub fn free(ptr: *mut c_void);
    pub fn printf(fmt: *const c_char, ...) -> c_int;
    pub fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn sprintf(buf: *mut c_char, fmt: *const c_char, ...) -> c_int;
    pub fn sscanf(buf: *const c_char, fmt: *const c_char, ...) -> c_int;
    pub fn fscanf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn fgets(s: *mut c_char, n: c_int, stream: *mut FILE) -> *mut c_char;
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    pub fn fclose(stream: *mut FILE) -> c_int;
    pub fn rewind(stream: *mut FILE);
    pub fn feof(stream: *mut FILE) -> c_int;
    pub fn puts(s: *const c_char) -> c_int;
    pub fn fputs(s: *const c_char, stream: *mut FILE) -> c_int;
    pub fn fread(ptr: *mut c_void, size: size_t, nmemb: size_t, stream: *mut FILE) -> size_t;
    pub fn fwrite(ptr: *const c_void, size: size_t, nmemb: size_t, stream: *mut FILE) -> size_t;
    pub fn fseek(stream: *mut FILE, offset: c_long, whence: c_int) -> c_int;
    pub fn ftell(stream: *mut FILE) -> c_long;
    pub fn fflush(stream: *mut FILE) -> c_int;
    pub fn exit(status: c_int) -> !;
    pub fn abort() -> !;
    pub fn atoi(s: *const c_char) -> c_int;
    pub fn atof(s: *const c_char) -> c_double;
    pub fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
    pub fn strtod(s: *const c_char, end: *mut *mut c_char) -> c_double;
    pub fn strlen(s: *const c_char) -> size_t;
    pub fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char;
    pub fn strcat(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strncat(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char;
    pub fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int;
    pub fn strncmp(s1: *const c_char, s2: *const c_char, n: size_t) -> c_int;
    pub fn strtok(s: *mut c_char, delim: *const c_char) -> *mut c_char;
    pub fn strchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
    pub fn memset(s: *mut c_void, c: c_int, n: size_t) -> *mut c_void;
    pub fn mkdir(path: *const c_char, mode: c_uint) -> c_int;
    pub fn pow(x: c_double, y: c_double) -> c_double;
    pub fn sqrt(x: c_double) -> c_double;
    pub fn fabs(x: c_double) -> c_double;
}

// stdin/stdout/stderr are macros on glibc, not symbols. Accessors:
extern "C" {
    #[link_name = "stdin"]
    pub static stdin_ptr: *mut FILE;
    #[link_name = "stdout"]
    pub static stdout_ptr: *mut FILE;
    #[link_name = "stderr"]
    pub static stderr_ptr: *mut FILE;
}

#[inline]
pub fn c_stdout() -> *mut FILE {
    // SAFETY: glibc exposes stdout as a stable global; read-only use.
    unsafe { stdout_ptr }
}

#[inline]
pub fn c_stderr() -> *mut FILE {
    // SAFETY: same as stdout.
    unsafe { stderr_ptr }
}
