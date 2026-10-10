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

// ---- unit body: transcribed from matc/src/optim.c ----
// Transcribed from matc/src/optim.c (constant-folding tree optimizer).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut math_err: *mut FILE;
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn com_check(nm: *mut c_char) -> *mut COMMAND;
    fn com_pointw(sub: *mut core::ffi::c_void, par: *mut VARIABLE) -> *mut VARIABLE;
    fn com_el(v: *mut VARIABLE) -> *mut VARIABLE;
    fn newtree() -> *mut TREE;
    fn free_tree(t: *mut TREE);
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_delete_temp(v: *mut VARIABLE);
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
}

// TREEENTRY field shorthands (mirror the C access macros on live trees).
// SAFETY: all take live tree pointers.
unsafe fn t_subs(t: *mut TREE) -> *mut TREE {
    (*t).tentry.subs
}
unsafe fn t_set_subs(t: *mut TREE, v: *mut TREE) {
    (*t).tentry.subs = v;
}
unsafe fn t_etype(t: *mut TREE) -> c_int {
    (*t).tentry.entrytype
}
unsafe fn t_set_etype(t: *mut TREE, v: c_int) {
    (*t).tentry.entrytype = v;
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
unsafe fn t_set_cdata(t: *mut TREE, v: *mut VARIABLE) {
    (*t).tentry.entrydata.c_data = v;
}
unsafe fn t_vdata(t: *mut TREE) -> VDataFn {
    (*t).tentry.entrydata.v_data
}
unsafe fn t_args(t: *mut TREE) -> *mut TREE {
    (*t).tentry.args
}
unsafe fn t_set_args(t: *mut TREE, v: *mut TREE) {
    (*t).tentry.args = v;
}

// SAFETY: mirrors C optimtree (constant folding over the LINK chain).
#[no_mangle]
pub unsafe extern "C" fn optimtree(mut root: *mut TREE) -> *mut TREE {
    let mut constant: c_int = TRUE;
    let mut csize: c_int = 0;
    let mut tptr = root;
    let mut tprev = root;
    let mut prevroot: *mut TREE = core::ptr::null_mut();
    while !tptr.is_null() {
        let mut constsubs: c_int = TRUE;
        let mut subs: *mut TREE = core::ptr::null_mut();
        let mut subvar: *mut VARIABLE = core::ptr::null_mut();
        let mut stmp: *mut VARIABLE;
        if !t_subs(tptr).is_null() {
            subs = optimtree(t_subs(tptr));
            t_set_subs(tptr, subs);
            if subs.is_null() {
                // SAFETY: static format string.
                error_matc(b"it's not worth it.\n\0".as_ptr() as *const c_char);
            }
            if t_etype(subs) != ETYPE_CONST || !(*subs).link.is_null() {
                constsubs = FALSE;
            }
            let mut prevsubs = subs;
            subs = (*subs).next;
            while !subs.is_null() {
                subs = optimtree(subs);
                if subs.is_null() {
                    // SAFETY: static format string.
                    error_matc(b"it's not worth it.\n\0".as_ptr() as *const c_char);
                }
                if t_etype(subs) != ETYPE_CONST || !(*subs).link.is_null() {
                    constsubs = FALSE;
                }
                (*prevsubs).next = subs;
                prevsubs = subs;
                subs = (*subs).next;
            }
            if constsubs != 0 {
                subs = t_subs(tptr);
                subvar = t_cdata(subs);
                stmp = subvar;
                subs = (*subs).next;
                while !subs.is_null() {
                    (*stmp).next = t_cdata(subs);
                    subs = (*subs).next;
                    stmp = (*stmp).next;
                }
            }
            subs = t_subs(tptr);
            t_set_subs(tptr, core::ptr::null_mut());
        }
        match t_etype(tptr) {
            x if x == ETYPE_NAME => {
                let mut constargs: c_int = TRUE;
                let mut con: c_int = FALSE;
                let mut argcount: c_int = 0;
                let mut parroot: *mut VARIABLE = core::ptr::null_mut();
                let mut par: *mut VARIABLE;
                let mut tmp: *mut VARIABLE = core::ptr::null_mut();
                if !t_args(tptr).is_null() {
                    let mut args = optimtree(t_args(tptr));
                    t_set_args(tptr, args);
                    if args.is_null() {
                        // SAFETY: static format string.
                        error_matc(b"it's not worth it.\n\0".as_ptr() as *const c_char);
                    }
                    if t_etype(args) != ETYPE_CONST || !(*args).link.is_null() {
                        constargs = FALSE;
                    }
                    let mut prevargs = args;
                    args = (*args).next;
                    argcount += 1;
                    while !args.is_null() {
                        args = optimtree(args);
                        if args.is_null() {
                            // SAFETY: static format string.
                            error_matc(b"it's not worth it.\n\0".as_ptr() as *const c_char);
                        }
                        if t_etype(args) != ETYPE_CONST || !(*args).link.is_null() {
                            constargs = FALSE;
                        }
                        (*prevargs).next = args;
                        prevargs = args;
                        args = (*args).next;
                        argcount += 1;
                    }
                }
                // SAFETY: com_check on the live name.
                let com = com_check(t_sdata(tptr));
                if !com.is_null() && constargs != 0 && ((*com).flags & CMDFLAG_CE) != 0 {
                    if argcount < (*com).minp || argcount > (*com).maxp {
                        if (*com).minp == (*com).maxp {
                            // SAFETY: math_err live; static formats.
                            fprintf(
                                math_err,
                                b"Builtin function [%s] requires %d argument(s).\n\0".as_ptr() as *const c_char,
                                t_sdata(tptr),
                                (*com).minp,
                            );
                            error_matc(b"\0".as_ptr() as *const c_char);
                        } else {
                            // SAFETY: math_err live; static formats.
                            fprintf(
                                math_err,
                                b"Builtin function [%s] takes from %d to %d argument(s).\n\0".as_ptr() as *const c_char,
                                t_sdata(tptr),
                                (*com).minp,
                                (*com).maxp,
                            );
                            error_matc(b"\0".as_ptr() as *const c_char);
                        }
                    }
                    let mut args = t_args(tptr);
                    if !args.is_null() {
                        parroot = t_cdata(args);
                        par = parroot;
                        args = (*args).next;
                        while !args.is_null() {
                            (*par).next = t_cdata(args);
                            args = (*args).next;
                            par = (*par).next;
                        }
                    }
                    if ((*com).flags & CMDFLAG_PW) != 0 {
                        // SAFETY: com_pointw with the stored sub bits.
                        tmp = com_pointw(core::mem::transmute::<CommandSub, *mut core::ffi::c_void>((*com).sub), parroot);
                    } else if let Some(sub) = (*com).sub {
                        // SAFETY: parroot chain built above.
                        tmp = sub(parroot);
                    }
                    par = parroot;
                    while !par.is_null() {
                        parroot = (*par).next;
                        (*par).next = core::ptr::null_mut();
                        par = parroot;
                    }
                    if !tmp.is_null() {
                        let newroot = newtree();
                        if tptr == root {
                            root = newroot;
                        } else {
                            (*tprev).link = newroot;
                        }
                        (*newroot).next = (*tptr).next;
                        (*tptr).next = core::ptr::null_mut();
                        (*newroot).link = (*tptr).link;
                        (*tptr).link = core::ptr::null_mut();
                        free_tree(tptr);
                        tptr = newroot;
                        t_set_etype(tptr, ETYPE_CONST);
                        t_set_cdata(tptr, tmp);
                        if constsubs != 0 {
                            if constant == 0 {
                                prevroot = tprev;
                            }
                            con = TRUE;
                            csize += var_nrow(tmp) * var_ncol(tmp);
                        }
                    }
                }
                constant = con;
            }
            x if x == ETYPE_NUMBER => {
                if constsubs != 0 {
                    if constant == 0 {
                        prevroot = tprev;
                    }
                    constant = TRUE;
                    csize += 1;
                }
            }
            x if x == ETYPE_STRING => {
                if constsubs != 0 {
                    if constant == 0 {
                        prevroot = tprev;
                    }
                    constant = TRUE;
                    // SAFETY: SDATA is a live C string.
                    csize += strlen(t_sdata(tptr)) as c_int;
                }
            }
            x if x == ETYPE_EQUAT => {
                let leftptr = optimtree((*tptr).left);
                (*tptr).left = leftptr;
                if !leftptr.is_null() && t_etype(leftptr) == ETYPE_CONST && (*leftptr).link.is_null() {
                    let newroot = leftptr;
                    if tptr == root {
                        root = newroot;
                    } else {
                        (*tprev).link = newroot;
                    }
                    (*newroot).next = (*tptr).next;
                    (*tptr).next = core::ptr::null_mut();
                    (*newroot).link = (*tptr).link;
                    (*tptr).link = core::ptr::null_mut();
                    (*tptr).left = core::ptr::null_mut();
                    free_tree(tptr);
                    tptr = newroot;
                    if constsubs != 0 {
                        if constant == 0 {
                            prevroot = tprev;
                        }
                        constant = TRUE;
                        csize += var_nrow(t_cdata(tptr)) * var_ncol(t_cdata(tptr));
                    }
                } else {
                    constant = FALSE;
                }
            }
            x if x == ETYPE_OPER => {
                let tmp: *mut VARIABLE;
                let leftptr = optimtree((*tptr).left);
                (*tptr).left = leftptr;
                let rightptr = optimtree((*tptr).right);
                (*tptr).right = rightptr;
                let mut opres: *mut MATRIX = core::ptr::null_mut();
                if !leftptr.is_null() && !rightptr.is_null() {
                    if t_etype(leftptr) == ETYPE_CONST && t_etype(rightptr) == ETYPE_CONST
                        && (*leftptr).link.is_null() && (*rightptr).link.is_null()
                    {
                        // SAFETY: VDATA holds the operator fn (parser-wired).
                        if let Some(f) = t_vdata(tptr) {
                            let g: unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX =
                                core::mem::transmute(f);
                            // SAFETY: both CONST payloads live.
                            opres = g((*t_cdata(leftptr)).this, (*t_cdata(rightptr)).this);
                        }
                        (*t_cdata(leftptr)).next = core::ptr::null_mut();
                    }
                } else if !leftptr.is_null() && t_etype(leftptr) == ETYPE_CONST {
                    if (*leftptr).link.is_null() {
                        // SAFETY: VDATA holds the operator fn.
                        if let Some(f) = t_vdata(tptr) {
                            let g: unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX =
                                core::mem::transmute(f);
                            // SAFETY: CONST payload live.
                            opres = g((*t_cdata(leftptr)).this, core::ptr::null_mut());
                        }
                    }
                } else if !rightptr.is_null() && t_etype(rightptr) == ETYPE_CONST {
                    if (*rightptr).link.is_null() {
                        // SAFETY: VDATA holds the operator fn.
                        if let Some(f) = t_vdata(tptr) {
                            let g: unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX =
                                core::mem::transmute(f);
                            // SAFETY: CONST payload live.
                            opres = g((*t_cdata(rightptr)).this, core::ptr::null_mut());
                        }
                    }
                }
                if !opres.is_null() {
                    // SAFETY: fresh VARIABLE sharing opres (C ALLOCMEM here).
                    tmp = mem_alloc(VARIABLESIZE) as *mut VARIABLE;
                    (*tmp).next = core::ptr::null_mut();
                    (*tmp).name = core::ptr::null_mut();
                    (*tmp).changed = 0;
                    (*tmp).this = opres;
                    (*opres).refcount = 1;
                    let newroot = newtree();
                    if tptr == root {
                        root = newroot;
                    } else {
                        (*tprev).link = newroot;
                    }
                    (*newroot).next = (*tptr).next;
                    (*tptr).next = core::ptr::null_mut();
                    (*newroot).link = (*tptr).link;
                    (*tptr).link = core::ptr::null_mut();
                    free_tree(tptr);
                    tptr = newroot;
                    t_set_etype(tptr, ETYPE_CONST);
                    t_set_cdata(tptr, tmp);
                    if constsubs != 0 {
                        if constant == 0 {
                            prevroot = tprev;
                        }
                        constant = TRUE;
                        csize += var_nrow(tmp) * var_ncol(tmp);
                    }
                } else {
                    constant = FALSE;
                }
            }
            _ => {}
        }
        if constsubs != 0 && constant != 0 && !subs.is_null() {
            if !t_cdata(tptr).is_null() {
                csize -= var_nrow(t_cdata(tptr)) * var_ncol(t_cdata(tptr));
                stmp = t_cdata(tptr);
                (*stmp).next = subvar;
                // SAFETY: stmp chain built above.
                let el = com_el(stmp);
                if !el.is_null() {
                    t_set_cdata(tptr, el);
                    csize += var_nrow(t_cdata(tptr)) * var_ncol(t_cdata(tptr));
                }
                var_delete_temp(stmp);
            }
            free_tree(subs);
            t_set_subs(tptr, core::ptr::null_mut());
        } else if constsubs != 0 && !subs.is_null() {
            t_set_subs(tptr, subs);
            while !subvar.is_null() {
                stmp = (*subvar).next;
                (*subvar).next = core::ptr::null_mut();
                subvar = stmp;
            }
        } else if !subs.is_null() {
            t_set_subs(tptr, subs);
        } else {
            t_set_subs(tptr, core::ptr::null_mut());
        }
        constant &= constsubs;
        if constant == 0 && csize > 0 {
            let newroot = newtree();
            t_set_etype(newroot, ETYPE_CONST);
            let mut ptr = if !prevroot.is_null() { (*prevroot).link } else { root };
            // SAFETY: ptr is a live tree here (csize > 0 implies progress).
            if t_etype(ptr) == ETYPE_STRING {
                t_set_cdata(newroot, var_temp_new(TYPE_STRING, 1, csize));
            } else if t_etype(ptr) == ETYPE_NUMBER {
                t_set_cdata(newroot, var_temp_new(TYPE_DOUBLE, 1, csize));
            } else if t_etype(ptr) == ETYPE_CONST {
                t_set_cdata(newroot, var_temp_new(var_type(t_cdata(ptr)), 1, csize));
            }
            let mut i: c_int = 0;
            while ptr != tptr {
                match t_etype(ptr) {
                    x if x == ETYPE_NUMBER => {
                        var_set_m(t_cdata(newroot), 0, i, t_ddata(ptr));
                        i += 1;
                    }
                    x if x == ETYPE_STRING => {
                        // SAFETY: SDATA is a live C string.
                        let slen = strlen(t_sdata(ptr)) as c_int;
                        let mut j: c_int = 0;
                        while j < slen {
                            // SAFETY: SDATA bytes live.
                            var_set_m(t_cdata(newroot), 0, i, *t_sdata(ptr).offset(j as isize) as c_double);
                            i += 1;
                            j += 1;
                        }
                    }
                    x if x == ETYPE_CONST => {
                        let j = var_matsize(t_cdata(ptr)) as c_int;
                        // SAFETY: both data blocks live; j is the byte size.
                        memcpy(
                            var_matr(t_cdata(newroot)).offset(i as isize) as *mut core::ffi::c_void,
                            var_matr(t_cdata(ptr)) as *const core::ffi::c_void,
                            j as usize,
                        );
                        i += j >> 3;
                    }
                    _ => {}
                }
                ptr = (*ptr).link;
            }
            (*newroot).link = tptr;
            (*tprev).link = core::ptr::null_mut();
            if !prevroot.is_null() {
                free_tree((*prevroot).link);
                (*prevroot).link = newroot;
            } else {
                (*newroot).next = (*root).next;
                (*root).next = core::ptr::null_mut();
                free_tree(root);
                root = newroot;
            }
            constant = FALSE;
            csize = 0;
        }
        tprev = tptr;
        tptr = (*tptr).link;
    }
    if constant != 0 && csize > 0 {
        let newroot = newtree();
        t_set_etype(newroot, ETYPE_CONST);
        let mut ptr = if !prevroot.is_null() { (*prevroot).link } else { root };
        // SAFETY: ptr is a live tree here.
        if t_etype(ptr) == ETYPE_STRING {
            t_set_cdata(newroot, var_temp_new(TYPE_STRING, 1, csize));
        } else if t_etype(ptr) == ETYPE_NUMBER {
            t_set_cdata(newroot, var_temp_new(TYPE_DOUBLE, 1, csize));
        } else if t_etype(ptr) == ETYPE_CONST {
            t_set_cdata(newroot, var_temp_new(var_type(t_cdata(ptr)), 1, csize));
        }
        let mut i: c_int = 0;
        while !ptr.is_null() {
            match t_etype(ptr) {
                x if x == ETYPE_NUMBER => {
                    var_set_m(t_cdata(newroot), 0, i, t_ddata(ptr));
                    i += 1;
                }
                x if x == ETYPE_STRING => {
                    // SAFETY: SDATA is a live C string.
                    let slen = strlen(t_sdata(ptr)) as c_int;
                    let mut j: c_int = 0;
                    while j < slen {
                        // SAFETY: SDATA bytes live.
                        var_set_m(t_cdata(newroot), 0, i, *t_sdata(ptr).offset(j as isize) as c_double);
                        i += 1;
                        j += 1;
                    }
                }
                x if x == ETYPE_CONST => {
                    let j = var_matsize(t_cdata(ptr)) as c_int;
                    // SAFETY: both data blocks live; j is the byte size.
                    memcpy(
                        var_matr(t_cdata(newroot)).offset(i as isize) as *mut core::ffi::c_void,
                        var_matr(t_cdata(ptr)) as *const core::ffi::c_void,
                        j as usize,
                    );
                    i += j >> 3;
                }
                _ => {}
            }
            ptr = (*ptr).link;
        }
        if !prevroot.is_null() {
            free_tree((*prevroot).link);
            (*prevroot).link = newroot;
        } else {
            (*newroot).next = (*root).next;
            (*root).next = core::ptr::null_mut();
            if t_etype(root) == ETYPE_CONST && (*root).link.is_null() {
                (*t_cdata(newroot)).this.as_mut().unwrap().nrow = (*t_cdata(root)).this.as_ref().unwrap().nrow;
                (*t_cdata(newroot)).this.as_mut().unwrap().ncol = (*t_cdata(root)).this.as_ref().unwrap().ncol;
            }
            free_tree(root);
            root = newroot;
        }
    } else if constant != 0 {
        free_tree(root);
        root = core::ptr::null_mut();
    }
    root
}

// SAFETY: mirrors C optimclause (clause-list recursion).
#[no_mangle]
pub unsafe extern "C" fn optimclause(root: *mut CLAUSE) -> *mut CLAUSE {
    let mut cptr = root;
    while !cptr.is_null() {
        match (*cptr).data {
            x if x == funcsym => {
                (*cptr).this = optimtree((*cptr).this);
                (*cptr).link = optimclause((*cptr).link);
                return root;
            }
            x if x == assignsym => {
                if !(*cptr).this.is_null() {
                    (*cptr).this = optimtree((*cptr).this);
                }
                (*(*cptr).link).this = optimtree((*(*cptr).link).this);
                cptr = (*cptr).link;
            }
            x if x == ifsym => {
                (*cptr).this = optimtree((*cptr).this);
                (*cptr).link = optimclause((*cptr).link);
                cptr = (*cptr).jmp;
                if (*cptr).data == elsesym {
                    (*cptr).link = optimclause((*cptr).link);
                    cptr = (*cptr).jmp;
                }
            }
            x if x == whilesym => {
                (*cptr).this = optimtree((*cptr).this);
                (*cptr).link = optimclause((*cptr).link);
                cptr = (*cptr).jmp;
            }
            x if x == forsym => {
                (*(*cptr).this).link = optimtree((*(*cptr).this).link);
                (*cptr).link = optimclause((*cptr).link);
                cptr = (*cptr).jmp;
            }
            x if x == endsym => {
                return root;
            }
            _ => {}
        }
        cptr = (*cptr).link;
    }
    root
}
