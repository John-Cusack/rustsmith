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

// ---- unit body: transcribed from matc/src/matrix.c ----
// Transcribed from matc/src/matrix.c (math builtins + matrix commands).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn urand(iy: *mut c_int) -> c_double;
    fn mtr_det(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mtr_inv(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mtr_eig(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mtr_jacob(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mtr_LUD(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mtr_hesse(v: *mut VARIABLE) -> *mut VARIABLE;
    fn com_init(
        nm: *const c_char,
        pw: c_int,
        ce: c_int,
        sub: CommandSub,
        minp: c_int,
        maxp: c_int,
        help: *const c_char,
    );
}

// SAFETY: mirrors C func_abs (abs macro on a double: manual ternary).
#[no_mangle]
pub unsafe extern "C" fn func_abs(arg: c_double) -> c_double {
    if arg > 0.0 {
        arg
    } else {
        -arg
    }
}

// SAFETY: mirrors C func_mod (round-half toward +inf via truncation, then
// C int remainder; values in range like the original).
#[no_mangle]
pub unsafe extern "C" fn func_mod(x: c_double, y: c_double) -> c_double {
    let ix = (x + 0.5) as c_int;
    let iy = (y + 0.5) as c_int;
    (ix % iy) as c_double
}

// SAFETY: mirrors C mtr_sum (vector sum or column sums; temp is calloc'd
// zero like the original, so += accumulates from 0).
#[no_mangle]
pub unsafe extern "C" fn mtr_sum(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let mut a = var_matr(aa);
    if nrowa == 1 || ncola == 1 {
        // SAFETY: fresh temp owns its storage (zeroed).
        let cc = var_temp_new(TYPE_DOUBLE, 1, 1);
        // SAFETY: cc data live through return.
        let c = var_matr(cc);
        let n = if nrowa == 1 { ncola } else { nrowa };
        let mut i: c_int = 0;
        while i < n {
            *c += *a;
            a = a.offset(1);
            i += 1;
        }
        cc
    } else {
        // SAFETY: fresh temp owns its storage (zeroed).
        let cc = var_temp_new(TYPE_DOUBLE, 1, ncola);
        // SAFETY: cc data live through return.
        let c = var_matr(cc);
        let mut i: c_int = 0;
        while i < ncola {
            let mut j: c_int = 0;
            while j < nrowa {
                *c.offset(i as isize) += *a.offset((j * ncola + i) as isize);
                j += 1;
            }
            i += 1;
        }
        cc
    }
}

// SAFETY: mirrors C mtr_trace.
#[no_mangle]
pub unsafe extern "C" fn mtr_trace(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let a = var_matr(aa);
    if nrowa != ncola {
        // SAFETY: static format string.
        error_matc(b"trace: not square.\n\0".as_ptr() as *const c_char);
    }
    let mut temp = 0.0;
    let mut i: c_int = 0;
    while i < nrowa {
        temp += *a.offset((i * ncola + i) as isize);
        i += 1;
    }
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(var_type(aa), 1, 1);
    // SAFETY: cc data live through return.
    *var_matr(cc) = temp;
    cc
}

// SAFETY: mirrors C mtr_zeros (1- or 2-arg size form; calloc zero).
#[no_mangle]
pub unsafe extern "C" fn mtr_zeros(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let (ind1, ind2) = if !(*aa).next.is_null() {
        (*var_matr(aa) as c_int, *var_matr((*aa).next) as c_int)
    } else {
        (1, *var_matr(aa) as c_int)
    };
    if ind1 < 1 || ind2 < 1 {
        // SAFETY: static string (original lacks trailing newline).
        error_matc(b"Zeros: invalid size for and array\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh temp owns its storage.
    var_temp_new(TYPE_DOUBLE, ind1, ind2)
}

// SAFETY: mirrors C mtr_ones (zeros then fill 1.0).
#[no_mangle]
pub unsafe extern "C" fn mtr_ones(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: mtr_zeros returns a live temp.
    let cc = mtr_zeros(aa);
    // SAFETY: cc data live through return.
    let mut c = var_matr(cc);
    let n = var_nrow(cc) * var_ncol(cc);
    let mut i: c_int = 0;
    while i < n {
        *c = 1.0;
        c = c.offset(1);
        i += 1;
    }
    cc
}

static mut mtr_rand_seed: c_int = 0;

// SAFETY: mirrors C mtr_rand (session-persistent seed, time-seeded once).
#[no_mangle]
pub unsafe extern "C" fn mtr_rand(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: mtr_zeros returns a live temp.
    let cc = mtr_zeros(aa);
    // SAFETY: cc data live through return.
    let mut c = var_matr(cc);
    let n = var_nrow(cc) * var_ncol(cc);
    // SAFETY: file static mirrors the C original.
    if mtr_rand_seed == 0 {
        mtr_rand_seed = time(core::ptr::null_mut()) as c_int;
    }
    let mut i: c_int = 0;
    while i < n {
        *c = urand(core::ptr::addr_of_mut!(mtr_rand_seed));
        c = c.offset(1);
        i += 1;
    }
    cc
}

// SAFETY: mirrors C mtr_resize (2- or 3-arg form, cyclic fill).
#[no_mangle]
pub unsafe extern "C" fn mtr_resize(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let (ind1, ind2) = if !(*(*aa).next).next.is_null() {
        (*var_matr((*aa).next) as c_int, *var_matr((*(*aa).next).next) as c_int)
    } else {
        (1, *var_matr((*aa).next) as c_int)
    };
    if ind1 < 1 || ind2 < 1 {
        // SAFETY: static string (original lacks trailing newline).
        error_matc(b"resize: invalid size for and array\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(var_type(aa), ind1, ind2);
    // SAFETY: both data blocks live through the call.
    let mut c = var_matr(cc);
    let a = var_matr(aa);
    let n = ind1 * ind2;
    let m = var_nrow(aa) * var_ncol(aa);
    let mut j: c_int = 0;
    let mut i: c_int = 0;
    while i < n {
        *c = *a.offset(j as isize);
        c = c.offset(1);
        j += 1;
        if j == m {
            j = 0;
        }
        i += 1;
    }
    cc
}

// SAFETY: mirrors C mtr_vector (start/stop/[incr] range).
#[no_mangle]
pub unsafe extern "C" fn mtr_vector(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let start = *var_matr(aa);
    let stop = *var_matr((*aa).next);
    let mut incr = if !(*(*aa).next).next.is_null() {
        *var_matr((*(*aa).next).next)
    } else if start < stop {
        1.0
    } else {
        -1.0
    };
    if incr == 0.0 {
        incr = if start < stop { 1.0 } else { -1.0 };
    }
    // C abs macro on doubles: manual ternary, same value.
    let dabs = |v: c_double| -> c_double { if v > 0.0 { v } else { -v } };
    let eval = (dabs(stop - start) / dabs(incr)) as c_int + 1;
    if eval < 1 {
        return core::ptr::null_mut();
    }
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(TYPE_DOUBLE, 1, eval);
    // SAFETY: cc data live through return.
    let mut c = var_matr(cc);
    let mut x = start;
    let mut i: c_int = 0;
    while i < eval {
        *c = x;
        c = c.offset(1);
        x += incr;
        i += 1;
    }
    cc
}

// SAFETY: mirrors C mtr_eye.
#[no_mangle]
pub unsafe extern "C" fn mtr_eye(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument data live.
    if *var_matr(aa) < 1.0 {
        // SAFETY: static format string.
        error_matc(b"eye: Invalid size for an array.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: argument data live.
    let ind = *var_matr(aa) as c_int;
    // SAFETY: fresh temp owns its storage (zeroed).
    let cc = var_temp_new(TYPE_DOUBLE, ind, ind);
    // SAFETY: cc data live through return.
    let c = var_matr(cc);
    let mut i: c_int = 0;
    while i < ind {
        *c.offset((i * ind + i) as isize) = 1.0;
        i += 1;
    }
    cc
}

// SAFETY: mirrors C mtr_size ([nrow ncol] vector).
#[no_mangle]
pub unsafe extern "C" fn mtr_size(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(TYPE_DOUBLE, 1, 2);
    // SAFETY: cc data live through return.
    let c = var_matr(cc);
    // SAFETY: argument dims live.
    *c.offset(0) = var_nrow(aa) as c_double;
    *c.offset(1) = var_ncol(aa) as c_double;
    cc
}

// SAFETY: mirrors C mtr_min (scalar or column minima; C min macro on pure
// args becomes a ternary with the same value).
#[no_mangle]
pub unsafe extern "C" fn mtr_min(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let mut a = var_matr(aa);
    if nrowa == 1 || ncola == 1 {
        // SAFETY: fresh temp owns its storage.
        let cc = var_temp_new(TYPE_DOUBLE, 1, 1);
        // SAFETY: cc data live through return.
        let c = var_matr(cc);
        *c = *a;
        a = a.offset(1);
        let n = if ncola > nrowa { ncola } else { nrowa };
        let mut i: c_int = 1;
        while i < n {
            *c = if *c < *a { *c } else { *a };
            a = a.offset(1);
            i += 1;
        }
        cc
    } else {
        // SAFETY: fresh temp owns its storage.
        let cc = var_temp_new(TYPE_DOUBLE, 1, ncola);
        // SAFETY: cc data live through return.
        let mut c = var_matr(cc);
        let mut i: c_int = 0;
        while i < ncola {
            *c = *a.offset((0 * ncola + i) as isize);
            let mut j: c_int = 1;
            while j < nrowa {
                let v = *a.offset((j * ncola + i) as isize);
                *c = if *c < v { *c } else { v };
                j += 1;
            }
            c = c.offset(1);
            i += 1;
        }
        cc
    }
}

// SAFETY: mirrors C mtr_max (scalar or column maxima).
#[no_mangle]
pub unsafe extern "C" fn mtr_max(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let mut a = var_matr(aa);
    if nrowa == 1 || ncola == 1 {
        // SAFETY: fresh temp owns its storage.
        let cc = var_temp_new(TYPE_DOUBLE, 1, 1);
        // SAFETY: cc data live through return.
        let c = var_matr(cc);
        *c = *a;
        a = a.offset(1);
        let n = if ncola > nrowa { ncola } else { nrowa };
        let mut i: c_int = 1;
        while i < n {
            *c = if *c > *a { *c } else { *a };
            a = a.offset(1);
            i += 1;
        }
        cc
    } else {
        // SAFETY: fresh temp owns its storage.
        let cc = var_temp_new(TYPE_DOUBLE, 1, ncola);
        // SAFETY: cc data live through return.
        let mut c = var_matr(cc);
        let mut i: c_int = 0;
        while i < ncola {
            *c = *a.offset((0 * ncola + i) as isize);
            let mut j: c_int = 1;
            while j < nrowa {
                let v = *a.offset((j * ncola + i) as isize);
                *c = if *c > v { *c } else { v };
                j += 1;
            }
            c = c.offset(1);
            i += 1;
        }
        cc
    }
}

// SAFETY: mirrors C mtr_diag (both directions; C min/max macros on pure
// args become ternaries with the same values).
#[no_mangle]
pub unsafe extern "C" fn mtr_diag(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let mut a = var_matr(aa);
    if nrowa == 1 || ncola == 1 {
        let n = if nrowa > ncola { nrowa } else { ncola };
        // SAFETY: fresh temp owns its storage (zeroed).
        let cc = var_temp_new(TYPE_DOUBLE, n, n);
        // SAFETY: cc data live through return.
        let c = var_matr(cc);
        let mut i: c_int = 0;
        while i < n {
            *c.offset((i * n + i) as isize) = *a;
            a = a.offset(1);
            i += 1;
        }
        cc
    } else {
        // SAFETY: fresh temp owns its storage.
        let cc = var_temp_new(TYPE_DOUBLE, 1, nrowa);
        // SAFETY: cc data live through return.
        let mut c = var_matr(cc);
        let m = if nrowa < ncola { nrowa } else { ncola };
        let mut i: c_int = 0;
        while i < m {
            *c = *a.offset((i * ncola + i) as isize);
            c = c.offset(1);
            i += 1;
        }
        cc
    }
}

// SAFETY: mirrors C mtr_pow (elementwise power).
#[no_mangle]
pub unsafe extern "C" fn mtr_pow(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: chain cells live.
    let bb = (*aa).next;
    // SAFETY: both data blocks live through the call.
    let mut a = var_matr(aa);
    let b = *var_matr(bb);
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(TYPE_DOUBLE, nrowa, ncola);
    // SAFETY: cc data live through return.
    let mut c = var_matr(cc);
    let mut i: c_int = 0;
    while i < nrowa * ncola {
        *c = pow(*a, b);
        c = c.offset(1);
        a = a.offset(1);
        i += 1;
    }
    cc
}

// SAFETY: mirrors C mtr_where (linear indices of nonzero cells).
#[no_mangle]
pub unsafe extern "C" fn mtr_where(aa: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let nrowa = var_nrow(aa);
    let ncola = var_ncol(aa);
    // SAFETY: argument data live through the call.
    let a = var_matr(aa);
    let mut n: c_int = 0;
    let mut i: c_int = 0;
    while i < nrowa * ncola {
        if *a.offset(i as isize) != 0.0 {
            n += 1;
        }
        i += 1;
    }
    // SAFETY: fresh temp owns its storage.
    let cc = var_temp_new(TYPE_DOUBLE, 1, n);
    // SAFETY: cc data live through return.
    let mut c = var_matr(cc);
    let mut i: c_int = 0;
    while i < nrowa * ncola {
        if *a.offset(i as isize) != 0.0 {
            *c = i as c_double;
            c = c.offset(1);
        }
        i += 1;
    }
    cc
}

// SAFETY: libm fns stored as pointwise command subs, exactly like the C
// (VARIABLE*(*)()) casts; com_pointw calls them per element.
unsafe fn libm_sub(f: unsafe extern "C" fn(c_double) -> c_double) -> CommandSub {
    // SAFETY: same address-preserving conversion the C cast performs; the
    // slot is only invoked through com_pointw with one double in/out.
    Some(core::mem::transmute::<
        unsafe extern "C" fn(c_double) -> c_double,
        unsafe extern "C" fn(*mut VARIABLE) -> *mut VARIABLE,
    >(f))
}

// SAFETY: 2-double-arg libm-style fns stored as command subs (mod path).
unsafe fn libm_sub2(f: unsafe extern "C" fn(c_double, c_double) -> c_double) -> CommandSub {
    // SAFETY: same address-preserving conversion as libm_sub.
    Some(core::mem::transmute::<
        unsafe extern "C" fn(c_double, c_double) -> c_double,
        unsafe extern "C" fn(*mut VARIABLE) -> *mut VARIABLE,
    >(f))
}

// SAFETY: registers math commands with byte-identical help texts.
#[no_mangle]
pub unsafe extern "C" fn mtr_com_init() {
    // SAFETY: com_init copies into session memory; statics live forever.
    com_init(b"sin\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(sin), 1, 1, b"r=sin(x)\0".as_ptr() as *const c_char);
    com_init(b"cos\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(cos), 1, 1, b"r=cos(x)\0".as_ptr() as *const c_char);
    com_init(b"tan\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(tan), 1, 1, b"r=tan(x)\0".as_ptr() as *const c_char);
    com_init(b"asin\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(asin), 1, 1, b"r=asin(x)\0".as_ptr() as *const c_char);
    com_init(b"acos\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(acos), 1, 1, b"r=acos(x)\0".as_ptr() as *const c_char);
    com_init(b"atan\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(atan), 1, 1, b"r=atan(x)\0".as_ptr() as *const c_char);
    com_init(b"atan2\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub2(atan2), 2, 2, b"r=atan2(y,x)\0".as_ptr() as *const c_char);
    com_init(b"sinh\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(sinh), 1, 1, b"r=sinh(x)\0".as_ptr() as *const c_char);
    com_init(b"cosh\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(cosh), 1, 1, b"r=cosh(x)\0".as_ptr() as *const c_char);
    com_init(b"tanh\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(tanh), 1, 1, b"r=tanh(x)\0".as_ptr() as *const c_char);
    com_init(b"exp\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(exp), 1, 1, b"r=exp(x)\0".as_ptr() as *const c_char);
    com_init(b"ln\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(log), 1, 1, b"r=ln(x)\nNatural logarithm.\0".as_ptr() as *const c_char);
    com_init(b"log\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(log10), 1, 1, b"r=log(x)\nBase 10 logarithm.\0".as_ptr() as *const c_char);
    com_init(b"sqrt\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(sqrt), 1, 1, b"r=sqrt(x)\0".as_ptr() as *const c_char);
    com_init(b"ceil\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(ceil), 1, 1, b"r=ceil(x)\nSmallest integer not less than x.\0".as_ptr() as *const c_char);
    com_init(b"floor\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(floor), 1, 1, b"r=floor(x)\nLargest integer not more than x.\0".as_ptr() as *const c_char);
    com_init(b"abs\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub(func_abs), 1, 1, b"r=abs(x)\0".as_ptr() as *const c_char);
    com_init(b"mod\0".as_ptr() as *const c_char, TRUE, TRUE, libm_sub2(func_mod), 2, 2, b"r=mod(x,y)\0".as_ptr() as *const c_char);
    com_init(b"pow\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_pow), 2, 2, b"r=pow(x,y)\0".as_ptr() as *const c_char);
    com_init(b"min\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_min), 1, 1, b"r = min(matrix)\nReturn value is a vector containing smallest element in columns of given matrix.\nr=min(min(matrix) gives smallest element of the matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"max\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_max), 1, 1, b"r = max(matrix)\nReturn value is a vector containing largest element in columns of given matrix.\nr=max(max(matrix)) gives largest element of the matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"sum\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_sum), 1, 1, b"r = sum(matrix)\nReturn vector is column sums of given matrix. r=sum(sum(matrix)) gives\nthe total sum of elements of the matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"trace\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_trace), 1, 1, b"r = trace(matrix)\nReturn value is sum of matrix diagonal elements.\n\n\0".as_ptr() as *const c_char);
    com_init(b"det\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_det), 1, 1, b"r = det(matrix)\nReturn value is determinant of given square matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"inv\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_inv), 1, 1, b"r = inv(matrix)\nInvert given square matrix. Computed also by r=matrix^(-1).\n\n\0".as_ptr() as *const c_char);
    com_init(b"eig\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_eig), 1, 1, b"r = eig(matrix)\nReturn eigenvalues of given square matrix. r(n,0) is real part of the\nn:th eigenvalue, r(n,1) is the imaginary part respectively\n\n\0".as_ptr() as *const c_char);
    com_init(b"jacob\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_jacob), 3, 3, b"r = jacob(a,b,eps)\nSolve symmetric positive definite eigenvalue problem by Jacob iteration.\nReturn values are the eigenvalues. Also a variable eigv is created containing\neigenvectors.\n\n\0".as_ptr() as *const c_char);
    com_init(b"lud\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_LUD), 1, 1, b"r = lud(matrix)\nReturn value is lud decomposition of given square matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"hesse\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_hesse), 1, 1, b"r = hesse(matrix)\nReturn the upper hessenberg form of given matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"eye\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_eye), 1, 1, b"r = eye(n)\nReturn n by n identity matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"zeros\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_zeros), 1, 2, b"r = zeros(n,m)\nReturn n by m matrix with elements initialized to zero.\0".as_ptr() as *const c_char);
    com_init(b"ones\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_ones), 1, 2, b"r = ones(n,m)\nReturn n by m matrix with elements initialized to one.\0".as_ptr() as *const c_char);
    com_init(b"rand\0".as_ptr() as *const c_char, FALSE, FALSE, Some(mtr_rand), 1, 2, b"r = rand(n,m)\nReturn n by m matrix with elements initialized to with random number from\nzero to one.\n\n\0".as_ptr() as *const c_char);
    com_init(b"diag\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_diag), 1, 1, b"r=diag(matrix) or r=diag(vector)\nGiven matrix return diagonal entries as a vector. Given vector return matrix\nwith diagonal elements from vector. r=diag(diag(a)) gives matrix with diagonal\nelements from matrix a otherwise elements are zero.\n\n\0".as_ptr() as *const c_char);
    com_init(b"vector\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_vector), 2, 3, b"r=vector(start,end,inc)\nReturn vector of values going from start to end by inc.\n\n\0".as_ptr() as *const c_char);
    com_init(b"size\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_size), 1, 1, b"r = size(matrix)\nReturn size of given matrix.\0".as_ptr() as *const c_char);
    com_init(b"resize\0".as_ptr() as *const c_char, FALSE, TRUE, Some(mtr_resize), 2, 3, b"r = resize(matrix,n,m)\nMake a matrix to look as a n by m matrix.\n\n\0".as_ptr() as *const c_char);
    com_init(b"where\0".as_ptr() as *const c_char, FALSE, FALSE, Some(mtr_where), 1, 1, b"r = where(l)\nReturn linear indexes of where l is true.\n\n\0".as_ptr() as *const c_char);
}
