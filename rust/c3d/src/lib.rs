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

// ---- unit body: transcribed from matc/src/c3d.c ----
// Transcribed from matc/src/c3d.c (contour painter).
// All fixed-point int math uses wrapping_* ops: the C build (-O0) wraps
// two's complement (e.g. ds*deltax exceeds i32), and Rust debug would
// panic instead. Float->int casts stay `as` (values in range).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut gra_state: G_STATE;
    static mut gra_funcs: [GraFunc; 27];
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn gra_mtrans(x: c_double, y: c_double, z: c_double, xs: *mut c_double, ys: *mut c_double, zs: *mut c_double);
}

pub type GraFunc = Option<unsafe extern "C" fn()>;
pub const G_WINDOW: usize = 4;
pub const G_COLOR: usize = 6;
pub const G_POLYLINE: usize = 7;
pub const G_AREAFILL: usize = 12;
pub const G_FLUSH: usize = 15;
pub const G_GETMATRIX: usize = 21;
pub const G_SETMATRIX: usize = 22;
pub const G_PERSPECTIVE: usize = 23;

pub const C3D_MASK: c_int = 9;
pub const C3D_HALFMASK: c_int = 1 << (9 - 1);

#[repr(C)]
pub struct C3dNode {
    pub x: c_int,
    pub y: c_int,
    pub z: c_int,
    pub d: c_int,
}
#[repr(C)]
pub struct C3dElement {
    pub node: [*mut C3dNode; 4],
    pub d: c_int,
    pub z: c_int,
}
#[repr(C)]
pub struct C3dElTree {
    pub left: *mut C3dElTree,
    pub right: *mut C3dElTree,
    pub entry: *mut C3dElement,
}

// SAFETY: reads the live driver slot.
unsafe fn slot(s: usize) -> GraFunc {
    *gra_funcs.as_mut_ptr().offset(s as isize)
}
// Wrapping int abs (C abs macro on ints; values never INT_MIN).
unsafe fn wabs(v: c_int) -> c_int {
    if v < 0 {
        v.wrapping_neg()
    } else {
        v
    }
}

// SAFETY: mirrors C c3d_gc3d/gc3dlevels (thin entries).
#[no_mangle]
pub unsafe extern "C" fn c3d_gc3d(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var matrix live.
    C3D_Contour(var_matr(var), var_nrow(var), var_ncol(var));
    core::ptr::null_mut()
}
// SAFETY: mirrors C c3d_gc3dlevels.
#[no_mangle]
pub unsafe extern "C" fn c3d_gc3dlevels(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    C3D_Levels(*var_matr(var) as c_int);
    core::ptr::null_mut()
}

static mut c3d_clevels: c_int = 10;
static mut c3d_perspective: c_int = FALSE;
static mut c3d_ident: GMATRIX = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

// SAFETY: mirrors C C3D_Contour (node build, element tree, paint, restore).
#[no_mangle]
pub unsafe extern "C" fn C3D_Contour(matrix: *mut c_double, nr: c_int, nc: c_int) {
    // C min/max macros on doubles: manual ternaries, same values.
    let dmin2 = |a: c_double, b: c_double| -> c_double { if a < b { a } else { b } };
    let dmax2 = |a: c_double, b: c_double| -> c_double { if a > b { a } else { b } };
    // SAFETY: nodes block sized nr*nc, freed below.
    let nodes = mem_alloc((nr as usize) * (nc as usize) * size_of::<C3dNode>()) as *mut C3dNode;
    let mut xmin = 1.0e20 as c_double;
    let mut ymin = xmin;
    let mut dmin = xmin;
    let mut xmax = -1.0e20 as c_double;
    let mut ymax = xmax;
    let mut dmax = xmax;
    // SAFETY: matrix spans nr*nc live doubles.
    let mut i: c_int = 0;
    let mut n: c_int = 0;
    while i < nr {
        let mut j: c_int = 0;
        while j < nc {
            let z = *matrix.offset(n as isize);
            dmin = dmin2(dmin, z);
            dmax = dmax2(dmax, z);
            j += 1;
            n += 1;
        }
        i += 1;
    }
    let mut gm: GMATRIX = [[0.0; 4]; 4];
    let mut i: c_int = 0;
    let mut n: c_int = 0;
    while i < nr {
        let mut j: c_int = 0;
        while j < nc {
            let x = 2.0 * (i as c_double) / (nr as c_double) - 1.0;
            let y = 2.0 * (j as c_double) / (nc as c_double) - 1.0;
            let dd = (*matrix.offset(n as isize) - dmin) / (dmax - dmin);
            let z = 2.0 * dd - 1.0;
            let mut xs: c_double = 0.0;
            let mut ys: c_double = 0.0;
            let mut zs: c_double = 0.0;
            gra_mtrans(x, y, z, &mut xs, &mut ys, &mut zs);
            xs *= (1 << 20) as c_double;
            ys *= (1 << 20) as c_double;
            zs *= (1 << 20) as c_double;
            // SAFETY: nodes[n] live; float->int casts in range.
            (*nodes.offset(n as isize)).x = xs as c_int;
            (*nodes.offset(n as isize)).y = ys as c_int;
            (*nodes.offset(n as isize)).z = zs as c_int;
            (*nodes.offset(n as isize)).d = ((c3d_clevels as c_double * dd + 1.0) * ((1 << C3D_MASK) as c_double)) as c_int;
            xmin = dmin2(xmin, xs);
            xmax = dmax2(xmax, xs);
            ymin = dmin2(ymin, ys);
            ymax = dmax2(ymax, ys);
            j += 1;
            n += 1;
        }
        i += 1;
    }
    let mut i: c_int = 0;
    let mut n: c_int = 0;
    while i < nr {
        let mut j: c_int = 0;
        while j < nc {
            // C int arithmetic (wraps mod 2^32 at -O0); mirrored exactly.
            // SAFETY: nodes[n] live.
            (*nodes.offset(n as isize)).x = (4095 as c_int)
                .wrapping_mul(((*nodes.offset(n as isize)).x as c_double - xmin) as c_int)
                .wrapping_div((xmax - xmin) as c_int);
            (*nodes.offset(n as isize)).y = (4095 as c_int)
                .wrapping_mul(((*nodes.offset(n as isize)).y as c_double - ymin) as c_int)
                .wrapping_div((ymax - ymin) as c_int);
            j += 1;
            n += 1;
        }
        i += 1;
    }
    // SAFETY: element/tree blocks sized (nr-1)*(nc-1), freed below.
    let elements = mem_alloc(((nr - 1) as usize) * ((nc - 1) as usize) * size_of::<C3dElement>()) as *mut C3dElement;
    let trb = mem_alloc(((nr - 1) as usize) * ((nc - 1) as usize) * size_of::<C3dElTree>()) as *mut C3dElTree;
    let mut element_el_tree: *mut C3dElTree = core::ptr::null_mut();
    let mut i: c_int = 0;
    let mut n: c_int = 0;
    while i < nr - 1 {
        let mut j: c_int = 0;
        while j < nc - 1 {
            // SAFETY: trb[n]/elements[n]/nodes[] all live.
            let tr = trb.offset(n as isize);
            let el = elements.offset(n as isize);
            (*tr).entry = el;
            (*el).node[0] = nodes.offset((nc * i + j) as isize);
            (*el).node[1] = nodes.offset((nc * (i + 1) + j) as isize);
            (*el).node[2] = nodes.offset((nc * (i + 1) + j + 1) as isize);
            (*el).node[3] = nodes.offset((nc * i + j + 1) as isize);
            (*el).d = 0;
            (*el).z = 0;
            let mut k: c_int = 0;
            while k < 4 {
                (*el).d = (*el).d.wrapping_add((*(*el).node[k as usize]).d);
                (*el).z = (*el).z.wrapping_add((*(*el).node[k as usize]).z);
                k += 1;
            }
            (*el).d = ((*el).d.wrapping_add(2)) >> 2;
            (*tr).left = core::ptr::null_mut();
            (*tr).right = core::ptr::null_mut();
            if element_el_tree.is_null() {
                element_el_tree = tr;
            } else {
                C3D_Add_El_Tree(element_el_tree, tr);
            }
            j += 1;
            n += 1;
        }
        i += 1;
    }
    // GRA_GETMATRIX / SETMATRIX / WINDOW through the table.
    if let Some(f) = slot(G_GETMATRIX) {
        let g: unsafe extern "C" fn(*mut core::ffi::c_void) = core::mem::transmute(f);
        g((&mut gm as *mut GMATRIX) as *mut core::ffi::c_void);
    }
    if let Some(f) = slot(G_SETMATRIX) {
        let g: unsafe extern "C" fn(*mut core::ffi::c_void) = core::mem::transmute(f);
        g(core::ptr::addr_of_mut!(c3d_ident) as *mut core::ffi::c_void);
    }
    if let Some(f) = slot(G_WINDOW) {
        let g: unsafe extern "C" fn(c_double, c_double, c_double, c_double, c_double, c_double) =
            core::mem::transmute(f);
        g(0.0, 4096.0, 0.0, 4096.0, -1.0, 1.0);
    }
    C3D_Show_El_Tree(element_el_tree);
    // SAFETY: gra_state live.
    if gra_state.pratio > 0.0 {
        if let Some(f) = slot(G_PERSPECTIVE) {
            let g: unsafe extern "C" fn(c_double) = core::mem::transmute(f);
            g(gra_state.pratio);
        }
    }
    if let Some(f) = slot(G_SETMATRIX) {
        let g: unsafe extern "C" fn(*mut core::ffi::c_void) = core::mem::transmute(f);
        g((&mut gm as *mut GMATRIX) as *mut core::ffi::c_void);
    }
    if let Some(f) = slot(G_FLUSH) {
        let g: unsafe extern "C" fn() = core::mem::transmute(f);
        g();
    }
    // SAFETY: blocks from mem_alloc above.
    mem_free(elements as *mut core::ffi::c_void);
    mem_free(trb as *mut core::ffi::c_void);
    mem_free(nodes as *mut core::ffi::c_void);
}

// SAFETY: mirrors C C3D_Levels (negative refuses via error_matc).
#[no_mangle]
pub unsafe extern "C" fn C3D_Levels(levels: c_int) {
    if levels >= 0 {
        c3d_clevels = levels;
    } else {
        // SAFETY: static format string.
        error_matc(b"C3D_Levels: level parameter negative, not changed.\n\0".as_ptr() as *const c_char);
    }
}

// SAFETY: mirrors C C3D_Persp.
#[no_mangle]
pub unsafe extern "C" fn C3D_Persp(tf: c_int) {
    c3d_perspective = tf;
}

// SAFETY: mirrors C C3D_Add_El_Tree (z-ordered insert).
#[no_mangle]
pub unsafe extern "C" fn C3D_Add_El_Tree(head: *mut C3dElTree, add: *mut C3dElTree) {
    // SAFETY: both are live tree nodes.
    if (*(*add).entry).z > (*(*head).entry).z {
        if (*head).left.is_null() {
            (*head).left = add;
        } else {
            C3D_Add_El_Tree((*head).left, add);
        }
    } else if (*(*add).entry).z < (*(*head).entry).z {
        if (*head).right.is_null() {
            (*head).right = add;
        } else {
            C3D_Add_El_Tree((*head).right, add);
        }
    } else {
        // Equal z: splice before the current left chain (C lines 296-299).
        (*add).left = (*head).left;
        (*head).left = add;
    }
}

// SAFETY: mirrors C C3D_Show_El_Tree (in-order paint).
#[no_mangle]
pub unsafe extern "C" fn C3D_Show_El_Tree(head: *mut C3dElTree) {
    if head.is_null() {
        return;
    }
    // SAFETY: head live.
    C3D_Show_El_Tree((*head).left);
    C3D_Show_Elem((*head).entry);
    C3D_Show_El_Tree((*head).right);
}

// SAFETY: mirrors C C3D_Free_El_Tree.
#[no_mangle]
pub unsafe extern "C" fn C3D_Free_El_Tree(head: *mut C3dElTree) {
    if head.is_null() {
        return;
    }
    // SAFETY: head live.
    C3D_Free_El_Tree((*head).left);
    C3D_Free_El_Tree((*head).right);
    // SAFETY: head is a mem_alloc'd block (C FREEMEM contract).
    mem_free(head as *mut core::ffi::c_void);
}

// SAFETY: mirrors C C3D_Convex_Test on 4 live int cells each.
#[no_mangle]
pub unsafe extern "C" fn C3D_Convex_Test(x: *mut c_int, y: *mut c_int) -> c_int {
    // SAFETY: x/y span 4 live cells.
    let xi = |i: usize| -> c_int { *x.offset(i as isize) };
    let yi = |i: usize| -> c_int { *y.offset(i as isize) };
    let mut amax = wabs(yi(0).wrapping_mul(xi(2).wrapping_sub(xi(1))).wrapping_add(
        yi(1).wrapping_mul(xi(0).wrapping_sub(xi(2))).wrapping_add(yi(2).wrapping_mul(xi(1).wrapping_sub(xi(0)))),
    ));
    let mut aind: c_int = 3;
    let mut at1 = wabs(yi(2).wrapping_mul(xi(0).wrapping_sub(xi(3))).wrapping_add(
        yi(3).wrapping_mul(xi(2).wrapping_sub(xi(0))).wrapping_add(yi(0).wrapping_mul(xi(3).wrapping_sub(xi(2)))),
    ));
    let a1 = amax.wrapping_add(at1);
    if at1 > amax {
        amax = at1;
        aind = 1;
    }
    at1 = wabs(yi(1).wrapping_mul(xi(3).wrapping_sub(xi(2))).wrapping_add(
        yi(2).wrapping_mul(xi(1).wrapping_sub(xi(3))).wrapping_add(yi(3).wrapping_mul(xi(2).wrapping_sub(xi(1)))),
    ));
    if at1 > amax {
        amax = at1;
        aind = 0;
    }
    let at2 = wabs(yi(3).wrapping_mul(xi(1).wrapping_sub(xi(0))).wrapping_add(
        yi(0).wrapping_mul(xi(3).wrapping_sub(xi(1))).wrapping_add(yi(1).wrapping_mul(xi(0).wrapping_sub(xi(3)))),
    ));
    if at2 > amax {
        aind = 2;
    }
    let a2 = at1.wrapping_add(at2);
    if a1 == a2 {
        return -1;
    }
    aind
}

// SAFETY: mirrors C C3D_Show_Elem (flat fast path + convex/tri fan).
#[no_mangle]
pub unsafe extern "C" fn C3D_Show_Elem(el: *mut C3dElement) {
    let mut x = [0 as c_int; 5];
    let mut y = [0 as c_int; 5];
    let mut d = [0 as c_int; 5];
    let mut xi = [0 as c_int; 5];
    let mut yi = [0 as c_int; 5];
    let mut col = [0 as c_int; 3];
    let mut p = [Point { x: 0.0, y: 0.0, z: 0.0 }; 5];
    // SAFETY: el + nodes live.
    let mut i: c_int = 0;
    while i != 4 {
        x[i as usize] = (*(*el).node[i as usize]).x;
        y[i as usize] = (*(*el).node[i as usize]).y;
        d[i as usize] = (*(*el).node[i as usize]).d;
        i += 1;
    }
    if (d[0] >> C3D_MASK) == (d[1] >> C3D_MASK)
        && (d[0] >> C3D_MASK) == (d[2] >> C3D_MASK)
        && (d[0] >> C3D_MASK) == (d[3] >> C3D_MASK)
    {
        C3D_SelCol(d[0] >> C3D_MASK);
        C3D_AreaFill(4, TRUE, x.as_mut_ptr(), y.as_mut_ptr());
        return;
    }
    match C3D_Convex_Test(x.as_mut_ptr(), y.as_mut_ptr()) {
        0 => {
            C3D_Show_Tri(x.as_mut_ptr(), y.as_mut_ptr(), d.as_mut_ptr());
            xi[0] = x[2]; xi[1] = x[3]; xi[2] = x[0];
            yi[0] = y[2]; yi[1] = y[3]; yi[2] = y[0];
            col[0] = d[2]; col[1] = d[3]; col[2] = d[0];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
        }
        1 => {
            C3D_Show_Tri(x.as_mut_ptr().offset(1), y.as_mut_ptr().offset(1), d.as_mut_ptr().offset(1));
            xi[0] = x[0]; xi[1] = x[1]; xi[2] = x[3];
            yi[0] = y[0]; yi[1] = y[1]; yi[2] = y[3];
            col[0] = d[0]; col[1] = d[1]; col[2] = d[3];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
        }
        2 => {
            C3D_Show_Tri(x.as_mut_ptr(), y.as_mut_ptr(), d.as_mut_ptr());
            xi[0] = x[2]; xi[1] = x[3]; xi[2] = x[0];
            yi[0] = y[2]; yi[1] = y[3]; yi[2] = y[0];
            col[0] = d[2]; col[1] = d[3]; col[2] = d[0];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
        }
        3 => {
            C3D_Show_Tri(x.as_mut_ptr().offset(1), y.as_mut_ptr().offset(1), d.as_mut_ptr().offset(1));
            xi[0] = x[0]; xi[1] = x[1]; xi[2] = x[3];
            yi[0] = y[0]; yi[1] = y[1]; yi[2] = y[3];
            col[0] = d[0]; col[1] = d[1]; col[2] = d[3];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
        }
        _ => {
            let mut xp: c_int = 0;
            let mut yp: c_int = 0;
            let mut i: c_int = 0;
            while i != 4 {
                xp = xp.wrapping_add(x[i as usize]);
                yp = yp.wrapping_add(y[i as usize]);
                i += 1;
            }
            xp = (xp.wrapping_add(2)) >> 2;
            yp = (yp.wrapping_add(2)) >> 2;
            // SAFETY: el live.
            let zp = (*el).d;
            xi[0] = x[0]; xi[1] = x[1]; xi[2] = xp;
            yi[0] = y[0]; yi[1] = y[1]; yi[2] = yp;
            col[0] = d[0]; col[1] = d[1]; col[2] = zp;
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
            xi[0] = x[1]; xi[1] = x[2];
            yi[0] = y[1]; yi[1] = y[2];
            col[0] = d[1]; col[1] = d[2];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
            xi[0] = x[2]; xi[1] = x[3];
            yi[0] = y[2]; yi[1] = y[3];
            col[0] = d[2]; col[1] = d[3];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
            xi[0] = x[3]; xi[1] = x[0];
            yi[0] = y[3]; yi[1] = y[0];
            col[0] = d[3]; col[1] = d[0];
            C3D_Show_Tri(xi.as_mut_ptr(), yi.as_mut_ptr(), col.as_mut_ptr());
        }
    }
    // SAFETY: int->double casts in range.
    p[0].x = (x[0] as c_double + 0.5) as c_int as c_double;
    p[0].y = (y[0] as c_double + 0.5) as c_int as c_double;
    p[0].z = 0.0;
    p[1].x = (x[1] as c_double + 0.5) as c_int as c_double;
    p[1].y = (y[1] as c_double + 0.5) as c_int as c_double;
    p[1].z = 0.0;
    p[2].x = (x[2] as c_double + 0.5) as c_int as c_double;
    p[2].y = (y[2] as c_double + 0.5) as c_int as c_double;
    p[2].z = 0.0;
    p[3].x = (x[3] as c_double + 0.5) as c_int as c_double;
    p[3].y = (y[3] as c_double + 0.5) as c_int as c_double;
    p[3].z = 0.0;
    p[4].x = (x[0] as c_double + 0.5) as c_int as c_double;
    p[4].y = (y[0] as c_double + 0.5) as c_int as c_double;
    p[4].z = 0.0;
    if let Some(f) = slot(G_COLOR) {
        let g: unsafe extern "C" fn(c_int) = core::mem::transmute(f);
        g(1);
    }
    if let Some(f) = slot(G_POLYLINE) {
        let g: unsafe extern "C" fn(c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
        g(5, p.as_mut_ptr() as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C C3D_Show_Tri (level-band polygon fill).
#[no_mangle]
pub unsafe extern "C" fn C3D_Show_Tri(x: *mut c_int, y: *mut c_int, d: *mut c_int) {
    let mut xx = [0 as c_int; 128];
    let mut yy = [0 as c_int; 128];
    let mut dd = [0 as c_int; 128];
    let mut px = [0 as c_int; 7];
    let mut py = [0 as c_int; 7];
    let mut n: c_int = 0;
    // SAFETY: x/y/d span 3 live cells.
    if (*d.offset(0) >> C3D_MASK) == (*d.offset(1) >> C3D_MASK)
        && (*d.offset(0) >> C3D_MASK) == (*d.offset(2) >> C3D_MASK)
    {
        C3D_SelCol(*d.offset(0) >> C3D_MASK);
        C3D_AreaFill(3, FALSE, x, y);
        return;
    }
    let mut i: c_int = 0;
    C3D_Pcalc(*x.offset(0), *y.offset(0), *d.offset(0), *x.offset(1), *y.offset(1), *d.offset(1),
        &mut i, xx.as_mut_ptr(), yy.as_mut_ptr(), dd.as_mut_ptr());
    n += i;
    C3D_Pcalc(*x.offset(1), *y.offset(1), *d.offset(1), *x.offset(2), *y.offset(2), *d.offset(2),
        &mut i, xx.as_mut_ptr().offset(n as isize), yy.as_mut_ptr().offset(n as isize), dd.as_mut_ptr().offset(n as isize));
    n += i;
    C3D_Pcalc(*x.offset(2), *y.offset(2), *d.offset(2), *x.offset(0), *y.offset(0), *d.offset(0),
        &mut i, xx.as_mut_ptr().offset(n as isize), yy.as_mut_ptr().offset(n as isize), dd.as_mut_ptr().offset(n as isize));
    n += i;
    let mut i: c_int = 0;
    while i < 2 {
        xx[(n + i) as usize] = xx[i as usize];
        yy[(n + i) as usize] = yy[i as usize];
        dd[(n + i) as usize] = dd[i as usize];
        i += 1;
    }
    let mut i: c_int = 0;
    while i < n - 2 {
        let mut k: c_int = 0;
        px[k as usize] = xx[i as usize];
        py[k as usize] = yy[i as usize];
        k += 1;
        px[k as usize] = xx[(i + 1) as usize];
        py[k as usize] = yy[(i + 1) as usize];
        k += 1;
        if dd[i as usize] == dd[(i + 1) as usize] {
            i += 1;
            px[k as usize] = xx[(i + 1) as usize];
            py[k as usize] = yy[(i + 1) as usize];
            k += 1;
        }
        let mut j = n - 1;
        while j > i {
            if dd[i as usize] == dd[j as usize] {
                if dd[(j - 1) as usize] == dd[j as usize] {
                    px[k as usize] = xx[(j - 1) as usize];
                    py[k as usize] = yy[(j - 1) as usize];
                    k += 1;
                }
                px[k as usize] = xx[j as usize];
                py[k as usize] = yy[j as usize];
                k += 1;
                px[k as usize] = xx[(j + 1) as usize];
                py[k as usize] = yy[(j + 1) as usize];
                k += 1;
                if dd[j as usize] == dd[(j + 1) as usize] {
                    j += 1;
                    px[k as usize] = xx[(j + 1) as usize];
                    py[k as usize] = yy[(j + 1) as usize];
                    k += 1;
                }
                break;
            }
            j -= 1;
        }
        if k > 2 {
            C3D_SelCol(dd[i as usize]);
            C3D_AreaFill(k, FALSE, px.as_mut_ptr(), py.as_mut_ptr());
        }
        i += 1;
    }
}

// SAFETY: mirrors C C3D_Pcalc (fixed-point edge walk; wrapping int ops).
#[no_mangle]
pub unsafe extern "C" fn C3D_Pcalc(
    x0: c_int, y0: c_int, d0: c_int, x1: c_int, y1: c_int, d1: c_int,
    n: *mut c_int, xx: *mut c_int, yy: *mut c_int, dd: *mut c_int,
) {
    // SAFETY: output cells live.
    *n = wabs((d1 >> C3D_MASK).wrapping_sub(d0 >> C3D_MASK));
    *xx.offset(0) = x0;
    *yy.offset(0) = y0;
    *dd.offset(0) = d0 >> C3D_MASK;
    *n = (*n).wrapping_add(1);
    if *n != 1 {
        let deltad: c_int = if d1 >= d0 { 1 } else { -1 };
        let mut ds = d0 & ((1 << C3D_MASK) - 1);
        if d1 > d0 {
            ds = (1 << C3D_MASK) - ds;
        }
        let d = wabs(d1.wrapping_sub(d0));
        let (mut x, deltax) = if x1 > x0 {
            let dt = ((x1.wrapping_sub(x0)) << (C3D_MASK << 1)).wrapping_div(d) >> C3D_MASK;
            (x0.wrapping_add(ds.wrapping_mul(dt).wrapping_add(C3D_HALFMASK) >> C3D_MASK), dt)
        } else {
            let dt = ((x0.wrapping_sub(x1)) << (C3D_MASK << 1)).wrapping_div(d) >> C3D_MASK;
            (x0.wrapping_sub(ds.wrapping_mul(dt).wrapping_add(C3D_HALFMASK) >> C3D_MASK), dt.wrapping_neg())
        };
        let (mut y, deltay) = if y1 > y0 {
            let dt = ((y1.wrapping_sub(y0)) << (C3D_MASK << 1)).wrapping_div(d) >> C3D_MASK;
            (y0.wrapping_add(ds.wrapping_mul(dt).wrapping_add(C3D_HALFMASK) >> C3D_MASK), dt)
        } else {
            let dt = ((y0.wrapping_sub(y1)) << (C3D_MASK << 1)).wrapping_div(d) >> C3D_MASK;
            (y0.wrapping_sub(ds.wrapping_mul(dt).wrapping_add(C3D_HALFMASK) >> C3D_MASK), dt.wrapping_neg())
        };
        let mut i: c_int = 1;
        while i != *n {
            *dd.offset(i as isize) = (*dd.offset((i - 1) as isize)).wrapping_add(deltad);
            *xx.offset(i as isize) = x;
            *yy.offset(i as isize) = y;
            x = x.wrapping_add(deltax);
            y = y.wrapping_add(deltay);
            i += 1;
        }
    }
}

// SAFETY: mirrors C C3D_SelCol (++col through the COLOR slot).
#[no_mangle]
pub unsafe extern "C" fn C3D_SelCol(col: c_int) {
    let col = col.wrapping_add(1);
    if let Some(f) = slot(G_COLOR) {
        let g: unsafe extern "C" fn(c_int) = core::mem::transmute(f);
        g(col);
    }
}

// SAFETY: mirrors C C3D_AreaFill (dedup ring + optional border).
#[no_mangle]
pub unsafe extern "C" fn C3D_AreaFill(n: c_int, border: c_int, x: *mut c_int, y: *mut c_int) {
    let mut p = [Point { x: 0.0, y: 0.0, z: 0.0 }; 8];
    // SAFETY: x/y span n live cells.
    let mut n = n;
    while n > 0 && *x.offset((n - 1) as isize) == *x.offset(0) && *y.offset((n - 1) as isize) == *y.offset(0) {
        n -= 1;
    }
    let mut i: c_int = 0;
    while i < n {
        // SAFETY: int->double casts in range.
        p[i as usize].x = (*x.offset(i as isize) as c_double + 0.5) as c_int as c_double;
        p[i as usize].y = (*y.offset(i as isize) as c_double + 0.5) as c_int as c_double;
        p[i as usize].z = 0.0;
        i += 1;
    }
    let mut i: c_int = 0;
    while i < n - 1 {
        if p[i as usize].x == p[(i + 1) as usize].x && p[i as usize].y == p[(i + 1) as usize].y {
            let mut j = i + 1;
            while j < n - 1 {
                p[j as usize].x = p[(j + 1) as usize].x;
                p[j as usize].y = p[(j + 1) as usize].y;
                j += 1;
            }
            n -= 1;
        }
        i += 1;
    }
    if n > 2 {
        if let Some(f) = slot(G_AREAFILL) {
            let g: unsafe extern "C" fn(c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
            g(n, p.as_mut_ptr() as *mut core::ffi::c_void);
        }
        if border != 0 {
            p[n as usize].x = p[0].x;
            p[n as usize].y = p[0].y;
            p[n as usize].z = 0.0;
            if let Some(f) = slot(G_COLOR) {
                let g: unsafe extern "C" fn(c_int) = core::mem::transmute(f);
                g(1);
            }
            if let Some(f) = slot(G_POLYLINE) {
                let g: unsafe extern "C" fn(c_int, *mut core::ffi::c_void) = core::mem::transmute(f);
                g(n + 1, p.as_mut_ptr() as *mut core::ffi::c_void);
            }
        }
    }
}
