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

// ---- unit body: transcribed from matc/src/funcs.c ----
// Transcribed from matc/src/funcs.c (user-defined functions).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut listheaders: [LIST; 5];
    static mut math_out: *mut FILE;
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn PrintOut(fmt: *const c_char, ...);
    fn lst_find(list: c_int, nm: *mut c_char) -> *mut LIST;
    fn lst_add(list: c_int, item: *mut LIST);
    fn lst_unlink(list: c_int, item: *mut LIST);
    fn lst_free(list: c_int, item: *mut LIST);
    fn var_to_string(v: *mut VARIABLE) -> *mut c_char;
    fn var_check(nm: *mut c_char) -> *mut VARIABLE;
    fn var_temp_copy(v: *mut VARIABLE) -> *mut VARIABLE;
    fn var_delete(nm: *mut c_char);
    fn var_delete_temp(v: *mut VARIABLE);
    fn var_free();
    fn free_clause(c: *mut CLAUSE);
    fn evalclause(c: *mut CLAUSE) -> *mut VARIABLE;
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

// SAFETY: VAR_HEAD is listheaders[VARIABLES].next (live global list).
unsafe fn var_head() -> *mut VARIABLE {
    (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next as *mut VARIABLE
}
// SAFETY: replaces the VAR_HEAD list (callers preserve/restore it).
unsafe fn set_var_head(v: *mut VARIABLE) {
    (*listheaders.as_mut_ptr().offset(VARIABLES as isize)).next = v as *mut LIST;
}

// SAFETY: mirrors C fnc_check (NULL when absent).
#[no_mangle]
pub unsafe extern "C" fn fnc_check(nm: *mut c_char) -> *mut FUNCTION {
    // SAFETY: FUNCTIONS list live; lst_find NULL-tolerant.
    lst_find(FUNCTIONS, nm) as *mut FUNCTION
}

// SAFETY: mirrors C fnc_delete; var is the evaluated argument.
#[no_mangle]
pub unsafe extern "C" fn fnc_delete(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: var_to_string returns a fresh mem_alloc'd string.
    let s = var_to_string(ptr);
    // SAFETY: fnc_check on a live name.
    let fnc = fnc_check(s);
    if !fnc.is_null() {
        fnc_free_entry(fnc);
    } else {
        // SAFETY: static format; live name arg.
        error_matc(b"Function definition not found: %s.\n\0".as_ptr() as *const c_char, s);
    }
    // SAFETY: s from var_to_string above.
    mem_free(s as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C fnc_list, including the file-append form and the
// (commented-out) body print: header only, exactly like the original.
#[no_mangle]
pub unsafe extern "C" fn fnc_list(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: math_out is the live REPL output stream.
    let mut fp = math_out;
    // SAFETY: var_to_string returns a fresh mem_alloc'd string.
    let s = var_to_string(ptr);
    // SAFETY: fnc_check on a live name.
    let fnc = fnc_check(s);
    if !fnc.is_null() {
        // SAFETY: NEXT cell check like C.
        if !(*ptr).next.is_null() {
            // SAFETY: NEXT arg string fresh; fopen in append mode.
            let file = var_to_string((*ptr).next);
            let f = fopen(file, b"a\0".as_ptr() as *const c_char);
            if f.is_null() {
                // SAFETY: static format; live file arg.
                error_matc(b"flist: can't open file: %s.\0".as_ptr() as *const c_char, file);
            }
            // SAFETY: file from var_to_string above.
            mem_free(file as *mut core::ffi::c_void);
            fp = f;
            let _ = fp;
        }
        // SAFETY: NAME(fnc) is the live function name.
        PrintOut(b"function %s\0".as_ptr() as *const c_char, (*fnc).name);
        if (*fnc).parcount != 0 {
            // SAFETY: parnames[0] live.
            PrintOut(b"(%s\0".as_ptr() as *const c_char, *(*fnc).parnames);
            let mut i: c_int = 1;
            while i < (*fnc).parcount {
                // SAFETY: parnames[i] live.
                PrintOut(b",%s\0".as_ptr() as *const c_char, *(*fnc).parnames.offset(i as isize));
                i += 1;
            }
            // SAFETY: static format.
            PrintOut(b")\0".as_ptr() as *const c_char);
        }
        // SAFETY: static format.
        PrintOut(b"\n\0".as_ptr() as *const c_char);
        // SAFETY: fp is math_out or a successfully opened file; the C
        // original closes only the file form (fp != math_out).
        if fp != math_out {
            fclose(fp);
        }
    } else {
        // SAFETY: static format; live name arg.
        error_matc(b"Function definition not found: %s\n\0".as_ptr() as *const c_char, s);
    }
    // SAFETY: s from var_to_string above.
    mem_free(s as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C fnc_free_entry (body + names + list unlink).
#[no_mangle]
pub unsafe extern "C" fn fnc_free_entry(fnc: *mut FUNCTION) {
    // SAFETY: body is a live clause tree (or NULL; free_clause handles it
    // the same way the C original does).
    free_clause((*fnc).body);
    if (*fnc).parcount > 0 {
        let mut i: c_int = 0;
        while i < (*fnc).parcount {
            // SAFETY: parnames[i] mem_alloc'd.
            mem_free(*(*fnc).parnames.offset(i as isize) as *mut core::ffi::c_void);
            i += 1;
        }
        // SAFETY: parnames array mem_alloc'd.
        mem_free((*fnc).parnames as *mut core::ffi::c_void);
    }
    if !(*fnc).imports.is_null() {
        let mut i: c_int = 0;
        while !(*(*fnc).imports.offset(i as isize)).is_null() {
            // SAFETY: imports[i] mem_alloc'd.
            mem_free(*(*fnc).imports.offset(i as isize) as *mut core::ffi::c_void);
            i += 1;
        }
        // SAFETY: imports array mem_alloc'd.
        mem_free((*fnc).imports as *mut core::ffi::c_void);
    }
    if !(*fnc).exports.is_null() {
        let mut i: c_int = 0;
        while !(*(*fnc).exports.offset(i as isize)).is_null() {
            // SAFETY: exports[i] mem_alloc'd.
            mem_free(*(*fnc).exports.offset(i as isize) as *mut core::ffi::c_void);
            i += 1;
        }
        // SAFETY: exports array mem_alloc'd.
        mem_free((*fnc).exports as *mut core::ffi::c_void);
    }
    lst_free(FUNCTIONS, fnc as *mut LIST);
}

// SAFETY: mirrors C fnc_free (drains the FUNCTIONS list).
#[no_mangle]
pub unsafe extern "C" fn fnc_free() {
    // SAFETY: FUNC_HEAD is the live global function list.
    let mut fnc = (*listheaders.as_mut_ptr().offset(FUNCTIONS as isize)).next as *mut FUNCTION;
    while !fnc.is_null() {
        let fnc1 = (*fnc).next as *mut FUNCTION;
        fnc_free_entry(fnc);
        fnc = fnc1;
    }
    (*listheaders.as_mut_ptr().offset(FUNCTIONS as isize)).next = core::ptr::null_mut();
}

// SAFETY: mirrors C fnc_exec, including the VAR_HEAD swap dance, the
// export wrap (ALLOCMEM VARIABLE sharing MATRIX, refcount bump), and the
// _function_name return convention.
#[no_mangle]
pub unsafe extern "C" fn fnc_exec(fnc: *mut FUNCTION, mut par: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: VAR_HEAD is the live global variable list.
    let mut headsave = var_head();
    let mut i: c_int = 0;
    let mut ptr = par;
    while !ptr.is_null() {
        if ptr.is_null() {
            break;
        }
        if i < (*fnc).parcount {
            // SAFETY: parnames[i] live.
            (*ptr).name = strcopy(*(*fnc).parnames.offset(i as isize) as *const c_char);
        } else {
            // SAFETY: 1-byte mem_alloc block (original stores no name).
            (*ptr).name = mem_alloc(1) as *mut c_char;
        }
        ptr = (*ptr).next;
        i += 1;
    }
    if !(*fnc).imports.is_null() {
        let mut i: c_int = 0;
        while !(*(*fnc).imports.offset(i as isize)).is_null() {
            // SAFETY: var_check on a live import name.
            let imp = var_check(*(*fnc).imports.offset(i as isize));
            if !imp.is_null() {
                set_var_head(par);
                // SAFETY: re-check against the parameter list.
                if var_check(*(*fnc).imports.offset(i as isize)).is_null() {
                    // SAFETY: imp live; copy shares nothing (temp copy).
                    let cpy = var_temp_copy(imp);
                    (*cpy).name = strcopy(*(*fnc).imports.offset(i as isize) as *const c_char);
                    lst_add(VARIABLES, cpy as *mut LIST);
                }
                par = var_head();
                set_var_head(headsave);
            } else {
                // SAFETY: static format; live name args.
                PrintOut(
                    b"WARNING: %s: imported variable [%s] doesn't exist\n\0".as_ptr() as *const c_char,
                    (*fnc).name,
                    *(*fnc).imports.offset(i as isize),
                );
            }
            i += 1;
        }
    }
    set_var_head(par);
    // SAFETY: body is the live function clause tree. Call kept for its side
    // effects; C assigns to res but reads it only after var_check below.
    let mut res: *mut VARIABLE;
    evalclause((*fnc).body);
    par = var_head();
    if !(*fnc).exports.is_null() {
        let mut i: c_int = 0;
        while !(*(*fnc).exports.offset(i as isize)).is_null() {
            // SAFETY: var_check on a live export name.
            let p = var_check(*(*fnc).exports.offset(i as isize));
            if !p.is_null() {
                set_var_head(headsave);
                // SAFETY: fresh VARIABLE sharing MATRIX (refcount bump),
                // exactly like the C #else branch.
                let var = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
                (*var).next = core::ptr::null_mut();
                (*var).this = (*p).this;
                (*(*p).this).refcount += 1;
                (*var).name = strcopy(*(*fnc).exports.offset(i as isize) as *const c_char);
                (*var).changed = 0;
                var_delete(*(*fnc).exports.offset(i as isize));
                lst_add(VARIABLES, var as *mut LIST);
                // C re-reads VAR_HEAD: a lexical insert may have prepended,
                // moving the outer list head. Mirror exactly.
                headsave = var_head();
                set_var_head(par);
            }
            i += 1;
        }
    }
    // SAFETY: _<name> buffer sized strlen+2, same as C.
    let str = mem_alloc(strlen((*fnc).name) as usize + 2) as *mut c_char;
    // SAFETY: str has strlen+2 bytes.
    *str.offset(0) = b'_' as c_char;
    strcpy(str.offset(1), (*fnc).name);
    // SAFETY: var_check on the live return-name.
    res = var_check(str);
    if !res.is_null() {
        lst_unlink(VARIABLES, res as *mut LIST);
        // SAFETY: NAME(res) mem_alloc'd.
        mem_free((*res).name as *mut core::ffi::c_void);
        (*res).next = core::ptr::null_mut();
    } else {
        // NOTE: C calls var_delete_temp(res) with res==NULL (no-op there
        // only if var_delete_temp NULL-checks; mirrored call kept to run
        // the same callee). res stays NULL via the check below.
        var_delete_temp(res);
        res = core::ptr::null_mut();
    }
    // SAFETY: str from mem_alloc above.
    mem_free(str as *mut core::ffi::c_void);
    var_free();
    set_var_head(headsave);
    res
}

// SAFETY: registers funcdel/funclist with byte-identical help texts.
#[no_mangle]
pub unsafe extern "C" fn fnc_com_init() {
    // SAFETY: com_init copies into session memory; statics live forever.
    com_init(
        b"funcdel\0".as_ptr() as *const c_char,
        FALSE,
        FALSE,
        Some(fnc_delete),
        1,
        1,
        b"funcdel(name)\nDelete function definition from parser.\n\0".as_ptr() as *const c_char,
    );
    com_init(
        b"funclist\0".as_ptr() as *const c_char,
        FALSE,
        FALSE,
        Some(fnc_list),
        1,
        2,
        b"funclist(name)\nGive header of a given function.\n\nSEE ALSO: help.\n\0".as_ptr() as *const c_char,
    );
}
