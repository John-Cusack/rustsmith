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

// ---- unit body: transcribed from matc/src/eig.c ----
// Transcribed from matc/src/eig.c (hesse/francis QR eigensolver).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn var_temp_copy(v: *mut VARIABLE) -> *mut VARIABLE;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_delete_temp(v: *mut VARIABLE);
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
}

const MAXITER: c_int = 1000;
const EPS: c_double = 1e-16;

// SAFETY: mirrors C mtr_hesse; var is the evaluated argument.
#[no_mangle]
pub unsafe extern "C" fn mtr_hesse(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var matrix live.
    if var_ncol(var) != var_nrow(var) {
        // SAFETY: static format; two int args.
        error_matc(
            b"hesse: matrix must be square, current dimensions: [%d,%d]\n\0".as_ptr() as *const c_char,
            var_nrow(var),
            var_ncol(var),
        );
    }
    // SAFETY: temp copy owns its storage.
    let res = var_temp_copy(var);
    // SAFETY: res data live through return.
    let a = var_matr(res);
    let n = var_nrow(res);
    if var_nrow(res) == 1 {
        return res;
    }
    hesse(a, n, n);
    res
}

// SAFETY: mirrors C mtr_eig (Hessenberg + Francis QR, complex pairs split
// into adjacent real/imag rows exactly like the original).
#[no_mangle]
pub unsafe extern "C" fn mtr_eig(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var matrix live.
    if var_ncol(var) != var_nrow(var) {
        // SAFETY: static format; two int args.
        error_matc(
            b"eig: matrix must be square, current dimensions: [%d,%d]\n\0".as_ptr() as *const c_char,
            var_nrow(var),
            var_ncol(var),
        );
    }
    // SAFETY: temp copy owns its storage until var_delete_temp below.
    let ptr = var_temp_copy(var);
    // SAFETY: ptr data live until freed below.
    let a = var_matr(ptr);
    let n = var_nrow(ptr);
    if var_nrow(ptr) == 1 {
        return ptr;
    }
    hesse(a, n, n);
    // C abs macro on doubles: manual ternary, same value.
    let dabs = |v: c_double| -> c_double { if v > 0.0 { v } else { -v } };
    let at = |i: c_int, j: c_int| -> c_double { *a.offset((n * i + j) as isize) };
    let set = |i: c_int, j: c_int, v: c_double| {
        *a.offset((n * i + j) as isize) = v;
    };
    let mut iter: c_int = 0;
    while iter < MAXITER {
        let mut i: c_int = 0;
        while i < n - 1 {
            let s = EPS * (dabs(at(i, i)) + dabs(at(i + 1, i + 1)));
            if dabs(at(i + 1, i)) < s {
                set(i + 1, i, 0.0);
            }
            i += 1;
        }
        let mut i: c_int = 0;
        let mut j: c_int;
        let mut k: c_int;
        loop {
            let mut jj = i;
            while jj < n - 1 {
                if at(jj + 1, jj) != 0.0 {
                    break;
                }
                jj += 1;
            }
            j = jj;
            let mut kk = j;
            while kk < n - 1 {
                if at(kk + 1, kk) == 0.0 {
                    break;
                }
                kk += 1;
            }
            k = kk;
            i = k;
            if !(i < n - 1 && k - j + 1 < 3) {
                break;
            }
        }
        if k - j + 1 < 3 {
            break;
        }
        francis(&mut *a.offset((n * j + j) as isize), k - j + 1, n);
        iter += 1;
    }
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(TYPE_DOUBLE, n, 2);
    let mut i: c_int = 0;
    let mut j: c_int = 0;
    while i < n - 1 {
        if at(i + 1, i) == 0.0 {
            var_set_m(res, j, 0, at(i, i));
            j += 1;
        } else {
            let b = at(i, i) + at(i + 1, i + 1);
            let s0 = b * b;
            let t = at(i, i) * at(i + 1, i + 1) - at(i, i + 1) * at(i + 1, i);
            let s = s0 - 4.0 * t;
            if s < 0.0 {
                var_set_m(res, j, 0, b / 2.0);
                var_set_m(res, j, 1, sqrt(-s) / 2.0);
                j += 1;
                var_set_m(res, j, 0, b / 2.0);
                var_set_m(res, j, 1, -sqrt(-s) / 2.0);
                j += 1;
            } else {
                var_set_m(res, j, 0, b / 2.0 + sqrt(s) / 2.0);
                j += 1;
                var_set_m(res, j, 0, b / 2.0 - sqrt(s) / 2.0);
                j += 1;
            }
            i += 1;
        }
        i += 1;
    }
    // NOTE: the C loop above always runs i to n-1, then this tail handles
    // the last row; mirrored exactly (the i++ inside the else pairs with
    // the loop i++ to skip the consumed pair).
    if at(n - 1, n - 2) == 0.0 {
        var_set_m(res, j, 0, at(n - 1, n - 1));
    }
    // SAFETY: ptr live temp from above.
    var_delete_temp(ptr);
    res
}

// SAFETY: mirrors C vbcalc on live caller arrays.
#[no_mangle]
pub unsafe extern "C" fn vbcalc(
    x: *mut c_double,
    v: *mut c_double,
    b: *mut c_double,
    beg: c_int,
    end: c_int,
) {
    // SAFETY: caller arrays over [beg..=end].
    let xat = |i: c_int| -> c_double { *x.offset(i as isize) };
    let vset = |i: c_int, val: c_double| {
        *v.offset(i as isize) = val;
    };
    let dabs = |val: c_double| -> c_double { if val > 0.0 { val } else { -val } };
    let mut m = dabs(xat(beg));
    let mut i = beg + 1;
    while i <= end {
        // C max macro on pure args: manual ternary, same value.
        let ax = dabs(xat(i));
        m = if m > ax { m } else { ax };
        i += 1;
    }
    if m < EPS {
        // SAFETY: zeroing (end-beg+1) doubles at v[beg] (C memset <<3).
        memset(
            v.offset(beg as isize) as *mut core::ffi::c_void,
            0,
            ((end - beg + 1) as usize) << 3,
        );
    } else {
        let mut alpha = 0.0;
        let mp1 = 1.0 / m;
        let mut i = beg;
        while i <= end {
            vset(i, xat(i) * mp1);
            alpha = alpha + *v.offset(i as isize) * *v.offset(i as isize);
            i += 1;
        }
        alpha = sqrt(alpha);
        *b = 1.0 / (alpha * (alpha + dabs(*v.offset(beg as isize))));
        vset(beg, *v.offset(beg as isize) + c_sign_d(*v.offset(beg as isize)) as c_double * alpha);
    }
}

// SAFETY: mirrors C hesse (in-place Hessenberg reduction on DIM*DIM h;
// N is the row stride). ALLOCMEM/FREEMEM-paired work vectors.
#[no_mangle]
pub unsafe extern "C" fn hesse(h: *mut c_double, dim: c_int, n: c_int) {
    // SAFETY: h spans dim*n doubles (square: dim==n in all callers).
    let hget = |i: c_int, j: c_int| -> c_double { *h.offset((i * n + j) as isize) };
    let hset = |i: c_int, j: c_int, val: c_double| {
        *h.offset((i * n + j) as isize) = val;
    };
    // SAFETY: C-heap work vectors, freed at the end.
    let x = mem_alloc((dim as usize) * size_of::<c_double>()) as *mut c_double;
    let v = mem_alloc((dim as usize) * size_of::<c_double>()) as *mut c_double;
    let mut i: c_int = 0;
    while i < dim - 2 {
        let mut j = i + 1;
        while j < dim {
            *x.offset(j as isize) = hget(j, i);
            j += 1;
        }
        let mut b: c_double = 0.0;
        vbcalc(x, v, &mut b, i + 1, dim - 1);
        if *v.offset((i + 1) as isize) == 0.0 {
            break;
        }
        let mut j = i + 2;
        while j < dim {
            *x.offset(j as isize) = *v.offset(j as isize) / *v.offset((i + 1) as isize);
            *v.offset(j as isize) = b * *v.offset((i + 1) as isize) * *v.offset(j as isize);
            j += 1;
        }
        *v.offset((i + 1) as isize) = b * *v.offset((i + 1) as isize) * *v.offset((i + 1) as isize);
        let mut j: c_int = 0;
        while j < dim {
            let mut s = 0.0;
            let mut k = i + 1;
            while k < dim {
                s = s + hget(j, k) * *v.offset(k as isize);
                k += 1;
            }
            hset(j, i + 1, hget(j, i + 1) - s);
            let mut k = i + 2;
            while k < dim {
                hset(j, k, hget(j, k) - s * *x.offset(k as isize));
                k += 1;
            }
            j += 1;
        }
        let mut j: c_int = 0;
        while j < dim {
            let mut s = hget(i + 1, j);
            let mut k = i + 2;
            while k < dim {
                s = s + hget(k, j) * *x.offset(k as isize);
                k += 1;
            }
            let mut k = i + 1;
            while k < dim {
                hset(k, j, hget(k, j) - s * *v.offset(k as isize));
                k += 1;
            }
            j += 1;
        }
        let mut j = i + 2;
        while j < dim {
            hset(j, i, 0.0);
            j += 1;
        }
        i += 1;
    }
    // SAFETY: x/v from mem_alloc above.
    mem_free(x as *mut core::ffi::c_void);
    mem_free(v as *mut core::ffi::c_void);
}

// SAFETY: mirrors C francis (implicit QR step on DIM*DIM h, stride N).
#[no_mangle]
pub unsafe extern "C" fn francis(h: *mut c_double, dim: c_int, n: c_int) {
    // SAFETY: h spans dim*n doubles.
    let hget = |i: c_int, j: c_int| -> c_double { *h.offset((i * n + j) as isize) };
    let hset = |i: c_int, j: c_int, val: c_double| {
        *h.offset((i * n + j) as isize) = val;
    };
    let nn = dim - 1;
    let m = nn - 1;
    let t = hget(m, m) * hget(nn, nn) - hget(m, nn) * hget(nn, m);
    let s = hget(m, m) + hget(nn, nn);
    let mut x = [0.0 as c_double; 3];
    let mut v = [0.0 as c_double; 3];
    x[0] = hget(0, 0) * hget(0, 0) + hget(0, 1) * hget(1, 0) - s * hget(0, 0) + t;
    x[1] = hget(1, 0) * (hget(0, 0) + hget(1, 1) - s);
    x[2] = hget(1, 0) * hget(2, 1);
    let mut b: c_double = 0.0;
    vbcalc(x.as_mut_ptr(), v.as_mut_ptr(), &mut b, 0, 2);
    if v[0] == 0.0 {
        return;
    }
    let bv = b * v[0];
    x[1] = v[1] / v[0];
    v[1] = bv * v[1];
    x[2] = v[2] / v[0];
    v[2] = bv * v[2];
    x[0] = 1.0;
    v[0] = b * v[0] * v[0];
    let mut i: c_int = 0;
    while i < dim {
        let i1 = i * n;
        let s = *h.offset(i1 as isize) * v[0] + *h.offset((i1 + 1) as isize) * v[1]
            + *h.offset((i1 + 2) as isize) * v[2];
        hset(i, 0, hget(i, 0) - s);
        *h.offset((i1 + 1) as isize) = *h.offset((i1 + 1) as isize) - s * x[1];
        *h.offset((i1 + 2) as isize) = *h.offset((i1 + 2) as isize) - s * x[2];
        i += 1;
    }
    let mut i: c_int = 0;
    while i < dim {
        let s = *h.offset(i as isize) + *h.offset((n + i) as isize) * x[1]
            + *h.offset(((n << 1) + i) as isize) * x[2];
        *h.offset(i as isize) = *h.offset(i as isize) - s * v[0];
        *h.offset((n + i) as isize) = *h.offset((n + i) as isize) - s * v[1];
        *h.offset(((n << 1) + i) as isize) = *h.offset(((n << 1) + i) as isize) - s * v[2];
        i += 1;
    }
    let mut i: c_int = 0;
    while i < dim - 2 {
        // C min macro on pure args: manual ternary, same value.
        let end = if 2 < dim - i - 2 { 2 } else { dim - i - 2 };
        let mut j: c_int = 0;
        while j <= end {
            x[j as usize] = hget(i + j + 1, i);
            j += 1;
        }
        vbcalc(x.as_mut_ptr(), v.as_mut_ptr(), &mut b, 0, end);
        if v[0] == 0.0 {
            break;
        }
        let mut j: c_int = 1;
        while j <= end {
            x[j as usize] = v[j as usize] / v[0];
            v[j as usize] = b * v[0] * v[j as usize];
            j += 1;
        }
        x[0] = 1.0;
        v[0] = b * v[0] * v[0];
        let mut j: c_int = 0;
        while j < dim {
            let mut s = 0.0;
            let mut k: c_int = 0;
            while k <= end {
                s = s + hget(j, i + k + 1) * v[k as usize];
                k += 1;
            }
            hset(j, i + 1, hget(j, i + 1) - s);
            let mut k: c_int = 1;
            while k <= end {
                hset(j, i + k + 1, hget(j, i + k + 1) - s * x[k as usize]);
                k += 1;
            }
            j += 1;
        }
        let mut j: c_int = 0;
        while j < dim {
            let mut s = hget(i + 1, j);
            let mut k: c_int = 1;
            while k <= end {
                s = s + hget(i + k + 1, j) * x[k as usize];
                k += 1;
            }
            let mut k: c_int = 0;
            while k <= end {
                hset(i + k + 1, j, hget(i + k + 1, j) - s * v[k as usize]);
                k += 1;
            }
            j += 1;
        }
        let mut j = i + 2;
        while j < dim {
            hset(j, i, 0.0);
            j += 1;
        }
        i += 1;
    }
}
