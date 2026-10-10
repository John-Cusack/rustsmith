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

// ---- unit body: transcribed from matc/src/gra.c ----
// Transcribed from matc/src/gra.c (2D/3D transform core + device init).
// GRA_DRV_PS is the only compiled driver (IRIS/TEKLIB ifdefs are dead
// upstream); the switch keeps case 4 plus the default error like the C.

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut gra_state: G_STATE;
    static mut gra_funcs: [GraFunc; 27];
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn gra_ps_open(dev: c_int);
    fn gra_ps_close();
    fn gra_ps_clear();
    fn gra_ps_defcolor(index: c_int, r: c_double, g: c_double, b: c_double);
    fn gra_ps_color(index: c_int);
    fn gra_ps_polyline(n: c_int, p: *mut Point);
    fn gra_ps_draw(p: *mut Point);
    fn gra_ps_move(p: *mut Point);
    fn gra_ps_polymarker(index: c_int, n: c_int, p: *mut Point);
    fn gra_ps_marker(index: c_int, p: *mut Point);
    fn gra_ps_areafill(n: c_int, p: *mut Point);
    fn gra_ps_image(w: c_int, h: c_int, d: c_int, r: *mut c_uchar);
    fn gra_ps_text(h: c_double, r: c_double, s: *mut c_char);
    fn gra_ps_flush();
    fn gra_ps_reset();
}

// Untyped driver slot (C `void(*)()`), transmuted at store/call like the
// C function-pointer conversions.
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
pub const GRA_DRV_PS: c_int = 4;

static mut gra_vsx: c_double = 0.0;
static mut gra_vsy: c_double = 0.0;
static mut gra_vtx: c_double = 0.0;
static mut gra_vty: c_double = 0.0;
// C function-local static in gra_rotate, hoisted (same one-time value).
static mut gra_rotate_pip180: c_double = 3.1415926535898 / 180.0;

// SAFETY: stores a typed driver fn into the untyped slot (same conversion
// the C assignment performs).
unsafe fn set_func(slot: usize, f: GraFunc) {
    *gra_funcs.as_mut_ptr().offset(slot as isize) = f;
}
// SAFETY: transmutes a concrete driver fn to the untyped slot type.
unsafe fn uf<F>(f: F) -> GraFunc
where
    F: Copy,
{
    // SAFETY: all driver fns share the C ABI; the slot is only ever called
    // back through its matching GRA_* macro type below.
    core::mem::transmute_copy::<F, GraFunc>(&f)
}

// SAFETY: mirrors C gra_init_matc (PS driver wiring + default view).
#[no_mangle]
pub unsafe extern "C" fn gra_init_matc(devtype: c_int, nm: *mut c_char) {
    // SAFETY: gra_state/gra_funcs are the live driver state.
    if gra_state.driver != 0 {
        // GRA_CLOSE(): call slot 1 as fn().
        if let Some(f) = *gra_funcs.as_mut_ptr().offset(G_CLOSE as isize) {
            let g: unsafe extern "C" fn() = core::mem::transmute(f);
            g();
        }
    }
    if !nm.is_null() {
        // SAFETY: fopen on the live name; NULL-checked like C.
        let f = fopen(nm as *const c_char, b"w\0".as_ptr() as *const c_char);
        if f.is_null() {
            // SAFETY: static format string.
            error_matc(b"gra: open: Can't open named output stream\n\0".as_ptr() as *const c_char);
        }
        gra_state.out_fp = f;
    }
    set_func(G_VIEWPORT, uf(gra_set_viewport as unsafe extern "C" fn(c_double, c_double, c_double, c_double)));
    set_func(G_WINDOW, uf(gra_set_window as unsafe extern "C" fn(c_double, c_double, c_double, c_double, c_double, c_double)));
    set_func(G_PERSPECTIVE, uf(gra_perspective as unsafe extern "C" fn(c_double)));
    set_func(G_TRANSLATE, uf(gra_translate as unsafe extern "C" fn(c_double, c_double, c_double)));
    set_func(G_ROTATE, uf(gra_rotate as unsafe extern "C" fn(c_double, c_double, c_double)));
    set_func(G_SCALE, uf(gra_scale as unsafe extern "C" fn(c_double, c_double, c_double)));
    set_func(G_VIEWPOINT, uf(gra_viewpoint as unsafe extern "C" fn(c_double, c_double, c_double, c_double, c_double, c_double)));
    set_func(G_GETMATRIX, uf(gra_getmatrix as unsafe extern "C" fn(*mut GMATRIX)));
    set_func(G_SETMATRIX, uf(gra_setmatrix as unsafe extern "C" fn(*mut GMATRIX)));
    set_func(G_DBUFFER, uf(gra_dbuffer_null as unsafe extern "C" fn()));
    set_func(G_SBUFFER, uf(gra_dbuffer_null as unsafe extern "C" fn()));
    set_func(G_SWAPBUF, uf(gra_dbuffer_null as unsafe extern "C" fn()));
    if devtype == 4 {
        set_func(G_OPEN, uf(gra_ps_open as unsafe extern "C" fn(c_int)));
        set_func(G_CLOSE, uf(gra_ps_close as unsafe extern "C" fn()));
        set_func(G_CLEAR, uf(gra_ps_clear as unsafe extern "C" fn()));
        set_func(G_DEFCOLOR, uf(gra_ps_defcolor as unsafe extern "C" fn(c_int, c_double, c_double, c_double)));
        set_func(G_COLOR, uf(gra_ps_color as unsafe extern "C" fn(c_int)));
        set_func(G_POLYLINE, uf(gra_ps_polyline as unsafe extern "C" fn(c_int, *mut Point)));
        set_func(G_DRAW, uf(gra_ps_draw as unsafe extern "C" fn(*mut Point)));
        set_func(G_MOVE, uf(gra_ps_move as unsafe extern "C" fn(*mut Point)));
        set_func(G_POLYMARKER, uf(gra_ps_polymarker as unsafe extern "C" fn(c_int, c_int, *mut Point)));
        set_func(G_MARKER, uf(gra_ps_marker as unsafe extern "C" fn(c_int, *mut Point)));
        set_func(G_AREAFILL, uf(gra_ps_areafill as unsafe extern "C" fn(c_int, *mut Point)));
        set_func(G_IMAGE, uf(gra_ps_image as unsafe extern "C" fn(c_int, c_int, c_int, *mut c_uchar)));
        set_func(G_TEXT, uf(gra_ps_text as unsafe extern "C" fn(c_double, c_double, *mut c_char)));
        set_func(G_FLUSH, uf(gra_ps_flush as unsafe extern "C" fn()));
        set_func(G_RESET, uf(gra_ps_reset as unsafe extern "C" fn()));
        gra_state.driver = GRA_DRV_PS;
    } else {
        // SAFETY: static format string.
        error_matc(b"gra: Unknown device selection\n\0".as_ptr() as *const c_char);
    }
    // GRA_OPEN(devtype): call slot 0 as fn(int).
    if let Some(f) = *gra_funcs.as_mut_ptr().offset(G_OPEN as isize) {
        let g: unsafe extern "C" fn(c_int) = core::mem::transmute(f);
        g(devtype);
    }
    gra_ident(core::ptr::addr_of_mut!(gra_state.modelm));
    gra_ident(core::ptr::addr_of_mut!(gra_state.viewm));
    gra_ident(core::ptr::addr_of_mut!(gra_state.projm));
    gra_ident(core::ptr::addr_of_mut!(gra_state.transfm));
    // GRA_WINDOW(-1,1,...): slot 4 as fn(6 doubles).
    if let Some(f) = *gra_funcs.as_mut_ptr().offset(G_WINDOW as isize) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double, c_double, c_double, c_double) =
            core::mem::transmute(f);
        g(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
    }
    // GRA_VIEWPORT(0,1,0,1): slot 3 as fn(4 doubles).
    if let Some(f) = *gra_funcs.as_mut_ptr().offset(G_VIEWPORT as isize) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double, c_double) = core::mem::transmute(f);
        g(0.0, 1.0, 0.0, 1.0);
    }
    gra_state.pratio = 0.0;
}

// SAFETY: mirrors C gra_close_sys (close stream, reset table to gra_error).
#[no_mangle]
pub unsafe extern "C" fn gra_close_sys() {
    // SAFETY: gra_state/gra_funcs live.
    if !gra_state.out_fp.is_null() {
        // SAFETY: out_fp is a live open stream.
        fclose(gra_state.out_fp);
        gra_state.out_fp = core::ptr::null_mut();
    }
    let mut i: usize = 0;
    while i < GRA_FUNCS {
        *gra_funcs.as_mut_ptr().offset(i as isize) = uf(gra_error as unsafe extern "C" fn());
        i += 1;
    }
    gra_state.driver = 0;
}

// SAFETY: no-op like the C original.
#[no_mangle]
pub unsafe extern "C" fn gra_dbuffer_null() {}

// SAFETY: mirrors C gra_getmatrix/gra_setmatrix (128-byte copies).
#[no_mangle]
pub unsafe extern "C" fn gra_getmatrix(gm: *mut GMATRIX) {
    // SAFETY: gm points at 128 live bytes; transfm live.
    memcpy(
        gm as *mut core::ffi::c_void,
        core::ptr::addr_of!(gra_state.transfm) as *const core::ffi::c_void,
        size_of::<GMATRIX>(),
    );
}
// SAFETY: mirrors C gra_setmatrix (copy + reset view stack).
#[no_mangle]
pub unsafe extern "C" fn gra_setmatrix(gm: *mut GMATRIX) {
    // SAFETY: gm points at 128 live bytes; transfm live.
    memcpy(
        core::ptr::addr_of_mut!(gra_state.transfm) as *mut core::ffi::c_void,
        gm as *const core::ffi::c_void,
        size_of::<GMATRIX>(),
    );
    gra_ident(core::ptr::addr_of_mut!(gra_state.modelm));
    gra_ident(core::ptr::addr_of_mut!(gra_state.projm));
    gra_ident(core::ptr::addr_of_mut!(gra_state.viewm));
}

// SAFETY: mirrors C gra_set_transfm (model*view*proj product).
#[no_mangle]
pub unsafe extern "C" fn gra_set_transfm() {
    // SAFETY: gra_state live.
    let mut i: c_int = 0;
    while i < 4 {
        let mut j: c_int = 0;
        while j < 4 {
            gra_state.transfm[i as usize][j as usize] = gra_state.modelm[i as usize][j as usize];
            j += 1;
        }
        i += 1;
    }
    gra_mult(
        core::ptr::addr_of_mut!(gra_state.transfm),
        core::ptr::addr_of_mut!(gra_state.viewm),
    );
    gra_mult(
        core::ptr::addr_of_mut!(gra_state.transfm),
        core::ptr::addr_of_mut!(gra_state.projm),
    );
}

// SAFETY: mirrors C gra_mult (4x4 product into gm1).
#[no_mangle]
pub unsafe extern "C" fn gra_mult(gm1: *mut GMATRIX, gm2: *mut GMATRIX) {
    // SAFETY: both point at live 4x4 doubles.
    let mut s = [0.0 as c_double; 4];
    let mut i: c_int = 0;
    while i < 4 {
        let mut j: c_int = 0;
        while j < 4 {
            s[j as usize] = 0.0;
            let mut k: c_int = 0;
            while k < 4 {
                s[j as usize] += (*gm1)[i as usize][k as usize] * (*gm2)[k as usize][j as usize];
                k += 1;
            }
            j += 1;
        }
        let mut j: c_int = 0;
        while j < 4 {
            (*gm1)[i as usize][j as usize] = s[j as usize];
            j += 1;
        }
        i += 1;
    }
}

// SAFETY: mirrors C gra_ident (4x4 identity).
#[no_mangle]
pub unsafe extern "C" fn gra_ident(gm: *mut GMATRIX) {
    // SAFETY: gm points at a live 4x4.
    (*gm)[0][0] = 1.0; (*gm)[0][1] = 0.0; (*gm)[0][2] = 0.0; (*gm)[0][3] = 0.0;
    (*gm)[1][0] = 0.0; (*gm)[1][1] = 1.0; (*gm)[1][2] = 0.0; (*gm)[1][3] = 0.0;
    (*gm)[2][0] = 0.0; (*gm)[2][1] = 0.0; (*gm)[2][2] = 1.0; (*gm)[2][3] = 0.0;
    (*gm)[3][0] = 0.0; (*gm)[3][1] = 0.0; (*gm)[3][2] = 0.0; (*gm)[3][3] = 1.0;
}

// SAFETY: mirrors C gra_viewpoint (view matrix from eye/target).
#[no_mangle]
pub unsafe extern "C" fn gra_viewpoint(
    mut xf: c_double,
    mut yf: c_double,
    mut zf: c_double,
    xt: c_double,
    yt: c_double,
    zt: c_double,
) {
    let mut gvm: GMATRIX = [[0.0; 4]; 4];
    // SAFETY: gra_state live.
    gra_ident(core::ptr::addr_of_mut!(gra_state.viewm));
    gra_state.viewm[3][0] = -xf;
    gra_state.viewm[3][1] = -yf;
    gra_state.viewm[3][2] = -zf;
    xf = xf - xt;
    yf = yf - yt;
    zf = zf - zt;
    gra_ident(&mut gvm);
    gvm[1][2] = -1.0;
    gvm[2][1] = 1.0;
    gvm[1][1] = 0.0;
    gvm[2][2] = 0.0;
    gra_mult(core::ptr::addr_of_mut!(gra_state.viewm), &mut gvm);
    let r1 = sqrt(xf * xf + yf * yf);
    if r1 != 0.0 {
        gra_ident(&mut gvm);
        gvm[0][0] = -yf / r1;
        gvm[2][2] = gvm[0][0];
        gvm[0][2] = xf / r1;
        gvm[2][0] = -gvm[0][2];
        gra_mult(core::ptr::addr_of_mut!(gra_state.viewm), &mut gvm);
    }
    let r2 = sqrt(yf * yf + zf * zf);
    if r2 != 0.0 {
        gra_ident(&mut gvm);
        gvm[1][1] = r1 / r2;
        gvm[2][2] = gvm[1][1];
        gvm[1][2] = zf / r2;
        gvm[2][1] = -gvm[1][2];
        gra_mult(core::ptr::addr_of_mut!(gra_state.viewm), &mut gvm);
    }
    gra_ident(&mut gvm);
    gvm[2][2] = -1.0;
    gra_mult(core::ptr::addr_of_mut!(gra_state.viewm), &mut gvm);
    gra_set_transfm();
}

// SAFETY: mirrors C gra_rotate (XYZ euler steps on the model matrix).
#[no_mangle]
pub unsafe extern "C" fn gra_rotate(mut rx: c_double, mut ry: c_double, mut rz: c_double) {
    let mut grm: GMATRIX = [[0.0; 4]; 4];
    // SAFETY: hoisted fn static mirrors the C function-local static.
    rx *= gra_rotate_pip180;
    gra_ident(&mut grm);
    grm[1][1] = cos(rx);
    grm[1][2] = -sin(rx);
    grm[2][1] = sin(rx);
    grm[2][2] = cos(rx);
    // SAFETY: gra_state live.
    gra_mult(core::ptr::addr_of_mut!(gra_state.modelm), &mut grm);
    ry *= gra_rotate_pip180;
    gra_ident(&mut grm);
    grm[0][0] = cos(ry);
    grm[0][2] = sin(ry);
    grm[2][0] = -sin(ry);
    grm[2][2] = cos(ry);
    gra_mult(core::ptr::addr_of_mut!(gra_state.modelm), &mut grm);
    rz *= gra_rotate_pip180;
    gra_ident(&mut grm);
    grm[0][0] = cos(rz);
    grm[0][1] = -sin(rz);
    grm[1][0] = sin(rz);
    grm[1][1] = cos(rz);
    gra_mult(core::ptr::addr_of_mut!(gra_state.modelm), &mut grm);
    gra_set_transfm();
}

// SAFETY: mirrors C gra_scale.
#[no_mangle]
pub unsafe extern "C" fn gra_scale(sx: c_double, sy: c_double, sz: c_double) {
    let mut gsm: GMATRIX = [[0.0; 4]; 4];
    gra_ident(&mut gsm);
    gsm[0][0] = sx;
    gsm[1][1] = sy;
    gsm[2][2] = sz;
    // SAFETY: gra_state live.
    gra_mult(core::ptr::addr_of_mut!(gra_state.modelm), &mut gsm);
    gra_set_transfm();
}

// SAFETY: mirrors C gra_translate.
#[no_mangle]
pub unsafe extern "C" fn gra_translate(tx: c_double, ty: c_double, tz: c_double) {
    let mut gtm: GMATRIX = [[0.0; 4]; 4];
    gra_ident(&mut gtm);
    gtm[3][0] = tx;
    gtm[3][1] = ty;
    gtm[3][2] = tz;
    // SAFETY: gra_state live.
    gra_mult(core::ptr::addr_of_mut!(gra_state.modelm), &mut gtm);
    gra_set_transfm();
}

// SAFETY: mirrors C gra_perspective.
#[no_mangle]
pub unsafe extern "C" fn gra_perspective(r: c_double) {
    // SAFETY: gra_state live.
    gra_ident(core::ptr::addr_of_mut!(gra_state.projm));
    gra_state.projm[0][0] = r;
    gra_state.projm[1][1] = r;
    gra_state.pratio = r;
    gra_set_transfm();
}

// SAFETY: mirrors C gra_set_proj (viewport scale/translate factors).
#[no_mangle]
pub unsafe extern "C" fn gra_set_proj() {
    // SAFETY: gra_state + file statics mirror the C originals.
    gra_vsx = (gra_state.viewport.xhigh - gra_state.viewport.xlow) / 2.0;
    gra_vsy = (gra_state.viewport.yhigh - gra_state.viewport.ylow) / 2.0;
    gra_vtx = gra_state.viewport.xlow + gra_vsx;
    gra_vty = gra_state.viewport.ylow + gra_vsy;
}

// SAFETY: mirrors C gra_set_window (projection from world box).
#[no_mangle]
pub unsafe extern "C" fn gra_set_window(
    x1: c_double,
    x2: c_double,
    y1: c_double,
    y2: c_double,
    z1: c_double,
    z2: c_double,
) {
    let mut gvm: GMATRIX = [[0.0; 4]; 4];
    // SAFETY: gra_state live.
    gra_state.window.xlow = x1;
    gra_state.window.xhigh = x2;
    gra_state.window.ylow = y1;
    gra_state.window.yhigh = y2;
    gra_state.window.zlow = z1;
    gra_state.window.zhigh = z2;
    gra_ident(core::ptr::addr_of_mut!(gra_state.projm));
    gra_state.projm[0][0] = 2.0 / (x2 - x1);
    gra_state.projm[1][1] = 2.0 / (y2 - y1);
    gra_state.projm[2][2] = 2.0 / (z2 - z1);
    gra_ident(&mut gvm);
    gvm[3][0] = -1.0 - gra_state.projm[0][0] * x1;
    gvm[3][1] = -1.0 - gra_state.projm[1][1] * y1;
    gvm[3][2] = -1.0 - gra_state.projm[2][2] * z1;
    gra_mult(core::ptr::addr_of_mut!(gra_state.projm), &mut gvm);
    gra_state.pratio = 0.0;
    gra_set_transfm();
}

// SAFETY: mirrors C gra_set_viewport.
#[no_mangle]
pub unsafe extern "C" fn gra_set_viewport(x1: c_double, x2: c_double, y1: c_double, y2: c_double) {
    // SAFETY: gra_state live.
    gra_state.viewport.xlow = x1;
    gra_state.viewport.xhigh = x2;
    gra_state.viewport.ylow = y1;
    gra_state.viewport.yhigh = y2;
    gra_set_proj();
}

// SAFETY: mirrors C gra_mtrans (model-view-proj transform + perspective).
#[no_mangle]
pub unsafe extern "C" fn gra_mtrans(
    x: c_double,
    y: c_double,
    z: c_double,
    xe: *mut c_double,
    ye: *mut c_double,
    ze: *mut c_double,
) {
    // SAFETY: gra_state live; output cells are caller slots.
    *xe = x * gra_state.transfm[0][0] + y * gra_state.transfm[1][0] + z * gra_state.transfm[2][0]
        + gra_state.transfm[3][0];
    *ye = x * gra_state.transfm[0][1] + y * gra_state.transfm[1][1] + z * gra_state.transfm[2][1]
        + gra_state.transfm[3][1];
    *ze = x * gra_state.transfm[0][2] + y * gra_state.transfm[1][2] + z * gra_state.transfm[2][2]
        + gra_state.transfm[3][2];
    if gra_state.pratio > 0.0 && *ze != 0.0 {
        *xe /= *ze;
        *ye /= *ze;
    }
}

// SAFETY: mirrors C gra_window_to_viewport (affine viewport map).
#[no_mangle]
pub unsafe extern "C" fn gra_window_to_viewport(
    x: c_double,
    y: c_double,
    z: c_double,
    xs: *mut c_double,
    ys: *mut c_double,
) {
    let _ = z;
    // SAFETY: file statics mirror the C originals; outputs are caller slots.
    *xs = gra_vsx * x + gra_vtx;
    *ys = gra_vsy * y + gra_vty;
}

// SAFETY: mirrors C gra_error (uninitialized-package refusal).
#[no_mangle]
pub unsafe extern "C" fn gra_error() {
    // SAFETY: static format string.
    error_matc(b"gra: graphics package not initialized\n\0".as_ptr() as *const c_char);
}
