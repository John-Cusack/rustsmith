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

// ---- unit body: transcribed from matc/src/eval.c ----
// Transcribed from matc/src/eval.c (tree/clause evaluator + assignment).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn com_check(nm: *mut c_char) -> *mut COMMAND;
    fn com_pointw(sub: *mut core::ffi::c_void, par: *mut VARIABLE) -> *mut VARIABLE;
    fn com_source(v: *mut VARIABLE) -> *mut VARIABLE;
    fn com_el(v: *mut VARIABLE) -> *mut VARIABLE;
    fn var_check(nm: *mut c_char) -> *mut VARIABLE;
    fn var_new(nm: *mut c_char, typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_rename(v: *mut VARIABLE, nm: *mut c_char) -> *mut VARIABLE;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_wrapper_new(m: *mut MATRIX) -> *mut VARIABLE;
    fn var_delete(nm: *mut c_char);
    fn var_delete_temp(v: *mut VARIABLE);
    fn var_print(v: *mut VARIABLE);
    fn fnc_check(nm: *mut c_char) -> *mut FUNCTION;
    fn fnc_exec(f: *mut FUNCTION, par: *mut VARIABLE) -> *mut VARIABLE;
    fn lst_find(list: c_int, nm: *mut c_char) -> *mut LIST;
    fn mat_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut MATRIX;
    fn mat_copy(m: *mut MATRIX) -> *mut MATRIX;
    fn lst_add(list: c_int, item: *mut LIST);
    fn mat_free(m: *mut MATRIX);
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn PrintOut(fmt: *const c_char, ...);
}

// TREEENTRY shorthands (mirror the C access macros on live trees).
// SAFETY: all take live tree pointers.
unsafe fn t_subs(t: *mut TREE) -> *mut TREE {
    (*t).tentry.subs
}
unsafe fn t_args(t: *mut TREE) -> *mut TREE {
    (*t).tentry.args
}
unsafe fn t_etype(t: *mut TREE) -> c_int {
    (*t).tentry.entrytype
}
unsafe fn t_sdata(t: *mut TREE) -> *mut c_char {
    (*t).tentry.entrydata.s_data
}
unsafe fn t_ddata(t: *mut TREE) -> c_double {
    (*t).tentry.entrydata.d_data
}
unsafe fn t_cdata(t: *mut TREE) -> *mut VARIABLE {
    (*t).tentry.entrydata.c_data
}
unsafe fn t_vdata(t: *mut TREE) -> VDataFn {
    (*t).tentry.entrydata.v_data
}

// Operator call through VDATA (parser-wired MATRIX fn; same casts as C).
// SAFETY: VDATA holds the operator for the node (NULL would crash like C).
unsafe fn call_oper(f: VDataFn, a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX {
    let g: unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX = core::mem::transmute(f);
    g(a, b)
}

// SAFETY: mirrors C evaltree (equation-tree evaluation, temporaries owned
// through the chain exactly like the original).
#[no_mangle]
pub unsafe extern "C" fn evaltree(mut root: *mut TREE) -> *mut VARIABLE {
    if root.is_null() {
        return core::ptr::null_mut();
    }
    let mut dim: c_int = 0;
    let mut first: *mut VARIABLE = core::ptr::null_mut();
    let mut tmp: *mut VARIABLE = core::ptr::null_mut();
    let mut res: *mut VARIABLE = core::ptr::null_mut();
    while !root.is_null() {
        let mut subs: *mut VARIABLE = core::ptr::null_mut();
        let mut par: *mut VARIABLE = core::ptr::null_mut();
        tmp = core::ptr::null_mut();
        let mut argptr = t_subs(root);
        if !argptr.is_null() {
            subs = evaltree(argptr);
            tmp = subs;
            argptr = (*argptr).next;
            while !argptr.is_null() {
                (*tmp).next = evaltree(argptr);
                argptr = (*argptr).next;
                tmp = (*tmp).next;
            }
        }
        match t_etype(root) {
            x if x == ETYPE_NAME => {
                argptr = t_args(root);
                let mut argcount: c_int = 0;
                if !argptr.is_null() {
                    par = evaltree(argptr);
                    tmp = par;
                    argptr = (*argptr).next;
                    argcount += 1;
                    while !argptr.is_null() {
                        argcount += 1;
                        (*tmp).next = evaltree(argptr);
                        argptr = (*argptr).next;
                        tmp = (*tmp).next;
                    }
                }
                // SAFETY: com_check on the live name.
                let com = com_check(t_sdata(root));
                if !com.is_null() {
                    if argcount < (*com).minp || argcount > (*com).maxp {
                        if (*com).minp == (*com).maxp {
                            // SAFETY: static format; live args.
                            error_matc(
                                b"Builtin function [%s] requires %d argument(s).\n\0".as_ptr() as *const c_char,
                                t_sdata(root),
                                (*com).minp,
                            );
                        } else {
                            // SAFETY: static format; live args.
                            error_matc(
                                b"Builtin function [%s] takes from %d to %d argument(s).\n\0".as_ptr() as *const c_char,
                                t_sdata(root),
                                (*com).minp,
                                (*com).maxp,
                            );
                        }
                    }
                    if ((*com).flags & CMDFLAG_PW) != 0 {
                        // SAFETY: com_pointw with the stored sub bits.
                        tmp = com_pointw(core::mem::transmute::<CommandSub, *mut core::ffi::c_void>((*com).sub), par);
                    } else if let Some(sub) = (*com).sub {
                        // SAFETY: par chain built above.
                        tmp = sub(par);
                    }
                } else {
                    // SAFETY: var_check on the live name.
                    let tmp1 = var_check(t_sdata(root));
                    if !tmp1.is_null() {
                        // SAFETY: tmp1 matrix live.
                        tmp = var_wrapper_new((*tmp1).this);
                        if !par.is_null() {
                            subs = par;
                            par = core::ptr::null_mut();
                        }
                    } else {
                        // SAFETY: fnc_check on the live name.
                        let fnc = fnc_check(t_sdata(root));
                        if !fnc.is_null() {
                            // SAFETY: fnc + par chain live.
                            tmp = fnc_exec(fnc, par);
                            par = core::ptr::null_mut();
                        } else {
                            // SAFETY: SDATA is a live C string.
                            let fp = fopen(t_sdata(root), b"r\0".as_ptr() as *const c_char);
                            if !fp.is_null() {
                                // SAFETY: fp live.
                                fclose(fp);
                                // SAFETY: strlen on the live name.
                                let slen = strlen(t_sdata(root));
                                // SAFETY: fresh temp owns its storage.
                                tmp = var_temp_new(TYPE_STRING, 1, slen as c_int);
                                let mut i: c_int = 0;
                                // SAFETY: strlen re-evaluated per iteration
                                // exactly like the C loop condition.
                                while (i as usize) < strlen(t_sdata(root)) {
                                    // SAFETY: tmp data live; name bytes live.
                                    var_set_m(tmp, 0, i, *t_sdata(root).offset(i as isize) as c_double);
                                    i += 1;
                                }
                                com_source(tmp);
                                var_delete_temp(tmp);
                                tmp = core::ptr::null_mut();
                            } else {
                                // SAFETY: static format; live name arg.
                                error_matc(
                                    b"Undeclared identifier: [%s].\n\0".as_ptr() as *const c_char,
                                    t_sdata(root),
                                );
                            }
                        }
                    }
                }
            }
            x if x == ETYPE_STRING => {
                // SAFETY: strlen on the live constant string.
                let slen = strlen(t_sdata(root));
                // SAFETY: fresh temp owns its storage.
                tmp = var_temp_new(TYPE_STRING, 1, slen as c_int);
                let mut i: c_int = 0;
                // SAFETY: strlen re-evaluated per iteration like C.
                while (i as usize) < strlen(t_sdata(root)) {
                    // SAFETY: tmp data live; string bytes live.
                    var_set_m(tmp, 0, i, *t_sdata(root).offset(i as isize) as c_double);
                    i += 1;
                }
            }
            x if x == ETYPE_NUMBER => {
                // SAFETY: fresh temp owns its storage.
                tmp = var_temp_new(TYPE_DOUBLE, 1, 1);
                // SAFETY: tmp data live through the chain.
                var_set_m(tmp, 0, 0, t_ddata(root));
            }
            x if x == ETYPE_CONST => {
                // SAFETY: payload matrix live.
                tmp = var_wrapper_new((*t_cdata(root)).this);
            }
            x if x == ETYPE_EQUAT => {
                tmp = evaltree((*root).left);
            }
            x if x == ETYPE_OPER => {
                // SAFETY: leaves evaluated below; payloads live.
                let leftptr = evaltree((*root).left);
                let rightptr = evaltree((*root).right);
                let mut opres: *mut MATRIX = core::ptr::null_mut();
                if !leftptr.is_null() && !rightptr.is_null() {
                    opres = call_oper(t_vdata(root), (*leftptr).this, (*rightptr).this);
                } else if !leftptr.is_null() {
                    opres = call_oper(t_vdata(root), (*leftptr).this, core::ptr::null_mut());
                } else if !rightptr.is_null() {
                    opres = call_oper(t_vdata(root), (*rightptr).this, core::ptr::null_mut());
                }
                var_delete_temp(leftptr);
                var_delete_temp(rightptr);
                if !opres.is_null() {
                    if (*opres).nrow == 1 && (*opres).ncol == 1 && (*opres).typ == TYPE_DOUBLE {
                        // SAFETY: fresh pool temp owns its storage.
                        tmp = var_temp_new(TYPE_DOUBLE, 1, 1);
                        // SAFETY: both cells live; opres at ALLOC_HEAD.
                        *var_matr(tmp) = *(*opres).data;
                        mat_free(opres);
                    } else {
                        // SAFETY: fresh ALLOCMEM block.
                        tmp = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
                        (*tmp).next = core::ptr::null_mut();
                        (*tmp).name = core::ptr::null_mut();
                        (*tmp).changed = 0;
                        (*tmp).this = opres;
                        (*opres).refcount = 1;
                    }
                }
            }
            _ => {}
        }
        if !subs.is_null() {
            if !tmp.is_null() {
                let tmp1 = tmp;
                (*tmp1).next = subs;
                // SAFETY: tmp1 chain built above.
                tmp = com_el(tmp1);
                var_delete_temp(tmp1);
            } else {
                var_delete_temp(subs);
            }
            subs = core::ptr::null_mut();
        }
        if first.is_null() {
            first = tmp;
            res = tmp;
        } else if !tmp.is_null() {
            (*res).next = tmp;
            res = (*res).next;
        }
        if !subs.is_null() {
            var_delete_temp(subs);
        }
        if !par.is_null() {
            var_delete_temp(par);
        }
        if !tmp.is_null() {
            dim += var_nrow(tmp) * var_ncol(tmp);
        }
        root = (*root).link;
    }
    if tmp == first {
        return first;
    }
    // SAFETY: fresh temp owns its storage.
    res = var_temp_new(var_type(first), 1, dim);
    // SAFETY: res data live; sources live until var_delete_temp below.
    let mut resbeg = var_matr(res) as *mut c_char;
    let mut t2 = first;
    while !t2.is_null() {
        // SAFETY: memcpy of the live source bytes.
        memcpy(
            resbeg as *mut core::ffi::c_void,
            var_matr(t2) as *const core::ffi::c_void,
            var_matsize(t2),
        );
        resbeg = resbeg.add(var_matsize(t2));
        t2 = (*t2).next;
    }
    var_delete_temp(first);
    res
}

// SAFETY: mirrors C evalclause (statement-list evaluation with
// assign/if/while/for/funcdef/system forms).
#[no_mangle]
pub unsafe extern "C" fn evalclause(mut root: *mut CLAUSE) -> *mut VARIABLE {
    let mut ptr: *mut VARIABLE = core::ptr::null_mut();
    while !root.is_null() {
        if (*root).data == endsym {
            return ptr;
        }
        match (*root).data {
            x if x == systemcall => {
                // SAFETY: SDATA of the live command tree.
                let fp = popen(t_sdata_sys((*root).this), b"r\0".as_ptr() as *const c_char);
                // SAFETY: fixed 121-byte stack buffer like C.
                let mut s = [0 as c_char; 121];
                if fp.is_null() {
                    // SAFETY: static format; live command arg.
                    error_matc(
                        b"systemcall: open failure: [%s].\n\0".as_ptr() as *const c_char,
                        t_sdata_sys((*root).this),
                    );
                }
                // SAFETY: fp live; fgets into the 120-byte window.
                while !fgets(s.as_mut_ptr(), 120, fp).is_null() {
                    // SAFETY: PrintOut appends; buffer NUL-terminated.
                    PrintOut(s.as_ptr());
                }
                // SAFETY: fp live.
                pclose(fp);
            }
            x if x == funcsym => {
                // SAFETY: SDATA of the live function-name tree.
                let nm = t_sdata_sys((*root).this);
                // SAFETY: var_check/com_check on the live name.
                if !var_check(nm).is_null() || !com_check(nm).is_null() {
                    // SAFETY: static format; live name arg.
                    error_matc(
                        b"Function not created [%s], identifier in use.\n\0".as_ptr() as *const c_char,
                        nm,
                    );
                }
                // SAFETY: fnc_check on the live name.
                let old = fnc_check(nm);
                if !old.is_null() {
                    fnc_free_entry(old);
                }
                // SAFETY: fresh ALLOCMEM block.
                let fnc = mem_alloc(size_of::<FUNCTION>()) as *mut FUNCTION;
                // SAFETY: strcopy helper below (STRCOPY equivalent).
                (*fnc).name = strcopy(t_sdata_sys((*root).this) as *const c_char);
                // NOTE: lst_add(FUNCTIONS,...) below after parnames.
                let mut argcount: c_int = 0;
                let mut tptr = t_args_sys((*root).this);
                while !tptr.is_null() {
                    argcount += 1;
                    tptr = (*tptr).next;
                }
                if argcount > 0 {
                    // SAFETY: C-heap name pointer array.
                    (*fnc).parnames = mem_alloc((argcount as usize) * size_of::<*mut c_char>()) as *mut *mut c_char;
                    let mut i: c_int = 0;
                    tptr = t_args_sys((*root).this);
                    while !tptr.is_null() {
                        // SAFETY: SDATA of the live arg tree.
                        *(*fnc).parnames.offset(i as isize) = strcopy(t_sdata_sys(tptr) as *const c_char);
                        i += 1;
                        tptr = (*tptr).next;
                    }
                } else {
                    (*fnc).parnames = core::ptr::null_mut();
                }
                (*fnc).parcount = argcount;
                argcount = 0;
                let mut n: c_int = 0;
                tptr = t_subs_sys((*root).this);
                while !tptr.is_null() {
                    if !t_sdata_sys(tptr).is_null() {
                        argcount += 1;
                        // SAFETY: strlen on the live help line.
                        n += strlen(t_sdata_sys(tptr)) as c_int;
                    }
                    tptr = (*tptr).next;
                }
                if argcount > 0 && n > 0 {
                    // SAFETY: C-heap help buffer (calloc-zeroed: strcat
                    // starts from NUL like the C original).
                    (*fnc).help = mem_alloc((n + argcount + 1) as usize) as *mut c_char;
                    tptr = t_subs_sys((*root).this);
                    while !tptr.is_null() {
                        if !t_sdata_sys(tptr).is_null() {
                            // SAFETY: help buffer sized for all lines.
                            strcat((*fnc).help, t_sdata_sys(tptr));
                            strcat((*fnc).help, b"\n\0".as_ptr() as *const c_char);
                        }
                        tptr = (*tptr).next;
                    }
                } else {
                    (*fnc).help = core::ptr::null_mut();
                }
                argcount = 0;
                tptr = (*(*root).this).left;
                while !tptr.is_null() {
                    argcount += 1;
                    tptr = (*tptr).next;
                }
                if argcount > 0 {
                    // SAFETY: C-heap import array (+1 for NULL).
                    (*fnc).imports = mem_alloc(((argcount + 1) as usize) * size_of::<*mut c_char>()) as *mut *mut c_char;
                    let mut i: c_int = 0;
                    tptr = (*(*root).this).left;
                    while !tptr.is_null() {
                        // SAFETY: SDATA of the live import tree.
                        *(*fnc).imports.offset(i as isize) = strcopy(t_sdata_sys(tptr) as *const c_char);
                        i += 1;
                        tptr = (*tptr).next;
                    }
                    *(*fnc).imports.offset(i as isize) = core::ptr::null_mut();
                } else {
                    (*fnc).imports = core::ptr::null_mut();
                }
                argcount = 0;
                tptr = (*(*root).this).right;
                while !tptr.is_null() {
                    argcount += 1;
                    tptr = (*tptr).next;
                }
                if argcount > 0 {
                    // SAFETY: C-heap export array (+1 for NULL).
                    (*fnc).exports = mem_alloc(((argcount + 1) as usize) * size_of::<*mut c_char>()) as *mut *mut c_char;
                    let mut i: c_int = 0;
                    tptr = (*(*root).this).right;
                    while !tptr.is_null() {
                        // SAFETY: SDATA of the live export tree.
                        *(*fnc).exports.offset(i as isize) = strcopy(t_sdata_sys(tptr) as *const c_char);
                        i += 1;
                        tptr = (*tptr).next;
                    }
                    *(*fnc).exports.offset(i as isize) = core::ptr::null_mut();
                } else {
                    (*fnc).exports = core::ptr::null_mut();
                }
                (*fnc).next = core::ptr::null_mut();
                (*fnc).body = (*root).link;
                (*root).link = core::ptr::null_mut();
                lst_add_helper(fnc);
                return core::ptr::null_mut();
            }
            x if x == assignsym => {
                let mut iflg: c_int = FALSE;
                let mut pflg: c_int = TRUE;
                // SAFETY: "ans" static lives forever.
                let mut r = b"ans\0".as_ptr() as *mut c_char;
                let mut par: *mut VARIABLE = core::ptr::null_mut();
                if !(*root).this.is_null() {
                    // SAFETY: SDATA of the live target tree.
                    r = t_sdata_sys((*root).this);
                    // SAFETY: *_check on the live target name.
                    if !fnc_check(r).is_null() || !com_check(r).is_null() || !lst_find(CONSTANTS, r).is_null() {
                        // SAFETY: static format; live name arg.
                        error_matc(
                            b"VARIABLE not created [%s], identifier in use.\n\0".as_ptr() as *const c_char,
                            r,
                        );
                    }
                    pflg = FALSE;
                    let mut argptr = t_args_sys((*root).this);
                    if !argptr.is_null() {
                        iflg = TRUE;
                        par = evaltree(argptr);
                        let mut tmp = par;
                        if !tmp.is_null() {
                            argptr = (*argptr).next;
                            while !argptr.is_null() {
                                (*tmp).next = evaltree(argptr);
                                if (*tmp).next.is_null() {
                                    break;
                                }
                                argptr = (*argptr).next;
                                tmp = (*tmp).next;
                            }
                        }
                    }
                }
                // SAFETY: LINK tree live.
                ptr = evaltree((*(*root).link).this);
                ptr = put_result(ptr, r, par, iflg, pflg);
                if !par.is_null() {
                    var_delete_temp(par);
                }
                root = (*root).link;
            }
            x if x == ifsym => {
                // SAFETY: condition tree live.
                let res = evaltree((*root).this);
                if !res.is_null() {
                    // SAFETY: res data live through the scan.
                    let mut d = var_matr(res);
                    let mut i: c_int = 0;
                    while i < var_nrow(res) * var_ncol(res) {
                        if *d == 0.0 {
                            break;
                        }
                        d = d.offset(1);
                        i += 1;
                    }
                    d = d.offset(-1);
                    if *d == 0.0 {
                        root = (*root).jmp;
                        if (*root).data == elsesym {
                            // SAFETY: LINK tree live.
                            ptr = evalclause((*root).link);
                            root = (*root).jmp;
                        }
                    } else {
                        // SAFETY: LINK tree live.
                        ptr = evalclause((*root).link);
                        root = (*root).jmp;
                        if (*root).data == elsesym {
                            root = (*root).jmp;
                        }
                    }
                    var_delete_temp(res);
                } else {
                    root = (*root).jmp;
                    if (*root).data == elsesym {
                        root = (*root).jmp;
                    }
                }
            }
            x if x == whilesym => {
                loop {
                    // SAFETY: condition tree live.
                    let res = evaltree((*root).this);
                    if res.is_null() {
                        break;
                    }
                    // SAFETY: res data live through the scan.
                    let mut d = var_matr(res);
                    let mut i: c_int = 0;
                    while i < var_nrow(res) * var_ncol(res) {
                        if *d == 0.0 {
                            break;
                        }
                        d = d.offset(1);
                        i += 1;
                    }
                    d = d.offset(-1);
                    if *d != 0.0 {
                        // SAFETY: LINK tree live.
                        ptr = evalclause((*root).link);
                        var_delete_temp(res);
                    } else {
                        var_delete_temp(res);
                        break;
                    }
                }
                root = (*root).jmp;
            }
            x if x == forsym => {
                // SAFETY: SDATA of the live loop tree.
                let r = t_sdata_sys((*root).this);
                // SAFETY: *_check on the live loop name.
                if !fnc_check(r).is_null() || !com_check(r).is_null() || !lst_find(CONSTANTS, r).is_null() {
                    // SAFETY: static format (original trailing space kept).
                    error_matc(
                        b"VARIABLE not created [%s], identifier in use.\n \0".as_ptr() as *const c_char,
                        r,
                    );
                }
                // SAFETY: LINK tree live.
                let res = evaltree((*(*root).this).link);
                if !res.is_null() {
                    // SAFETY: var_check/new on live names.
                    let mut var = var_check(r);
                    if var.is_null() {
                        var = var_new(r, var_type(res), 1, 1);
                    }
                    // SAFETY: res data + var cell live.
                    let mut d = var_matr(res);
                    let mut i: c_int = 0;
                    while i < var_ncol(res) * var_nrow(res) {
                        *var_matr(var) = *d;
                        d = d.offset(1);
                        // SAFETY: LINK tree live.
                        ptr = evalclause((*root).link);
                        i += 1;
                    }
                    var_delete_temp(res);
                }
                root = (*root).jmp;
            }
            _ => {}
        }
        root = (*root).link;
    }
    ptr
}

// Clause-tree SDATA/ARGS/SUBS shorthands (same layout as t_*, named apart
// to keep the two call families visually distinct).
// SAFETY: all take live clause/tree pointers.
unsafe fn t_sdata_sys(t: *mut TREE) -> *mut c_char {
    (*t).tentry.entrydata.s_data
}
unsafe fn t_args_sys(t: *mut TREE) -> *mut TREE {
    (*t).tentry.args
}
unsafe fn t_subs_sys(t: *mut TREE) -> *mut TREE {
    (*t).tentry.subs
}
// SAFETY: STRCOPY equivalent (strcpy into a fresh mem_alloc block).
unsafe fn strcopy(s: *const c_char) -> *mut c_char {
    // SAFETY: strlen on a live C string; fresh block sized len+1.
    let n = strlen(s);
    // SAFETY: mem_alloc never returns NULL (errors instead).
    let dst = mem_alloc(n + 1) as *mut c_char;
    // SAFETY: dst has n+1 bytes; strcpy NUL-terminates.
    strcpy(dst, s);
    dst
}
// SAFETY: lst_add(FUNCTIONS, ...) on a live entry.
unsafe fn lst_add_helper(fnc: *mut FUNCTION) {
    // SAFETY: FUNCTIONS list live.
    lst_add(FUNCTIONS, fnc as *mut LIST);
}
// SAFETY: mirrors C put_values (indexed assignment incl. the logical-mask
// fast path and the grow/copy-on-write paths).
#[no_mangle]
pub unsafe extern "C" fn put_values(
    ptr: *mut VARIABLE,
    resname: *mut c_char,
    par: *mut VARIABLE,
) -> *mut VARIABLE {
    // SAFETY: fn-static cell mirrors the C original.
    static mut defind: c_double = 0.0;
    // SAFETY: var_check on a live name.
    let res = var_check(resname);
    if (*par).next.is_null() {
        if !res.is_null()
            && var_nrow(par) == var_nrow(res)
            && var_ncol(par) == var_ncol(res)
            && !(var_nrow(res) == 1 && var_ncol(res) == 1)
        {
            let mut logical: c_int = TRUE;
            let mut csize: c_int;
            // SAFETY: par data live.
            let dtmp0 = var_matr(par);
            let mut i: c_int = 0;
            while i < var_nrow(par) * var_ncol(par) {
                // SAFETY: par cells live.
                if *dtmp0.offset(i as isize) != 0.0 && *dtmp0.offset(i as isize) != 1.0 {
                    logical = FALSE;
                    break;
                }
                i += 1;
            }
            if logical != 0 {
                let imax1 = var_nrow(ptr) * var_ncol(ptr);
                // SAFETY: ptr data live.
                let dtmp = var_matr(ptr);
                let mut i: c_int = 0;
                let mut k: c_int = 0;
                while i < var_nrow(res) {
                    let mut j: c_int = 0;
                    csize = 0;
                    while j < var_ncol(res) {
                        while var_m(par, i, j) == 1.0 && j + csize < var_ncol(res) && k + csize < imax1 {
                            csize += 1;
                        }
                        if csize > 0 {
                            // SAFETY: res/ptr cells live; byte ranges match.
                            memcpy(
                                var_matr(res).offset((i * var_ncol(res) + j) as isize) as *mut core::ffi::c_void,
                                dtmp.offset(k as isize) as *const core::ffi::c_void,
                                (csize as usize) * size_of::<c_double>(),
                            );
                            j += csize - 1;
                            k += csize;
                            csize = 0;
                            if k >= imax1 {
                                k = 0;
                            }
                        }
                        j += 1;
                    }
                    i += 1;
                }
                var_delete_temp(ptr);
                return res;
            } else {
                // ind1=&defind,size1=1; ind2=MATR(par),size2=NCOL(par)
                return put_values_indexed(ptr, res, resname, core::ptr::addr_of_mut!(defind), 1, var_matr(par), var_ncol(par));
            }
        } else {
            return put_values_indexed(ptr, res, resname, core::ptr::addr_of_mut!(defind), 1, var_matr(par), var_ncol(par));
        }
    } else {
        return put_values_indexed(
            ptr,
            res,
            resname,
            var_matr(par),
            var_ncol(par),
            var_matr((*par).next),
            var_ncol((*par).next),
        );
    }
}

// Indexed tail of put_values, shared by the three index-source shapes.
// SAFETY: ind1/ind2 are live int-valued double cells (or the defind cell).
unsafe fn put_values_indexed(
    ptr: *mut VARIABLE,
    mut res: *mut VARIABLE,
    resname: *mut c_char,
    ind1: *mut c_double,
    size1: c_int,
    ind2: *mut c_double,
    size2: c_int,
) -> *mut VARIABLE {
    // C max macro on pure int args: manual ternary, same value.
    let imax = |a: c_int, b: c_int| -> c_int { if a > b { a } else { b } };
    // SAFETY: ind cells live.
    let mut imax1 = *ind1 as c_int;
    let mut i: c_int = 1;
    while i < size1 {
        imax1 = imax(imax1, *ind1.offset(i as isize) as c_int);
        i += 1;
    }
    // SAFETY: ind cells live.
    let mut imax2 = *ind2 as c_int;
    let mut i: c_int = 1;
    while i < size2 {
        imax2 = imax(imax2, *ind2.offset(i as isize) as c_int);
        i += 1;
    }
    if res.is_null() {
        // SAFETY: var_new returns a live global.
        res = var_new(resname, var_type(ptr), imax1 + 1, imax2 + 1);
    } else if var_nrow(res) <= imax1 || var_ncol(res) <= imax2 {
        let ir = var_nrow(res);
        let jc = var_ncol(res);
        imax1 = imax(ir, imax1 + 1);
        imax2 = imax(jc, imax2 + 1);
        // SAFETY: mat_new returns a live matrix.
        let t = mat_new(var_type(res), imax1, imax2);
        // SAFETY: t data live.
        let dtmp = (*t).data;
        let mut i: c_int = 0;
        while i < ir {
            // SAFETY: row byte ranges live.
            memcpy(
                dtmp.offset((i * imax2) as isize) as *mut core::ffi::c_void,
                var_matr(res).offset((i * var_ncol(res)) as isize) as *const core::ffi::c_void,
                (jc as usize) * size_of::<c_double>(),
            );
            i += 1;
        }
        (*(*res).this).refcount -= 1;
        if (*(*res).this).refcount == 0 {
            mat_free((*res).this);
        }
        (*res).this = t;
        (*t).refcount = 1;
    } else if (*(*res).this).refcount > 1 {
        (*(*res).this).refcount -= 1;
        // SAFETY: mat_copy returns a live matrix.
        (*res).this = mat_copy((*res).this);
    }
    let imax1n = var_nrow(ptr) * var_ncol(ptr);
    // SAFETY: ptr data live.
    let dtmp = var_matr(ptr);
    let mut i: c_int = 0;
    let mut k: c_int = 0;
    while i < size1 {
        let ind = *ind1.offset(i as isize) as c_int;
        let mut j: c_int = 0;
        while j < size2 {
            // SAFETY: res/ptr cells live; k wraps like C.
            var_set_m(res, ind, *ind2.offset(j as isize) as c_int, *dtmp.offset(k as isize));
            k += 1;
            if k >= imax1n {
                k = 0;
            }
            j += 1;
        }
        i += 1;
    }
    var_delete_temp(ptr);
    res
}

// SAFETY: mirrors C put_result (ans cleanup + indexed/plain store + print).
#[no_mangle]
pub unsafe extern "C" fn put_result(
    ptr: *mut VARIABLE,
    resname: *mut c_char,
    par: *mut VARIABLE,
    indexflag: c_int,
    printflag: c_int,
) -> *mut VARIABLE {
    var_delete(b"ans\0".as_ptr() as *mut c_char);
    let res: *mut VARIABLE = if indexflag != 0 && !par.is_null() {
        put_values(ptr, resname, par)
    } else {
        // SAFETY: var_rename on the live chain + name.
        var_rename(ptr, resname)
    };
    if !res.is_null() {
        (*res).changed = 1;
    }
    if printflag != 0 {
        var_print(res);
    }
    res
}

// Promise the linker the funcs-crate entry this file conceptually shares:
// (no-op; fnc_free_entry lives in the funcs crate and is extern below.)
extern "C" {
    fn fnc_free_entry(fnc: *mut FUNCTION);
}
