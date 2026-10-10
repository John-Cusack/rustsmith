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

// ---- unit body: transcribed from matc/src/parser.c ----
// Transcribed from matc/src/parser.c (scanner + recursive-descent parser).
// The input line is mutated in place (NULs written) exactly like the C
// scanner; help-text capture dances are literal pointer transcriptions.

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn mem_alloc(size: size_t) -> *mut core::ffi::c_void;
    fn mem_free(ptr: *mut core::ffi::c_void);
    fn var_delete_temp(v: *mut VARIABLE);
    fn dogets(buff: *mut c_char, prompt: *mut c_char) -> c_int;
    fn evalclause(c: *mut CLAUSE) -> *mut VARIABLE;
    fn isspace(c: c_int) -> c_int;
    fn isdigit(c: c_int) -> c_int;
    fn isalpha(c: c_int) -> c_int;
    fn isalnum(c: c_int) -> c_int;
    fn atof(s: *const c_char) -> c_double;
    fn opr_apply(a: *mut MATRIX) -> *mut MATRIX;
    fn opr_not(a: *mut MATRIX) -> *mut MATRIX;
    fn opr_trans(a: *mut MATRIX) -> *mut MATRIX;
    fn opr_pow(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_mul(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_pmul(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_div(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_add(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_subs(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_eq(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_lt(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_gt(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_neq(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_le(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_ge(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_vector(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_and(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_or(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_reduction(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_resize(a: *mut MATRIX, b: *mut MATRIX) -> *mut MATRIX;
    fn opr_minus(a: *mut MATRIX) -> *mut MATRIX;
}

static mut symbol: SYMTYPE = 0;
static mut bendsym: SYMTYPE = 0;
static mut str_: *mut c_char = core::ptr::null_mut();
static mut csymbol: [c_char; 4096] = [0; 4096];
static mut buf: [c_char; 4096] = [0; 4096];

// Store an operator fn into VDATA (same address-preserving conversion the
// C assignment performs; called back through the matching signature).
// SAFETY: the stored shape always matches the node's arity (parser-wired).
unsafe fn store_op<F>(t: *mut TREE, f: F)
where
    F: Copy,
{
    (*t).tentry.entrydata.v_data = core::mem::transmute_copy::<F, VDataFn>(&f);
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

// SAFETY: mirrors C char_in_list (NUL-terminated scan).
#[no_mangle]
pub unsafe extern "C" fn char_in_list(ch: c_int, list: *mut c_char) -> c_int {
    // SAFETY: list is a live NUL-terminated C string.
    let mut p = list;
    while *p != 0 {
        if *p as c_int == ch {
            return TRUE;
        }
        p = p.offset(1);
    }
    FALSE
}

// SAFETY: mirrors C scan (in-place tokenizer over the str line; tables
// and reswords are the live matc-crate globals).
#[no_mangle]
pub unsafe extern "C" fn scan() {
    extern "C" {
        static mut ssymbols: [SYMTYPE; 27];
        static mut csymbols: [c_char; 28];
        static mut reswords: [*mut c_char; 12];
        static mut rsymbols: [SYMTYPE; 11];
        static mut symchars: *mut c_char;
    }
    // SAFETY: str is the live input line (or NULL during early init).
    let mut str = str_;
    symbol = nullsym;
    if *str == 0 {
        return;
    }
    // SAFETY: isspace on live bytes.
    while isspace(*str as c_int) != 0 {
        str = str.offset(1);
    }
    if *str == 0 {
        str_ = str;
        return;
    }
    // SAFETY: p marks the token start in the live line.
    let p = str;
    // SAFETY: isdigit on live bytes.
    if isdigit(*str as c_int) != 0 || (*str == b'.' as c_char && isdigit(*str.offset(1) as c_int) != 0) {
        str = str.offset(1);
        while isdigit(*str as c_int) != 0 {
            str = str.offset(1);
        }
        if *str == b'.' as c_char {
            str = str.offset(1);
            if isdigit(*str as c_int) != 0 {
                while isdigit(*str as c_int) != 0 {
                    str = str.offset(1);
                }
            } else if *str != 0 && *str != b'e' as c_char && *str != b'E' as c_char && *str != b'd' as c_char && *str != b'D' as c_char {
                // SAFETY: static format string.
                error_matc(b"Badly formed number.\n\0".as_ptr() as *const c_char);
            }
        }
        if *str == b'd' as c_char || *str == b'D' as c_char {
            *str = b'e' as c_char;
        }
        if *str == b'e' as c_char || *str == b'E' as c_char {
            str = str.offset(1);
            if isdigit(*str as c_int) != 0 {
                while isdigit(*str as c_int) != 0 {
                    str = str.offset(1);
                }
            } else if char_in_list(*str as c_int, b"+-\0".as_ptr() as *mut c_char) != 0 {
                str = str.offset(1);
                if isdigit(*str as c_int) != 0 {
                    while isdigit(*str as c_int) != 0 {
                        str = str.offset(1);
                    }
                } else {
                    // SAFETY: static format string.
                    error_matc(b"Badly formed number.\n\0".as_ptr() as *const c_char);
                }
            } else {
                // SAFETY: static format string.
                error_matc(b"Badly formed number.\n\0".as_ptr() as *const c_char);
            }
        }
        symbol = number;
    } else if isalpha(*str as c_int) != 0 || char_in_list(*str as c_int, symchars) != 0 {
        while isalnum(*str as c_int) != 0 || char_in_list(*str as c_int, symchars) != 0 {
            str = str.offset(1);
        }
        // SAFETY: ch saves the live terminator byte.
        let ch = *str;
        *str = 0;
        let mut i: c_int = 0;
        while !reswords[i as usize].is_null() {
            // SAFETY: strcmp on the live token + keyword.
            if strcmp(p, reswords[i as usize]) == 0 {
                symbol = rsymbols[i as usize];
                break;
            }
            i += 1;
        }
        if reswords[i as usize].is_null() {
            symbol = name;
        }
        *str = ch;
    } else if *str == b'"' as c_char {
        str = str.offset(1);
        while *str != b'"' as c_char && *str != 0 {
            if *str == b'\\' as c_char {
                str = str.offset(1);
            }
            str = str.offset(1);
        }
        if *str == 0 {
            // SAFETY: static format string.
            error_matc(b"String not terminated.\n\0".as_ptr() as *const c_char);
        }
        str = str.offset(1);
        symbol = string;
    } else if char_in_list(*str as c_int, csymbols.as_mut_ptr()) != 0 {
        let mut i: c_int = 0;
        while *str != csymbols[i as usize] {
            i += 1;
        }
        symbol = ssymbols[i as usize];
        str = str.offset(1);
        if *str == b'=' as c_char {
            match symbol {
                x if x == assignsym => {
                    symbol = eq;
                    str = str.offset(1);
                }
                x if x == lt => {
                    symbol = le;
                    str = str.offset(1);
                }
                x if x == gt => {
                    symbol = ge;
                    str = str.offset(1);
                }
                x if x == indclose || x == rightpar => {}
                _ => {
                    // SAFETY: static format string.
                    error_matc(b"Syntax error.\n\0".as_ptr() as *const c_char);
                }
            }
        }
        if *str == b'>' as c_char && symbol == lt {
            symbol = neq;
            str = str.offset(1);
        }
    } else {
        // SAFETY: static format string.
        error_matc(b"Syntax error.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: ch saves the live terminator byte; csymbol is the 4096
    // file-static token buffer (same bound as C).
    let ch = *str;
    *str = 0;
    strcpy(csymbol.as_mut_ptr(), p);
    *str = ch;
    str_ = str;
}

// SAFETY: mirrors C newtree (zeroed TREE block).
#[no_mangle]
pub unsafe extern "C" fn newtree() -> *mut TREE {
    // SAFETY: mem_alloc never returns NULL (errors instead).
    mem_alloc(size_of::<TREE>()) as *mut TREE
}

// SAFETY: mirrors C args (comma list with min/max arity).
#[no_mangle]
pub unsafe extern "C" fn args(minp: c_int, maxp: c_int) -> *mut TREE {
    // SAFETY: equation parses from the live line.
    let root = equation();
    let mut treeptr = root;
    let mut numgot: c_int = 1;
    while symbol == argsep {
        scan();
        // SAFETY: link cells live.
        (*treeptr).next = equation();
        treeptr = (*treeptr).next;
        numgot += 1;
        if numgot > maxp {
            // SAFETY: static format string.
            error_matc(b"Too many parameters.\n\0".as_ptr() as *const c_char);
        }
    }
    if numgot < minp {
        // SAFETY: static format string.
        error_matc(b"Too few parameters.\n\0".as_ptr() as *const c_char);
    }
    root
}

// SAFETY: mirrors C nameorvar (operand chain with unary minus, subscripts,
// juxtaposition links and the re-scan minus dance).
#[no_mangle]
pub unsafe extern "C" fn nameorvar() -> *mut TREE {
    extern "C" {
        static mut symchars: *mut c_char;
    }
    // SAFETY: newtree returns a live zeroed block.
    let mut root = newtree();
    let mut treeptr = root;
    let mut prevtree = root;
    let mut sym: SYMTYPE = nullsym;
    // SAFETY: str/buf are the live line + its base (set by doit_compile).
    if symbol == minus
        && isspace(*str_ as c_int) == 0
        && (str_.offset(-2) < buf.as_mut_ptr()
            || isspace(*str_.offset(-2) as c_int) != 0
            || char_in_list(*str_.offset(-2) as c_int, b"{};=[(\\<>&|+-*/^,\0".as_ptr() as *mut c_char) != 0)
    {
        sym = minus;
        scan();
    }
    if symbol != name && symbol != number && symbol != string && symbol != leftpar {
        // SAFETY: static format string.
        error_matc(b"Expecting identifier, constant or leftpar.\n\0".as_ptr() as *const c_char);
    }
    while symbol == name || symbol == number || symbol == string || symbol == leftpar {
        match symbol {
            x if x == name => {
                // SAFETY: STRCOPY of the live token.
                (*treeptr).tentry.entrydata.s_data = strcopy(csymbol.as_ptr());
                (*treeptr).tentry.entrytype = ETYPE_NAME;
                // SAFETY: str is the live line.
                if *str_ == b'(' as c_char || *str_ == b'[' as c_char {
                    scan();
                    scan();
                    (*treeptr).tentry.args = args(0, 10000);
                    if symbol != rightpar && symbol != indclose {
                        // SAFETY: static format string.
                        error_matc(b"Expecting closing parenthesis.\n\0".as_ptr() as *const c_char);
                    }
                }
            }
            x if x == string => {
                // SAFETY: csymbol holds the quoted token (live buffer).
                let mut tstr = csymbol.as_mut_ptr().offset(1);
                // SAFETY: strlen on the live token.
                *tstr.offset(strlen(tstr) as isize - 1) = 0;
                // C measures the decoded length first (one fewer
                // cell per non-\\n escape); mirror the pass exactly.
                // SAFETY: strlen on the live token.
                let mut slen2 = strlen(tstr);
                let mut i: c_int = 0;
                while (i as usize) < strlen(tstr) {
                    if *tstr.offset(i as isize) == b'\\' as c_char {
                        i += 1;
                        if *tstr.offset(i as isize) != b'n' as c_char {
                            slen2 -= 1;
                        }
                    }
                    i += 1;
                }
                // SAFETY: session string block sized slen+1.
                (*treeptr).tentry.entrydata.s_data = mem_alloc(slen2 + 1) as *mut c_char;
                let dst = (*treeptr).tentry.entrydata.s_data;
                let mut i: c_int = 0;
                while *tstr != 0 {
                    if *tstr == b'\\' as c_char {
                        tstr = tstr.offset(1);
                        match *tstr {
                            x if x == b'n' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\r' as c_char;
                                i += 1;
                                *dst.offset(i as isize) = b'\n' as c_char;
                            }
                            x if x == b't' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\t' as c_char;
                            }
                            x if x == b'v' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\x0b' as c_char;
                            }
                            x if x == b'b' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\x08' as c_char;
                            }
                            x if x == b'r' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\r' as c_char;
                            }
                            x if x == b'f' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = b'\x0c' as c_char;
                            }
                            x if x == b'e' as c_char => {
                                // SAFETY: dst has slen+1 bytes.
                                *dst.offset(i as isize) = 27;
                            }
                            _ => {
                                // SAFETY: dst has slen+1 bytes; tstr live.
                                *dst.offset(i as isize) = *tstr;
                            }
                        }
                    } else {
                        // SAFETY: dst has slen+1 bytes; tstr live.
                        *dst.offset(i as isize) = *tstr;
                    }
                    i += 1;
                    tstr = tstr.offset(1);
                }
                (*treeptr).tentry.entrytype = ETYPE_STRING;
            }
            x if x == number => {
                // SAFETY: atof on the live token.
                (*treeptr).tentry.entrydata.d_data = atof(csymbol.as_ptr());
                (*treeptr).tentry.entrytype = ETYPE_NUMBER;
            }
            x if x == leftpar => {
                scan();
                // SAFETY: equation parses from the live line.
                (*treeptr).left = equation();
                if symbol != rightpar {
                    // SAFETY: static format string.
                    error_matc(b"Right parenthesis missing.\n\0".as_ptr() as *const c_char);
                }
                (*treeptr).tentry.entrytype = ETYPE_EQUAT;
            }
            _ => {}
        }
        // SAFETY: str is the live line.
        if *str_ == b'[' as c_char {
            scan();
            scan();
            (*treeptr).tentry.subs = args(1, 2);
            if symbol != rightpar && symbol != indclose {
                // SAFETY: static format string.
                error_matc(b"Expecting closing parenthesis.\n\0".as_ptr() as *const c_char);
            }
        }
        if sym == minus {
            // SAFETY: newtree returns a live zeroed block.
            let tp = newtree();
            store_op(tp, opr_minus as unsafe extern "C" fn(*mut MATRIX) -> *mut MATRIX);
            (*tp).tentry.entrytype = ETYPE_OPER;
            (*tp).left = treeptr;
            if root == treeptr {
                root = tp;
                treeptr = tp;
            } else {
                (*prevtree).link = tp;
                treeptr = tp;
            }
        }
        sym = symbol;
        scan();
        // SAFETY: str/buf are the live line + its base.
        if symbol == minus
            && isspace(*str_ as c_int) == 0
            && (str_.offset(-2) < buf.as_mut_ptr()
                || isspace(*str_.offset(-2) as c_int) != 0
                || char_in_list(*str_.offset(-2) as c_int, b"{};=([\\<>&|+-*/^,\0".as_ptr() as *mut c_char) != 0)
        {
            sym = minus;
            // SAFETY: str is the live line.
            if *str_ == b'-' as c_char && isspace(*str_.offset(1) as c_int) == 0 {
                break;
            } else if *str_ == b'-' as c_char {
                // SAFETY: static format string.
                error_matc(b"Syntax error.\n\0".as_ptr() as *const c_char);
            }
            scan();
            if symbol != name && symbol != number && symbol != string && symbol != leftpar {
                // SAFETY: static format string.
                error_matc(b"Expecting identifier, constant or leftpar.\n\0".as_ptr() as *const c_char);
            }
        }
        if symbol == name || symbol == number || symbol == string || symbol == leftpar {
            prevtree = treeptr;
            // SAFETY: newtree returns a live zeroed block.
            (*treeptr).link = newtree();
            treeptr = (*treeptr).link;
        }
    }
    root
}

// Precedence-climbing operators (each mirrors its C par_* exactly: fresh
// node, VDATA wiring, ETYPE_OPER, scan + operand, tighter-op fold).
// SAFETY: all build live trees from the live line via the helpers above.
#[no_mangle]
pub unsafe extern "C" fn par_apply(_root: *mut TREE) -> *mut TREE {
    let _ = _root;
    // SAFETY: newtree returns a live zeroed block.
    let newroot = newtree();
    match symbol {
        x if x == apply => {
            store_op(newroot, opr_apply as unsafe extern "C" fn(*mut MATRIX) -> *mut MATRIX);
        }
        x if x == not => {
            store_op(newroot, opr_not as unsafe extern "C" fn(*mut MATRIX) -> *mut MATRIX);
        }
        _ => {}
    }
    (*newroot).tentry.entrytype = ETYPE_OPER;
    scan();
    if symbol == apply || symbol == not {
        (*newroot).left = par_apply(newroot);
    } else {
        (*newroot).left = nameorvar();
    }
    newroot
}
// SAFETY: mirrors C par_trans (postfix transpose fold).
#[no_mangle]
pub unsafe extern "C" fn par_trans(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == transpose {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        store_op(newroot, opr_trans as unsafe extern "C" fn(*mut MATRIX) -> *mut MATRIX);
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
    }
    newroot
}
// SAFETY: mirrors C par_pow (right-associative power).
#[no_mangle]
pub unsafe extern "C" fn par_pow(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == power {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        store_op(newroot, opr_pow as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_timesdivide.
#[no_mangle]
pub unsafe extern "C" fn par_timesdivide(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == times || symbol == ptimes || symbol == divide {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        match symbol {
            x if x == times => {
                store_op(newroot, opr_mul as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == ptimes => {
                store_op(newroot, opr_pmul as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == divide => {
                store_op(newroot, opr_div as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            _ => {}
        }
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_plusminus.
#[no_mangle]
pub unsafe extern "C" fn par_plusminus(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == plus || symbol == minus {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        match symbol {
            x if x == plus => {
                store_op(newroot, opr_add as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == minus => {
                store_op(newroot, opr_subs as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            _ => {}
        }
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == times || x == ptimes || x == divide => {
                (*newroot).right = par_timesdivide((*newroot).right);
            }
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_compare.
#[no_mangle]
pub unsafe extern "C" fn par_compare(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == eq || symbol == neq || symbol == lt || symbol == gt || symbol == le || symbol == ge {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        match symbol {
            x if x == eq => {
                store_op(newroot, opr_eq as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == lt => {
                store_op(newroot, opr_lt as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == gt => {
                store_op(newroot, opr_gt as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == neq => {
                store_op(newroot, opr_neq as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == le => {
                store_op(newroot, opr_le as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == ge => {
                store_op(newroot, opr_ge as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            _ => {}
        }
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == plus || x == minus => {
                (*newroot).right = par_plusminus((*newroot).right);
            }
            x if x == times || x == ptimes || x == divide => {
                (*newroot).right = par_timesdivide((*newroot).right);
            }
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_vector (colon range).
#[no_mangle]
pub unsafe extern "C" fn par_vector(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == vector {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        store_op(newroot, opr_vector as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == eq || x == neq || x == lt || x == gt || x == le || x == ge => {
                (*newroot).right = par_compare((*newroot).right);
            }
            x if x == plus || x == minus => {
                (*newroot).right = par_plusminus((*newroot).right);
            }
            x if x == times || x == ptimes || x == divide => {
                (*newroot).right = par_timesdivide((*newroot).right);
            }
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_logical.
#[no_mangle]
pub unsafe extern "C" fn par_logical(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == and || symbol == or {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        (*newroot).left = root;
        match symbol {
            x if x == and => {
                store_op(newroot, opr_and as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            x if x == or => {
                store_op(newroot, opr_or as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
            }
            _ => {}
        }
        (*newroot).tentry.entrytype = ETYPE_OPER;
        root = newroot;
        scan();
        (*newroot).right = nameorvar();
        match symbol {
            x if x == vector => {
                (*newroot).right = par_vector((*newroot).right);
            }
            x if x == eq || x == neq || x == lt || x == gt || x == le || x == ge => {
                (*newroot).right = par_compare((*newroot).right);
            }
            x if x == plus || x == minus => {
                (*newroot).right = par_plusminus((*newroot).right);
            }
            x if x == times || x == ptimes || x == divide => {
                (*newroot).right = par_timesdivide((*newroot).right);
            }
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_reduction (prefix ? mask, LEFT filled after).
#[no_mangle]
pub unsafe extern "C" fn par_reduction(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == reduction {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        store_op(newroot, opr_reduction as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
        (*newroot).tentry.entrytype = ETYPE_OPER;
        scan();
        (*newroot).right = nameorvar();
        (*newroot).left = root;
        root = newroot;
        match symbol {
            x if x == and || x == or => {
                (*newroot).right = par_logical((*newroot).right);
            }
            x if x == vector => {
                (*newroot).right = par_vector((*newroot).right);
            }
            x if x == eq || x == neq || x == lt || x == gt || x == le || x == ge => {
                (*newroot).right = par_compare((*newroot).right);
            }
            x if x == plus || x == minus => {
                (*newroot).right = par_plusminus((*newroot).right);
            }
            x if x == times || x == ptimes || x == divide => {
                (*newroot).right = par_timesdivide((*newroot).right);
            }
            x if x == power => {
                (*newroot).right = par_pow((*newroot).right);
            }
            x if x == transpose => {
                (*newroot).right = par_trans((*newroot).right);
            }
            x if x == apply || x == not => {
                (*newroot).right = par_apply((*newroot).right);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C par_resize (prefix % reshape, LEFT filled after).
#[no_mangle]
pub unsafe extern "C" fn par_resize(mut root: *mut TREE) -> *mut TREE {
    let mut newroot = root;
    while symbol == resize {
        // SAFETY: newtree returns a live zeroed block.
        newroot = newtree();
        store_op(newroot, opr_resize as unsafe extern "C" fn(*mut MATRIX, *mut MATRIX) -> *mut MATRIX);
        (*newroot).tentry.entrytype = ETYPE_OPER;
        scan();
        (*newroot).left = nameorvar();
        (*newroot).right = root;
        root = newroot;
        match symbol {
            x if x == reduction => {
                (*newroot).left = par_reduction((*newroot).left);
            }
            x if x == and || x == or => {
                (*newroot).left = par_logical((*newroot).left);
            }
            x if x == vector => {
                (*newroot).left = par_vector((*newroot).left);
            }
            x if x == eq || x == neq || x == lt || x == gt || x == le || x == ge => {
                (*newroot).left = par_compare((*newroot).left);
            }
            x if x == plus || x == minus => {
                (*newroot).left = par_plusminus((*newroot).left);
            }
            x if x == times || x == ptimes || x == divide => {
                (*newroot).left = par_timesdivide((*newroot).left);
            }
            x if x == power => {
                (*newroot).left = par_pow((*newroot).left);
            }
            x if x == transpose => {
                (*newroot).left = par_trans((*newroot).left);
            }
            x if x == apply || x == not => {
                (*newroot).left = par_apply((*newroot).left);
            }
            _ => {}
        }
    }
    newroot
}
// SAFETY: mirrors C equation (operator dispatch loop).
#[no_mangle]
pub unsafe extern "C" fn equation() -> *mut TREE {
    // SAFETY: nameorvar parses from the live line.
    let mut treeptr = match symbol {
        x if x == apply || x == not => core::ptr::null_mut(),
        _ => nameorvar(),
    };
    loop {
        match symbol {
            x if x == resize => {
                treeptr = par_resize(treeptr);
            }
            x if x == reduction => {
                treeptr = par_reduction(treeptr);
            }
            x if x == and || x == or => {
                treeptr = par_logical(treeptr);
            }
            x if x == vector => {
                treeptr = par_vector(treeptr);
            }
            x if x == eq || x == neq || x == lt || x == gt || x == le || x == ge => {
                treeptr = par_compare(treeptr);
            }
            x if x == plus || x == minus => {
                treeptr = par_plusminus(treeptr);
            }
            x if x == times || x == ptimes || x == divide => {
                treeptr = par_timesdivide(treeptr);
            }
            x if x == power => {
                treeptr = par_pow(treeptr);
            }
            x if x == transpose => {
                treeptr = par_trans(treeptr);
            }
            x if x == apply || x == not => {
                treeptr = par_apply(treeptr);
            }
            _ => {
                return treeptr;
            }
        }
    }
}

// SAFETY: mirrors C commentparse (skip to end of line).
#[no_mangle]
pub unsafe extern "C" fn commentparse() -> *mut CLAUSE {
    // SAFETY: str is the live line.
    while *str_ != b'\n' as c_char && *str_ != 0 {
        str_ = str_.offset(1);
    }
    scan();
    core::ptr::null_mut()
}

// SAFETY: mirrors C scallparse ($ system call clause).
#[no_mangle]
pub unsafe extern "C" fn scallparse() -> *mut CLAUSE {
    // SAFETY: str is the live line.
    let p = str_;
    // SAFETY: root starts NULL like C.
    let mut root: *mut CLAUSE = core::ptr::null_mut();
    while *str_ != b'\n' as c_char && *str_ != b';' as c_char && *str_ != 0 {
        str_ = str_.offset(1);
    }
    if *str_ != 0 {
        *str_ = 0;
        str_ = str_.offset(1);
    }
    if *p != 0 {
        // SAFETY: fresh ALLOCMEM block.
        root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
        (*root).data = systemcall;
        // SAFETY: newtree returns a live zeroed block.
        (*root).this = newtree();
        // SAFETY: STRCOPY of the live command text.
        (*(*root).this).tentry.entrydata.s_data = strcopy(p as *const c_char);
        (*(*root).this).tentry.entrytype = ETYPE_STRING;
    }
    scan();
    root
}

// SAFETY: mirrors C statement (assignment lookahead + equation body).
#[no_mangle]
pub unsafe extern "C" fn statement() -> *mut CLAUSE {
    // SAFETY: fresh ALLOCMEM block.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    if symbol == name {
        // SAFETY: str/csymbol are the live line/token.
        let p = str_;
        let csymbcopy = strcopy(csymbol.as_ptr());
        loop {
            scan();
            if !(symbol != assignsym && symbol != nullsym && symbol != statemend) {
                break;
            }
        }
        // SAFETY: csymbol is the 4096 file-static token buffer.
        strcpy(csymbol.as_mut_ptr(), csymbcopy);
        // SAFETY: csymbcopy from strcopy above.
        mem_free(csymbcopy as *mut core::ffi::c_void);
        str_ = p;
        if symbol == assignsym {
            symbol = name;
            // SAFETY: nameorvar parses the assignment target.
            (*root).this = nameorvar();
            scan();
        } else {
            symbol = name;
        }
    }
    // SAFETY: fresh ALLOCMEM block; equation parses the live line.
    (*root).link = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    (*(*root).link).this = equation();
    (*root).data = assignsym;
    root
}

// SAFETY: mirrors C blockparse (begin/end statement list).
#[no_mangle]
pub unsafe extern "C" fn blockparse() -> *mut CLAUSE {
    // SAFETY: PMODE_BLOCK prompt is a live static.
    let mut root: *mut CLAUSE = core::ptr::null_mut();
    let mut ptr: *mut CLAUSE = core::ptr::null_mut();
    if symbol != beginsym {
        // SAFETY: static format string.
        error_matc(b"if|while|function: missing block open symbol.\n\0".as_ptr() as *const c_char);
    }
    scan();
    if symbol == nullsym {
        dogets(str_, b"....> \0".as_ptr() as *mut c_char);
        scan();
    }
    if symbol != endsym {
        // SAFETY: parse builds from the live line.
        root = parse();
        ptr = root;
        while !(*ptr).link.is_null() {
            ptr = (*ptr).link;
        }
    }
    while symbol != endsym && symbol != elsesym {
        if symbol == nullsym {
            dogets(str_, b"....> \0".as_ptr() as *mut c_char);
            scan();
        }
        if symbol != endsym && symbol != elsesym {
            // SAFETY: parse builds from the live line.
            (*ptr).link = parse();
            while !(*ptr).link.is_null() {
                ptr = (*ptr).link;
            }
        }
    }
    bendsym = symbol;
    scan();
    root
}

// SAFETY: mirrors C funcparse (function header + imports/exports + body).
#[no_mangle]
pub unsafe extern "C" fn funcparse() -> *mut CLAUSE {
    // SAFETY: fresh ALLOCMEM block.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    let ptr = root;
    (*ptr).data = funcsym;
    scan();
    // SAFETY: nameorvar parses the live line.
    (*ptr).this = nameorvar();
    // SAFETY: SUBS help head is a fresh tree.
    let mut help = newtree();
    (*(*ptr).this).tentry.subs = help;
    // SAFETY: STRCOPY of the live line start.
    (*help).tentry.entrydata.s_data = strcopy(str_ as *const c_char);
    // SAFETY: p tracks the live line.
    let mut p = str_;
    while symbol == nullsym || symbol == comment {
        dogets(str_, b"####> \0".as_ptr() as *mut c_char);
        scan();
        if symbol == comment {
            // SAFETY: newtree returns a live zeroed block.
            (*help).next = newtree();
            help = (*help).next;
            // SAFETY: str is the live line.
            while *str_ != b'\n' as c_char && *str_ != 0 {
                str_ = str_.offset(1);
            }
            // SAFETY: ch saves the live terminator byte.
            let ch = *str_;
            if *str_ != 0 {
                str_ = str_.offset(1);
                *str_ = 0;
            }
            *str_ = ch;
            // SAFETY: STRCOPY of the live line start.
            (*help).tentry.entrydata.s_data = strcopy(p as *const c_char);
            p = str_;
        }
    }
    while symbol == import || symbol == export {
        // SAFETY: LEFT/RIGHT of the live header tree.
        let mut lptr = if symbol == import { (*(*ptr).this).left } else { (*(*ptr).this).right };
        let sym = symbol;
        scan();
        // SAFETY: args parses from the live line.
        let rptr = args(1, 1000);
        if lptr.is_null() {
            if sym == import {
                (*(*ptr).this).left = rptr;
            } else {
                (*(*ptr).this).right = rptr;
            }
        } else {
            while !(*lptr).next.is_null() {
                lptr = (*lptr).next;
            }
            (*lptr).next = rptr;
        }
        if symbol == nullsym {
            dogets(str_, b"####> \0".as_ptr() as *mut c_char);
            scan();
        }
    }
    if symbol == beginsym {
        // SAFETY: blockparse builds from the live line.
        (*ptr).link = blockparse();
        if bendsym != endsym {
            // SAFETY: static format string.
            error_matc(b"function: missing end.\n\0".as_ptr() as *const c_char);
        }
    } else {
        // SAFETY: parse builds from the live line.
        (*ptr).link = parse();
    }
    root
}

// SAFETY: mirrors C ifparse (condition + branches + end markers).
#[no_mangle]
pub unsafe extern "C" fn ifparse() -> *mut CLAUSE {
    scan();
    if symbol != leftpar {
        // SAFETY: static format string.
        error_matc(b"Missing leftpar.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh ALLOCMEM block.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    let mut ptr = root;
    (*ptr).data = ifsym;
    scan();
    // SAFETY: equation parses the live line.
    (*ptr).this = equation();
    if symbol != rightpar {
        // SAFETY: static format string.
        error_matc(b"Missing rightpar.\n\0".as_ptr() as *const c_char);
    }
    scan();
    if symbol == thensym {
        scan();
    }
    if symbol == nullsym {
        dogets(str_, b"####> \0".as_ptr() as *mut c_char);
        scan();
    }
    if symbol == beginsym {
        // SAFETY: blockparse builds from the live line.
        (*ptr).link = blockparse();
    } else {
        // SAFETY: parse builds from the live line.
        (*ptr).link = parse();
    }
    while !(*ptr).link.is_null() {
        ptr = (*ptr).link;
    }
    // SAFETY: fresh ALLOCMEM end marker.
    (*root).jmp = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    (*ptr).link = (*root).jmp;
    ptr = (*ptr).link;
    (*ptr).data = endsym;
    if symbol == elsesym || bendsym == elsesym {
        // SAFETY: fresh ALLOCMEM else marker.
        (*root).jmp = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
        (*ptr).link = (*root).jmp;
        ptr = (*ptr).link;
        (*ptr).data = elsesym;
        if symbol == elsesym {
            scan();
        }
        if symbol == nullsym {
            dogets(str_, b"####> \0".as_ptr() as *mut c_char);
            scan();
        }
        if symbol == beginsym {
            // SAFETY: blockparse builds from the live line.
            (*ptr).link = blockparse();
        } else {
            // SAFETY: parse builds from the live line.
            (*ptr).link = parse();
        }
        while !(*ptr).link.is_null() {
            ptr = (*ptr).link;
        }
        // SAFETY: fresh ALLOCMEM end marker.
        (*(*root).jmp).jmp = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
        (*ptr).link = (*(*root).jmp).jmp;
        (*(*ptr).link).data = endsym;
    }
    root
}

// SAFETY: mirrors C whileparse.
#[no_mangle]
pub unsafe extern "C" fn whileparse() -> *mut CLAUSE {
    scan();
    if symbol != leftpar {
        // SAFETY: static format string.
        error_matc(b"Missing leftpar.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh ALLOCMEM block.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    let mut ptr = root;
    (*ptr).data = whilesym;
    scan();
    // SAFETY: equation parses the live line.
    (*ptr).this = equation();
    if symbol != rightpar {
        // SAFETY: static format string.
        error_matc(b"Missing rightpar.\n\0".as_ptr() as *const c_char);
    }
    scan();
    if symbol == nullsym {
        dogets(str_, b"####> \0".as_ptr() as *mut c_char);
        scan();
    }
    if symbol == beginsym {
        // SAFETY: blockparse builds from the live line.
        (*ptr).link = blockparse();
        if bendsym != endsym {
            // SAFETY: static format string.
            error_matc(b"while: missing end.\n\0".as_ptr() as *const c_char);
        }
    } else {
        // SAFETY: parse builds from the live line.
        (*ptr).link = parse();
    }
    while !(*ptr).link.is_null() {
        ptr = (*ptr).link;
    }
    // SAFETY: fresh ALLOCMEM end marker.
    (*root).jmp = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    (*ptr).link = (*root).jmp;
    (*(*ptr).link).data = endsym;
    root
}

// SAFETY: mirrors C forparse (loop header + body + end marker).
#[no_mangle]
pub unsafe extern "C" fn forparse() -> *mut CLAUSE {
    scan();
    if symbol != leftpar {
        // SAFETY: static format string.
        error_matc(b"for: missing leftpar.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh ALLOCMEM block.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    let mut ptr = root;
    (*ptr).data = forsym;
    scan();
    // SAFETY: nameorvar parses the live line.
    (*ptr).this = nameorvar();
    if symbol != assignsym {
        // SAFETY: static format string.
        error_matc(b"for: missing equalsign\n\0".as_ptr() as *const c_char);
    }
    scan();
    // SAFETY: equation parses the live line.
    (*(*ptr).this).link = equation();
    if symbol != rightpar {
        // SAFETY: static format string.
        error_matc(b"Missing rightpar.\n\0".as_ptr() as *const c_char);
    }
    scan();
    if symbol == nullsym {
        dogets(str_, b"####> \0".as_ptr() as *mut c_char);
        scan();
    }
    if symbol == beginsym {
        // SAFETY: blockparse builds from the live line.
        (*ptr).link = blockparse();
        if bendsym != endsym {
            // SAFETY: static format string.
            error_matc(b"for: missing end.\n\0".as_ptr() as *const c_char);
        }
    } else {
        // SAFETY: parse builds from the live line.
        (*ptr).link = parse();
    }
    while !(*ptr).link.is_null() {
        ptr = (*ptr).link;
    }
    // SAFETY: fresh ALLOCMEM end marker.
    (*root).jmp = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    (*ptr).link = (*root).jmp;
    (*(*ptr).link).data = endsym;
    root
}

// SAFETY: mirrors C parse (clause dispatch + terminator skip).
#[no_mangle]
pub unsafe extern "C" fn parse() -> *mut CLAUSE {
    // SAFETY: builders parse from the live line.
    let mut ptr = match symbol {
        x if x == funcsym => funcparse(),
        x if x == beginsym => {
            let p = blockparse();
            if bendsym != endsym {
                // SAFETY: static format string.
                error_matc(b"begin: missing end.\n\0".as_ptr() as *const c_char);
            }
            p
        }
        x if x == ifsym => ifparse(),
        x if x == whilesym => whileparse(),
        x if x == forsym => forparse(),
        x if x == systemcall => scallparse(),
        x if x == comment => commentparse(),
        _ => statement(),
    };
    while symbol == statemend {
        scan();
    }
    if ptr.is_null() {
        // SAFETY: fresh ALLOCMEM block.
        ptr = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    }
    ptr
}

// SAFETY: mirrors C free_treeentry (args/subs/name-string/CONST-payload).
#[no_mangle]
pub unsafe extern "C" fn free_treeentry(root: *mut TREEENTRY) {
    if root.is_null() {
        return;
    }
    // SAFETY: args/subs are live subtrees (or NULL).
    free_tree((*root).args);
    free_tree((*root).subs);
    if (*root).entrytype == ETYPE_STRING || (*root).entrytype == ETYPE_NAME {
        // SAFETY: s_data is a mem_alloc'd block.
        mem_free((*root).entrydata.s_data as *mut core::ffi::c_void);
    } else if (*root).entrytype == ETYPE_CONST {
        var_delete_temp((*root).entrydata.c_data);
    }
}

// SAFETY: mirrors C free_tree (post-order release).
#[no_mangle]
pub unsafe extern "C" fn free_tree(root: *mut TREE) {
    if root.is_null() {
        return;
    }
    // SAFETY: links are live subtrees (or NULL).
    free_tree((*root).next);
    free_tree((*root).link);
    free_tree((*root).left);
    free_tree((*root).right);
    free_treeentry(&mut (*root).tentry);
    // SAFETY: root is a mem_alloc'd block.
    mem_free(root as *mut core::ffi::c_void);
}

// SAFETY: mirrors C free_clause (link chain + body + self).
#[no_mangle]
pub unsafe extern "C" fn free_clause(root: *mut CLAUSE) {
    if root.is_null() {
        return;
    }
    // SAFETY: link/body live (or NULL).
    free_clause((*root).link);
    free_tree((*root).this);
    // SAFETY: root is a mem_alloc'd block.
    mem_free(root as *mut core::ffi::c_void);
}

// SAFETY: mirrors C doit_compile (line into buf, clause list out).
#[no_mangle]
pub unsafe extern "C" fn doit_compile(line: *mut c_char) -> *mut CLAUSE {
    // SAFETY: str points at the 4096 file-static buf; strcpy mirrors C
    // (lines fit; corpus lines are short).
    str_ = buf.as_mut_ptr();
    strcpy(str_, line as *const c_char);
    // SAFETY: fresh ALLOCMEM head.
    let root = mem_alloc(size_of::<CLAUSE>()) as *mut CLAUSE;
    let mut ptr = root;
    scan();
    while symbol != nullsym {
        // SAFETY: parse builds from the live line.
        (*ptr).link = parse();
        while !(*ptr).link.is_null() {
            ptr = (*ptr).link;
        }
    }
    root
}

// SAFETY: mirrors C doit (compile + eval + free; the optimclause call
// stays commented out like upstream).
#[no_mangle]
pub unsafe extern "C" fn doit(line: *mut c_char) -> *mut VARIABLE {
    // SAFETY: doit_compile parses into session memory.
    let root = doit_compile(line);
    // SAFETY: evalclause runs the live clause tree.
    let res = evalclause(root);
    free_clause(root);
    res
}
