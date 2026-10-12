// Shared prelude for elmergrid unit ports (lane 2, T008).
//
// Transcribed from elmergrid/src/{egdef.h, egtypes.h} @ 9f6af2f85 plus the
// libc surface slice-1 units use. Every unit crate links this as an rlib
// (`use eg_prelude::*;`) and appends only its own body (workspace layout
// per the merge-path verdict: per-unit staticlib archives, unchanged splice).
//
// Config scope (audited per unit, see lane evidence): default options —
// USE_MATC=0 (DISABLE_MATC on the exe, EG_PLUGIN on the core lib),
// USE_METIS=1 on the ElmerGrid exe / 0 on elmergrid_core, HAVE_UNISTD_H
// undefined. Units touching config-varying branches carry per-config
// treatment documented at their crate root.
//
// Faithful transcription notes (same contract as the matc slice):
// - All heap memory via libc malloc/free (C ABI: callers free with C
//   free(); Rust allocators must never be used).
// - `static mut` mirrors C file-scope globals (single-threaded utility).
// - Every `unsafe` use carries a SAFETY comment (unsafe-budget audit).
// - Formatting goes through C printf/fprintf/sprintf (extern); Rust
//   `format!` must never render user-visible text (locale/rounding drift).

// SAFETY: prelude contains no unsafe code, only declarations.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
// C-ABI global tables accessed exactly like the C original
// (single-threaded utility, no reentrancy beyond C's own).
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]

// ---- C scalar aliases -------------------------------------------------
pub type c_char = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type c_float = f32;
pub type c_double = f64;
pub type size_t = usize;
pub type Real = c_double;
pub type Integer = c_int;

// ---- opaque C types ----------------------------------------------------
pub enum FILE {}
pub use core::ffi::c_void;

// ---- egdef.h constants -------------------------------------------------
pub const TRUE: c_int = 1;
pub const FALSE: c_int = 0;

// NOTE: stdio EOF marker (glibc -1); comparisons only, never stored width-sensitively.
pub const EOF: c_int = -1;

// NOTE: Elmer's 9-digit literal, NOT full-precision PI; replacing it with
// core::f64::consts::PI would change RAD_TO_DEG/DEG_TO_RAD bit patterns.
#[allow(clippy::approx_constant)]
pub const FM_PI: Real = 3.141592653;
pub const NEARZERO: Real = 1.0e-50;

pub const XAXIS: c_int = 0;
pub const YAXIS: c_int = 1;
pub const ZAXIS: c_int = 2;

pub const INDEFINITE: c_int = -1;
pub const RIGHT: c_int = 1;
pub const UP: c_int = 2;
pub const LEFT: c_int = 3;
pub const DOWN: c_int = 0;

pub const BOTLEFT: c_int = 0;
pub const TOPLEFT: c_int = 3;
pub const BOTRIGHT: c_int = 1;
pub const TOPRIGHT: c_int = 2;

pub const FIRST: c_int = 0;
pub const SECOND: c_int = 1;

pub const COORD_CART2: c_int = 0;
pub const COORD_AXIS: c_int = 1;
pub const COORD_POLAR: c_int = 2;
pub const COORD_CART3: c_int = 3;
pub const COORD_CART1: c_int = 4;
pub const COORD_CYL: c_int = 5;

pub const BNDR_NOTHING: c_int = 0;

pub const KNOTS_ALL: c_int = 1;
pub const KNOTS_DIRICHLET: c_int = 2;
pub const KNOTS_FREE: c_int = 3;

pub const NUMBER_XY: c_int = 1;
pub const NUMBER_YX: c_int = 2;
pub const NUMBER_1D: c_int = 3;

pub const MAT_SMALLER: c_int = -11;
pub const MAT_BIGGER: c_int = -9;
pub const MAT_ANYTHING: c_int = -10;
pub const MAT_BOT: c_int = -1;
pub const MAT_RIGHT: c_int = -2;
pub const MAT_TOP: c_int = -3;
pub const MAT_LEFT: c_int = -4;
pub const MAT_NOTHING: c_int = 0;
pub const MAT_FIRSTNUMBER: c_int = 2;
pub const MAT_MAXNUMBER: c_int = 50;

// ---- egtypes.h sizing constants ----------------------------------------
pub const DIM: c_int = 2;
pub const MAXDOFS: usize = 20;
pub const MAXCELLS: usize = 100;
pub const MAXBOUNDARIES: usize = 1000;
pub const MAXCASES: usize = 12;
pub const MAXFILESIZE: usize = 600;
pub const MAXLINESIZE: usize = 600;
pub const LONGLINESIZE: usize = 1201;
pub const MAXNAMESIZE: usize = 50;
pub const MAXPARAMS: usize = 30;
pub const MAXNODESD3: usize = 64;
pub const MAXNODESD2: usize = 27;
pub const MAXNODESD1: usize = 9;
pub const MAXMAPPINGS: usize = 20;
pub const MAXCONNECTIONS: usize = 500;
pub const MAXBCS: usize = 4000;
pub const MAXBODIES: usize = 1000;
pub const MAXPARTITIONS: usize = 512;
pub const MAXHALOMODES: usize = 10;
pub const MAXELEMENTTYPE: c_int = 827;
pub const MAXNOPOINTS: usize = 20;
pub const MAXSIDEBULK: usize = 10;

// ---- config.h version strings (generated per build tree) ----------------
// Values below mirror build/elmergrid/config.h at lane-build time
// (VERSION/BRANCH/REVISION are git-derived and stable for the pinned
// checkout; COMPILATIONDATE changes on re-configure). A stale date fails
// loudly as a corpus diff, never silently: the usage banner is compared
// byte-identically pristine-vs-spliced.
pub const ELMER_FEM_VERSION_STR: &[u8] = b"26.2\0";
pub const ELMER_FEM_BRANCH_STR: &[u8] = b"devel\0";
pub const ELMER_FEM_REVISION_STR: &[u8] = b"9f6af2f85\0";
pub const ELMER_FEM_COMPILATIONDATE_STR: &[u8] = b"2026-10-11\0";

// ---- egtypes.h structures (field order == C layout, repr(C)) ------------
// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct CRSType {
    pub rows: *mut c_int,
    pub cols: *mut c_int,
    pub rowsize: c_int,
    pub colsize: c_int,
    pub created: c_int,
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct GridType {
    pub dimension: c_int,
    pub triangles: c_int,
    pub layeredbc: c_int,
    pub partitions: c_int,
    pub coordsystem: c_int,
    pub layered: c_int,
    pub autoratio: c_int,
    pub minxelems: c_int,
    pub minyelems: c_int,
    pub minzelems: c_int,
    pub totxelems: c_int,
    pub totyelems: c_int,
    pub totzelems: c_int,
    pub elemorder: c_int,
    pub elemmidpoints: c_int,
    pub wantedelems: c_int,
    pub limitdxverify: c_int,
    pub wantedelems3d: c_int,
    pub wantednodes3d: c_int,
    pub firstmaterial: c_int,
    pub lastmaterial: c_int,
    pub nocells: c_int,
    pub xcells: c_int,
    pub ycells: c_int,
    pub zcells: c_int,
    pub layerbcoffset: c_int,
    pub noelements: c_int,
    pub noknots: c_int,
    pub nonodes: c_int,
    pub numbering: c_int,
    pub maxwidth: c_int,
    pub noboundaries: c_int,
    pub maxmaterial: c_int,
    pub xlinear: [c_int; MAXCELLS + 1],
    pub ylinear: [c_int; MAXCELLS + 1],
    pub zlinear: [c_int; MAXCELLS + 1],
    pub xelems: [c_int; MAXCELLS + 1],
    pub yelems: [c_int; MAXCELLS + 1],
    pub zelems: [c_int; MAXCELLS + 1],
    pub zfirstmaterial: [c_int; MAXCELLS + 1],
    pub zlastmaterial: [c_int; MAXCELLS + 1],
    pub zmaterial: [c_int; MAXCELLS + 1],
    pub boundint: [c_int; MAXBOUNDARIES],
    pub boundext: [c_int; MAXBOUNDARIES],
    pub boundsolid: [c_int; MAXBOUNDARIES],
    pub boundtype: [c_int; MAXBOUNDARIES],
    pub zmaterialmap: *mut *mut c_int,
    pub zmaterialmapexists: c_int,
    pub zhelicity: Real,
    pub zhelicityexists: c_int,
    pub structure: [[c_int; MAXCELLS + 2]; MAXCELLS + 2],
    pub numbered: [[c_int; MAXCELLS + 2]; MAXCELLS + 2],
    pub dx0: Real,
    pub dy0: Real,
    pub dz0: Real,
    pub limitdx: Real,
    pub triangleangle: Real,
    pub xyratio: Real,
    pub xzratio: Real,
    pub rotateradius1: Real,
    pub rotateradius2: Real,
    pub rotateimprove: Real,
    pub rotate: c_int,
    pub rotateblocks: c_int,
    pub rotatecurve: c_int,
    pub rotatecartesian: c_int,
    pub mappings: c_int,
    pub reduceordermatmin: c_int,
    pub reduceordermatmax: c_int,
    pub curverad: Real,
    pub curveangle: Real,
    pub curvezet: Real,
    pub polarradius: Real,
    pub x: [Real; MAXCELLS + 1],
    pub y: [Real; MAXCELLS + 1],
    pub z: [Real; MAXCELLS + 1],
    pub xexpand: [Real; MAXCELLS + 1],
    pub yexpand: [Real; MAXCELLS + 1],
    pub zexpand: [Real; MAXCELLS + 1],
    pub xratios: [Real; MAXCELLS + 1],
    pub yratios: [Real; MAXCELLS + 1],
    pub zratios: [Real; MAXCELLS + 1],
    pub dx: [Real; MAXCELLS + 1],
    pub dy: [Real; MAXCELLS + 1],
    pub dz: [Real; MAXCELLS + 1],
    pub xdens: [Real; MAXCELLS + 1],
    pub ydens: [Real; MAXCELLS + 1],
    pub zdens: [Real; MAXCELLS + 1],
    pub mappingtype: [c_int; MAXMAPPINGS],
    pub mappingline: [c_int; MAXMAPPINGS],
    pub mappingpoints: [c_int; MAXMAPPINGS],
    pub mappinglimits: [Real; 2 * MAXMAPPINGS],
    pub mappingparams: [*mut Real; MAXMAPPINGS],
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct CellType {
    pub nonodes: c_int,
    pub dimension: c_int,
    pub numbering: c_int,
    pub xelem: c_int,
    pub yelem: c_int,
    pub levelwidth: c_int,
    pub left1st: c_int,
    pub left2nd: c_int,
    pub leftlast: c_int,
    pub levelwidthcenter: c_int,
    pub leftcenter: c_int,
    pub left2center: c_int,
    pub elem1st: c_int,
    pub elemwidth: c_int,
    pub xlinear: c_int,
    pub ylinear: c_int,
    pub material: c_int,
    pub xind: c_int,
    pub yind: c_int,
    pub boundary: [c_int; 8],
    pub neighbour: [c_int; 8],
    pub xwidth: Real,
    pub ywidth: Real,
    pub xratio: Real,
    pub yratio: Real,
    pub dx1: Real,
    pub dy1: Real,
    pub xcorner: [Real; 4],
    pub ycorner: [Real; 4],
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct FemType {
    pub created: c_int,
    pub noknots: c_int,
    pub noelements: c_int,
    pub nodepermexist: c_int,
    pub nodeperm: *mut c_int,
    pub coordsystem: c_int,
    pub nocells: c_int,
    pub maxnodes: c_int,
    pub dim: c_int,
    pub numbering: c_int,
    pub variables: c_int,
    pub indexwidth: c_int,
    pub mapgeo: c_int,
    pub nodalgraph: [*mut c_int; MAXCONNECTIONS],
    pub nodalmaxconnections: c_int,
    pub nodalexists: c_int,
    pub dualexists: c_int,
    pub partitiontable: [*mut c_int; MAXCONNECTIONS],
    pub maxpartitiontable: c_int,
    pub partitiontableexists: c_int,
    pub nocorners: c_int,
    pub timesteps: c_int,
    pub periodicexist: c_int,
    pub periodic: *mut c_int,
    pub nodeconnectexist: c_int,
    pub nodeconnect: *mut c_int,
    pub elemconnectexist: c_int,
    pub elemconnect: *mut c_int,
    pub partitionexist: c_int,
    pub nopartitions: c_int,
    pub elempart: *mut c_int,
    pub nodepart: *mut c_int,
    pub corners: *mut c_int,
    pub elementtypes: *mut c_int,
    pub material: *mut c_int,
    pub topology: *mut *mut c_int,
    pub bodynamesexist: c_int,
    pub boundarynamesexist: c_int,
    pub edofs: [c_int; MAXDOFS],
    pub bandwidth: [c_int; MAXDOFS],
    pub alldofs: [c_int; MAXDOFS],
    pub minsize: Real,
    pub maxsize: Real,
    pub x: *mut Real,
    pub y: *mut Real,
    pub z: *mut Real,
    pub times: *mut Real,
    pub dofs: [*mut Real; MAXDOFS],
    pub dofname: [[c_char; MAXNAMESIZE]; MAXDOFS],
    pub bodyname: [*mut c_char; MAXBODIES],
    pub boundaryname: [*mut c_char; MAXBCS],
    pub noboundaries: c_int,
    pub dualgraph: CRSType,
    pub nodalgraph2: CRSType,
    pub invtopo: CRSType,
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct BoundaryType {
    pub created: c_int,
    pub nosides: c_int,
    pub maxsidenodes: c_int,
    pub coordsystem: c_int,
    pub echain: c_int,
    pub ediscont: c_int,
    pub chainsize: c_int,
    pub parent: *mut c_int,
    pub parent2: *mut c_int,
    pub material: *mut c_int,
    pub side: *mut c_int,
    pub side2: *mut c_int,
    pub chain: *mut c_int,
    pub types: *mut c_int,
    pub discont: *mut c_int,
    pub normal: *mut c_int,
    pub elementtypes: *mut c_int,
    pub topology: *mut *mut c_int,
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct PointType {
    pub nopoints: c_int,
    pub parent: [c_int; MAXNOPOINTS],
    pub corner: [c_int; MAXNOPOINTS],
    pub material: [c_int; MAXNOPOINTS],
    pub ptype: [c_int; MAXNOPOINTS],
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ModelType {
    pub iparameters: c_int,
    pub rparameters: c_int,
    pub iparameter: [c_int; MAXPARAMS],
    pub rparameter: [Real; MAXPARAMS],
    pub ikeyword: [[c_char; MAXNAMESIZE]; MAXPARAMS],
    pub rkeyword: [[c_char; MAXNAMESIZE]; MAXPARAMS],
}

// Whole-struct copies mirror C struct assignment (e.g. data[k] = data[j]).
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ElmergridType {
    pub dim: c_int,
    pub silent: c_int,
    pub binary: c_int,
    pub center: c_int,
    pub scale: c_int,
    pub order: c_int,
    pub merge: c_int,
    pub translate: c_int,
    pub rotate: c_int,
    pub clone: [c_int; 3],
    pub mirror: [c_int; 3],
    pub cloneinds: c_int,
    pub canter: c_int,
    pub decimals: c_int,
    pub layers: c_int,
    pub layerbounds: [c_int; MAXBOUNDARIES],
    pub layernumber: [c_int; MAXBOUNDARIES],
    pub layermove: c_int,
    pub metis: c_int,
    pub metis_contig: c_int,
    pub metis_minconn: c_int,
    pub metis_seed: c_int,
    pub metis_volcut: c_int,
    pub metis_ncuts: c_int,
    pub partopt: c_int,
    pub partoptim: c_int,
    pub partbcoptim: c_int,
    pub partitions: c_int,
    pub partdim: [c_int; 3],
    pub partjoin: c_int,
    pub inmethod: c_int,
    pub outmethod: c_int,
    pub sidemap: [c_int; 3 * MAXBOUNDARIES],
    pub sidemappings: c_int,
    pub bulkmap: [c_int; 3 * MAXMAPPINGS],
    pub bulkmappings: c_int,
    pub coordinatemap: [c_int; 3],
    pub boundorder: c_int,
    pub bulkorder: c_int,
    pub boundbounds: c_int,
    pub boundbound: [c_int; 3 * MAXBOUNDARIES],
    pub bulkbounds: c_int,
    pub bulkbound: [c_int; 3 * MAXBOUNDARIES],
    pub mirrorbc: c_int,
    pub layerparents: [c_int; MAXBOUNDARIES],
    pub sidebulk: [c_int; MAXSIDEBULK],
    pub triangles: c_int,
    pub polar: c_int,
    pub usenames: c_int,
    pub isoparam: c_int,
    pub cylinder: c_int,
    pub unitemeshes: c_int,
    pub reduce: c_int,
    pub multidim: c_int,
    pub removelowdim: c_int,
    pub removeunused: c_int,
    pub removeintbcs: c_int,
    pub increase: c_int,
    pub reducemat1: c_int,
    pub reducemat2: c_int,
    pub findsides: c_int,
    pub vtuone: c_int,
    pub saveboundaries: c_int,
    pub nodes3d: c_int,
    pub elements3d: c_int,
    pub periodic: c_int,
    pub periodicdim: [c_int; 3],
    pub discont: c_int,
    pub discontbounds: [c_int; MAXBOUNDARIES],
    pub connect: c_int,
    pub connectbounds: [c_int; MAXBOUNDARIES],
    pub connectboundsset: [c_int; MAXBOUNDARIES],
    pub connectboundsnosets: c_int,
    pub partorder: c_int,
    pub parthalo: [c_int; MAXHALOMODES],
    pub partitionindirect: c_int,
    pub partbw: c_int,
    pub parthypre: c_int,
    pub partdual: c_int,
    pub partbcz: c_int,
    pub partbcr: c_int,
    pub partbcmetis: c_int,
    pub partbclayers: c_int,
    pub nofilesin: c_int,
    pub saveinterval: [c_int; 3],
    pub elementsredone: c_int,
    pub bcoffset: c_int,
    pub rotatecurve: c_int,
    pub timeron: c_int,
    pub nosave: c_int,
    pub nooverwrite: c_int,
    pub unitenooverlap: c_int,
    pub filerenamed: c_int,
    pub cscale: [Real; 3],
    pub corder: [Real; 3],
    pub parttol: Real,
    pub cmerge: Real,
    pub ctranslate: [Real; 3],
    pub crotate: [Real; 3],
    pub clonesize: [Real; 3],
    pub layerratios: [Real; MAXBOUNDARIES],
    pub layerthickness: [Real; MAXBOUNDARIES],
    pub layereps: Real,
    pub triangleangle: Real,
    pub partcorder: [Real; 3],
    pub polarradius: Real,
    pub curverad: Real,
    pub curveangle: Real,
    pub curvezet: Real,
    pub relh: Real,
    pub filesin: [[c_char; MAXFILESIZE]; MAXCASES],
    pub filesout: [[c_char; MAXFILESIZE]; MAXCASES],
    pub infofile: [c_char; MAXFILESIZE],
}

// ---- egdef.h function-like macros as #[inline] fns -----------------------
// One-line change per the merge-path verdict: cross-crate inlining keeps
// per-unit archives free of prelude helper copies.
#[inline]
pub fn c_min_int(x: c_int, y: c_int) -> c_int {
    if x < y {
        x
    } else {
        y
    }
}

#[inline]
pub fn c_max_int(x: c_int, y: c_int) -> c_int {
    if x > y {
        x
    } else {
        y
    }
}

#[inline]
pub fn c_min_real(x: Real, y: Real) -> Real {
    if x < y {
        x
    } else {
        y
    }
}

#[inline]
pub fn c_max_real(x: Real, y: Real) -> Real {
    if x > y {
        x
    } else {
        y
    }
}

#[inline]
pub fn c_sgn(x: Real) -> Real {
    if x < 0.0 {
        -1.0
    } else if x > 0.0 {
        1.0
    } else {
        0.0
    }
}

#[inline]
pub fn c_abs_int(x: c_int) -> c_int {
    if x >= 0 {
        x
    } else {
        -x
    }
}

#[inline]
pub fn c_fabs(x: Real) -> Real {
    if x >= 0.0 {
        x
    } else {
        -x
    }
}

#[inline]
pub fn c_sqr(x: Real) -> Real {
    x * x
}

#[inline]
pub fn c_pos(x: Real) -> Real {
    if x > 0.0 {
        x
    } else {
        0.0
    }
}

#[inline]
pub fn c_neg(x: Real) -> Real {
    if x < 0.0 {
        x
    } else {
        0.0
    }
}

#[inline]
pub fn c_rad_to_deg(x: Real) -> Real {
    x * 180.0 / FM_PI
}

#[inline]
pub fn c_deg_to_rad(x: Real) -> Real {
    x * FM_PI / 180.0
}

#[inline]
pub fn c_size_of<T>() -> usize {
    core::mem::size_of::<T>()
}

// ---- libc surface --------------------------------------------------------
pub const CLOCKS_PER_SEC: c_long = 1_000_000;

extern "C" {
    pub fn malloc(size: size_t) -> *mut c_void;
    pub fn free(ptr: *mut c_void);
    pub fn calloc(nmemb: size_t, size: size_t) -> *mut c_void;
    pub fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void;
    pub fn exit(status: c_int) -> !;
    pub fn abort() -> !;
    pub fn abs(j: c_int) -> c_int;
    pub fn atoi(s: *const c_char) -> c_int;
    pub fn atof(s: *const c_char) -> c_double;
    pub fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
    pub fn strtod(s: *const c_char, end: *mut *mut c_char) -> c_double;
    pub fn clock() -> c_long;
    pub fn printf(fmt: *const c_char, ...) -> c_int;
    pub fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn sprintf(buf: *mut c_char, fmt: *const c_char, ...) -> c_int;
    pub fn sscanf(buf: *const c_char, fmt: *const c_char, ...) -> c_int;
    pub fn fscanf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    pub fn fputc(c: c_int, stream: *mut FILE) -> c_int;
    pub fn fgetc(stream: *mut FILE) -> c_int;
    pub fn fread(
        ptr: *mut c_void,
        size: size_t,
        nmemb: size_t,
        stream: *mut FILE,
    ) -> size_t;
    pub fn fwrite(
        ptr: *const c_void,
        size: size_t,
        nmemb: size_t,
        stream: *mut FILE,
    ) -> size_t;
    pub fn fflush(stream: *mut FILE) -> c_int;
    pub fn puts(s: *const c_char) -> c_int;
    pub fn fputs(s: *const c_char, stream: *mut FILE) -> c_int;
    pub fn fgets(s: *mut c_char, n: c_int, stream: *mut FILE) -> *mut c_char;
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    pub fn fclose(stream: *mut FILE) -> c_int;
    pub fn rewind(stream: *mut FILE);
    pub fn strlen(s: *const c_char) -> size_t;
    pub fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char;
    pub fn strcat(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int;
    pub fn strncmp(s1: *const c_char, s2: *const c_char, n: size_t) -> c_int;
    pub fn memcmp(s1: *const c_void, s2: *const c_void, n: size_t) -> c_int;
    pub fn strchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn strrchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char;
    pub fn strspn(s: *const c_char, accept: *const c_char) -> size_t;
    pub fn toupper(c: c_int) -> c_int;
    pub fn putchar(c: c_int) -> c_int;
    pub fn chdir(path: *const c_char) -> c_int;
    pub fn mkdir(path: *const c_char, mode: c_uint) -> c_int;
    pub fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
    pub fn memmove(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
    pub fn memset(s: *mut c_void, c: c_int, n: size_t) -> *mut c_void;
    pub fn fabs(x: c_double) -> c_double;
    pub fn pow(x: c_double, y: c_double) -> c_double;
    pub fn sqrt(x: c_double) -> c_double;
    pub fn ceil(x: c_double) -> c_double;
    pub fn sin(x: c_double) -> c_double;
    pub fn cos(x: c_double) -> c_double;
    pub fn acos(x: c_double) -> c_double;
    pub fn atan2(y: c_double, x: c_double) -> c_double;
    pub static mut stdin: *mut FILE;
    pub static mut stdout: *mut FILE;
    pub static mut stderr: *mut FILE;
}
