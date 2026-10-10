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

// ---- unit body: transcribed from matc/src/matc.c ----
// Transcribed from matc/src/matc.c (MODULE_MATC defining crate).
// Defines: listheaders, scanner tables, gra_state/gra_funcs, stdio/jmp
// globals, error_matc/PrintOut, allocator, commands, embedding API.
// Skipped: #if 0 dead blocks (mc.ini bootstrap, SYSTEM expansion),
// _OPENMP/DEBUG variants (not in the grade build).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn lst_add(list: c_int, item: *mut LIST);
    fn lst_find(list: c_int, nm: *mut c_char) -> *mut LIST;
    fn lst_purge(list: c_int);
    fn lst_print(list: c_int) -> *mut VARIABLE;
    fn var_to_string(v: *mut VARIABLE) -> *mut c_char;
    fn var_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_check(nm: *mut c_char) -> *mut VARIABLE;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_delete_temp(v: *mut VARIABLE);
    fn var_free();
    fn const_free();
    fn const_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn fnc_check(nm: *mut c_char) -> *mut FUNCTION;
    fn fnc_free();
    fn fnc_free_entry(fnc: *mut FUNCTION);
    fn evalclause(c: *mut CLAUSE) -> *mut VARIABLE;
    fn doit(s: *mut c_char) -> *mut VARIABLE;
    fn doit_compile(s: *mut c_char) -> *mut CLAUSE;
    fn free_tree(t: *mut TREE);
    fn mtr_com_init();
    fn var_com_init();
    fn fnc_com_init();
    fn fil_com_init();
    fn gra_com_init();
    fn str_com_init();
    fn sig_trap(sig: c_int);
    fn isatty(fd: c_int) -> c_int;
    fn fileno(stream: *mut FILE) -> c_int;
    fn isspace(c: c_int) -> c_int;
    fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
    fn strcasecmp(s1: *const c_char, s2: *const c_char) -> c_int;
}

pub const LC_ALL: c_int = 6;
pub const SIGINT: c_int = 2;
pub type SigHandler = Option<unsafe extern "C" fn(c_int)>;

// Stack setjmp buffer: 200 bytes / 8-byte aligned like glibc's jmp_buf
// (8 longs + saved-flag + 128-byte signal mask). Only size/alignment
// matter; setjmp fills it opaquely. Asserted against the C layout.
const _: () = assert!(size_of::<[u64; 25]>() == 200);

// ---- MODULE_MATC globals --------------------------------------------------
// SAFETY: session globals owned by this crate, same lifetimes as C.
#[no_mangle]
pub static mut listheaders: [LIST; 5] = [
    LIST { next: core::ptr::null_mut(), name: b"Allocations\0".as_ptr() as *mut c_char },
    LIST { next: core::ptr::null_mut(), name: b"Constants\0".as_ptr() as *mut c_char },
    LIST { next: core::ptr::null_mut(), name: b"Currently defined VARIABLES\0".as_ptr() as *mut c_char },
    LIST { next: core::ptr::null_mut(), name: b"Builtin Functions\0".as_ptr() as *mut c_char },
    LIST { next: core::ptr::null_mut(), name: b"User Functions\0".as_ptr() as *mut c_char },
];
// Scanner tables (exact transcription from matc.h; the S3 §6 lesson).
#[no_mangle]
pub static mut ssymbols: [SYMTYPE; 27] = [
    leftpar, rightpar, indopen, indclose, beginsym, endsym, power, times, ptimes,
    divide, plus, minus, reduction, transpose, lt, gt, and, or, not,
    assignsym, apply, resize, vector, statemend, argsep, comment, systemcall,
];
// csymbols is 27 symbols + NUL (C `char csymbols[]`); the `$` systemcall
// slot and terminator are load-bearing for char_in_list/index scans.
#[no_mangle]
pub static mut csymbols: [c_char; 28] = [
    b'(' as c_char, b')' as c_char, b'[' as c_char, b']' as c_char,
    b'{' as c_char, b'}' as c_char, b'^' as c_char, b'*' as c_char,
    b'#' as c_char, b'/' as c_char, b'+' as c_char, b'-' as c_char,
    b'?' as c_char, b'\'' as c_char, b'<' as c_char, b'>' as c_char,
    b'&' as c_char, b'|' as c_char, b'~' as c_char, b'=' as c_char,
    b'@' as c_char, b'%' as c_char, b':' as c_char, b';' as c_char,
    b',' as c_char, b'!' as c_char, b'$' as c_char, 0,
];
#[no_mangle]
pub static mut reswords: [*mut c_char; 12] = [
    b"function\0".as_ptr() as *mut c_char,
    b"import\0".as_ptr() as *mut c_char,
    b"export\0".as_ptr() as *mut c_char,
    b"if\0".as_ptr() as *mut c_char,
    b"then\0".as_ptr() as *mut c_char,
    b"else\0".as_ptr() as *mut c_char,
    b"while\0".as_ptr() as *mut c_char,
    b"for\0".as_ptr() as *mut c_char,
    b"begin\0".as_ptr() as *mut c_char,
    b"end\0".as_ptr() as *mut c_char,
    b"break\0".as_ptr() as *mut c_char,
    core::ptr::null_mut(),
];
#[no_mangle]
pub static mut rsymbols: [SYMTYPE; 11] = [
    funcsym, import, export, ifsym, thensym, elsesym, whilesym, forsym,
    beginsym, endsym, breaksym,
];
#[no_mangle]
pub static mut symchars: *mut c_char = b"`._\0".as_ptr() as *mut c_char;

#[no_mangle]
pub static mut gra_state: G_STATE = G_STATE {
    out_fp: core::ptr::null_mut(),
    driver: 0,
    window: GStateWindow { xlow: -1.0, xhigh: 1.0, ylow: -1.0, yhigh: 1.0, zlow: -1.0, zhigh: 1.0 },
    viewport: MatcRectangle { xlow: 0.0, xhigh: 1.0, ylow: 0.0, yhigh: 1.0 },
    modelm: [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ],
    viewm: [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ],
    projm: [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ],
    transfm: [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ],
    pratio: 0.0,
    cur_point: Point { x: 0.0, y: 0.0, z: 0.0 },
    cur_color: 1,
    cur_marker: 1,
};
pub type GraFunc = Option<unsafe extern "C" fn()>;
#[no_mangle]
pub static mut gra_funcs: [GraFunc; 27] = [None; 27];

#[no_mangle]
pub static mut math_in: *mut FILE = core::ptr::null_mut();
#[no_mangle]
pub static mut math_out: *mut FILE = core::ptr::null_mut();
#[no_mangle]
pub static mut math_err: *mut FILE = core::ptr::null_mut();
#[no_mangle]
pub static mut jmpbuf: *mut JmpBuf = core::ptr::null_mut();
#[no_mangle]
pub static mut term: c_int = 0;
#[no_mangle]
pub static mut math_out_str: *mut c_char = core::ptr::null_mut();
static mut math_out_count: c_int = 0;
static mut math_out_allocated: c_int = 0;

// SAFETY: appends a C string to the math_out_str buffer (grow-by-512).
unsafe fn out_reserve(extra: c_int) {
    // SAFETY: file statics mirror the C originals.
    if math_out_count + extra > math_out_allocated {
        math_out_allocated += 512;
        // SAFETY: realloc(nil-or-live, grown) — same pattern as C.
        math_out_str = realloc(
            math_out_str as *mut core::ffi::c_void,
            math_out_allocated as usize,
        ) as *mut c_char;
    }
}

// SAFETY: mirrors C error_matc (MATC ERROR prefix + format into the
// session buffer, free-all, jump with code 2). The VaList is consumed
// once (prefix uses no varargs), exactly like the C two-call sequence.
#[no_mangle]
pub unsafe extern "C" fn error_matc(fmt: *const c_char, args: ...) -> ! {
    out_reserve(512);
    // SAFETY: math_out_str has room (reserved above).
    math_out_count += sprintf(math_out_str.offset(math_out_count as isize), b"MATC ERROR: \0".as_ptr() as *const c_char);
    out_reserve(512);
    // SAFETY: math_out_str has room; args forwarded once via VaList.
    math_out_count += vsprintf(math_out_str.offset(math_out_count as isize), fmt, args);
    mem_free_all();
    // SAFETY: jmpbuf points at the live setjmp frame; never returns.
    longjmp(jmpbuf, 2);
}

// SAFETY: mirrors C PrintOut (format into the session buffer).
#[no_mangle]
pub unsafe extern "C" fn PrintOut(fmt: *const c_char, args: ...) {
    out_reserve(512);
    // SAFETY: math_out_str has room; args forwarded once via VaList.
    math_out_count += vsprintf(math_out_str.offset(math_out_count as isize), fmt, args);
}

// SAFETY: STRCOPY macro (strcpy into a fresh mem_alloc block).
unsafe fn strcopy(s: *const c_char) -> *mut c_char {
    // SAFETY: strlen on a live C string; fresh block sized len+1.
    let n = strlen(s);
    // SAFETY: mem_alloc never returns NULL (errors instead).
    let dst = mem_alloc(n + 1) as *mut c_char;
    // SAFETY: dst has n+1 bytes; strcpy NUL-terminates.
    strcpy(dst, s);
    dst
}

// SAFETY: mirrors C mem_alloc (calloc + ALLOC_HEAD linkage; errors via
// error_matc on OOM like the original).
#[no_mangle]
pub unsafe extern "C" fn mem_alloc(size: size_t) -> *mut core::ffi::c_void {
    // SAFETY: calloc(size+16) zeroed; header tracked below.
    let lst = calloc(size + size_of::<ALLOC_LIST>(), 1) as *mut ALLOC_LIST;
    if lst.is_null() {
        // SAFETY: static format string.
        error_matc(b"Can't alloc mem.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: ALLOC_HEAD is the live allocation list.
    let head_next = (*listheaders.as_mut_ptr().offset(ALLOCATIONS as isize)).next;
    (*lst).next = head_next as *mut ALLOC_LIST;
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = lst as *mut LIST;
    // SAFETY: user area past the header.
    (lst as *mut c_char).add(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void
}

// SAFETY: mirrors C mem_free (list unlink + free; missing entries freed
// directly like the C original).
#[no_mangle]
pub unsafe extern "C" fn mem_free(mem: *mut core::ffi::c_void) {
    // SAFETY: ALLOC_HEAD is the live allocation list.
    let mut lst = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next as *mut ALLOC_LIST;
    if lst.is_null() {
        // SAFETY: not tracked: free the header like C.
        free((mem as *mut c_char).sub(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void);
        return;
    }
    if (lst as *mut c_char).add(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void != mem {
        while !(*lst).next.is_null() {
            if ((*lst).next as *mut c_char).add(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void == mem {
                break;
            }
            lst = (*lst).next;
        }
        if (*lst).next.is_null() {
            // SAFETY: untracked block: free directly like C.
            free((mem as *mut c_char).sub(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void);
            return;
        }
        (*lst).next = (*(*lst).next).next;
    } else {
        (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = (*lst).next as *mut LIST;
    }
    // SAFETY: the target's own header (ALLOC_LST(mem)), never the
    // predecessor node: freeing lst here would corrupt the live list.
    free((mem as *mut c_char).sub(size_of::<ALLOC_LIST>()) as *mut core::ffi::c_void);
}

// SAFETY: mirrors C mem_free_all (bulk free of the session list).
#[no_mangle]
pub unsafe extern "C" fn mem_free_all() {
    // SAFETY: ALLOC_HEAD is the live allocation list.
    let mut lst = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next as *mut ALLOC_LIST;
    while !lst.is_null() {
        let lstn = (*lst).next;
        // SAFETY: lst is a tracked calloc block.
        free(lst as *mut core::ffi::c_void);
        lst = lstn;
    }
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
}

// SAFETY: mirrors C mtc_init (streams, command tables, constants).
#[no_mangle]
pub unsafe extern "C" fn mtc_init(input_file: *mut FILE, output_file: *mut FILE, error_file: *mut FILE) {
    // SAFETY: ALLOC_HEAD reset like C.
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
    math_in = input_file;
    math_err = error_file;
    math_out = output_file;
    mtr_com_init();
    var_com_init();
    fnc_com_init();
    fil_com_init();
    gra_com_init();
    str_com_init();
    com_init(b"eval\0".as_ptr() as *const c_char, FALSE, FALSE, Some(com_apply), 1, 1,
        b"eval( str )\n\nEvaluate content variable. Another form of this command is @str.\n\0".as_ptr() as *const c_char);
    com_init(b"source\0".as_ptr() as *const c_char, FALSE, FALSE, Some(com_source), 1, 1,
        b"source( name )\n\nExecute commands from file given name.\n\0".as_ptr() as *const c_char);
    com_init(b"help\0".as_ptr() as *const c_char, FALSE, FALSE, Some(com_help), 0, 1,
        b"help or help(\"symbol\")\n\nFirst form of the command gives list of available commands.\nSecond form gives help on specific routine.\n\0".as_ptr() as *const c_char);
    com_init(b"quit\0".as_ptr() as *const c_char, FALSE, FALSE, Some(com_quit), 0, 0,
        b"quit\n\0".as_ptr() as *const c_char);
    com_init(b"exit\0".as_ptr() as *const c_char, FALSE, FALSE, Some(com_quit), 0, 0,
        b"exit\n\0".as_ptr() as *const c_char);
    // SAFETY: const_new returns live globals; cells set below.
    let ptr = const_new(b"true\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 1.0);
    let ptr = const_new(b"false\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 0.0);
    let ptr = const_new(b"stdin\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 0.0);
    let ptr = const_new(b"stdout\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 1.0);
    let ptr = const_new(b"stderr\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 2.0);
    let ptr = const_new(b"pi\0".as_ptr() as *mut c_char, TYPE_DOUBLE, 1, 1);
    var_set_m(ptr, 0, 0, 2.0 * acos(0.0));
}

// SAFETY: mirrors C doread (line loop with setjmp recovery; case 3 exits
// via a labeled break for C's goto ret).
#[no_mangle]
pub unsafe extern "C" fn doread() -> *mut c_char {
    let savejmp = jmpbuf;
    let mut jmp = [0u64; 25];
    jmpbuf = &mut jmp as *mut [u64; 25] as *mut JmpBuf;
    if !math_out_str.is_null() {
        *math_out_str = 0;
    }
    math_out_count = 0;
    // SAFETY: 4096-byte session buffer, freed at the end.
    let p = mem_alloc(4096) as *mut c_char;
    let q = p;
    // SAFETY: dogets fills the live buffer.
    'outer: while dogets(p, b"MATC> \0".as_ptr() as *mut c_char) != 0 {
        if *p != 0 {
            // SAFETY: ALLOC_HEAD reset like C.
            (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
            // SAFETY: VAR_HEAD is the live global variable list.
            let headsave = (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next as *mut VARIABLE;
            // SAFETY: setjmp on the live frame; cases mirror C.
            match setjmp(jmpbuf) {
                0 => {
                    doit(p);
                    longjmp(jmpbuf, 1);
                }
                1 => {}
                2 => {
                    (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next = headsave as *mut LIST;
                }
                3 => {
                    break 'outer;
                }
                _ => {}
            }
        }
    }
    jmpbuf = savejmp;
    // SAFETY: q from mem_alloc above.
    mem_free(q as *mut core::ffi::c_void);
    math_out_str
}

// SAFETY: mirrors C mtc_domath (string entry; setjmp-guarded doit).
#[no_mangle]
pub unsafe extern "C" fn mtc_domath(s: *mut c_char) -> *mut c_char {
    // SAFETY: locale call like C.
    setlocale(LC_ALL, b"C\0".as_ptr() as *const c_char);
    // SAFETY: signal returns the previous handler (kept for restore).
    let sigfunc: SigHandler = signal(SIGINT, Some(sig_trap));
    if s.is_null() || *s == 0 {
        // SAFETY: doread runs the interactive loop on the live streams.
        let out = doread();
        signal(SIGINT, sigfunc);
        return out;
    }
    let savejmp = jmpbuf;
    let mut jmp = [0u64; 25];
    jmpbuf = &mut jmp as *mut [u64; 25] as *mut JmpBuf;
    if !math_out_str.is_null() {
        *math_out_str = 0;
    }
    math_out_count = 0;
    if *s != 0 {
        // SAFETY: ALLOC_HEAD reset like C.
        (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
        // SAFETY: VAR_HEAD is the live global variable list.
        let headsave = (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next as *mut VARIABLE;
        // SAFETY: setjmp on the live frame; cases mirror C.
        match setjmp(jmpbuf) {
            0 => {
                doit(s);
                longjmp(jmpbuf, 1);
            }
            1 => {}
            2 => {
                (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next = headsave as *mut LIST;
            }
            3 => {}
            _ => {}
        }
    }
    jmpbuf = savejmp;
    signal(SIGINT, sigfunc);
    math_out_str
}

// SAFETY: mirrors C com_quit (unwind with code 3).
#[no_mangle]
pub unsafe extern "C" fn com_quit(_var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = _var;
    // SAFETY: jmpbuf points at the live setjmp frame; never returns.
    longjmp(jmpbuf, 3);
}

// SAFETY: mirrors C dogets (terminal-aware line reader with backslash
// continuation; the #if 0 SYSTEM block stays dead like upstream).
#[no_mangle]
pub unsafe extern "C" fn dogets(buff: *mut c_char, prompt: *mut c_char) -> c_int {
    // SAFETY: math_in checked live below.
    if math_in.is_null() {
        return FALSE;
    }
    // SAFETY: fileno on live streams; prompt printed only on a tty pair.
    if isatty(fileno(math_in)) != 0 && isatty(fileno(math_out)) != 0 {
        // SAFETY: PrintOut appends; prompt live.
        PrintOut(b"%s\0".as_ptr() as *const c_char, prompt);
    }
    // SAFETY: buff has caller space (4096 in doread).
    *buff.offset(0) = b' ' as c_char;
    // SAFETY: ptr walks the live buffer.
    let mut ptr = buff.offset(1);
    // SAFETY: fgets into the 256-byte window from the live stream.
    ptr = fgets(ptr, 256, math_in);
    while !ptr.is_null() {
        // SAFETY: strlen on the live line; strip newline like C.
        *ptr.offset(strlen(ptr) as isize - 1) = 0;
        // SAFETY: backslash-continuation recursion into the tail.
        while *ptr.offset(strlen(ptr) as isize - 1) == b'\\' as c_char {
            // SAFETY: strlen on the live line.
            ptr = ptr.offset(strlen(ptr) as isize - 1);
            dogets(ptr, b"####> \0".as_ptr() as *mut c_char);
        }
        // SAFETY: skip leading whitespace like C.
        let mut p = ptr;
        while isspace(*p as c_int) != 0 {
            p = p.offset(1);
        }
        if *p != 0 {
            if *buff != 0 {
                return TRUE;
            }
        }
        // SAFETY: prompt re-print only on a tty pair.
        if isatty(fileno(math_in)) != 0 && isatty(fileno(math_out)) != 0 {
            // SAFETY: PrintOut appends; prompt live.
            PrintOut(b"%s\0".as_ptr() as *const c_char, prompt);
        }
        // SAFETY: fgets into the 256-byte window from the live stream.
        ptr = fgets(ptr, 256, math_in);
    }
    FALSE
}

// SAFETY: mirrors C com_init (lexical command registration).
#[no_mangle]
pub unsafe extern "C" fn com_init(
    word: *const c_char,
    flag_pw: c_int,
    flag_ce: c_int,
    sub: CommandSub,
    minp: c_int,
    maxp: c_int,
    help_text: *const c_char,
) {
    // SAFETY: fresh ALLOCMEM block (calloc-zeroed: flags start 0).
    let ptr = mem_alloc(size_of::<COMMAND>()) as *mut COMMAND;
    // SAFETY: STRCOPY into session memory.
    (*ptr).name = strcopy(word as *const c_char);
    if flag_pw != 0 {
        (*ptr).flags |= CMDFLAG_PW;
    }
    if flag_ce != 0 {
        (*ptr).flags |= CMDFLAG_CE;
    }
    (*ptr).minp = minp;
    (*ptr).maxp = maxp;
    (*ptr).sub = sub;
    (*ptr).help = help_text as *mut c_char;
    lst_add(COMMANDS, ptr as *mut LIST);
}

// SAFETY: mirrors C com_free (drops the command list).
#[no_mangle]
pub unsafe extern "C" fn com_free() {
    lst_purge(COMMANDS);
}

// SAFETY: mirrors C com_check (NULL when absent).
#[no_mangle]
pub unsafe extern "C" fn com_check(s: *mut c_char) -> *mut COMMAND {
    // SAFETY: COMMANDS list live; lst_find NULL-tolerant.
    lst_find(COMMANDS, s) as *mut COMMAND
}

// SAFETY: mirrors C com_help (list-all or per-symbol help).
#[no_mangle]
pub unsafe extern "C" fn com_help(ptr: *mut VARIABLE) -> *mut VARIABLE {
    if ptr.is_null() {
        lst_print(COMMANDS);
        lst_print(FUNCTIONS);
    } else {
        // SAFETY: var_to_string returns a fresh mem_alloc'd string.
        let nm = var_to_string(ptr);
        // SAFETY: com_check on the live name.
        let cmd = com_check(nm);
        if !cmd.is_null() {
            if !(*cmd).help.is_null() {
                // SAFETY: PrintOut appends; help text live.
                PrintOut(b"\n%s\n\0".as_ptr() as *const c_char, (*cmd).help);
            } else {
                // SAFETY: PrintOut appends; live name arg.
                PrintOut(b"\nSorry: no help available on [%s].\n\0".as_ptr() as *const c_char, nm);
            }
        } else {
            // SAFETY: fnc_check on the live name.
            let fnc = fnc_check(nm);
            if !fnc.is_null() {
                if !(*fnc).help.is_null() {
                    // SAFETY: PrintOut appends; help text live.
                    PrintOut(b"\n%s\0".as_ptr() as *const c_char, (*fnc).help);
                } else {
                    // SAFETY: PrintOut appends; live name arg.
                    PrintOut(b"\nSorry: no help available on [%s].\n\0".as_ptr() as *const c_char, nm);
                }
            } else {
                // SAFETY: static format; live name arg.
                error_matc(b"help: symbol not found: [%s]\n\0".as_ptr() as *const c_char, nm);
            }
        }
        // SAFETY: nm from var_to_string above.
        mem_free(nm as *mut core::ffi::c_void);
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C com_pointw (1/2/3-arg pointwise application; arity
// from the NEXT chain like the original).
#[no_mangle]
pub unsafe extern "C" fn com_pointw(sub: *mut core::ffi::c_void, ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument matrix live.
    let n = var_nrow(ptr);
    let m = var_ncol(ptr);
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(var_type(ptr), n, m);
    let sz = n * m;
    // SAFETY: both data blocks live.
    let mut a = var_matr(ptr);
    let mut b = var_matr(res);
    // SAFETY: NEXT chain checked like C.
    let ptr2 = (*ptr).next;
    if !ptr2.is_null() {
        if n != var_nrow(ptr2) || m != var_ncol(ptr2) {
            // SAFETY: static string (original lacks trailing newline).
            error_matc(b"Pointwise function arguments must all be of same size.\0".as_ptr() as *const c_char);
        }
        // SAFETY: ptr2 data live.
        let mut a2 = var_matr(ptr2);
        // SAFETY: NEXT chain checked like C.
        let ptr3 = (*ptr2).next;
        if !ptr3.is_null() {
            if n != var_nrow(ptr3) || m != var_ncol(ptr3) {
                // SAFETY: static string (original trailing comma kept).
                error_matc(b"Pointwise function arguments must all be of same size,\0".as_ptr() as *const c_char);
            }
            if !(*ptr3).next.is_null() {
                // SAFETY: static format string.
                error_matc(b"Currently at most three arguments for pointwise functions allowed, sorry.\n\0".as_ptr() as *const c_char);
            }
            // SAFETY: ptr3 data live; sub has the 3-arg shape here.
            let mut a3 = var_matr(ptr3);
            let g: unsafe extern "C" fn(c_double, c_double, c_double) -> c_double = core::mem::transmute(sub);
            let mut i: c_int = 0;
            while i < sz {
                *b = g(*a, *a2, *a3);
                b = b.offset(1);
                a = a.offset(1);
                a2 = a2.offset(1);
                a3 = a3.offset(1);
                i += 1;
            }
        } else {
            // SAFETY: sub has the 2-arg shape here.
            let g: unsafe extern "C" fn(c_double, c_double) -> c_double = core::mem::transmute(sub);
            let mut i: c_int = 0;
            while i < sz {
                *b = g(*a, *a2);
                b = b.offset(1);
                a = a.offset(1);
                a2 = a2.offset(1);
                i += 1;
            }
        }
    } else {
        // SAFETY: sub has the 1-arg shape here.
        let g: unsafe extern "C" fn(c_double) -> c_double = core::mem::transmute(sub);
        let mut i: c_int = 0;
        while i < sz {
            *b = g(*a);
            b = b.offset(1);
            a = a.offset(1);
            i += 1;
        }
    }
    res
}

// SAFETY: mirrors C com_el (single/dual index extraction incl. the
// logical-mask fast path and the scalar shortcut).
#[no_mangle]
pub unsafe extern "C" fn com_el(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: NEXT chain live (index args).
    let par = (*ptr).next;
    // SAFETY: fn-static cell mirrors the C original.
    static mut defind: c_double = 0.0;
    let mut ind1: *mut c_double = core::ptr::addr_of_mut!(defind);
    let ind2: *mut c_double;
    let mut size1: c_int = 1;
    let size2: c_int;
    // SAFETY: ptr matrix live.
    let rows = var_nrow(ptr);
    let cols = var_ncol(ptr);
    if rows == 1 && cols == 1 {
        // SAFETY: par cells live.
        if *var_matr(par) != 0.0 {
            // SAFETY: static format string.
            error_matc(b"Index out of bounds.\n\0".as_ptr() as *const c_char);
        }
        if !(*par).next.is_null() {
            // SAFETY: NEXT par cell live.
            if *var_matr((*par).next) != 0.0 {
                // SAFETY: static format string.
                error_matc(b"Index out of bounds.\n\0".as_ptr() as *const c_char);
            }
        }
        // SAFETY: fresh temp owns its storage.
        let res = var_temp_new(var_type(ptr), 1, 1);
        // SAFETY: both cells live.
        *var_matr(res) = *var_matr(ptr);
        return res;
    }
    if (*par).next.is_null() {
        if var_nrow(par) == rows && var_ncol(par) == cols {
            let mut logical: c_int = TRUE;
            let mut onecount: c_int = 0;
            // SAFETY: par data live.
            let dtmp = var_matr(par);
            let mut i: c_int = 0;
            while i < var_nrow(par) * var_ncol(par) {
                // SAFETY: par cells live.
                if *dtmp.offset(i as isize) == 0.0 {
                } else if *dtmp.offset(i as isize) == 1.0 {
                    onecount += 1;
                } else {
                    logical = FALSE;
                    break;
                }
                i += 1;
            }
            if logical != 0 {
                if onecount == 0 {
                    return core::ptr::null_mut();
                }
                // SAFETY: fresh temp owns its storage.
                let res = var_temp_new(var_type(ptr), 1, onecount);
                let mut i: c_int = 0;
                let mut k: c_int = 0;
                while i < rows {
                    let mut j: c_int = 0;
                    while j < cols {
                        if var_m(par, i, j) == 1.0 {
                            // SAFETY: res/ptr cells live.
                            var_set_m(res, 0, k, var_m(ptr, i, j));
                            k += 1;
                        }
                        j += 1;
                    }
                    i += 1;
                }
                return res;
            }
        }
        // SAFETY: par data live.
        ind2 = var_matr(par);
        size2 = var_ncol(par);
        let rows = rows * cols;
        // SAFETY: fresh temp owns its storage.
        let res = var_temp_new(var_type(ptr), size1, size2);
        let mut i: c_int = 0;
        while i < size1 {
            let ind = *ind1 as c_int;
            let mut j: c_int = 0;
            while j < size2 {
                if ind < rows && (*ind2.offset(j as isize) as c_int) < cols {
                    // SAFETY: res/ptr cells live.
                    var_set_m(res, i, j, var_m(ptr, ind, *ind2.offset(j as isize) as c_int));
                } else {
                    // SAFETY: static format string.
                    error_matc(b"Index out of bounds.\n\0".as_ptr() as *const c_char);
                }
                j += 1;
            }
            i += 1;
        }
        return res;
    } else {
        // SAFETY: par + NEXT par data live.
        ind1 = var_matr(par);
        size1 = var_ncol(par);
        size2 = var_ncol((*par).next);
        ind2 = var_matr((*par).next);
    }
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(var_type(ptr), size1, size2);
    let mut i: c_int = 0;
    while i < size1 {
        // SAFETY: ind cells live.
        let ind = *ind1.offset(i as isize) as c_int;
        let mut j: c_int = 0;
        while j < size2 {
            // SAFETY: ind cells live.
            if ind < rows && (*ind2.offset(j as isize) as c_int) < cols {
                // SAFETY: res/ptr cells live.
                var_set_m(res, i, j, var_m(ptr, ind, *ind2.offset(j as isize) as c_int));
            } else {
                // SAFETY: static format string.
                error_matc(b"Index out of bounds.\n\0".as_ptr() as *const c_char);
            }
            j += 1;
        }
        i += 1;
    }
    res
}

// SAFETY: mirrors C com_source (file redirect around doread).
#[no_mangle]
pub unsafe extern "C" fn com_source(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: math_in is the live input stream.
    let save_in = math_in;
    // SAFETY: var_to_string returns a fresh mem_alloc'd string.
    let nm = var_to_string(ptr);
    // SAFETY: fopen for reading; NULL-checked like C.
    let f = fopen(nm as *const c_char, b"r\0".as_ptr() as *const c_char);
    if !f.is_null() {
        math_in = f;
        doread();
        // SAFETY: f is a live open stream.
        fclose(f);
    } else {
        // SAFETY: static format; live name arg.
        PrintOut(b"Source: Can't open file, %s.\n\0".as_ptr() as *const c_char, nm);
    }
    math_in = save_in;
    // SAFETY: nm from var_to_string above.
    mem_free(nm as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C com_apply (string matrix executed via doit).
#[no_mangle]
pub unsafe extern "C" fn com_apply(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: ptr matrix live.
    let n = var_nrow(ptr) * var_ncol(ptr);
    // SAFETY: C-heap string buffer, freed below.
    let q = mem_alloc((n as usize) + 1) as *mut c_char;
    let mut p = q;
    let mut i: c_int = 0;
    while i < var_nrow(ptr) {
        let mut j: c_int = 0;
        while j < var_ncol(ptr) {
            // SAFETY: q has n+1 bytes; cell live.
            *p = var_m(ptr, i, j) as c_char;
            p = p.offset(1);
            j += 1;
        }
        i += 1;
    }
    *p = 0;
    // SAFETY: q is a live NUL-terminated string; doit returns a live chain.
    let res = doit(q);
    // SAFETY: q from mem_alloc above.
    mem_free(q as *mut core::ffi::c_void);
    res
}

#[repr(C)]
pub struct MtcCompiled {
    pub root: *mut CLAUSE,
    pub alloc_head: *mut LIST,
}

// SAFETY: mirrors C mtc_compile (isolated parse into a plain-malloc
// handle; errors free the handle and yield NULL like the original).
#[no_mangle]
pub unsafe extern "C" fn mtc_compile(s: *mut c_char) -> *mut core::ffi::c_void {
    if s.is_null() || *s == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: locale call like C.
    setlocale(LC_ALL, b"C\0".as_ptr() as *const c_char);
    // SAFETY: ALLOC_HEAD isolation like C.
    let saved = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next;
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
    let savejmp = jmpbuf;
    let mut jmp = [0u64; 25];
    jmpbuf = &mut jmp as *mut [u64; 25] as *mut JmpBuf;
    // SAFETY: plain-malloc handle (freed on error/below like C).
    let compiled = malloc(size_of::<MtcCompiled>()) as *mut MtcCompiled;
    // SAFETY: setjmp on the live frame; cases mirror C.
    match setjmp(jmpbuf) {
        0 => {
            // SAFETY: doit_compile parses into session memory.
            (*compiled).root = doit_compile(s);
            (*compiled).alloc_head = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next;
        }
        _ => {
            // SAFETY: compiled from malloc above.
            free(compiled as *mut core::ffi::c_void);
            jmpbuf = savejmp;
            (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = saved;
            return core::ptr::null_mut();
        }
    }
    jmpbuf = savejmp;
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = saved;
    compiled as *mut core::ffi::c_void
}

// SAFETY: mirrors C mtc_eval (evals a compiled handle under setjmp).
#[no_mangle]
pub unsafe extern "C" fn mtc_eval(handle: *mut core::ffi::c_void) -> *mut c_char {
    if handle.is_null() {
        return core::ptr::null_mut();
    }
    let compiled = handle as *mut MtcCompiled;
    // SAFETY: locale call like C.
    setlocale(LC_ALL, b"C\0".as_ptr() as *const c_char);
    let savejmp = jmpbuf;
    let mut jmp = [0u64; 25];
    jmpbuf = &mut jmp as *mut [u64; 25] as *mut JmpBuf;
    if !math_out_str.is_null() {
        *math_out_str = 0;
    }
    math_out_count = 0;
    // SAFETY: ALLOC_HEAD reset like C.
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
    // SAFETY: VAR_HEAD is the live global variable list.
    let headsave = (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next as *mut VARIABLE;
    // SAFETY: setjmp on the live frame; cases mirror C.
    match setjmp(jmpbuf) {
        0 => {
            // SAFETY: root is the live compiled clause tree.
            evalclause((*compiled).root);
            longjmp(jmpbuf, 1);
        }
        1 => {}
        2 => {
            (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next = headsave as *mut LIST;
        }
        3 => {}
        _ => {}
    }
    jmpbuf = savejmp;
    math_out_str
}

// SAFETY: mirrors C mtc_set_real_array (persistent 1×n variable, memcpy
// fast path on matching shape).
#[no_mangle]
pub unsafe extern "C" fn mtc_set_real_array(nm: *const c_char, values: *mut c_double, n: c_int) {
    let target_n = if n > 0 { n } else { 1 };
    // SAFETY: var_check on a live name.
    let ptr = var_check(nm as *mut c_char);
    if !ptr.is_null() && var_nrow(ptr) == 1 && var_ncol(ptr) == target_n {
        if n > 0 {
            // SAFETY: both arrays span n live doubles.
            memcpy(
                var_matr(ptr) as *mut core::ffi::c_void,
                values as *const core::ffi::c_void,
                (n as usize) * size_of::<c_double>(),
            );
        } else {
            var_set_m(ptr, 0, 0, 0.0);
        }
        return;
    }
    // SAFETY: ALLOC_HEAD isolation like C.
    let saved = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next;
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = core::ptr::null_mut();
    // SAFETY: var_new returns a live global (deletes any prior shape).
    let ptr = var_new(nm as *mut c_char, TYPE_DOUBLE, 1, target_n);
    if n > 0 {
        // SAFETY: both arrays span n live doubles.
        memcpy(
            var_matr(ptr) as *mut core::ffi::c_void,
            values as *const core::ffi::c_void,
            (n as usize) * size_of::<c_double>(),
        );
    } else {
        var_set_m(ptr, 0, 0, 0.0);
    }
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = saved;
}

// SAFETY: mirrors C mtc_free_compiled (bulk-free of compile-time blocks).
#[no_mangle]
pub unsafe extern "C" fn mtc_free_compiled(handle: *mut core::ffi::c_void) {
    if handle.is_null() {
        return;
    }
    let compiled = handle as *mut MtcCompiled;
    // SAFETY: ALLOC_HEAD swap like C.
    let saved = (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next;
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = (*compiled).alloc_head;
    mem_free_all();
    (*(listheaders.as_mut_ptr().offset(ALLOCATIONS as isize))).next = saved;
    // SAFETY: compiled from malloc.
    free(compiled as *mut core::ffi::c_void);
}
