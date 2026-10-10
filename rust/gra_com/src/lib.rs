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

// ---- unit body: transcribed from matc/src/gra_com.c ----
// Transcribed from matc/src/gra_com.c (graphics command wrappers).
// Each wrapper dispatches through the gra_funcs table exactly like the
// GRA_* macros (transmute slot to the matching C signature and call).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut gra_funcs: [GraFunc; 27];
    fn var_to_string(v: *mut VARIABLE) -> *mut c_char;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn gra_init_matc(devtype: c_int, nm: *mut c_char);
    fn c3d_gc3d(v: *mut VARIABLE) -> *mut VARIABLE;
    fn c3d_gc3dlevels(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mem_free(ptr: *mut core::ffi::c_void);
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

pub type GraFunc = Option<unsafe extern "C" fn()>;

pub const G_OPEN: usize = 0;
pub const G_CLOSE: usize = 1;
pub const G_CLEAR: usize = 2;
pub const G_VIEWPORT: usize = 3;
pub const G_WINDOW: usize = 4;
pub const G_DEFCOLOR: usize = 5;
pub const G_COLOR: usize = 6;
pub const G_POLYLINE: usize = 7;
pub const G_DRAW: usize = 8;
pub const G_MOVE: usize = 9;
pub const G_POLYMARKER: usize = 10;
pub const G_MARKER: usize = 11;
pub const G_AREAFILL: usize = 12;
pub const G_IMAGE: usize = 13;
pub const G_TEXT: usize = 14;
pub const G_FLUSH: usize = 15;
pub const G_RESET: usize = 16;
pub const G_TRANSLATE: usize = 17;
pub const G_ROTATE: usize = 18;
pub const G_SCALE: usize = 19;
pub const G_VIEWPOINT: usize = 20;
pub const G_GETMATRIX: usize = 21;
pub const G_SETMATRIX: usize = 22;
pub const G_PERSPECTIVE: usize = 23;
pub const G_DBUFFER: usize = 24;
pub const G_SBUFFER: usize = 25;
pub const G_SWAPBUF: usize = 26;

// SAFETY: reads the live driver slot (mirrors the GRA_* macro dereference).
unsafe fn slot(s: usize) -> GraFunc {
    *gra_funcs.as_mut_ptr().offset(s as isize)
}
// SAFETY: transmutes a table slot to the call signature its GRA_* macro
// uses; the slot always holds a driver fn of that shape (wired by
// gra_init_matc, error stub otherwise).
unsafe fn call0(s: usize) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn() = core::mem::transmute(f);
        g();
    }
}
// SAFETY: see call0 (1-int-arg driver fns: OPEN, COLOR).
unsafe fn call1i(s: usize, a: c_int) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int) = core::mem::transmute(f);
        g(a);
    }
}
// SAFETY: see call0 (DEFCOLOR shape).
unsafe fn call1i3d(s: usize, a: c_int, b: c_double, c: c_double, d: c_double) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int, c_double, c_double, c_double) = core::mem::transmute(f);
        g(a, b, c, d);
    }
}
// SAFETY: see call0 (POLYLINE shape: void* data like the C macro).
unsafe fn call1i1p(s: usize, a: c_int, p: *mut core::ffi::c_void) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
        g(a, p);
    }
}
// SAFETY: see call0 (DRAW/MOVE/GETMATRIX/... shape).
unsafe fn call1p(s: usize, p: *mut core::ffi::c_void) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(*mut core::ffi::c_void) = core::mem::transmute(f);
        g(p);
    }
}
// SAFETY: see call0 (POLYMARKER shape).
unsafe fn call2i1p(s: usize, a: c_int, b: c_int, p: *mut core::ffi::c_void) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int, c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
        g(a, b, p);
    }
}
// SAFETY: see call0 (MARKER/AREAFILL shape).
unsafe fn call1i1p2(s: usize, a: c_int, p: *mut core::ffi::c_void) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
        g(a, p);
    }
}
// SAFETY: see call0 (IMAGE shape).
unsafe fn call3i1p(s: usize, a: c_int, b: c_int, c: c_int, p: *mut core::ffi::c_void) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_int, c_int, c_int, *mut core::ffi::c_void) =
            core::mem::transmute(f);
        g(a, b, c, p);
    }
}
// SAFETY: see call0 (TEXT shape).
unsafe fn call2d1p(s: usize, a: c_double, b: c_double, p: *mut c_char) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_double, c_double, *mut c_char) = core::mem::transmute(f);
        g(a, b, p);
    }
}
// SAFETY: see call0 (TRANSLATE/ROTATE/SCALE shape).
unsafe fn call3d(s: usize, a: c_double, b: c_double, c: c_double) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double) = core::mem::transmute(f);
        g(a, b, c);
    }
}
// SAFETY: see call0 (VIEWPOINT/WINDOW shape).
unsafe fn call6d(s: usize, a: c_double, b: c_double, c: c_double, d: c_double, e: c_double, f2: c_double) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double, c_double, c_double, c_double) =
            core::mem::transmute(f);
        g(a, b, c, d, e, f2);
    }
}
// SAFETY: see call0 (VIEWPORT shape).
unsafe fn call4d(s: usize, a: c_double, b: c_double, c: c_double, d: c_double) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double, c_double) = core::mem::transmute(f);
        g(a, b, c, d);
    }
}
// SAFETY: see call0 (PERSPECTIVE shape).
unsafe fn call1d(s: usize, a: c_double) {
    if let Some(f) = slot(s) {
        let g: unsafe extern "C" fn(c_double) = core::mem::transmute(f);
        g(a);
    }
}

// SAFETY: mirrors C gra_gopen (optional name second arg).
#[no_mangle]
pub unsafe extern "C" fn gra_gopen(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live; NEXT cell checked like C.
    if !(*var).next.is_null() {
        // SAFETY: NEXT arg string fresh mem_alloc'd.
        let nm = var_to_string((*var).next);
        gra_init_matc(*var_matr(var) as c_int, nm);
        // SAFETY: nm from var_to_string above.
        mem_free(nm as *mut core::ffi::c_void);
    } else {
        gra_init_matc(*var_matr(var) as c_int, core::ptr::null_mut());
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gclose/gclear/gflush (nullary table calls).
#[no_mangle]
pub unsafe extern "C" fn gra_gclose(_var: *mut VARIABLE) -> *mut VARIABLE {
    call0(G_CLOSE);
    core::ptr::null_mut()
}
// SAFETY: see gra_gclose.
#[no_mangle]
pub unsafe extern "C" fn gra_gclear(_var: *mut VARIABLE) -> *mut VARIABLE {
    call0(G_CLEAR);
    core::ptr::null_mut()
}
// SAFETY: see gra_gclose.
#[no_mangle]
pub unsafe extern "C" fn gra_gflush(_var: *mut VARIABLE) -> *mut VARIABLE {
    call0(G_FLUSH);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gdefcolor (index + rgb triple).
#[no_mangle]
pub unsafe extern "C" fn gra_gdefcolor(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var + NEXT data live.
    let mut m = var_matr((*var).next);
    let i = *var_matr(var) as c_int;
    let r = *m;
    m = m.offset(1);
    let g = *m;
    m = m.offset(1);
    let b = *m;
    call1i3d(G_DEFCOLOR, i, r, g, b);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gcolor.
#[no_mangle]
pub unsafe extern "C" fn gra_gcolor(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    call1i(G_COLOR, *var_matr(var) as c_int);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gpolyline (count + packed xyz triples).
#[no_mangle]
pub unsafe extern "C" fn gra_gpolyline(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var + NEXT data live; void* data like the C macro.
    call1i1p(G_POLYLINE, *var_matr(var) as c_int, var_matr((*var).next) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gdraw/gmove (packed xyz data).
#[no_mangle]
pub unsafe extern "C" fn gra_gdraw(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    call1p(G_DRAW, var_matr(var) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}
// SAFETY: see gra_gdraw.
#[no_mangle]
pub unsafe extern "C" fn gra_gmove(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    call1p(G_MOVE, var_matr(var) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gpolymarker (index + count + packed xyz).
#[no_mangle]
pub unsafe extern "C" fn gra_gpolymarker(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: chain cells live.
    call2i1p(
        G_POLYMARKER,
        *var_matr(var) as c_int,
        *var_matr((*var).next) as c_int,
        var_matr((*(*var).next).next) as *mut core::ffi::c_void,
    );
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gmarker (index + packed xy).
#[no_mangle]
pub unsafe extern "C" fn gra_gmarker(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var + NEXT data live.
    call1i1p2(G_MARKER, *var_matr(var) as c_int, var_matr((*var).next) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gareafill (count + packed xy).
#[no_mangle]
pub unsafe extern "C" fn gra_gareafill(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var + NEXT data live.
    call1i1p2(G_AREAFILL, *var_matr(var) as c_int, var_matr((*var).next) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gtext (hr pair + string).
#[no_mangle]
pub unsafe extern "C" fn gra_gtext(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live; NEXT arg string fresh.
    let mut m = var_matr(var);
    let h = *m;
    m = m.offset(1);
    let r = *m;
    let s = var_to_string((*var).next);
    call2d1p(G_TEXT, h, r, s);
    // SAFETY: s from var_to_string above.
    mem_free(s as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gimage (whd triple + raster).
#[no_mangle]
pub unsafe extern "C" fn gra_gimage(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var + NEXT data live.
    let mut m = var_matr(var);
    let w = *m as c_int;
    m = m.offset(1);
    let h = *m as c_int;
    m = m.offset(1);
    let d = *m as c_int;
    call3i1p(G_IMAGE, w, h, d, var_matr((*var).next) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gwindow (six extents).
#[no_mangle]
pub unsafe extern "C" fn gra_gwindow(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (6 cells).
    let mut m = var_matr(var);
    let x1 = *m; m = m.offset(1);
    let x2 = *m; m = m.offset(1);
    let y1 = *m; m = m.offset(1);
    let y2 = *m; m = m.offset(1);
    let z1 = *m; m = m.offset(1);
    let z2 = *m;
    call6d(G_WINDOW, x1, x2, y1, y2, z1, z2);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gviewport (four extents).
#[no_mangle]
pub unsafe extern "C" fn gra_gviewport(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (4 cells).
    let mut m = var_matr(var);
    let x1 = *m; m = m.offset(1);
    let x2 = *m; m = m.offset(1);
    let y1 = *m; m = m.offset(1);
    let y2 = *m;
    call4d(G_VIEWPORT, x1, x2, y1, y2);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gtranslate/grotate/gscale (xyz triple).
#[no_mangle]
pub unsafe extern "C" fn gra_gtranslate(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (3 cells).
    let mut m = var_matr(var);
    let x = *m; m = m.offset(1);
    let y = *m; m = m.offset(1);
    let z = *m;
    call3d(G_TRANSLATE, x, y, z);
    core::ptr::null_mut()
}
// SAFETY: see gra_gtranslate.
#[no_mangle]
pub unsafe extern "C" fn gra_grotate(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (3 cells).
    let mut m = var_matr(var);
    let x = *m; m = m.offset(1);
    let y = *m; m = m.offset(1);
    let z = *m;
    call3d(G_ROTATE, x, y, z);
    core::ptr::null_mut()
}
// SAFETY: see gra_gtranslate.
#[no_mangle]
pub unsafe extern "C" fn gra_gscale(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (3 cells).
    let mut m = var_matr(var);
    let x = *m; m = m.offset(1);
    let y = *m; m = m.offset(1);
    let z = *m;
    call3d(G_SCALE, x, y, z);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gviewpoint (eye triple + optional target triple).
#[no_mangle]
pub unsafe extern "C" fn gra_gviewpoint(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (3 cells); NEXT triple when present.
    let mut m = var_matr(var);
    let xf = *m; m = m.offset(1);
    let yf = *m; m = m.offset(1);
    let zf = *m;
    let (mut xt, mut yt, mut zt) = (0.0, 0.0, 0.0);
    if !(*var).next.is_null() {
        let mut m2 = var_matr((*var).next);
        xt = *m2; m2 = m2.offset(1);
        yt = *m2; m2 = m2.offset(1);
        zt = *m2;
    }
    call6d(G_VIEWPOINT, xf, yf, zf, xt, yt, zt);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_ggetmatrix (fresh 4x4 filled from the driver).
#[no_mangle]
pub unsafe extern "C" fn gra_ggetmatrix(_var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(TYPE_DOUBLE, 4, 4);
    // SAFETY: res data live.
    call1p(G_GETMATRIX, var_matr(res) as *mut core::ffi::c_void);
    res
}

// SAFETY: mirrors C gra_gsetmatrix.
#[no_mangle]
pub unsafe extern "C" fn gra_gsetmatrix(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live (16 cells).
    call1p(G_SETMATRIX, var_matr(var) as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gperspective.
#[no_mangle]
pub unsafe extern "C" fn gra_gperspective(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    call1d(G_PERSPECTIVE, *var_matr(var));
    core::ptr::null_mut()
}

// SAFETY: mirrors C gra_gdbuffer/gsbuffer/gswapbuf (nullary buffer calls).
#[no_mangle]
pub unsafe extern "C" fn gra_gdbuffer(var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = var;
    call0(G_DBUFFER);
    core::ptr::null_mut()
}
// SAFETY: see gra_gdbuffer.
#[no_mangle]
pub unsafe extern "C" fn gra_gsbuffer(var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = var;
    call0(G_SBUFFER);
    core::ptr::null_mut()
}
// SAFETY: see gra_gdbuffer.
#[no_mangle]
pub unsafe extern "C" fn gra_gswapbuf(var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = var;
    call0(G_SWAPBUF);
    core::ptr::null_mut()
}

// SAFETY: registers all graphics commands (+ gc3d pair) with the
// byte-identical placeholder help texts.
#[no_mangle]
pub unsafe extern "C" fn gra_com_init() {
    // SAFETY: com_init copies into session memory; statics live forever.
    let help = b"Sorry, no help available!\0".as_ptr() as *const c_char;
    com_init(b"gopen\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gopen), 1, 2, help);
    com_init(b"gclose\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gclose), 0, 0, help);
    com_init(b"gclear\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gclear), 0, 0, help);
    com_init(b"gflush\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gflush), 0, 0, help);
    com_init(b"gdefcolor\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gdefcolor), 2, 2, help);
    com_init(b"gcolor\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gcolor), 1, 1, help);
    com_init(b"gpolyline\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gpolyline), 2, 2, help);
    com_init(b"gdraw\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gdraw), 1, 1, help);
    com_init(b"gmove\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gmove), 1, 1, help);
    com_init(b"gpolymarker\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gpolymarker), 3, 3, help);
    com_init(b"gmarker\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gmarker), 2, 2, help);
    com_init(b"gareafill\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gareafill), 2, 2, help);
    com_init(b"gimage\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gimage), 2, 2, help);
    com_init(b"gtext\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gtext), 2, 2, help);
    com_init(b"gwindow\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gwindow), 1, 1, help);
    com_init(b"gviewport\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gviewport), 1, 1, help);
    com_init(b"gtranslate\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gtranslate), 1, 1, help);
    com_init(b"grotate\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_grotate), 1, 1, help);
    com_init(b"gscale\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gscale), 1, 1, help);
    com_init(b"gviewpoint\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gviewpoint), 1, 2, help);
    com_init(b"gdbuffer\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gdbuffer), 0, 0, help);
    com_init(b"gsbuffer\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gsbuffer), 0, 0, help);
    com_init(b"gswapbuf\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gswapbuf), 0, 0, help);
    com_init(b"ggetmatrix\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_ggetmatrix), 0, 0, help);
    com_init(b"gsetmatrix\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gsetmatrix), 1, 1, help);
    com_init(b"gperspective\0".as_ptr() as *const c_char, FALSE, FALSE, Some(gra_gperspective), 1, 1, help);
    com_init(b"gc3d\0".as_ptr() as *const c_char, FALSE, FALSE, Some(c3d_gc3d), 1, 1, help);
    com_init(b"gc3dlevels\0".as_ptr() as *const c_char, FALSE, FALSE, Some(c3d_gc3dlevels), 1, 1, help);
}
