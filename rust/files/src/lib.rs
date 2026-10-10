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

// ---- unit body: transcribed from matc/src/files.c ----
// Transcribed from matc/src/files.c (file I/O commands).

// SAFETY: all cross-unit/libc imports uphold their C contracts.
extern "C" {
    static mut math_in: *mut FILE;
    static mut math_out: *mut FILE;
    static mut math_err: *mut FILE;
    static mut stdin: *mut FILE;
    static mut stdout: *mut FILE;
    static mut stderr: *mut FILE;
    fn feof(stream: *mut FILE) -> c_int;
    fn ferror(stream: *mut FILE) -> c_int;
    fn clearerr(stream: *mut FILE);
    fn error_matc(fmt: *const c_char, ...) -> !;
    fn var_to_string(v: *mut VARIABLE) -> *mut c_char;
    fn var_temp_new(typ: c_int, nrow: c_int, ncol: c_int) -> *mut VARIABLE;
    fn var_delete_temp(v: *mut VARIABLE);
    fn str_sprintf(v: *mut VARIABLE) -> *mut VARIABLE;
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

pub const FILE_BINARY: c_int = 0;
pub const FILE_ASCII: c_int = 1;
pub const MAXFILES: usize = 32;

static mut fil_fps: [*mut FILE; MAXFILES] = [core::ptr::null_mut(); MAXFILES];
static mut fil_fps_save: [*mut FILE; 3] = [core::ptr::null_mut(); 3];

// SAFETY: mirrors C fil_fread; var is (fp, len).
#[no_mangle]
pub unsafe extern "C" fn fil_fread(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fread: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fread: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fread: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: NEXT cell live.
    let len = *var_matr((*var).next) as c_int;
    if len <= 0 {
        // SAFETY: static format string.
        error_matc(b"fread: invalid length specified.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(TYPE_DOUBLE, 1, (len + 8 - 1) >> 3);
    // SAFETY: res data live; fread into len bytes.
    let iosize = fread(var_matr(res) as *mut core::ffi::c_void, 1, len as usize, fp);
    let _ = iosize;
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fread: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fread: error reading file.\n\0".as_ptr() as *const c_char);
    }
    res
}

// SAFETY: mirrors C fil_fwrite; var is (fp, buf[, len]).
#[no_mangle]
pub unsafe extern "C" fn fil_fwrite(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fwrite: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fwrite: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    // SAFETY: NEXT cells live.
    let next = (*var).next;
    let len = if !(*next).next.is_null() {
        let l = *var_matr((*next).next) as c_int;
        if l as usize > var_matsize(next) {
            // SAFETY: static format string.
            error_matc(b"fwrite: attempt to write more data than provided.\n\0".as_ptr() as *const c_char);
        }
        l as usize
    } else {
        var_matsize(next)
    };
    // SAFETY: NEXT data live for len bytes; fp live.
    fwrite(var_matr(next) as *const core::ffi::c_void, 1, len, fp);
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fwrite: error writing file.\n\0".as_ptr() as *const c_char);
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_fscanf; var is (fp, fmt).
#[no_mangle]
pub unsafe extern "C" fn fil_fscanf(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: NEXT arg string fresh mem_alloc'd.
    let next = (*var).next;
    let fmt = var_to_string(next);
    // SAFETY: argument cells live.
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fscanf: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fscanf: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fscanf: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    // Raw slot pointers (same aliasing rationale as str_sscanf).
    let mut str_p: [c_double; STR_MAXVALS] = [0.0; STR_MAXVALS];
    let sp = str_p.as_mut_ptr();
    // SAFETY: C fscanf with 30 double slots; identical call shape.
    let got = fscanf(
        fp,
        fmt as *const c_char,
        sp.offset(0), sp.offset(1), sp.offset(2), sp.offset(3), sp.offset(4),
        sp.offset(5), sp.offset(6), sp.offset(7), sp.offset(8), sp.offset(9),
        sp.offset(10), sp.offset(11), sp.offset(12), sp.offset(13), sp.offset(14),
        sp.offset(15), sp.offset(16), sp.offset(17), sp.offset(18), sp.offset(19),
        sp.offset(20), sp.offset(21), sp.offset(22), sp.offset(23), sp.offset(24),
        sp.offset(25), sp.offset(26), sp.offset(27), sp.offset(28), sp.offset(29),
    );
    let mut res: *mut VARIABLE = core::ptr::null_mut();
    if got > 0 {
        // SAFETY: fresh temp owns its storage.
        res = var_temp_new(TYPE_DOUBLE, 1, got);
        let mut i: c_int = 0;
        while i < got {
            // SAFETY: res data live through return.
            var_set_m(res, 0, i, str_p[i as usize]);
            i += 1;
        }
    }
    // SAFETY: fmt from var_to_string above.
    mem_free(fmt as *mut core::ffi::c_void);
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fscanf: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fscanf: error reading file.\n\0".as_ptr() as *const c_char);
    }
    res
}

// SAFETY: mirrors C fil_fgets; var is (fp).
#[no_mangle]
pub unsafe extern "C" fn fil_fgets(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fgets: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fgets: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fgets: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    let mut str_pstr = [0 as c_char; STR_MAXLEN];
    // SAFETY: fgets into the 512-byte stack buffer from the live fp.
    fgets(str_pstr.as_mut_ptr(), STR_MAXLEN as c_int, fp);
    if feof(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fgets: end of file detected.\n\0".as_ptr() as *const c_char);
    }
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fgets: error reading file.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: strlen on the NUL-terminated buffer just read.
    let len = strlen(str_pstr.as_ptr());
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(TYPE_STRING, 1, len as c_int - 1);
    let mut i: c_int = 0;
    // SAFETY: strlen re-read each iteration exactly like the C loop.
    while (i as usize) < strlen(str_pstr.as_ptr()) - 1 {
        // SAFETY: res data live through return.
        var_set_m(res, 0, i, str_pstr[i as usize] as c_double);
        i += 1;
    }
    res
}

// SAFETY: mirrors C fil_fprintf; var is (fp, fmt[, vec]).
#[no_mangle]
pub unsafe extern "C" fn fil_fprintf(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument cells live.
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fprintf: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fprintf: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    // SAFETY: NEXT chain live; str_sprintf returns a live temp.
    let svar = str_sprintf((*var).next);
    // SAFETY: var_to_string returns a fresh mem_alloc'd string.
    let s = var_to_string(svar);
    // SAFETY: fp + string live.
    fprintf(fp, b"%s\0".as_ptr() as *const c_char, s);
    var_delete_temp(svar);
    // SAFETY: s from var_to_string above.
    mem_free(s as *mut core::ffi::c_void);
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format string.
        error_matc(b"fprintf: error writing file.\n\0".as_ptr() as *const c_char);
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_fputs; var is (fp, str).
#[no_mangle]
pub unsafe extern "C" fn fil_fputs(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: NEXT arg string fresh; argument cells live.
    let next = (*var).next;
    let s = var_to_string(next);
    let ind = *var_matr(var) as c_int;
    if ind < 0 || ind >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fputs: Invalid file number.\n\0".as_ptr() as *const c_char);
    } else if fil_fps[ind as usize].is_null() {
        // SAFETY: static format string.
        error_matc(b"fputs: File not open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fil_fps[ind] checked non-NULL above.
    let fp = fil_fps[ind as usize];
    // SAFETY: fp + string live.
    fprintf(fp, b"%s\0".as_ptr() as *const c_char, s);
    // SAFETY: s from var_to_string above.
    mem_free(s as *mut core::ffi::c_void);
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        clearerr(fp);
        // SAFETY: static format (original says fprintf here too).
        error_matc(b"fprintf: error writing file.\n\0".as_ptr() as *const c_char);
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_fopen, including the 0/1/2 stdio-swap; var is
// (name, mode).
#[no_mangle]
pub unsafe extern "C" fn fil_fopen(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: NEXT cells live; both strings fresh mem_alloc'd.
    let next = (*var).next;
    let mode = var_to_string(next);
    let nm = var_to_string(var);
    let mut file: c_int = 0;
    while file < MAXFILES as c_int {
        if fil_fps[file as usize].is_null() {
            break;
        }
        file += 1;
    }
    if file >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fopen: maximum number of files already open.\n\0".as_ptr() as *const c_char);
    }
    // SAFETY: name/mode live NUL-terminated strings.
    let f = fopen(nm as *const c_char, mode as *const c_char);
    if f.is_null() {
        // SAFETY: static format; live name arg.
        error_matc(b"fopen: can't open file: %s.\n\0".as_ptr() as *const c_char, nm);
    }
    fil_fps[file as usize] = f;
    match file {
        0 => {
            fil_fps_save[0] = math_in;
            math_in = fil_fps[0];
        }
        1 => {
            fil_fps_save[1] = math_out;
            math_out = fil_fps[1];
        }
        2 => {
            fil_fps_save[2] = math_err;
            math_err = fil_fps[2];
        }
        _ => {}
    }
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(TYPE_DOUBLE, 1, 1);
    // SAFETY: res data live through return.
    var_set_m(res, 0, 0, file as c_double);
    // SAFETY: both strings from var_to_string above.
    mem_free(nm as *mut core::ffi::c_void);
    mem_free(mode as *mut core::ffi::c_void);
    res
}

// SAFETY: mirrors C fil_fclose, including the stdio-restore dance; var is
// (fp).
#[no_mangle]
pub unsafe extern "C" fn fil_fclose(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument data live.
    let file = *var_matr(var) as c_int;
    if file < 0 || file >= MAXFILES as c_int {
        // SAFETY: static format string.
        error_matc(b"fclose: Invalid file number.\n\0".as_ptr() as *const c_char);
    }
    match file {
        0 => {
            math_in = fil_fps_save[0];
            if fil_fps[0] != math_out && !fil_fps[0].is_null() {
                // SAFETY: fil_fps[0] is a live open stream here.
                fclose(fil_fps[0]);
            }
            fil_fps[0] = math_in;
        }
        1 => {
            math_out = fil_fps_save[1];
            if fil_fps[1] != math_out && !fil_fps[1].is_null() {
                // SAFETY: fil_fps[1] is a live open stream here.
                fclose(fil_fps[1]);
            }
            fil_fps[1] = math_out;
        }
        2 => {
            math_err = fil_fps_save[2];
            if fil_fps[2] != math_err && !fil_fps[2].is_null() {
                // SAFETY: fil_fps[2] is a live open stream here.
                fclose(fil_fps[2]);
            }
            fil_fps[2] = math_err;
        }
        _ => {
            if !fil_fps[file as usize].is_null() {
                // SAFETY: fil_fps[file] is a live open stream here.
                fclose(fil_fps[file as usize]);
            }
            fil_fps[file as usize] = core::ptr::null_mut();
        }
    }
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_freopen (close + reopen under the same slot).
#[no_mangle]
pub unsafe extern "C" fn fil_freopen(var: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: argument data live.
    let file = *var_matr(var) as c_int;
    fil_fclose(var);
    fil_fps[file as usize] = core::ptr::null_mut();
    // SAFETY: NEXT chain is (name, mode) for fil_fopen; temp consumed.
    let v = fil_fopen((*var).next);
    var_delete_temp(v);
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_save (ascii %e dump or binary fwrite).
#[no_mangle]
pub unsafe extern "C" fn fil_save(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: file arg string fresh mem_alloc'd.
    let file = var_to_string(ptr);
    // SAFETY: fopen for writing; NULL-checked like C.
    let fp = fopen(file as *const c_char, b"w\0".as_ptr() as *const c_char);
    if fp.is_null() {
        // SAFETY: static format; live file arg.
        error_matc(b"save: can't open file: %s.\n\0".as_ptr() as *const c_char, file);
    }
    // SAFETY: NEXT cells live.
    let tmp = (*ptr).next;
    let mut ascflg: c_int = FALSE;
    if !(*tmp).next.is_null() {
        ascflg = *var_matr((*tmp).next) as c_int;
    }
    if ascflg != 0 {
        // SAFETY: fp + dims live.
        fprintf(fp, b"%d %d %d %d\n\0".as_ptr() as *const c_char, FILE_ASCII, var_type(tmp), var_nrow(tmp), var_ncol(tmp));
        if ferror(fp) != 0 {
            // SAFETY: fp live.
            fclose(fp);
            // SAFETY: static format string.
            error_matc(b"save: error writing file.\n\0".as_ptr() as *const c_char);
        }
        let mut i: c_int = 0;
        while i < var_nrow(tmp) {
            let mut j: c_int = 0;
            while j < var_ncol(tmp) {
                // SAFETY: fp + cell live.
                fprintf(fp, b"%e\n\0".as_ptr() as *const c_char, var_m(tmp, i, j));
                if ferror(fp) != 0 {
                    // SAFETY: fp live.
                    fclose(fp);
                    // SAFETY: static format string.
                    error_matc(b"save: error writing file.\n\0".as_ptr() as *const c_char);
                }
                j += 1;
            }
            i += 1;
        }
    } else {
        // SAFETY: fp + dims live.
        fprintf(fp, b"%d %d %d %d\n\0".as_ptr() as *const c_char, FILE_BINARY, var_type(tmp), var_nrow(tmp), var_ncol(tmp));
        if ferror(fp) != 0 {
            // SAFETY: fp live.
            fclose(fp);
            // SAFETY: static format string.
            error_matc(b"save: error writing file.\n\0".as_ptr() as *const c_char);
        }
        // SAFETY: tmp data live for MATSIZE bytes; fp live.
        fwrite(var_matr(tmp) as *const core::ffi::c_void, 1, var_matsize(tmp), fp);
        if ferror(fp) != 0 {
            // SAFETY: fp live.
            fclose(fp);
            // SAFETY: static format string.
            error_matc(b"save: error writing file.\n\0".as_ptr() as *const c_char);
        }
    }
    // SAFETY: fp live; file from var_to_string above.
    fclose(fp);
    mem_free(file as *mut core::ffi::c_void);
    core::ptr::null_mut()
}

// SAFETY: mirrors C fil_load (header fscanf + ascii/binary body).
#[no_mangle]
pub unsafe extern "C" fn fil_load(ptr: *mut VARIABLE) -> *mut VARIABLE {
    // SAFETY: file arg string fresh mem_alloc'd.
    let file = var_to_string(ptr);
    // SAFETY: fopen for reading; NULL-checked like C.
    let fp = fopen(file as *const c_char, b"r\0".as_ptr() as *const c_char);
    if fp.is_null() {
        // SAFETY: static format; live file arg.
        error_matc(b"load: can't open file: %s.\n\0".as_ptr() as *const c_char, file);
    }
    let mut ftype: c_int = 0;
    let mut typ: c_int = 0;
    let mut nrow: c_int = 0;
    let mut ncol: c_int = 0;
    // SAFETY: fp + int cells live.
    let iostat = fscanf(
        fp,
        b"%d %d %d %d\0".as_ptr() as *const c_char,
        &mut ftype,
        &mut typ,
        &mut nrow,
        &mut ncol,
    );
    let _ = iostat;
    if ferror(fp) != 0 {
        // SAFETY: fp live.
        fclose(fp);
        // SAFETY: static string (original typo kept: file.n).
        error_matc(b"load: error reading file.n\0".as_ptr() as *const c_char);
    }
    // SAFETY: fresh temp owns its storage.
    let res = var_temp_new(typ, nrow, ncol);
    if ftype == FILE_ASCII {
        let mut i: c_int = 0;
        while i < nrow {
            let mut j: c_int = 0;
            while j < ncol {
                // SAFETY: fp + res cell live.
                let iostat = fscanf(
                    fp,
                    b"%lf\0".as_ptr() as *const c_char,
                    &mut *var_matr(res).offset((i * ncol + j) as isize),
                );
                let _ = iostat;
                if ferror(fp) != 0 {
                    // SAFETY: fp live.
                    fclose(fp);
                    // SAFETY: static format string.
                    error_matc(b"load: error reading file.\n\0".as_ptr() as *const c_char);
                }
                j += 1;
            }
            i += 1;
        }
    } else {
        // SAFETY: fp live.
        fgetc(fp);
        // SAFETY: res data live for MATSIZE bytes; fp live.
        let iosize = fread(var_matr(res) as *mut core::ffi::c_void, 1, var_matsize(res), fp);
        let _ = iosize;
        if ferror(fp) != 0 {
            // SAFETY: fp live.
            fclose(fp);
            // SAFETY: static format string.
            error_matc(b"load: error reading file.\n\0".as_ptr() as *const c_char);
        }
    }
    // SAFETY: fp live; file from var_to_string above.
    fclose(fp);
    mem_free(file as *mut core::ffi::c_void);
    res
}

// SAFETY: registers file commands with byte-identical help texts and wires
// slots 0/1/2 to the process stdio triple.
#[no_mangle]
pub unsafe extern "C" fn fil_com_init() {
    // SAFETY: com_init copies into session memory; statics live forever.
    com_init(b"fread\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fread), 2, 2, b"str = fread( fp,len )\n\nRead len character from file fp. File pointer fp should have been\nobtained from a call to fopen or freopen, or be the standard input\nfile stdin. Characters are returned as function value.\n\nSEE ALSO: fopen,freopen,fgets,fscanf,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fscanf\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fscanf), 2, 2, b"vec = fscanf( fp,format )\n\nRead file fp as given in format. Format is equal to C-language format\nFile pointer fp should have been obtained from a call to fopen or freopen,\nor be the standard input.\n\nSEE ALSO: fopen,freopen,fgets,fread,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fgets\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fgets), 1, 1, b"str = fgets( fp )\n\nRead next line from fp. File pointer fp should have been obtained from a call\nto fopen or freopen or be the standard input.\n\nSEE ALSO: fopen,freopen,fread,fscanf,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fwrite\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fwrite), 2, 3, b"n = fwrite( fp, buf,len )\n\nWrite len bytes form buf to file fp. File pointer fp should have been obtained\nfrom a call to fopen or freopen or be the standard output (stdout) or standard\nerror (stderr). Return value is number of characters actually written.\n\nSEE ALSO: fopen,freopen,fputs,fprintf,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fprintf\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fprintf), 2, 3, b"n = fprintf( fp, format[, vec] )\n\nWrite formatted string to file fp. File pointer fp should have been obtained\nfrom a call to fopen or freopen or be the standard output (stdout) or standard\nerror (stderr). The format is equal to C-language format.\n\nSEE ALSO: fopen,freopen,fputs,fwrite,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fputs\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fputs), 2, 2, b"fputs( fp, str )\n\nWrite line to file fp. File pointer fp should have been obtained from a call\nto fopen or freopen or be the standard input (stdin).\n\nSEE ALSO: fopen,freopen,fwrite,matcvt,cvtmat.\n\0".as_ptr() as *const c_char);
    com_init(b"fopen\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fopen), 2, 2, b"fp = fopen( name, mode )\n\nOpen file given name and access mode. The most usual modes are \"r\" for reading\nand \"w\" for writing. Return value fp is used in functions reading and writing\nthe file.\n\nSEE ALSO: freopen.\n\0".as_ptr() as *const c_char);
    com_init(b"freopen\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_freopen), 3, 3, b"fp = freopen( fp, name, mode )\n\nReopen file given previous file pointer, name and access mode. The most usual modes\nare \"r\" for reading and \"w\" for writing. Return value fp is used in functions  \nreading and writing the file.\n\nSEE ALSO: fopen.\n\0".as_ptr() as *const c_char);
    com_init(b"fclose\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_fclose), 1, 1, b"fclose( fp )\n\nClose file previously opened with fopen or freopen.\n\nSEE ALSO: fopen, freopen.\n\0".as_ptr() as *const c_char);
    com_init(b"save\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_save), 2, 3, b"save( name, matrix[, ascii_flag] )\n\nSave matrix in file with name given as first parameter. If ascii_flag is\ngiven and is not zero the file will be in ascii format, otherwise matrix\nis saved in double precision binary format. In either case the first line\nof the file contains four digits (in ascii):\n\nascii_flag 0 NROW(matrix) NCOL(matrix).\n\nSEE ALSO: load.\n\0".as_ptr() as *const c_char);
    com_init(b"load\0".as_ptr() as *const c_char, FALSE, FALSE, Some(fil_load), 1, 1, b"matrix = load( name )\n\nLoad matrix from a file given name and in format used by save-command.\n\nSEE ALSO: save.\n\0".as_ptr() as *const c_char);
    // SAFETY: stdio globals are live process streams.
    fil_fps[0] = stdin;
    fil_fps_save[0] = stdin;
    fil_fps[1] = stdout;
    fil_fps_save[1] = stdout;
    fil_fps[2] = stderr;
    fil_fps_save[2] = stderr;
}
