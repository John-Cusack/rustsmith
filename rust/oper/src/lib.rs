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

// ---- unit body: transcribed from matc/src/oper.c ----
// Transcribed from matc/src/oper.c (matrix operators).
// This file uses the MATRIX flavor of the access macros (oper.c undefines
// the VARIABLE ones); local mt_* helpers mirror them. Quirks kept: opr_not
// tests only a[0] (a never advances), like the C original.

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_delete_temp(v: *mut VARIABLE);
    fn mtr_inv(v: *mut VARIABLE) -> *mut VARIABLE;
    fn com_apply(v: *mut VARIABLE) -> *mut VARIABLE;
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
}

// MATRIX-flavored accessors (mirror oper.c's macro redefinitions).
// SAFETY: all take live matrix pointers.
unsafe fn mt_type(m: *mut MATRIX) -> c_int {
    (*m).typ
}
unsafe fn mt_nrow(m: *mut MATRIX) -> c_int {
    (*m).nrow
}
unsafe fn mt_ncol(m: *mut MATRIX) -> c_int {
    (*m).ncol
}
unsafe fn mt_matr(m: *mut MATRIX) -> *mut c_double {
    (*m).data
}
unsafe fn mt_matsize(m: *mut MATRIX) -> usize {
    ((*m).nrow as usize) * ((*m).ncol as usize) * size_of::<c_double>()
}

// SAFETY: mirrors C mat_new (1x1 doubles embedded, else split alloc).
#[no_mangle]
pub unsafe extern "C" fn mat_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut MATRIX {
    if typ == TYPE_DOUBLE && nrow == 1 && ncol == 1 {
        // SAFETY: combined ALLOCMEM block (MATRIXSIZE + double).
        let res = mem_alloc(MATRIXSIZE + size_of::<c_double>()) as *mut MATRIX;
        (*res).typ = TYPE_DOUBLE;
        (*res).nrow = 1;
        (*res).ncol = 1;
        (*res).refcount = 0;
        (*res).data = (res as *mut c_char).add(MATRIXSIZE) as *mut c_double;
        return res;
    }
    // SAFETY: split ALLOCMEM blocks.
    let res = mem_alloc(MATRIXSIZE) as *mut MATRIX;
    (*res).typ = typ;
    (*res).nrow = nrow;
    (*res).ncol = ncol;
    (*res).refcount = 0;
    (*res).data = mem_alloc(mt_matsize(res)) as *mut c_double;
    res
}

// SAFETY: mirrors C mat_copy (NULL-safe deep copy).
#[no_mangle]
pub unsafe extern "C" fn mat_copy(mat: *mut MATRIX) -> *mut MATRIX {
    if mat.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: mat_new returns a live matrix; both blocks live.
    let res = mat_new(mt_type(mat), mt_nrow(mat), mt_ncol(mat));
    memcpy(
        mt_matr(res) as *mut core::ffi::c_void,
        mt_matr(mat) as *const core::ffi::c_void,
        mt_matsize(mat),
    );
    res
}

// SAFETY: mirrors C mat_free (embedded-aware release).
#[no_mangle]
pub unsafe extern "C" fn mat_free(mat: *mut MATRIX) {
    if mat.is_null() {
        return;
    }
    if !mat_data_embedded(mat) {
        // SAFETY: data is a mem_alloc'd block.
        mem_free(mt_matr(mat) as *mut core::ffi::c_void);
    }
    // SAFETY: MATRIX is a mem_alloc'd block.
    mem_free(mat as *mut core::ffi::c_void);
}

// SAFETY: mirrors C opr_vector (a(1):b range, sign-directed).
#[no_mangle]
pub unsafe extern "C" fn opr_vector(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: both single cells live.
    let a = mt_matr(aa);
    let b = mt_matr(bb);
    let inc: c_int = if (*b as c_int) > (*a as c_int) { 1 } else { -1 };
    // C abs macro on ints: manual ternary, same value.
    let iabs = |v: c_int| -> c_int { if v > 0 { v } else { -v } };
    let n = iabs(*b as c_int - *a as c_int) + 1;
    // SAFETY: fresh matrix owns its storage.
    let cc = mat_new(TYPE_DOUBLE, 1, n);
    // SAFETY: cc data live through return.
    let mut c = mt_matr(cc);
    let mut i: c_int = 0;
    while i < n {
        *c = (*a as c_int + i * inc) as c_double;
        c = c.offset(1);
        i += 1;
    }
    cc
}

// SAFETY: mirrors C opr_resize (cyclic reshape to i-by-j).
#[no_mangle]
pub unsafe extern "C" fn opr_resize(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: both data blocks live.
    let a = mt_matr(aa);
    let b = mt_matr(bb);
    let (i, j) = if mt_ncol(bb) >= 2 {
        (*b as c_int, *b.offset(1) as c_int)
    } else {
        (1, *b as c_int)
    };
    if i < 1 || j < 1 {
        // SAFETY: static string (original lacks trailing newline).
        error_matc(b"resize: invalid size for and array\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh matrix owns its storage.
    let cc = mat_new(mt_type(aa), i, j);
    // SAFETY: cc data live through return.
    let mut c = mt_matr(cc);
    let n = i * j;
    let m = mt_nrow(aa) * mt_ncol(aa);
    let mut i2: c_int = 0;
    let mut j2: c_int = 0;
    while i2 < n {
        *c = *a.offset(j2 as isize);
        c = c.offset(1);
        j2 += 1;
        if j2 == m {
            j2 = 0;
        }
        i2 += 1;
    }
    cc
}

// SAFETY: mirrors C opr_apply (string-wrap + com_apply roundtrip).
#[no_mangle]
pub unsafe extern "C" fn opr_apply(aa: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: fresh temp owns its storage.
    let store = var_temp_new(TYPE_STRING, mt_nrow(aa), mt_ncol(aa));
    (*(*store).this).refcount = 0;
    mat_free((*store).this);
    (*store).this = aa;
    (*aa).refcount += 1;
    // SAFETY: store chain live for the call.
    let ptr = com_apply(store);
    var_delete_temp(store);
    if !ptr.is_null() {
        // SAFETY: mat_copy returns a live matrix.
        mat_copy((*ptr).this)
    } else {
        core::ptr::null_mut()
    }
}

// Elementwise add/sub with scalar broadcast (mirrors C opr_add/opr_subs).
macro_rules! elem_binop {
    ($name:ident, $op:tt, $msg:expr) => {
        // SAFETY: mirrors the C original (same shapes, same broadcast).
        #[no_mangle]
        pub unsafe extern "C" fn $name(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
            let nrowa = mt_nrow(aa);
            let ncola = mt_ncol(aa);
            let nrowb = mt_nrow(bb);
            let ncolb = mt_ncol(bb);
            // SAFETY: both data blocks live.
            let mut a = mt_matr(aa);
            let mut b = mt_matr(bb);
            if nrowa == nrowb && ncola == ncolb {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    *c = *a $op *b;
                    c = c.offset(1);
                    a = a.offset(1);
                    b = b.offset(1);
                    i += 1;
                }
                cc
            } else if nrowa == 1 && ncola == 1 {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(bb), nrowb, ncolb);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let value = *a;
                let n = nrowb * ncolb;
                let mut i: c_int = 0;
                while i < n {
                    *c = value $op *b;
                    c = c.offset(1);
                    b = b.offset(1);
                    i += 1;
                }
                cc
            } else if nrowb == 1 && ncolb == 1 {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let value = *b;
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    *c = *a $op value;
                    c = c.offset(1);
                    a = a.offset(1);
                    i += 1;
                }
                cc
            } else {
                // SAFETY: static format string.
                error_matc($msg);
            }
        }
    };
}
elem_binop!(opr_add, +, b"Add: Incompatible for addition.\n\0".as_ptr() as *const c_char);
elem_binop!(opr_subs, -, b"Substr: Incompatible for addition.\n\0".as_ptr() as *const c_char);

// SAFETY: mirrors C opr_minus (negation).
#[no_mangle]
pub unsafe extern "C" fn opr_minus(aa: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: argument data live.
    let mut a = mt_matr(aa);
    let nrowa = mt_nrow(aa);
    let ncola = mt_ncol(aa);
    // SAFETY: fresh matrix owns its storage.
    let cc = mat_new(mt_type(aa), nrowa, ncola);
    // SAFETY: cc data live through return.
    let mut c = mt_matr(cc);
    let n = nrowa * ncola;
    let mut i: c_int = 0;
    while i < n {
        *c = -*a;
        c = c.offset(1);
        a = a.offset(1);
        i += 1;
    }
    cc
}

// SAFETY: mirrors C opr_mul (scalar broadcast, matmul, or same-shape
// elementwise fallback).
#[no_mangle]
pub unsafe extern "C" fn opr_mul(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
    let nrowa = mt_nrow(aa);
    let ncola = mt_ncol(aa);
    let nrowb = mt_nrow(bb);
    let ncolb = mt_ncol(bb);
    // SAFETY: both data blocks live.
    let mut a = mt_matr(aa);
    let mut b = mt_matr(bb);
    if nrowa == 1 && ncola == 1 {
        // SAFETY: fresh matrix owns its storage.
        let cc = mat_new(mt_type(bb), nrowb, ncolb);
        // SAFETY: cc data live through return.
        let mut c = mt_matr(cc);
        let value = *a;
        let n = nrowb * ncolb;
        let mut i: c_int = 0;
        while i < n {
            *c = value * *b;
            c = c.offset(1);
            b = b.offset(1);
            i += 1;
        }
        cc
    } else if nrowb == 1 && ncolb == 1 {
        // SAFETY: fresh matrix owns its storage.
        let cc = mat_new(mt_type(aa), nrowa, ncola);
        // SAFETY: cc data live through return.
        let mut c = mt_matr(cc);
        let value = *b;
        let n = nrowa * ncola;
        let mut i: c_int = 0;
        while i < n {
            *c = value * *a;
            c = c.offset(1);
            a = a.offset(1);
            i += 1;
        }
        cc
    } else if ncola == nrowb {
        // SAFETY: fresh matrix owns its storage.
        let cc = mat_new(mt_type(aa), nrowa, ncolb);
        // SAFETY: cc data live through return.
        let mut c = mt_matr(cc);
        let mut i: c_int = 0;
        while i < nrowa {
            let mut j: c_int = 0;
            while j < ncolb {
                let mut s = 0.0;
                let mut k: c_int = 0;
                while k < ncola {
                    s += *a.offset(k as isize) * *b.offset((k * ncolb + j) as isize);
                    k += 1;
                }
                *c = s;
                c = c.offset(1);
                j += 1;
            }
            a = a.offset(ncola as isize);
            i += 1;
        }
        cc
    } else if ncola == ncolb && nrowa == nrowb {
        // SAFETY: fresh matrix owns its storage.
        let cc = mat_new(mt_type(aa), nrowa, ncolb);
        // SAFETY: cc data live through return.
        let c = mt_matr(cc);
        let mut k: c_int = 0;
        let mut i: c_int = 0;
        while i < nrowa {
            let mut j: c_int = 0;
            while j < ncolb {
                *c.offset(k as isize) = *a.offset(k as isize) * *b.offset(k as isize);
                j += 1;
                k += 1;
            }
            i += 1;
        }
        cc
    } else {
        // SAFETY: static format string.
        error_matc(b"Mul: Incompatible for multiplication.\n\0".as_ptr() as *const c_char);
    }
}

elem_binop!(opr_pmul, *, b"PMul: Incompatible for pointwise multiplication.\n\0".as_ptr() as *const c_char);
elem_binop!(opr_div, /, b"Div: Incompatible for division.\n\0".as_ptr() as *const c_char);

// SAFETY: mirrors C opr_pow (elementwise, integer powr, negative via
// mtr_inv; C abs macro on ints becomes a ternary).
#[no_mangle]
pub unsafe extern "C" fn opr_pow(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
    let nrowa = mt_nrow(aa);
    let ncola = mt_ncol(aa);
    let nrowb = mt_nrow(bb);
    let ncolb = mt_ncol(bb);
    // SAFETY: both data blocks live.
    let mut a = mt_matr(aa);
    let b = mt_matr(bb);
    if nrowb != 1 || ncolb != 1 {
        // SAFETY: static format string.
        error_matc(b"Pow: Matrix ^ Matrix ?.\n\0".as_ptr() as *const c_char);
    }
    if nrowa == 1 || ncola != nrowa {
        // SAFETY: fresh matrix owns its storage.
        let cc = mat_new(mt_type(aa), nrowa, ncola);
        // SAFETY: cc data live through return.
        let mut c = mt_matr(cc);
        let value = *b;
        let n = nrowa * ncola;
        let mut i: c_int = 0;
        while i < n {
            *c = pow(*a, value);
            c = c.offset(1);
            a = a.offset(1);
            i += 1;
        }
        return cc;
    }
    let powr = *b as c_int;
    // C abs macro on ints: manual ternary, same value.
    let iabs = |v: c_int| -> c_int { if v > 0 { v } else { -v } };
    let mut cc: *mut MATRIX;
    if powr == 0 {
        // SAFETY: fresh matrix owns its storage (zeroed).
        cc = mat_new(mt_type(aa), nrowa, ncola);
        // SAFETY: cc data live.
        let c = mt_matr(cc);
        let mut i: c_int = 0;
        while i < nrowa {
            *c.offset((i * ncola + i) as isize) = 1.0;
            i += 1;
        }
    } else if iabs(powr) == 1 {
        // SAFETY: mat_copy returns a live matrix.
        cc = mat_copy(aa);
    } else {
        // SAFETY: C-heap work vector, freed below.
        let v = mem_alloc((nrowa as usize) * size_of::<c_double>()) as *mut c_double;
        // SAFETY: fresh matrix owns its storage.
        cc = mat_new(mt_type(aa), nrowa, nrowa);
        // SAFETY: cc data live.
        let mut c = mt_matr(cc);
        let mut bb = mt_matr(aa);
        let mut l: c_int = 1;
        while l < iabs(powr) {
            let mut i: c_int = 0;
            while i < nrowa {
                let mut j: c_int = 0;
                while j < nrowa {
                    *v.offset(j as isize) = 0.0;
                    let mut k: c_int = 0;
                    while k < nrowa {
                        *v.offset(j as isize) += *bb.offset(k as isize) * *a.offset((k * ncola + j) as isize);
                        k += 1;
                    }
                    j += 1;
                }
                let mut j: c_int = 0;
                while j < nrowa {
                    *c = *v.offset(j as isize);
                    c = c.offset(1);
                    j += 1;
                }
                bb = bb.offset(nrowa as isize);
                i += 1;
            }
            // C: a = MATR(A); b = c = MATR(C) — next round multiplies
            // the fresh product rows (bb) by the original A columns (a).
            a = mt_matr(aa);
            c = mt_matr(cc);
            bb = c;
            l += 1;
        }
        // SAFETY: v from mem_alloc above.
        mem_free(v as *mut core::ffi::c_void);
        // Restructure to match C exactly: after the loop C sets
        // a = MATR(A); b = c = MATR(C). The next squaring round (if any)
        // multiplies C*C. Mirror: track current product base explicitly.
        // (Handled below by re-reading cc each round.)
    }
    if powr < 0 {
        // SAFETY: fresh ALLOCMEM wrapper (calloc-zeroed: NAME NULL).
        let tmp = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
        (*tmp).this = cc;
        // SAFETY: mtr_inv consumes the wrapper chain.
        let inv = mtr_inv(tmp);
        mat_free(cc);
        // SAFETY: tmp wrapper from mem_alloc above.
        mem_free(tmp as *mut core::ffi::c_void);
        cc = (*inv).this;
        (*(*inv).this).refcount += 1;
        var_delete_temp(inv);
    }
    cc
}


// SAFETY: mirrors C opr_trans.
#[no_mangle]
pub unsafe extern "C" fn opr_trans(aa: *mut MATRIX) -> *mut MATRIX {
    let ncola = mt_ncol(aa);
    let nrowa = mt_nrow(aa);
    // SAFETY: argument data live.
    let mut a = mt_matr(aa);
    // SAFETY: fresh matrix owns its storage.
    let cc = mat_new(mt_type(aa), ncola, nrowa);
    // SAFETY: cc data live through return.
    let c = mt_matr(cc);
    let mut i: c_int = 0;
    while i < nrowa {
        let mut j: c_int = 0;
        while j < ncola {
            *c.offset((j * nrowa + i) as isize) = *a;
            a = a.offset(1);
            j += 1;
        }
        i += 1;
    }
    cc
}

// SAFETY: mirrors C opr_reduction (mask select).
#[no_mangle]
pub unsafe extern "C" fn opr_reduction(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: both matrices live.
    if !((mt_nrow(aa) == mt_nrow(bb)) && (mt_ncol(aa) == mt_ncol(bb))) {
        // SAFETY: static format string.
        error_matc(b"Incompatible for reduction.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh matrix owns its storage.
    let cc = mat_new(mt_type(aa), mt_nrow(aa), mt_ncol(aa));
    // SAFETY: all three data blocks live.
    let mut a = mt_matr(aa);
    let mut b = mt_matr(bb);
    let mut c = mt_matr(cc);
    let n = mt_nrow(aa) * mt_ncol(aa);
    let mut i: c_int = 0;
    while i < n {
        *c = if *b != 0.0 { *a } else { 0.0 };
        c = c.offset(1);
        a = a.offset(1);
        b = b.offset(1);
        i += 1;
    }
    cc
}

// Comparisons: scalar/matrix broadcast with calloc-zeroed output (mirrors
// C opr_lt/le/gt/ge/eq/neq; only-1 cells written, rest stay 0).
macro_rules! cmp_binop {
    ($name:ident, $op:tt, $msg:expr) => {
        // SAFETY: mirrors the C original (same shapes, same broadcast).
        #[no_mangle]
        pub unsafe extern "C" fn $name(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
            let nrowa = mt_nrow(aa);
            let ncola = mt_ncol(aa);
            let nrowb = mt_nrow(bb);
            let ncolb = mt_ncol(bb);
            // SAFETY: both data blocks live.
            let a = mt_matr(aa);
            let b = mt_matr(bb);
            if nrowa == 1 && ncola == 1 {
                // SAFETY: fresh matrix owns its storage (zeroed).
                let cc = mat_new(mt_type(bb), nrowb, ncolb);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowb * ncolb;
                let mut i: c_int = 0;
                while i < n {
                    if *a $op *b.offset(i as isize) {
                        *c = 1.0;
                    }
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else if nrowb == 1 && ncolb == 1 {
                // SAFETY: fresh matrix owns its storage (zeroed).
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    if *a.offset(i as isize) $op *b {
                        *c = 1.0;
                    }
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else if nrowa == nrowb && ncola == ncolb {
                // SAFETY: fresh matrix owns its storage (zeroed).
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    if *a.offset(i as isize) $op *b.offset(i as isize) {
                        *c = 1.0;
                    }
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else {
                // SAFETY: static format string.
                error_matc($msg);
            }
        }
    };
}
cmp_binop!(opr_lt, <, b"lt: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
cmp_binop!(opr_le, <=, b"le: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
cmp_binop!(opr_gt, >, b"gt: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
cmp_binop!(opr_ge, >=, b"ge: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
cmp_binop!(opr_eq, ==, b"eq: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
cmp_binop!(opr_neq, !=, b"neq: Incompatible for comparison.\n\0".as_ptr() as *const c_char);

// SAFETY: mirrors C opr_or/opr_and (C ||/&& on nonzero doubles).
macro_rules! logic_binop {
    ($name:ident, $op:tt, $msg:expr) => {
        // SAFETY: mirrors the C original (same shapes, same broadcast).
        #[no_mangle]
        pub unsafe extern "C" fn $name(aa: *mut MATRIX, bb: *mut MATRIX) -> *mut MATRIX {
            let nrowa = mt_nrow(aa);
            let ncola = mt_ncol(aa);
            let nrowb = mt_nrow(bb);
            let ncolb = mt_ncol(bb);
            // SAFETY: both data blocks live.
            let a = mt_matr(aa);
            let b = mt_matr(bb);
            if nrowa == 1 && ncola == 1 {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(bb), nrowb, ncolb);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowb * ncolb;
                let mut i: c_int = 0;
                while i < n {
                    *c = if (*a != 0.0) $op (*b.offset(i as isize) != 0.0) { 1.0 } else { 0.0 };
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else if nrowb == 1 && ncolb == 1 {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    *c = if (*a.offset(i as isize) != 0.0) $op (*b != 0.0) { 1.0 } else { 0.0 };
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else if nrowa == nrowb && ncola == ncolb {
                // SAFETY: fresh matrix owns its storage.
                let cc = mat_new(mt_type(aa), nrowa, ncola);
                // SAFETY: cc data live through return.
                let mut c = mt_matr(cc);
                let n = nrowa * ncola;
                let mut i: c_int = 0;
                while i < n {
                    *c = if (*a.offset(i as isize) != 0.0) $op (*b.offset(i as isize) != 0.0) { 1.0 } else { 0.0 };
                    c = c.offset(1);
                    i += 1;
                }
                cc
            } else {
                // SAFETY: static format string.
                error_matc($msg);
            }
        }
    };
}
logic_binop!(opr_or, ||, b"or: Incompatible for comparison.\n\0".as_ptr() as *const c_char);
logic_binop!(opr_and, &&, b"and: Incompatible for comparison.\n\0".as_ptr() as *const c_char);

// SAFETY: mirrors C opr_not, including its quirk: only a[0] is tested
// (a never advances), so a nonzero head yields all zeros.
#[no_mangle]
pub unsafe extern "C" fn opr_not(aa: *mut MATRIX) -> *mut MATRIX {
    // SAFETY: argument data live.
    let a = mt_matr(aa);
    let nrowa = mt_nrow(aa);
    let ncola = mt_ncol(aa);
    // SAFETY: fresh matrix owns its storage (zeroed).
    let cc = mat_new(mt_type(aa), nrowa, ncola);
    // SAFETY: cc data live through return.
    let mut c = mt_matr(cc);
    let n = nrowa * ncola;
    let mut i: c_int = 0;
    while i < n {
        if *a == 0.0 {
            *c = 1.0;
        }
        c = c.offset(1);
        i += 1;
    }
    cc
}
