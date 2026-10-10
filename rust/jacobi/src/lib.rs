// Master prelude for matc unit ports (T005). Pasted verbatim at the top of
// every rust/<stem>/src/lib.rs; per-crate code appends its own extern block
// (matc-cross imports it uses) plus the transcribed unit body.
// Faithful transcription notes:
// - All heap memory via libc calloc/free (C ABI: mem_free_all in the matc
//   crate frees blocks with C free(); Rust allocators must never be used).
// - `static mut` mirrors C file-scope globals (single-threaded REPL).
// - Every `unsafe` use carries a SAFETY comment (unsafe-budget audit).

// SAFETY: prelude contains no unsafe code, only declarations.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
// C-ABI global tables accessed exactly like the C original (single-threaded
// REPL, no reentrancy beyond C's own): raw-pointer access is the port.
#![allow(static_mut_refs)]
#![allow(dead_code)]

use std::mem::size_of;

// ---- C scalar aliases -------------------------------------------------
pub type c_char = i8;
pub type c_uchar = u8;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_double = f64;
pub type size_t = usize;

// ---- opaque C types ----------------------------------------------------
pub enum FILE {}
pub enum JmpBuf {}

// ---- LIST --------------------------------------------------------------
#[repr(C)]
pub struct LIST {
    pub next: *mut LIST,
    pub name: *mut c_char,
}
pub const ALLOCATIONS: c_int = 0;
pub const CONSTANTS: c_int = 1;
pub const VARIABLES: c_int = 2;
pub const COMMANDS: c_int = 3;
pub const FUNCTIONS: c_int = 4;

#[repr(C)]
pub struct ALLOC_LIST {
    pub next: *mut ALLOC_LIST,
    pub mem: *mut c_char,
}

// ---- MATRIX / VARIABLE --------------------------------------------------
#[repr(C)]
pub struct MATRIX {
    pub typ: c_int,
    pub refcount: c_int,
    pub nrow: c_int,
    pub ncol: c_int,
    pub data: *mut c_double,
}
#[repr(C)]
pub struct VARIABLE {
    pub next: *mut VARIABLE,
    pub name: *mut c_char,
    pub changed: c_int,
    pub this: *mut MATRIX,
}
pub const TYPE_DOUBLE: c_int = 0;
pub const TYPE_COMPLEX: c_int = 1;
pub const TYPE_STRING: c_int = 2;

pub const VARIABLESIZE: usize = 32;
pub const MATRIXSIZE: usize = 24;
const _: () = assert!(size_of::<VARIABLE>() == VARIABLESIZE);
const _: () = assert!(size_of::<MATRIX>() == MATRIXSIZE);
const _: () = assert!(size_of::<LIST>() == 16);
const _: () = assert!(size_of::<ALLOC_LIST>() == 16);

// SAFETY: single-threaded REPL; mirrors C file-scope access macros.
#[inline]
pub unsafe fn lst_next(lp: *mut LIST) -> *mut LIST {
    (*lp).next
}
#[inline]
pub unsafe fn var_next(var_p: *mut VARIABLE) -> *mut VARIABLE {
    (*var_p).next
}
#[inline]
pub unsafe fn var_name(var_p: *mut VARIABLE) -> *mut c_char {
    (*var_p).name
}
#[inline]
pub unsafe fn var_matr(var_p: *mut VARIABLE) -> *mut c_double {
    (*(*var_p).this).data
}
#[inline]
pub unsafe fn var_type(var_p: *mut VARIABLE) -> c_int {
    (*(*var_p).this).typ
}
#[inline]
pub unsafe fn var_nrow(var_p: *mut VARIABLE) -> c_int {
    (*(*var_p).this).nrow
}
#[inline]
pub unsafe fn var_ncol(var_p: *mut VARIABLE) -> c_int {
    (*(*var_p).this).ncol
}
#[inline]
pub unsafe fn var_refcnt(var_p: *mut VARIABLE) -> c_int {
    (*(*var_p).this).refcount
}
#[inline]
pub unsafe fn var_set_refcnt(var_p: *mut VARIABLE, r: c_int) {
    (*(*var_p).this).refcount = r;
}
#[inline]
pub unsafe fn var_m(var_p: *mut VARIABLE, i: c_int, j: c_int) -> c_double {
    let mm = (*var_p).this;
    *(*mm).data.offset((i * (*mm).ncol + j) as isize)
}
#[inline]
pub unsafe fn var_set_m(var_p: *mut VARIABLE, i: c_int, j: c_int, val: c_double) {
    let mm = (*var_p).this;
    *(*mm).data.offset((i * (*mm).ncol + j) as isize) = val;
}
#[inline]
pub unsafe fn mat_type(mat_p: *mut MATRIX) -> c_int {
    (*mat_p).typ
}
#[inline]
pub unsafe fn var_matsize(var_p: *mut VARIABLE) -> usize {
    ((*var_p).this.as_ref().unwrap().nrow as usize)
        * ((*var_p).this.as_ref().unwrap().ncol as usize)
        * size_of::<c_double>()
}
pub unsafe fn mat_matsize(mat_p: *mut MATRIX) -> usize {
    ((*mat_p).nrow as usize) * ((*mat_p).ncol as usize) * size_of::<c_double>()
}
#[inline]
pub unsafe fn mat_data_embedded(mat_p: *mut MATRIX) -> bool {
    (*mat_p).data == ((mat_p as *mut c_char).add(MATRIXSIZE) as *mut c_double)
}
#[inline]
pub unsafe fn var_scalar_combined(var_p: *mut VARIABLE) -> bool {
    (*var_p).this == ((var_p as *mut c_char).add(VARIABLESIZE) as *mut MATRIX)
}
pub const VAR_WRAPPER_POOL_FLAG: c_int = 0x40000000;
#[inline]
pub unsafe fn var_wrapper_pooled(var_p: *mut VARIABLE) -> bool {
    (*var_p).changed & VAR_WRAPPER_POOL_FLAG != 0
}

// ---- COMMAND / FUNCTION --------------------------------------------------
pub type CommandSub = Option<unsafe extern "C" fn(*mut VARIABLE) -> *mut VARIABLE>;
#[repr(C)]
pub struct COMMAND {
    pub next: *mut COMMAND,
    pub name: *mut c_char,
    pub flags: c_int,
    pub minp: c_int,
    pub maxp: c_int,
    pub sub: CommandSub,
    pub help: *mut c_char,
}
pub const CMDFLAG_PW: c_int = 1;
pub const CMDFLAG_CE: c_int = 2;
const _: () = assert!(size_of::<COMMAND>() == 48);

#[repr(C)]
pub struct FUNCTION {
    pub next: *mut FUNCTION,
    pub name: *mut c_char,
    pub parnames: *mut *mut c_char,
    pub exports: *mut *mut c_char,
    pub imports: *mut *mut c_char,
    pub help: *mut c_char,
    pub parcount: c_int,
    pub body: *mut CLAUSE,
}
const _: () = assert!(size_of::<FUNCTION>() == 64);
const _: () = assert!(size_of::<TREEENTRY>() == 32);

// ---- parser symbols -------------------------------------------------------
pub type SYMTYPE = c_int;
pub const nullsym: SYMTYPE = 0;
pub const leftpar: SYMTYPE = 1;
pub const rightpar: SYMTYPE = 2;
pub const indopen: SYMTYPE = 3;
pub const indclose: SYMTYPE = 4;
pub const power: SYMTYPE = 5;
pub const times: SYMTYPE = 6;
pub const ptimes: SYMTYPE = 7;
pub const divide: SYMTYPE = 8;
pub const plus: SYMTYPE = 9;
pub const minus: SYMTYPE = 10;
pub const reduction: SYMTYPE = 11;
pub const transpose: SYMTYPE = 12;
pub const eq: SYMTYPE = 13;
pub const neq: SYMTYPE = 14;
pub const lt: SYMTYPE = 15;
pub const gt: SYMTYPE = 16;
pub const le: SYMTYPE = 17;
pub const ge: SYMTYPE = 18;
pub const and: SYMTYPE = 19;
pub const or: SYMTYPE = 20;
pub const not: SYMTYPE = 21;
pub const assignsym: SYMTYPE = 22;
pub const apply: SYMTYPE = 23;
pub const resize: SYMTYPE = 24;
pub const vector: SYMTYPE = 25;
pub const statemend: SYMTYPE = 26;
pub const argsep: SYMTYPE = 27;
pub const name: SYMTYPE = 28;
pub const number: SYMTYPE = 29;
pub const string: SYMTYPE = 30;
pub const funcsym: SYMTYPE = 31;
pub const import: SYMTYPE = 32;
pub const export: SYMTYPE = 33;
pub const ifsym: SYMTYPE = 34;
pub const thensym: SYMTYPE = 35;
pub const elsesym: SYMTYPE = 36;
pub const whilesym: SYMTYPE = 37;
pub const forsym: SYMTYPE = 38;
pub const beginsym: SYMTYPE = 39;
pub const endsym: SYMTYPE = 40;
pub const breaksym: SYMTYPE = 41;
pub const comment: SYMTYPE = 42;
pub const systemcall: SYMTYPE = 43;

// ---- TREE / CLAUSE ----------------------------------------------------------
pub type VDataFn = Option<unsafe extern "C" fn() -> *mut MATRIX>;
#[repr(C)]
pub union DataEntry {
    pub s_data: *mut c_char,
    pub d_data: c_double,
    pub c_data: *mut VARIABLE,
    pub v_data: VDataFn,
}
impl Copy for DataEntry {}
impl Clone for DataEntry {
    fn clone(&self) -> Self {
        *self
    }
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct TREEENTRY {
    pub args: *mut TREE,
    pub subs: *mut TREE,
    pub entrytype: c_int,
    pub entrydata: DataEntry,
}
pub const ETYPE_NAME: c_int = 0;
pub const ETYPE_NUMBER: c_int = 1;
pub const ETYPE_STRING: c_int = 2;
pub const ETYPE_OPER: c_int = 3;
pub const ETYPE_CONST: c_int = 4;
pub const ETYPE_EQUAT: c_int = 5;

#[repr(C)]
pub struct TREE {
    pub next: *mut TREE,
    pub link: *mut TREE,
    pub left: *mut TREE,
    pub right: *mut TREE,
    pub tentry: TREEENTRY,
}
const _: () = assert!(size_of::<TREE>() == 64);

#[repr(C)]
pub struct CLAUSE {
    pub link: *mut CLAUSE,
    pub jmp: *mut CLAUSE,
    pub this: *mut TREE,
    pub data: SYMTYPE,
}
const _: () = assert!(size_of::<CLAUSE>() == 32);

// ---- misc --------------------------------------------------------------------
pub const TRUE: c_int = 1;
pub const FALSE: c_int = 0;
pub const STR_MAXVALS: usize = 32;
pub const STR_MAXLEN: usize = 512;
pub const COMMENT_CH: c_char = b'!' as c_char;
pub const SYSTEM_CH: c_char = b'$' as c_char;

// SAFETY: pure arithmetic, mirrors C sign/max/min/abs macros (single eval).
#[inline]
pub fn c_sign(x: c_int) -> c_int {
    if x > 0 {
        1
    } else if x < 0 {
        -1
    } else {
        0
    }
}
#[inline]
pub fn c_sign_d(x: c_double) -> c_int {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

// ---- graphics types (gra.h) -----------------------------------------------------
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Point {
    pub x: c_double,
    pub y: c_double,
    pub z: c_double,
}
pub type GMATRIX = [[c_double; 4]; 4];
#[repr(C)]
#[derive(Copy, Clone)]
pub struct MatcRectangle {
    pub xlow: c_double,
    pub xhigh: c_double,
    pub ylow: c_double,
    pub yhigh: c_double,
}
#[repr(C)]
pub struct GStateWindow {
    pub xlow: c_double,
    pub xhigh: c_double,
    pub ylow: c_double,
    pub yhigh: c_double,
    pub zlow: c_double,
    pub zhigh: c_double,
}
#[repr(C)]
pub struct G_STATE {
    pub out_fp: *mut FILE,
    pub driver: c_int,
    pub window: GStateWindow,
    pub viewport: MatcRectangle,
    pub modelm: GMATRIX,
    pub viewm: GMATRIX,
    pub projm: GMATRIX,
    pub transfm: GMATRIX,
    pub pratio: c_double,
    pub cur_point: Point,
    pub cur_color: c_int,
    pub cur_marker: c_int,
}
const _: () = assert!(size_of::<G_STATE>() == 648);
pub const GRA_FUNCS: usize = 27;

// ---- libc externs (resolved at final link against glibc) --------------------------
// SAFETY: declarations only; each call site upholds the C contract.
extern "C" {
    pub fn calloc(nmemb: size_t, size: size_t) -> *mut core::ffi::c_void;
    pub fn malloc(size: size_t) -> *mut core::ffi::c_void;
    pub fn realloc(ptr: *mut core::ffi::c_void, size: size_t) -> *mut core::ffi::c_void;
    pub fn free(ptr: *mut core::ffi::c_void);
    pub fn strlen(s: *const c_char) -> size_t;
    pub fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char;
    pub fn strcat(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int;
    pub fn strncmp(s1: *const c_char, s2: *const c_char, n: size_t) -> c_int;
    pub fn strchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn strstr(hay: *const c_char, needle: *const c_char) -> *mut c_char;
    pub fn memcpy(dst: *mut core::ffi::c_void, src: *const core::ffi::c_void, n: size_t) -> *mut core::ffi::c_void;
    pub fn memmove(dst: *mut core::ffi::c_void, src: *const core::ffi::c_void, n: size_t) -> *mut core::ffi::c_void;
    pub fn memset(s: *mut core::ffi::c_void, c: c_int, n: size_t) -> *mut core::ffi::c_void;
    pub fn memcmp(s1: *const core::ffi::c_void, s2: *const core::ffi::c_void, n: size_t) -> c_int;
    pub fn printf(fmt: *const c_char, ...) -> c_int;
    pub fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn sprintf(s: *mut c_char, fmt: *const c_char, ...) -> c_int;
    pub fn vsprintf(s: *mut c_char, fmt: *const c_char, ap: core::ffi::VaList) -> c_int;
    pub fn sscanf(s: *const c_char, fmt: *const c_char, ...) -> c_int;
    pub fn fgets(s: *mut c_char, n: c_int, stream: *mut FILE) -> *mut c_char;
    pub fn fputs(s: *const c_char, stream: *mut FILE) -> c_int;
    pub fn fputc(c: c_int, stream: *mut FILE) -> c_int;
    pub fn fgetc(stream: *mut FILE) -> c_int;
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    pub fn freopen(path: *const c_char, mode: *const c_char, stream: *mut FILE) -> *mut FILE;
    pub fn fclose(stream: *mut FILE) -> c_int;
    pub fn fread(ptr: *mut core::ffi::c_void, size: size_t, nmemb: size_t, stream: *mut FILE) -> size_t;
    pub fn fwrite(ptr: *const core::ffi::c_void, size: size_t, nmemb: size_t, stream: *mut FILE) -> size_t;
    pub fn fscanf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn fseek(stream: *mut FILE, off: core::ffi::c_long, whence: c_int) -> c_int;
    pub fn ftell(stream: *mut FILE) -> core::ffi::c_long;
    pub fn fflush(stream: *mut FILE) -> c_int;
    pub fn popen(cmd: *const c_char, mode: *const c_char) -> *mut FILE;
    pub fn pclose(stream: *mut FILE) -> c_int;
    pub fn system(cmd: *const c_char) -> c_int;
    pub fn exit(status: c_int) -> !;
    pub fn signal(signum: c_int, handler: Option<unsafe extern "C" fn(c_int)>) -> Option<unsafe extern "C" fn(c_int)>;
    pub fn setjmp(env: *mut JmpBuf) -> c_int;
    pub fn longjmp(env: *mut JmpBuf, val: c_int) -> !;
    pub fn sin(x: c_double) -> c_double;
    pub fn cos(x: c_double) -> c_double;
    pub fn tan(x: c_double) -> c_double;
    pub fn asin(x: c_double) -> c_double;
    pub fn acos(x: c_double) -> c_double;
    pub fn atan(x: c_double) -> c_double;
    pub fn atan2(y: c_double, x: c_double) -> c_double;
    pub fn exp(x: c_double) -> c_double;
    pub fn log(x: c_double) -> c_double;
    pub fn log10(x: c_double) -> c_double;
    pub fn sqrt(x: c_double) -> c_double;
    pub fn pow(x: c_double, y: c_double) -> c_double;
    pub fn fabs(x: c_double) -> c_double;
    pub fn floor(x: c_double) -> c_double;
    pub fn ceil(x: c_double) -> c_double;
    pub fn fmod(x: c_double, y: c_double) -> c_double;
    pub fn sinh(x: c_double) -> c_double;
    pub fn cosh(x: c_double) -> c_double;
    pub fn tanh(x: c_double) -> c_double;
    pub fn time(t: *mut core::ffi::c_long) -> core::ffi::c_long;
    pub fn getenv(name: *const c_char) -> *mut c_char;
}

// va_list for vsprintf: glibc x86_64 __builtin_va_list = __va_list_tag[1].
#[repr(C)]
#[derive(Copy, Clone)]
pub struct VaListTag {
    pub gp_offset: core::ffi::c_uint,
    pub fp_offset: core::ffi::c_uint,
    pub overflow_arg_area: *mut core::ffi::c_void,
    pub reg_save_area: *mut core::ffi::c_void,
}

// ---- unit body: transcribed from matc/src/jacobi.c ----
// Transcribed from matc/src/jacobi.c (generalized Jacobi eigensolver).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn PrintOut(fmt: *const c_char, ...);
    fn var_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
}

// SAFETY: mirrors C matc_jacobi on live caller arrays (n*n a/b/x, n eigv/d).
// The check<=0 path prints via C printf exactly like the original.
#[no_mangle]
pub unsafe extern "C" fn matc_jacobi(
    a: *mut c_double,
    b: *mut c_double,
    x: *mut c_double,
    eigv: *mut c_double,
    d: *mut c_double,
    n: c_int,
    rtol: c_double,
) -> c_int {
    // SAFETY: caller arrays; identical indexing to the C original.
    let aget = |i: c_int, j: c_int| -> c_double { *a.offset((i * n + j) as isize) };
    let aset = |i: c_int, j: c_int, v: c_double| {
        *a.offset((i * n + j) as isize) = v;
    };
    let bget = |i: c_int, j: c_int| -> c_double { *b.offset((i * n + j) as isize) };
    let bset = |i: c_int, j: c_int, v: c_double| {
        *b.offset((i * n + j) as isize) = v;
    };
    let xget = |i: c_int, j: c_int| -> c_double { *x.offset((i * n + j) as isize) };
    let xset = |i: c_int, j: c_int, v: c_double| {
        *x.offset((i * n + j) as isize) = v;
    };
    // C abs macro on doubles: manual ternary, same value.
    let dabs = |v: c_double| -> c_double { if v > 0.0 { v } else { -v } };
    let nsmax: c_int = 50;
    let mut i: c_int = 0;
    while i < n {
        let _ii = i * n + i;
        if aget(i, i) <= 0.0 || bget(i, i) <= 0.0 {
            return 0;
        }
        let r = aget(i, i) / bget(i, i);
        *eigv.offset(i as isize) = r;
        *d.offset(i as isize) = r;
        xset(i, i, 1.0);
        i += 1;
    }
    if n == 1 {
        return 1;
    }
    let nr = n - 1;
    let mut nsweep: c_int = 0;
    while nsweep < nsmax {
        let eps = pow(0.01, 2.0 * ((nsweep + 1) as c_double));
        let mut j: c_int = 0;
        while j < nr {
            let jj = j + 1;
            let mut k = jj;
            while k < n {
                let eptola = aget(j, k) * aget(j, k) / (aget(j, j) * aget(k, k));
                let eptolb = bget(j, k) * bget(j, k) / (bget(j, j) * bget(k, k));
                if eptola >= eps || eptolb >= eps {
                    let akk = aget(k, k) * bget(j, k) - bget(k, k) * aget(j, k);
                    let ajj = aget(j, j) * bget(j, k) - bget(j, j) * aget(j, k);
                    let ab = aget(j, j) * bget(k, k) - aget(k, k) * bget(j, j);
                    let check = (ab * ab + 4.0 * akk * ajj) / 4.0;
                    if check <= 0.0 {
                        // SAFETY: static formats; check passed as double.
                        printf(b"***Error   solution stop in *jacobi*\n\0".as_ptr() as *const c_char);
                        printf(b"        check = %20.14e\n\0".as_ptr() as *const c_char, check);
                        return 1;
                    }
                    let sqch = sqrt(check);
                    let d1 = ab / 2.0 + sqch;
                    let d2 = ab / 2.0 - sqch;
                    let mut den = d1;
                    if dabs(d2) > dabs(d1) {
                        den = d2;
                    }
                    let (ca, cg) = if den == 0.0 {
                        (0.0, -aget(j, k) / aget(k, k))
                    } else {
                        (akk / den, -ajj / den)
                    };
                    if n != 2 {
                        let jp1 = j + 1;
                        let jm1 = j - 1;
                        let kp1 = k + 1;
                        let km1 = k - 1;
                        if jm1 >= 0 {
                            let mut i = 0;
                            while i <= jm1 {
                                let aj = aget(i, j);
                                let bj = bget(i, j);
                                let ak = aget(i, k);
                                let bk = bget(i, k);
                                aset(i, j, aj + cg * ak);
                                bset(i, j, bj + cg * bk);
                                aset(i, k, ak + ca * aj);
                                bset(i, k, bk + ca * bj);
                                i += 1;
                            }
                        }
                        if kp1 - n + 1 <= 0 {
                            let mut i = kp1;
                            while i < n {
                                let aj = aget(j, i);
                                let bj = bget(j, i);
                                let ak = aget(k, i);
                                let bk = bget(k, i);
                                aset(j, i, aj + cg * ak);
                                bset(j, i, bj + cg * bk);
                                aset(k, i, ak + ca * aj);
                                bset(k, i, bk + ca * bj);
                                i += 1;
                            }
                        }
                        if jp1 - km1 <= 0 {
                            let mut i = jp1;
                            while i <= km1 {
                                let aj = aget(j, i);
                                let bj = bget(j, i);
                                let ak = aget(i, k);
                                let bk = bget(i, k);
                                aset(j, i, aj + cg * ak);
                                bset(j, i, bj + cg * bk);
                                aset(i, k, ak + ca * aj);
                                bset(i, k, bk + ca * bj);
                                i += 1;
                            }
                        }
                    }
                    let ak = aget(k, k);
                    let bk = bget(k, k);
                    aset(k, k, ak + 2.0 * ca * aget(j, k) + ca * ca * aget(j, j));
                    bset(k, k, bk + 2.0 * ca * bget(j, k) + ca * ca * bget(j, j));
                    aset(j, j, aget(j, j) + 2.0 * cg * aget(j, k) + cg * cg * ak);
                    bset(j, j, bget(j, j) + 2.0 * cg * bget(j, k) + cg * cg * bk);
                    aset(j, k, 0.0);
                    bset(j, k, 0.0);
                    let mut i = 0;
                    while i < n {
                        let xj = xget(i, j);
                        let xk = xget(i, k);
                        xset(i, j, xj + cg * xk);
                        xset(i, k, xk + ca * xj);
                        i += 1;
                    }
                }
                k += 1;
            }
            j += 1;
        }
        let mut i: c_int = 0;
        while i < n {
            if aget(i, i) <= 0.0 || bget(i, i) <= 0.0 {
                // SAFETY: format string has no holes; single static call.
                error_matc(
                    b"*** Error  solution stop in *jacobi*\n Matrix not positive definite.\0".as_ptr()
                        as *const c_char,
                );
            }
            *eigv.offset(i as isize) = aget(i, i) / bget(i, i);
            i += 1;
        }
        let mut convergence: c_int = 1;
        let mut i: c_int = 0;
        while i < n {
            let tol = rtol * *d.offset(i as isize);
            let dif = dabs(*eigv.offset(i as isize) - *d.offset(i as isize));
            if dif > tol {
                convergence = 0;
            }
            if convergence == 0 {
                break;
            }
            i += 1;
        }
        if convergence != 0 {
            let eps = rtol * rtol;
            let mut j: c_int = 0;
            while j < nr {
                let jj = j + 1;
                let mut k = jj;
                while k < n {
                    let epsa = aget(j, k) * aget(j, k) / (aget(j, j) * aget(k, k));
                    let epsb = bget(j, k) * bget(j, k) / (bget(j, j) * bget(k, k));
                    if epsa >= eps || epsb >= eps {
                        convergence = 0;
                    }
                    if convergence == 0 {
                        break;
                    }
                    k += 1;
                }
                if convergence == 0 {
                    break;
                }
                j += 1;
            }
        }
        if convergence == 0 {
            let mut i: c_int = 0;
            while i < n {
                *d.offset(i as isize) = *eigv.offset(i as isize);
                i += 1;
            }
        }
        if convergence != 0 {
            break;
        }
        nsweep += 1;
    }
    let mut i: c_int = 0;
    while i < n {
        let mut j = i;
        while j < n {
            bset(j, i, bget(i, j));
            aset(j, i, aget(i, j));
            j += 1;
        }
        i += 1;
    }
    let mut j: c_int = 0;
    while j < n {
        let bb = sqrt(bget(j, j));
        let mut k: c_int = 0;
        while k < n {
            xset(k, j, xget(k, j) / bb);
            k += 1;
        }
        j += 1;
    }
    // SAFETY: static format; nsweep passed as int.
    PrintOut(b"jacobi: nsweeps %d\n\0".as_ptr() as *const c_char, nsweep);
    1
}

// SAFETY: mirrors C mtr_jacob; a is the evaluated argument chain
// (a, b, rtol); creates a global "eigv" exactly like the original.
#[no_mangle]
pub unsafe extern "C" fn mtr_jacob(a: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument chain cells are live temps.
    if var_nrow(a) != var_ncol(a) {
        // SAFETY: static format string.
        error_matc(b"Jacob: Matrix must be square.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: NEXT cells live through the call.
    let bvar = (*a).next;
    let b = var_matr(bvar);
    let n = var_nrow(a);
    if var_nrow(bvar) != var_ncol(bvar) || n != var_nrow(bvar) {
        // SAFETY: static format string.
        error_matc(b"Jacob: Matrix dimensions incompatible.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: rtol cell live.
    let rtol = *var_matr((*bvar).next);
    // SAFETY: var_new links a live global; ALLOCMEM/FREEMEM pair below.
    let x = var_new(b"eigv\0".as_ptr() as *mut c_char, TYPE_DOUBLE, var_nrow(a), var_ncol(a));
    // SAFETY: C-heap work vector, freed below.
    let d = mem_alloc((n as usize) * size_of::<c_double>()) as *mut c_double;
    // SAFETY: fresh temp owns its storage.
    let ev = var_temp_new(TYPE_DOUBLE, 1, n);
    // SAFETY: all arrays live for the call (a/b from chain, x/ev fresh, d above).
    let _ = matc_jacobi(var_matr(a), b, var_matr(x), var_matr(ev), d, n, rtol);
    // SAFETY: d from mem_alloc above.
    mem_free(d as *mut core::ffi::c_void);
    ev
}
