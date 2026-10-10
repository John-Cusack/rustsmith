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

// ---- unit body: transcribed from matc/src/dri_ps.c ----
// Transcribed from matc/src/dri_ps.c (PostScript graphics driver).
// NOTE: gra_ps_polyline reads xn[n]/yn[n] (one past the mem_alloc'd row,
// a latent C OOB read). This port keeps the same read against the same
// C-heap layout (identical allocation pattern), so it observes the same
// bytes the C build does; no corpus prints them.

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut gra_state: G_STATE;
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn gra_mtrans(x: c_double, y: c_double, z: c_double, xs: *mut c_double, ys: *mut c_double, zs: *mut c_double);
    fn gra_window_to_viewport(x: c_double, y: c_double, z: c_double, xs: *mut c_double, ys: *mut c_double);
    fn clip_line(n: *mut c_int, x: *mut c_double, y: *mut c_double) -> c_int;
    fn clip_poly(n: *mut c_int, x: *mut c_double, y: *mut c_double) -> c_int;
    fn gra_close_sys();
}

pub const GRA_PS_FILE: &[u8] = b"matc.ps\0";
pub const GRA_PS_MAXC: usize = 16;
pub const GRA_DRV_NULL: c_int = 0;
pub const CL_XMIN: c_double = -1.0;
pub const CL_XMAX: c_double = 1.0;
pub const CL_YMIN: c_double = -1.0;
pub const CL_YMAX: c_double = 1.0;

static mut gra_ps_rgb: [[c_uchar; 3]; GRA_PS_MAXC] = [
    [255, 255, 255],
    [0, 0, 0],
    [0, 83, 255],
    [0, 166, 255],
    [0, 255, 255],
    [0, 83, 0],
    [0, 166, 0],
    [0, 255, 0],
    [255, 255, 0],
    [255, 166, 0],
    [255, 83, 0],
    [255, 0, 0],
    [255, 0, 255],
    [255, 83, 255],
    [255, 166, 255],
    [0, 0, 0],
];
static mut sh: c_double = -1.0;
static mut fs: c_double = -1.0;
static mut pip180: c_double = 3.14158 / 180.0;

// SAFETY: mirrors C gra_ps_open; writes the PS header to matc.ps.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_open(dev: c_int) {
    let _ = dev;
    // SAFETY: gra_state is the live graphics state (matc crate owns it).
    if gra_state.out_fp.is_null() {
        // SAFETY: fopen for writing; NULL-checked like C.
        let f = fopen(GRA_PS_FILE.as_ptr() as *const c_char, b"w\0".as_ptr() as *const c_char);
        if f.is_null() {
            gra_state.driver = GRA_DRV_NULL;
            // SAFETY: static format string.
            error_matc(b"gra: open: Can't open output file...\n\0".as_ptr() as *const c_char);
        }
        gra_state.out_fp = f;
    }
    // SAFETY: out_fp is a live writable stream from here on.
    let fp = gra_state.out_fp;
    fprintf(fp, b"%%!PS-Adobe-1.0\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/m { moveto } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/l { lineto } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/d { stroke } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/t { show } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/c { setrgbcolor } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/p { eofill } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/f { findfont } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/h { scalefont } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/x { setfont } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/w { setlinewidth } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/s { gsave } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/r { grestore } def\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"/a { rotate } def\n\0".as_ptr() as *const c_char);
    fprintf(
        fp,
        b"gsave clippath pathbbox 2 copy lt { exch } if 0.9 mul dup scale 0.07 dup translate\n\0".as_ptr() as *const c_char,
    );
    fprintf(fp, b"%g w\n\0".as_ptr() as *const c_char, 0.001 as c_double);
    let mut i: c_int = 0;
    while (i as usize) < GRA_PS_MAXC {
        gra_ps_defcolor(
            i,
            gra_ps_rgb[i as usize][0] as c_double / 255.0,
            gra_ps_rgb[i as usize][1] as c_double / 255.0,
            gra_ps_rgb[i as usize][2] as c_double / 255.0,
        );
        i += 1;
    }
    fprintf(fp, b"newpath\n\0".as_ptr() as *const c_char);
    fprintf(fp, b"c1\n\0".as_ptr() as *const c_char);
    sh = -1.0;
}

// SAFETY: mirrors C gra_ps_close/clear/flush/reset.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_close() {
    // SAFETY: out_fp live (opened by gra_ps_open like the C path).
    fprintf(gra_state.out_fp, b"showpage grestore\n\0".as_ptr() as *const c_char);
    gra_close_sys();
}
// SAFETY: mirrors C gra_ps_clear.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_clear() {
    // SAFETY: gra_state live.
    gra_state.cur_point.x = 0.0;
    gra_state.cur_point.y = 0.0;
}
// SAFETY: no-op like the C original.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_flush() {}
// SAFETY: no-op like the C original.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_reset() {}

// SAFETY: mirrors C gra_ps_defcolor.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_defcolor(index: c_int, r: c_double, g: c_double, b: c_double) {
    // SAFETY: out_fp live; static format with double args.
    fprintf(
        gra_state.out_fp,
        b"/c%d {%.3g %.3g %.3g c} def\n\0".as_ptr() as *const c_char,
        index, r, g, b,
    );
    // SAFETY: gra_state live.
    if gra_state.cur_color == index {
        // SAFETY: out_fp live; static format with int arg.
        fprintf(gra_state.out_fp, b"c%d\n\0".as_ptr() as *const c_char, index);
    }
}

// SAFETY: mirrors C gra_ps_color.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_color(index: c_int) {
    // SAFETY: gra_state live.
    if gra_state.cur_color != index {
        // SAFETY: out_fp live.
        fprintf(gra_state.out_fp, b"c%d\n\0".as_ptr() as *const c_char, index);
        gra_state.cur_color = index;
    }
}

// SAFETY: mirrors C gra_ps_polyline, including the one-past-end xn[n]
// read (see file note: same C-heap layout, same observed bytes).
#[no_mangle]
pub unsafe extern "C" fn gra_ps_polyline(n: c_int, p: *mut Point) {
    // SAFETY: p spans n live Points.
    if n > 1 {
        // SAFETY: C-heap double rows, freed below.
        let xn = mem_alloc((n as usize) * size_of::<c_double>()) as *mut c_double;
        let yn = mem_alloc((n as usize) * size_of::<c_double>()) as *mut c_double;
        let mut zn: c_double = 0.0;
        let mut i: c_int = 1;
        while i < n {
            let mut xi: c_double = 0.0;
            let mut yi: c_double = 0.0;
            // SAFETY: Point fields live; output cells live.
            gra_mtrans((*p.offset(i as isize)).x, (*p.offset(i as isize)).y, (*p.offset(i as isize)).z, &mut xi, &mut yi, &mut zn);
            *xn.offset(i as isize) = xi;
            *yn.offset(i as isize) = yi;
            i += 1;
        }
        // One-past-end read, exactly like the C original (see file note).
        // SAFETY: same allocator, same pattern, same adjacent bytes.
        gra_state.cur_point.x = *xn.offset(n as isize);
        gra_state.cur_point.y = *yn.offset(n as isize);
        let mut nn = n;
        let mut ni: c_int = 0;
        while nn > 1 {
            let mut xi: c_double = 0.0;
            let mut yi: c_double = 0.0;
            // SAFETY: Point p[ni] live; output cells live.
            gra_mtrans((*p.offset(ni as isize)).x, (*p.offset(ni as isize)).y, (*p.offset(ni as isize)).z, &mut xi, &mut yi, &mut zn);
            *xn.offset(ni as isize) = xi;
            *yn.offset(ni as isize) = yi;
            if clip_line(&mut nn, xn.offset(ni as isize), yn.offset(ni as isize)) > 1 {
                let mut vx: c_double = 0.0;
                let mut vy: c_double = 0.0;
                gra_window_to_viewport(*xn.offset(ni as isize), *yn.offset(ni as isize), zn, &mut vx, &mut vy);
                // SAFETY: out_fp live.
                fprintf(gra_state.out_fp, b"%.3g %.3g m\n\0".as_ptr() as *const c_char, vx, vy);
                let mut np: c_int = 0;
                let mut i: c_int = 1;
                while i < nn {
                    gra_window_to_viewport(*xn.offset((i + ni) as isize), *yn.offset((i + ni) as isize), zn, &mut vx, &mut vy);
                    np += 1;
                    if np > 32 && i != n - 1 {
                        // SAFETY: out_fp live.
                        fprintf(
                            gra_state.out_fp,
                            b"%.3g %.3g l %.3g %.3g m\n\0".as_ptr() as *const c_char,
                            vx, vy, vx, vy,
                        );
                        np = 0;
                    } else {
                        // SAFETY: out_fp live.
                        fprintf(gra_state.out_fp, b"%.3g %.3g l\n\0".as_ptr() as *const c_char, vx, vy);
                    }
                    i += 1;
                }
                // SAFETY: out_fp live.
                fprintf(gra_state.out_fp, b"d\n\0".as_ptr() as *const c_char);
                ni += nn - 1;
            } else {
                ni += 1;
            }
            nn = n - ni;
        }
        // SAFETY: xn/yn from mem_alloc above.
        mem_free(yn as *mut core::ffi::c_void);
        mem_free(xn as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C gra_ps_draw.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_draw(p: *mut Point) {
    let mut xn = [0.0 as c_double; 2];
    let mut yn = [0.0 as c_double; 2];
    let mut zn: c_double = 0.0;
    let mut n: c_int = 2;
    // SAFETY: gra_state live.
    xn[0] = gra_state.cur_point.x;
    yn[0] = gra_state.cur_point.y;
    let mut x1: c_double = 0.0;
    let mut y1: c_double = 0.0;
    // SAFETY: p[0] live; output cells are stack slots.
    gra_mtrans((*p).x, (*p).y, (*p).z, &mut x1, &mut y1, &mut zn);
    xn[1] = x1;
    yn[1] = y1;
    // SAFETY: gra_state live.
    gra_state.cur_point.x = xn[1];
    gra_state.cur_point.y = yn[1];
    if clip_line(&mut n, xn.as_mut_ptr(), yn.as_mut_ptr()) > 1 {
        let mut vx: c_double = 0.0;
        let mut vy: c_double = 0.0;
        gra_window_to_viewport(xn[0], yn[0], zn, &mut vx, &mut vy);
        // SAFETY: out_fp live.
        fprintf(gra_state.out_fp, b"%.3g %.3g m \0".as_ptr() as *const c_char, vx, vy);
        gra_window_to_viewport(xn[1], yn[1], zn, &mut vx, &mut vy);
        // SAFETY: out_fp live.
        fprintf(gra_state.out_fp, b"%.3g %.3g l d\n\0".as_ptr() as *const c_char, vx, vy);
    }
}

// SAFETY: mirrors C gra_ps_move.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_move(p: *mut Point) {
    let mut wx: c_double = 0.0;
    let mut wy: c_double = 0.0;
    let mut wz: c_double = 0.0;
    // SAFETY: p[0] live; outputs are stack slots.
    gra_mtrans((*p).x, (*p).y, (*p).z, &mut wx, &mut wy, &mut wz);
    // SAFETY: gra_state live.
    gra_state.cur_point.x = wx;
    gra_state.cur_point.y = wy;
}

// SAFETY: mirrors C gra_ps_polymarker (x/y allocs are unused scratch in
// the original too; kept to preserve the allocation pattern).
#[no_mangle]
pub unsafe extern "C" fn gra_ps_polymarker(index: c_int, n: c_int, p: *mut Point) {
    // SAFETY: gra_state live.
    if gra_state.cur_marker != index {
        gra_state.cur_marker = index;
    }
    if n > 0 {
        // SAFETY: C-heap int scratch, freed below (allocation pattern kept).
        let x = mem_alloc((n as usize) * size_of::<c_int>()) as *mut c_int;
        let y = mem_alloc((n as usize) * size_of::<c_int>()) as *mut c_int;
        let mut nm: c_int = 0;
        let mut i: c_int = 0;
        while i < n {
            let mut wx: c_double = 0.0;
            let mut wy: c_double = 0.0;
            let mut wz: c_double = 0.0;
            // SAFETY: p[i] live; outputs are stack slots.
            gra_mtrans((*p.offset(i as isize)).x, (*p.offset(i as isize)).y, (*p.offset(i as isize)).z, &mut wx, &mut wy, &mut wz);
            // SAFETY: gra_state live.
            gra_state.cur_point.x = wx;
            gra_state.cur_point.y = wy;
            if wx >= CL_XMIN && wx <= CL_XMAX && wy >= CL_YMIN && wy <= CL_YMAX {
                let mut vx: c_double = 0.0;
                let mut vy: c_double = 0.0;
                gra_window_to_viewport(wx, wy, wz, &mut vx, &mut vy);
                nm += 1;
            }
            i += 1;
        }
        let _ = (x, y, nm);
        // SAFETY: x/y from mem_alloc above.
        mem_free(x as *mut core::ffi::c_void);
        mem_free(y as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C gra_ps_marker.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_marker(index: c_int, p: *mut Point) {
    let _ = index;
    let mut wx: c_double = 0.0;
    let mut wy: c_double = 0.0;
    let mut wz: c_double = 0.0;
    // SAFETY: p[0] live; outputs are stack slots.
    gra_mtrans((*p).x, (*p).y, (*p).z, &mut wx, &mut wy, &mut wz);
    // SAFETY: gra_state live.
    gra_state.cur_point.x = wx;
    gra_state.cur_point.y = wy;
}

// SAFETY: mirrors C gra_ps_areafill.
#[no_mangle]
pub unsafe extern "C" fn gra_ps_areafill(n: c_int, p: *mut Point) {
    if n > 2 {
        // SAFETY: C-heap rows sized (2n+2), freed below.
        let xn = mem_alloc(((2 * n + 2) as usize) * size_of::<c_double>()) as *mut c_double;
        let yn = mem_alloc(((2 * n + 2) as usize) * size_of::<c_double>()) as *mut c_double;
        let mut zn: c_double = 0.0;
        let mut i: c_int = 0;
        while i < n {
            let mut xi: c_double = 0.0;
            let mut yi: c_double = 0.0;
            // SAFETY: p[i] live; row cells live.
            gra_mtrans((*p.offset(i as isize)).x, (*p.offset(i as isize)).y, (*p.offset(i as isize)).z, &mut xi, &mut yi, &mut zn);
            *xn.offset(i as isize) = xi;
            *yn.offset(i as isize) = yi;
            i += 1;
        }
        // SAFETY: gra_state live.
        gra_state.cur_point.x = *xn.offset(0);
        gra_state.cur_point.y = *yn.offset(0);
        let mut nn = n;
        clip_poly(&mut nn, xn, yn);
        if nn > 2 {
            let mut vx: c_double = 0.0;
            let mut vy: c_double = 0.0;
            gra_window_to_viewport(*xn.offset(0), *yn.offset(0), zn, &mut vx, &mut vy);
            // SAFETY: out_fp live.
            fprintf(gra_state.out_fp, b"%.3g %.3g m\n\0".as_ptr() as *const c_char, vx, vy);
            let mut i: c_int = 1;
            while i < nn {
                gra_window_to_viewport(*xn.offset(i as isize), *yn.offset(i as isize), zn, &mut vx, &mut vy);
                // SAFETY: out_fp live.
                fprintf(gra_state.out_fp, b"%.3g %.3g l\n\0".as_ptr() as *const c_char, vx, vy);
                i += 1;
            }
            // SAFETY: out_fp live.
            fprintf(gra_state.out_fp, b"p\n\0".as_ptr() as *const c_char);
        }
        // SAFETY: xn/yn from mem_alloc above.
        mem_free(yn as *mut core::ffi::c_void);
        mem_free(xn as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C gra_ps_image (%02x hex raster dump).
#[no_mangle]
pub unsafe extern "C" fn gra_ps_image(w: c_int, h: c_int, d: c_int, r: *mut c_uchar) {
    if d != 8 {
        // SAFETY: static format string. No return: error_matc diverges
        // (C has a defensive return after a noreturn call).
        error_matc(b"gra: ps: driver does (currently) support only 8 bits/pixel.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: out_fp live.
    let fp = gra_state.out_fp;
    // SAFETY: static formats; int/double args from live state.
    fprintf(fp, b"gsave\n/picstr %d string def\n\0".as_ptr() as *const c_char, w);
    fprintf(
        fp,
        b"%.3g %.3g translate %.3g %.3g scale\n\0".as_ptr() as *const c_char,
        gra_state.viewport.xlow,
        gra_state.viewport.ylow,
        gra_state.viewport.xhigh - gra_state.viewport.xlow,
        gra_state.viewport.yhigh - gra_state.viewport.ylow,
    );
    fprintf(fp, b"%d %d %d [%d 0 0 %d 0 0]\n\0".as_ptr() as *const c_char, w, h, d, w, h);
    fprintf(fp, b"{ currentfile picstr readhexstring pop } image\n\0".as_ptr() as *const c_char);
    let mut rr = r;
    let mut i: c_int = 0;
    let mut k: c_int = 0;
    while i < h {
        let mut j: c_int = 0;
        while j < w {
            // SAFETY: raster row live (w*h bytes).
            fprintf(fp, b"%02x\0".as_ptr() as *const c_char, *rr as c_int);
            rr = rr.offset(1);
            k += 1;
            if k >= 40 {
                // SAFETY: static format.
                fprintf(fp, b"\n\0".as_ptr() as *const c_char);
                k = 0;
            }
            j += 1;
        }
        i += 1;
    }
    // SAFETY: static format.
    fprintf(fp, b" grestore\n\0".as_ptr() as *const c_char);
}

// SAFETY: mirrors C gra_ps_text (font caching via sh/fs statics).
#[no_mangle]
pub unsafe extern "C" fn gra_ps_text(h: c_double, r: c_double, s: *mut c_char) {
    // SAFETY: gra_state live.
    let wx = gra_state.cur_point.x;
    let wy = gra_state.cur_point.y;
    let wz = 0.0;
    if !(wx >= CL_XMIN && wx <= CL_XMAX && wy >= CL_YMIN && wy <= CL_YMAX) {
        return;
    }
    let mut vx: c_double = 0.0;
    let mut vy: c_double = 0.0;
    gra_window_to_viewport(wx, wy, wz, &mut vx, &mut vy);
    // SAFETY: out_fp live.
    fprintf(gra_state.out_fp, b"%.3g %.3g m\n\0".as_ptr() as *const c_char, vx, vy);
    // SAFETY: file statics sh/fs mirror the C originals.
    if sh != h {
        fs = (gra_state.viewport.xhigh - gra_state.viewport.xlow)
            / (gra_state.window.xhigh - gra_state.window.xlow);
        fs *= 1.65 * h;
        sh = h;
        // SAFETY: out_fp live; %g double.
        fprintf(gra_state.out_fp, b"/Times-Roman f %g h x\n\0".as_ptr() as *const c_char, fs);
    }
    if r != 0.0 {
        // SAFETY: out_fp live; s is a live C string.
        fprintf(gra_state.out_fp, b"s %.3g a (%s) t r\n\0".as_ptr() as *const c_char, r, s);
    } else {
        // SAFETY: out_fp live; s is a live C string.
        fprintf(gra_state.out_fp, b"(%s) t\n\0".as_ptr() as *const c_char, s);
    }
    // SAFETY: strlen on the live string s; file statics mirror C.
    gra_state.cur_point.x += cos(r * pip180) * fs * strlen(s) as c_double;
    gra_state.cur_point.y += sin(r * pip180) * fs * strlen(s) as c_double;
}
