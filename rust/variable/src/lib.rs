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

// ---- unit body: transcribed from matc/src/variable.c ----
// Transcribed from matc/src/variable.c (variable lifecycle + pools).
// Pool blocks use plain libc malloc/free (never mem_alloc/mem_free): they
// carry no ALLOC_LIST header, and FREEMEM on them would corrupt the heap.

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut listheaders: [LIST; 5];
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
    fn PrintOut(fmt: *const c_char, ...);
    fn lst_addhead(list: c_int, item: *mut LIST);
    fn lst_add(list: c_int, item: *mut LIST);
    fn lst_find(list: c_int, nm: *mut c_char) -> *mut LIST;
    fn lst_free(list: c_int, item: *mut LIST);
    fn lst_purge(list: c_int);
    fn lst_print(list: c_int) -> *mut VARIABLE;
    fn mat_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut MATRIX;
    fn mat_copy(m: *mut MATRIX) -> *mut MATRIX;
    fn mat_free(m: *mut MATRIX);
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

pub const SCALAR_POOL_CAP: usize = 32;
pub const SCALAR_BLOCK_SZ: usize = 32 + 24 + 8;
pub const WRAPPER_POOL_CAP: usize = 32;

static mut scalar_pool_head: *mut VARIABLE = core::ptr::null_mut();
static mut scalar_pool_count: c_int = 0;
static mut wrapper_pool_head: *mut VARIABLE = core::ptr::null_mut();
static mut wrapper_pool_count: c_int = 0;

// SAFETY: pops a combined scalar block (pool or fresh malloc).
unsafe fn scalar_pool_pop() -> *mut VARIABLE {
    // SAFETY: file statics mirror the C originals.
    if !scalar_pool_head.is_null() {
        let ptr = scalar_pool_head;
        scalar_pool_head = (*ptr).next;
        scalar_pool_count -= 1;
        ptr
    } else {
        // SAFETY: plain malloc (no ALLOC_LIST header by contract).
        malloc(SCALAR_BLOCK_SZ) as *mut VARIABLE
    }
}
// SAFETY: pushes a combined scalar block (or frees past capacity).
unsafe fn scalar_pool_push(ptr: *mut VARIABLE) {
    // SAFETY: file statics mirror the C originals.
    if (scalar_pool_count as usize) < SCALAR_POOL_CAP {
        (*ptr).next = scalar_pool_head;
        scalar_pool_head = ptr;
        scalar_pool_count += 1;
    } else {
        // SAFETY: ptr is a plain-malloc block.
        free(ptr as *mut core::ffi::c_void);
    }
}
// SAFETY: pushes a bare wrapper block (or frees past capacity).
unsafe fn wrapper_pool_push(ptr: *mut VARIABLE) {
    // SAFETY: file statics mirror the C originals.
    if (wrapper_pool_count as usize) < WRAPPER_POOL_CAP {
        (*ptr).next = wrapper_pool_head;
        wrapper_pool_head = ptr;
        wrapper_pool_count += 1;
    } else {
        // SAFETY: ptr is a plain-malloc block.
        free(ptr as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C var_wrapper_new (pool-backed MATRIX borrower).
#[no_mangle]
pub unsafe extern "C" fn var_wrapper_new(m: *mut MATRIX) -> *mut VARIABLE {
    // SAFETY: wrapper pool mirrors the C original.
    let ptr = if !wrapper_pool_head.is_null() {
        let p = wrapper_pool_head;
        wrapper_pool_head = (*p).next;
        wrapper_pool_count -= 1;
        p
    } else {
        // SAFETY: plain malloc (no ALLOC_LIST header by contract).
        malloc(VARIABLESIZE) as *mut VARIABLE
    };
    (*ptr).next = core::ptr::null_mut();
    (*ptr).name = core::ptr::null_mut();
    (*ptr).changed = VAR_WRAPPER_POOL_FLAG;
    (*ptr).this = m;
    (*m).refcount += 1;
    ptr
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

// SAFETY: mirrors C const_new (named global in CONSTANTS).
#[no_mangle]
pub unsafe extern "C" fn const_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE {
    // SAFETY: fresh ALLOCMEM block.
    let ptr = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
    // SAFETY: mat_new returns a live matrix.
    (*ptr).this = mat_new(typ, nrow, ncol);
    (*(*ptr).this).refcount = 1;
    (*ptr).next = core::ptr::null_mut();
    (*ptr).changed = 0;
    // SAFETY: nm is a live C string; STRCOPY into session memory.
    (*ptr).name = strcopy(nm);
    lst_add(CONSTANTS, ptr as *mut LIST);
    ptr
}

// SAFETY: mirrors C var_new (named global in VARIABLES, head insert).
#[no_mangle]
pub unsafe extern "C" fn var_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE {
    var_delete(nm);
    // SAFETY: fresh ALLOCMEM block.
    let ptr = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
    // SAFETY: mat_new returns a live matrix.
    (*ptr).this = mat_new(typ, nrow, ncol);
    (*(*ptr).this).refcount = 1;
    (*ptr).next = core::ptr::null_mut();
    (*ptr).changed = 0;
    // SAFETY: nm is a live C string; STRCOPY into session memory.
    (*ptr).name = strcopy(nm);
    lst_addhead(VARIABLES, ptr as *mut LIST);
    ptr
}

// SAFETY: mirrors C var_create_vector (adopts a caller double array).
#[no_mangle]
pub unsafe extern "C" fn var_create_vector(nm: *mut c_char, ntime: c_int, ncol: c_int, data: *mut c_double) {
    // SAFETY: var_new returns a live global.
    let var = var_new(nm, TYPE_DOUBLE, ntime, ncol);
    // SAFETY: var matrix live; embedded-data check like C.
    if !mat_data_embedded((*var).this) {
        mem_free((*(*var).this).data as *mut core::ffi::c_void);
    }
    (*(*var).this).data = data;
}

// SAFETY: mirrors C var_rename (shared MATRIX, refcount protocol, combined
// scalars always copied so var_delete_temp can free the whole block).
#[no_mangle]
pub unsafe extern "C" fn var_rename(ptr: *mut VARIABLE, s: *mut c_char) -> *mut VARIABLE {
    if ptr.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: var_check on a live name.
    let mut res = var_check(s);
    if res.is_null() && (*(*ptr).this).refcount > 1 {
        // SAFETY: fresh ALLOCMEM block.
        res = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
        // SAFETY: s live; STRCOPY into session memory.
        (*res).name = strcopy(s);
        // SAFETY: mat_copy returns a live matrix.
        (*res).this = mat_copy((*ptr).this);
        (*(*res).this).refcount = 1;
        (*res).next = core::ptr::null_mut();
        (*res).changed = 0;
        lst_addhead(VARIABLES, res as *mut LIST);
    } else if res.is_null() {
        // SAFETY: fresh ALLOCMEM block.
        res = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
        // SAFETY: s live; STRCOPY into session memory.
        (*res).name = strcopy(s);
        if var_scalar_combined(ptr) {
            // SAFETY: mat_copy returns a live matrix.
            (*res).this = mat_copy((*ptr).this);
            (*(*res).this).refcount = 1;
        } else {
            (*res).this = (*ptr).this;
            (*(*ptr).this).refcount += 1;
        }
        (*res).next = core::ptr::null_mut();
        (*res).changed = 0;
        lst_addhead(VARIABLES, res as *mut LIST);
    } else if res != ptr {
        // SAFETY: res/ptr matrices live.
        if (*(*res).this).nrow == (*(*ptr).this).nrow && (*(*res).this).ncol == (*(*ptr).this).ncol {
            // SAFETY: same-shape memcpy of the data bytes.
            memcpy(
                (*(*res).this).data as *mut core::ffi::c_void,
                (*(*ptr).this).data as *const core::ffi::c_void,
                ((*(*res).this).nrow as usize) * ((*(*res).this).ncol as usize) * size_of::<c_double>(),
            );
        } else {
            (*(*res).this).refcount -= 1;
            if (*(*res).this).refcount == 0 {
                if !mat_data_embedded((*res).this) {
                    // SAFETY: data is a mem_alloc'd block.
                    mem_free((*(*res).this).data as *mut core::ffi::c_void);
                }
                // SAFETY: MATRIX is a mem_alloc'd block.
                mem_free((*res).this as *mut core::ffi::c_void);
            }
            if var_scalar_combined(ptr) {
                // SAFETY: mat_copy returns a live matrix.
                (*res).this = mat_copy((*ptr).this);
                (*(*res).this).refcount = 1;
            } else {
                (*res).this = (*ptr).this;
                (*(*ptr).this).refcount += 1;
            }
        }
    }
    if res != ptr {
        var_delete_temp(ptr);
    }
    res
}

static mut var_pprec: c_int = 3;
static mut var_pinp: c_int = FALSE;
static mut var_rowintime: c_int = FALSE;

// SAFETY: mirrors C var_format (print precision/mode globals).
#[no_mangle]
pub unsafe extern "C" fn var_format(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var data live.
    if *var_matr(var) > 0.0 && *var_matr(var) < 20.0 {
        var_pprec = *var_matr(var) as c_int;
    }
    // SAFETY: NEXT cell checked like C.
    if !(*var).next.is_null() {
        // SAFETY: var_to_string returns a fresh mem_alloc'd string.
        let frm = var_to_string((*var).next);
        // SAFETY: strcmp on live C strings.
        if strcmp(frm, b"input\0".as_ptr() as *const c_char) == 0 {
            var_pinp = TRUE;
        } else {
            var_pinp = FALSE;
            // SAFETY: strcmp on live C strings.
            if strcmp(frm, b"rowform\0".as_ptr() as *const c_char) == 0 {
                var_rowintime = TRUE;
            } else {
                var_rowintime = FALSE;
            }
        }
        // SAFETY: frm from var_to_string above.
        mem_free(frm as *mut core::ffi::c_void);
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C var_print, including the column-block layout and the
// pinp/rowform format strings (all output via PrintOut = C printf).
#[no_mangle]
pub unsafe extern "C" fn var_print(ptr: *mut VARIABLE) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr matrix live.
    if var_type(ptr) == TYPE_STRING {
        // SAFETY: var_pinp mirrors the C global.
        if var_pinp != 0 {
            // SAFETY: PrintOut appends to the live buffer.
            PrintOut(b"%d %d %% \"\0".as_ptr() as *const c_char, var_nrow(ptr), var_ncol(ptr));
        }
        let mut i: c_int = 0;
        while i < var_nrow(ptr) {
            let mut j: c_int = 0;
            while j < var_ncol(ptr) {
                // SAFETY: PrintOut appends to the live buffer.
                PrintOut(b"%c\0".as_ptr() as *const c_char, var_m(ptr, i, j) as c_int);
                j += 1;
            }
            if var_pinp != 0 {
                if i < var_nrow(ptr) - 1 {
                    // SAFETY: PrintOut appends to the live buffer.
                    PrintOut(b"\"\\\0".as_ptr() as *const c_char);
                } else {
                    // SAFETY: PrintOut appends to the live buffer.
                    PrintOut(b"\"\0".as_ptr() as *const c_char);
                }
            }
            // SAFETY: PrintOut appends to the live buffer.
            PrintOut(b"\n\0".as_ptr() as *const c_char);
            i += 1;
        }
        return;
    }
    let mut fmt = [0 as c_char; 80];
    let mut k: c_int = 0;
    loop {
        // SAFETY: var_pinp/var_rowintime mirror the C globals.
        if var_pinp != 0 {
            // SAFETY: PrintOut appends to the live buffer.
            PrintOut(b"%d %d %% \0".as_ptr() as *const c_char, var_nrow(ptr), var_ncol(ptr));
        } else if var_ncol(ptr) > 8 && var_rowintime == 0 {
            let lo = k;
            let hi = if var_ncol(ptr) - 1 < k + 7 { var_ncol(ptr) - 1 } else { k + 7 };
            // SAFETY: PrintOut appends to the live buffer.
            PrintOut(b"\nColumns %d trough %d\n\n\0".as_ptr() as *const c_char, lo, hi);
        }
        if var_pinp != 0 || var_rowintime != 0 {
            // SAFETY: sprintf into the 80-byte stack buffer (same bound).
            sprintf(fmt.as_mut_ptr(), b"%%.%dg\0".as_ptr() as *const c_char, var_pprec);
        } else {
            // SAFETY: sprintf into the 80-byte stack buffer (same bound).
            sprintf(fmt.as_mut_ptr(), b"%% %d.%dg\0".as_ptr() as *const c_char, var_pprec + 7, var_pprec);
        }
        // C reuses one j across rows; the trailing count advances k.
        let mut j: c_int = 0;
        let mut i: c_int = 0;
        while i < var_nrow(ptr) {
            if var_rowintime != 0 {
                j = 0;
                while j < var_ncol(ptr) {
                    if j > 0 {
                        // SAFETY: PrintOut appends to the live buffer.
                        PrintOut(b" \0".as_ptr() as *const c_char);
                    }
                    // SAFETY: PrintOut appends; cell live.
                    PrintOut(fmt.as_ptr(), var_m(ptr, i, j));
                    j += 1;
                }
            } else {
                j = 0;
                while j < 80 / (var_pprec + 7) && k + j < var_ncol(ptr) {
                    // SAFETY: PrintOut appends; cell live.
                    PrintOut(fmt.as_ptr(), var_m(ptr, i, j + k));
                    j += 1;
                }
                if var_pinp != 0 && i < var_nrow(ptr) - 1 {
                    // SAFETY: PrintOut appends to the live buffer.
                    PrintOut(b"\\\0".as_ptr() as *const c_char);
                }
            }
            // SAFETY: PrintOut appends to the live buffer.
            PrintOut(b"\n\0".as_ptr() as *const c_char);
            i += 1;
        }
        k += j;
        if k >= var_ncol(ptr) {
            break;
        }
    }
}

// SAFETY: mirrors C var_delete (refcount drop + list unlink).
#[no_mangle]
pub unsafe extern "C" fn var_delete(s: *mut c_char) {
    // SAFETY: var_check on a live name.
    let ptr = var_check(s);
    if !ptr.is_null() {
        (*(*ptr).this).refcount -= 1;
        if (*(*ptr).this).refcount == 0 {
            if !mat_data_embedded((*ptr).this) {
                // SAFETY: data is a mem_alloc'd block.
                mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
            }
            // SAFETY: MATRIX is a mem_alloc'd block.
            mem_free((*ptr).this as *mut core::ffi::c_void);
        }
        lst_free(VARIABLES, ptr as *mut LIST);
    }
}

// SAFETY: mirrors C var_vdelete (delete-by-string-arg).
#[no_mangle]
pub unsafe extern "C" fn var_vdelete(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var_to_string returns a fresh string; var_delete frees
    // nothing of it (lookup only). The C original leaks this temp too.
    let s = var_to_string(var);
    var_delete(s);
    core::ptr::null_mut()
}

// SAFETY: mirrors C var_free (drains VAR_HEAD with the three-way free;
// normal VARIABLEs are FREEMEM'd, never pool-recycled, like C).
#[no_mangle]
pub unsafe extern "C" fn var_free() {
    // SAFETY: VAR_HEAD is the live global variable list.
    let mut ptr = (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next as *mut VARIABLE;
    while !ptr.is_null() {
        let ptrn = (*ptr).next;
        if !(*ptr).name.is_null() {
            // SAFETY: NAME is a mem_alloc'd block.
            mem_free((*ptr).name as *mut core::ffi::c_void);
        }
        if var_scalar_combined(ptr) {
            // SAFETY: plain-malloc combined block (no ALLOC_LIST header).
            free(ptr as *mut core::ffi::c_void);
        } else if var_wrapper_pooled(ptr) {
            (*(*ptr).this).refcount -= 1;
            if (*(*ptr).this).refcount == 0 {
                if !mat_data_embedded((*ptr).this) {
                    // SAFETY: data is a mem_alloc'd block.
                    mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
                }
                // SAFETY: MATRIX is a mem_alloc'd block.
                mem_free((*ptr).this as *mut core::ffi::c_void);
            }
            // SAFETY: plain-malloc wrapper block.
            free(ptr as *mut core::ffi::c_void);
        } else {
            (*(*ptr).this).refcount -= 1;
            if (*(*ptr).this).refcount == 0 {
                if !mat_data_embedded((*ptr).this) {
                    // SAFETY: data is a mem_alloc'd block.
                    mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
                }
                // SAFETY: MATRIX is a mem_alloc'd block.
                mem_free((*ptr).this as *mut core::ffi::c_void);
            }
            // SAFETY: VARIABLE is a mem_alloc'd block.
            mem_free(ptr as *mut core::ffi::c_void);
        }
        ptr = ptrn;
    }
    (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next = core::ptr::null_mut();
}

// SAFETY: mirrors C const_free (refcount drain + list purge).
#[no_mangle]
pub unsafe extern "C" fn const_free() {
    // SAFETY: CONST_HEAD is the live global constant list.
    let mut ptr = (*listheaders.as_mut_ptr().offset(CONSTANTS as isize)).next as *mut VARIABLE;
    while !ptr.is_null() {
        (*(*ptr).this).refcount -= 1;
        if (*(*ptr).this).refcount == 0 {
            if !mat_data_embedded((*ptr).this) {
                // SAFETY: data is a mem_alloc'd block.
                mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
            }
            // SAFETY: MATRIX is a mem_alloc'd block.
            mem_free((*ptr).this as *mut core::ffi::c_void);
        }
        ptr = (*ptr).next;
    }
    lst_purge(CONSTANTS);
}

// SAFETY: mirrors C var_varlist (prints both lists).
#[no_mangle]
pub unsafe extern "C" fn var_varlist(_var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = _var;
    lst_print(CONSTANTS);
    lst_print(VARIABLES);
    core::ptr::null_mut()
}

// SAFETY: mirrors C var_ccheck (existence vector over the arg chain).
#[no_mangle]
pub unsafe extern "C" fn var_ccheck(mut var: *mut VARIABLE) -> *mut VARIABLE {
    let mut n: c_int = 0;
    let mut res = var;
    while !res.is_null() {
        n += 1;
        res = (*res).next;
    }
    // SAFETY: fresh temp owns its storage.
    res = var_temp_new(TYPE_DOUBLE, 1, n);
    let mut i: c_int = 0;
    while i < n {
        // SAFETY: var_to_string returns a fresh string.
        let s = var_to_string(var);
        // SAFETY: var_check on a live name.
        if var_check(s).is_null() {
            var_set_m(res, 0, i, FALSE as c_double);
        } else {
            var_set_m(res, 0, i, TRUE as c_double);
        }
        // SAFETY: s from var_to_string above.
        mem_free(s as *mut core::ffi::c_void);
        var = (*var).next;
        i += 1;
    }
    res
}

// SAFETY: mirrors C var_check (VARIABLES then CONSTANTS).
#[no_mangle]
pub unsafe extern "C" fn var_check(s: *mut c_char) -> *mut VARIABLE {
    // SAFETY: both lists live; lst_find NULL-tolerant.
    let mut res = lst_find(VARIABLES, s) as *mut VARIABLE;
    if res.is_null() {
        res = lst_find(CONSTANTS, s) as *mut VARIABLE;
    }
    res
}

// SAFETY: mirrors C var_temp_copy (NULL-safe detached deep copy).
#[no_mangle]
pub unsafe extern "C" fn var_temp_copy(from: *mut VARIABLE) -> *mut VARIABLE {
    if from.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: fresh ALLOCMEM block.
    let to = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
    // SAFETY: mat_copy returns a live matrix.
    (*to).this = mat_copy((*from).this);
    (*(*to).this).refcount = 1;
    (*to).next = core::ptr::null_mut();
    (*to).name = core::ptr::null_mut();
    (*to).changed = 0;
    to
}

// SAFETY: mirrors C var_temp_new (1x1 doubles from the scalar pool,
// everything else via ALLOCMEM + mat_new).
#[no_mangle]
pub unsafe extern "C" fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE {
    if typ == TYPE_DOUBLE && nrow == 1 && ncol == 1 {
        // SAFETY: pool block layout mirrors the C original exactly.
        let ptr = scalar_pool_pop();
        (*ptr).next = core::ptr::null_mut();
        (*ptr).name = core::ptr::null_mut();
        (*ptr).changed = 0;
        let mat = (ptr as *mut c_char).add(VARIABLESIZE) as *mut MATRIX;
        (*mat).typ = TYPE_DOUBLE;
        (*mat).refcount = 1;
        (*mat).nrow = 1;
        (*mat).ncol = 1;
        (*mat).data = (mat as *mut c_char).add(MATRIXSIZE) as *mut c_double;
        *(*mat).data = 0.0;
        (*ptr).this = mat;
        return ptr;
    }
    // SAFETY: fresh ALLOCMEM block.
    let ptr = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
    // SAFETY: mat_new returns a live matrix.
    (*ptr).this = mat_new(typ, nrow, ncol);
    (*(*ptr).this).refcount = 1;
    (*ptr).next = core::ptr::null_mut();
    (*ptr).name = core::ptr::null_mut();
    (*ptr).changed = 0;
    ptr
}

// SAFETY: mirrors C var_copy_transpose (column-major export into a caller
// buffer, clamped to the variable shape; C min macro becomes a ternary).
#[no_mangle]
pub unsafe extern "C" fn var_copy_transpose(nm: *mut c_char, values: *mut c_double, nrows: c_int, ncols: c_int) {
    // SAFETY: var_check on a live name.
    let var = var_check(nm);
    if var.is_null() {
        return;
    }
    // SAFETY: var matrix live; values spans nrows*ncols caller doubles.
    let mut i: c_int = 0;
    while i < (if nrows < var_nrow(var) { nrows } else { var_nrow(var) }) {
        let mut j: c_int = 0;
        while j < (if ncols < var_ncol(var) { ncols } else { var_ncol(var) }) {
            *values.offset((nrows * i + j) as isize) = var_m(var, j, i);
            j += 1;
        }
        i += 1;
    }
}

// SAFETY: mirrors C var_delete_temp_el (three-way temp release).
#[no_mangle]
pub unsafe extern "C" fn var_delete_temp_el(ptr: *mut VARIABLE) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr matrix live.
    (*(*ptr).this).refcount -= 1;
    if (*(*ptr).this).refcount == 0 {
        if var_scalar_combined(ptr) {
            // SAFETY: plain-malloc combined block.
            scalar_pool_push(ptr);
        } else if var_wrapper_pooled(ptr) {
            if !mat_data_embedded((*ptr).this) {
                // SAFETY: data is a mem_alloc'd block.
                mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
            }
            // SAFETY: MATRIX is a mem_alloc'd block.
            mem_free((*ptr).this as *mut core::ffi::c_void);
            wrapper_pool_push(ptr);
        } else {
            if !mat_data_embedded((*ptr).this) {
                // SAFETY: data is a mem_alloc'd block.
                mem_free((*(*ptr).this).data as *mut core::ffi::c_void);
            }
            // SAFETY: MATRIX is a mem_alloc'd block.
            mem_free((*ptr).this as *mut core::ffi::c_void);
            // SAFETY: VARIABLE is a mem_alloc'd block.
            mem_free(ptr as *mut core::ffi::c_void);
        }
    } else if var_scalar_combined(ptr) {
        // never shared — dead branch, mirrored.
    } else if var_wrapper_pooled(ptr) {
        wrapper_pool_push(ptr);
    } else {
        // SAFETY: VARIABLE is a mem_alloc'd block.
        mem_free(ptr as *mut core::ffi::c_void);
    }
}

// SAFETY: mirrors C var_delete_temp (chain drain; NULL-safe).
#[no_mangle]
pub unsafe extern "C" fn var_delete_temp(head: *mut VARIABLE) {
    let mut ptr = head;
    while !ptr.is_null() {
        // SAFETY: NEXT read before release.
        let ptr1 = (*ptr).next;
        var_delete_temp_el(ptr);
        ptr = ptr1;
    }
}

// SAFETY: mirrors C var_to_string (calloc'd, zero-filled past NCOL).
#[no_mangle]
pub unsafe extern "C" fn var_to_string(ptr: *mut VARIABLE) -> *mut c_char {
    // SAFETY: mem_alloc zeroes (calloc); NCOL+1 bytes.
    let s = mem_alloc((var_ncol(ptr) as usize) + 1) as *mut c_char;
    let mut i: c_int = 0;
    while i < var_ncol(ptr) {
        // SAFETY: s has NCOL+1 bytes; cell live.
        *s.offset(i as isize) = var_m(ptr, 0, i) as c_char;
        i += 1;
    }
    s
}

// SAFETY: mirrors C var_reset_status/var_get_status/var_com_free.
#[no_mangle]
pub unsafe extern "C" fn var_reset_status(nm: *mut c_char) {
    // SAFETY: var_check on a live name.
    let ptr = var_check(nm);
    if !ptr.is_null() {
        (*ptr).changed = 0;
    }
}
// SAFETY: mirrors C var_com_free (clear all variables).
#[no_mangle]
pub unsafe extern "C" fn var_com_free(_var: *mut VARIABLE) -> *mut VARIABLE {
    let _ = _var;
    var_free();
    core::ptr::null_mut()
}
// SAFETY: mirrors C var_get_status.
#[no_mangle]
pub unsafe extern "C" fn var_get_status(nm: *mut c_char) -> c_int {
    // SAFETY: var_check on a live name.
    let ptr = var_check(nm);
    if !ptr.is_null() {
        (*ptr).changed
    } else {
        0
    }
}

// SAFETY: registers exists/who/format/delete/clear with byte-identical
// help texts.
#[no_mangle]
pub unsafe extern "C" fn var_com_init() {
    // SAFETY: com_init copies into session memory; statics live forever.
    com_init(b"exists\0".as_ptr() as *const c_char, FALSE, FALSE, Some(var_ccheck), 1, 1000, b"exists(name)\nReturn TRUE if variable by given name exists otherwise return FALSE.\n\0".as_ptr() as *const c_char);
    com_init(b"who\0".as_ptr() as *const c_char, FALSE, FALSE, Some(var_varlist), 0, 0, b"who\nGives list of currently defined variables.\n\0".as_ptr() as *const c_char);
    com_init(b"format\0".as_ptr() as *const c_char, FALSE, FALSE, Some(var_format), 1, 2, b"format(precision)\nSet number of digits used in printing values in MATC.\n\n\0".as_ptr() as *const c_char);
    com_init(b"delete\0".as_ptr() as *const c_char, FALSE, FALSE, Some(var_vdelete), 1, 1, b"delete(name)\nDelete a variable with given name.\n\0".as_ptr() as *const c_char);
    com_init(b"clear\0".as_ptr() as *const c_char, FALSE, FALSE, Some(var_com_free), 0, 0, b"clear()\nClear all variables.\n\0".as_ptr() as *const c_char);
}
