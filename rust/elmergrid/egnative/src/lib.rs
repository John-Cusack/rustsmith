// Faithful Rust transcription of elmergrid/src/egnative.c @ 9f6af2f85.
// Workspace layout: shared declarations live in eg-prelude; this crate
// appends only the unit body.
// Config scope: DUAL-target unit (ElmerGrid exe + elmergrid_core), but the
// USE_METIS branches diverge per config (live in the exe, compiled out
// under EG_PLUGIN). This port transcribes the exe (USE_METIS=1) shape and
// splices the ElmerGrid object only; the core object stays pristine C
// (ADR-031 default rule). USE_MATC=0 in both default configs (DISABLE_MATC
// exe, EG_PLUGIN core), so all MATC arms take the no-MATC branch.
// HAVE_UNISTD_H is undefined tree-wide (duplicate unistd.h include no-op).
// SAFETY: single-threaded utility; raw-pointer arithmetic mirrors C 1:1.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]

use eg_prelude::*;
// ---- sibling-unit imports (declared in headers, resolved at final link) --
extern "C" {
    fn AddExtension(fname1: *const c_char, fname2: *mut c_char, newext: *const c_char);
    fn bigerror(error_text: *const c_char);
    fn smallerror(error_text: *const c_char);
    fn dvector(nl: c_int, nh: c_int) -> *mut c_double;
    fn ivector(nl: c_int, nh: c_int) -> *mut c_int;
    fn cvector(nl: c_int, nh: c_int) -> *mut c_char;
    fn imatrix(nrl: c_int, nrh: c_int, ncl: c_int, nch: c_int) -> *mut *mut c_int;
    fn free_ivector(v: *mut c_int, nl: c_int, nh: c_int);
    fn next_int(start: *mut *mut c_char) -> c_int;
    fn next_real(start: *mut *mut c_char) -> Real;
    fn timer_activate(prefix: *const c_char);
    fn timer_show();
    fn timer_init();
    fn FileExists(filename: *mut c_char) -> c_int;
    fn StringToStrings(
        buf: *const c_char,
        args: *mut [c_char; 15],
        maxcnt: c_int,
        separator: c_char,
    ) -> c_int;
    fn StringToReal(buf: *const c_char, dest: *mut Real, maxcnt: c_int, separator: c_char)
        -> c_int;
    fn StringToInteger(
        buf: *const c_char,
        dest: *mut c_int,
        maxcnt: c_int,
        separator: c_char,
    ) -> c_int;
    fn AllocateBoundary(bound: *mut BoundaryType, size: c_int) -> c_int;
    fn AllocateKnots(data: *mut FemType);
    fn CreateBoundary(
        cell: *mut CellType,
        data: *mut FemType,
        bound: *mut BoundaryType,
        material1: c_int,
        material2: c_int,
        solidmat: c_int,
        boundarytype: c_int,
        info: c_int,
    ) -> c_int;
    fn CreateKnots(
        grid: *mut GridType,
        cell: *mut CellType,
        data: *mut FemType,
        noknots: c_int,
        info: c_int,
    );
    fn CreatePoints(
        cell: *mut CellType,
        data: *mut FemType,
        bound: *mut BoundaryType,
        param1: c_int,
        param2: c_int,
        pointmode: c_int,
        pointtype: c_int,
        info: c_int,
    ) -> c_int;
    fn GetBoundaryElement(
        sideind: c_int,
        bound: *mut BoundaryType,
        data: *mut FemType,
        ind: *mut c_int,
        sideelemtype: *mut c_int,
    );
    fn GetElementDimension(elementtype: c_int) -> c_int;
    fn GetElementSide(
        element: c_int,
        side: c_int,
        normal: c_int,
        data: *mut FemType,
        ind: *mut c_int,
        sideelemtype: *mut c_int,
    );
    fn InitializeKnots(data: *mut FemType);
    // NOTE: LoadElmergrid/SaveElmergrid/CreateExampleGrid/SetElementDivision/
    // CreateCells are defined in THIS unit (below), not extern here.
    fn CreateVariable(
        data: *mut FemType,
        variable: c_int,
        unknowns: c_int,
        value: Real,
        variablename: *const c_char,
        eorder: c_int,
    ) -> c_int;
    fn SaveMeshGmsh(
        data: *mut FemType,
        bound: *mut BoundaryType,
        nobound: c_int,
        prefix: *mut c_char,
        decimals: c_int,
        info: c_int,
    ) -> c_int;
    fn SaveMeshVtu(
        data: *mut FemType,
        bound: *mut BoundaryType,
        nobound: c_int,
        prefix: *mut c_char,
        dummyzero: c_int,
        info: c_int,
    ) -> c_int;
}

// NOTE: the C GETLINE macro assigns the file-scope `ioptr`; write-only here.
static mut ioptr: *mut c_char = core::ptr::null_mut();

// SAFETY: C file-scope globals (single-threaded utility); matcactive guards
// the compiled-out MATC path, iodebug gates debug prints.
#[no_mangle]
pub static mut matcactive: c_int = FALSE;
#[no_mangle]
pub static mut iodebug: c_int = FALSE;

// SAFETY: method-name tables (C `const char *[]`; the array itself is a
// mutable global, initialized once with literal pointers, never written).
#[no_mangle]
pub static mut InMethods: [*const c_char; 22] = [
    b"EG\0".as_ptr() as *const c_char,
    b"ELMERGRID\0".as_ptr() as *const c_char,
    b"ELMERSOLVER\0".as_ptr() as *const c_char,
    b"ELMERPOST\0".as_ptr() as *const c_char,
    b"ANSYS\0".as_ptr() as *const c_char,
    b"IDEAS\0".as_ptr() as *const c_char,
    b"ABAQUS\0".as_ptr() as *const c_char,
    b"FIDAP\0".as_ptr() as *const c_char,
    b"UNV\0".as_ptr() as *const c_char,
    b"COMSOL\0".as_ptr() as *const c_char,
    b"FIELDVIEW\0".as_ptr() as *const c_char,
    b"TRIANGLE\0".as_ptr() as *const c_char,
    b"MEDIT\0".as_ptr() as *const c_char,
    b"GID\0".as_ptr() as *const c_char,
    b"GMSH\0".as_ptr() as *const c_char,
    b"PARTITIONED\0".as_ptr() as *const c_char,
    b"FVCOM\0".as_ptr() as *const c_char,
    b"NASTRAN\0".as_ptr() as *const c_char,
    b"CGSIM\0".as_ptr() as *const c_char,
    b"GEO\0".as_ptr() as *const c_char,
    b"FLUX2D\0".as_ptr() as *const c_char,
    b"FLUX3D\0".as_ptr() as *const c_char,
];

#[no_mangle]
pub static mut OutMethods: [*const c_char; 6] = [
    b"EG\0".as_ptr() as *const c_char,
    b"ELMERGRID\0".as_ptr() as *const c_char,
    b"ELMERSOLVER\0".as_ptr() as *const c_char,
    b"ELMERPOST\0".as_ptr() as *const c_char,
    b"GMSH\0".as_ptr() as *const c_char,
    b"VTU\0".as_ptr() as *const c_char,
];

// SAFETY: usage text; the Instructions body is long literal output.
// (Transcribed in full below; elided here for the append.)
#[no_mangle]
pub unsafe extern "C" fn Instructions() {
    printf(
        b"This program can create simple 2D structured meshes consisting of\n\0".as_ptr()
            as *const c_char,
    );
    instructions_tail();
}

// SAFETY: remainder of the usage text (split for patch sizing only).
unsafe fn instructions_tail() {
    printf(
        b"linear, quadratic or cubic rectangles or triangles. The meshes may\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"also be extruded and revolved to create 3D forms. In addition many\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"mesh formats may be imported into Elmer software. Some options have\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"not been properly tested. Contact the author if you face problems.\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"------------------------------------------------------------------\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"The program has two operation modes\n\0".as_ptr() as *const c_char);
    printf(
        b"A) Command file mode which has the command file as the only argument\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"   'ElmerGrid commandfile.eg'\n\n\0".as_ptr() as *const c_char);
    printf(
        b"------------------------------------------------------------------\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"B) Inline mode which expects at least three input parameters\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"   'ElmerGrid 1 3 test'\n\n\0".as_ptr() as *const c_char);
    printf(b"The first parameter defines the input file format:\n\0".as_ptr() as *const c_char);
    instructions_formats();
}

// SAFETY: format list (literal output); the `#if 0` Nastran/CGsim/Geo/Flux
// lines are runtime-dead exactly as in C.
unsafe fn instructions_formats() {
    printf(b"1)  .grd      : ElmerGrid file format\n\0".as_ptr() as *const c_char);
    printf(b"2)  .mesh.*   : Elmer input format\n\0".as_ptr() as *const c_char);
    printf(b"3)  .ep       : Elmer output format\n\0".as_ptr() as *const c_char);
    printf(b"4)  .ansys    : Ansys input format\n\0".as_ptr() as *const c_char);
    printf(b"5)  .inp      : Abaqus input format by Ideas\n\0".as_ptr() as *const c_char);
    printf(b"6)  .fil      : Abaqus output format\n\0".as_ptr() as *const c_char);
    printf(b"7)  .FDNEUT   : Gambit (Fidap) neutral file\n\0".as_ptr() as *const c_char);
    printf(b"8)  .unv      : Universal mesh file format\n\0".as_ptr() as *const c_char);
    printf(b"9)  .mphtxt   : Comsol Multiphysics mesh format\n\0".as_ptr() as *const c_char);
    printf(b"10) .dat      : Fieldview format\n\0".as_ptr() as *const c_char);
    printf(b"11) .node,.ele: Triangle 2D mesh format\n\0".as_ptr() as *const c_char);
    printf(b"12) .mesh     : Medit mesh format\n\0".as_ptr() as *const c_char);
    printf(b"13) .msh      : GID mesh format\n\0".as_ptr() as *const c_char);
    printf(b"14) .msh      : Gmsh mesh format\n\0".as_ptr() as *const c_char);
    printf(b"15) .ep.i     : Partitioned ElmerPost format\n\0".as_ptr() as *const c_char);
    printf(b"16) .2dm      : 2D triangular FVCOM format\n\0".as_ptr() as *const c_char);
    // NOTE: `#if 0` entries 17-21 omitted (unreachable in C as well).
    printf(b"\nThe second parameter defines the output file format:\n\0".as_ptr() as *const c_char);
    printf(b"1)  .grd      : ElmerGrid file format\n\0".as_ptr() as *const c_char);
    printf(
        b"2)  .mesh.*   : ElmerSolver format (also partitioned .part format)\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"3)  .ep       : ElmerPost format\n\0".as_ptr() as *const c_char);
    printf(b"4)  .msh      : Gmsh mesh format\n\0".as_ptr() as *const c_char);
    printf(b"5)  .vtu      : VTK ascii XML format\n\0".as_ptr() as *const c_char);
    // NOTE: `#if 0` Abaqus/Fidap/Fastcap entries omitted.
    printf(
        b"------------------------------------------------------------------\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"\nThe third parameter is the name of the input file.\n\0".as_ptr() as *const c_char);
    printf(
        b"If the file does not exist, an example with the same name is created.\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"The default output file name is the same with a different suffix.\n\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"There are several additional in-line parameters that are\n\0".as_ptr() as *const c_char,
    );
    printf(
        b"taken into account only when applicable to the given format.\n\0".as_ptr()
            as *const c_char,
    );
    instructions_options();
}

// SAFETY: inline-parameter reference (literal output).
unsafe fn instructions_options() {
    printf(b"-out str             : name of the output file\n\0".as_ptr() as *const c_char);
    printf(b"-in str              : name of a secondary input file\n\0".as_ptr() as *const c_char);
    printf(
        b"-decimals            : number of decimals in the saved mesh (eg. 8)\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-bin                 : save mesh for ElmerSolver in binary format\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-sbin                : save mesh for ElmerSolver in binary & single precision format\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-relh real           : give relative mesh density parameter for ElmerGrid meshing\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-triangles           : rectangles will be divided to triangles\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-merge real          : merges nodes that are close to each other\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-order real[3]       : reorder elements and nodes using c1*x+c2*y+c3*z\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-centralize          : set the center of the mesh to origin\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-scale real[3]       : scale the coordinates with vector real[3]\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-translate real[3]   : translate the nodes with vector real[3]\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-rotate real[3]      : rotate around the main axis with angles real[3]\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-clone int[3]        : make identical copies of the mesh\n\0".as_ptr() as *const c_char,
    );
    printf(
        b"-clonesize real[3]   : the size of the mesh to be cloned if larger to the original\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-mirror int[3]       : copy the mesh around the origin in coordinate directions\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-cloneinds           : when performing cloning should cloned entities be given new indexes\n\0"
            .as_ptr() as *const c_char,
    );
    printf(b"-unite               : the meshes will be united\n\0".as_ptr() as *const c_char);
    printf(
        b"-unitenooverlap      : the meshes will be united without overlap in entity numbering\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-polar real          : map 2D mesh to a cylindrical shell with given radius\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-cylinder            : map 2D/3D cylindrical mesh to a cartesian mesh\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-reduce int[2]       : reduce element order at material interval [int1 int2]\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-increase            : increase element order from linear to quadratic\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-bcoffset int        : add an offset to the boundary conditions\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-discont int         : make the boundary to have secondary nodes\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-connect int         : make the boundary to have internal connection among its elements\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-removeintbcs        : remove internal boundaries if they are not needed\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-removelowdim        : remove boundaries that are two ranks lower than highest dim\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-removeunused        : remove nodes that are not used in any element\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-bulkorder           : renumber materials types from 1 so that every number is used\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-boundorder          : renumber boundary types from 1 so that every number is used\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-autoclean           : this performs the united action of the four above\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-multidim            : keep lower order entities even if they are not boundaries\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-bulkbound int[3]    : set the intersection of materials [int1 int2] to be boundary int3\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-boundbound int[3]   : set the intersection of boundaries [int1 int2] to be boundary int3\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-bulktype int[3]     : set material types in interval [int1 int2] to type int3\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-boundtype int[3]    : set sidetypes in interval [int1 int2] to type int3\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-layer int[2] real[2]: make a boundary layer for given boundary\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-layermove int       : apply Jacobi filter int times to move the layered mesh\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-divlayer int[2] real[2]: make a boundary layer for given boundary\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-3d / -2d / -1d      : mesh is 3, 2 or 1-dimensional (applies to examples)\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-isoparam            : ensure that higher order elements are convex\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-nonames             : disable use of mesh.names even if it would be supported by the format\n\0"
            .as_ptr() as *const c_char,
    );
    printf(b"-nosave              : disable saving part altogether\n\0".as_ptr() as *const c_char);
    printf(
        b"-nooverwrite         : if mesh already exists don't overwrite it\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-vtuone              : start real node indexes in vtu file from one\n\0".as_ptr()
            as *const c_char,
    );
    printf(b"-timer               : show timer information\n\0".as_ptr() as *const c_char);
    printf(
        b"-infofile str        : file for saving the timer and size information\n\0".as_ptr()
            as *const c_char,
    );
    instructions_keywords();
}

// SAFETY: partitioning keywords (literal output); USE_METIS=1 arms live.
unsafe fn instructions_keywords() {
    printf(
        b"\nKeywords are related to mesh partitioning for parallel ElmerSolver runs:\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partition int[3]    : the mesh will be partitioned in cartesian main directions\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-partorder real[3]   : in the 'partition' method set the direction of the ordering\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-parttol real        : in the 'partition' method set the tolerance for ordering\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-partcell int[3]     : the mesh will be partitioned in cells of fixed sizes\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partcyl int[3]      : the mesh will be partitioned in cylindrical main directions\n\0"
            .as_ptr() as *const c_char,
    );
    // NOTE: `#if USE_METIS` — live here (pairs build the METIS binary).
    printf(
        b"-metiskway int       : mesh will be partitioned with Metis using graph Kway routine\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-metisrec int        : mesh will be partitioned with Metis using graph Recursive routine\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-metiscontig         : enforce that the metis partitions are contiguous\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-metisvol            : minimize total communication volume in Metis\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-metisminconn        : minimize the maximum connectivity count in Metis\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-metisseed int       : random number generator seed for Metis algorithms\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-metisncuts int      : number of competing partitions to generate\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partdual            : use the dual graph in partition method (when available)\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-halo                : create halo for the partitioning for DG\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-halobc              : create halo for the partitioning at boundaries only\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-haloz / -halor      : create halo for the the special z- or r-partitioning\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-halogreedy          : create halo being greedy over the partition interfaces\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-periodic int[3]     : periodic coordinate directions for parallel & conforming meshes\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-partoptim           : apply aggressive optimization to node sharing\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partnobcoptim       : do not apply optimization to bc ownership sharing\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partbw              : minimize the bandwidth of partition-partition couplings\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-parthypre           : number the nodes continuously partitionwise\n\0".as_ptr()
            as *const c_char,
    );
    printf(
        b"-partzbc             : partition connected BCs separately to partitions in Z-direction\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-partrbc             : partition connected BCs separately to partitions in R-direction\n\0"
            .as_ptr() as *const c_char,
    );
    // NOTE: `#if USE_METIS` — live.
    printf(
        b"-metisbc             : partition connected BCs separately to partitions by Metis\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"-partlayers int      : extend boundary partitioning by element layers\n\0".as_ptr()
            as *const c_char,
    );
    // NOTE: `#if 0` ElmerPost block and `if(0)` names line omitted.
    printf(
        b"------------------------------------------------------------------\n\0".as_ptr()
            as *const c_char,
    );
}

// SAFETY: diverges via exit(0) on every path (see fempre note).
#[no_mangle]
pub unsafe extern "C" fn Goodbye() -> ! {
    printf(b"\nThank you for using ElmerGrid!\n\0".as_ptr() as *const c_char);
    printf(b"Repository: https://github.com/ElmerCSC/elmerfem\n\0".as_ptr() as *const c_char);
    printf(b"Contact: elmeradm@csc.fi\n\0".as_ptr() as *const c_char);
    exit(0);
}

// SAFETY: line reader with #-comment skipping; `goto newline` is continue.
// USE_MATC=0: the no-MATC #else arm is live, the substitution block compiled out.
unsafe fn Getline(line1: *mut c_char, io: *mut FILE) -> c_int {
    let mut i: c_int;
    let mut line0: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: charend is read (NULL check); matcpntr pair lives only in the
    // compiled-out MATC block and is dropped.
    let mut charend: *mut c_char;
    let mut isend: c_int;
    i = 0;
    while i < MAXLINESIZE as c_int {
        line0[i as usize] = 0x00;
        i += 1;
    }
    loop {
        charend = fgets(line0.as_mut_ptr(), MAXLINESIZE as c_int, io);
        isend = (charend.is_null()) as c_int;
        if isend != 0 {
            return 1;
        }
        if line0[0] == '#' as c_char || line0[0] == '%' as c_char || line0[0] == '!' as c_char {
            continue;
        }
        if matcactive == 0 && line0[0] == '*' as c_char {
            continue;
        }
        if matcactive == 0 && !strchr(line0.as_ptr(), '$' as c_int).is_null() {
            // NOTE: USE_MATC=0 arm (the #else branch in C).
            printf(
                b"\n a $ found and MATC has not been compiled into ElmerGrid,\n\0".as_ptr()
                    as *const c_char,
            );
            printf(
                b"either remove the $ or compile ElmerGrid with MATC\n\0".as_ptr() as *const c_char,
            );
            bigerror(
                b"Error: $ found in command and MATC is not active\0".as_ptr() as *const c_char,
            );
        }
        // NOTE: USE_MATC substitution block compiled out exactly as in C.
        if !strstr(
            line0.as_ptr(),
            b"subcell boundaries\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            continue;
        }
        if !strstr(
            line0.as_ptr(),
            b"material structure\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            continue;
        }
        if !strstr(line0.as_ptr(), b"mode\0".as_ptr() as *const c_char).is_null() {
            continue;
        }
        if !strstr(line0.as_ptr(), b"type\0".as_ptr() as *const c_char).is_null() {
            continue;
        }
        i = 0;
        while i < MAXLINESIZE as c_int {
            *line1.wrapping_offset(i as isize) = toupper(line0[i as usize] as c_int) as c_char;
            i += 1;
        }
        if iodebug != 0 {
            printf(b"line: %s\n\0".as_ptr() as *const c_char, line1);
        }
        return 0;
    }
}
// SAFETY: grid-default initializer over a valid GridType.
#[no_mangle]
pub unsafe extern "C" fn InitGrid(grid: *mut GridType) {
    // NOTE: C function-local define; hoisted as a Rust const.
    const MAXBODYID: c_int = 1000;
    let mut i: c_int;
    let mut j: c_int;
    (*grid).layered = FALSE;
    (*grid).layeredbc = TRUE;
    (*grid).layerbcoffset = 0;
    (*grid).triangles = FALSE;
    (*grid).triangleangle = 0.0;
    (*grid).partitions = FALSE;
    (*grid).wantedelems = 0;
    (*grid).wantedelems3d = 0;
    (*grid).wantednodes3d = 0;
    (*grid).limitdx = 0.0;
    (*grid).limitdxverify = FALSE;
    (*grid).dimension = 2;
    (*grid).xcells = 0;
    (*grid).ycells = 0;
    (*grid).zcells = 0;
    (*grid).nocells = 0;
    (*grid).noknots = 0;
    (*grid).noelements = 0;
    (*grid).totxelems = 0;
    (*grid).totyelems = 0;
    (*grid).totzelems = 0;
    (*grid).minxelems = 1;
    (*grid).minyelems = 1;
    (*grid).minzelems = 2;
    (*grid).firstmaterial = 1;
    (*grid).lastmaterial = MAXBODYID;
    (*grid).autoratio = 1;
    (*grid).xyratio = 1.0;
    (*grid).xzratio = 1.0;
    (*grid).nonodes = 4;
    (*grid).numbering = NUMBER_XY;
    (*grid).rotate = FALSE;
    (*grid).rotateradius1 = 0.0;
    (*grid).rotateradius2 = 0.0;
    (*grid).rotateimprove = 1.0;
    (*grid).rotateblocks = 4;
    (*grid).rotatecartesian = FALSE;
    (*grid).reduceordermatmin = 0;
    (*grid).reduceordermatmax = 0;
    (*grid).polarradius = 1.0;
    (*grid).elemorder = 1;
    (*grid).elemmidpoints = FALSE;
    (*grid).rotatecurve = FALSE;
    (*grid).curverad = 0.5;
    (*grid).curveangle = 90.0;
    (*grid).curvezet = 1.0;
    i = 0;
    while i <= MAXCELLS as c_int {
        (*grid).xelems[i as usize] = 0;
        (*grid).xlinear[i as usize] = TRUE;
        (*grid).xratios[i as usize] = 1.0;
        (*grid).xexpand[i as usize] = 1.0;
        (*grid).xdens[i as usize] = 1.0;
        (*grid).x[i as usize] = 0.0;
        i += 1;
    }
    i = 0;
    while i <= MAXCELLS as c_int {
        (*grid).yelems[i as usize] = 0;
        (*grid).ylinear[i as usize] = TRUE;
        (*grid).yratios[i as usize] = 1.0;
        (*grid).yexpand[i as usize] = 1.0;
        (*grid).ydens[i as usize] = 1.0;
        (*grid).y[i as usize] = 0.0;
        i += 1;
    }
    i = 0;
    while i <= MAXCELLS as c_int {
        (*grid).zelems[i as usize] = 0;
        (*grid).zlinear[i as usize] = TRUE;
        (*grid).zratios[i as usize] = 1.0;
        (*grid).zexpand[i as usize] = 1.0;
        (*grid).zdens[i as usize] = 1.0;
        (*grid).z[i as usize] = 0.0;
        (*grid).zfirstmaterial[i as usize] = 1;
        (*grid).zlastmaterial[i as usize] = MAXBODYID;
        (*grid).zmaterial[i as usize] = 0;
        i += 1;
    }
    (*grid).zmaterialmapexists = FALSE;
    (*grid).zhelicityexists = FALSE;
    (*grid).zhelicity = 0.0;
    // NOTE: initializes the numbering of the cells.
    j = 0;
    while j <= MAXCELLS as c_int + 1 {
        i = 0;
        while i <= MAXCELLS as c_int + 1 {
            (*grid).structure[j as usize][i as usize] = MAT_NOTHING;
            i += 1;
        }
        j += 1;
    }
    j = 0;
    while j <= MAXCELLS as c_int + 1 {
        i = 0;
        while i <= MAXCELLS as c_int + 1 {
            (*grid).numbered[j as usize][i as usize] = 0;
            i += 1;
        }
        j += 1;
    }
    (*grid).noboundaries = 0;
    i = 0;
    while i < MAXBOUNDARIES as c_int {
        (*grid).boundint[i as usize] = 0;
        (*grid).boundext[i as usize] = 0;
        (*grid).boundsolid[i as usize] = 0;
        (*grid).boundtype[i as usize] = 0;
        i += 1;
    }
    (*grid).mappings = 0;
    i = 0;
    while i < MAXMAPPINGS as c_int {
        (*grid).mappingtype[i as usize] = 0;
        (*grid).mappingline[i as usize] = 0;
        (*grid).mappinglimits[(2 * i) as usize] = 0.0;
        (*grid).mappinglimits[(2 * i + 1) as usize] = 0.0;
        (*grid).mappingpoints[i as usize] = 0;
        i += 1;
    }
}
// SAFETY: allocating 1D example grid (C `malloc` shape preserved).
unsafe fn ExampleGrid1D(grids: *mut *mut GridType, nogrids: *mut c_int, info: c_int) {
    let mut j: c_int;
    let grid: *mut GridType;
    *nogrids = 1;
    *grids = malloc((*nogrids as usize) * size_of::<GridType>()) as *mut GridType;
    grid = *grids;
    InitGrid(grid);
    (*grid).nonodes = 2;
    (*grid).xcells = 3;
    (*grid).ycells = 1;
    (*grid).wantedelems = 40;
    (*grid).firstmaterial = 1;
    (*grid).lastmaterial = 1;
    (*grid).autoratio = 1;
    (*grid).coordsystem = COORD_CART1;
    (*grid).numbering = NUMBER_1D;
    (*grid).x[0] = 0.0;
    (*grid).x[1] = 1.0;
    (*grid).x[2] = 3.0;
    (*grid).x[3] = 4.0;
    (*grid).xexpand[1] = 2.0;
    (*grid).xexpand[3] = 0.5;
    (*grid).xdens[2] = 0.5;
    j = 1;
    while j <= 3 {
        (*grid).structure[1][j as usize] = 1;
        j += 1;
    }
    (*grid).noboundaries = 2;
    (*grid).boundint[0] = 1;
    (*grid).boundint[1] = 1;
    (*grid).boundext[0] = -1;
    (*grid).boundext[1] = -2;
    (*grid).boundtype[0] = 1;
    (*grid).boundtype[1] = 2;
    (*grid).boundsolid[0] = 1;
    (*grid).boundsolid[1] = 1;
    (*grid).mappings = 0;
    if info != 0 {
        printf(b"A simple 1D example mesh was created\n\0".as_ptr() as *const c_char);
    }
}

// SAFETY: allocating 2D example grid (C `malloc` shape preserved).
unsafe fn ExampleGrid2D(grids: *mut *mut GridType, nogrids: *mut c_int, info: c_int) {
    let mut j: c_int;
    let grid: *mut GridType;
    *nogrids = 1;
    *grids = malloc((*nogrids as usize) * size_of::<GridType>()) as *mut GridType;
    grid = *grids;
    InitGrid(grid);
    (*grid).xcells = 4;
    (*grid).ycells = 3;
    (*grid).wantedelems = 100;
    (*grid).totxelems = 10;
    (*grid).totyelems = 10;
    (*grid).totzelems = 10;
    (*grid).firstmaterial = 1;
    (*grid).lastmaterial = 1;
    (*grid).autoratio = 1;
    (*grid).coordsystem = COORD_CART2;
    (*grid).numbering = NUMBER_XY;
    (*grid).x[0] = -1.0;
    (*grid).x[1] = 0.0;
    (*grid).x[2] = 2.0;
    (*grid).x[3] = 6.0;
    (*grid).x[4] = 7.0;
    (*grid).y[0] = 0.0;
    (*grid).y[1] = 0.5;
    (*grid).y[2] = 0.75;
    (*grid).y[3] = 1.0;
    (*grid).xexpand[2] = 0.4;
    (*grid).xexpand[3] = 5.0;
    (*grid).yexpand[2] = 2.0;
    (*grid).yexpand[3] = 0.5;
    j = 1;
    while j <= 3 {
        (*grid).structure[j as usize][1] = 2;
        j += 1;
    }
    j = 1;
    while j <= 3 {
        (*grid).structure[j as usize][2] = 1;
        j += 1;
    }
    (*grid).structure[1][2] = 0;
    j = 1;
    while j <= 3 {
        (*grid).structure[j as usize][3] = 1;
        j += 1;
    }
    j = 1;
    while j <= 3 {
        (*grid).structure[j as usize][4] = 3;
        j += 1;
    }
    (*grid).noboundaries = 3;
    (*grid).boundint[0] = 1;
    (*grid).boundint[1] = 1;
    (*grid).boundint[2] = 1;
    (*grid).boundext[0] = 2;
    (*grid).boundext[1] = 3;
    (*grid).boundext[2] = 0;
    (*grid).boundtype[0] = 1;
    (*grid).boundtype[1] = 2;
    (*grid).boundtype[2] = 3;
    (*grid).boundsolid[0] = 1;
    (*grid).boundsolid[1] = 1;
    (*grid).boundsolid[2] = 1;
    (*grid).mappings = 2;
    (*grid).mappingtype[0] = 1;
    (*grid).mappingtype[1] = 1;
    (*grid).mappingline[0] = 0;
    (*grid).mappingline[1] = 3;
    (*grid).mappinglimits[0] = 0.5;
    (*grid).mappinglimits[1] = 0.5;
    (*grid).mappinglimits[2] = 0.5;
    (*grid).mappinglimits[3] = 0.5;
    (*grid).mappingpoints[0] = 4;
    (*grid).mappingpoints[1] = 4;
    // NOTE: C `Rvector` is a macro for `dvector` (egutils.h:15).
    (*grid).mappingparams[0] = dvector(0, (*grid).mappingpoints[0]);
    (*grid).mappingparams[1] = dvector(0, (*grid).mappingpoints[1]);
    *(*grid).mappingparams[0].wrapping_offset(0) = 2.0;
    *(*grid).mappingparams[0].wrapping_offset(1) = 0.0;
    *(*grid).mappingparams[0].wrapping_offset(2) = 6.0;
    *(*grid).mappingparams[0].wrapping_offset(3) = -0.2;
    *(*grid).mappingparams[1].wrapping_offset(0) = 0.0;
    *(*grid).mappingparams[1].wrapping_offset(1) = 0.0;
    *(*grid).mappingparams[1].wrapping_offset(2) = 6.0;
    *(*grid).mappingparams[1].wrapping_offset(3) = 0.3;
    if info != 0 {
        printf(b"A simple 2D example mesh was created\n\0".as_ptr() as *const c_char);
    }
}
// SAFETY: allocating 3D example grid (C `malloc` shape preserved).
unsafe fn ExampleGrid3D(grids: *mut *mut GridType, nogrids: *mut c_int, info: c_int) {
    let mut i: c_int;
    let mut j: c_int;
    let grid: *mut GridType;
    *nogrids = 1;
    *grids = malloc((*nogrids as usize) * size_of::<GridType>()) as *mut GridType;
    grid = *grids;
    InitGrid(grid);
    (*grid).dimension = 3;
    (*grid).coordsystem = COORD_CART3;
    (*grid).layered = TRUE;
    (*grid).xcells = 4;
    (*grid).ycells = 5;
    (*grid).zcells = 5;
    (*grid).wantedelems = 1000;
    (*grid).firstmaterial = 1;
    (*grid).lastmaterial = 3;
    (*grid).autoratio = 1;
    (*grid).coordsystem = COORD_CART3;
    (*grid).numbering = NUMBER_XY;
    (*grid).x[0] = 0.0;
    (*grid).x[1] = 0.1;
    (*grid).x[2] = 0.4;
    (*grid).x[3] = 0.5;
    (*grid).x[4] = 0.6;
    (*grid).y[0] = 0.0;
    (*grid).y[1] = 0.1;
    (*grid).y[2] = 0.2;
    (*grid).y[3] = 0.4;
    (*grid).y[4] = 0.5;
    (*grid).y[5] = 0.6;
    (*grid).z[0] = 0.0;
    (*grid).z[1] = 0.1;
    (*grid).z[2] = 0.2;
    (*grid).z[3] = 0.4;
    (*grid).z[4] = 0.5;
    (*grid).z[5] = 0.6;
    j = 1;
    while j <= 5 {
        i = 1;
        while i <= 4 {
            (*grid).structure[j as usize][i as usize] = 1;
            i += 1;
        }
        j += 1;
    }
    (*grid).structure[2][2] = 3;
    (*grid).structure[2][3] = 3;
    (*grid).structure[3][3] = 3;
    (*grid).structure[4][3] = 3;
    (*grid).structure[4][2] = 3;
    (*grid).structure[3][2] = 2;
    // NOTE: C assigns zlastmaterial[5] twice (3 then 3); both kept.
    (*grid).zlastmaterial[5] = 3;
    (*grid).zlastmaterial[2] = 1;
    (*grid).zlastmaterial[3] = 2;
    (*grid).zlastmaterial[4] = 1;
    (*grid).zlastmaterial[5] = 3;
    (*grid).zmaterial[1] = 1;
    (*grid).zmaterial[2] = 2;
    (*grid).zmaterial[3] = 3;
    (*grid).zmaterial[4] = 4;
    (*grid).zmaterial[5] = 5;
    (*grid).noboundaries = 4;
    (*grid).boundint[0] = 1;
    (*grid).boundint[1] = 1;
    (*grid).boundint[2] = 1;
    // NOTE: C overwrites boundint[2] (=2); both kept.
    (*grid).boundint[2] = 2;
    (*grid).boundext[0] = 0;
    (*grid).boundext[1] = 2;
    (*grid).boundext[2] = 3;
    (*grid).boundext[3] = 3;
    (*grid).boundtype[0] = 1;
    (*grid).boundtype[1] = 2;
    (*grid).boundtype[2] = 3;
    (*grid).boundtype[3] = 4;
    (*grid).boundsolid[0] = 1;
    (*grid).boundsolid[1] = 1;
    (*grid).boundsolid[2] = 1;
    (*grid).boundsolid[3] = 1;
    if info != 0 {
        printf(b"A simple 3D example mesh was created\n\0".as_ptr() as *const c_char);
    }
}

// SAFETY: allocating example-grid dispatcher.
#[no_mangle]
pub unsafe extern "C" fn CreateExampleGrid(
    dim: c_int,
    grids: *mut *mut GridType,
    nogrids: *mut c_int,
    info: c_int,
) {
    // NOTE: cases fall through to no default arm, exactly as in C.
    if dim == 1 {
        ExampleGrid1D(grids, nogrids, info);
    } else if dim == 2 {
        ExampleGrid2D(grids, nogrids, info);
    } else if dim == 3 {
        ExampleGrid3D(grids, nogrids, info);
    }
}
// SAFETY: per-cell derivation over valid grid + cells.
#[no_mangle]
pub unsafe extern "C" fn SetCellData(grid: *mut GridType, cell: *mut CellType, info: c_int) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned at loop head before the occupancy test below.
    let mut cnew: c_int;
    j = 1;
    // NOTE: cells direction up.
    while j <= (*grid).ycells {
        i = 1;
        // NOTE: cells direction right.
        while i <= (*grid).xcells {
            cnew = (*grid).numbered[j as usize][i as usize];
            // NOTE: if cell is occupied.
            if cnew != 0 {
                // NOTE: initialize the numbering to zero.
                (*cell.wrapping_offset(cnew as isize)).left1st = 0;
                (*cell.wrapping_offset(cnew as isize)).left2nd = 0;
                (*cell.wrapping_offset(cnew as isize)).leftlast = 0;
                (*cell.wrapping_offset(cnew as isize)).levelwidth = 0;
                (*cell.wrapping_offset(cnew as isize)).levelwidthcenter = 0;
                (*cell.wrapping_offset(cnew as isize)).leftcenter = 0;
                (*cell.wrapping_offset(cnew as isize)).elemwidth = 0;
                (*cell.wrapping_offset(cnew as isize)).elem1st = 0;
                // NOTE: set the element type.
                (*cell.wrapping_offset(cnew as isize)).nonodes = (*grid).nonodes;
                (*cell.wrapping_offset(cnew as isize)).numbering = (*grid).numbering;
                // NOTE: set the cell coordinates.
                (*cell.wrapping_offset(cnew as isize)).xcorner[BOTLEFT as usize] =
                    (*grid).x[(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).xcorner[TOPLEFT as usize] =
                    (*grid).x[(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).xcorner[TOPRIGHT as usize] =
                    (*grid).x[i as usize];
                (*cell.wrapping_offset(cnew as isize)).xcorner[BOTRIGHT as usize] =
                    (*grid).x[i as usize];
                (*cell.wrapping_offset(cnew as isize)).ycorner[BOTRIGHT as usize] =
                    (*grid).y[(j - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).ycorner[BOTLEFT as usize] =
                    (*grid).y[(j - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).ycorner[TOPRIGHT as usize] =
                    (*grid).y[j as usize];
                (*cell.wrapping_offset(cnew as isize)).ycorner[TOPLEFT as usize] =
                    (*grid).y[j as usize];
                // NOTE: set the cell width in both directions.
                (*cell.wrapping_offset(cnew as isize)).xwidth =
                    (*grid).x[i as usize] - (*grid).x[(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).ywidth =
                    (*grid).y[j as usize] - (*grid).y[(j - 1) as usize];
                // NOTE: set the number of elements in a subcell.
                (*cell.wrapping_offset(cnew as isize)).xelem = (*grid).xelems[i as usize];
                (*cell.wrapping_offset(cnew as isize)).yelem = (*grid).yelems[j as usize];
                // NOTE: set the ratio of elements when going left to right and down to up.
                (*cell.wrapping_offset(cnew as isize)).xratio = (*grid).xratios[i as usize];
                (*cell.wrapping_offset(cnew as isize)).yratio = (*grid).yratios[j as usize];
                (*cell.wrapping_offset(cnew as isize)).xlinear = (*grid).xlinear[i as usize];
                (*cell.wrapping_offset(cnew as isize)).ylinear = (*grid).ylinear[j as usize];
                (*cell.wrapping_offset(cnew as isize)).xlinear = (*grid).xlinear[i as usize];
                (*cell.wrapping_offset(cnew as isize)).ylinear = (*grid).ylinear[j as usize];
                if (*grid).xexpand[i as usize] < 0.0 && (*grid).xlinear[i as usize] == FALSE {
                    if (*grid).xelems[i as usize] == 2 {
                        (*cell.wrapping_offset(cnew as isize)).xlinear = 1;
                    } else {
                        (*cell.wrapping_offset(cnew as isize)).xlinear = 2;
                    }
                }
                if (*grid).yexpand[j as usize] < 0.0 && (*grid).ylinear[j as usize] == FALSE {
                    if (*grid).yelems[j as usize] == 2 {
                        (*cell.wrapping_offset(cnew as isize)).ylinear = 1;
                    } else {
                        (*cell.wrapping_offset(cnew as isize)).ylinear = 2;
                    }
                }
                // NOTE: set the length of the first element in both directions.
                (*cell.wrapping_offset(cnew as isize)).dx1 = (*grid).dx[i as usize];
                (*cell.wrapping_offset(cnew as isize)).dy1 = (*grid).dy[j as usize];
                // NOTE: set the material in question.
                (*cell.wrapping_offset(cnew as isize)).material =
                    (*grid).structure[j as usize][i as usize];
                // NOTE: set the boundary types for sides and corners.
                (*cell.wrapping_offset(cnew as isize)).boundary[LEFT as usize] =
                    (*grid).structure[j as usize][(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[DOWN as usize] =
                    (*grid).structure[(j - 1) as usize][i as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[RIGHT as usize] =
                    (*grid).structure[j as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[UP as usize] =
                    (*grid).structure[(j + 1) as usize][i as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[4] =
                    (*grid).structure[(j - 1) as usize][(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[5] =
                    (*grid).structure[(j - 1) as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[6] =
                    (*grid).structure[(j + 1) as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).boundary[7] =
                    (*grid).structure[(j + 1) as usize][(i - 1) as usize];
                // NOTE: set the neighbour cell indices for sides and corners.
                (*cell.wrapping_offset(cnew as isize)).neighbour[LEFT as usize] =
                    (*grid).numbered[j as usize][(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[DOWN as usize] =
                    (*grid).numbered[(j - 1) as usize][i as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[RIGHT as usize] =
                    (*grid).numbered[j as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[UP as usize] =
                    (*grid).numbered[(j + 1) as usize][i as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[4] =
                    (*grid).numbered[(j - 1) as usize][(i - 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[5] =
                    (*grid).numbered[(j - 1) as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[6] =
                    (*grid).numbered[(j + 1) as usize][(i + 1) as usize];
                (*cell.wrapping_offset(cnew as isize)).neighbour[7] =
                    (*grid).numbered[(j + 1) as usize][(i - 1) as usize];
            }
            i += 1;
        }
        j += 1;
    }
    if info != 0 {
        printf(
            b"%d cells were created.\n\0".as_ptr() as *const c_char,
            (*grid).nocells,
        );
    }
}
// SAFETY: knot numbering over valid grid + cells; returns C status codes.
unsafe fn SetCellKnots(grid: *mut GridType, cell: *mut CellType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut level: c_int;
    let mut center: c_int;
    let degree: c_int;
    let centernodes: c_int;
    let sidenodes: c_int;
    let nonodes: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut cnew: c_int = 0;
    let mut cup: c_int = 0;
    let mut cleft: c_int = 0;
    let mut cleftup: c_int = 0;
    let mut elemno: c_int;
    let mut knotno: c_int;
    let mut maxwidth: c_int;
    let mut width: c_int;
    let numbering: c_int;
    let xcells: c_int;
    let ycells: c_int;
    // NOTE: NULL-inits in C; set on every path before reads (the
    // numbering error path returns early).
    let yelems: *mut c_int;
    let xelems: *mut c_int;
    numbering = (*grid).numbering;
    nonodes = (*grid).nonodes;
    knotno = 0;
    elemno = 0;
    if numbering == NUMBER_XY {
        xcells = (*grid).xcells;
        ycells = (*grid).ycells;
        xelems = (*grid).xelems.as_mut_ptr();
        yelems = (*grid).yelems.as_mut_ptr();
    } else if numbering == NUMBER_YX {
        xcells = (*grid).ycells;
        ycells = (*grid).xcells;
        xelems = (*grid).yelems.as_mut_ptr();
        yelems = (*grid).xelems.as_mut_ptr();
    } else {
        printf(
            b"No %d numbering scheme exists!\n\0".as_ptr() as *const c_char,
            numbering,
        );
        return 1;
    }
    // NOTE: C switch over 4/5/8/9/12/16.
    if nonodes == 4 {
        centernodes = FALSE;
        sidenodes = FALSE;
        degree = 1;
    } else if nonodes == 5 {
        centernodes = TRUE;
        sidenodes = FALSE;
        degree = 2;
    } else if nonodes == 8 {
        centernodes = FALSE;
        sidenodes = TRUE;
        degree = 2;
    } else if nonodes == 9 {
        centernodes = TRUE;
        sidenodes = TRUE;
        degree = 2;
    } else if nonodes == 12 {
        centernodes = FALSE;
        sidenodes = TRUE;
        degree = 3;
    } else if nonodes == 16 {
        centernodes = TRUE;
        sidenodes = TRUE;
        degree = 3;
    } else {
        printf(
            b"SetCellKnots: No numbering scheme for %d-node elements.\n\0".as_ptr()
                as *const c_char,
            (*grid).nonodes,
        );
        return 2;
    }
    j = 1;
    // NOTE: cells direction up.
    while j <= ycells {
        level = 0;
        // NOTE: lines inside cells.
        while level <= *yelems.wrapping_offset(j as isize) {
            center = 0;
            while center < degree {
                i = 1;
                // NOTE: cells direction right.
                while i <= xcells {
                    if numbering == NUMBER_XY {
                        cnew = (*grid).numbered[j as usize][i as usize];
                        cleft = (*grid).numbered[j as usize][(i - 1) as usize];
                        cup = (*grid).numbered[(j + 1) as usize][i as usize];
                        cleftup = (*grid).numbered[(j + 1) as usize][(i - 1) as usize];
                        if cnew != 0 {
                            (*cell.wrapping_offset(cnew as isize)).xind = i;
                            (*cell.wrapping_offset(cnew as isize)).yind = j;
                        }
                    } else if numbering == NUMBER_YX {
                        cnew = (*grid).numbered[i as usize][j as usize];
                        cleft = (*grid).numbered[(i - 1) as usize][j as usize];
                        cup = (*grid).numbered[i as usize][(j + 1) as usize];
                        cleftup = (*grid).numbered[(i - 1) as usize][(j + 1) as usize];
                        if cnew != 0 {
                            (*cell.wrapping_offset(cnew as isize)).xind = j;
                            (*cell.wrapping_offset(cnew as isize)).yind = i;
                        }
                    }
                    if center == 0 {
                        // NOTE: the first line of an occupied cell is
                        // unnumbered, this happens only in the bottom.
                        if cnew != 0
                            && level == 0
                            && (*cell.wrapping_offset(cnew as isize)).left1st == 0
                        {
                            if cleft == 0 {
                                knotno += 1;
                            }
                            (*cell.wrapping_offset(cnew as isize)).left1st = knotno;
                            if sidenodes != 0 {
                                knotno += degree * *xelems.wrapping_offset(i as isize);
                            } else {
                                knotno += *xelems.wrapping_offset(i as isize);
                            }
                        } else if cnew != 0
                            && level > 0
                            && level < *yelems.wrapping_offset(j as isize)
                        {
                            // NOTE: the n:th line of an occupied cell.
                            if cleft == 0 {
                                knotno += 1;
                            }
                            if level == 1 {
                                (*cell.wrapping_offset(cnew as isize)).left2nd = knotno;
                            }
                            if level == 2 {
                                (*cell.wrapping_offset(cnew as isize)).levelwidth =
                                    knotno - (*cell.wrapping_offset(cnew as isize)).left2nd;
                            }
                            if sidenodes != 0 {
                                knotno += degree * *xelems.wrapping_offset(i as isize);
                            } else {
                                knotno += *xelems.wrapping_offset(i as isize);
                            }
                        } else if (cnew != 0 || cup != 0)
                            && level == *yelems.wrapping_offset(j as isize)
                        {
                            // NOTE: last line of this cell or first line of upper cell.
                            if cleft == 0 && cleftup == 0 {
                                knotno += 1;
                            }
                            if cnew != 0 {
                                (*cell.wrapping_offset(cnew as isize)).leftlast = knotno;
                            }
                            if level == 2 {
                                (*cell.wrapping_offset(cnew as isize)).levelwidth =
                                    knotno - (*cell.wrapping_offset(cnew as isize)).left2nd;
                            }
                            if *yelems.wrapping_offset(j as isize) == 1 && cnew != 0 {
                                (*cell.wrapping_offset(cnew as isize)).left2nd =
                                    (*cell.wrapping_offset(cnew as isize)).leftlast;
                            }
                            if cup != 0 {
                                (*cell.wrapping_offset(cup as isize)).left1st = knotno;
                            }
                            if sidenodes != 0 {
                                knotno += degree * *xelems.wrapping_offset(i as isize);
                            } else {
                                knotno += *xelems.wrapping_offset(i as isize);
                            }
                        }
                        // NOTE: number the elements.
                        if cnew != 0 && level > 0 {
                            if level == 1 {
                                (*cell.wrapping_offset(cnew as isize)).elem1st = elemno + 1;
                            }
                            if level == 2 {
                                (*cell.wrapping_offset(cnew as isize)).elemwidth =
                                    (elemno + 1) - (*cell.wrapping_offset(cnew as isize)).elem1st;
                            }
                            elemno += *xelems.wrapping_offset(i as isize);
                        }
                    }
                    if center != 0 && level < *yelems.wrapping_offset(j as isize) && cnew != 0 {
                        if cleft == 0 && sidenodes != 0 {
                            knotno += 1;
                        }
                        if level == 0 && center == 1 {
                            (*cell.wrapping_offset(cnew as isize)).leftcenter = knotno;
                        }
                        if level == 0 && center == 2 {
                            (*cell.wrapping_offset(cnew as isize)).left2center = knotno;
                        }
                        if level == 1 && center == 1 {
                            (*cell.wrapping_offset(cnew as isize)).levelwidthcenter =
                                knotno - (*cell.wrapping_offset(cnew as isize)).leftcenter;
                        }
                        if centernodes != 0 && sidenodes != 0 {
                            knotno += degree * *xelems.wrapping_offset(i as isize);
                        } else {
                            knotno += *xelems.wrapping_offset(i as isize);
                        }
                    }
                    i += 1;
                }
                center += 1;
            }
            level += 1;
        }
        j += 1;
    }
    maxwidth = 0;
    j = 1;
    while j <= ycells {
        i = 1;
        while i <= xcells {
            if numbering == NUMBER_XY {
                cnew = (*grid).numbered[j as usize][i as usize];
            } else if numbering == NUMBER_YX {
                cnew = (*grid).numbered[i as usize][j as usize];
            }
            if cnew != 0 {
                width = (*cell.wrapping_offset(cnew as isize)).left2nd
                    - (*cell.wrapping_offset(cnew as isize)).left1st;
                if width > maxwidth {
                    maxwidth = width;
                }
                width = (*cell.wrapping_offset(cnew as isize)).levelwidth;
                if width > maxwidth {
                    maxwidth = width;
                }
                width = (*cell.wrapping_offset(cnew as isize)).leftlast
                    - (*cell.wrapping_offset(cnew as isize)).left2nd
                    - (*yelems.wrapping_offset(j as isize) - 2)
                        * (*cell.wrapping_offset(cnew as isize)).levelwidth;
                if width > maxwidth {
                    maxwidth = width;
                }
            }
            i += 1;
        }
        j += 1;
    }
    maxwidth += 2;
    (*grid).maxwidth = maxwidth;
    (*grid).noknots = knotno;
    (*grid).noelements = elemno;
    if info != 0 {
        printf(
            b"Numbered %d knots in %d %d-node elements.\n\0".as_ptr() as *const c_char,
            knotno,
            elemno,
            nonodes,
        );
        if numbering == NUMBER_XY {
            printf(
                b"Numbering order was <x><y> and max levelwidth %d.\n\0".as_ptr() as *const c_char,
                maxwidth,
            );
        } else if numbering == NUMBER_YX {
            printf(
                b"Numbering order was <y><x> and max levelwidth %d.\n\0".as_ptr() as *const c_char,
                maxwidth,
            );
        }
    }
    0
}
// SAFETY: 1D knot numbering over valid grid + cells; returns C status codes.
unsafe fn SetCellKnots1D(grid: *mut GridType, cell: *mut CellType, info: c_int) -> c_int {
    let mut i: c_int;
    let degree: c_int;
    let nonodes: c_int;
    let mut cnew: c_int;
    let mut cleft: c_int;
    let mut elemno: c_int;
    let mut knotno: c_int;
    let maxwidth: c_int;
    let xcells: c_int;
    // NOTE: points at grid storage (C array decay).
    let xelems: *mut c_int;
    nonodes = (*grid).nonodes;
    knotno = 0;
    elemno = 0;
    xcells = (*grid).xcells;
    xelems = (*grid).xelems.as_mut_ptr();
    // NOTE: C switch over 2/3/4.
    if nonodes == 2 {
        degree = 1;
    } else if nonodes == 3 {
        degree = 2;
    } else if nonodes == 4 {
        degree = 3;
    } else {
        printf(
            b"SetCellKnots1D: No numbering scheme for %d-node elements.\n\0".as_ptr()
                as *const c_char,
            (*grid).nonodes,
        );
        return 2;
    }
    i = 1;
    while i <= xcells {
        cnew = (*grid).numbered[1][i as usize];
        cleft = (*grid).numbered[1][(i - 1) as usize];
        if cnew != 0 {
            (*cell.wrapping_offset(cnew as isize)).xind = i;
        }
        if cnew != 0 {
            // NOTE: number the nodes.
            if cleft == 0 {
                knotno += 1;
            }
            (*cell.wrapping_offset(cnew as isize)).left1st = knotno;
            knotno += degree * *xelems.wrapping_offset(i as isize);
            // NOTE: number the elements.
            (*cell.wrapping_offset(cnew as isize)).elem1st = elemno + 1;
            elemno += *xelems.wrapping_offset(i as isize);
        }
        i += 1;
    }
    maxwidth = degree;
    (*grid).maxwidth = maxwidth;
    (*grid).noknots = knotno;
    (*grid).noelements = elemno;
    if info != 0 {
        printf(
            b"Numbered %d knots in %d %d-node elements.\n\0".as_ptr() as *const c_char,
            knotno,
            elemno,
            nonodes,
        );
    }
    0
}

// SAFETY: allocating cell array (C `malloc` shape preserved).
#[no_mangle]
pub unsafe extern "C" fn CreateCells(grid: *mut GridType, cell: *mut *mut CellType, info: c_int) {
    *cell = malloc((((*grid).nocells + 1) as usize) * size_of::<CellType>()) as *mut CellType;
    SetCellData(grid, *cell, info);
    if (*grid).dimension == 1 {
        SetCellKnots1D(grid, *cell, info);
    } else {
        SetCellKnots(grid, *cell, info);
    }
}

// SAFETY: never called in C either (no callers); kept for ABI shape.
#[no_mangle]
pub unsafe extern "C" fn DestroyCells(cell: *mut *mut CellType) {
    // NOTE: C literally frees the handle (`free(cell)` on the `**`);
    // mirrored exactly since the function is dead.
    free(cell as *mut c_void);
}

// SAFETY: pure index arithmetic over a valid cell.
#[no_mangle]
pub unsafe extern "C" fn GetKnotIndex(cell: *mut CellType, i: c_int, j: c_int) -> c_int {
    // NOTE: assigned on every path before reads (C `=0` decl init shape).
    let mut ind: c_int;
    // NOTE: YX-swap temp, assigned before use.
    let aid: c_int;
    // NOTE: assigned on every path before reads (C `=0` decl init shape).
    let maxj: c_int;
    // NOTE: C reassigns the by-value params for YX (mirrored with shadows).
    let (mut i, mut j) = (i, j);
    if (*cell).numbering == NUMBER_1D {
        ind = (*cell).left1st;
        if (*cell).nonodes == 2 {
            ind += i;
        } else if (*cell).nonodes == 3 {
            ind += 2 * i;
        } else if (*cell).nonodes == 4 {
            ind += 3 * i;
        }
        return ind;
    }
    if (*cell).numbering == NUMBER_XY {
        maxj = (*cell).yelem;
    } else if (*cell).numbering == NUMBER_YX {
        aid = j;
        j = i;
        i = aid;
        maxj = (*cell).xelem;
    } else {
        maxj = 0;
        bigerror(b"GetKnotIndex: Unknown numbering scheme!\0".as_ptr() as *const c_char);
    }
    if j == 0 {
        ind = (*cell).left1st;
    } else if j == maxj {
        ind = (*cell).leftlast;
    } else {
        ind = (*cell).left2nd + (j - 1) * (*cell).levelwidth;
    }
    if (*cell).nonodes == 4 {
        ind += i;
    } else if (*cell).nonodes == 5 {
        ind += i;
    } else if (*cell).nonodes == 8 || (*cell).nonodes == 9 {
        ind += 2 * i;
    } else if (*cell).nonodes == 12 || (*cell).nonodes == 16 {
        ind += 3 * i;
    }
    ind
}
// SAFETY: pure coordinate arithmetic over a valid cell.
#[no_mangle]
pub unsafe extern "C" fn GetKnotCoordinate(
    cell: *mut CellType,
    i: c_int,
    j: c_int,
    x: *mut Real,
    y: *mut Real,
) -> c_int {
    let ind: c_int;
    if (*cell).xlinear == 1 {
        *x = (*cell).xcorner[BOTLEFT as usize] + (*cell).dx1 * i as Real;
    } else if (*cell).xlinear == 0 {
        *x = (*cell).xcorner[BOTLEFT as usize]
            + (*cell).dx1 * (1.0 - pow((*cell).xratio, i as Real)) / (1.0 - (*cell).xratio);
    } else if (*cell).xlinear == 2 {
        if i <= (*cell).xelem / 2 {
            *x = (*cell).xcorner[BOTLEFT as usize]
                + (*cell).dx1 * (1.0 - pow((*cell).xratio, i as Real)) / (1.0 - (*cell).xratio);
        } else {
            *x = (*cell).xcorner[BOTRIGHT as usize]
                - (*cell).dx1 * (1.0 - pow((*cell).xratio, ((*cell).xelem - i) as Real))
                    / (1.0 - (*cell).xratio);
        }
    }
    if (*cell).ylinear == 1 {
        *y = (*cell).ycorner[BOTLEFT as usize] + (*cell).dy1 * j as Real;
    } else if (*cell).ylinear == 0 {
        *y = (*cell).ycorner[BOTLEFT as usize]
            + (*cell).dy1 * (1.0 - pow((*cell).yratio, j as Real)) / (1.0 - (*cell).yratio);
    } else if (*cell).ylinear == 2 {
        if j <= (*cell).yelem / 2 {
            *y = (*cell).ycorner[BOTLEFT as usize]
                + (*cell).dy1 * (1.0 - pow((*cell).yratio, j as Real)) / (1.0 - (*cell).yratio);
        } else {
            *y = (*cell).ycorner[TOPLEFT as usize]
                - (*cell).dy1 * (1.0 - pow((*cell).yratio, ((*cell).yelem - j) as Real))
                    / (1.0 - (*cell).yratio);
        }
    }
    ind = GetKnotIndex(cell, i, j);
    ind
}
// SAFETY: knot-index fill over a valid cell with caller-sized ind.
#[no_mangle]
pub unsafe extern "C" fn GetElementIndices(
    cell: *mut CellType,
    i: c_int,
    j: c_int,
    ind: *mut c_int,
) -> c_int {
    let nonodes: c_int;
    let numbering: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut elemind: c_int = 0;
    nonodes = (*cell).nonodes;
    numbering = (*cell).numbering;
    if numbering != NUMBER_1D {
        *ind.wrapping_offset(TOPRIGHT as isize) = GetKnotIndex(cell, i, j);
        *ind.wrapping_offset(BOTRIGHT as isize) = GetKnotIndex(cell, i, j - 1);
        *ind.wrapping_offset(TOPLEFT as isize) = GetKnotIndex(cell, i - 1, j);
        *ind.wrapping_offset(BOTLEFT as isize) = GetKnotIndex(cell, i - 1, j - 1);
    }
    if numbering == NUMBER_XY {
        elemind = (*cell).elem1st + (i - 1) + (j - 1) * (*cell).elemwidth;
        if nonodes == 4 {
            return elemind;
        }
        if nonodes == 5 {
            *ind.wrapping_offset(4) = (*cell).leftcenter + (j - 1) * (*cell).levelwidthcenter + i;
        } else if nonodes == 8 {
            *ind.wrapping_offset(4) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(6) = *ind.wrapping_offset(3) + 1;
            *ind.wrapping_offset(5) = (*cell).leftcenter + (j - 1) * (*cell).levelwidthcenter + i;
            *ind.wrapping_offset(7) = *ind.wrapping_offset(5) - 1;
        } else if nonodes == 9 {
            *ind.wrapping_offset(4) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(6) = *ind.wrapping_offset(3) + 1;
            *ind.wrapping_offset(5) =
                (*cell).leftcenter + (j - 1) * (*cell).levelwidthcenter + 2 * i;
            *ind.wrapping_offset(7) = *ind.wrapping_offset(5) - 2;
            *ind.wrapping_offset(8) = *ind.wrapping_offset(5) - 1;
        } else if nonodes == 12 {
            *ind.wrapping_offset(4) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(0) + 2;
            *ind.wrapping_offset(9) = *ind.wrapping_offset(3) + 1;
            *ind.wrapping_offset(8) = *ind.wrapping_offset(3) + 2;
            *ind.wrapping_offset(6) = (*cell).leftcenter + (j - 1) * (*cell).levelwidthcenter + i;
            *ind.wrapping_offset(11) = *ind.wrapping_offset(6) - 1;
            *ind.wrapping_offset(7) = (*cell).left2center + (j - 1) * (*cell).levelwidthcenter + i;
            *ind.wrapping_offset(10) = *ind.wrapping_offset(7) - 1;
        } else if nonodes == 16 {
            *ind.wrapping_offset(4) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(0) + 2;
            *ind.wrapping_offset(9) = *ind.wrapping_offset(3) + 1;
            *ind.wrapping_offset(8) = *ind.wrapping_offset(3) + 2;
            *ind.wrapping_offset(6) =
                (*cell).leftcenter + (j - 1) * (*cell).levelwidthcenter + 3 * i;
            *ind.wrapping_offset(11) = *ind.wrapping_offset(6) - 3;
            *ind.wrapping_offset(7) =
                (*cell).left2center + (j - 1) * (*cell).levelwidthcenter + 3 * i;
            *ind.wrapping_offset(10) = *ind.wrapping_offset(7) - 3;
            *ind.wrapping_offset(13) = *ind.wrapping_offset(6) - 1;
            *ind.wrapping_offset(12) = *ind.wrapping_offset(6) - 2;
            *ind.wrapping_offset(14) = *ind.wrapping_offset(7) - 1;
            *ind.wrapping_offset(15) = *ind.wrapping_offset(7) - 2;
        } else {
            printf(
                b"GetElementIndices: not implemented for %d nodes.\n\0".as_ptr() as *const c_char,
                nonodes,
            );
        }
    } else if numbering == NUMBER_YX {
        elemind = (*cell).elem1st + (j - 1) + (i - 1) * (*cell).elemwidth;
        if nonodes == 4 {
            return elemind;
        }
        if nonodes == 5 {
            *ind.wrapping_offset(4) = (*cell).leftcenter + (i - 1) * (*cell).levelwidthcenter + j;
        } else if nonodes == 8 {
            *ind.wrapping_offset(7) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(1) + 1;
            *ind.wrapping_offset(6) = (*cell).leftcenter + (i - 1) * (*cell).levelwidthcenter + j;
            *ind.wrapping_offset(4) = *ind.wrapping_offset(6) - 1;
        } else if nonodes == 9 {
            *ind.wrapping_offset(7) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(1) + 1;
            *ind.wrapping_offset(6) =
                (*cell).leftcenter + (i - 1) * (*cell).levelwidthcenter + 2 * j;
            *ind.wrapping_offset(4) = *ind.wrapping_offset(6) - 2;
            *ind.wrapping_offset(8) = *ind.wrapping_offset(6) - 1;
        } else if nonodes == 12 {
            *ind.wrapping_offset(11) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(10) = *ind.wrapping_offset(0) + 2;
            *ind.wrapping_offset(6) = *ind.wrapping_offset(1) + 1;
            *ind.wrapping_offset(7) = *ind.wrapping_offset(1) + 2;
            *ind.wrapping_offset(9) = (*cell).leftcenter + (i - 1) * (*cell).levelwidthcenter + j;
            *ind.wrapping_offset(4) = *ind.wrapping_offset(9) - 1;
            *ind.wrapping_offset(8) = (*cell).left2center + (i - 1) * (*cell).levelwidthcenter + j;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(8) - 1;
        } else if nonodes == 16 {
            *ind.wrapping_offset(11) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(10) = *ind.wrapping_offset(0) + 2;
            *ind.wrapping_offset(6) = *ind.wrapping_offset(1) + 1;
            *ind.wrapping_offset(7) = *ind.wrapping_offset(1) + 2;
            *ind.wrapping_offset(9) =
                (*cell).leftcenter + (i - 1) * (*cell).levelwidthcenter + 3 * j;
            *ind.wrapping_offset(4) = *ind.wrapping_offset(9) - 3;
            *ind.wrapping_offset(15) = *ind.wrapping_offset(9) - 1;
            *ind.wrapping_offset(12) = *ind.wrapping_offset(9) - 2;
            *ind.wrapping_offset(8) =
                (*cell).left2center + (i - 1) * (*cell).levelwidthcenter + 3 * j;
            *ind.wrapping_offset(5) = *ind.wrapping_offset(8) - 3;
            *ind.wrapping_offset(14) = *ind.wrapping_offset(8) - 1;
            *ind.wrapping_offset(13) = *ind.wrapping_offset(8) - 2;
        } else {
            printf(
                b"GetElementIndices: not implemented for %d nodes.\n\0".as_ptr() as *const c_char,
                nonodes,
            );
        }
    } else if numbering == NUMBER_1D {
        elemind = (*cell).elem1st + (i - 1);
        *ind.wrapping_offset(0) = GetKnotIndex(cell, i - 1, 1);
        if nonodes == 2 {
            *ind.wrapping_offset(1) = *ind.wrapping_offset(0) + 1;
        } else if nonodes == 3 {
            *ind.wrapping_offset(2) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(1) = *ind.wrapping_offset(0) + 2;
        } else if nonodes == 4 {
            *ind.wrapping_offset(2) = *ind.wrapping_offset(0) + 1;
            *ind.wrapping_offset(3) = *ind.wrapping_offset(0) + 2;
            *ind.wrapping_offset(1) = *ind.wrapping_offset(0) + 3;
        }
    }
    elemind
}
// SAFETY: pure index arithmetic over a valid cell.
#[no_mangle]
pub unsafe extern "C" fn GetElementIndex(cell: *mut CellType, i: c_int, j: c_int) -> c_int {
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut elemind: c_int = 0;
    if (*cell).numbering == NUMBER_XY {
        elemind = (*cell).elem1st + (i - 1) + (j - 1) * (*cell).elemwidth;
    } else if (*cell).numbering == NUMBER_YX {
        elemind = (*cell).elem1st + (j - 1) + (i - 1) * (*cell).elemwidth;
    }
    elemind
}

// SAFETY: coordinate + index fill over a valid cell with caller-sized buffers.
// Assumes rectangular elements exactly as in C.
#[no_mangle]
pub unsafe extern "C" fn GetElementCoordinates(
    cell: *mut CellType,
    i: c_int,
    j: c_int,
    globalcoord: *mut Real,
    ind: *mut c_int,
) -> c_int {
    let k: c_int;
    let nonodes: c_int;
    let numbering: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut elemind: c_int = 0;
    // NOTE: `k = nonodes` decides the 2nd-half stride for y coords.
    let xrat: Real;
    let yrat: Real;
    k = (*cell).nonodes;
    nonodes = (*cell).nonodes;
    numbering = (*cell).numbering;
    if numbering == NUMBER_1D {
        elemind = (*cell).elem1st + (i - 1);
        *ind.wrapping_offset(0) = GetKnotCoordinate(
            cell,
            i - 1,
            j - 1,
            globalcoord.wrapping_offset(0),
            globalcoord.wrapping_offset(nonodes as isize),
        );
        *ind.wrapping_offset(1) = GetKnotCoordinate(
            cell,
            i,
            j - 1,
            globalcoord.wrapping_offset(1),
            globalcoord.wrapping_offset((nonodes + 1) as isize),
        );
        if nonodes == 3 {
            *globalcoord.wrapping_offset(2) =
                (*globalcoord.wrapping_offset(0) + *globalcoord.wrapping_offset(1)) / 2.0;
            *globalcoord.wrapping_offset(5) =
                (*globalcoord.wrapping_offset(3) + *globalcoord.wrapping_offset(4)) / 2.0;
            *ind.wrapping_offset(2) = *ind.wrapping_offset(0) + 1;
        } else if nonodes == 4 {
            *globalcoord.wrapping_offset(2) =
                (2.0 * *globalcoord.wrapping_offset(0) + *globalcoord.wrapping_offset(1)) / 3.0;
            *globalcoord.wrapping_offset(6) =
                (2.0 * *globalcoord.wrapping_offset(4) + *globalcoord.wrapping_offset(5)) / 3.0;
            *ind.wrapping_offset(2) = *ind.wrapping_offset(0) + 1;
            *globalcoord.wrapping_offset(3) =
                (*globalcoord.wrapping_offset(0) + 2.0 * *globalcoord.wrapping_offset(1)) / 3.0;
            *globalcoord.wrapping_offset(7) =
                (*globalcoord.wrapping_offset(4) + 2.0 * *globalcoord.wrapping_offset(5)) / 3.0;
            *ind.wrapping_offset(3) = *ind.wrapping_offset(0) + 2;
        }
        return elemind;
    } else if numbering == NUMBER_XY {
        elemind = (*cell).elem1st + (i - 1) + (j - 1) * (*cell).elemwidth;
    } else if numbering == NUMBER_YX {
        elemind = (*cell).elem1st + (j - 1) + (i - 1) * (*cell).elemwidth;
    }
    *ind.wrapping_offset(TOPRIGHT as isize) = GetKnotCoordinate(
        cell,
        i,
        j,
        globalcoord.wrapping_offset(TOPRIGHT as isize),
        globalcoord.wrapping_offset((TOPRIGHT + nonodes) as isize),
    );
    *ind.wrapping_offset(BOTRIGHT as isize) = GetKnotCoordinate(
        cell,
        i,
        j - 1,
        globalcoord.wrapping_offset(BOTRIGHT as isize),
        globalcoord.wrapping_offset((BOTRIGHT + nonodes) as isize),
    );
    *ind.wrapping_offset(TOPLEFT as isize) = GetKnotCoordinate(
        cell,
        i - 1,
        j,
        globalcoord.wrapping_offset(TOPLEFT as isize),
        globalcoord.wrapping_offset((TOPLEFT + nonodes) as isize),
    );
    *ind.wrapping_offset(BOTLEFT as isize) = GetKnotCoordinate(
        cell,
        i - 1,
        j - 1,
        globalcoord.wrapping_offset(BOTLEFT as isize),
        globalcoord.wrapping_offset((BOTLEFT + nonodes) as isize),
    );
    if nonodes == 4 {
        return elemind;
    }
    GetElementIndices(cell, i, j, ind);
    // NOTE: `#if 0` sqrt-ratio block is runtime-dead; the live tail
    // assigns both ratios to 1.0.
    xrat = 1.0;
    yrat = 1.0;
    if nonodes == 5 {
        *globalcoord.wrapping_offset(4) =
            0.5 * (*globalcoord.wrapping_offset(0) + *globalcoord.wrapping_offset(2));
        *globalcoord.wrapping_offset((4 + k) as isize) = 0.5
            * (*globalcoord.wrapping_offset(k as isize)
                + *globalcoord.wrapping_offset((2 + k) as isize));
    } else if nonodes == 8 || nonodes == 9 {
        *globalcoord.wrapping_offset(4) = (xrat * *globalcoord.wrapping_offset(0)
            + *globalcoord.wrapping_offset(1))
            / (1.0 + xrat);
        *globalcoord.wrapping_offset((4 + k) as isize) = *globalcoord.wrapping_offset(k as isize);
        *globalcoord.wrapping_offset(5) = *globalcoord.wrapping_offset(1);
        *globalcoord.wrapping_offset((5 + k) as isize) = (yrat
            * *globalcoord.wrapping_offset((1 + k) as isize)
            + *globalcoord.wrapping_offset((2 + k) as isize))
            / (1.0 + yrat);
        *globalcoord.wrapping_offset(6) = *globalcoord.wrapping_offset(4);
        *globalcoord.wrapping_offset((6 + k) as isize) =
            *globalcoord.wrapping_offset((2 + k) as isize);
        *globalcoord.wrapping_offset(7) = *globalcoord.wrapping_offset(3);
        *globalcoord.wrapping_offset((7 + k) as isize) =
            *globalcoord.wrapping_offset((5 + k) as isize);
        if nonodes == 9 {
            *globalcoord.wrapping_offset(8) = *globalcoord.wrapping_offset(4);
            *globalcoord.wrapping_offset((8 + k) as isize) =
                *globalcoord.wrapping_offset((5 + k) as isize);
        }
    } else if nonodes == 12 || nonodes == 16 {
        coords12(globalcoord, k, nonodes);
    }
    // NOTE: trailing `#if 0` debug loop omitted.
    elemind
}

// SAFETY: 12/16-node side-midpoint interpolation (called with valid buffers).
unsafe fn coords12(globalcoord: *mut Real, k: c_int, nonodes: c_int) {
    *globalcoord.wrapping_offset(4) =
        (2.0 * *globalcoord.wrapping_offset(0) + *globalcoord.wrapping_offset(1)) / 3.0;
    *globalcoord.wrapping_offset((4 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset(k as isize)
        + *globalcoord.wrapping_offset((1 + k) as isize))
        / 3.0;
    *globalcoord.wrapping_offset(5) =
        (2.0 * *globalcoord.wrapping_offset(1) + *globalcoord.wrapping_offset(0)) / 3.0;
    *globalcoord.wrapping_offset((5 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((1 + k) as isize)
        + *globalcoord.wrapping_offset(k as isize))
        / 3.0;
    *globalcoord.wrapping_offset(6) =
        (2.0 * *globalcoord.wrapping_offset(1) + *globalcoord.wrapping_offset(2)) / 3.0;
    *globalcoord.wrapping_offset((6 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((1 + k) as isize)
        + *globalcoord.wrapping_offset((2 + k) as isize))
        / 3.0;
    *globalcoord.wrapping_offset(7) =
        (2.0 * *globalcoord.wrapping_offset(2) + *globalcoord.wrapping_offset(1)) / 3.0;
    *globalcoord.wrapping_offset((7 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((2 + k) as isize)
        + *globalcoord.wrapping_offset((1 + k) as isize))
        / 3.0;
    *globalcoord.wrapping_offset(8) =
        (2.0 * *globalcoord.wrapping_offset(2) + *globalcoord.wrapping_offset(3)) / 3.0;
    *globalcoord.wrapping_offset((8 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((2 + k) as isize)
        + *globalcoord.wrapping_offset((3 + k) as isize))
        / 3.0;
    *globalcoord.wrapping_offset(9) =
        (2.0 * *globalcoord.wrapping_offset(3) + *globalcoord.wrapping_offset(2)) / 3.0;
    *globalcoord.wrapping_offset((9 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((3 + k) as isize)
        + *globalcoord.wrapping_offset((2 + k) as isize))
        / 3.0;
    *globalcoord.wrapping_offset(10) =
        (2.0 * *globalcoord.wrapping_offset(3) + *globalcoord.wrapping_offset(0)) / 3.0;
    *globalcoord.wrapping_offset((10 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset((3 + k) as isize)
        + *globalcoord.wrapping_offset(k as isize))
        / 3.0;
    *globalcoord.wrapping_offset(11) =
        (2.0 * *globalcoord.wrapping_offset(0) + *globalcoord.wrapping_offset(3)) / 3.0;
    *globalcoord.wrapping_offset((11 + k) as isize) = (2.0
        * *globalcoord.wrapping_offset(k as isize)
        + *globalcoord.wrapping_offset((3 + k) as isize))
        / 3.0;
    if nonodes == 16 {
        *globalcoord.wrapping_offset(12) =
            (2.0 * *globalcoord.wrapping_offset(11) + *globalcoord.wrapping_offset(6)) / 3.0;
        *globalcoord.wrapping_offset((12 + k) as isize) = (2.0
            * *globalcoord.wrapping_offset((11 + k) as isize)
            + *globalcoord.wrapping_offset((6 + k) as isize))
            / 3.0;
        *globalcoord.wrapping_offset(13) =
            (2.0 * *globalcoord.wrapping_offset(6) + *globalcoord.wrapping_offset(11)) / 3.0;
        *globalcoord.wrapping_offset((13 + k) as isize) = (2.0
            * *globalcoord.wrapping_offset((6 + k) as isize)
            + *globalcoord.wrapping_offset((11 + k) as isize))
            / 3.0;
        *globalcoord.wrapping_offset(14) =
            (2.0 * *globalcoord.wrapping_offset(7) + *globalcoord.wrapping_offset(10)) / 3.0;
        *globalcoord.wrapping_offset((14 + k) as isize) = (2.0
            * *globalcoord.wrapping_offset((7 + k) as isize)
            + *globalcoord.wrapping_offset((10 + k) as isize))
            / 3.0;
        *globalcoord.wrapping_offset(15) =
            (2.0 * *globalcoord.wrapping_offset(10) + *globalcoord.wrapping_offset(7)) / 3.0;
        *globalcoord.wrapping_offset((15 + k) as isize) = (2.0
            * *globalcoord.wrapping_offset((10 + k) as isize)
            + *globalcoord.wrapping_offset((7 + k) as isize))
            / 3.0;
    }
}
// SAFETY: side-element lookup over valid cells; returns C boolean status.
#[no_mangle]
pub unsafe extern "C" fn GetSideInfo(
    cell: *mut CellType,
    cellno: c_int,
    side: c_int,
    element: c_int,
    elemind: *mut c_int,
) -> c_int {
    let mut more: c_int;
    // NOTE: assigned before every read (C uninit decl shape; the 1D-side
    // and default arms only write through it).
    let sideno: c_int;
    // NOTE: scratch fill buffer for GetElementIndices (contents never read).
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    more = TRUE;
    if (*cell.wrapping_offset(cellno as isize)).numbering == NUMBER_1D {
        // NOTE: C switch over side 0/1.
        if side == 0 {
            more = FALSE;
            *elemind.wrapping_offset(0) = GetElementIndices(
                &mut (*cell.wrapping_offset(cellno as isize)),
                1,
                element,
                ind.as_mut_ptr(),
            );
            sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[LEFT as usize];
            if sideno != 0 {
                *elemind.wrapping_offset(1) = GetElementIndices(
                    &mut (*cell.wrapping_offset(sideno as isize)),
                    (*cell.wrapping_offset(sideno as isize)).xelem,
                    element,
                    ind.as_mut_ptr(),
                );
            } else {
                *elemind.wrapping_offset(1) = 0;
            }
        } else if side == 1 {
            more = FALSE;
            *elemind.wrapping_offset(0) = GetElementIndices(
                &mut (*cell.wrapping_offset(cellno as isize)),
                (*cell.wrapping_offset(cellno as isize)).xelem,
                element,
                ind.as_mut_ptr(),
            );
            sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[RIGHT as usize];
            if sideno != 0 {
                *elemind.wrapping_offset(1) = GetElementIndices(
                    &mut (*cell.wrapping_offset(sideno as isize)),
                    1,
                    element,
                    ind.as_mut_ptr(),
                );
            } else {
                *elemind.wrapping_offset(1) = 0;
            }
        }
        return more;
    }
    // NOTE: C switch over LEFT/RIGHT/DOWN/UP.
    if side == LEFT {
        if element == (*cell.wrapping_offset(cellno as isize)).yelem {
            more = FALSE;
        }
        *elemind.wrapping_offset(0) = GetElementIndices(
            &mut (*cell.wrapping_offset(cellno as isize)),
            1,
            element,
            ind.as_mut_ptr(),
        );
        sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[LEFT as usize];
        if sideno != 0 {
            *elemind.wrapping_offset(1) = GetElementIndices(
                &mut (*cell.wrapping_offset(sideno as isize)),
                (*cell.wrapping_offset(sideno as isize)).xelem,
                element,
                ind.as_mut_ptr(),
            );
        } else {
            *elemind.wrapping_offset(1) = 0;
        }
    } else if side == RIGHT {
        if element == (*cell.wrapping_offset(cellno as isize)).yelem {
            more = FALSE;
        }
        *elemind.wrapping_offset(0) = GetElementIndices(
            &mut (*cell.wrapping_offset(cellno as isize)),
            (*cell.wrapping_offset(cellno as isize)).xelem,
            element,
            ind.as_mut_ptr(),
        );
        sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[RIGHT as usize];
        if sideno != 0 {
            *elemind.wrapping_offset(1) = GetElementIndices(
                &mut (*cell.wrapping_offset(sideno as isize)),
                1,
                element,
                ind.as_mut_ptr(),
            );
        } else {
            *elemind.wrapping_offset(1) = 0;
        }
    } else if side == DOWN {
        if element == (*cell.wrapping_offset(cellno as isize)).xelem {
            more = FALSE;
        }
        *elemind.wrapping_offset(0) = GetElementIndices(
            &mut (*cell.wrapping_offset(cellno as isize)),
            element,
            1,
            ind.as_mut_ptr(),
        );
        sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[DOWN as usize];
        if sideno != 0 {
            *elemind.wrapping_offset(1) = GetElementIndices(
                &mut (*cell.wrapping_offset(sideno as isize)),
                element,
                (*cell.wrapping_offset(sideno as isize)).yelem,
                ind.as_mut_ptr(),
            );
        } else {
            *elemind.wrapping_offset(1) = 0;
        }
    } else if side == UP {
        if element == (*cell.wrapping_offset(cellno as isize)).xelem {
            more = FALSE;
        }
        *elemind.wrapping_offset(0) = GetElementIndices(
            &mut (*cell.wrapping_offset(cellno as isize)),
            element,
            (*cell.wrapping_offset(cellno as isize)).yelem,
            ind.as_mut_ptr(),
        );
        sideno = (*cell.wrapping_offset(cellno as isize)).neighbour[UP as usize];
        if sideno != 0 {
            *elemind.wrapping_offset(1) = GetElementIndices(
                &mut (*cell.wrapping_offset(sideno as isize)),
                element,
                1,
                ind.as_mut_ptr(),
            );
        } else {
            *elemind.wrapping_offset(1) = 0;
        }
    } else {
        printf(b"Impossible option in GetSideInfo.\n\0".as_ptr() as *const c_char);
    }
    more
}
// SAFETY: z-division sizing over a valid grid.
#[no_mangle]
pub unsafe extern "C" fn SetElementDivisionExtruded(grid: *mut GridType, info: c_int) {
    let mut i: c_int;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut nzmax: c_int = 0;
    let mut sumzelems: c_int;
    let mut ratio: Real;
    let linearlimit: Real;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dzmax: Real = 0.0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dz: Real = 0.0;
    linearlimit = 0.001;
    if (*grid).autoratio != 0 {
        i = 1;
        while i <= (*grid).zcells {
            (*grid).zelems[i as usize] = (*grid).minzelems;
            i += 1;
        }
    } else {
        i = 1;
        while i <= (*grid).zcells {
            if (*grid).zelems[i as usize] < (*grid).minzelems {
                (*grid).zelems[i as usize] = (*grid).minzelems;
            }
            i += 1;
        }
    }
    sumzelems = (*grid).zcells * (*grid).minzelems;
    if sumzelems > (*grid).totzelems {
        // NOTE: `#if 0` diagnostic omitted (runtime-dead as in C).
        (*grid).totzelems = sumzelems;
    }
    // NOTE: put the linearity flags.
    i = 1;
    while i <= (*grid).zcells {
        if fabs(1.0 - (*grid).zexpand[i as usize]) < linearlimit {
            (*grid).zlinear[i as usize] = TRUE;
        } else {
            (*grid).zlinear[i as usize] = FALSE;
        }
        i += 1;
    }
    if (*grid).autoratio != 0 {
        let mut active: c_int;
        loop {
            dzmax = 0.0;
            active = FALSE;
            i = 1;
            while i <= (*grid).zcells {
                if (*grid).zelems[i as usize] == 0 {
                    i += 1;
                    continue;
                }
                if (*grid).zlinear[i as usize] == TRUE {
                    dz = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                        / ((*grid).zelems[i as usize] + 1) as Real;
                } else if (*grid).zexpand[i as usize] > 0.0 {
                    ratio = pow(
                        (*grid).zexpand[i as usize],
                        1.0 / (1.0 * (*grid).zelems[i as usize] as Real),
                    );
                    dz = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize]) * (1.0 - ratio)
                        / (1.0 - pow(ratio, ((*grid).zelems[i as usize] + 1) as Real));
                    if ratio < 1.0 {
                        dz *= (*grid).zexpand[i as usize];
                    }
                } else if (*grid).zelems[i as usize] == 1 {
                    dz = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                        / ((*grid).zelems[i as usize] + 1) as Real;
                } else if ((*grid).zelems[i as usize] + 1) % 2 == 0 {
                    ratio = pow(
                        -(*grid).zexpand[i as usize],
                        1.0 / (((*grid).zelems[i as usize] + 1) / 2 - 1) as Real,
                    );
                    dz =
                        0.5 * ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize]) * (1.0 - ratio)
                            / (1.0 - pow(ratio, (((*grid).zelems[i as usize] + 1) / 2) as Real));
                } else if ((*grid).zelems[i as usize] + 1) % 2 == 1 {
                    ratio = pow(
                        -(*grid).zexpand[i as usize],
                        1.0 / (((*grid).zelems[i as usize] + 1) / 2) as Real,
                    );
                    dz = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                        / (2.0
                            * (1.0 - pow(ratio, (((*grid).zelems[i as usize] + 1) / 2) as Real))
                            / (1.0 - ratio)
                            + pow(
                                ratio,
                                (((*grid).zelems[i as usize] + 1) / 2 + 1) as Real - 0.5,
                            ));
                }
                dz *= (*grid).zdens[i as usize] / (*grid).xzratio;
                if dz > dzmax {
                    dzmax = dz;
                    nzmax = i;
                }
                if (*grid).autoratio != 0
                    && fabs(dz - (*grid).dx0)
                        < fabs(
                            dz * (1.0 / (1.0 - 1.0 / (*grid).zelems[i as usize] as Real))
                                - (*grid).dx0,
                        )
                {
                    (*grid).zelems[i as usize] += 1;
                    sumzelems += 1;
                    active = TRUE;
                }
                i += 1;
            }
            if (*grid).autoratio != 0 {
                if active == 0 {
                    break;
                }
            } else {
                (*grid).zelems[nzmax as usize] += 1;
                sumzelems += 1;
                if sumzelems >= (*grid).totzelems {
                    break;
                }
            }
        }
    }
    // NOTE: set the size of the first element within each subcell.
    (*grid).totzelems = 0;
    i = 1;
    while i <= (*grid).zcells {
        (*grid).totzelems += (*grid).zelems[i as usize];
        if (*grid).zlinear[i as usize] == TRUE || (*grid).zelems[i as usize] == 1 {
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (*grid).zelems[i as usize] as Real;
        } else if (*grid).zexpand[i as usize] > 0.0 {
            ratio = pow(
                (*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] as Real - 1.0),
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                * (1.0 - ratio)
                / (1.0 - pow(ratio, (*grid).zelems[i as usize] as Real));
        } else if (*grid).zelems[i as usize] == 2 {
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (*grid).zelems[i as usize] as Real;
        } else if (*grid).zelems[i as usize] % 2 == 0 {
            ratio = pow(
                -(*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] / 2 - 1) as Real,
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] =
                0.5 * ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize]) * (1.0 - ratio)
                    / (1.0 - pow(ratio, ((*grid).zelems[i as usize] / 2) as Real));
        } else if (*grid).zelems[i as usize] % 2 == 1 {
            ratio = pow(
                -(*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] / 2) as Real,
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (2.0 * (1.0 - pow(ratio, ((*grid).zelems[i as usize] / 2) as Real))
                    / (1.0 - ratio)
                    + pow(ratio, (((*grid).zelems[i as usize] / 2 + 1) as Real) - 0.5));
        }
        i += 1;
    }
    if info != 0 {
        printf(
            b"Created %d extruded divisions.\n\0".as_ptr() as *const c_char,
            (*grid).totzelems,
        );
    }
    (*grid).dz0 = dzmax;
}
// SAFETY: rotation-direction division sizing over a valid grid.
#[no_mangle]
pub unsafe extern "C" fn SetElementDivisionCylinder(grid: *mut GridType, info: c_int) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let k: c_int;
    let mut ratio: Real;
    let eps: Real;
    let mut dzmax: Real;
    eps = 1.0e-8;
    (*grid).zcells = 2 * (*grid).rotateblocks;
    (*grid).z[0] = 0.0;
    i = 0;
    // NOTE: C `x[i]<eps` (NaN stops, unlike a negated `>=`).
    while (*grid).x[i as usize] < eps {
        i += 1;
    }
    k = i;
    (*grid).rotateradius1 = (*grid).x[k as usize];
    if (*grid).rotateradius2 < 0.0 {
        (*grid).rotatecartesian = TRUE;
        (*grid).rotateradius2 = 1.0e6;
    }
    if (*grid).rotateradius2 <= sqrt(2.0) * (*grid).rotateradius1 {
        if k + 1 <= (*grid).xcells {
            (*grid).rotateradius2 = (*grid).x[(k + 1) as usize];
        }
        // NOTE: C `MAX(a,b)` macro (egdef.h:97); both arms pure.
        if sqrt(2.0) * (*grid).rotateradius1 > (*grid).rotateradius2 {
            (*grid).rotateradius2 = sqrt(2.0) * (*grid).rotateradius1;
        }
    }
    if (*grid).xlinear[k as usize] == 0 {
        printf(
            b"SetElementDivisionCylinder: The division must be linear!\n\0".as_ptr()
                as *const c_char,
        );
    }
    i = 1;
    while i <= (*grid).zcells {
        (*grid).z[i as usize] = i as Real * (2.0 * FM_PI) / 8.0;
        (*grid).zelems[i as usize] = (*grid).xelems[k as usize];
        (*grid).zlinear[i as usize] = (*grid).xlinear[k as usize];
        (*grid).zratios[i as usize] = (*grid).xratios[k as usize];
        (*grid).zexpand[i as usize] = (*grid).xexpand[k as usize];
        (*grid).zmaterial[i as usize] = 0;
        i += 1;
    }
    (*grid).totzelems = (*grid).zcells * (*grid).xelems[k as usize];
    dzmax = 0.0;
    i = 1;
    while i <= (*grid).zcells {
        if (*grid).zlinear[i as usize] == TRUE || (*grid).zelems[i as usize] == 1 {
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (*grid).zelems[i as usize] as Real;
        } else if (*grid).zexpand[i as usize] > 0.0 {
            ratio = pow(
                (*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] as Real - 1.0),
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                * (1.0 - ratio)
                / (1.0 - pow(ratio, (*grid).zelems[i as usize] as Real));
        } else if (*grid).zelems[i as usize] == 2 {
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (*grid).zelems[i as usize] as Real;
        } else if (*grid).zelems[i as usize] % 2 == 0 {
            ratio = pow(
                -(*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] / 2 - 1) as Real,
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] =
                0.5 * ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize]) * (1.0 - ratio)
                    / (1.0 - pow(ratio, ((*grid).zelems[i as usize] / 2) as Real));
        } else if (*grid).zelems[i as usize] % 2 == 1 {
            ratio = pow(
                -(*grid).zexpand[i as usize],
                1.0 / ((*grid).zelems[i as usize] / 2) as Real,
            );
            (*grid).zratios[i as usize] = ratio;
            (*grid).dz[i as usize] = ((*grid).z[i as usize] - (*grid).z[(i - 1) as usize])
                / (2.0 * (1.0 - pow(ratio, ((*grid).zelems[i as usize] / 2) as Real))
                    / (1.0 - ratio)
                    + pow(ratio, (((*grid).zelems[i as usize] / 2) as Real) + 0.5));
        }
        if dzmax < (*grid).dz[i as usize] {
            dzmax = (*grid).dz[i as usize];
        }
        i += 1;
    }
    if info != 0 {
        printf(
            b"Created %d divisions in %d cells for rotation [%.2lg  %.2lg].\n\0".as_ptr()
                as *const c_char,
            (*grid).totzelems,
            (*grid).zcells,
            (*grid).rotateradius1,
            (*grid).rotateradius2,
        );
    }
    (*grid).dz0 = dzmax;
}
// SAFETY: command/argument line reader; `goto newline` is continue.
// USE_MATC=0: the no-MATC #else arms are live, substitution compiled out.
unsafe fn GetCommand(line1: *mut c_char, line2: *mut c_char, io: *mut FILE) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut isend: c_int;
    let mut line0: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: charend is read (NULL checks); the matcpntr pair lives only
    // in the compiled-out MATC block and is dropped.
    let mut charend: *mut c_char;
    let mut gotlinefeed: c_int;
    loop {
        i = 0;
        while i < MAXLINESIZE as c_int {
            *line2.wrapping_offset(i as isize) = 0x00;
            *line1.wrapping_offset(i as isize) = 0x00;
            line0[i as usize] = 0x00;
            i += 1;
        }
        charend = fgets(line0.as_mut_ptr(), MAXLINESIZE as c_int, io);
        isend = (charend.is_null()) as c_int;
        if isend != 0 {
            return 1;
        }
        if strlen(line0.as_ptr()) <= strspn(line0.as_ptr(), b" \t\r\n\0".as_ptr() as *const c_char)
        {
            continue;
        }
        if line0[0] == '#' as c_char
            || line0[0] == '%' as c_char
            || line0[0] == '!' as c_char
            || line0[0] == '\n' as c_char
        {
            continue;
        }
        if matcactive == 0 && line0[0] == '*' as c_char {
            continue;
        }
        if matcactive == 0 && !strchr(line0.as_ptr(), '$' as c_int).is_null() {
            // NOTE: USE_MATC=0 arm (the #else branch in C).
            printf(
                b"\n a $ found and MATC has not been compiled into ElmerGrid,\n\0".as_ptr()
                    as *const c_char,
            );
            printf(
                b"either remove the $ or compile ElmerGrid with MATC\n\0".as_ptr() as *const c_char,
            );
            bigerror(
                b"Error: $ found in command and MATC is not active\0".as_ptr() as *const c_char,
            );
        }
        // NOTE: USE_MATC substitution block compiled out exactly as in C.
        gotlinefeed = FALSE;
        j = 0;
        i = 0;
        while i < MAXLINESIZE as c_int {
            if line0[i as usize] == '\n' as c_char || line0[i as usize] == 0x00 {
                gotlinefeed = TRUE;
                break;
            }
            if line0[i as usize] == '=' as c_char {
                j = i;
                break;
            }
            *line1.wrapping_offset(i as isize) = toupper(line0[i as usize] as c_int) as c_char;
            i += 1;
        }
        // NOTE: after these commands there will be no nextline even though
        // there is no equality sign.
        if !strstr(line1, b"END\0".as_ptr() as *const c_char).is_null() {
            return 0;
        }
        if !strstr(line1, b"NEW MESH\0".as_ptr() as *const c_char).is_null() {
            return 0;
        }
        if j != 0 {
            // NOTE: arguments are actually on the same line after '='.
            i = j + 1;
            while i < MAXLINESIZE as c_int {
                *line2.wrapping_offset((i - j - 1) as isize) = line0[i as usize];
                if line0[i as usize] == '\n' as c_char || line0[i as usize] == 0x00 {
                    gotlinefeed = TRUE;
                    break;
                }
                i += 1;
            }
            if gotlinefeed == 0 {
                printf(
                    b"There is a risk that somethings was missed in line:\n\0".as_ptr()
                        as *const c_char,
                );
                printf(b"%s\n\0".as_ptr() as *const c_char, line0.as_ptr());
                smallerror(b"Check your output line length!\n\0".as_ptr() as *const c_char);
            }
        } else {
            // NOTE: arguments are on the next line.
            loop {
                charend = fgets(line2, MAXLINESIZE as c_int, io);
                isend = (charend.is_null()) as c_int;
                if isend != 0 {
                    return 2;
                }
                if *line2.wrapping_offset(0) == '#' as c_char
                    || *line2.wrapping_offset(0) == '%' as c_char
                    || *line2.wrapping_offset(0) == '!' as c_char
                {
                    continue;
                }
                if matcactive == 0 && *line2.wrapping_offset(0) == '*' as c_char {
                    continue;
                }
                break;
            }
            gotlinefeed = FALSE;
            i = 0;
            while i < MAXLINESIZE as c_int {
                if *line2.wrapping_offset(i as isize) == '\n' as c_char
                    || *line2.wrapping_offset(i as isize) == 0x00
                {
                    gotlinefeed = TRUE;
                    break;
                }
                i += 1;
            }
            if gotlinefeed == 0 {
                printf(
                    b"There is a risk that somethings was missed in line:\n\0".as_ptr()
                        as *const c_char,
                );
                printf(b"%s\n\0".as_ptr() as *const c_char, line2);
                smallerror(b"Check your output line length!\n\0".as_ptr() as *const c_char);
            }
            // NOTE: C tests line0 here (not line2); mirrored exactly.
            if matcactive == 0 && !strchr(line0.as_ptr(), '$' as c_int).is_null() {
                // NOTE: USE_MATC=0 arm (the #else branch in C).
                printf(
                    b"\n a $ found and MATC has not been compiled into ElmerGrid,\n\0".as_ptr()
                        as *const c_char,
                );
                printf(
                    b"either remove the $ or compile ElmerGrid with MATC\n\0".as_ptr()
                        as *const c_char,
                );
                bigerror(
                    b"Error: $ found in command and MATC is not active\0".as_ptr() as *const c_char,
                );
            }
            // NOTE: USE_MATC substitution block compiled out exactly as in C.
        }
        if iodebug != 0 {
            printf(b"command: %s\n\0".as_ptr() as *const c_char, line1);
            printf(b"params:  %s\n\0".as_ptr() as *const c_char, line2);
        }
        return 0;
    }
}
// SAFETY: .grd writer over valid grids; return codes as in C.
#[no_mangle]
pub unsafe extern "C" fn SaveElmergrid(
    grid: *mut GridType,
    nogrids: c_int,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut sameline: c_int;
    let maxsameline: c_int;
    let mut i: c_int;
    let mut j: c_int;
    let mut dim: c_int;
    let out: *mut FILE;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"grd\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    dim = (*grid).dimension;
    if (*grid).coordsystem == COORD_CART1 {
        dim = 1;
    }
    // NOTE: C also zeroes `j` here, but the mappings loop (or the
    // rotate-block `j = 0` below) overwrites it before any read.
    sameline = TRUE;
    maxsameline = 6;
    if (*grid).xcells > maxsameline {
        sameline = FALSE;
    }
    if dim >= 2 && (*grid).ycells > maxsameline {
        sameline = FALSE;
    }
    if dim >= 3 && (*grid).zcells > maxsameline {
        sameline = FALSE;
    }
    fprintf(
        out,
        b"#####  ElmerGrid input file for structured grid generation  ######\n\0".as_ptr()
            as *const c_char,
    );
    fprintf(out, b"Version = 210903\n\0".as_ptr() as *const c_char);
    fprintf(out, b"Coordinate System = \0".as_ptr() as *const c_char);
    if (*grid).coordsystem == COORD_AXIS {
        fprintf(out, b"2D Axisymmetric\n\0".as_ptr() as *const c_char);
    } else if (*grid).coordsystem == COORD_POLAR {
        fprintf(out, b"2D Polar\n\0".as_ptr() as *const c_char);
    } else {
        fprintf(out, b"Cartesian %dD\n\0".as_ptr() as *const c_char, dim);
    }
    fprintf(
        out,
        b"Subcell Divisions in %dD = \0".as_ptr() as *const c_char,
        dim,
    );
    if dim >= 1 {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, (*grid).xcells);
    }
    if dim >= 2 {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, (*grid).ycells);
    }
    if dim >= 3 {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, (*grid).zcells);
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    // NOTE: C `sameline ? "= " : "\n  "` (same pattern throughout).
    if sameline != 0 {
        fprintf(out, b"Subcell Limits 1 = \0".as_ptr() as *const c_char);
    } else {
        fprintf(out, b"Subcell Limits 1 \n  \0".as_ptr() as *const c_char);
    }
    i = 0;
    while i <= (*grid).xcells {
        fprintf(
            out,
            b"%.5lg \0".as_ptr() as *const c_char,
            (*grid).x[i as usize],
        );
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    if dim >= 2 {
        if sameline != 0 {
            fprintf(out, b"Subcell Limits 2 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Subcell Limits 2 \n  \0".as_ptr() as *const c_char);
        }
        i = 0;
        while i <= (*grid).ycells {
            fprintf(
                out,
                b"%.5lg \0".as_ptr() as *const c_char,
                (*grid).y[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    if dim >= 3 {
        if sameline != 0 {
            fprintf(out, b"Subcell Limits 3 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Subcell Limits 3 \n  \0".as_ptr() as *const c_char);
        }
        i = 0;
        while i <= (*grid).zcells {
            fprintf(
                out,
                b"%.5lg \0".as_ptr() as *const c_char,
                (*grid).z[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    if dim == 1 {
        fprintf(
            out,
            b"Material Structure in %dD\n\0".as_ptr() as *const c_char,
            1,
        );
    } else {
        fprintf(
            out,
            b"Material Structure in %dD\n\0".as_ptr() as *const c_char,
            2,
        );
    }
    j = (*grid).ycells;
    while j >= 1 {
        fprintf(out, b"  \0".as_ptr() as *const c_char);
        i = 1;
        while i <= (*grid).xcells {
            fprintf(
                out,
                b"%-5d\0".as_ptr() as *const c_char,
                (*grid).structure[j as usize][i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        j -= 1;
    }
    fprintf(out, b"End\n\0".as_ptr() as *const c_char);
    if (*grid).mappings > 0 {
        fprintf(out, b"Geometry Mappings\n\0".as_ptr() as *const c_char);
        fprintf(
            out,
            b"# mode  line  limits(2)   Np  params(Np)\n\0".as_ptr() as *const c_char,
        );
        i = 0;
        while i < (*grid).mappings {
            fprintf(
                out,
                b"  %-5d %-5d %-7.5lg %-7.5lg %-3d \0".as_ptr() as *const c_char,
                (*grid).mappingtype[i as usize],
                (*grid).mappingline[i as usize],
                (*grid).mappinglimits[(2 * i) as usize],
                (*grid).mappinglimits[(2 * i + 1) as usize],
                (*grid).mappingpoints[i as usize],
            );
            j = 0;
            while j < (*grid).mappingpoints[i as usize] {
                fprintf(
                    out,
                    b"%.4lg \0".as_ptr() as *const c_char,
                    *(*grid).mappingparams[i as usize].wrapping_offset(j as isize),
                );
                j += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
            i += 1;
        }
        fprintf(out, b"End\n\0".as_ptr() as *const c_char);
    }
    j = 0;
    if (*grid.wrapping_offset(j as isize)).rotate != 0 {
        fprintf(
            out,
            b"Revolve Blocks = %d\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).rotateblocks,
        );
        fprintf(
            out,
            b"Revolve Radius = %-8.3lg\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).rotateradius2,
        );
        if fabs((*grid.wrapping_offset(j as isize)).rotateimprove - 1.0) > 1.0e-10 {
            fprintf(
                out,
                b"Revolve Improve = %-8.3lg\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).rotateimprove,
            );
        }
    }
    if (*grid.wrapping_offset(j as isize)).rotatecurve != 0 {
        fprintf(
            out,
            b"Revolve Curve Direct = %-8.3lg\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).curvezet,
        );
        fprintf(
            out,
            b"Revolve Curve Radius = %-8.3lg\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).curverad,
        );
        fprintf(
            out,
            b"Revolve Curve Angle = %-8.3lg\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).curveangle,
        );
    }
    if (*grid.wrapping_offset(j as isize)).coordsystem == COORD_POLAR {
        fprintf(
            out,
            b"Polar Radius = %.3lg\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).polarradius,
        );
    }
    save_grids(out, grid, nogrids, dim, sameline);
    if info != 0 {
        printf(
            b"The Elmergrid input was saved to file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    fclose(out);
    0
}

// SAFETY: per-grid section of the .grd writer (split for patch sizing only).
unsafe fn save_grids(
    out: *mut FILE,
    grid: *mut GridType,
    nogrids: c_int,
    dim: c_int,
    sameline: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    j = 0;
    while j < nogrids {
        if j > 0 {
            fprintf(out, b"\nStart New Mesh\n\0".as_ptr() as *const c_char);
        }
        fprintf(
            out,
            b"Materials Interval = %d %d\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).firstmaterial,
            (*grid.wrapping_offset(j as isize)).lastmaterial,
        );
        if dim == 3 {
            fprintf(out, b"Extruded Structure\n\0".as_ptr() as *const c_char);
            fprintf(
                out,
                b"# %-8s %-8s %-8s\n\0".as_ptr() as *const c_char,
                b"1stmat\0".as_ptr() as *const c_char,
                b"lastmat\0".as_ptr() as *const c_char,
                b"newmat\0".as_ptr() as *const c_char,
            );
            i = 1;
            while i <= (*grid.wrapping_offset(j as isize)).zcells {
                fprintf(
                    out,
                    b"  %-8d %-8d %-8d\n\0".as_ptr() as *const c_char,
                    (*grid.wrapping_offset(j as isize)).zfirstmaterial[i as usize],
                    (*grid.wrapping_offset(j as isize)).zlastmaterial[i as usize],
                    (*grid.wrapping_offset(j as isize)).zmaterial[i as usize],
                );
                i += 1;
            }
            fprintf(out, b"End\n\0".as_ptr() as *const c_char);
        }
        if (*grid.wrapping_offset(j as isize)).noboundaries > 0 {
            fprintf(out, b"Boundary Definitions\n\0".as_ptr() as *const c_char);
            fprintf(
                out,
                b"# %-8s %-8s %-8s\n\0".as_ptr() as *const c_char,
                b"type\0".as_ptr() as *const c_char,
                b"out\0".as_ptr() as *const c_char,
                b"int\0".as_ptr() as *const c_char,
            );
            i = 0;
            while i < (*grid.wrapping_offset(j as isize)).noboundaries {
                fprintf(
                    out,
                    b"  %-8d %-8d %-8d %-8d\n\0".as_ptr() as *const c_char,
                    (*grid.wrapping_offset(j as isize)).boundtype[i as usize],
                    (*grid.wrapping_offset(j as isize)).boundext[i as usize],
                    (*grid.wrapping_offset(j as isize)).boundint[i as usize],
                    (*grid.wrapping_offset(j as isize)).boundsolid[i as usize],
                );
                i += 1;
            }
            fprintf(out, b"End\n\0".as_ptr() as *const c_char);
        }
        // NOTE: C reads grid[0] here (`grid->numbering`), not grid[j].
        if (*grid).numbering == NUMBER_XY {
            fprintf(out, b"Numbering = Horizontal\n\0".as_ptr() as *const c_char);
        }
        if (*grid).numbering == NUMBER_YX {
            fprintf(out, b"Numbering = Vertical\n\0".as_ptr() as *const c_char);
        }
        fprintf(
            out,
            b"Element Degree = %d\n\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).elemorder,
        );
        if (*grid.wrapping_offset(j as isize)).elemmidpoints != 0 {
            fprintf(
                out,
                b"Element Innernodes = %s\n\0".as_ptr() as *const c_char,
                b"True\0".as_ptr() as *const c_char,
            );
        } else {
            fprintf(
                out,
                b"Element Innernodes = %s\n\0".as_ptr() as *const c_char,
                b"False\0".as_ptr() as *const c_char,
            );
        }
        if (*grid.wrapping_offset(j as isize)).triangles != 0 {
            fprintf(
                out,
                b"Triangles = %s\n\0".as_ptr() as *const c_char,
                b"True\0".as_ptr() as *const c_char,
            );
        } else {
            fprintf(
                out,
                b"Triangles = %s\n\0".as_ptr() as *const c_char,
                b"False\0".as_ptr() as *const c_char,
            );
        }
        if (*grid.wrapping_offset(j as isize)).autoratio != 0 {
            fprintf(
                out,
                b"Surface Elements = %d\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).wantedelems,
            );
        }
        if dim == 3 && (*grid.wrapping_offset(j as isize)).wantedelems3d != 0 {
            fprintf(
                out,
                b"Volume Elements = %d\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).wantedelems3d,
            );
        }
        if dim == 3 && (*grid.wrapping_offset(j as isize)).wantednodes3d != 0 {
            fprintf(
                out,
                b"Volume Nodes = %d\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).wantednodes3d,
            );
        }
        if dim == 2 {
            fprintf(
                out,
                b"Coordinate Ratios = %-8.3lg\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).xyratio,
            );
        }
        if dim == 3 {
            fprintf(
                out,
                b"Coordinate Ratios = %-8.3lg %-8.3lg\n\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).xyratio,
                (*grid.wrapping_offset(j as isize)).xzratio,
            );
        }
        fprintf(
            out,
            b"Minimum Element Divisions = %d\0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).minxelems,
        );
        if dim >= 2 {
            fprintf(
                out,
                b" %d\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).minyelems,
            );
        }
        if dim >= 3 {
            fprintf(
                out,
                b" %d\0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).minzelems,
            );
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        save_divisions(out, grid, j, dim, sameline);
        j += 1;
    }
}

// SAFETY: ratios/densities/divisions section of the .grd writer.
unsafe fn save_divisions(
    out: *mut FILE,
    grid: *mut GridType,
    j: c_int,
    dim: c_int,
    sameline: c_int,
) {
    let mut i: c_int;
    if sameline != 0 {
        fprintf(out, b"Element Ratios 1 = \0".as_ptr() as *const c_char);
    } else {
        fprintf(out, b"Element Ratios 1 \n  \0".as_ptr() as *const c_char);
    }
    i = 1;
    while i <= (*grid.wrapping_offset(j as isize)).xcells {
        fprintf(
            out,
            b"%.3lg \0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).xexpand[i as usize],
        );
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    if dim >= 2 {
        if sameline != 0 {
            fprintf(out, b"Element Ratios 2 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Element Ratios 2 \n  \0".as_ptr() as *const c_char);
        }
        i = 1;
        while i <= (*grid.wrapping_offset(j as isize)).ycells {
            fprintf(
                out,
                b"%.3lg \0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).yexpand[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    if dim >= 3 {
        if sameline != 0 {
            fprintf(out, b"Element Ratios 3 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Element Ratios 3 \n  \0".as_ptr() as *const c_char);
        }
        i = 1;
        while i <= (*grid.wrapping_offset(j as isize)).zcells {
            fprintf(
                out,
                b"%.3lg \0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).zexpand[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    if (*grid.wrapping_offset(j as isize)).autoratio != 0 {
        save_densities(out, grid, j, dim, sameline);
    } else {
        if sameline != 0 {
            fprintf(out, b"Element Divisions 1 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Element Divisions 1 \n  \0".as_ptr() as *const c_char);
        }
        i = 1;
        while i <= (*grid.wrapping_offset(j as isize)).xcells {
            fprintf(
                out,
                b"%d \0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).xelems[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        if dim >= 2 {
            if sameline != 0 {
                fprintf(out, b"Element Divisions 2 = \0".as_ptr() as *const c_char);
            } else {
                fprintf(out, b"Element Divisions 2 \n  \0".as_ptr() as *const c_char);
            }
            i = 1;
            while i <= (*grid.wrapping_offset(j as isize)).ycells {
                fprintf(
                    out,
                    b"%d \0".as_ptr() as *const c_char,
                    (*grid.wrapping_offset(j as isize)).yelems[i as usize],
                );
                i += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
        }
        if dim >= 3 {
            if sameline != 0 {
                fprintf(out, b"Element Divisions 3 = \0".as_ptr() as *const c_char);
            } else {
                fprintf(out, b"Element Divisions 3 \n  \0".as_ptr() as *const c_char);
            }
            i = 1;
            while i <= (*grid.wrapping_offset(j as isize)).zcells {
                fprintf(
                    out,
                    b"%d \0".as_ptr() as *const c_char,
                    (*grid.wrapping_offset(j as isize)).zelems[i as usize],
                );
                i += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
        }
    }
}

// SAFETY: densities section of the .grd writer.
unsafe fn save_densities(
    out: *mut FILE,
    grid: *mut GridType,
    j: c_int,
    dim: c_int,
    sameline: c_int,
) {
    let mut i: c_int;
    if sameline != 0 {
        fprintf(out, b"Element Densities 1 = \0".as_ptr() as *const c_char);
    } else {
        fprintf(out, b"Element Densities 1 \n  \0".as_ptr() as *const c_char);
    }
    i = 1;
    while i <= (*grid.wrapping_offset(j as isize)).xcells {
        fprintf(
            out,
            b"%.3lg \0".as_ptr() as *const c_char,
            (*grid.wrapping_offset(j as isize)).xdens[i as usize],
        );
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    if dim >= 2 {
        if sameline != 0 {
            fprintf(out, b"Element Densities 2 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Element Densities 2 \n  \0".as_ptr() as *const c_char);
        }
        i = 1;
        while i <= (*grid.wrapping_offset(j as isize)).ycells {
            fprintf(
                out,
                b"%.3lg \0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).ydens[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    if dim >= 3 {
        if sameline != 0 {
            fprintf(out, b"Element Densities 3 = \0".as_ptr() as *const c_char);
        } else {
            fprintf(out, b"Element Densities 3 \n  \0".as_ptr() as *const c_char);
        }
        i = 1;
        while i <= (*grid.wrapping_offset(j as isize)).zcells {
            fprintf(
                out,
                b"%.3lg \0".as_ptr() as *const c_char,
                (*grid.wrapping_offset(j as isize)).zdens[i as usize],
            );
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
}
// SAFETY: 2D element-division optimizer over a valid grid.
#[no_mangle]
pub unsafe extern "C" fn SetElementDivision(grid: *mut GridType, relh: Real, _info: c_int) {
    // NOTE: C never reads `info` here either.
    let mut i: c_int;
    let mut j: c_int;
    let nx: c_int;
    let ny: c_int;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut nxmax: c_int = 0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut nymax: c_int = 0;
    let mut sumxelems: c_int;
    let mut sumyelems: c_int;
    // NOTE: 0-init for the &mut handoff (set inside setdiv_joint before reads).
    let mut sumxyelems: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set inside setdiv_joint before reads).
    let mut wantedelems: c_int = 0;
    let mut wantedelemsx: c_int;
    let mut wantedelemsy: c_int;
    // NOTE: 0-init for the &mut handoff (set before reads on every path).
    let mut ratio: Real = 0.0;
    let linearlimit: Real;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dxmax: Real = 0.0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dymax: Real = 0.0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dx: Real = 0.0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dy: Real = 0.0;
    // NOTE: assigned before every read (C `= 0` decl init shape).
    let mut dxlimit: Real = 0.0;
    // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    if (*grid).dimension == 1 {
        (*grid).numbering = NUMBER_1D;
    }
    nx = (*grid).xcells;
    ny = (*grid).ycells;
    linearlimit = 0.001;
    // NOTE: number the cells from left to right and up to down.
    (*grid).nocells = 0;
    j = 1;
    while j <= ny {
        i = 1;
        while i <= nx {
            if (*grid).structure[j as usize][i as usize] >= (*grid).firstmaterial
                && (*grid).structure[j as usize][i as usize] <= (*grid).lastmaterial
            {
                (*grid).nocells += 1;
                (*grid).numbered[j as usize][i as usize] = (*grid).nocells;
            }
            i += 1;
        }
        j += 1;
    }
    // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    // NOTE: put the linearity flags.
    i = 1;
    while i <= nx {
        if fabs(1.0 - (*grid).xexpand[i as usize]) < linearlimit {
            (*grid).xlinear[i as usize] = TRUE;
        } else if fabs(1.0 + (*grid).xexpand[i as usize]) < linearlimit {
            (*grid).xlinear[i as usize] = TRUE;
        } else {
            (*grid).xlinear[i as usize] = FALSE;
        }
        i += 1;
    }
    i = 1;
    while i <= ny {
        if fabs(1.0 - (*grid).yexpand[i as usize]) < linearlimit {
            (*grid).ylinear[i as usize] = TRUE;
        } else if fabs(1.0 + (*grid).yexpand[i as usize]) < linearlimit {
            (*grid).ylinear[i as usize] = TRUE;
        } else {
            (*grid).ylinear[i as usize] = FALSE;
        }
        i += 1;
    }
    // NOTE: if there are no materials no elements either; otherwise at
    // least minelems elements (see C comment on numbering choice).
    if (*grid).autoratio != 0 {
        i = 1;
        while i <= nx {
            (*grid).xelems[i as usize] = (*grid).minxelems;
            i += 1;
        }
        i = 1;
        while i <= ny {
            (*grid).yelems[i as usize] = (*grid).minyelems;
            i += 1;
        }
    } else {
        i = 1;
        while i <= nx {
            if (*grid).xelems[i as usize] < (*grid).minxelems {
                (*grid).xelems[i as usize] = (*grid).minxelems;
            }
            i += 1;
        }
        i = 1;
        while i <= ny {
            if (*grid).yelems[i as usize] < (*grid).minyelems {
                (*grid).yelems[i as usize] = (*grid).minyelems;
            }
            i += 1;
        }
    }
    sumxelems = 0;
    i = 1;
    while i <= nx {
        if (*grid).dimension == 1 && (*grid).numbered[1][i as usize] == 0 {
            i += 1;
            continue;
        }
        sumxelems += (*grid).xelems[i as usize];
        i += 1;
    }
    sumyelems = 0;
    i = 1;
    while i <= ny {
        sumyelems += (*grid).yelems[i as usize];
        i += 1;
    }
    if (*grid).dimension == 1 {
        (*grid).autoratio = 2;
        (*grid).totxelems = (*grid).wantedelems;
        (*grid).totyelems = 1;
    }
    // NOTE: allocate elements for both axis separately.
    if (*grid).autoratio == 2 {
        wantedelemsx = (1.0 * (*grid).totxelems as Real / relh) as c_int;
        wantedelemsy = (1.0 * (*grid).totyelems as Real / relh) as c_int;
        if sumxelems > wantedelemsx {
            printf(
                b"SetElementDivision: %d is too few elements in x-direction\n\0".as_ptr()
                    as *const c_char,
                (*grid).totxelems,
            );
            wantedelemsx = sumxelems + 1;
        }
        if sumyelems > wantedelemsy {
            printf(
                b"SetElementDivision: %d is too few elements in y-direction\n\0".as_ptr()
                    as *const c_char,
                (*grid).totyelems,
            );
            wantedelemsy = sumyelems + 1;
        }
        while sumxelems < wantedelemsx {
            dxmax = 0.0;
            i = 1;
            while i <= nx {
                if (*grid).xelems[i as usize] == 0 {
                    i += 1;
                    continue;
                }
                // NOTE: don't put elements to passive subcells.
                if (*grid).dimension == 1 && (*grid).numbered[1][i as usize] == 0 {
                    i += 1;
                    continue;
                }
                if (*grid).xlinear[i as usize] == TRUE || (*grid).xelems[i as usize] == 1 {
                    dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (*grid).xelems[i as usize] as Real;
                } else if (*grid).xexpand[i as usize] > 0.0 {
                    ratio = pow(
                        (*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] as Real - 1.0),
                    );
                    dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize]) * (1.0 - ratio)
                        / (1.0 - pow(ratio, (*grid).xelems[i as usize] as Real));
                    if ratio < 1.0 {
                        dx *= (*grid).xexpand[i as usize];
                    }
                } else if (*grid).xelems[i as usize] == 2 {
                    dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (*grid).xelems[i as usize] as Real;
                } else if (*grid).xelems[i as usize] % 2 == 0 {
                    ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] / 2 - 1) as Real,
                    );
                    dx =
                        0.5 * ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize]) * (1.0 - ratio)
                            / (1.0 - pow(ratio, ((*grid).xelems[i as usize] / 2) as Real));
                } else if (*grid).xelems[i as usize] % 2 == 1 {
                    ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] / 2) as Real,
                    );
                    dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (2.0 * (1.0 - pow(ratio, ((*grid).xelems[i as usize] / 2) as Real))
                            / (1.0 - ratio)
                            + pow(ratio, (((*grid).xelems[i as usize] / 2) as Real) + 0.5));
                }
                dx *= (*grid).xdens[i as usize];
                if dx > dxmax {
                    dxmax = dx;
                    nxmax = i;
                }
                i += 1;
            }
            (*grid).xelems[nxmax as usize] += 1;
            sumxelems += 1;
        }
        if (*grid).dimension > 1 {
            while sumyelems < wantedelemsy {
                dymax = 0.0;
                i = 1;
                while i <= ny {
                    if (*grid).yelems[i as usize] == 0 {
                        i += 1;
                        continue;
                    }
                    if (*grid).ylinear[i as usize] != 0 || (*grid).yelems[i as usize] == 1 {
                        dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                            / (*grid).yelems[i as usize] as Real;
                    } else {
                        ratio = pow(
                            (*grid).yexpand[i as usize],
                            1.0 / ((*grid).yelems[i as usize] as Real - 1.0),
                        );
                        dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize]) * (1.0 - ratio)
                            / (1.0 - pow(ratio, (*grid).yelems[i as usize] as Real));
                        if ratio < 1.0 {
                            dy *= (*grid).yexpand[i as usize];
                        }
                    }
                    dy *= (*grid).ydens[i as usize];
                    if dy > dymax {
                        dymax = dy;
                        nymax = i;
                    }
                    i += 1;
                }
                (*grid).yelems[nymax as usize] += 1;
                sumyelems += 1;
            }
        }
    }
    setdiv_joint(
        grid,
        relh,
        nx,
        ny,
        &mut nxmax,
        &mut nymax,
        &mut sumxelems,
        &mut sumyelems,
        &mut wantedelems,
        &mut sumxyelems,
        &mut dxmax,
        &mut dymax,
        &mut dx,
        &mut dy,
        &mut dxlimit,
        &mut ratio,
    );
}
// SAFETY: joint-axes + verify + final-sizing tail of SetElementDivision
// (split for patch sizing only; scratch locals re-declared here).
// NOTE: the `sumxelems/sumyelems += ...` tails are write-only in C as
// well (only sumxyelems feeds `noelements`); kept for C-shape fidelity.
#[allow(unused_assignments)]
unsafe fn setdiv_joint(
    grid: *mut GridType,
    relh: Real,
    nx: c_int,
    ny: c_int,
    nxmax: &mut c_int,
    nymax: &mut c_int,
    sumxelems: &mut c_int,
    sumyelems: &mut c_int,
    wantedelems: &mut c_int,
    sumxyelems: &mut c_int,
    dxmax: &mut Real,
    dymax: &mut Real,
    dx: &mut Real,
    dy: &mut Real,
    dxlimit: &mut Real,
    ratio: &mut Real,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: both axis dependently.
    if (*grid).autoratio == 1 {
        *wantedelems = (1.0 * (*grid).wantedelems as Real / (relh * relh)) as c_int;
        *sumxyelems = 0;
        loop {
            *dxmax = 0.0;
            i = 1;
            while i <= nx {
                if (*grid).xelems[i as usize] == 0 {
                    i += 1;
                    continue;
                }
                if (*grid).xlinear[i as usize] != 0 || (*grid).xelems[i as usize] == 1 {
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (*grid).xelems[i as usize] as Real;
                } else if (*grid).xexpand[i as usize] > 0.0 {
                    *ratio = pow(
                        (*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] as Real - 1.0),
                    );
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize]) * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, (*grid).xelems[i as usize] as Real));
                    if *ratio < 1.0 {
                        *dx *= (*grid).xexpand[i as usize];
                    }
                } else if (*grid).xelems[i as usize] == 2 {
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (*grid).xelems[i as usize] as Real;
                } else if (*grid).xelems[i as usize] % 2 == 0 {
                    *ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] / 2 - 1) as Real,
                    );
                    *dx = 0.5
                        * ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, ((*grid).xelems[i as usize] / 2) as Real));
                } else if (*grid).xelems[i as usize] % 2 == 1 {
                    *ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] / 2) as Real,
                    );
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (2.0 * (1.0 - pow(*ratio, ((*grid).xelems[i as usize] / 2) as Real))
                            / (1.0 - *ratio)
                            + pow(*ratio, (((*grid).xelems[i as usize] / 2) as Real) + 0.5));
                }
                *dx *= (*grid).xdens[i as usize];
                if *dx > *dxmax {
                    *dxmax = *dx;
                    *nxmax = i;
                }
                i += 1;
            }
            *dymax = 0.0;
            i = 1;
            while i <= ny {
                if (*grid).yelems[i as usize] == 0 {
                    i += 1;
                    continue;
                }
                if (*grid).ylinear[i as usize] != 0 || (*grid).yelems[i as usize] == 1 {
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / (*grid).yelems[i as usize] as Real;
                } else if (*grid).yexpand[i as usize] > 0.0 {
                    *ratio = pow(
                        (*grid).yexpand[i as usize],
                        1.0 / ((*grid).yelems[i as usize] as Real - 1.0),
                    );
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize]) * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, (*grid).yelems[i as usize] as Real));
                    if *ratio < 1.0 {
                        *dy *= (*grid).yexpand[i as usize];
                    }
                } else if (*grid).yelems[i as usize] == 2 {
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / (*grid).yelems[i as usize] as Real;
                } else if (*grid).yelems[i as usize] % 2 == 0 {
                    *ratio = pow(
                        -(*grid).yexpand[i as usize],
                        1.0 / ((*grid).yelems[i as usize] / 2 - 1) as Real,
                    );
                    *dy = 0.5
                        * ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, ((*grid).yelems[i as usize] / 2) as Real));
                } else if (*grid).yelems[i as usize] % 2 == 1 {
                    *ratio = pow(
                        -(*grid).yexpand[i as usize],
                        1.0 / ((*grid).yelems[i as usize] / 2) as Real,
                    );
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / (2.0 * (1.0 - pow(*ratio, ((*grid).yelems[i as usize] / 2) as Real))
                            / (1.0 - *ratio)
                            + pow(*ratio, (((*grid).yelems[i as usize] / 2) as Real) + 0.5));
                }
                *dy *= (*grid).ydens[i as usize];
                if *dy > *dymax {
                    *dymax = *dy;
                    *nymax = i;
                }
                i += 1;
            }
            *dymax /= (*grid).xyratio;
            if *dxmax > *dymax {
                (*grid).xelems[*nxmax as usize] += 1;
                *sumxelems += 1;
                *dxlimit = *dxmax;
            } else {
                (*grid).yelems[*nymax as usize] += 1;
                *sumyelems += 1;
                *dxlimit = *dymax;
            }
            *sumxyelems = 0;
            j = 1;
            while j <= (*grid).ycells {
                i = 1;
                while i <= (*grid).xcells {
                    if (*grid).numbered[j as usize][i as usize] != 0 {
                        *sumxyelems += (*grid).xelems[i as usize] * (*grid).yelems[j as usize];
                    }
                    i += 1;
                }
                j += 1;
            }
            if *wantedelems <= *sumxyelems {
                break;
            }
        }
    }
    setdiv_verify(
        grid, relh, nx, ny, sumxelems, sumyelems, sumxyelems, dx, dy, dxlimit, ratio,
    );
    // NOTE: put the linearity flags if there is only one division.
    i = 1;
    while i <= nx {
        if (*grid).xelems[i as usize] == 1 {
            (*grid).xlinear[i as usize] = TRUE;
        }
        i += 1;
    }
    i = 1;
    while i <= ny {
        if (*grid).yelems[i as usize] == 1 {
            (*grid).ylinear[i as usize] = TRUE;
        }
        i += 1;
    }
    // NOTE: set the size of the first element within each subcell.
    setdiv_dx(grid, nx, ratio);
    setdiv_dy(grid, ny, ratio);
    // NOTE: calculate the total number of elements.
    *sumxyelems = 0;
    j = 1;
    while j <= (*grid).ycells {
        i = 1;
        while i <= (*grid).xcells {
            if (*grid).numbered[j as usize][i as usize] != 0 {
                *sumxyelems += (*grid).xelems[i as usize] * (*grid).yelems[j as usize];
            }
            i += 1;
        }
        j += 1;
    }
    (*grid).noelements = *sumxyelems;
    // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    (*grid).dx0 = *dxmax;
    (*grid).dy0 = *dymax;
}
// SAFETY: density-verify pass of SetElementDivision (split for sizing).
#[allow(unused_assignments)]
unsafe fn setdiv_verify(
    grid: *mut GridType,
    relh: Real,
    nx: c_int,
    ny: c_int,
    sumxelems: &mut c_int,
    sumyelems: &mut c_int,
    sumxyelems: &mut c_int,
    dx: &mut Real,
    dy: &mut Real,
    dxlimit: &mut Real,
    ratio: &mut Real,
) {
    let mut i: c_int;
    let mut j: c_int;
    if (*grid).autoratio == 3 || (*grid).limitdxverify != 0 {
        if (*grid).autoratio == 3 {
            *dxlimit = relh * (*grid).limitdx;
            *dx = *dxlimit;
            *dy = *dxlimit;
        }
        i = 1;
        while i <= nx {
            loop {
                if (*grid).xlinear[i as usize] != 0 || (*grid).xelems[i as usize] == 0 {
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / ((*grid).xelems[i as usize] + 1) as Real;
                } else if (*grid).xexpand[i as usize] > 0.0 {
                    *ratio = pow(
                        (*grid).xexpand[i as usize],
                        1.0 / (*grid).xelems[i as usize] as Real,
                    );
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize]) * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, ((*grid).xelems[i as usize] + 1) as Real));
                    if *ratio < 1.0 {
                        *dx *= (*grid).xexpand[i as usize];
                    }
                } else if (*grid).xelems[i as usize] == 1 {
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / ((*grid).xelems[i as usize] + 1) as Real;
                } else if ((*grid).xelems[i as usize] + 1) % 2 == 0 {
                    *ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / ((*grid).xelems[i as usize] / 2) as Real,
                    );
                    *dx = 0.5
                        * ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, (((*grid).xelems[i as usize] / 2) + 1) as Real));
                } else if ((*grid).xelems[i as usize] + 1) % 2 == 1 {
                    *ratio = pow(
                        -(*grid).xexpand[i as usize],
                        1.0 / (((*grid).xelems[i as usize] + 1) / 2) as Real,
                    );
                    *dx = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                        / (2.0
                            * (1.0 - pow(*ratio, (((*grid).xelems[i as usize] + 1) / 2) as Real))
                            / (1.0 - *ratio)
                            + pow(
                                *ratio,
                                ((((*grid).xelems[i as usize] + 1) / 2) as Real) + 0.5,
                            ));
                }
                *dx *= (*grid).xdens[i as usize];
                // NOTE: choose the best fit for desired density.
                if fabs(*dx - *dxlimit)
                    < fabs(*dx * (1.0 + 1.0 / (*grid).xelems[i as usize] as Real) - *dxlimit)
                {
                    (*grid).xelems[i as usize] += 1;
                } else {
                    break;
                }
            }
            *sumxelems += (*grid).xelems[i as usize];
            i += 1;
        }
        i = 1;
        while i <= ny {
            loop {
                if (*grid).ylinear[i as usize] != 0 || (*grid).yelems[i as usize] == 0 {
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / ((*grid).yelems[i as usize] + 1) as Real;
                } else if (*grid).yexpand[i as usize] > 0.0 {
                    *ratio = pow(
                        (*grid).yexpand[i as usize],
                        1.0 / (*grid).yelems[i as usize] as Real,
                    );
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize]) * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, ((*grid).yelems[i as usize] + 1) as Real));
                    if *ratio < 1.0 {
                        *dy *= (*grid).yexpand[i as usize];
                    }
                } else if (*grid).yelems[i as usize] == 1 {
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / ((*grid).yelems[i as usize] + 1) as Real;
                } else if ((*grid).yelems[i as usize] + 1) % 2 == 0 {
                    *ratio = pow(
                        -(*grid).yexpand[i as usize],
                        1.0 / ((*grid).yelems[i as usize] / 2) as Real,
                    );
                    *dy = 0.5
                        * ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        * (1.0 - *ratio)
                        / (1.0 - pow(*ratio, (((*grid).yelems[i as usize] / 2) + 1) as Real));
                } else if ((*grid).yelems[i as usize] + 1) % 2 == 1 {
                    *ratio = pow(
                        -(*grid).yexpand[i as usize],
                        1.0 / (((*grid).yelems[i as usize] + 1) / 2) as Real,
                    );
                    *dy = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                        / (2.0
                            * (1.0 - pow(*ratio, (((*grid).yelems[i as usize] + 1) / 2) as Real))
                            / (1.0 - *ratio)
                            + pow(
                                *ratio,
                                ((((*grid).yelems[i as usize] + 1) / 2) as Real) + 0.5,
                            ));
                }
                *dy *= (*grid).ydens[i as usize] / (*grid).xyratio;
                // NOTE: choose the best fit for desired density.
                if fabs(*dy - *dxlimit)
                    < fabs(*dy * (1.0 + 1.0 / (*grid).yelems[i as usize] as Real) - *dxlimit)
                {
                    (*grid).yelems[i as usize] += 1;
                } else {
                    break;
                }
            }
            *sumyelems += (*grid).yelems[i as usize];
            i += 1;
        }
        *sumxyelems = 0;
        j = 1;
        while j <= (*grid).ycells {
            i = 1;
            while i <= (*grid).xcells {
                if (*grid).numbered[j as usize][i as usize] != 0 {
                    *sumxyelems += (*grid).xelems[i as usize] * (*grid).yelems[j as usize];
                }
                i += 1;
            }
            j += 1;
        }
    }
}

// SAFETY: first-element x-size computation of SetElementDivision.
unsafe fn setdiv_dx(grid: *mut GridType, nx: c_int, ratio: &mut Real) {
    let mut i: c_int;
    i = 1;
    while i <= nx {
        if (*grid).xlinear[i as usize] == TRUE {
            (*grid).dx[i as usize] = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                / (*grid).xelems[i as usize] as Real;
        } else if (*grid).xexpand[i as usize] > 0.0 {
            *ratio = pow(
                (*grid).xexpand[i as usize],
                1.0 / ((*grid).xelems[i as usize] as Real - 1.0),
            );
            (*grid).xratios[i as usize] = *ratio;
            (*grid).dx[i as usize] = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                * (1.0 - *ratio)
                / (1.0 - pow(*ratio, (*grid).xelems[i as usize] as Real));
        } else if (*grid).xelems[i as usize] == 2 {
            (*grid).dx[i as usize] = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                / (*grid).xelems[i as usize] as Real;
        } else if (*grid).xelems[i as usize] % 2 == 0 {
            *ratio = pow(
                -(*grid).xexpand[i as usize],
                1.0 / ((*grid).xelems[i as usize] / 2 - 1) as Real,
            );
            (*grid).xratios[i as usize] = *ratio;
            (*grid).dx[i as usize] =
                0.5 * ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize]) * (1.0 - *ratio)
                    / (1.0 - pow(*ratio, ((*grid).xelems[i as usize] / 2) as Real));
        } else if (*grid).xelems[i as usize] % 2 == 1 {
            *ratio = pow(
                -(*grid).xexpand[i as usize],
                1.0 / ((*grid).xelems[i as usize] / 2) as Real,
            );
            (*grid).xratios[i as usize] = *ratio;
            (*grid).dx[i as usize] = ((*grid).x[i as usize] - (*grid).x[(i - 1) as usize])
                / (2.0 * (1.0 - pow(*ratio, ((*grid).xelems[i as usize] / 2) as Real))
                    / (1.0 - *ratio)
                    + pow(*ratio, (((*grid).xelems[i as usize] / 2) as Real) + 0.5));
        }
        i += 1;
    }
}

// SAFETY: first-element y-size computation of SetElementDivision.
unsafe fn setdiv_dy(grid: *mut GridType, ny: c_int, ratio: &mut Real) {
    let mut i: c_int;
    i = 1;
    while i <= ny {
        if (*grid).ylinear[i as usize] == TRUE {
            (*grid).dy[i as usize] = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                / (*grid).yelems[i as usize] as Real;
        } else if (*grid).yexpand[i as usize] > 0.0 {
            *ratio = pow(
                (*grid).yexpand[i as usize],
                1.0 / ((*grid).yelems[i as usize] as Real - 1.0),
            );
            (*grid).yratios[i as usize] = *ratio;
            (*grid).dy[i as usize] = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                * (1.0 - *ratio)
                / (1.0 - pow(*ratio, (*grid).yelems[i as usize] as Real));
        } else if (*grid).yelems[i as usize] == 2 {
            (*grid).dy[i as usize] = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                / (*grid).yelems[i as usize] as Real;
        } else if (*grid).yelems[i as usize] % 2 == 0 {
            *ratio = pow(
                -(*grid).yexpand[i as usize],
                1.0 / ((*grid).yelems[i as usize] / 2 - 1) as Real,
            );
            (*grid).yratios[i as usize] = *ratio;
            (*grid).dy[i as usize] =
                0.5 * ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize]) * (1.0 - *ratio)
                    / (1.0 - pow(*ratio, ((*grid).yelems[i as usize] / 2) as Real));
        } else if (*grid).yelems[i as usize] % 2 == 1 {
            *ratio = pow(
                -(*grid).yexpand[i as usize],
                1.0 / ((*grid).yelems[i as usize] / 2) as Real,
            );
            (*grid).yratios[i as usize] = *ratio;
            (*grid).dy[i as usize] = ((*grid).y[i as usize] - (*grid).y[(i - 1) as usize])
                / (2.0 * (1.0 - pow(*ratio, ((*grid).yelems[i as usize] / 2) as Real))
                    / (1.0 - *ratio)
                    + pow(*ratio, (((*grid).yelems[i as usize] / 2) as Real) + 0.5));
        }
        i += 1;
    }
}
// SAFETY: legacy .grd reader over caller-allocated grids; C status codes.
unsafe fn LoadElmergridOld(
    grid: *mut *mut GridType,
    nogrids: *mut c_int,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let input: *mut FILE;
    // NOTE: 0-init for the &mut handoff (set in the mode arms before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the mode arms before reads).
    let mut j: c_int = 0;
    // NOTE: assigned before the read loop below.
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set in the mode arms before reads).
    let mut l: c_int = 0;
    // NOTE: C `=0` decl init shape (never reassigned: kept mutable-free).
    let error: c_int = 0;
    // NOTE: assigned `= 1.0` below before the &mut handoff.
    let mut scaling: Real;
    // NOTE: null-init for the &mut handoff (set before every next_* call).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut mode: c_int;
    let noknots: c_int;
    let noelements: c_int;
    let dim: c_int;
    let axisymmetric: c_int;
    let mut elemcode: c_int;
    let maxnodes: c_int;
    let totelems: c_int;
    let nogrids0: c_int;
    // NOTE: 0-init for the &mut handoff (set in mode 2 before reads).
    let mut minmat: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mode 2 before reads).
    let mut maxmat: c_int = 0;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"grd\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadElmergrid: opening of the file '%s' wasn't successful !\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading the geometry from file '%s'.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitGrid(*grid.wrapping_offset(*nogrids as isize));
    k = *nogrids;
    nogrids0 = *nogrids;
    mode = 0;
    noknots = 0;
    noelements = 0;
    dim = 0;
    axisymmetric = FALSE;
    elemcode = 0;
    maxnodes = 4;
    totelems = 0;
    scaling = 1.0;
    // NOTE: `let _ =` keeps the C-shape assignments visible for audit.
    let _ = (noknots, noelements, dim, axisymmetric, maxnodes, totelems);
    Getline(line.as_mut_ptr(), input);
    loop {
        if Getline(line.as_mut_ptr(), input) != 0 {
            break;
        }
        if line[0] == 0x00 {
            break;
        }
        if !strstr(line.as_ptr(), b"END\0".as_ptr() as *const c_char).is_null() {
            break;
        }
        if !strstr(line.as_ptr(), b"RESULTS\0".as_ptr() as *const c_char).is_null() {
            break;
        }
        // NOTE: control information.
        if !strstr(line.as_ptr(), b"VERSION\0".as_ptr() as *const c_char).is_null() {
            mode = 1;
        } else if !strstr(line.as_ptr(), b"GEOMETRY\0".as_ptr() as *const c_char).is_null() {
            mode = 2;
        } else if !strstr(line.as_ptr(), b"MAPPINGS IN\0".as_ptr() as *const c_char).is_null() {
            mode = 31;
        } else if !strstr(line.as_ptr(), b"MAPPINGS OUT\0".as_ptr() as *const c_char).is_null() {
            mode = 32;
        } else if !strstr(line.as_ptr(), b"MAPPINGS\0".as_ptr() as *const c_char).is_null() {
            mode = 3;
        } else if !strstr(line.as_ptr(), b"NUMBERING\0".as_ptr() as *const c_char).is_null() {
            mode = 4;
        } else if !strstr(line.as_ptr(), b"MESHING\0".as_ptr() as *const c_char).is_null() {
            mode = 5;
        } else if !strstr(line.as_ptr(), b"ELEMENTS\0".as_ptr() as *const c_char).is_null() {
            mode = 6;
        } else if !strstr(line.as_ptr(), b"ELEMENT NUMBER\0".as_ptr() as *const c_char).is_null() {
            mode = 29;
        } else if !strstr(line.as_ptr(), b"NODES\0".as_ptr() as *const c_char).is_null() {
            mode = 7;
        } else if !strstr(line.as_ptr(), b"TRIANGLE\0".as_ptr() as *const c_char).is_null() {
            mode = 8;
        } else if !strstr(line.as_ptr(), b"SQUARE\0".as_ptr() as *const c_char).is_null() {
            mode = 17;
        } else if !strstr(
            line.as_ptr(),
            b"COORDINATE RATIO\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            mode = 10;
        } else if !strstr(line.as_ptr(), b"MATERIALS\0".as_ptr() as *const c_char).is_null() {
            mode = 11;
        } else if !strstr(line.as_ptr(), b"LAYERED ST\0".as_ptr() as *const c_char).is_null() {
            mode = 12;
        } else if !strstr(line.as_ptr(), b"ELEMENT RAT\0".as_ptr() as *const c_char).is_null() {
            mode = 13;
        } else if !strstr(line.as_ptr(), b"ELEMENT DENS\0".as_ptr() as *const c_char).is_null() {
            mode = 14;
        } else if !strstr(
            line.as_ptr(),
            b"ELEMENT MINIMUM\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            mode = 27;
        } else if !strstr(line.as_ptr(), b"BOUNDARY COND\0".as_ptr() as *const c_char).is_null() {
            mode = 15;
        } else if !strstr(line.as_ptr(), b"ELEMENTTYPE\0".as_ptr() as *const c_char).is_null()
            || !strstr(line.as_ptr(), b"ELEMENTCODE\0".as_ptr() as *const c_char).is_null()
        {
            mode = 16;
        } else if !strstr(line.as_ptr(), b"ROTATE\0".as_ptr() as *const c_char).is_null() {
            mode = 20;
        } else if !strstr(line.as_ptr(), b"ROTRAD\0".as_ptr() as *const c_char).is_null() {
            mode = 21;
        } else if !strstr(line.as_ptr(), b"ROTBLOCK\0".as_ptr() as *const c_char).is_null() {
            mode = 22;
        } else if !strstr(line.as_ptr(), b"ROTIMP\0".as_ptr() as *const c_char).is_null() {
            mode = 24;
        } else if !strstr(line.as_ptr(), b"ROTCURVE\0".as_ptr() as *const c_char).is_null() {
            mode = 25;
        } else if !strstr(line.as_ptr(), b"REDUCE ELEMENT\0".as_ptr() as *const c_char).is_null() {
            mode = 26;
        } else if !strstr(line.as_ptr(), b"SCALING\0".as_ptr() as *const c_char).is_null() {
            mode = 23;
        } else if !strstr(line.as_ptr(), b"LAYERED BO\0".as_ptr() as *const c_char).is_null() {
            mode = 28;
        } else if !strstr(line.as_ptr(), b"POLAR RADIUS\0".as_ptr() as *const c_char).is_null() {
            mode = 30;
        }
        // NOTE: returns true on the MESHING-overflow `goto end` path.
        if loadold_mode(
            grid,
            nogrids,
            nogrids0,
            &mut k,
            &mut l,
            &mut mode,
            &mut elemcode,
            &mut minmat,
            &mut maxmat,
            &mut scaling,
            &mut cp,
            &mut i,
            &mut j,
            line.as_mut_ptr(),
            input,
            info,
        ) {
            break;
        }
    }
    if info != 0 {
        printf(
            b"Found %d divisions for grid\n\0".as_ptr() as *const c_char,
            *nogrids,
        );
    }
    k = nogrids0;
    while k < *nogrids && k < MAXCASES as c_int {
        SetElementDivision(&mut (*(*grid).wrapping_offset(k as isize)), 1.0, info);
        k += 1;
    }
    fclose(input);
    error
}
// SAFETY: one legacy-.grd mode arm (split for sizing); true = break loop.
unsafe fn loadold_mode(
    grid: *mut *mut GridType,
    nogrids: *mut c_int,
    nogrids0: c_int,
    k: &mut c_int,
    l: &mut c_int,
    mode: &mut c_int,
    elemcode: &mut c_int,
    minmat: &mut c_int,
    maxmat: &mut c_int,
    scaling: &mut Real,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
    _info: c_int,
) -> bool {
    // NOTE: grid[k] shorthand (C `(*grid)[k]` / `grid[k]`).
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if *mode == 1 {
        printf(
            b"Loading Elmergrid file: %s\n\0".as_ptr() as *const c_char,
            line,
        );
        *mode = 0;
        return false;
    }
    if *mode == 2 {
        gk.dimension = 2;
        if !strstr(line, b"CARTES\0".as_ptr() as *const c_char).is_null()
            && !strstr(line, b"1D\0".as_ptr() as *const c_char).is_null()
        {
            gk.coordsystem = COORD_CART1;
            gk.dimension = 1;
        } else if !strstr(line, b"CARTES\0".as_ptr() as *const c_char).is_null()
            && !strstr(line, b"2D\0".as_ptr() as *const c_char).is_null()
        {
            gk.coordsystem = COORD_CART2;
        } else if !strstr(line, b"AXIS\0".as_ptr() as *const c_char).is_null()
            && !strstr(line, b"2D\0".as_ptr() as *const c_char).is_null()
        {
            gk.coordsystem = COORD_AXIS;
        } else if !strstr(line, b"POLAR\0".as_ptr() as *const c_char).is_null()
            && !strstr(line, b"2D\0".as_ptr() as *const c_char).is_null()
        {
            gk.coordsystem = COORD_POLAR;
        } else if !strstr(line, b"CARTES\0".as_ptr() as *const c_char).is_null()
            && !strstr(line, b"3D\0".as_ptr() as *const c_char).is_null()
        {
            gk.coordsystem = COORD_CART3;
            gk.dimension = 3;
        } else if !strstr(line, b"CYLINDRICAL\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_CYL;
            gk.dimension = 3;
        } else {
            printf(
                b"Unknown coordinate system: %s\n\0".as_ptr() as *const c_char,
                line,
            );
        }
        printf(
            b"Defining the coordinate system (%d-DIM).\n\0".as_ptr() as *const c_char,
            gk.dimension,
        );
        Getline(line, input);
        if gk.dimension == 1 {
            sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut gk.xcells);
            gk.ycells = 1;
        }
        if gk.dimension == 2 {
            sscanf(
                line,
                b"%d %d\0".as_ptr() as *const c_char,
                &mut gk.xcells,
                &mut gk.ycells,
            );
        }
        if gk.dimension == 3 {
            sscanf(
                line,
                b"%d %d %d\0".as_ptr() as *const c_char,
                &mut gk.xcells,
                &mut gk.ycells,
                &mut gk.zcells,
            );
        }
        if gk.xcells >= MAXCELLS as c_int
            || gk.ycells >= MAXCELLS as c_int
            || gk.zcells >= MAXCELLS as c_int
        {
            printf(
                b"LoadGrid: Too many subcells [%d %d %d] vs. %d:\n\0".as_ptr() as *const c_char,
                gk.xcells,
                gk.ycells,
                gk.zcells,
                MAXCELLS as c_int,
            );
        }
        if gk.dimension == 1 {
            printf(
                b"Loading [%d] subcell intervals in 1D\n\0".as_ptr() as *const c_char,
                gk.xcells,
            );
        } else if gk.dimension == 2 {
            printf(
                b"Loading [%d %d] subcell intervals in 2D\n\0".as_ptr() as *const c_char,
                gk.xcells,
                gk.ycells,
            );
        } else {
            printf(
                b"Loading [%d %d %d] subcell intervals in 3D\n\0".as_ptr() as *const c_char,
                gk.xcells,
                gk.ycells,
                gk.zcells,
            );
        }
        *j = 1;
        while *j <= gk.dimension {
            Getline(line, input);
            *cp = line;
            if *j == 1 {
                *i = 0;
                while *i <= gk.xcells {
                    gk.x[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 2 {
                *i = 0;
                while *i <= gk.ycells {
                    gk.y[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 3 {
                *i = 0;
                while *i <= gk.zcells {
                    gk.z[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            *j += 1;
        }
        printf(b"Loading material structure\n\0".as_ptr() as *const c_char);
        *j = gk.ycells;
        while *j >= 1 {
            Getline(line, input);
            *cp = line;
            *i = 1;
            while *i <= gk.xcells {
                gk.structure[*j as usize][*i as usize] = next_int(&mut (*cp));
                *i += 1;
            }
            *j -= 1;
        }
        *minmat = gk.structure[1][1];
        *maxmat = gk.structure[1][1];
        *j = gk.ycells;
        while *j >= 1 {
            *i = 1;
            while *i <= gk.xcells {
                if *minmat > gk.structure[*j as usize][*i as usize] {
                    *minmat = gk.structure[*j as usize][*i as usize];
                }
                if *maxmat < gk.structure[*j as usize][*i as usize] {
                    *maxmat = gk.structure[*j as usize][*i as usize];
                }
                *i += 1;
            }
            *j -= 1;
        }
        if *minmat < 0 {
            printf(
                b"LoadElmergrid: please use positive material indices.\n\0".as_ptr()
                    as *const c_char,
            );
        }
        *mode = 0;
        return false;
    }
    loadold_mode2(
        grid, nogrids, nogrids0, k, l, mode, elemcode, minmat, maxmat, scaling, cp, i, j, line,
        input,
    )
}
// SAFETY: legacy-.grd modes 3-29 (split for sizing); true = break loop.
unsafe fn loadold_mode2(
    grid: *mut *mut GridType,
    nogrids: *mut c_int,
    nogrids0: c_int,
    k: &mut c_int,
    l: &mut c_int,
    mode: &mut c_int,
    elemcode: &mut c_int,
    _minmat: &mut c_int,
    _maxmat: &mut c_int,
    scaling: &mut Real,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
) -> bool {
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if *mode == 3 || *mode == 31 || *mode == 32 {
        // NOTE: obsolete mapping loader (see C comment).
        *l = 1;
        *i = gk.mappings;
        while *i < gk.mappings + *l {
            Getline(line, input);
            *cp = line;
            gk.mappingtype[*i as usize] = next_int(&mut (*cp));
            if *mode == 32 {
                // NOTE: C `SGN` macro (egdef.h:98).
                let t = gk.mappingtype[*i as usize];
                gk.mappingtype[*i as usize] += 50
                    * (if t < 0 {
                        -1
                    } else if t > 0 {
                        1
                    } else {
                        0
                    });
            }
            gk.mappingline[*i as usize] = next_int(&mut (*cp));
            gk.mappinglimits[(2 * *i) as usize] = next_real(&mut (*cp));
            gk.mappinglimits[(2 * *i + 1) as usize] = next_real(&mut (*cp));
            gk.mappingpoints[*i as usize] = next_int(&mut (*cp));
            // NOTE: C `Rvector` is a macro for `dvector` (egutils.h:15).
            gk.mappingparams[*i as usize] = dvector(0, gk.mappingpoints[*i as usize]);
            *j = 0;
            while *j < gk.mappingpoints[*i as usize] {
                *gk.mappingparams[*i as usize].wrapping_offset(*j as isize) = next_real(&mut (*cp));
                *j += 1;
            }
            *i += 1;
        }
        printf(
            b"Loaded %d geometry mappings\n\0".as_ptr() as *const c_char,
            *l,
        );
        gk.mappings += *l;
        *mode = 0;
        return false;
    }
    if *mode == 4 {
        if !strstr(line, b"HORIZ\0".as_ptr() as *const c_char).is_null() {
            gk.numbering = NUMBER_XY;
        }
        if !strstr(line, b"VERTI\0".as_ptr() as *const c_char).is_null() {
            gk.numbering = NUMBER_YX;
        }
        *mode = 0;
        return false;
    }
    if *mode == 5 {
        if *nogrids >= MAXCASES as c_int {
            printf(b"There are more grids than was allocated for!\n\0".as_ptr() as *const c_char);
            printf(
                b"Ignoring meshes starting from %d\n.\0".as_ptr() as *const c_char,
                *nogrids + 1,
            );
            return true;
        }
        *nogrids += 1;
        printf(
            b"Loading element meshing no %d\n\0".as_ptr() as *const c_char,
            *nogrids,
        );
        *k = *nogrids - 1;
        if *k > nogrids0 {
            *(*grid).wrapping_offset(*k as isize) = *(*grid).wrapping_offset((*k - 1) as isize);
        }
        *mode = 0;
        return false;
    }
    if *mode == 6 {
        sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut gk.wantedelems);
        *mode = 0;
        return false;
    }
    if *mode == 7 {
        sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut gk.nonodes);
        gk.elemmidpoints = FALSE;
        if gk.nonodes == 4 {
            gk.elemorder = 1;
        }
        if gk.nonodes == 8 {
            gk.elemorder = 2;
        }
        if gk.nonodes == 16 {
            gk.elemorder = 3;
        }
        if gk.nonodes == 9 {
            gk.elemorder = 2;
            gk.elemmidpoints = TRUE;
        }
        if gk.nonodes == 12 {
            gk.elemorder = 3;
            gk.elemmidpoints = TRUE;
        }
        *mode = 0;
        return false;
    }
    if *mode == 8 {
        gk.triangles = TRUE;
        *mode = 0;
        return false;
    }
    if *mode == 17 {
        gk.triangles = FALSE;
        *mode = 0;
        return false;
    }
    if *mode == 16 {
        sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut (*elemcode));
        if *elemcode / 100 == 2 {
            gk.triangles = FALSE;
            gk.nonodes = *elemcode % 100;
        } else if *elemcode / 100 == 4 {
            gk.triangles = FALSE;
            gk.nonodes = *elemcode % 100;
        } else if *elemcode / 100 == 3 {
            gk.triangles = TRUE;
            if *elemcode % 100 == 3 {
                gk.nonodes = 4;
            } else if *elemcode % 100 == 6 {
                gk.nonodes = 9;
            } else if *elemcode % 100 == 10 {
                gk.nonodes = 16;
            }
        }
        gk.elemmidpoints = FALSE;
        if gk.nonodes == 4 {
            gk.elemorder = 1;
        }
        if gk.nonodes == 8 {
            gk.elemorder = 2;
        }
        if gk.nonodes == 16 {
            gk.elemorder = 3;
        }
        if gk.nonodes == 9 {
            gk.elemorder = 2;
            gk.elemmidpoints = TRUE;
        }
        if gk.nonodes == 12 {
            gk.elemorder = 3;
            gk.elemmidpoints = TRUE;
        }
        *mode = 0;
        return false;
    }
    loadold_mode3(grid, k, mode, scaling, cp, i, j, line, input)
}
// SAFETY: legacy-.grd modes 10-23 (split for sizing); always false.
unsafe fn loadold_mode3(
    grid: *mut *mut GridType,
    k: &mut c_int,
    mode: &mut c_int,
    scaling: &mut Real,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
) -> bool {
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if *mode == 10 {
        if gk.dimension == 2 {
            sscanf(line, b"%le\0".as_ptr() as *const c_char, &mut gk.xyratio);
        }
        if gk.dimension == 3 {
            sscanf(
                line,
                b"%le %le\0".as_ptr() as *const c_char,
                &mut gk.xyratio,
                &mut gk.xzratio,
            );
        }
        *mode = 0;
    } else if *mode == 11 {
        sscanf(
            line,
            b"%d %d\0".as_ptr() as *const c_char,
            &mut gk.firstmaterial,
            &mut gk.lastmaterial,
        );
        *mode = 0;
    } else if *mode == 12 {
        *i = 1;
        while *i <= gk.zcells {
            Getline(line, input);
            sscanf(
                line,
                b"%d %d %d\n\0".as_ptr() as *const c_char,
                &mut gk.zfirstmaterial[*i as usize],
                &mut gk.zlastmaterial[*i as usize],
                &mut gk.zmaterial[*i as usize],
            );
            *i += 1;
        }
        *mode = 0;
    } else if *mode == 13 {
        printf(b"Loading element ratios\n\0".as_ptr() as *const c_char);
        *j = 1;
        while *j <= gk.dimension {
            Getline(line, input);
            *cp = line;
            if *j == 1 {
                *i = 1;
                while *i <= gk.xcells {
                    gk.xexpand[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 2 {
                *i = 1;
                while *i <= gk.ycells {
                    gk.yexpand[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 3 {
                *i = 1;
                while *i <= gk.zcells {
                    gk.zexpand[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            *j += 1;
        }
        *mode = 0;
    } else if *mode == 29 {
        printf(b"Loading element numbers\n\0".as_ptr() as *const c_char);
        *j = 1;
        while *j <= gk.dimension {
            Getline(line, input);
            *cp = line;
            if *j == 1 {
                *i = 1;
                while *i <= gk.xcells {
                    gk.xelems[*i as usize] = next_int(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 2 {
                *i = 1;
                while *i <= gk.ycells {
                    gk.yelems[*i as usize] = next_int(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 3 {
                *i = 1;
                while *i <= gk.zcells {
                    gk.zelems[*i as usize] = next_int(&mut (*cp));
                    *i += 1;
                }
            }
            *j += 1;
        }
        gk.autoratio = 0;
        *mode = 0;
    } else if *mode == 27 {
        printf(b"Loading minimum number of elements\n\0".as_ptr() as *const c_char);
        if gk.dimension == 1 {
            sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut gk.minxelems);
        }
        if gk.dimension == 2 {
            sscanf(
                line,
                b"%d %d\0".as_ptr() as *const c_char,
                &mut gk.minxelems,
                &mut gk.minyelems,
            );
        }
        if gk.dimension == 3 {
            sscanf(
                line,
                b"%d %d %d\0".as_ptr() as *const c_char,
                &mut gk.minxelems,
                &mut gk.minyelems,
                &mut gk.minzelems,
            );
        }
        *mode = 0;
    } else if *mode == 14 {
        printf(b"Loading element densities\n\0".as_ptr() as *const c_char);
        *j = 1;
        while *j <= gk.dimension {
            Getline(line, input);
            *cp = line;
            if *j == 1 {
                *i = 1;
                while *i <= gk.xcells {
                    gk.xdens[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 2 {
                *i = 1;
                while *i <= gk.ycells {
                    gk.ydens[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            if *j == 3 {
                *i = 1;
                while *i <= gk.zcells {
                    gk.zdens[*i as usize] = next_real(&mut (*cp));
                    *i += 1;
                }
            }
            *j += 1;
        }
        *mode = 0;
    } else if *mode == 15 {
        sscanf(
            line,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.noboundaries,
        );
        printf(
            b"Loading %d boundary conditions\n\0".as_ptr() as *const c_char,
            gk.noboundaries,
        );
        *i = 0;
        while *i < gk.noboundaries {
            Getline(line, input);
            sscanf(
                line,
                b"%d %d %d %d\0".as_ptr() as *const c_char,
                &mut gk.boundtype[*i as usize],
                &mut gk.boundext[*i as usize],
                &mut gk.boundint[*i as usize],
                &mut gk.boundsolid[*i as usize],
            );
            *i += 1;
        }
        *mode = 0;
    } else if *mode == 20 {
        gk.rotate = TRUE;
        *mode = 0;
    } else if *mode == 21 {
        sscanf(
            line,
            b"%le\0".as_ptr() as *const c_char,
            &mut gk.rotateradius2,
        );
        *mode = 0;
    } else if *mode == 22 {
        sscanf(
            line,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.rotateblocks,
        );
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        *mode = 0;
    } else if *mode == 24 {
        sscanf(
            line,
            b"%le\0".as_ptr() as *const c_char,
            &mut gk.rotateimprove,
        );
        *mode = 0;
    } else if *mode == 30 {
        sscanf(
            line,
            b"%le\0".as_ptr() as *const c_char,
            &mut gk.polarradius,
        );
        *mode = 0;
    } else if *mode == 25 {
        gk.rotatecurve = TRUE;
        sscanf(
            line,
            b"%le%le%le\0".as_ptr() as *const c_char,
            &mut gk.curvezet,
            &mut gk.curverad,
            &mut gk.curveangle,
        );
        *mode = 0;
    } else if *mode == 26 {
        sscanf(
            line,
            b"%d%d\0".as_ptr() as *const c_char,
            &mut gk.reduceordermatmin,
            &mut gk.reduceordermatmax,
        );
        *mode = 0;
    } else if *mode == 28 {
        sscanf(line, b"%d\0".as_ptr() as *const c_char, &mut gk.layeredbc);
        *mode = 0;
    } else if *mode == 23 {
        sscanf(line, b"%le\0".as_ptr() as *const c_char, &mut (*scaling));
        *i = 0;
        while *i <= gk.xcells {
            gk.x[*i as usize] *= *scaling;
            *i += 1;
        }
        if gk.dimension > 1 {
            *i = 0;
            while *i <= gk.ycells {
                gk.y[*i as usize] *= *scaling;
                *i += 1;
            }
        }
        if gk.dimension == 3 {
            // NOTE: C loops ycells here (not zcells); mirrored exactly.
            *i = 0;
            while *i <= gk.ycells {
                gk.z[*i as usize] *= *scaling;
                *i += 1;
            }
        }
        gk.rotateradius2 *= *scaling;
        gk.curverad *= *scaling;
        gk.curvezet *= *scaling;
        *mode = 0;
    } else {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    }
    false
}
// NOTE: C function-local `MAXBODYID` (egnative.c:335, file-visible after
// its definition); hoisted as a module const for later functions.
const MAXBODYID: c_int = 1000;

// SAFETY: modern .grd reader over caller-allocated grids; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadElmergrid(
    grid: *mut *mut GridType,
    nogrids: *mut c_int,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut command: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut params: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let input: *mut FILE;
    // NOTE: 0-init for the &mut handoff (set in the command arms before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the command arms before reads).
    let mut j: c_int = 0;
    // NOTE: assigned before the read loop below.
    let mut k: c_int;
    // NOTE: null-init for the &mut handoff (set before every next_* call).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let noknots: c_int;
    let noelements: c_int;
    let dim: c_int;
    let axisymmetric: c_int;
    // NOTE: 0-init for the mode arms (ELEMENTTYPE/ELEMENTCODE may set it).
    let mut elemcode: c_int = 0;
    let maxnodes: c_int;
    let totelems: c_int;
    let nogrids0: c_int;
    // NOTE: 0-init for the mode arms (MATERIAL STRUCTURE may set them).
    let mut minmat: c_int = 0;
    // NOTE: 0-init for the mode arms (MATERIAL STRUCTURE may set it).
    let mut maxmat: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads on the VERSION path).
    let mut code: c_long = 0;
    // NOTE: 0-init for the &mut handoff (set in ORIGIN arms before reads).
    let mut raid: Real = 0.0;
    // NOTE: dead C locals kept read-visible for audit (never read in C either).
    noknots = 0;
    noelements = 0;
    dim = 0;
    axisymmetric = FALSE;
    maxnodes = 4;
    totelems = 0;
    let _ = (
        &noknots,
        &noelements,
        &dim,
        &axisymmetric,
        &maxnodes,
        &totelems,
    );
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"grd\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadElmergrid: opening of the file '%s' wasn't successful !\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading the geometry from file '%s'.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitGrid(*grid.wrapping_offset(*nogrids as isize));
    k = *nogrids;
    nogrids0 = *nogrids;
    iodebug = FALSE;
    matcactive = FALSE;
    loop {
        if GetCommand(command.as_mut_ptr(), params.as_mut_ptr(), input) != 0 {
            if info != 0 {
                printf(b"Reached the end of command file\n\0".as_ptr() as *const c_char);
            }
            break;
        }
        // NOTE: control information.
        if !strstr(command.as_ptr(), b"VERSION\0".as_ptr() as *const c_char).is_null() {
            if !strstr(command.as_ptr(), b"080500\0".as_ptr() as *const c_char).is_null() {
                if info != 0 {
                    printf(b"Loading old version of Elmergrid file.\n\0".as_ptr() as *const c_char);
                }
                i = LoadElmergridOld(grid, nogrids, prefix, info);
                return i;
            } else {
                sscanf(
                    params.as_ptr(),
                    b"%ld\0".as_ptr() as *const c_char,
                    &mut code,
                );
                if code == 210903 {
                    if info != 0 {
                        printf(
                            b"Loading ElmerGrid file version: %ld\n\0".as_ptr() as *const c_char,
                            code,
                        );
                    }
                } else {
                    printf(
                        b"Unknown ElmerGrid file version: %ld\n\0".as_ptr() as *const c_char,
                        code,
                    );
                    return 2;
                }
            }
            *nogrids += 1;
        } else if !strstr(
            command.as_ptr(),
            b"START NEW MESH\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            if *nogrids >= MAXCASES as c_int {
                printf(
                    b"There are more grids than was allocated for!\n\0".as_ptr() as *const c_char
                );
                printf(
                    b"Ignoring meshes starting from %d\n.\0".as_ptr() as *const c_char,
                    *nogrids + 1,
                );
                break;
            }
            *nogrids += 1;
            printf(
                b"\nLoading element meshing no %d\n\0".as_ptr() as *const c_char,
                *nogrids,
            );
            k = *nogrids - 1;
            if k > nogrids0 {
                *(*grid).wrapping_offset(k as isize) = *(*grid).wrapping_offset((k - 1) as isize);
            }
        } else {
            load_cmd(
                grid,
                &mut k,
                &mut cp,
                &mut i,
                &mut j,
                &mut elemcode,
                &mut minmat,
                &mut maxmat,
                &mut raid,
                command.as_mut_ptr(),
                params.as_mut_ptr(),
                input,
                info,
            );
        }
    }
    if info != 0 {
        printf(
            b"Found %d divisions for grid\n\0".as_ptr() as *const c_char,
            *nogrids,
        );
    }
    k = nogrids0;
    while k < *nogrids && k < MAXCASES as c_int {
        load_elemcode(grid, k, &mut elemcode);
        k += 1;
    }
    // NOTE: commented-out SetElementDivision loop omitted (dead in C).
    fclose(input);
    0
}

// SAFETY: post-load element-code resolution (end loop of LoadElmergrid).
unsafe fn load_elemcode(grid: *mut *mut GridType, k: c_int, elemcode: &mut c_int) {
    let gk = &mut (*(*grid).wrapping_offset(k as isize));
    if *elemcode == 0 {
        if gk.dimension == 1 {
            gk.nonodes = gk.elemorder + 1;
        } else if gk.elemmidpoints == FALSE {
            gk.nonodes = 4 * gk.elemorder;
        } else {
            if gk.elemorder == 2 {
                gk.nonodes = 9;
            }
            if gk.elemorder == 3 {
                gk.nonodes = 16;
            }
        }
    } else if *elemcode / 100 == 2 {
        gk.triangles = FALSE;
        gk.nonodes = *elemcode % 100;
    } else if *elemcode / 100 == 4 {
        gk.triangles = FALSE;
        gk.nonodes = *elemcode % 100;
    } else if *elemcode / 100 == 3 {
        gk.triangles = TRUE;
        if *elemcode % 100 == 3 {
            gk.nonodes = 4;
        } else if *elemcode % 100 == 6 {
            gk.nonodes = 9;
        } else if *elemcode % 100 == 10 {
            gk.nonodes = 16;
        }
    }
}
// SAFETY: one modern-.grd command arm, geometry half (split for sizing).
unsafe fn load_cmd(
    grid: *mut *mut GridType,
    k: &mut c_int,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    elemcode: &mut c_int,
    minmat: &mut c_int,
    maxmat: &mut c_int,
    raid: &mut Real,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if !strstr(command, b"DEBUG IO\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            iodebug = FALSE;
        } else {
            iodebug = TRUE;
            printf(b"IO debugging activated\n\0".as_ptr() as *const c_char);
        }
    } else if !strstr(command, b"MATC\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            matcactive = FALSE;
        } else {
            // NOTE: USE_MATC=0 arm (the #else branch in C).
            matcactive = FALSE;
            printf(
                b"Unable to enable MATC, as it is not even compiled.\n\0".as_ptr() as *const c_char,
            );
            bigerror(
                b"Unable to enable MATC, as it is not even compiled.\0".as_ptr() as *const c_char,
            );
        }
    } else if !strstr(command, b"COORDINATE SYSTEM\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        gk.dimension = 2;
        if !strstr(params, b"CARTESIAN 1D\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_CART1;
            gk.dimension = 1;
        } else if !strstr(params, b"CARTESIAN 2D\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_CART2;
        } else if !strstr(params, b"AXISYMMETRIC\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_AXIS;
        } else if !strstr(params, b"POLAR\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_POLAR;
        } else if !strstr(params, b"CARTESIAN 3D\0".as_ptr() as *const c_char).is_null() {
            gk.coordsystem = COORD_CART3;
            gk.dimension = 3;
        } else {
            printf(
                b"Unknown coordinate system: %s\n\0".as_ptr() as *const c_char,
                params,
            );
        }
        if info != 0 {
            printf(
                b"Defining the coordinate system (%d-DIM).\n\0".as_ptr() as *const c_char,
                gk.dimension,
            );
        }
    } else if !strstr(command, b"SUBCELL DIVISIONS\0".as_ptr() as *const c_char).is_null() {
        if gk.dimension == 1 {
            sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut gk.xcells);
            gk.ycells = 1;
        } else if gk.dimension == 2 {
            sscanf(
                params,
                b"%d %d\0".as_ptr() as *const c_char,
                &mut gk.xcells,
                &mut gk.ycells,
            );
        } else if gk.dimension == 3 {
            sscanf(
                params,
                b"%d %d %d\0".as_ptr() as *const c_char,
                &mut gk.xcells,
                &mut gk.ycells,
                &mut gk.zcells,
            );
        }
        if gk.xcells >= MAXCELLS as c_int
            || gk.ycells >= MAXCELLS as c_int
            || gk.zcells >= MAXCELLS as c_int
        {
            printf(
                b"LoadElmergrid: Too many subcells [%d %d %d] vs. %d:\n\0".as_ptr()
                    as *const c_char,
                gk.xcells,
                gk.ycells,
                gk.zcells,
                MAXCELLS as c_int,
            );
        }
        // NOTE: initialize the default structure with ones.
        *j = gk.ycells;
        while *j >= 1 {
            *i = 1;
            while *i <= gk.xcells {
                gk.structure[*j as usize][*i as usize] = 1;
                *i += 1;
            }
            *j -= 1;
        }
    } else if !strstr(
        command,
        b"MINIMUM ELEMENT DIVISION\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        if info != 0 {
            printf(b"Loading minimum number of elements\n\0".as_ptr() as *const c_char);
        }
        if gk.dimension == 1 {
            sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut gk.minxelems);
        }
        if gk.dimension == 2 {
            sscanf(
                params,
                b"%d %d\0".as_ptr() as *const c_char,
                &mut gk.minxelems,
                &mut gk.minyelems,
            );
        }
        if gk.dimension == 3 {
            sscanf(
                params,
                b"%d %d %d\0".as_ptr() as *const c_char,
                &mut gk.minxelems,
                &mut gk.minyelems,
                &mut gk.minzelems,
            );
        }
    } else if !strstr(command, b"SUBCELL LIMITS 1\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(
                b"Loading %d subcell limits in X-direction\n\0".as_ptr() as *const c_char,
                gk.xcells + 1,
            );
        }
        *cp = params;
        *i = 0;
        while *i <= gk.xcells {
            gk.x[*i as usize] = next_real(&mut (*cp));
            if *i > 0 && gk.x[*i as usize] < gk.x[(*i - 1) as usize] {
                printf(
                    b"Subcell limits 1(%d): %12.6le %12.6le\n\0".as_ptr() as *const c_char,
                    *i,
                    gk.x[*i as usize],
                    gk.x[(*i - 1) as usize],
                );
                bigerror(
                    b"Values for limits 1 should be a growing series, existing\n\0".as_ptr()
                        as *const c_char,
                );
            }
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL LIMITS 2\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(
                b"Loading %d subcell limits in Y-direction\n\0".as_ptr() as *const c_char,
                gk.ycells + 1,
            );
        }
        *cp = params;
        *i = 0;
        while *i <= gk.ycells {
            gk.y[*i as usize] = next_real(&mut (*cp));
            if *i > 0 && gk.y[*i as usize] < gk.y[(*i - 1) as usize] {
                printf(
                    b"Subcell limits 2(%d): %12.6le %12.6le\n\0".as_ptr() as *const c_char,
                    *i,
                    gk.y[*i as usize],
                    gk.y[(*i - 1) as usize],
                );
                bigerror(
                    b"Values for limits should be a growing series, existing\n\0".as_ptr()
                        as *const c_char,
                );
            }
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL LIMITS 3\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(
                b"Loading %d subcell limits in Z-direction\n\0".as_ptr() as *const c_char,
                gk.zcells + 1,
            );
        }
        *cp = params;
        *i = 0;
        while *i <= gk.zcells {
            gk.z[*i as usize] = next_real(&mut (*cp));
            if *i > 0 && gk.z[*i as usize] < gk.z[(*i - 1) as usize] {
                printf(
                    b"Subcell limits 3(%d): %12.6le %12.6le\n\0".as_ptr() as *const c_char,
                    *i,
                    gk.z[*i as usize],
                    gk.z[(*i - 1) as usize],
                );
                bigerror(
                    b"Values for limits should be a growing series, existing\n\0".as_ptr()
                        as *const c_char,
                );
            }
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL SIZES 1\0".as_ptr() as *const c_char).is_null() {
        // NOTE: C prints no info line for SIZES 1 (unlike 2/3).
        *cp = params;
        *i = 1;
        while *i <= gk.xcells {
            gk.x[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
        *i = 1;
        while *i <= gk.xcells {
            gk.x[*i as usize] = gk.x[(*i - 1) as usize] + gk.x[*i as usize];
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL SIZES 2\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(
                b"Loading %d subcell sizes in Y-direction\n\0".as_ptr() as *const c_char,
                gk.ycells,
            );
        }
        *cp = params;
        *i = 1;
        while *i <= gk.ycells {
            gk.y[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
        *i = 1;
        while *i <= gk.ycells {
            gk.y[*i as usize] = gk.y[(*i - 1) as usize] + gk.y[*i as usize];
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL SIZES 3\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(
                b"Loading %d subcell sizes in Z-direction\n\0".as_ptr() as *const c_char,
                gk.zcells,
            );
        }
        *cp = params;
        *i = 1;
        while *i <= gk.zcells {
            gk.z[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
        *i = 1;
        while *i <= gk.zcells {
            gk.z[*i as usize] = gk.z[(*i - 1) as usize] + gk.z[*i as usize];
            *i += 1;
        }
    } else {
        load_cmd2(
            grid, k, cp, i, j, elemcode, minmat, maxmat, raid, command, params, input, info,
        );
    }
}
// SAFETY: modern-.grd command arms, materials/boundary half (split).
unsafe fn load_cmd2(
    grid: *mut *mut GridType,
    k: &mut c_int,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    elemcode: &mut c_int,
    minmat: &mut c_int,
    maxmat: &mut c_int,
    raid: &mut Real,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if !strstr(command, b"SUBCELL ORIGIN 1\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"CENTER\0".as_ptr() as *const c_char).is_null() {
            *raid = 0.5 * (gk.x[0] + gk.x[gk.xcells as usize]);
        } else if !strstr(params, b"LEFT\0".as_ptr() as *const c_char).is_null()
            || !strstr(params, b"MIN\0".as_ptr() as *const c_char).is_null()
        {
            *raid = gk.x[0];
        } else if !strstr(params, b"RIGHT\0".as_ptr() as *const c_char).is_null()
            || !strstr(params, b"MAX\0".as_ptr() as *const c_char).is_null()
        {
            *raid = gk.x[gk.xcells as usize];
        } else {
            *cp = params;
            *raid = next_real(&mut (*cp));
        }
        *i = 0;
        while *i <= gk.xcells {
            gk.x[*i as usize] -= *raid;
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL ORIGIN 2\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"CENTER\0".as_ptr() as *const c_char).is_null() {
            *raid = 0.5 * (gk.y[0] + gk.y[gk.ycells as usize]);
        } else if !strstr(params, b"LEFT\0".as_ptr() as *const c_char).is_null() {
            *raid = gk.y[0];
        } else if !strstr(params, b"RIGHT\0".as_ptr() as *const c_char).is_null() {
            *raid = gk.y[gk.ycells as usize];
        } else {
            *cp = params;
            *raid = next_real(&mut (*cp));
        }
        *i = 0;
        while *i <= gk.ycells {
            gk.y[*i as usize] -= *raid;
            *i += 1;
        }
    } else if !strstr(command, b"SUBCELL ORIGIN 3\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"CENTER\0".as_ptr() as *const c_char).is_null() {
            *raid = 0.5 * (gk.z[0] + gk.z[gk.zcells as usize]);
        } else if !strstr(params, b"LEFT\0".as_ptr() as *const c_char).is_null() {
            *raid = gk.z[0];
        } else if !strstr(params, b"RIGHT\0".as_ptr() as *const c_char).is_null() {
            *raid = gk.z[gk.zcells as usize];
        } else {
            *cp = params;
            *raid = next_real(&mut (*cp));
        }
        *i = 0;
        while *i <= gk.zcells {
            gk.z[*i as usize] -= *raid;
            *i += 1;
        }
    } else if !strstr(command, b"MATERIAL STRUCTURE\0".as_ptr() as *const c_char).is_null() {
        if info != 0 {
            printf(b"Loading material structure\n\0".as_ptr() as *const c_char);
        }
        // NOTE: initialize the default structure with zeros.
        *j = gk.ycells;
        while *j >= 1 {
            *i = 1;
            while *i <= gk.xcells {
                gk.structure[*j as usize][*i as usize] = 0;
                *i += 1;
            }
            *j -= 1;
        }
        *j = gk.ycells;
        while *j >= 1 {
            if *j < gk.ycells {
                Getline(params, input);
            }
            *cp = params;
            *i = 1;
            while *i <= gk.xcells {
                gk.structure[*j as usize][*i as usize] = next_int(&mut (*cp));
                *i += 1;
            }
            *j -= 1;
        }
        *minmat = gk.structure[1][1];
        *maxmat = gk.structure[1][1];
        *j = gk.ycells;
        while *j >= 1 {
            *i = 1;
            while *i <= gk.xcells {
                if *minmat > gk.structure[*j as usize][*i as usize] {
                    *minmat = gk.structure[*j as usize][*i as usize];
                }
                if *maxmat < gk.structure[*j as usize][*i as usize] {
                    *maxmat = gk.structure[*j as usize][*i as usize];
                }
                *i += 1;
            }
            *j -= 1;
        }
        if *minmat < 0 {
            printf(
                b"LoadElmergrid: please use positive material indices.\n\0".as_ptr()
                    as *const c_char,
            );
        }
        if *maxmat > MAXBODYID {
            printf(
                b"LoadElmergrid: material indices larger to %d may create problems.\n\0".as_ptr()
                    as *const c_char,
                MAXBODYID,
            );
        }
        printf(
            b"LoadElmergrid: materials interval is [%d,%d]\n\0".as_ptr() as *const c_char,
            *minmat,
            *maxmat,
        );
        gk.maxmaterial = *maxmat;
    } else if !strstr(command, b"MATERIALS INTERVAL\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d %d\0".as_ptr() as *const c_char,
            &mut gk.firstmaterial,
            &mut gk.lastmaterial,
        );
    } else if !strstr(command, b"REVOLVE\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        if !strstr(command, b"REVOLVE RADIUS\0".as_ptr() as *const c_char).is_null() {
            gk.rotate = TRUE;
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut gk.rotateradius2,
            );
        } else if !strstr(command, b"REVOLVE BLOCKS\0".as_ptr() as *const c_char).is_null() {
            gk.rotate = TRUE;
            sscanf(
                params,
                b"%d\0".as_ptr() as *const c_char,
                &mut gk.rotateblocks,
            );
        } else if !strstr(command, b"REVOLVE IMPROVE\0".as_ptr() as *const c_char).is_null() {
            gk.rotate = TRUE;
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut gk.rotateimprove,
            );
        } else if !strstr(command, b"REVOLVE RADIUS\0".as_ptr() as *const c_char).is_null() {
            // NOTE: unreachable (same test as the first arm); kept for C shape.
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut gk.polarradius,
            );
        } else if !strstr(command, b"REVOLVE CURVE DIRECT\0".as_ptr() as *const c_char).is_null() {
            gk.rotatecurve = TRUE;
            sscanf(params, b"%le\0".as_ptr() as *const c_char, &mut gk.curvezet);
        } else if !strstr(command, b"REVOLVE CURVE RADIUS\0".as_ptr() as *const c_char).is_null() {
            gk.rotatecurve = TRUE;
            sscanf(params, b"%le\0".as_ptr() as *const c_char, &mut gk.curverad);
        } else if !strstr(command, b"REVOLVE CURVE ANGLE\0".as_ptr() as *const c_char).is_null() {
            gk.rotatecurve = TRUE;
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut gk.curveangle,
            );
        }
    } else if !strstr(
        command,
        b"REDUCE ORDER INTERVAL\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        sscanf(
            params,
            b"%d%d\0".as_ptr() as *const c_char,
            &mut gk.reduceordermatmin,
            &mut gk.reduceordermatmax,
        );
    } else if !strstr(command, b"BOUNDARY DEFINITION\0".as_ptr() as *const c_char).is_null() {
        printf(b"Loading boundary conditions\n\0".as_ptr() as *const c_char);
        *i = 0;
        while *i < MAXBOUNDARIES as c_int {
            if *i > 0 {
                Getline(params, input);
            }
            *j = 0;
            while *j < MAXLINESIZE as c_int {
                *params.wrapping_offset(*j as isize) =
                    toupper(*params.wrapping_offset(*j as isize) as c_int) as c_char;
                *j += 1;
            }
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            sscanf(
                params,
                b"%d %d %d %d\0".as_ptr() as *const c_char,
                &mut gk.boundtype[*i as usize],
                &mut gk.boundext[*i as usize],
                &mut gk.boundint[*i as usize],
                &mut gk.boundsolid[*i as usize],
            );
            *i += 1;
        }
        printf(b"Found %d boundaries\n\0".as_ptr() as *const c_char, *i);
        gk.noboundaries = *i;
    } else {
        load_cmd3(grid, k, cp, i, j, elemcode, command, params, input, info);
    }
}
// SAFETY: modern-.grd command arms, element/mapping tail (split).
// NOTE: the block-commented LAYERED-BOUNDARIES and EXTRUDED-STRUCTURE
// arms are dead in C and omitted.
unsafe fn load_cmd3(
    grid: *mut *mut GridType,
    k: &mut c_int,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    elemcode: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    let _ = info;
    let gk = &mut (*(*grid).wrapping_offset(*k as isize));
    if !strstr(command, b"NUMBERING\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"HORIZONTAL\0".as_ptr() as *const c_char).is_null() {
            gk.numbering = NUMBER_XY;
        }
        if !strstr(params, b"VERTICAL\0".as_ptr() as *const c_char).is_null() {
            gk.numbering = NUMBER_YX;
        }
    } else if !strstr(command, b"ELEMENT DEGREE\0".as_ptr() as *const c_char).is_null() {
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut gk.elemorder);
    } else if !strstr(command, b"ELEMENT INNERNODES\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            gk.elemmidpoints = TRUE;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            gk.elemmidpoints = FALSE;
        }
    } else if !strstr(command, b"ELEMENTTYPE\0".as_ptr() as *const c_char).is_null()
        || !strstr(command, b"ELEMENTCODE\0".as_ptr() as *const c_char).is_null()
    {
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut (*elemcode));
    } else if !strstr(
        command,
        b"TRIANGLES CRITICAL ANGLE\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut gk.triangleangle,
        );
    } else if !strstr(command, b"TRIANGLES\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            gk.triangles = TRUE;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            gk.triangles = FALSE;
        }
    } else if !strstr(command, b"PLANE ELEMENTS\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.wantedelems,
        );
    } else if !strstr(command, b"SURFACE ELEMENTS\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.wantedelems,
        );
    } else if !strstr(command, b"REFERENCE DENSITY\0".as_ptr() as *const c_char).is_null() {
        sscanf(params, b"%le\0".as_ptr() as *const c_char, &mut gk.limitdx);
        gk.autoratio = 3;
    } else if !strstr(command, b"VERIFY DENSITY\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            gk.limitdxverify = TRUE;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            gk.limitdxverify = FALSE;
        }
    } else if !strstr(command, b"VOLUME ELEMENTS\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.wantedelems3d,
        );
    } else if !strstr(command, b"VOLUME NODES\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut gk.wantednodes3d,
        );
    } else if !strstr(command, b"COORDINATE RATIO\0".as_ptr() as *const c_char).is_null() {
        if gk.dimension == 2 {
            sscanf(params, b"%le\0".as_ptr() as *const c_char, &mut gk.xyratio);
        }
        if gk.dimension == 3 {
            sscanf(
                params,
                b"%le %le\0".as_ptr() as *const c_char,
                &mut gk.xyratio,
                &mut gk.xzratio,
            );
        }
    } else if !strstr(command, b"ELEMENT RATIOS 1\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.xcells {
            gk.xexpand[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT RATIOS 2\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.ycells {
            gk.yexpand[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT RATIOS 3\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.zcells {
            gk.zexpand[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT DENSITIES 1\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.xcells {
            gk.xdens[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT DENSITIES 2\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.ycells {
            gk.ydens[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT DENSITIES 3\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.zcells {
            gk.zdens[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"ELEMENT DIVISIONS 1\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.xcells {
            gk.xelems[*i as usize] = next_int(&mut (*cp));
            *i += 1;
        }
        gk.autoratio = 0;
    } else if !strstr(command, b"ELEMENT DIVISIONS 2\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.ycells {
            gk.yelems[*i as usize] = next_int(&mut (*cp));
            *i += 1;
        }
        gk.autoratio = 0;
    } else if !strstr(command, b"ELEMENT DIVISIONS 3\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= gk.zcells {
            gk.zelems[*i as usize] = next_int(&mut (*cp));
            *i += 1;
        }
        gk.autoratio = 0;
    } else if !strstr(command, b"GEOMETRY MAPPINGS\0".as_ptr() as *const c_char).is_null() {
        if *k > 0 {
            gk.mappings = 0;
        }
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        *i = gk.mappings;
        while *i < MAXMAPPINGS as c_int {
            if *i > gk.mappings {
                Getline(params, input);
            }
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            gk.mappingtype[*i as usize] = next_int(&mut (*cp));
            // NOTE: `#if 0` SGN adjustment omitted (runtime-dead as in C).
            gk.mappingline[*i as usize] = next_int(&mut (*cp));
            gk.mappinglimits[(2 * *i) as usize] = next_real(&mut (*cp));
            gk.mappinglimits[(2 * *i + 1) as usize] = next_real(&mut (*cp));
            gk.mappingpoints[*i as usize] = next_int(&mut (*cp));
            // NOTE: C `Rvector` is a macro for `dvector` (egutils.h:15).
            gk.mappingparams[*i as usize] = dvector(0, gk.mappingpoints[*i as usize]);
            *j = 0;
            while *j < gk.mappingpoints[*i as usize] {
                *gk.mappingparams[*i as usize].wrapping_offset(*j as isize) = next_real(&mut (*cp));
                *j += 1;
            }
            *i += 1;
        }
        printf(
            b"Loaded %d geometry mappings\n\0".as_ptr() as *const c_char,
            *i,
        );
        gk.mappings = *i;
    } else if !strstr(command, b"END\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    } else {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    }
}
// SAFETY: parameter-default initializer over a valid ElmergridType.
#[no_mangle]
pub unsafe extern "C" fn InitParameters(eg: *mut ElmergridType) {
    let mut i: c_int;
    (*eg).relh = 1.0;
    (*eg).inmethod = 0;
    (*eg).outmethod = 0;
    (*eg).silent = FALSE;
    (*eg).binary = 0;
    (*eg).nofilesin = 1;
    (*eg).unitemeshes = FALSE;
    (*eg).unitenooverlap = FALSE;
    (*eg).triangles = FALSE;
    (*eg).triangleangle = 0.0;
    (*eg).rotate = FALSE;
    (*eg).polar = FALSE;
    (*eg).cylinder = FALSE;
    (*eg).usenames = TRUE;
    (*eg).layers = 0;
    (*eg).layereps = 0.0;
    (*eg).layermove = 0;
    (*eg).partitions = 0;
    (*eg).elements3d = 0;
    (*eg).nodes3d = 0;
    (*eg).metis = 0;
    (*eg).metis_contig = FALSE;
    (*eg).metis_volcut = FALSE;
    (*eg).metis_seed = 0;
    (*eg).metis_ncuts = 1;
    (*eg).metis_minconn = FALSE;
    (*eg).partopt = 0;
    (*eg).partoptim = FALSE;
    (*eg).partbcoptim = TRUE;
    (*eg).partjoin = 0;
    i = 0;
    while i < MAXHALOMODES as c_int {
        (*eg).parthalo[i as usize] = FALSE;
        i += 1;
    }
    (*eg).reduce = FALSE;
    (*eg).increase = FALSE;
    (*eg).translate = FALSE;
    (*eg).isoparam = FALSE;
    (*eg).multidim = FALSE;
    (*eg).removelowdim = FALSE;
    (*eg).removeintbcs = FALSE;
    (*eg).removeunused = FALSE;
    (*eg).dim = 3;
    (*eg).center = FALSE;
    (*eg).scale = FALSE;
    (*eg).order = FALSE;
    (*eg).boundbounds = 0;
    (*eg).saveinterval[0] = 0;
    (*eg).saveinterval[1] = 0;
    (*eg).saveinterval[2] = 0;
    (*eg).bulkbounds = 0;
    (*eg).partorder = FALSE;
    (*eg).findsides = FALSE;
    (*eg).parthypre = FALSE;
    (*eg).partdual = FALSE;
    (*eg).partbcz = 0;
    (*eg).partbcr = 0;
    (*eg).partbclayers = 1;
    (*eg).partbcmetis = 0;
    (*eg).partbw = FALSE;
    (*eg).saveboundaries = TRUE;
    (*eg).vtuone = FALSE;
    (*eg).timeron = FALSE;
    (*eg).nosave = FALSE;
    (*eg).nooverwrite = FALSE;
    (*eg).merge = FALSE;
    (*eg).bcoffset = FALSE;
    (*eg).periodic = 0;
    (*eg).periodicdim[0] = 0;
    (*eg).periodicdim[1] = 0;
    (*eg).periodicdim[2] = 0;
    (*eg).bulkorder = FALSE;
    (*eg).boundorder = FALSE;
    (*eg).sidemappings = 0;
    (*eg).bulkmappings = 0;
    (*eg).coordinatemap[0] = 0;
    (*eg).coordinatemap[1] = 0;
    (*eg).coordinatemap[2] = 0;
    (*eg).clone[0] = 0;
    (*eg).clone[1] = 0;
    (*eg).clone[2] = 0;
    (*eg).clonesize[0] = 0.0;
    (*eg).clonesize[1] = 0.0;
    (*eg).clonesize[2] = 0.0;
    (*eg).mirror[0] = 0;
    (*eg).mirror[1] = 0;
    (*eg).mirror[2] = 0;
    (*eg).cloneinds = FALSE;
    (*eg).mirrorbc = 0;
    (*eg).decimals = 12;
    (*eg).discont = 0;
    (*eg).connect = 0;
    (*eg).connectboundsnosets = 0;
    (*eg).rotatecurve = FALSE;
    (*eg).curverad = 0.5;
    (*eg).curveangle = 90.0;
    (*eg).curvezet = 0.0;
    (*eg).parttol = 0.0;
    (*eg).filerenamed = FALSE;
    i = 0;
    while i < MAXSIDEBULK as c_int {
        (*eg).sidebulk[i as usize] = 0;
        i += 1;
    }
}
// NOTE: C file-local defines (egnative.c:65,92).
const MAXINMETHODS: c_int = 21;
const MAXOUTMETHODS: c_int = 5;

// SAFETY: inline-argument parser over valid eg + argv; C status codes.
#[no_mangle]
pub unsafe extern "C" fn InlineParameters(
    eg: *mut ElmergridType,
    argc: c_int,
    argv: *mut *mut c_char,
    first: c_int,
    mut info: c_int,
) -> c_int {
    let mut arg: c_int;
    // NOTE: 0-init for the &mut handoff (set in the arg arms before reads).
    let mut i: c_int = 0;
    let mut dim: c_int;
    let mut command: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    dim = (*eg).dim;
    printf(b"Elmergrid reading in-line arguments\n\0".as_ptr() as *const c_char);
    // NOTE: type of input file.
    if first > 3 {
        i = 0;
        while i < MAXLINESIZE as c_int {
            command[i as usize] = ' ' as c_char;
            i += 1;
        }
        strcpy(command.as_mut_ptr(), *argv.wrapping_offset(1));
        i = 0;
        while i < MAXLINESIZE as c_int {
            command[i as usize] = toupper(command[i as usize] as c_int) as c_char;
            i += 1;
        }
        i = 0;
        while i <= MAXINMETHODS {
            if !strstr(command.as_ptr(), InMethods[i as usize]).is_null() {
                (*eg).inmethod = i;
                break;
            }
            i += 1;
        }
        if i > MAXINMETHODS {
            (*eg).inmethod = atoi(*argv.wrapping_offset(1));
        }
        // NOTE: type of output file (fewer options).
        strcpy(command.as_mut_ptr(), *argv.wrapping_offset(2));
        i = 0;
        while i < MAXLINESIZE as c_int {
            command[i as usize] = toupper(command[i as usize] as c_int) as c_char;
            i += 1;
        }
        i = 1;
        while i <= MAXOUTMETHODS {
            if !strstr(command.as_ptr(), OutMethods[i as usize]).is_null() {
                (*eg).outmethod = i;
                break;
            }
            i += 1;
        }
        if i > MAXOUTMETHODS {
            (*eg).outmethod = atoi(*argv.wrapping_offset(2));
        }
        // NOTE: name of output file.
        strcpy((*eg).filesin[0].as_mut_ptr(), *argv.wrapping_offset(3));
        strcpy((*eg).filesout[0].as_mut_ptr(), (*eg).filesin[0].as_ptr());
        strcpy((*eg).infofile.as_mut_ptr(), (*eg).filesin[0].as_ptr());
    }
    // NOTE: the optional inline parameters.
    arg = first;
    while arg < argc {
        // NOTE: nonzero return = early C `return(code)`.
        let rc = inline_arg(eg, argc, argv, arg, &mut dim, &mut info, &mut i);
        if rc != 0 {
            return rc;
        }
        arg += 1;
    }
    {
        let mut badpoint: c_int;
        let ptr1: *mut c_char;
        let ptr2: *mut c_char;
        ptr1 = strrchr((*eg).filesout[0].as_ptr(), '.' as c_int);
        if !ptr1.is_null() {
            badpoint = FALSE;
            ptr2 = strrchr((*eg).filesout[0].as_ptr(), '/' as c_int);
            if !ptr2.is_null() && ptr2 > ptr1 {
                badpoint = TRUE;
            }
            if badpoint == 0 {
                *ptr1 = 0 as c_char;
            }
        }
    }
    printf(
        b"Output will be saved to file %s.\n\0".as_ptr() as *const c_char,
        (*eg).filesout[0].as_ptr(),
    );
    0
}
// SAFETY: one inline argument (split for sizing); nonzero = early return.
unsafe fn inline_arg(
    eg: *mut ElmergridType,
    argc: c_int,
    argv: *mut *mut c_char,
    arg: c_int,
    dim: &mut c_int,
    info: &mut c_int,
    i: &mut c_int,
) -> c_int {
    // NOTE: C `argv[arg]` shorthand used throughout.
    let a = *argv.wrapping_offset(arg as isize);
    if strcmp(a, b"-silent\0".as_ptr() as *const c_char) == 0 {
        (*eg).silent = TRUE;
        *info = FALSE;
    } else if strcmp(a, b"-verbose\0".as_ptr() as *const c_char) == 0 {
        (*eg).silent = FALSE;
        *info = TRUE;
    } else if strcmp(a, b"-in\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The secondary input file name is required as a parameter\n\0".as_ptr()
                    as *const c_char,
            );
            return 1;
        } else {
            strcpy(
                (*eg).filesin[(*eg).nofilesin as usize].as_mut_ptr(),
                *argv.wrapping_offset((arg + 1) as isize),
            );
            printf(
                b"A secondary input file %s will be loaded.\n\0".as_ptr() as *const c_char,
                (*eg).filesin[(*eg).nofilesin as usize].as_ptr(),
            );
            (*eg).nofilesin += 1;
        }
    } else if strcmp(a, b"-out\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"The output name is required as a parameter\n\0".as_ptr() as *const c_char);
            return 2;
        } else {
            strcpy(
                (*eg).filesout[0].as_mut_ptr(),
                *argv.wrapping_offset((arg + 1) as isize),
            );
            (*eg).filerenamed = TRUE;
        }
    } else if strcmp(a, b"-bin\0".as_ptr() as *const c_char) == 0 {
        (*eg).binary = 1;
    } else if strcmp(a, b"-sbin\0".as_ptr() as *const c_char) == 0 {
        (*eg).binary = 2;
    } else if strcmp(a, b"-decimals\0".as_ptr() as *const c_char) == 0 {
        (*eg).decimals = atoi(*argv.wrapping_offset((arg + 1) as isize));
    } else if strcmp(a, b"-triangles\0".as_ptr() as *const c_char) == 0 {
        (*eg).triangles = TRUE;
        printf(b"The rectangles will be split to triangles.\n\0".as_ptr() as *const c_char);
        if arg + 1 < argc
            && strcmp(
                *argv.wrapping_offset((arg + 1) as isize),
                b"-\0".as_ptr() as *const c_char,
            ) != 0
        {
            (*eg).triangleangle = atof(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-merge\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"Give a parameter for critical distance.\n\0".as_ptr() as *const c_char);
            return 3;
        } else {
            (*eg).merge = TRUE;
            (*eg).cmerge = atof(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-relh\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"Give a relative mesh density related to the specifications\n\0".as_ptr()
                    as *const c_char,
            );
            return 3;
        } else {
            (*eg).relh = atof(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-order\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give %d parameters for the order vector.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 4;
        } else {
            (*eg).order = TRUE;
            (*eg).corder[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).corder[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).corder[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-parttol\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"Give a tolerance for geometric partition algorithms\n\0".as_ptr()
                    as *const c_char,
            );
            return 3;
        } else {
            (*eg).parttol = atof(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-autoorder\0".as_ptr() as *const c_char) == 0 {
        (*eg).order = 2;
    } else if strcmp(a, b"-halo\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[1] = TRUE;
    } else if strcmp(a, b"-halobc\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[2] = TRUE;
    } else if strcmp(a, b"-halodb\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[1] = TRUE;
        (*eg).parthalo[2] = TRUE;
    } else if strcmp(a, b"-haloz\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[3] = TRUE;
    } else if strcmp(a, b"-halor\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[3] = TRUE;
    } else if strcmp(a, b"-halogreedy\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthalo[4] = TRUE;
    } else if strcmp(a, b"-metisorder\0".as_ptr() as *const c_char) == 0 {
        (*eg).order = 3;
    } else if strcmp(a, b"-centralize\0".as_ptr() as *const c_char) == 0 {
        (*eg).center = TRUE;
    } else if strcmp(a, b"-scale\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give %d parameters for the scaling.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 5;
        } else {
            (*eg).scale = TRUE;
            (*eg).cscale[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).cscale[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).cscale[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-translate\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give %d parameters for the translate vector.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 6;
        } else {
            (*eg).translate = TRUE;
            (*eg).ctranslate[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).ctranslate[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).ctranslate[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-saveinterval\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(b"Give min, max and step for the interval.\n\0".as_ptr() as *const c_char);
            return 7;
        } else {
            (*eg).saveinterval[0] = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).saveinterval[1] = atoi(*argv.wrapping_offset((arg + 2) as isize));
            (*eg).saveinterval[2] = atoi(*argv.wrapping_offset((arg + 3) as isize));
        }
    } else if strcmp(a, b"-rotate\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-rotate\0".as_ptr() as *const c_char) == 0
    {
        // NOTE: C tests "-rotate" twice (mirror of the original chain).
        if arg + *dim >= argc {
            printf(b"Give three parameters for the rotation angles.\n\0".as_ptr() as *const c_char);
            return 8;
        } else {
            (*eg).rotate = TRUE;
            (*eg).crotate[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).crotate[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            (*eg).crotate[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
        }
    } else {
        return inline_arg2(eg, argc, argv, arg, dim, i);
    }
    0
}
// SAFETY: inline arguments, clone/partition half (split for sizing).
unsafe fn inline_arg2(
    eg: *mut ElmergridType,
    argc: c_int,
    argv: *mut *mut c_char,
    arg: c_int,
    dim: &mut c_int,
    i: &mut c_int,
) -> c_int {
    let a = *argv.wrapping_offset(arg as isize);
    if strcmp(a, b"-clone\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give the number of clones in each %d directions.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 9;
        } else {
            (*eg).clone[0] = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).clone[1] = atoi(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).clone[2] = atoi(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-clonesize\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give the clone size in each %d directions.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 10;
        } else {
            (*eg).clonesize[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).clonesize[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).clonesize[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-cloneinds\0".as_ptr() as *const c_char) == 0 {
        (*eg).cloneinds = TRUE;
    } else if strcmp(a, b"-mirror\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give the symmetry of the coordinate directions, eg. 1 1 0\n\0".as_ptr()
                    as *const c_char,
            );
        } else {
            (*eg).mirror[0] = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).mirror[1] = atoi(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).mirror[2] = atoi(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-mirrorbc\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"Give the number of symmetry BC.\n\0".as_ptr() as *const c_char);
            return 11;
        } else {
            (*eg).mirrorbc = atoi(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-unite\0".as_ptr() as *const c_char) == 0 {
        (*eg).unitemeshes = TRUE;
        printf(b"The meshes will be united.\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-unitenooverlap\0".as_ptr() as *const c_char) == 0 {
        (*eg).unitemeshes = TRUE;
        (*eg).unitenooverlap = TRUE;
        printf(
            b"The meshes will be united without overlap in BCs or bodies.\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-nonames\0".as_ptr() as *const c_char) == 0 {
        (*eg).usenames = FALSE;
        printf(b"Names will be omitted even if they would exist\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-multidim\0".as_ptr() as *const c_char) == 0 {
        (*eg).multidim = TRUE;
        printf(b"Lower dimensional entities may be bulk too!\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-removelowdim\0".as_ptr() as *const c_char) == 0 {
        (*eg).removelowdim = TRUE;
        printf(b"Lower dimensional boundaries will be removed\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-removeintbcs\0".as_ptr() as *const c_char) == 0 {
        (*eg).removeintbcs = TRUE;
        printf(b"Lower dimensional boundaries will be removed\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-removeunused\0".as_ptr() as *const c_char) == 0 {
        (*eg).removeunused = TRUE;
        printf(
            b"Nodes that do not appear in any element will be removed\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-autoclean\0".as_ptr() as *const c_char) == 0 {
        (*eg).removelowdim = TRUE;
        (*eg).bulkorder = TRUE;
        (*eg).boundorder = TRUE;
        (*eg).removeunused = TRUE;
        printf(b"Lower dimensional boundaries will be removed\n\0".as_ptr() as *const c_char);
        printf(b"Materials and boundaries will be renumbered\n\0".as_ptr() as *const c_char);
        printf(
            b"Nodes that do not appear in any element will be removed\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-polar\0".as_ptr() as *const c_char) == 0 {
        (*eg).polar = TRUE;
        printf(b"Making transformation to polar coordinates.\n\0".as_ptr() as *const c_char);
        if arg + 1 >= argc {
            printf(
                b"The preferred radius is required as a parameter\n\0".as_ptr() as *const c_char,
            );
            (*eg).polarradius = 1.0;
        } else {
            // NOTE: C uses atoi (not atof) for the radius here.
            (*eg).polarradius = atoi(*argv.wrapping_offset((arg + 1) as isize)) as Real;
        }
    } else if strcmp(a, b"-cylinder\0".as_ptr() as *const c_char) == 0 {
        (*eg).cylinder = TRUE;
        printf(
            b"Making transformation from cylindrical to cartesian coordinates.\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-reduce\0".as_ptr() as *const c_char) == 0 {
        if arg + 2 >= argc {
            printf(b"Give two material for the interval.\n\0".as_ptr() as *const c_char);
            return 12;
        } else {
            (*eg).reduce = TRUE;
            (*eg).reducemat1 = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).reducemat2 = atoi(*argv.wrapping_offset((arg + 2) as isize));
        }
    } else if strcmp(a, b"-increase\0".as_ptr() as *const c_char) == 0 {
        (*eg).increase = TRUE;
    } else if strcmp(a, b"-bulkorder\0".as_ptr() as *const c_char) == 0 {
        (*eg).bulkorder = TRUE;
    } else if strcmp(a, b"-boundorder\0".as_ptr() as *const c_char) == 0 {
        (*eg).boundorder = TRUE;
    } else if strcmp(a, b"-partition\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-partcell\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-partcyl\0".as_ptr() as *const c_char) == 0
    {
        if arg + *dim >= argc {
            printf(
                b"The number of partitions in %d dims is required as parameters.\n\0".as_ptr()
                    as *const c_char,
                *dim,
            );
            return 13;
        } else {
            (*eg).partitions = 1;
            (*eg).partdim[0] = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).partdim[1] = atoi(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).partdim[2] = atoi(*argv.wrapping_offset((arg + 3) as isize));
            }
            (*eg).partitions = 1;
            *i = 0;
            while *i < 3 {
                if (*eg).partdim[*i as usize] == 0 {
                    (*eg).partdim[*i as usize] = 1;
                }
                (*eg).partitions *= (*eg).partdim[*i as usize];
                *i += 1;
            }
            (*eg).partopt = -1;
            if strcmp(a, b"-partition\0".as_ptr() as *const c_char) == 0 {
                if arg + 4 < argc
                    && *(*argv.wrapping_offset((arg + 4) as isize)).wrapping_offset(0)
                        != '-' as c_char
                {
                    (*eg).partopt = atoi(*argv.wrapping_offset((arg + 4) as isize));
                }
            } else if strcmp(a, b"-partcell\0".as_ptr() as *const c_char) == 0 {
                (*eg).partopt = 2;
            } else if strcmp(a, b"-partcyl\0".as_ptr() as *const c_char) == 0 {
                (*eg).partopt = 3;
            }
            printf(
                b"The mesh will be partitioned geometrically to %d partitions.\n\0".as_ptr()
                    as *const c_char,
                (*eg).partitions,
            );
        }
    } else if strcmp(a, b"-partorder\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give %d parameters for the order vector.\n\0".as_ptr() as *const c_char,
                *dim,
            );
            return 14;
        } else {
            (*eg).partorder = 1;
            (*eg).partcorder[0] = atof(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).partcorder[1] = atof(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).partcorder[2] = atof(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-partoptim\0".as_ptr() as *const c_char) == 0 {
        (*eg).partoptim = TRUE;
        printf(
            b"Aggressive optimization will be applied to node sharing.\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-partnobcoptim\0".as_ptr() as *const c_char) == 0 {
        (*eg).partbcoptim = FALSE;
        printf(
            b"Aggressive optimization will not be applied to parent element sharing.\n\0".as_ptr()
                as *const c_char,
        );
    } else if strcmp(a, b"-partbw\0".as_ptr() as *const c_char) == 0 {
        (*eg).partbw = TRUE;
        printf(b"Bandwidth will be optimized for partitions.\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-parthypre\0".as_ptr() as *const c_char) == 0 {
        (*eg).parthypre = TRUE;
        printf(b"Numbering of partitions will be made continuous.\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-partdual\0".as_ptr() as *const c_char) == 0 {
        (*eg).partdual = TRUE;
        printf(b"Using dual (elemental) graph in partitioning.\n\0".as_ptr() as *const c_char);
    } else if strcmp(a, b"-metis\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-metisrec\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-metiskway\0".as_ptr() as *const c_char) == 0
    {
        // NOTE: `#if USE_METIS` — live here (pairs build the METIS binary).
        if arg + 1 >= argc {
            printf(
                b"The number of partitions is required as a parameter\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).metis = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The mesh will be partitioned with Metis to %d partitions.\n\0".as_ptr()
                    as *const c_char,
                (*eg).metis,
            );
            (*eg).partopt = 0;
            if strcmp(a, b"-metisrec\0".as_ptr() as *const c_char) == 0 {
                (*eg).partopt = 2;
            } else if strcmp(a, b"-metiskway\0".as_ptr() as *const c_char) == 0 {
                (*eg).partopt = 3;
            } else if arg + 2 < argc
                && *(*argv.wrapping_offset((arg + 2) as isize)).wrapping_offset(0) != '-' as c_char
            {
                (*eg).partopt = atoi(*argv.wrapping_offset((arg + 2) as isize));
            }
        }
    } else {
        return inline_arg3(eg, argc, argv, arg, dim, i);
    }
    0
}
// SAFETY: inline arguments, metis-tail half (split for sizing).
unsafe fn inline_arg3(
    eg: *mut ElmergridType,
    argc: c_int,
    argv: *mut *mut c_char,
    arg: c_int,
    dim: &mut c_int,
    i: &mut c_int,
) -> c_int {
    let a = *argv.wrapping_offset(arg as isize);
    if strcmp(a, b"-metisseed\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The random number seed is required as parameter for -metisseed!\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).metis_seed = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"Seed for Metis partitioning routines: %d\n\0".as_ptr() as *const c_char,
                (*eg).metis_seed,
            );
        }
    } else if strcmp(a, b"-metisncuts\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The number of parameters is required as parameter for -metisncuts!\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).metis_ncuts = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"Number of competing partitions to generate : %d\n\0".as_ptr() as *const c_char,
                (*eg).metis_ncuts,
            );
        }
    } else if strcmp(a, b"-partjoin\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The number of partitions is required as a parameter!\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).partjoin = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The results will joined using %d partitions.\n\0".as_ptr() as *const c_char,
                (*eg).partjoin,
            );
        }
    } else if strcmp(a, b"-partconnect\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-partzbc\0".as_ptr() as *const c_char) == 0
    {
        if arg + 1 >= argc {
            printf(
                b"The number of 1D partitions is required as a parameter!\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).partbcz = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The connected BCs will be partitioned to %d partitions in Z.\n\0".as_ptr()
                    as *const c_char,
                (*eg).partbcz,
            );
        }
    } else if strcmp(a, b"-partrbc\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The number of 1D partitions is required as a parameter!\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).partbcr = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The connected BCs will be partitioned to %d partitions in R.\n\0".as_ptr()
                    as *const c_char,
                (*eg).partbcr,
            );
        }
    } else if strcmp(a, b"-partlayers\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(
                b"The number of layers to be extended is required as a parameter\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).partbclayers = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The boundary partitioning will be extended by %d layers.\n\0".as_ptr()
                    as *const c_char,
                (*eg).partbclayers,
            );
        }
    } else if strcmp(a, b"-metiscontig\0".as_ptr() as *const c_char) == 0 {
        (*eg).metis_contig = TRUE;
    } else if strcmp(a, b"-metisvol\0".as_ptr() as *const c_char) == 0 {
        (*eg).metis_volcut = TRUE;
    } else if strcmp(a, b"-metisminconn\0".as_ptr() as *const c_char) == 0 {
        (*eg).metis_minconn = TRUE;
    } else if strcmp(a, b"-metisconnect\0".as_ptr() as *const c_char) == 0
        || strcmp(a, b"-metisbc\0".as_ptr() as *const c_char) == 0
    {
        if arg + 1 >= argc {
            printf(
                b"The number of Metis partitions is required as a parameter\n\0".as_ptr()
                    as *const c_char,
            );
            return 15;
        } else {
            (*eg).partbcmetis = atoi(*argv.wrapping_offset((arg + 1) as isize));
            printf(
                b"The connected BCs will be partitioned to %d partitions by Metis.\n\0".as_ptr()
                    as *const c_char,
                (*eg).partbcmetis,
            );
        }
    } else if strcmp(a, b"-periodic\0".as_ptr() as *const c_char) == 0 {
        if arg + *dim >= argc {
            printf(
                b"Give the periodic coordinate directions (e.g. 1 1 0)\n\0".as_ptr()
                    as *const c_char,
            );
            return 16;
        } else {
            (*eg).periodicdim[0] = atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).periodicdim[1] = atoi(*argv.wrapping_offset((arg + 2) as isize));
            if *dim == 3 {
                (*eg).periodicdim[2] = atoi(*argv.wrapping_offset((arg + 3) as isize));
            }
        }
    } else if strcmp(a, b"-discont\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"Give the discontinuous boundary conditions.\n\0".as_ptr() as *const c_char);
            return 17;
        } else {
            (*eg).discontbounds[(*eg).discont as usize] =
                atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).discont += 1;
        }
    } else if strcmp(a, b"-connect\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"Give the connected boundary conditions.\n\0".as_ptr() as *const c_char);
            return 10;
        } else {
            (*eg).connectboundsnosets += 1;
            *i = arg + 1;
            while *i < argc
                && strncmp(
                    *argv.wrapping_offset(*i as isize),
                    b"-\0".as_ptr() as *const c_char,
                    1,
                ) != 0
            {
                (*eg).connectbounds[(*eg).connect as usize] =
                    atoi(*argv.wrapping_offset(*i as isize));
                (*eg).connectboundsset[(*eg).connect as usize] = (*eg).connectboundsnosets;
                (*eg).connect += 1;
                *i += 1;
            }
        }
    } else if strcmp(a, b"-connectall\0".as_ptr() as *const c_char) == 0 {
        (*eg).connectboundsnosets += 1;
        (*eg).connectbounds[(*eg).connect as usize] = -1;
        (*eg).connectboundsset[(*eg).connect as usize] = (*eg).connectboundsnosets;
        (*eg).connect += 1;
    } else if strcmp(a, b"-connectint\0".as_ptr() as *const c_char) == 0 {
        (*eg).connectboundsnosets += 1;
        (*eg).connectbounds[(*eg).connect as usize] = -2;
        (*eg).connectboundsset[(*eg).connect as usize] = (*eg).connectboundsnosets;
        (*eg).connect += 1;
    } else if strcmp(a, b"-connectfree\0".as_ptr() as *const c_char) == 0 {
        (*eg).connectboundsnosets += 1;
        (*eg).connectbounds[(*eg).connect as usize] = -3;
        (*eg).connectboundsset[(*eg).connect as usize] = (*eg).connectboundsnosets;
        (*eg).connect += 1;
    } else {
        return inline_arg4(eg, argc, argv, arg, dim, i);
    }
    0
}
// SAFETY: inline arguments, mapping/layer/misc tail (split for sizing).
// NOTE: the dirty-trick comment on -divlayer (negative layer indexing)
// is preserved below.
unsafe fn inline_arg4(
    eg: *mut ElmergridType,
    argc: c_int,
    argv: *mut *mut c_char,
    arg: c_int,
    dim: &mut c_int,
    i: &mut c_int,
) -> c_int {
    let a = *argv.wrapping_offset(arg as isize);
    if strcmp(a, b"-boundbound\0".as_ptr() as *const c_char) == 0 {
        *i = arg + 1;
        while *i <= arg + 3 && *i < argc {
            (*eg).boundbound[(3 * (*eg).boundbounds + *i - (1 + arg)) as usize] =
                atoi(*argv.wrapping_offset(*i as isize));
            if (*i - arg) % 3 == 0 {
                (*eg).boundbounds += 1;
            }
            *i += 1;
        }
    } else if strcmp(a, b"-bulkbound\0".as_ptr() as *const c_char) == 0 {
        *i = arg + 1;
        while *i <= arg + 3 && *i < argc {
            (*eg).bulkbound[(3 * (*eg).bulkbounds + *i - (1 + arg)) as usize] =
                atoi(*argv.wrapping_offset(*i as isize));
            if (*i - arg) % 3 == 0 {
                (*eg).bulkbounds += 1;
            }
            *i += 1;
        }
    } else if strcmp(a, b"-boundtype\0".as_ptr() as *const c_char) == 0 {
        *i = arg + 1;
        while *i < argc
            && strncmp(
                *argv.wrapping_offset(*i as isize),
                b"-\0".as_ptr() as *const c_char,
                1,
            ) != 0
        {
            (*eg).sidemap[(3 * (*eg).sidemappings + *i - 1 - arg) as usize] =
                atoi(*argv.wrapping_offset(*i as isize));
            *i += 1;
        }
        (*eg).sidemappings += 1;
    } else if strcmp(a, b"-bulktype\0".as_ptr() as *const c_char) == 0 {
        *i = arg + 1;
        while *i < argc
            && strncmp(
                *argv.wrapping_offset(*i as isize),
                b"-\0".as_ptr() as *const c_char,
                1,
            ) != 0
        {
            (*eg).bulkmap[(3 * (*eg).bulkmappings + *i - 1 - arg) as usize] =
                atoi(*argv.wrapping_offset(*i as isize));
            *i += 1;
        }
        (*eg).bulkmappings += 1;
    } else if strcmp(a, b"-coordinatemap\0".as_ptr() as *const c_char) == 0 {
        if arg + 3 >= argc {
            printf(
                b"Give three parameters for the index permutation\n\0".as_ptr() as *const c_char,
            );
            return 18;
        } else {
            *i = 0;
            while *i < 3 {
                (*eg).coordinatemap[*i as usize] =
                    atoi(*argv.wrapping_offset((arg + 1 + *i) as isize));
                *i += 1;
            }
        }
    } else if strcmp(a, b"-layer\0".as_ptr() as *const c_char) == 0 {
        if arg + 4 >= argc {
            printf(
                b"Give four parameters for the layer: boundary, elements, thickness, ratio.\n\0"
                    .as_ptr() as *const c_char,
            );
            return 18;
        } else if (*eg).layers == MAXBOUNDARIES as c_int {
            printf(
                b"There can only be %d layers, sorry.\n\0".as_ptr() as *const c_char,
                MAXBOUNDARIES as c_int,
            );
            return 19;
        } else {
            (*eg).layerbounds[(*eg).layers as usize] =
                atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).layernumber[(*eg).layers as usize] =
                atoi(*argv.wrapping_offset((arg + 2) as isize));
            (*eg).layerthickness[(*eg).layers as usize] =
                atof(*argv.wrapping_offset((arg + 3) as isize));
            (*eg).layerratios[(*eg).layers as usize] =
                atof(*argv.wrapping_offset((arg + 4) as isize));
            (*eg).layerparents[(*eg).layers as usize] = 0;
            (*eg).layers += 1;
        }
    } else if strcmp(a, b"-layermove\0".as_ptr() as *const c_char) == 0 {
        if arg + 1 >= argc {
            printf(b"Give maximum number of Jacobi filters.\n\0".as_ptr() as *const c_char);
            return 20;
        } else {
            (*eg).layermove = atoi(*argv.wrapping_offset((arg + 1) as isize));
        }
    } else if strcmp(a, b"-divlayer\0".as_ptr() as *const c_char) == 0 {
        // NOTE: very dirty trick — the -layer variables are used with
        // negative indexing here.
        if arg + 4 >= argc {
            printf(b"Give four parameters for the layer: boundary, elements, relative thickness, ratio.\n\0".as_ptr() as *const c_char);
            return 21;
        } else if abs((*eg).layers) == MAXBOUNDARIES as c_int {
            printf(
                b"There can only be %d layers, sorry.\n\0".as_ptr() as *const c_char,
                MAXBOUNDARIES as c_int,
            );
            return 22;
        } else {
            (*eg).layerbounds[abs((*eg).layers) as usize] =
                atoi(*argv.wrapping_offset((arg + 1) as isize));
            (*eg).layernumber[abs((*eg).layers) as usize] =
                atoi(*argv.wrapping_offset((arg + 2) as isize));
            (*eg).layerthickness[abs((*eg).layers) as usize] =
                atof(*argv.wrapping_offset((arg + 3) as isize));
            (*eg).layerratios[abs((*eg).layers) as usize] =
                atof(*argv.wrapping_offset((arg + 4) as isize));
            (*eg).layerparents[abs((*eg).layers) as usize] = 0;
            (*eg).layers -= 1;
        }
    } else if strcmp(a, b"-3d\0".as_ptr() as *const c_char) == 0 {
        (*eg).dim = 3;
        *dim = 3;
    } else if strcmp(a, b"-2d\0".as_ptr() as *const c_char) == 0 {
        (*eg).dim = 2;
        *dim = 2;
    } else if strcmp(a, b"-1d\0".as_ptr() as *const c_char) == 0 {
        (*eg).dim = 1;
        *dim = 1;
    } else if strcmp(a, b"-isoparam\0".as_ptr() as *const c_char) == 0 {
        (*eg).isoparam = TRUE;
    } else if strcmp(a, b"-nobound\0".as_ptr() as *const c_char) == 0 {
        (*eg).saveboundaries = FALSE;
    } else if strcmp(a, b"-vtuone\0".as_ptr() as *const c_char) == 0 {
        (*eg).vtuone = TRUE;
    } else if strcmp(a, b"-nosave\0".as_ptr() as *const c_char) == 0 {
        (*eg).nosave = TRUE;
    } else if strcmp(a, b"-nooverwrite\0".as_ptr() as *const c_char) == 0 {
        (*eg).nooverwrite = TRUE;
    } else if strcmp(a, b"-timer\0".as_ptr() as *const c_char) == 0 {
        (*eg).timeron = TRUE;
    } else if strcmp(a, b"-infofile\0".as_ptr() as *const c_char) == 0 {
        (*eg).timeron = TRUE;
        if arg + 1 >= argc {
            printf(b"The output name is required as a parameter\n\0".as_ptr() as *const c_char);
            return 2;
        } else {
            strcpy(
                (*eg).infofile.as_mut_ptr(),
                *argv.wrapping_offset((arg + 1) as isize),
            );
        }
    } else if strcmp(a, b"-bcoffset\0".as_ptr() as *const c_char) == 0 {
        // NOTE: not actively used (see C comment).
        (*eg).bcoffset = atoi(*argv.wrapping_offset((arg + 1) as isize));
    } else if strcmp(a, b"-noelements\0".as_ptr() as *const c_char) == 0 {
        (*eg).elements3d = atoi(*argv.wrapping_offset((arg + 1) as isize));
    } else if strcmp(a, b"-nonodes\0".as_ptr() as *const c_char) == 0 {
        (*eg).nodes3d = atoi(*argv.wrapping_offset((arg + 1) as isize));
    } else if strcmp(a, b"-sidefind\0".as_ptr() as *const c_char) == 0 {
        (*eg).findsides = 0;
        *i = arg + 1;
        while *i < argc
            && strncmp(
                *argv.wrapping_offset(*i as isize),
                b"-\0".as_ptr() as *const c_char,
                1,
            ) != 0
        {
            (*eg).sidebulk[(*i - 1 - arg) as usize] = atoi(*argv.wrapping_offset(*i as isize));
            (*eg).findsides += 1;
            *i += 1;
        }
    } else if strcmp(a, b"-findbound\0".as_ptr() as *const c_char) == 0 {
        (*eg).findsides = 0;
        *i = arg + 1;
        while *i + 1 < argc
            && strncmp(
                *argv.wrapping_offset(*i as isize),
                b"-\0".as_ptr() as *const c_char,
                1,
            ) != 0
        {
            (*eg).sidebulk[(*i - 1 - arg) as usize] = atoi(*argv.wrapping_offset(*i as isize));
            (*eg).sidebulk[(*i - arg) as usize] = atoi(*argv.wrapping_offset((*i + 1) as isize));
            (*eg).findsides += 1;
            *i += 2;
        }
    } else if strcmp(a, b"-\0".as_ptr() as *const c_char) == 0 {
        printf(
            b"Unknown in-line argument: %s\n\0".as_ptr() as *const c_char,
            a,
        );
        bigerror(b"Cannot deal with argument, aborting!\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: command-file reader over valid eg + grid; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadCommands(
    prefix: *mut c_char,
    eg: *mut ElmergridType,
    grid: *mut GridType,
    mode: c_int,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut command: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut params: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init for the &mut handoff (set before every next_* call).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut input: *mut FILE = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in the command arms before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the command arms before reads).
    let mut j: c_int = 0;
    let iostat: c_int;
    iodebug = FALSE;
    if mode == 0 {
        input = fopen(
            b"ELMERGRID_STARTINFO\0".as_ptr() as *const c_char,
            b"r\0".as_ptr() as *const c_char,
        );
        if !input.is_null() {
            iostat = fscanf(
                input,
                b"%s\0".as_ptr() as *const c_char,
                filename.as_mut_ptr(),
            );
            let _ = iostat;
            fclose(input);
            printf(
                b"Using the file %s defined in ELMERGRID_STARTINFO\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
            if input.is_null() {
                printf(
                    b"LoadCommands: opening of the file '%s' wasn't successful !\n\0".as_ptr()
                        as *const c_char,
                    filename.as_ptr(),
                );
                return 1;
            } else {
                printf(
                    b"Loading ElmerGrid commands from file '%s'.\n\0".as_ptr() as *const c_char,
                    filename.as_ptr(),
                );
            }
        } else {
            return 2;
        }
    } else if mode == 1 {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"eg\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadCommands: opening of the file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 3;
        }
        if info != 0 {
            printf(
                b"Loading ElmerGrid commands from file '%s'.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
    } else if mode == 2 {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"grd\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadCommands: opening of the file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 4;
        }
        if info != 0 {
            printf(
                b"\nLoading ElmerGrid commands from file '%s'.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
    }
    loop {
        if GetCommand(command.as_mut_ptr(), params.as_mut_ptr(), input) != 0 {
            printf(b"Reached the end of command file\n\0".as_ptr() as *const c_char);
            break;
        }
        // NOTE: with command-file modes the file info also comes from
        // the command file.
        if mode <= 1 {
            if !strstr(command.as_ptr(), b"INPUT FILE\0".as_ptr() as *const c_char).is_null() {
                sscanf(
                    params.as_ptr(),
                    b"%s\0".as_ptr() as *const c_char,
                    (*eg).filesin[0].as_mut_ptr(),
                );
            } else if !strstr(command.as_ptr(), b"OUTPUT FILE\0".as_ptr() as *const c_char)
                .is_null()
            {
                sscanf(
                    params.as_ptr(),
                    b"%s\0".as_ptr() as *const c_char,
                    (*eg).filesout[0].as_mut_ptr(),
                );
                (*eg).filerenamed = TRUE;
            } else if !strstr(command.as_ptr(), b"INPUT MODE\0".as_ptr() as *const c_char).is_null()
            {
                j = 0;
                while j < MAXLINESIZE as c_int {
                    params[j as usize] = toupper(params[j as usize] as c_int) as c_char;
                    j += 1;
                }
                i = 0;
                while i <= MAXINMETHODS {
                    if !strstr(params.as_ptr(), InMethods[i as usize]).is_null() {
                        (*eg).inmethod = i;
                        break;
                    }
                    i += 1;
                }
                if i > MAXINMETHODS {
                    sscanf(
                        params.as_ptr(),
                        b"%d\0".as_ptr() as *const c_char,
                        &mut (*eg).inmethod,
                    );
                }
            } else if !strstr(command.as_ptr(), b"OUTPUT MODE\0".as_ptr() as *const c_char)
                .is_null()
            {
                j = 0;
                while j < MAXLINESIZE as c_int {
                    params[j as usize] = toupper(params[j as usize] as c_int) as c_char;
                    j += 1;
                }
                // NOTE: type of output file (fewer options).
                i = 1;
                while i <= MAXOUTMETHODS {
                    if !strstr(params.as_ptr(), OutMethods[i as usize]).is_null() {
                        (*eg).outmethod = i;
                        break;
                    }
                    i += 1;
                }
                if i > MAXOUTMETHODS {
                    sscanf(
                        params.as_ptr(),
                        b"%d\0".as_ptr() as *const c_char,
                        &mut (*eg).outmethod,
                    );
                }
            }
        }
        // NOTE: end of command-file specific part.
        load_ccmd(
            eg,
            grid,
            &mut cp,
            &mut i,
            &mut j,
            command.as_mut_ptr(),
            params.as_mut_ptr(),
            input,
        );
    }
    printf(b"Read commands from a file\n\0".as_ptr() as *const c_char);
    fclose(input);
    0
}
// SAFETY: command-file arms, mesh-operation half (split for sizing).
unsafe fn load_ccmd(
    eg: *mut ElmergridType,
    grid: *mut GridType,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
) {
    if !strstr(command, b"DECIMALS\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*eg).decimals,
        );
    } else if !strstr(command, b"BOUNDARY OFFSET\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*eg).bcoffset,
        );
    } else if !strstr(
        command,
        b"TRIANGLES CRITICAL ANGLE\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut (*eg).triangleangle,
        );
    } else if !strstr(command, b"TRIANGLES\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).triangles = TRUE;
        }
    } else if !strstr(command, b"MERGE NODES\0".as_ptr() as *const c_char).is_null() {
        (*eg).merge = TRUE;
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut (*eg).cmerge,
        );
    } else if !strstr(command, b"UNITE\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).unitemeshes = TRUE;
        }
    } else if !strstr(command, b"UNITENOOVERLAP\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).unitemeshes = TRUE;
            (*eg).unitenooverlap = TRUE;
        }
    } else if !strstr(command, b"ORDER NODES\0".as_ptr() as *const c_char).is_null() {
        (*eg).order = TRUE;
        if (*eg).dim == 1 {
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut (*eg).corder[0],
            );
        } else if (*eg).dim == 2 {
            sscanf(
                params,
                b"%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).corder[0],
                &mut (*eg).corder[1],
            );
        } else if (*eg).dim == 3 {
            sscanf(
                params,
                b"%le%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).corder[0],
                &mut (*eg).corder[1],
                &mut (*eg).corder[2],
            );
        }
    } else if !strstr(command, b"SCALE\0".as_ptr() as *const c_char).is_null() {
        (*eg).scale = TRUE;
        if (*eg).dim == 1 {
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut (*eg).cscale[0],
            );
        } else if (*eg).dim == 2 {
            sscanf(
                params,
                b"%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).cscale[0],
                &mut (*eg).cscale[1],
            );
        } else if (*eg).dim == 3 {
            sscanf(
                params,
                b"%le%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).cscale[0],
                &mut (*eg).cscale[1],
                &mut (*eg).cscale[2],
            );
        }
    } else if !strstr(command, b"CENTRALIZE\0".as_ptr() as *const c_char).is_null() {
        (*eg).center = TRUE;
    } else if !strstr(command, b"TRANSLATE\0".as_ptr() as *const c_char).is_null() {
        (*eg).translate = TRUE;
        if (*eg).dim == 1 {
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut (*eg).ctranslate[0],
            );
        } else if (*eg).dim == 2 {
            sscanf(
                params,
                b"%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).ctranslate[0],
                &mut (*eg).ctranslate[1],
            );
        } else if (*eg).dim == 3 {
            sscanf(
                params,
                b"%le%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).ctranslate[0],
                &mut (*eg).ctranslate[1],
                &mut (*eg).ctranslate[2],
            );
        }
    } else if !strstr(command, b"ROTATE MESH\0".as_ptr() as *const c_char).is_null() {
        (*eg).rotate = TRUE;
        sscanf(
            params,
            b"%le%le%le\0".as_ptr() as *const c_char,
            &mut (*eg).crotate[0],
            &mut (*eg).crotate[1],
            &mut (*eg).crotate[2],
        );
    } else if !strstr(command, b"CLONE\0".as_ptr() as *const c_char).is_null() {
        if !strstr(command, b"CLONE SIZE\0".as_ptr() as *const c_char).is_null() {
            if (*eg).dim == 1 {
                sscanf(
                    params,
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*eg).clonesize[0],
                );
            } else if (*eg).dim == 2 {
                sscanf(
                    params,
                    b"%le%le\0".as_ptr() as *const c_char,
                    &mut (*eg).clonesize[0],
                    &mut (*eg).clonesize[1],
                );
            } else if (*eg).dim == 3 {
                sscanf(
                    params,
                    b"%le%le%le\0".as_ptr() as *const c_char,
                    &mut (*eg).clonesize[0],
                    &mut (*eg).clonesize[1],
                    &mut (*eg).clonesize[2],
                );
            }
        } else if (*eg).dim == 1 {
            sscanf(
                params,
                b"%d\0".as_ptr() as *const c_char,
                &mut (*eg).clone[0],
            );
        } else if (*eg).dim == 2 {
            sscanf(
                params,
                b"%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).clone[0],
                &mut (*eg).clone[1],
            );
        } else if (*eg).dim == 3 {
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).clone[0],
                &mut (*eg).clone[1],
                &mut (*eg).clone[2],
            );
        }
    } else if !strstr(command, b"MERGE\0".as_ptr() as *const c_char).is_null() {
        (*eg).merge = TRUE;
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut (*eg).cmerge,
        );
    } else if !strstr(command, b"MIRROR\0".as_ptr() as *const c_char).is_null() {
        if (*eg).dim == 1 {
            sscanf(
                params,
                b"%d\0".as_ptr() as *const c_char,
                &mut (*eg).mirror[0],
            );
        } else if (*eg).dim == 2 {
            sscanf(
                params,
                b"%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).mirror[0],
                &mut (*eg).mirror[1],
            );
        } else if (*eg).dim == 3 {
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).mirror[0],
                &mut (*eg).mirror[1],
                &mut (*eg).mirror[2],
            );
        }
    } else if !strstr(command, b"POLAR RADIUS\0".as_ptr() as *const c_char).is_null() {
        (*eg).polar = TRUE;
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut (*eg).polarradius,
        );
    } else if !strstr(command, b"CYLINDER\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).cylinder = TRUE;
        }
    } else if !strstr(command, b"REDUCE DEGREE\0".as_ptr() as *const c_char).is_null() {
        (*eg).reduce = TRUE;
        sscanf(
            params,
            b"%d%d\0".as_ptr() as *const c_char,
            &mut (*eg).reducemat1,
            &mut (*eg).reducemat2,
        );
    } else if !strstr(command, b"INCREASE DEGREE\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).increase = TRUE;
        }
    } else if !strstr(command, b"METIS OPTION\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `#if USE_METIS` — live here (pairs build the METIS binary).
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*eg).partopt,
        );
    } else if !strstr(command, b"METIS\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `#if USE_METIS` — live here.
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut (*eg).metis);
    } else if !strstr(command, b"METISKWAY\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `#if USE_METIS` — live here.
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut (*eg).metis);
        (*eg).partopt = 3;
    } else if !strstr(command, b"METISREC\0".as_ptr() as *const c_char).is_null() {
        // NOTE: `#if USE_METIS` — live here.
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut (*eg).metis);
        (*eg).partopt = 2;
    } else {
        load_ccmd2(eg, grid, cp, i, j, command, params, input);
    }
}

// SAFETY: params uppercaser (C `for(j...) params[j] = toupper(...)` idiom).
unsafe fn upper(params: *mut c_char, j: &mut c_int) {
    *j = 0;
    while *j < MAXLINESIZE as c_int {
        *params.wrapping_offset(*j as isize) =
            toupper(*params.wrapping_offset(*j as isize) as c_int) as c_char;
        *j += 1;
    }
}
// SAFETY: command-file arms, partition/mapping/extrude tail (split).
unsafe fn load_ccmd2(
    eg: *mut ElmergridType,
    grid: *mut GridType,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
) {
    if !strstr(command, b"PARTITION DUAL\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).partdual = TRUE;
        }
    } else if !strstr(command, b"PARTJOIN\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*eg).partjoin,
        );
    } else if !strstr(command, b"PARTITION ORDER\0".as_ptr() as *const c_char).is_null() {
        (*eg).partorder = 1;
        if (*eg).dim == 2 {
            sscanf(
                params,
                b"%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).partcorder[0],
                &mut (*eg).partcorder[1],
            );
        }
        if (*eg).dim == 3 {
            sscanf(
                params,
                b"%le%le%le\0".as_ptr() as *const c_char,
                &mut (*eg).partcorder[0],
                &mut (*eg).partcorder[1],
                &mut (*eg).partcorder[2],
            );
        }
    } else if !strstr(command, b"PARTITION\0".as_ptr() as *const c_char).is_null()
        || !strstr(command, b"PARTCYL\0".as_ptr() as *const c_char).is_null()
        || !strstr(command, b"PARTCELL\0".as_ptr() as *const c_char).is_null()
    {
        if (*eg).dim == 2 {
            sscanf(
                params,
                b"%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).partdim[0],
                &mut (*eg).partdim[1],
            );
        }
        if (*eg).dim == 3 {
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).partdim[0],
                &mut (*eg).partdim[1],
                &mut (*eg).partdim[2],
            );
        }
        (*eg).partitions = 1;
        *i = 0;
        while *i < (*eg).dim {
            if (*eg).partdim[*i as usize] < 1 {
                (*eg).partdim[*i as usize] = 1;
            }
            (*eg).partitions *= (*eg).partdim[*i as usize];
            *i += 1;
        }
        if !strstr(command, b"PARTCYL\0".as_ptr() as *const c_char).is_null() {
            (*eg).partopt = 3;
        }
        // NOTE: C tests "PARTCCELL" (double C); mirrored exactly.
        if !strstr(command, b"PARTCCELL\0".as_ptr() as *const c_char).is_null() {
            (*eg).partopt = 2;
        }
    } else if !strstr(command, b"PERIODIC\0".as_ptr() as *const c_char).is_null() {
        if (*eg).dim == 2 {
            sscanf(
                params,
                b"%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).periodicdim[0],
                &mut (*eg).periodicdim[1],
            );
        }
        if (*eg).dim == 3 {
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).periodicdim[0],
                &mut (*eg).periodicdim[1],
                &mut (*eg).periodicdim[2],
            );
        }
    } else if !strstr(command, b"HALO\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).parthalo[1] = TRUE;
        }
    } else if !strstr(command, b"BOUNDARY HALO\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).parthalo[2] = TRUE;
        }
    } else if !strstr(command, b"EXTRUDED HALO\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).parthalo[3] = TRUE;
        }
    } else if !strstr(command, b"GREEDY HALO\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).parthalo[4] = TRUE;
        }
    } else if !strstr(command, b"PARTBW\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).partbw = TRUE;
        }
    } else if !strstr(command, b"PARTHYPRE\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).parthypre = TRUE;
        }
    } else {
        load_ccmd3(eg, grid, cp, i, j, command, params, input);
    }
}
// SAFETY: command-file arms, mappings half (split for sizing).
unsafe fn load_ccmd3(
    eg: *mut ElmergridType,
    grid: *mut GridType,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
) {
    if !strstr(
        command,
        b"BOUNDARY TYPE MAPPINGS\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        *i = 0;
        while *i < MAXMAPPINGS as c_int {
            if *i > 0 {
                Getline(params, input);
            }
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).sidemap[(3 * *i) as usize],
                &mut (*eg).sidemap[(3 * *i + 1) as usize],
                &mut (*eg).sidemap[(3 * *i + 2) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d boundary type mappings\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).sidemappings = *i;
    } else if !strstr(command, b"BULK TYPE MAPPINGS\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXMAPPINGS as c_int {
            if *i > 0 {
                Getline(params, input);
            }
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).bulkmap[(3 * *i) as usize],
                &mut (*eg).bulkmap[(3 * *i + 1) as usize],
                &mut (*eg).bulkmap[(3 * *i + 2) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d bulk type mappings\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).bulkmappings = *i;
    } else if !strstr(command, b"COORDINATE MAPPING\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d%d%d\0".as_ptr() as *const c_char,
            &mut (*eg).coordinatemap[0],
            &mut (*eg).coordinatemap[1],
            &mut (*eg).coordinatemap[2],
        );
    } else if !strstr(command, b"BOUNDARY BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXBOUNDARIES as c_int {
            if *i > 0 {
                Getline(params, input);
            }
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            // NOTE: C reads into [3i+2],[3i],[3i+1] order; mirrored exactly.
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).boundbound[(3 * *i + 2) as usize],
                &mut (*eg).boundbound[(3 * *i) as usize],
                &mut (*eg).boundbound[(3 * *i + 1) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d boundary boundary definitions\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).boundbounds = *i;
    } else if !strstr(command, b"MATERIAL BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXBOUNDARIES as c_int {
            if *i > 0 {
                Getline(params, input);
            }
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            // NOTE: C reads into [3i+2],[3i],[3i+1] order; mirrored exactly.
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).bulkbound[(3 * *i + 2) as usize],
                &mut (*eg).bulkbound[(3 * *i) as usize],
                &mut (*eg).bulkbound[(3 * *i + 1) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d material boundary definitions\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).bulkbounds = *i;
    } else if !strstr(command, b"RENUMBER BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXBOUNDARIES as c_int {
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).sidemap[(3 * *i) as usize],
                &mut (*eg).sidemap[(3 * *i + 1) as usize],
                &mut (*eg).sidemap[(3 * *i + 2) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d boundary mappings\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).sidemappings = *i;
    } else if !strstr(command, b"RENUMBER MATERIAL\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXBOUNDARIES as c_int {
            upper(params, j);
            if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            *cp = params;
            sscanf(
                params,
                b"%d%d%d\0".as_ptr() as *const c_char,
                &mut (*eg).bulkmap[(3 * *i) as usize],
                &mut (*eg).bulkmap[(3 * *i + 1) as usize],
                &mut (*eg).bulkmap[(3 * *i + 2) as usize],
            );
            *i += 1;
        }
        printf(
            b"Found %d material mappings\n\0".as_ptr() as *const c_char,
            *i,
        );
        (*eg).bulkmappings = *i;
    } else {
        load_ccmd4(eg, grid, cp, i, j, command, params, input);
    }
}
// SAFETY: command-file arms, layer/extrude tail (split for sizing).
unsafe fn load_ccmd4(
    eg: *mut ElmergridType,
    grid: *mut GridType,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
) {
    if !strstr(command, b"BOUNDARY LAYER\0".as_ptr() as *const c_char).is_null() {
        if !strstr(command, b"BOUNDARY LAYER MOVE\0".as_ptr() as *const c_char).is_null() {
            sscanf(
                params,
                b"%d\0".as_ptr() as *const c_char,
                &mut (*eg).layermove,
            );
        } else if !strstr(
            command,
            b"BOUNDARY LAYER EPSILON\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            sscanf(
                params,
                b"%le\0".as_ptr() as *const c_char,
                &mut (*eg).layereps,
            );
        } else {
            *i = 0;
            while *i < MAXBOUNDARIES as c_int {
                if *i > 0 {
                    Getline(params, input);
                }
                upper(params, j);
                *cp = params;
                if !strstr(params, b"END\0".as_ptr() as *const c_char).is_null()
                    || !strstr(params, b"End\0".as_ptr() as *const c_char).is_null()
                {
                    break;
                }
                (*eg).layerbounds[*i as usize] = next_int(&mut (*cp));
                (*eg).layernumber[*i as usize] = next_int(&mut (*cp));
                (*eg).layerthickness[*i as usize] = next_real(&mut (*cp));
                (*eg).layerratios[*i as usize] = next_real(&mut (*cp));
                (*eg).layerparents[*i as usize] = next_int(&mut (*cp));
                *i += 1;
            }
            printf(
                b"Found %d boundary layers\n\0".as_ptr() as *const c_char,
                *i,
            );
            (*eg).layers = *i;
        }
    } else if !strstr(
        command,
        b"REMOVE LOWER DIMENSIONAL BOUNDARIES\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).removelowdim = TRUE;
        }
    } else if !strstr(
        command,
        b"REMOVE INTERNAL BOUNDARIES\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).removeintbcs = TRUE;
        }
    } else if !strstr(command, b"REMOVE UNUSED NODES\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).removeunused = TRUE;
        }
    } else if !strstr(command, b"NO MESH NAMES\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).usenames = FALSE;
        }
    } else if !strstr(command, b"REORDER MATERIAL\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).bulkorder = TRUE;
        }
    } else if !strstr(command, b"REORDER BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).boundorder = TRUE;
        }
    } else if !strstr(command, b"DIMENSION\0".as_ptr() as *const c_char).is_null() {
        sscanf(params, b"%d\0".as_ptr() as *const c_char, &mut (*eg).dim);
    } else if !strstr(command, b"ISOPARAMETRIC\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).isoparam = TRUE;
        }
    } else if !strstr(command, b"NO BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        upper(params, j);
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*eg).saveboundaries = FALSE;
        }
    } else if !strstr(command, b"LAYERED BOUNDARIES\0".as_ptr() as *const c_char).is_null() {
        *i = 0;
        while *i < MAXLINESIZE as c_int {
            *params.wrapping_offset(*i as isize) =
                toupper(*params.wrapping_offset(*i as isize) as c_int) as c_char;
            *i += 1;
        }
        if !strstr(params, b"TRUE\0".as_ptr() as *const c_char).is_null() {
            (*grid).layeredbc = 1;
        }
        if !strstr(params, b"FALSE\0".as_ptr() as *const c_char).is_null() {
            (*grid).layeredbc = 0;
        }
    } else if !strstr(command, b"EXTRUDED\0".as_ptr() as *const c_char).is_null() {
        (*grid).dimension = 3;
        load_extruded(eg, grid, cp, i, j, command, params, input);
    }
}

// SAFETY: EXTRUDED sub-arms of LoadCommands (split for sizing).
unsafe fn load_extruded(
    _eg: *mut ElmergridType,
    grid: *mut GridType,
    cp: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    command: *mut c_char,
    params: *mut c_char,
    input: *mut FILE,
) {
    if !strstr(command, b"EXTRUDED DIVISIONS\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*grid).zcells,
        );
    }
    if !strstr(command, b"EXTRUDED BC OFFSET\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*grid).layerbcoffset,
        );
    } else if !strstr(command, b"EXTRUDED LIMITS\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 0;
        while *i <= (*grid).zcells {
            (*grid).z[*i as usize] = next_real(&mut (*cp));
            if *i > 0 && (*grid).z[*i as usize] < (*grid).z[(*i - 1) as usize] {
                printf(
                    b"Extruded limits %d: %12.6le %12.6le\n\0".as_ptr() as *const c_char,
                    *i,
                    (*grid).z[*i as usize],
                    (*grid).z[(*i - 1) as usize],
                );
                bigerror(
                    b"Values for limits should be a growing series, existing\n\0".as_ptr()
                        as *const c_char,
                );
            }
            *i += 1;
        }
    } else if !strstr(command, b"EXTRUDED SIZES\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= (*grid).zcells {
            (*grid).z[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
        *i = 1;
        while *i <= (*grid).zcells {
            (*grid).z[*i as usize] = (*grid).z[(*i - 1) as usize] + (*grid).z[*i as usize];
            *i += 1;
        }
    } else if !strstr(command, b"EXTRUDED ELEMENTS\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= (*grid).zcells {
            (*grid).zelems[*i as usize] = next_int(&mut (*cp));
            *i += 1;
        }
        (*grid).autoratio = FALSE;
    } else if !strstr(command, b"EXTRUDED RATIOS\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= (*grid).zcells {
            (*grid).zexpand[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"EXTRUDED DENSITIES\0".as_ptr() as *const c_char).is_null() {
        *cp = params;
        *i = 1;
        while *i <= (*grid).zcells {
            (*grid).zdens[*i as usize] = next_real(&mut (*cp));
            *i += 1;
        }
    } else if !strstr(command, b"EXTRUDED STRUCTURE\0".as_ptr() as *const c_char).is_null() {
        *i = 1;
        while *i <= (*grid).zcells {
            if *i > 1 {
                Getline(params, input);
            }
            sscanf(
                params,
                b"%d %d %d\n\0".as_ptr() as *const c_char,
                &mut (*grid).zfirstmaterial[*i as usize],
                &mut (*grid).zlastmaterial[*i as usize],
                &mut (*grid).zmaterial[*i as usize],
            );
            *i += 1;
        }
    } else if !strstr(
        command,
        b"EXTRUDED MAX MATERIAL\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        sscanf(
            params,
            b"%d\0".as_ptr() as *const c_char,
            &mut (*grid).maxmaterial,
        );
    } else if !strstr(
        command,
        b"EXTRUDED MATERIAL MAPPINGS\0".as_ptr() as *const c_char,
    )
    .is_null()
    {
        (*grid).zmaterialmap = imatrix(1, (*grid).zcells, 1, (*grid).maxmaterial);
        *i = 1;
        while *i <= (*grid).zcells {
            if *i > 1 {
                Getline(params, input);
            }
            *cp = params;
            *j = 1;
            while *j <= (*grid).maxmaterial {
                // NOTE: C `zmaterialmap[i][j]` (1-based imatrix).
                *(*(*grid).zmaterialmap.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize) = next_int(&mut (*cp));
                *j += 1;
            }
            *i += 1;
        }
        (*grid).zmaterialmapexists = TRUE;
    } else if !strstr(command, b"EXTRUDED HELICITY\0".as_ptr() as *const c_char).is_null() {
        sscanf(
            params,
            b"%le\0".as_ptr() as *const c_char,
            &mut (*grid).zhelicity,
        );
        (*grid).zhelicityexists = TRUE;
    }
}
// SAFETY: parent-side locator over valid data + bound; C status codes.
unsafe fn FindParentSide(
    data: *mut FemType,
    bound: *mut BoundaryType,
    sideelem: c_int,
    sideelemtype: c_int,
    sideind: *mut c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (GetElementSide overwrites before any read).
    let mut sideelemtype2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemind: c_int;
    let mut parent: c_int;
    let mut normal: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemsides: c_int;
    let mut side: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidenodes: c_int;
    let mut nohits: c_int;
    let mut hit: c_int;
    let mut hit1: c_int;
    let mut hit2: c_int;
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: never set (stays FALSE, as in C).
    let debug: c_int = FALSE;
    // NOTE: C zeroes these at top; every read below is dominated by a
    // fresh assignment, so the dead inits are dropped (bare lets).
    hit1 = FALSE;
    hit2 = FALSE;
    parent = 1;
    while parent <= 2 {
        if parent == 1 {
            elemind = *(*bound).parent.wrapping_offset(sideelem as isize);
        } else {
            elemind = *(*bound).parent2.wrapping_offset(sideelem as isize);
        }
        if elemind > 0 {
            elemtype = *(*data).elementtypes.wrapping_offset(elemind as isize);
            elemsides = elemtype / 100;
            if elemsides == 8 {
                elemsides = 6;
            } else if elemsides == 7 {
                elemsides = 5;
            } else if elemsides == 6 {
                elemsides = 5;
            } else if elemsides == 5 {
                elemsides = 4;
            }
            // NOTE: C `goto skip` is a labeled break here.
            'skip: {
                normal = 1;
                while normal >= -1 {
                    side = 0;
                    while side < elemsides {
                        if debug != 0 {
                            printf(
                                b"elem = %d %d %d %d\n\0".as_ptr() as *const c_char,
                                elemind,
                                elemsides,
                                normal,
                                side,
                            );
                        }
                        GetElementSide(
                            elemind,
                            side,
                            normal,
                            data,
                            sideind2.as_mut_ptr(),
                            &mut sideelemtype2,
                        );
                        if sideelemtype2 < 300 && sideelemtype > 300 {
                            break;
                        }
                        if sideelemtype2 < 200 && sideelemtype > 200 {
                            break;
                        }
                        if sideelemtype != sideelemtype2 {
                            side += 1;
                            continue;
                        }
                        sidenodes = sideelemtype / 100;
                        j = 0;
                        while j < sidenodes {
                            if debug != 0 {
                                printf(
                                    b"sidenode: %d %d %d\n\0".as_ptr() as *const c_char,
                                    j,
                                    *sideind.wrapping_offset(j as isize),
                                    sideind2[j as usize],
                                );
                            }
                            hit = TRUE;
                            i = 0;
                            while i < sidenodes {
                                if *sideind.wrapping_offset(((i + j) % sidenodes) as isize)
                                    != sideind2[i as usize]
                                {
                                    hit = FALSE;
                                }
                                i += 1;
                            }
                            if hit == TRUE {
                                if parent == 1 {
                                    hit1 = TRUE;
                                    *(*bound).side.wrapping_offset(sideelem as isize) = side;
                                    *(*bound).normal.wrapping_offset(sideelem as isize) = normal;
                                } else {
                                    hit2 = TRUE;
                                    *(*bound).side2.wrapping_offset(sideelem as isize) = side;
                                }
                                break 'skip;
                            }
                            j += 1;
                        }
                        side += 1;
                    }
                    normal -= 2;
                }
                // NOTE: this finding of sides does not guarantee that
                // normals are oriented correctly.
                normal = 1;
                hit = FALSE;
                side = 0;
                loop {
                    // NOTE: `if(0)` diagnostic omitted (runtime-dead).
                    GetElementSide(
                        elemind,
                        side,
                        normal,
                        data,
                        sideind2.as_mut_ptr(),
                        &mut sideelemtype2,
                    );
                    if sideelemtype2 == 0 {
                        break;
                    }
                    if sideelemtype2 < 300 && sideelemtype > 300 {
                        break;
                    }
                    if sideelemtype2 < 200 && sideelemtype > 200 {
                        break;
                    }
                    if sideelemtype != sideelemtype2 {
                        side += 1;
                        continue;
                    }
                    sidenodes = sideelemtype % 100;
                    nohits = 0;
                    j = 0;
                    while j < sidenodes {
                        i = 0;
                        while i < sidenodes {
                            if *sideind.wrapping_offset(i as isize) == sideind2[j as usize] {
                                nohits += 1;
                            }
                            i += 1;
                        }
                        j += 1;
                    }
                    if nohits == sidenodes {
                        hit = TRUE;
                        if parent == 1 {
                            hit1 = TRUE;
                            *(*bound).side.wrapping_offset(sideelem as isize) = side;
                        } else {
                            hit2 = TRUE;
                            *(*bound).side2.wrapping_offset(sideelem as isize) = side;
                        }
                        break 'skip;
                    }
                    side += 1;
                }
            }
            if hit == 0 {
                printf(
                    b"FindParentSide: cannot locate BC element in parent %d: %d\n\0".as_ptr()
                        as *const c_char,
                    parent,
                    elemind,
                );
                printf(
                    b"BC elem %d of type %d with corner indexes: \0".as_ptr() as *const c_char,
                    sideelem,
                    sideelemtype,
                );
                i = 0;
                while i < sideelemtype / 100 {
                    printf(
                        b" %d \0".as_ptr() as *const c_char,
                        *sideind.wrapping_offset(i as isize),
                    );
                    i += 1;
                }
                printf(b"\n\0".as_ptr() as *const c_char);
                printf(
                    b"Bulk elem %d of type %d with corner indexes: \0".as_ptr() as *const c_char,
                    elemind,
                    elemtype,
                );
                j = elemtype / 100;
                if j >= 5 && j <= 7 {
                    j = j - 1;
                }
                i = 0;
                while i < j {
                    printf(
                        b" %d \0".as_ptr() as *const c_char,
                        *(*(*data).topology.wrapping_offset(elemind as isize))
                            .wrapping_offset(i as isize),
                    );
                    i += 1;
                }
                printf(b"\n\0".as_ptr() as *const c_char);
            }
        }
        parent += 1;
    }
    if hit1 != 0 || hit2 != 0 {
        0
    } else {
        1
    }
}
// SAFETY: single line reader; 1 = EOF (the `upper` arg is unused in C too).
unsafe fn Getnamerow(line: *mut c_char, io: *mut FILE, _upper: c_int) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let isend: c_int;
    // NOTE: read for the NULL check below.
    let charend: *mut c_char;
    charend = fgets(line, MAXLINESIZE as c_int, io);
    isend = (charend.is_null()) as c_int;
    if isend != 0 {
        1
    } else {
        0
    }
}

// SAFETY: ElmerSolver-format mesh reader; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadElmerInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    nonames: c_int,
    info: c_int,
) -> c_int {
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut noknots: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut noelements: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut nosides: c_int = 0;
    let mut maxelemtype: c_int;
    let mut maxnodes: c_int;
    // NOTE: 0-init (fscanf overwrites before reads).
    let mut nonodes: c_int = 0;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut tottypes: c_int = 0;
    // NOTE: 0-init (fscanf overwrites before reads).
    let mut elementtype: c_int = 0;
    // NOTE: `i = 1` follows unconditionally below.
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init (sscanf/fscanf overwrites before reads).
    let mut dummyint: c_int = 0;
    // NOTE: chdir overwrites unconditionally below.
    let mut cdstat: c_int;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut fail: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut falseparents: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut noparents: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut bctopocreated: c_int = 0;
    let mut activeperm: c_int;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut activeelemperm: c_int = 0;
    let mini: c_int;
    let maxi: c_int;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut minelem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut maxelem: c_int = 0;
    // NOTE: 0-init (fscanf overwrites before reads).
    let mut p1: c_int = 0;
    // NOTE: 0-init (fscanf overwrites before reads).
    let mut p2: c_int = 0;
    // NOTE: null-init (set under activeperm before guarded reads).
    let mut nodeperm: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (set under activeelemperm before guarded reads).
    let mut elemperm: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (set under activeperm before guarded reads).
    let mut invperm: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (set under activeelemperm before guarded reads).
    let mut invelemperm: *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in load_elements before reads).
    let mut iostat: c_int = 0;
    let noelements0: c_int;
    let mut input: *mut FILE;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut line2: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut directoryname: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    // NOTE: null-init for the &mut handoff (set in load_elements before reads).
    let mut ptr1: *mut c_char = core::ptr::null_mut();
    // NOTE: null-init for the &mut handoff (set in load_elements before reads).
    let mut ptr2: *mut c_char = core::ptr::null_mut();
    sprintf(
        directoryname.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        prefix,
    );
    cdstat = chdir(directoryname.as_ptr());
    if info != 0 {
        if cdstat != 0 {
            printf(
                b"Loading mesh in ElmerSolver format from root directory.\n\0".as_ptr()
                    as *const c_char,
            );
        } else {
            printf(
                b"Loading mesh in ElmerSolver format from directory %s.\n\0".as_ptr()
                    as *const c_char,
                directoryname.as_ptr(),
            );
        }
    }
    InitializeKnots(data);
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.header\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadElmerInput: The opening of the header-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    } else {
        printf(
            b"Loading header from %s\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr is write-only here.
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    sscanf(
        line.as_ptr(),
        b"%d %d %d\0".as_ptr() as *const c_char,
        &mut noknots,
        &mut noelements,
        &mut nosides,
    );
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    sscanf(
        line.as_ptr(),
        b"%d\0".as_ptr() as *const c_char,
        &mut tottypes,
    );
    maxelemtype = 0;
    maxnodes = 0;
    i = 1;
    while i <= tottypes {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        sscanf(
            line.as_ptr(),
            b"%d\0".as_ptr() as *const c_char,
            &mut dummyint,
        );
        // NOTE: C `MAX` macro (egdef.h:97); both arms pure.
        if dummyint > maxelemtype {
            maxelemtype = dummyint;
        }
        j = maxelemtype % 100;
        if j > maxnodes {
            maxnodes = j;
        }
        i += 1;
    }
    printf(
        b"Maximum elementtype index is: %d\n\0".as_ptr() as *const c_char,
        maxelemtype,
    );
    printf(
        b"Maximum number of nodes in element is: %d\n\0".as_ptr() as *const c_char,
        maxnodes,
    );
    fclose(input);
    (*data).dim = GetElementDimension(maxelemtype);
    (*data).maxnodes = maxnodes;
    (*data).noknots = noknots;
    noelements0 = noelements;
    (*data).noelements = noelements0;
    if info != 0 {
        printf(
            b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
            noknots,
            noelements,
        );
    }
    AllocateKnots(data);
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.nodes\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        if info != 0 {
            printf(
                b"LoadElmerInput: The opening of the nodes-file %s failed!\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
        }
        bigerror(b"Cannot continue without nodes file!\n\0".as_ptr() as *const c_char);
    } else {
        printf(
            b"Loading %d Elmer nodes from %s\n\0".as_ptr() as *const c_char,
            noknots,
            filename.as_ptr(),
        );
    }
    activeperm = FALSE;
    i = 1;
    while i <= noknots {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        sscanf(
            line.as_ptr(),
            b"%d %d %le %le %le\0".as_ptr() as *const c_char,
            &mut j,
            &mut dummyint,
            &mut (*(*data).x.wrapping_offset(i as isize)),
            &mut (*(*data).y.wrapping_offset(i as isize)),
            &mut (*(*data).z.wrapping_offset(i as isize)),
        );
        if j != i && activeperm == 0 {
            printf(
                b"LoadElmerInput: The node number (%d) at node %d is not compact, creating permutation\n\0".as_ptr()
                    as *const c_char,
                j,
                i,
            );
            activeperm = TRUE;
            // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
            nodeperm = ivector(1, noknots);
            k = 1;
            while k < i {
                *nodeperm.wrapping_offset(k as isize) = k;
                k += 1;
            }
        }
        if activeperm != 0 {
            *nodeperm.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    fclose(input);
    // NOTE: inverse node permutation (returns the compact index range).
    (mini, maxi) = load_nodes_perm(nodeperm, &mut invperm, activeperm, noknots, info);
    load_elements(
        data,
        &mut activeelemperm,
        &mut elemperm,
        &mut invelemperm,
        &mut noelements,
        noelements0,
        maxelemtype,
        maxnodes,
        activeperm,
        invperm,
        mini,
        maxi,
        &mut i,
        &mut j,
        &mut k,
        &mut l,
        &mut elementtype,
        &mut nonodes,
        &mut iostat,
        &mut minelem,
        &mut maxelem,
        &mut filename,
        &mut input,
        info,
    );
    // NOTE: C `return(4)` on a missing boundary file.
    if load_boundary(
        data,
        bound,
        activeperm,
        activeelemperm,
        elemperm,
        invelemperm,
        noelements0,
        maxelemtype,
        maxnodes,
        invperm,
        minelem,
        maxelem,
        &mut nosides,
        &mut i,
        &mut j,
        &mut k,
        &mut l,
        &mut dummyint,
        &mut elementtype,
        &mut nonodes,
        &mut iostat,
        &mut p1,
        &mut p2,
        &mut fail,
        &mut falseparents,
        &mut noparents,
        &mut bctopocreated,
        &mut sideind,
        &mut filename,
        &mut input,
        info,
    ) != 0
    {
        return 4;
    }
    load_names(
        data,
        nonames,
        info,
        &mut line,
        &mut line2,
        &mut filename,
        &mut input,
        &mut ptr1,
        &mut ptr2,
        &mut i,
        &mut j,
        &mut k,
    );
    // NOTE: C never fcloses mesh.names; mirrored (leak preserved).
    // NOTE: save node permutation for later use.
    (*data).nodepermexist = activeperm;
    if activeperm != 0 {
        (*data).nodeperm = nodeperm;
        // NOTE: C `free_Ivector` is a macro for `free_ivector` (egutils.h:21).
        free_ivector(invperm, mini, maxi);
    }
    // NOTE: element permutation is irrelevant for practical purposes (see
    // C comment) and hence it is forgotten.
    if activeelemperm != 0 {
        free_ivector(invelemperm, minelem, maxelem);
        free_ivector(elemperm, 1, noelements0);
    }
    if cdstat == 0 {
        cdstat = chdir(b"..\0".as_ptr() as *const c_char);
    }
    let _ = cdstat;
    if info != 0 {
        printf(b"Elmer mesh loaded successfully\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: inverse node permutation (split for sizing); returns index range.
unsafe fn load_nodes_perm(
    nodeperm: *mut c_int,
    invperm: &mut *mut c_int,
    activeperm: c_int,
    noknots: c_int,
    info: c_int,
) -> (c_int, c_int) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init is live on the zero-trip path (C leaves it uninit there).
    let mut mini: c_int = 0;
    // NOTE: 0-init is live on the zero-trip path (C leaves it uninit there).
    let mut maxi: c_int = 0;
    // NOTE: create inverse permutation for nodes.
    if activeperm != 0 {
        i = 1;
        while i <= noknots {
            if i == 1 {
                mini = *nodeperm.wrapping_offset(i as isize);
                maxi = *nodeperm.wrapping_offset(i as isize);
            } else {
                // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
                if *nodeperm.wrapping_offset(i as isize) < mini {
                    mini = *nodeperm.wrapping_offset(i as isize);
                }
                if *nodeperm.wrapping_offset(i as isize) > maxi {
                    maxi = *nodeperm.wrapping_offset(i as isize);
                }
            }
            i += 1;
        }
        if info != 0 {
            printf(
                b"LoadElmerInput: Node index range is: [%d %d]\n\0".as_ptr() as *const c_char,
                mini,
                maxi,
            );
        }
        *invperm = ivector(mini, maxi);
        i = mini;
        while i <= maxi {
            *(*invperm).wrapping_offset(i as isize) = -1;
            i += 1;
        }
        i = 1;
        while i <= noknots {
            j = *nodeperm.wrapping_offset(i as isize);
            if *(*invperm).wrapping_offset(j as isize) > 0 {
                printf(
                    b"LoadElmerInput: Node %d is redundant which may be problematic!\n\0".as_ptr()
                        as *const c_char,
                    j,
                );
            } else {
                *(*invperm).wrapping_offset(j as isize) = i;
            }
            i += 1;
        }
    } else {
        mini = 1;
        maxi = noknots;
    }
    (mini, maxi)
}
// SAFETY: bulk-element reader + inverse element permutation (split).
unsafe fn load_elements(
    data: *mut FemType,
    activeelemperm: &mut c_int,
    elemperm: &mut *mut c_int,
    invelemperm: &mut *mut c_int,
    noelements: &mut c_int,
    noelements0: c_int,
    maxelemtype: c_int,
    maxnodes: c_int,
    activeperm: c_int,
    invperm: *mut c_int,
    mini: c_int,
    maxi: c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    elementtype: &mut c_int,
    nonodes: &mut c_int,
    iostat: &mut c_int,
    minelem: &mut c_int,
    maxelem: &mut c_int,
    filename: &mut [c_char; MAXFILESIZE],
    input: &mut *mut FILE,
    info: c_int,
) {
    *activeelemperm = FALSE;
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.elements\0".as_ptr() as *const c_char,
    );
    *input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if (*input).is_null() {
        printf(
            b"LoadElmerInput: The opening of the element-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        bigerror(b"Cannot continue without element file!\n\0".as_ptr() as *const c_char);
    } else if info != 0 {
        printf(
            b"Loading %d bulk elements from %s\n\0".as_ptr() as *const c_char,
            *noelements,
            filename.as_ptr(),
        );
    }
    *i = 1;
    while *i <= *noelements {
        *iostat = fscanf(*input, b"%d\0".as_ptr() as *const c_char, &mut (*j));
        if *iostat <= 0 {
            printf(
                b"LoadElmerInput: Failed reading element line %d, reducing size of element table to %d!\n\0".as_ptr()
                    as *const c_char,
                *i,
                *i - 1,
            );
            *noelements = *i - 1;
            (*data).noelements = *noelements;
            break;
        }
        if *i != *j && *activeelemperm == 0 {
            printf(
                b"LoadElmerInput: The element numbering (%d) at element %d is not compact, creating permutation\n\0".as_ptr()
                    as *const c_char,
                *j,
                *i,
            );
            *activeelemperm = TRUE;
            *elemperm = ivector(1, noelements0);
            *k = 1;
            while *k < *i {
                *(*elemperm).wrapping_offset(*k as isize) = *k;
                *k += 1;
            }
        }
        if *activeelemperm != 0 {
            *(*elemperm).wrapping_offset(*i as isize) = *j;
        }
        *iostat = fscanf(
            *input,
            b"%d %d\0".as_ptr() as *const c_char,
            &mut (*(*data).material.wrapping_offset(*i as isize)),
            &mut (*elementtype),
        );
        if *iostat < 2 {
            printf(
                b"LoadElmerInput: Failed reading definitions for bulk element %d\n\0".as_ptr()
                    as *const c_char,
                *j,
            );
            bigerror(b"Cannot continue without this data!\n\0".as_ptr() as *const c_char);
        }
        if *elementtype > maxelemtype {
            printf(
                b"Invalid bulk elementtype: %d\n\0".as_ptr() as *const c_char,
                *elementtype,
            );
            bigerror(b"Cannot continue with invalid elements\0".as_ptr() as *const c_char);
        }
        *(*data).elementtypes.wrapping_offset(*i as isize) = *elementtype;
        *nonodes = *elementtype % 100;
        if *nonodes > maxnodes {
            printf(
                b"Number of nodes %d in element %d is greater than allocated maximum %d\n\0"
                    .as_ptr() as *const c_char,
                *nonodes,
                *j,
                maxnodes,
            );
            bigerror(b"Cannot continue with invalid elements\0".as_ptr() as *const c_char);
        }
        *k = 0;
        while *k < *nonodes {
            *iostat = fscanf(*input, b"%d\0".as_ptr() as *const c_char, &mut (*l));
            if *l < mini || *l > maxi {
                printf(
                    b"Node %d in element %d is out of range: %d\n\0".as_ptr() as *const c_char,
                    *k + 1,
                    *j,
                    *l,
                );
                bigerror(b"Cannot continue with this node numbering\0".as_ptr() as *const c_char);
            }
            if activeperm != 0 {
                *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*k as isize) =
                    *invperm.wrapping_offset(*l as isize);
            } else {
                *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*k as isize) = *l;
            }
            *k += 1;
        }
        *i += 1;
    }
    fclose(*input);
    // NOTE: create inverse permutation for bulk elements.
    if *activeelemperm != 0 {
        *i = 1;
        while *i <= *noelements {
            if *i == 1 {
                *minelem = *(*elemperm).wrapping_offset(*i as isize);
                *maxelem = *(*elemperm).wrapping_offset(*i as isize);
            } else {
                // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
                if *(*elemperm).wrapping_offset(*i as isize) < *minelem {
                    *minelem = *(*elemperm).wrapping_offset(*i as isize);
                }
                if *(*elemperm).wrapping_offset(*i as isize) > *maxelem {
                    *maxelem = *(*elemperm).wrapping_offset(*i as isize);
                }
            }
            *i += 1;
        }
        if info != 0 {
            printf(
                b"LoadElmerInput: Element index range is: [%d %d]\n\0".as_ptr() as *const c_char,
                *minelem,
                *maxelem,
            );
        }
        *invelemperm = ivector(*minelem, *maxelem);
        *i = *minelem;
        while *i <= *maxelem {
            *(*invelemperm).wrapping_offset(*i as isize) = -1;
            *i += 1;
        }
        *i = 1;
        while *i <= *noelements {
            *j = *(*elemperm).wrapping_offset(*i as isize);
            if *(*invelemperm).wrapping_offset(*j as isize) > 0 {
                printf(
                    b"LoadElmerInput: Element %d is redundant which may be problematic!\n\0"
                        .as_ptr() as *const c_char,
                    *j,
                );
            } else {
                *(*invelemperm).wrapping_offset(*j as isize) = *i;
            }
            *i += 1;
        }
    } else {
        *minelem = 1;
        *maxelem = *noelements;
    }
}
// SAFETY: boundary-element reader + parent resolution (split for sizing).
// Returns 1 on a missing boundary file (the caller maps it to C's `return(4)`).
unsafe fn load_boundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    activeperm: c_int,
    activeelemperm: c_int,
    elemperm: *mut c_int,
    invelemperm: *mut c_int,
    noelements0: c_int,
    maxelemtype: c_int,
    maxnodes: c_int,
    invperm: *mut c_int,
    minelem: c_int,
    maxelem: c_int,
    nosides: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    dummyint: &mut c_int,
    elementtype: &mut c_int,
    nonodes: &mut c_int,
    iostat: &mut c_int,
    p1: &mut c_int,
    p2: &mut c_int,
    fail: &mut c_int,
    falseparents: &mut c_int,
    noparents: &mut c_int,
    bctopocreated: &mut c_int,
    sideind: &mut [c_int; MAXNODESD1],
    filename: &mut [c_char; MAXFILESIZE],
    input: &mut *mut FILE,
    info: c_int,
) -> c_int {
    // NOTE: elemperm/noelements0 play no role in this section (as in C).
    let _ = (elemperm, noelements0);
    *falseparents = 0;
    *noparents = 0;
    *bctopocreated = FALSE;
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.boundary\0".as_ptr() as *const c_char,
    );
    *input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if (*input).is_null() {
        printf(
            b"LoadElmerInput: The opening of the boundary-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    } else if info != 0 {
        printf(
            b"Loading %d boundary elements from %s\n\0".as_ptr() as *const c_char,
            *nosides,
            filename.as_ptr(),
        );
    }
    if *nosides > 0 {
        AllocateBoundary(bound, *nosides);
        (*data).noboundaries = 1;
    }
    *i = 0;
    *k = 1;
    while *k <= *nosides {
        *iostat = fscanf(*input, b"%d\0".as_ptr() as *const c_char, &mut (*dummyint));
        if *iostat < 1 {
            printf(
                b"LoadElmerInput: Failed reading boundary element line %d, reducing size of element table to %d!\n\0".as_ptr()
                    as *const c_char,
                *k,
                *i,
            );
            *nosides = *i;
            (*bound).nosides = *nosides;
            break;
        }
        *i += 1;
        *iostat = fscanf(
            *input,
            b"%d %d %d %d\0".as_ptr() as *const c_char,
            &mut (*(*bound).types.wrapping_offset(*i as isize)),
            &mut (*p1),
            &mut (*p2),
            &mut (*elementtype),
        );
        if *iostat < 4 {
            printf(
                b"LoadElmerInput: Failed reading definitions for boundary element %d\n\0".as_ptr()
                    as *const c_char,
                *k,
            );
            bigerror(b"Cannot continue without this data!\n\0".as_ptr() as *const c_char);
        }
        if *p1 > 0 && (*p1 < minelem || *p1 > maxelem) {
            printf(
                b"Parent in boundary element %d out of range: %d\n\0".as_ptr() as *const c_char,
                *k,
                *p1,
            );
            bigerror(b"Cannot continue with bad parents\0".as_ptr() as *const c_char);
        }
        if *p2 > 0 && (*p2 < minelem || *p2 > maxelem) {
            printf(
                b"Parent in boundary element %d out of range: %d\n\0".as_ptr() as *const c_char,
                *k,
                *p2,
            );
            bigerror(b"Cannot continue with bad parents\0".as_ptr() as *const c_char);
        }
        if activeelemperm != 0 {
            if *p1 > 0 {
                *p1 = *invelemperm.wrapping_offset(*p1 as isize);
            }
            if *p2 > 0 {
                *p2 = *invelemperm.wrapping_offset(*p2 as isize);
            }
        }
        if *elementtype > maxelemtype {
            printf(
                b"Invalid boundary elementtype: %d\n\0".as_ptr() as *const c_char,
                *elementtype,
            );
            bigerror(b"Cannot continue with invalid elements\0".as_ptr() as *const c_char);
        }
        *nonodes = *elementtype % 100;
        if *nonodes > maxnodes {
            printf(
                b"Number of nodes %d in side element %d is greater than allocated maximum %d\n\0"
                    .as_ptr() as *const c_char,
                *nonodes,
                *dummyint,
                maxnodes,
            );
            bigerror(b"Cannot continue with invalid elements\0".as_ptr() as *const c_char);
        }
        *j = 0;
        while *j < *nonodes {
            *iostat = fscanf(*input, b"%d\0".as_ptr() as *const c_char, &mut (*l));
            if activeperm != 0 {
                sideind[*j as usize] = *invperm.wrapping_offset(*l as isize);
            } else {
                sideind[*j as usize] = *l;
            }
            *j += 1;
        }
        if *p1 == 0 && *p2 != 0 {
            *(*bound).parent.wrapping_offset(*i as isize) = *p2;
            *(*bound).parent2.wrapping_offset(*i as isize) = *p1;
        } else {
            *(*bound).parent.wrapping_offset(*i as isize) = *p1;
            *(*bound).parent2.wrapping_offset(*i as isize) = *p2;
        }
        if *(*bound).parent.wrapping_offset(*i as isize) > 0 {
            *fail = FindParentSide(data, bound, *i, *elementtype, sideind.as_mut_ptr());
            if *fail != 0 {
                *falseparents += 1;
            }
        } else {
            // NOTE: `#if 0` diagnostic omitted (runtime-dead as in C).
            if *bctopocreated == 0 {
                // NOTE: C `Ivector`/`Imatrix` are macros for ivector/imatrix.
                (*bound).elementtypes = ivector(1, *nosides);
                *j = 1;
                while *j <= *nosides {
                    *(*bound).elementtypes.wrapping_offset(*j as isize) = 0;
                    *j += 1;
                }
                (*bound).topology = imatrix(1, *nosides, 0, (*data).maxnodes - 1);
                *bctopocreated = TRUE;
            }
            *j = 0;
            while *j < *elementtype % 100 {
                *(*(*bound).topology.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) =
                    sideind[*j as usize];
                *j += 1;
            }
            *(*bound).elementtypes.wrapping_offset(*i as isize) = *elementtype;
            printf(
                b"elementtype = %d %d %d\n\0".as_ptr() as *const c_char,
                *i,
                *elementtype,
                sideind[0],
            );
            *noparents += 1;
        }
        *k += 1;
    }
    if *falseparents != 0 {
        printf(
            b"There seems to be %d false parents in the mesh\n\0".as_ptr() as *const c_char,
            *falseparents,
        );
    }
    if *noparents != 0 {
        printf(
            b"There seems to be %d bc elements without parents in the mesh\n\0".as_ptr()
                as *const c_char,
            *noparents,
        );
    }
    (*bound).nosides = *i;
    fclose(*input);
    0
}
// SAFETY: mesh.names reader (split for sizing); C `goto namesend` is break.
unsafe fn load_names(
    data: *mut FemType,
    nonames: c_int,
    info: c_int,
    line: &mut [c_char; MAXLINESIZE],
    line2: &mut [c_char; MAXLINESIZE],
    filename: &mut [c_char; MAXFILESIZE],
    input: &mut *mut FILE,
    ptr1: &mut *mut c_char,
    ptr2: &mut *mut c_char,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: block-scoped in C; declared at top here.
    let mut isbody: c_int;
    let mut started: c_int;
    let mut nameproblem: c_int;
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.names\0".as_ptr() as *const c_char,
    );
    *input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if !(*input).is_null() {
        isbody = TRUE;
        nameproblem = FALSE;
        if nonames != 0 {
            printf(
                b"Ignoring > mesh.names < because it was explicitly requested!\n\0".as_ptr()
                    as *const c_char,
            );
        } else {
            if info != 0 {
                printf(
                    b"Loading names for mesh parts from file %s\n\0".as_ptr() as *const c_char,
                    filename.as_ptr(),
                );
            }
            loop {
                // NOTE: C `goto namesend` on EOF.
                if Getnamerow(line.as_mut_ptr(), *input, FALSE) != 0 {
                    break;
                }
                if !strstr(
                    line.as_ptr(),
                    b"names for boundaries\0".as_ptr() as *const c_char,
                )
                .is_null()
                {
                    if info != 0 {
                        printf(b"Reading names for mesh boundaries\n\0".as_ptr() as *const c_char);
                    }
                    isbody = FALSE;
                    continue;
                } else if !strstr(
                    line.as_ptr(),
                    b"names for bodies\0".as_ptr() as *const c_char,
                )
                .is_null()
                {
                    if info != 0 {
                        printf(b"Reading names for mesh bodies\n\0".as_ptr() as *const c_char);
                    }
                    isbody = TRUE;
                    continue;
                }
                // NOTE: get position for entity name.
                *ptr1 = strchr(line.as_ptr(), '$' as c_int);
                if (*ptr1).is_null() {
                    continue;
                }
                *ptr1 = (*ptr1).wrapping_offset(1);
                // NOTE: get position for entity index and read it.
                *ptr2 = strchr(line.as_ptr(), '=' as c_int);
                if (*ptr2).is_null() {
                    continue;
                }
                *ptr2 = (*ptr2).wrapping_offset(1);
                *j = next_int(&mut (*ptr2));
                // NOTE: initialize the entity name by white spaces.
                *i = 0;
                while *i < MAXLINESIZE as c_int {
                    line2[*i as usize] = ' ' as c_char;
                    *i += 1;
                }
                started = FALSE;
                *k = 0;
                *i = 0;
                while *i < MAXLINESIZE as c_int {
                    if *(*ptr1).wrapping_offset(0) == '=' as c_char {
                        // NOTE: remove possible trailing white space.
                        if line2[(*k - 1) as usize] == ' ' as c_char {
                            *k -= 1;
                        }
                        line2[*k as usize] = 0 as c_char;
                        break;
                    }
                    if started != 0 || *(*ptr1).wrapping_offset(0) != ' ' as c_char {
                        // NOTE: remove possible leading white space.
                        line2[*k as usize] = *(*ptr1).wrapping_offset(0);
                        started = TRUE;
                        *k += 1;
                    }
                    *ptr1 = (*ptr1).wrapping_offset(1);
                    *i += 1;
                }
                // NOTE: copy the entityname to mesh structure.
                if isbody != 0 {
                    if *j < 0 || *j >= MAXBODIES as c_int {
                        if nameproblem == 0 {
                            printf(
                                b"Cannot treat names for bodies beyond %d\n\0".as_ptr()
                                    as *const c_char,
                                *j,
                            );
                        }
                        nameproblem += 1;
                    } else {
                        if (*data).bodyname[*j as usize].is_null() {
                            // NOTE: C `Cvector` is a macro for `cvector` (egutils.h).
                            (*data).bodyname[*j as usize] = cvector(0, MAXNAMESIZE as c_int);
                        }
                        strcpy((*data).bodyname[*j as usize], line2.as_ptr());
                        (*data).bodynamesexist = TRUE;
                    }
                } else if *j < 0 || *j >= MAXBCS as c_int {
                    if nameproblem == 0 {
                        printf(
                            b"Cannot treat names for boundaries beyond %d\n\0".as_ptr()
                                as *const c_char,
                            *j,
                        );
                    }
                    nameproblem += 1;
                } else {
                    if (*data).boundaryname[*j as usize].is_null() {
                        (*data).boundaryname[*j as usize] = cvector(0, MAXNAMESIZE as c_int);
                    }
                    strcpy((*data).boundaryname[*j as usize], line2.as_ptr());
                    (*data).boundarynamesexist = TRUE;
                }
            }
        }
        // NOTE: `namesend` landing pad (C `goto` target).
        if nameproblem != 0 {
            (*data).boundarynamesexist = FALSE;
            (*data).bodynamesexist = FALSE;
            printf(
                b"Number of indexes beyond range: %d\n\0".as_ptr() as *const c_char,
                nameproblem,
            );
            smallerror(
                b"Omitting use of names because some indexes are beyond range, code some more...\0"
                    .as_ptr() as *const c_char,
            );
        }
        // NOTE: C never fcloses mesh.names; mirrored (leak preserved).
    }
}
// SAFETY: ElmerSolver-format mesh writer; C status codes.
#[no_mangle]
pub unsafe extern "C" fn SaveElmerInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    decimals: c_int,
    binary: c_int,
    nooverwrite: c_int,
    info: c_int,
) -> c_int {
    let noknots: c_int;
    let mut noelements: c_int;
    // NOTE: 0-init for the &mut handoff (set in save_bulk before reads).
    let mut material: c_int = 0;
    // NOTE: `sumsides = 0` follows unconditionally below.
    let mut sumsides: c_int;
    // NOTE: 0-init for the &mut handoff (set in save_bulk before reads).
    let mut elemtype: c_int = 0;
    let mut fail: c_int;
    // NOTE: 0-init for the &mut handoff (set in save_bulk before reads).
    let mut bcdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (GetBoundaryElement overwrites).
    let mut sideelemtype: c_int = 0;
    // NOTE: C declares but never uses `conelemtype`; dropped.
    // NOTE: 0-init for the &mut handoff (set in save_bulk before reads).
    let mut nodesd1: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in save_bulk before reads).
    let mut nodesd2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in save_header_names).
    let mut newtype: c_int = 0;
    let mut singleprec: c_int;
    // NOTE: `i = 0` follows unconditionally below.
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut l: c_int = 0;
    let mut bulktypes: [c_int; MAXELEMENTTYPE as usize + 1] = [0; MAXELEMENTTYPE as usize + 1];
    let mut sidetypes: [c_int; MAXELEMENTTYPE as usize + 1] = [0; MAXELEMENTTYPE as usize + 1];
    let mut alltypes: [c_int; MAXELEMENTTYPE as usize + 1] = [0; MAXELEMENTTYPE as usize + 1];
    // NOTE: 0-init for the &mut handoff (set in save_header_names).
    let mut tottypes: c_int = 0;
    let mut ind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: C declares a second scratch array that is never used; kept
    // (underscore-prefixed per the unused-local convention).
    let _ind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut usedbody: [c_int; MAXBODIES] = [0; MAXBODIES];
    let mut usedbc: [c_int; MAXBCS] = [0; MAXBCS];
    let mut out: *mut FILE;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut outstyle: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut directoryname: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut coords: [c_double; 3] = [0.0; 3];
    let mut scoords: [c_float; 3] = [0.0; 3];
    if (*data).created == 0 {
        printf(b"You tried to save points that were never created.\n\0".as_ptr() as *const c_char);
        return 1;
    }
    singleprec = FALSE;
    if binary == 2 {
        singleprec = TRUE;
    }
    noelements = (*data).noelements;
    noknots = (*data).noknots;
    sumsides = 0;
    i = 0;
    while i <= MAXELEMENTTYPE {
        alltypes[i as usize] = 0;
        bulktypes[i as usize] = 0;
        sidetypes[i as usize] = 0;
        i += 1;
    }
    i = 0;
    while i < MAXBODIES as c_int {
        usedbody[i as usize] = 0;
        i += 1;
    }
    i = 0;
    while i < MAXBCS as c_int {
        usedbc[i as usize] = 0;
        i += 1;
    }
    sprintf(
        directoryname.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        prefix,
    );
    if info != 0 {
        printf(
            b"Saving mesh in ElmerSolver format to directory %s.\n\0".as_ptr() as *const c_char,
            directoryname.as_ptr(),
        );
    }
    fail = chdir(directoryname.as_ptr());
    if fail != 0 {
        // NOTE: non-MINGW arm live on Linux (egnative.c:5529-5533).
        fail = mkdir(directoryname.as_ptr(), 0o750);
        if fail != 0 {
            printf(b"Could not create a result directory!\n\0".as_ptr() as *const c_char);
            return 1;
        } else {
            // NOTE: C stores into write-only `cdstat`; the value is dropped.
            let _ = chdir(directoryname.as_ptr());
        }
    } else {
        if info != 0 {
            printf(b"Reusing an existing directory\n\0".as_ptr() as *const c_char);
        }
        if nooverwrite != 0 {
            out = fopen(
                b"mesh.header\0".as_ptr() as *const c_char,
                b"r\0".as_ptr() as *const c_char,
            );
            if !out.is_null() {
                printf(
                    b"Mesh seems to already exist, writing is cancelled!\n\0".as_ptr()
                        as *const c_char,
                );
                return 1;
            }
        }
    }
    // NOTE: save mesh nodes in binary or ascii format.
    if binary != 0 {
        if singleprec != 0 {
            sprintf(
                filename.as_mut_ptr(),
                b"%s\0".as_ptr() as *const c_char,
                b"mesh.nodes.sbin\0".as_ptr() as *const c_char,
            );
        } else {
            sprintf(
                filename.as_mut_ptr(),
                b"%s\0".as_ptr() as *const c_char,
                b"mesh.nodes.bin\0".as_ptr() as *const c_char,
            );
        }
        out = fopen(filename.as_ptr(), b"wb\0".as_ptr() as *const c_char);
    } else {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.nodes\0".as_ptr() as *const c_char,
        );
        out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    }
    if info != 0 {
        printf(
            b"Saving %d coordinates to %s.\n\0".as_ptr() as *const c_char,
            noknots,
            filename.as_ptr(),
        );
    }
    if out.is_null() {
        printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
        return 2;
    }
    if binary != 0 {
        i = 1;
        while i <= noknots {
            fwrite(
                &i as *const c_int as *const c_void,
                size_of::<c_int>(),
                1,
                out,
            );
            // NOTE: in binary format the obsolete "-1" is not saved.
            if singleprec != 0 {
                // NOTE: coordinates in single precision format.
                scoords[0] = *(*data).x.wrapping_offset(i as isize) as c_float;
                scoords[1] = *(*data).y.wrapping_offset(i as isize) as c_float;
                scoords[2] = *(*data).z.wrapping_offset(i as isize) as c_float;
                fwrite(
                    scoords.as_ptr() as *const c_void,
                    size_of::<c_float>(),
                    3,
                    out,
                );
            } else {
                coords[0] = *(*data).x.wrapping_offset(i as isize);
                coords[1] = *(*data).y.wrapping_offset(i as isize);
                coords[2] = *(*data).z.wrapping_offset(i as isize);
                fwrite(
                    coords.as_ptr() as *const c_void,
                    size_of::<c_double>(),
                    3,
                    out,
                );
            }
            i += 1;
        }
    } else {
        sprintf(
            outstyle.as_mut_ptr(),
            b"%%d %%d %%.%dg %%.%dg %%.%dg\n\0".as_ptr() as *const c_char,
            decimals,
            decimals,
            decimals,
        );
        i = 1;
        while i <= noknots {
            fprintf(
                out,
                outstyle.as_ptr(),
                i,
                -1,
                *(*data).x.wrapping_offset(i as isize),
                *(*data).y.wrapping_offset(i as isize),
                *(*data).z.wrapping_offset(i as isize),
            );
            i += 1;
        }
    }
    fclose(out);
    // NOTE: C `return(3/4)` codes propagate out of the whole writer.
    let rc = save_bulk(
        data,
        bound,
        &mut noelements,
        &mut material,
        &mut elemtype,
        &mut nodesd2,
        &mut i,
        &mut j,
        &mut k,
        &mut l,
        &mut bulktypes,
        &mut usedbody,
        &mut sidetypes,
        &mut sumsides,
        &mut sideelemtype,
        &mut nodesd1,
        &mut ind,
        &mut usedbc,
        &mut bcdim,
        &mut filename,
        &mut out,
        binary,
        info,
    );
    if rc != 0 {
        return rc;
    }
    save_header_names(
        data,
        bound,
        noknots,
        noelements,
        sumsides,
        &mut newtype,
        &mut tottypes,
        &mut i,
        &mut j,
        &bulktypes,
        &sidetypes,
        &mut alltypes,
        &usedbody,
        &usedbc,
        &mut bcdim,
        &mut filename,
        &mut out,
        info,
    )
}
// SAFETY: bulk + boundary element writer (split for sizing).
unsafe fn save_bulk(
    data: *mut FemType,
    bound: *mut BoundaryType,
    noelements: &mut c_int,
    material: &mut c_int,
    elemtype: &mut c_int,
    nodesd2: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    bulktypes: &mut [c_int; MAXELEMENTTYPE as usize + 1],
    usedbody: &mut [c_int; MAXBODIES],
    sidetypes: &mut [c_int; MAXELEMENTTYPE as usize + 1],
    sumsides: &mut c_int,
    sideelemtype: &mut c_int,
    nodesd1: &mut c_int,
    ind: &mut [c_int; MAXNODESD1],
    usedbc: &mut [c_int; MAXBCS],
    bcdim: &mut c_int,
    filename: &mut [c_char; MAXFILESIZE],
    out: &mut *mut FILE,
    binary: c_int,
    info: c_int,
) -> c_int {
    // NOTE: `noelements` mirrors C's local copy of data->noelements.
    // NOTE: save bulk elements in binary or ascii format.
    if binary != 0 {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.elements.bin\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"wb\0".as_ptr() as *const c_char);
    } else {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.elements\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    }
    if info != 0 {
        printf(
            b"Saving %d element topologies to %s.\n\0".as_ptr() as *const c_char,
            *noelements,
            filename.as_ptr(),
        );
    }
    if (*out).is_null() {
        printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
        return 3;
    }
    *i = 1;
    while *i <= *noelements {
        *elemtype = *(*data).elementtypes.wrapping_offset(*i as isize);
        *material = *(*data).material.wrapping_offset(*i as isize);
        if *material < MAXBODIES as c_int {
            usedbody[*material as usize] += 1;
        }
        bulktypes[*elemtype as usize] += 1;
        *nodesd2 = *elemtype % 100;
        if binary != 0 {
            fwrite(
                &(*i) as *const c_int as *const c_void,
                size_of::<c_int>(),
                1,
                *out,
            );
            *k = -1;
            // NOTE: partition is saved in binary also in serial as "-1".
            fwrite(
                &(*k) as *const c_int as *const c_void,
                size_of::<c_int>(),
                1,
                *out,
            );
            fwrite(
                &(*material) as *const c_int as *const c_void,
                size_of::<c_int>(),
                1,
                *out,
            );
            fwrite(
                &(*elemtype) as *const c_int as *const c_void,
                size_of::<c_int>(),
                1,
                *out,
            );
            fwrite(
                *(*data).topology.wrapping_offset(*i as isize) as *const c_void,
                size_of::<c_int>(),
                *nodesd2 as usize,
                *out,
            );
        } else {
            fprintf(
                *out,
                b"%d %d %d\0".as_ptr() as *const c_char,
                *i,
                *material,
                *elemtype,
            );
            *j = 0;
            while *j < *nodesd2 {
                fprintf(
                    *out,
                    b" %d\0".as_ptr() as *const c_char,
                    *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*j as isize),
                );
                *j += 1;
            }
            fprintf(*out, b"\n\0".as_ptr() as *const c_char);
        }
        *i += 1;
    }
    fclose(*out);
    // NOTE: save boundary elements in binary or ascii format.
    if binary != 0 {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.boundary.bin\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"wb\0".as_ptr() as *const c_char);
    } else {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.boundary\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    }
    if info != 0 {
        printf(
            b"Saving boundary elements to %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    if (*out).is_null() {
        printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
        return 4;
    }
    *sumsides = 0;
    // NOTE: save normal boundaries.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == FALSE {
            *j += 1;
            continue;
        }
        if (*bound.wrapping_offset(*j as isize)).nosides == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            GetBoundaryElement(
                *i,
                &mut (*bound.wrapping_offset(*j as isize)),
                data,
                ind.as_mut_ptr(),
                &mut (*sideelemtype),
            );
            *sumsides += 1;
            *k = *(*bound.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            if *k < MAXBCS as c_int {
                *bcdim = GetElementDimension(*sideelemtype);
                // NOTE: C `MAX` macro (egdef.h:97); arms pure.
                if usedbc[*k as usize] < *bcdim + 1 {
                    usedbc[*k as usize] = *bcdim + 1;
                }
            }
            sidetypes[*sideelemtype as usize] += 1;
            *nodesd1 = *sideelemtype % 100;
            if binary != 0 {
                fwrite(
                    &(*sumsides) as *const c_int as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                *k = -1;
                fwrite(
                    &(*k) as *const c_int as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                fwrite(
                    &(*(*bound.wrapping_offset(*j as isize))
                        .types
                        .wrapping_offset(*i as isize)) as *const c_int
                        as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                fwrite(
                    &(*(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize)) as *const c_int
                        as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                fwrite(
                    &(*(*bound.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize)) as *const c_int
                        as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                fwrite(
                    &(*sideelemtype) as *const c_int as *const c_void,
                    size_of::<c_int>(),
                    1,
                    *out,
                );
                fwrite(
                    ind.as_ptr() as *const c_void,
                    size_of::<c_int>(),
                    *nodesd1 as usize,
                    *out,
                );
            } else {
                fprintf(
                    *out,
                    b"%d %d %d %d %d\0".as_ptr() as *const c_char,
                    *sumsides,
                    *(*bound.wrapping_offset(*j as isize))
                        .types
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize),
                    *sideelemtype,
                );
                *l = 0;
                while *l < *nodesd1 {
                    fprintf(*out, b" %d\0".as_ptr() as *const c_char, ind[*l as usize]);
                    *l += 1;
                }
                fprintf(*out, b"\n\0".as_ptr() as *const c_char);
            }
            *i += 1;
        }
        *j += 1;
    }
    0
}
// SAFETY: header + names + nodeperm writer (split for sizing); C codes.
unsafe fn save_header_names(
    data: *mut FemType,
    bound: *mut BoundaryType,
    noknots: c_int,
    noelements: c_int,
    sumsides: c_int,
    newtype: &mut c_int,
    tottypes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    bulktypes: &[c_int; MAXELEMENTTYPE as usize + 1],
    sidetypes: &[c_int; MAXELEMENTTYPE as usize + 1],
    alltypes: &mut [c_int; MAXELEMENTTYPE as usize + 1],
    usedbody: &[c_int; MAXBODIES],
    usedbc: &[c_int; MAXBCS],
    bcdim: &mut c_int,
    filename: &mut [c_char; MAXFILESIZE],
    out: &mut *mut FILE,
    info: c_int,
) -> c_int {
    *newtype = 0;
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == FALSE {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            // NOTE: C `MAX` macro (egdef.h:97); arms pure.
            if *newtype
                < *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
            {
                *newtype = *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize);
            }
            *i += 1;
        }
        *j += 1;
    }
    fclose(*out);
    *tottypes = 0;
    *i = 0;
    while *i <= MAXELEMENTTYPE {
        alltypes[*i as usize] = bulktypes[*i as usize] + sidetypes[*i as usize];
        if alltypes[*i as usize] != 0 {
            *tottypes += 1;
        }
        *i += 1;
    }
    sprintf(
        filename.as_mut_ptr(),
        b"%s\0".as_ptr() as *const c_char,
        b"mesh.header\0".as_ptr() as *const c_char,
    );
    *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving header info to %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    if (*out).is_null() {
        printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
        return 4;
    }
    fprintf(
        *out,
        b"%-6d %-6d %-6d\n\0".as_ptr() as *const c_char,
        noknots,
        noelements,
        sumsides,
    );
    fprintf(*out, b"%-6d\n\0".as_ptr() as *const c_char, *tottypes);
    *i = 0;
    while *i <= MAXELEMENTTYPE {
        if alltypes[*i as usize] != 0 {
            fprintf(
                *out,
                b"%-6d %-6d\n\0".as_ptr() as *const c_char,
                *i,
                bulktypes[*i as usize] + sidetypes[*i as usize],
            );
        }
        *i += 1;
    }
    fclose(*out);
    if (*data).boundarynamesexist != 0 || (*data).bodynamesexist != 0 {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.names\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
        if info != 0 {
            printf(
                b"Saving names info to %s.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
        if (*out).is_null() {
            printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
            return 5;
        }
        if (*data).bodynamesexist != 0 {
            fprintf(
                *out,
                b"! ----- names for bodies -----\n\0".as_ptr() as *const c_char,
            );
            *i = 1;
            while *i < MAXBODIES as c_int {
                if usedbody[*i as usize] != 0 {
                    if !(*data).bodyname[*i as usize].is_null() {
                        fprintf(
                            *out,
                            b"$ %s = %d\n\0".as_ptr() as *const c_char,
                            (*data).bodyname[*i as usize],
                            *i,
                        );
                    } else {
                        fprintf(*out, b"$ body%d = %d\n\0".as_ptr() as *const c_char, *i, *i);
                    }
                }
                *i += 1;
            }
        }
        if (*data).boundarynamesexist != 0 {
            fprintf(
                *out,
                b"! ----- names for boundaries -----\n\0".as_ptr() as *const c_char,
            );
            *i = 1;
            while *i < MAXBCS as c_int {
                if usedbc[*i as usize] != 0 {
                    *bcdim = usedbc[*i as usize] - 1;
                    if !(*data).boundaryname[*i as usize].is_null() {
                        fprintf(
                            *out,
                            b"$ %s = %d\n\0".as_ptr() as *const c_char,
                            (*data).boundaryname[*i as usize],
                            *i,
                        );
                    } else if *bcdim == 2 {
                        fprintf(
                            *out,
                            b"$ surf_bc%d = %d\n\0".as_ptr() as *const c_char,
                            *i,
                            *i,
                        );
                    } else if *bcdim == 1 {
                        fprintf(
                            *out,
                            b"$ line_bc%d = %d\n\0".as_ptr() as *const c_char,
                            *i,
                            *i,
                        );
                    } else if *bcdim == 0 {
                        fprintf(
                            *out,
                            b"$ node_bc%d = %d\n\0".as_ptr() as *const c_char,
                            *i,
                            *i,
                        );
                    }
                }
                *i += 1;
            }
        }
        fclose(*out);
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"entities.sif\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
        if info != 0 {
            printf(
                b"Saving entities info to %s.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
        if (*out).is_null() {
            printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
            return 5;
        }
        if (*data).bodynamesexist != 0 {
            fprintf(
                *out,
                b"!------ Skeleton for body section -----\n\0".as_ptr() as *const c_char,
            );
            *j = 0;
            *i = 1;
            while *i < MAXBODIES as c_int {
                if usedbody[*i as usize] != 0 {
                    *j = *j + 1;
                    fprintf(*out, b"Body %d\n\0".as_ptr() as *const c_char, *j);
                    if !(*data).bodyname[*i as usize].is_null() {
                        fprintf(
                            *out,
                            b"  Name = %s\n\0".as_ptr() as *const c_char,
                            (*data).bodyname[*i as usize],
                        );
                    } else {
                        fprintf(*out, b"  Name = body%d\n\0".as_ptr() as *const c_char, *i);
                    }
                    fprintf(*out, b"End\n\n\0".as_ptr() as *const c_char);
                }
                *i += 1;
            }
        }
        if (*data).boundarynamesexist != 0 {
            fprintf(
                *out,
                b"!------ Skeleton for boundary section -----\n\0".as_ptr() as *const c_char,
            );
            *j = 0;
            *i = 1;
            while *i < MAXBCS as c_int {
                if usedbc[*i as usize] != 0 {
                    *j = *j + 1;
                    fprintf(
                        *out,
                        b"Boundary Condition %d\n\0".as_ptr() as *const c_char,
                        *j,
                    );
                    if !(*data).boundaryname[*i as usize].is_null() {
                        fprintf(
                            *out,
                            b"  Name = %s\n\0".as_ptr() as *const c_char,
                            (*data).boundaryname[*i as usize],
                        );
                    } else {
                        fprintf(*out, b"  Name = bc%d\n\0".as_ptr() as *const c_char, *i);
                    }
                    fprintf(*out, b"End\n\n\0".as_ptr() as *const c_char);
                }
                *i += 1;
            }
        }
        fclose(*out);
    }
    if (*data).nodepermexist != 0 {
        sprintf(
            filename.as_mut_ptr(),
            b"%s\0".as_ptr() as *const c_char,
            b"mesh.nodeperm\0".as_ptr() as *const c_char,
        );
        *out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
        if info != 0 {
            printf(
                b"Saving initial node permutation to %s.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
        if (*out).is_null() {
            printf(b"opening of file was not successful\n\0".as_ptr() as *const c_char);
            return 3;
        }
        *i = 1;
        while *i <= noknots {
            fprintf(
                *out,
                b"%d %d\n\0".as_ptr() as *const c_char,
                *i,
                *(*data).nodeperm.wrapping_offset(*i as isize),
            );
            *i += 1;
        }
        // NOTE: C never fcloses mesh.nodeperm; mirrored (leak preserved).
    }
    // NOTE: C assigns the chdir result to write-only `cdstat`; dropped here.
    let _ = chdir(b"..\0".as_ptr() as *const c_char);
    0
}
// SAFETY: structured-mesh driver over valid grid/data/boundaries.
#[no_mangle]
pub unsafe extern "C" fn CreateElmerGridMesh(
    grid: *mut GridType,
    data: *mut FemType,
    boundaries: *mut BoundaryType,
    relh: Real,
    info: c_int,
) -> c_int {
    let mut j: c_int;
    let cell: *mut CellType;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        (*boundaries.wrapping_offset(j as isize)).created = FALSE;
        (*boundaries.wrapping_offset(j as isize)).nosides = FALSE;
        j += 1;
    }
    SetElementDivision(grid, relh, info);
    cell = malloc((((*grid).nocells + 1) as usize) * size_of::<CellType>()) as *mut CellType;
    SetCellData(grid, cell, info);
    if (*grid).dimension == 1 {
        SetCellKnots1D(grid, cell, info);
    } else {
        SetCellKnots(grid, cell, info);
    }
    CreateKnots(grid, cell, data, 0, 0);
    if (*grid).noboundaries > 0 {
        j = 0;
        while j < (*grid).noboundaries {
            if (*grid).boundsolid[j as usize] < 4 {
                CreateBoundary(
                    cell,
                    data,
                    &mut (*boundaries.wrapping_offset(j as isize)),
                    (*grid).boundext[j as usize],
                    (*grid).boundint[j as usize],
                    1,
                    (*grid).boundtype[j as usize],
                    info,
                );
            } else {
                CreatePoints(
                    cell,
                    data,
                    &mut (*boundaries.wrapping_offset(j as isize)),
                    (*grid).boundext[j as usize],
                    (*grid).boundint[j as usize],
                    (*grid).boundsolid[j as usize],
                    (*grid).boundtype[j as usize],
                    info,
                );
            }
            j += 1;
        }
    }
    free(cell as *mut c_void);
    0
}
