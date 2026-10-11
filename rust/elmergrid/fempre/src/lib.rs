// Faithful Rust transcription of elmergrid/src/fempre.c @ 9f6af2f85.
// Workspace layout: shared declarations live in eg-prelude; this crate
// appends only the unit body.
// Config scope: exe-only unit (not in elmergrid_core); USE_METIS=1 branches
// live (exe config), USE_MATC=0 branches dead in both default configs.
// SAFETY: single-threaded utility; raw-pointer arithmetic mirrors C 1:1.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]

use eg_prelude::*;
// ---- sibling-unit imports (declared in headers, resolved at final link) --
// Signatures transcribed from egnative.h / egconvert.h / egmesh.h /
// egextra.h / egparallel.h / egexport.h @ 9f6af2f85.
extern "C" {
    fn InitParameters(eg: *mut ElmergridType);
    fn InitGrid(grid: *mut GridType);
    fn LoadCommands(
        prefix: *mut c_char,
        eg: *mut ElmergridType,
        grid: *mut GridType,
        mode: c_int,
        info: c_int,
    ) -> c_int;
    fn Instructions();
    // SAFETY: diverges via exit(0) on every path (egnative.c); the `!`
    // documents what C leaves implicit (code after a Goodbye() call is
    // dead there too).
    fn Goodbye() -> !;
    fn InlineParameters(
        eg: *mut ElmergridType,
        argc: c_int,
        argv: *mut *mut c_char,
        first: c_int,
        info: c_int,
    ) -> c_int;
    fn timer_activate(prefix: *const c_char);
    fn timer_show();
    fn LoadElmergrid(
        grid: *mut *mut GridType,
        nogrids: *mut c_int,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn CreateExampleGrid(dim: c_int, grids: *mut *mut GridType, nogrids: *mut c_int, info: c_int);
    fn SaveElmergrid(grid: *mut GridType, nogrids: c_int, prefix: *mut c_char, info: c_int)
    -> c_int;
    fn SetElementDivision(grid: *mut GridType, relh: Real, info: c_int);
    fn LoadElmerInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        nonames: c_int,
        info: c_int,
    ) -> c_int;
    fn LoadSolutionElmer(data: *mut FemType, results: c_int, prefix: *mut c_char, info: c_int)
    -> c_int;
    fn LoadAnsysInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadAbaqusInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadAbaqusOutput(data: *mut FemType, prefix: *mut c_char, info: c_int) -> c_int;
    fn LoadFidapInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadUniversalMesh(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadComsolMesh(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadFieldviewInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadTriangleInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadMeditInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadGidInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadGmshInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        keeporphans: c_int,
        dim: c_int,
        info: c_int,
    ) -> c_int;
    fn FuseSolutionElmerPartitioned(
        prefix: *mut c_char,
        outfile: *mut c_char,
        decimals: c_int,
        parts: c_int,
        minstep: c_int,
        maxstep: c_int,
        dstep: c_int,
        info: c_int,
    ) -> c_int;
    fn LoadFvcomMesh(
        data: *mut FemType,
        bound: *mut BoundaryType,
        filename: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadNastranInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadCGsimMesh(data: *mut FemType, prefix: *mut c_char, info: c_int) -> c_int;
    fn LoadGeoInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadFluxMesh(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn LoadFluxMesh3D(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn CreateCells(grid: *mut GridType, cell: *mut *mut CellType, info: c_int);
    fn CreateKnots(
        grid: *mut GridType,
        cell: *mut CellType,
        data: *mut FemType,
        noknots: c_int,
        info: c_int,
    );
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
    fn GetCoordinateDimension(data: *mut FemType, info: c_int) -> c_int;
    fn SetDiscontinuousBoundary(
        data: *mut FemType,
        bound: *mut BoundaryType,
        boundtype: c_int,
        endnodes: c_int,
        info: c_int,
    ) -> c_int;
    fn ElementsToTriangles(
        data: *mut FemType,
        bound: *mut BoundaryType,
        critangle: Real,
        info: c_int,
    ) -> c_int;
    fn CreateBoundaryLayer(
        data: *mut FemType,
        bound: *mut BoundaryType,
        nolayers: c_int,
        layerbounds: *mut c_int,
        layernumber: *mut c_int,
        layerratios: *mut Real,
        layerthickness: *mut Real,
        layerparents: *mut c_int,
        maxfilters: c_int,
        layereps: Real,
        info: c_int,
    ) -> c_int;
    fn CreateBoundaryLayerDivide(
        data: *mut FemType,
        bound: *mut BoundaryType,
        nolayers: c_int,
        layerbounds: *mut c_int,
        layernumber: *mut c_int,
        layerratios: *mut Real,
        layerthickness: *mut Real,
        layerparents: *mut c_int,
        info: c_int,
    ) -> c_int;
    fn CreateKnotsExtruded(
        dataxy: *mut FemType,
        boundxy: *mut BoundaryType,
        grid: *mut GridType,
        data: *mut FemType,
        bound: *mut BoundaryType,
        info: c_int,
    );
    fn DestroyKnots(data: *mut FemType);
    fn PolarCoordinates(data: *mut FemType, rad: Real, info: c_int) -> c_int;
    fn CylinderCoordinates(data: *mut FemType, info: c_int) -> c_int;
    fn RotateTranslateScale(data: *mut FemType, eg: *mut ElmergridType, info: c_int) -> c_int;
    fn CylindricalCoordinateCurve(data: *mut FemType, zet: Real, rad: Real, angle: Real);
    fn UniteMeshes(
        data1: *mut FemType,
        data2: *mut FemType,
        bound1: *mut BoundaryType,
        bound2: *mut BoundaryType,
        nooverlap: c_int,
        info: c_int,
    ) -> c_int;
    fn CloneMeshes(
        data: *mut FemType,
        bound: *mut BoundaryType,
        ncopies: *mut c_int,
        meshsize: *mut Real,
        diffmats: c_int,
        info: c_int,
    ) -> c_int;
    fn MirrorMeshes(
        data: *mut FemType,
        bound: *mut BoundaryType,
        symmaxis: *mut c_int,
        diffmats: c_int,
        meshsize: *mut Real,
        symmbound: c_int,
        info: c_int,
    ) -> c_int;
    fn MergeElements(
        data: *mut FemType,
        bound: *mut BoundaryType,
        manual: c_int,
        corder: *mut Real,
        eps: Real,
        mergebounds: c_int,
        info: c_int,
    );
    fn ReduceElementOrder(data: *mut FemType, matmin: c_int, matmax: c_int);
    fn IncreaseElementOrder(data: *mut FemType, info: c_int) -> c_int;
    fn ReorderElementsMetis(data: *mut FemType, info: c_int) -> c_int;
    fn ReorderElements(
        data: *mut FemType,
        bound: *mut BoundaryType,
        manual: c_int,
        corder: *mut Real,
        info: c_int,
    );
    fn IsoparametricElements(
        data: *mut FemType,
        bound: *mut BoundaryType,
        bcstoo: c_int,
        info: c_int,
    );
    fn SideAndBulkBoundaries(
        data: *mut FemType,
        bound: *mut BoundaryType,
        eg: *mut ElmergridType,
        info: c_int,
    ) -> c_int;
    fn SideAndBulkMappings(
        data: *mut FemType,
        bound: *mut BoundaryType,
        eg: *mut ElmergridType,
        info: c_int,
    ) -> c_int;
    fn RemoveLowerDimensionalBoundaries(
        data: *mut FemType,
        bound: *mut BoundaryType,
        info: c_int,
    ) -> c_int;
    fn RemoveInternalBoundaries(data: *mut FemType, bound: *mut BoundaryType, info: c_int)
    -> c_int;
    fn RemoveUnusedNodes(data: *mut FemType, info: c_int) -> c_int;
    fn RenumberBoundaryTypes(
        data: *mut FemType,
        bound: *mut BoundaryType,
        renumber: c_int,
        bcoffset: c_int,
        info: c_int,
    );
    fn RenumberMaterialTypes(data: *mut FemType, bound: *mut BoundaryType, info: c_int);
    fn SetConnectedNodes(
        data: *mut FemType,
        bound: *mut BoundaryType,
        bctype: c_int,
        connecttype: c_int,
        info: c_int,
    ) -> c_int;
    fn FindPeriodicNodes(data: *mut FemType, periodicdim: *mut c_int, info: c_int) -> c_int;
    fn FindPeriodicParents(data: *mut FemType, bound: *mut BoundaryType, info: c_int) -> c_int;
    fn PartitionSimpleElements(
        data: *mut FemType,
        eg: *mut ElmergridType,
        bound: *mut BoundaryType,
        dimpart: *mut c_int,
        dimper: *mut c_int,
        partorder: c_int,
        corder: *mut Real,
        parttol: Real,
        info: c_int,
    ) -> c_int;
    fn PartitionSimpleElementsNonRecursive(
        data: *mut FemType,
        dimpart: *mut c_int,
        dimper: *mut c_int,
        info: c_int,
    ) -> c_int;
    fn PartitionSimpleElementsRotational(
        data: *mut FemType,
        dimpart: *mut c_int,
        dimper: *mut c_int,
        info: c_int,
    ) -> c_int;
    fn PartitionSimpleNodes(
        data: *mut FemType,
        dimpart: *mut c_int,
        dimper: *mut c_int,
        partorder: c_int,
        corder: *mut Real,
        parttol: Real,
        info: c_int,
    ) -> c_int;
    fn PartitionMetisMesh(
        data: *mut FemType,
        eg: *mut ElmergridType,
        partitions: c_int,
        dual: c_int,
        info: c_int,
    ) -> c_int;
    fn PartitionMetisGraph(
        data: *mut FemType,
        bound: *mut BoundaryType,
        eg: *mut ElmergridType,
        partitions: c_int,
        metisopt: c_int,
        dual: c_int,
        info: c_int,
    ) -> c_int;
    fn OptimizePartitioningAtBoundary(
        data: *mut FemType,
        bound: *mut BoundaryType,
        info: c_int,
    ) -> c_int;
    fn OptimizePartitioning(
        data: *mut FemType,
        bound: *mut BoundaryType,
        noopt: c_int,
        partbw: c_int,
        info: c_int,
    ) -> c_int;
    // NOTE: C spells this free_Ivector (Rvector-family macro for free_ivector).
    fn free_ivector(v: *mut c_int, nl: c_int, nh: c_int);
    fn SaveSizeInfo(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    fn BoundingBox(data: *mut FemType, nomesh: c_int, nomeshes: c_int, info: c_int) -> c_int;
    fn MeshPieces(data: *mut FemType, nomesh: c_int, nomeshes: c_int, info: c_int) -> c_int;
    fn SaveElmerInputPartitioned(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        decimals: c_int,
        binary: c_int,
        parthalo: *mut c_int,
        parthypre: c_int,
        subparts: c_int,
        nooverwrite: c_int,
        info: c_int,
    ) -> c_int;
    fn SaveElmerInput(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        decimals: c_int,
        binary: c_int,
        nooverwrite: c_int,
        info: c_int,
    ) -> c_int;
    fn SaveElmerInputFemBem(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        decimals: c_int,
        info: c_int,
    ) -> c_int;
    fn SaveSolutionElmer(
        data: *mut FemType,
        bound: *mut BoundaryType,
        nobound: c_int,
        prefix: *mut c_char,
        decimals: c_int,
        info: c_int,
    ) -> c_int;
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
    fn SaveAbaqusInput(data: *mut FemType, prefix: *mut c_char, info: c_int) -> c_int;
    // NOTE: the header declares the C-variadic form; the egexport crate
    // defines the fixed triple (see its SAFETY note). This decl matches the
    // sole tree call shape (vctrs = 1, one pointer); the extra definition
    // params are never read on that path.
    fn SaveFidapOutput(
        data: *mut FemType,
        prefix: *mut c_char,
        info: c_int,
        vctrs: c_int,
        vect1: *mut Real,
    ) -> c_int;
    fn SaveBoundary(
        data: *mut FemType,
        bound: *mut BoundaryType,
        prefix: *mut c_char,
        info: c_int,
    ) -> c_int;
    // NOTE: C spells bigerror as returning void although it exits(1); keep
    // the () shape so the dead-continue statements transcribe without
    // unreachable-code warnings (they never execute in either language).
    fn bigerror(error_text: *const c_char);
}

// SAFETY: program entry (replaces fempre.c.o wholesale, so this crate owns
// the `main` symbol). Single-threaded utility; `goto`s become loops.
// NOTE: C-dead locals `l`/`showmem` are dropped (declared/assigned, never
// read in the C source either).
#[export_name = "main"]
pub unsafe extern "C" fn fempre_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let (mut i, mut j, mut k);
    let (inmethod, outmethod, info, mut errorstat): (c_int, c_int, c_int, c_int);
    let (mut nogrids, mut nogrids0, mut nomeshes, mut nofile, dim): (
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    );
    let (mut nodes3d, mut elements3d): (c_int, c_int);
    let mut elementsredone: c_int = 0;
    let mut mergeeps: Real;
    let mut prefix: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut grids: *mut GridType;
    let mut cell: [*mut CellType; MAXCASES] = [core::ptr::null_mut(); MAXCASES];
    // SAFETY: loaders initialize elements before reads (same contract as C's
    // indeterminate locals); zeroed rather than uninit so any missed path
    // fails deterministically, and the corpus verifies behavior.
    let mut data: [FemType; MAXCASES] = core::mem::zeroed();
    let mut boundaries: [*mut BoundaryType; MAXCASES] = [core::ptr::null_mut(); MAXCASES];
    let mut eg: ElmergridType = core::mem::zeroed();
    printf(
        b"==================================================================\n\0"
            .as_ptr() as *const c_char,
    );
    printf(
        b"ElmerGrid mesh conversion and manipulation utility, Welcome!\n\0".as_ptr()
            as *const c_char,
    );
    // NOTE: ELMER_FEM_VERSION + ELMER_FEM_BRANCH both defined in config.h.
    printf(
        b"Version: %s-%s (Rev: %s, Compiled: %s)\n\0".as_ptr() as *const c_char,
        ELMER_FEM_VERSION_STR.as_ptr() as *const c_char,
        ELMER_FEM_BRANCH_STR.as_ptr() as *const c_char,
        ELMER_FEM_REVISION_STR.as_ptr() as *const c_char,
        ELMER_FEM_COMPILATIONDATE_STR.as_ptr() as *const c_char,
    );
    printf(
        b"This program is free software licensed under GPL\n\0".as_ptr() as *const c_char,
    );
    printf(
        b"==================================================================\n\0"
            .as_ptr() as *const c_char,
    );
    InitParameters(&mut eg);
    grids = malloc((MAXCASES as size_t) * c_size_of::<GridType>()) as *mut GridType;
    InitGrid(grids);
    info = TRUE;
    if argc <= 1 {
        errorstat = LoadCommands(*argv.wrapping_offset(1), &mut eg, grids, argc - 1, info);
        if errorstat != 0 {
            Instructions();
            Goodbye();
        }
    } else if argc == 2 {
        errorstat = LoadCommands(*argv.wrapping_offset(1), &mut eg, grids, argc - 1, info);
        if errorstat != 0 {
            Goodbye();
        }
    } else if argc < 4 {
        Instructions();
        Goodbye();
    } else {
        errorstat = InlineParameters(&mut eg, argc, argv, 4, info);
        if errorstat != 0 {
            Goodbye();
        }
    }
    if eg.outmethod == 0 || eg.inmethod == 0 {
        printf(
            b"Please define the input and output formats\n\0".as_ptr() as *const c_char,
        );
    }
    if eg.inmethod != 1 {
        if eg.outmethod == 1 || eg.outmethod == 8 || eg.outmethod == 9 || eg.outmethod == 10 {
            printf(
                b"input of type %d can't create output of type %d\n\0".as_ptr() as *const c_char,
                eg.inmethod,
                eg.outmethod,
            );
            // NOTE: dead in C too (exit(0) inside Goodbye follows); kept for
            // shape fidelity, allowed instead of deleted.
            #[allow(unused_assignments)]
            {
                errorstat += 1;
            }
            Goodbye();
        }
    }
    if eg.timeron != 0 {
        timer_activate(eg.infofile.as_ptr());
    }
    printf(b"\nElmergrid loading data:\n\0".as_ptr() as *const c_char);
    printf(b"-----------------------\n\0".as_ptr() as *const c_char);
    dim = eg.dim;
    nofile = 0;
    nomeshes = 0;
    nogrids = 0;
    inmethod = eg.inmethod;
    outmethod = eg.outmethod;
    if eg.nooverwrite != 0 && eg.filerenamed == 0 {
        if eg.partitions == 1 || eg.metis == 1 {
            if inmethod == outmethod {
                printf(
                    b"Nothing to do, one partition, same input/output format and no overwriting allowed!\n\0"
                        .as_ptr() as *const c_char,
                );
                // NOTE: C `return(0)` after Goodbye() is dead (exit(0)
                // inside); omitted to avoid unreachable-code warnings.
                Goodbye();
            }
        }
    }
    // NOTE: `goto read_another_file` becomes loop-continue.
    loop {
        timer_show();
        match inmethod {
            1 => {
                nogrids0 = nogrids;
                if LoadElmergrid(
                    &mut grids,
                    &mut nogrids,
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    info,
                ) == 1
                {
                    CreateExampleGrid(
                        eg.dim,
                        &mut grids,
                        &mut nogrids,
                        info,
                    );
                    SaveElmergrid(
                        grids,
                        nogrids,
                        eg.filesin[nofile as usize].as_mut_ptr(),
                        info,
                    );
                    printf(
                        b"Because file %s didn't exist, it was created for you.\n\0".as_ptr()
                            as *const c_char,
                        eg.filesin[nofile as usize].as_ptr(),
                    );
                    Goodbye();
                }
                LoadCommands(
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    &mut eg,
                    grids,
                    2,
                    info,
                );
                k = nogrids0;
                while k < nogrids {
                    SetElementDivision(
                        grids.wrapping_offset(k as isize),
                        eg.relh,
                        info,
                    );
                    k += 1;
                }
            }
            2 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadElmerInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    (eg.usenames == 0) as c_int,
                    info,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            3 => {
                if LoadSolutionElmer(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    TRUE,
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    info,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            4 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                if LoadAnsysInput(
                    data.as_mut_ptr(),
                    boundaries[0],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    info,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            5 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadAbaqusInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            6 => {
                if LoadAbaqusOutput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            7 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadFidapInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                eg.bulkorder = TRUE;
                eg.boundorder = TRUE;
                if eg.usenames == 0 {
                    data[nofile as usize].boundarynamesexist = FALSE;
                    data[nofile as usize].bodynamesexist = FALSE;
                }
                nomeshes += 1;
            }
            8 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadUniversalMesh(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            9 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadComsolMesh(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    info,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            10 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadFieldviewInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            11 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadTriangleInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            12 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadMeditInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            13 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadGidInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            14 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                data[nofile as usize].dim = if eg.dim >= 1 && eg.dim <= 3 {
                    eg.dim
                } else {
                    3
                };
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadGmshInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    eg.multidim,
                    eg.dim,
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            15 => {
                if info != 0 {
                    printf(
                        b"Partitioned solution is fused on-the-fly therefore no other operations may be performed.\n\0"
                            .as_ptr() as *const c_char,
                    );
                }
                FuseSolutionElmerPartitioned(
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    eg.filesout[nofile as usize].as_mut_ptr(),
                    eg.decimals,
                    eg.partjoin,
                    eg.saveinterval[0],
                    eg.saveinterval[1],
                    eg.saveinterval[2],
                    info,
                );
                if info != 0 {
                    printf(
                        b"Finishing with the fusion of partitioned Elmer solutions\n\0".as_ptr()
                            as *const c_char,
                    );
                }
                Goodbye();
            }
            16 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadFvcomMesh(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            17 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadNastranInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            18 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadCGsimMesh(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    info,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            19 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadGeoInput(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            20 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadFluxMesh(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            21 => {
                boundaries[nofile as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                i = 0;
                while i < MAXBOUNDARIES as c_int {
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).created = FALSE;
                    (*boundaries[nofile as usize].wrapping_offset(i as isize)).nosides = 0;
                    i += 1;
                }
                if LoadFluxMesh3D(
                    data.as_mut_ptr().wrapping_offset(nofile as isize),
                    boundaries[nofile as usize],
                    eg.filesin[nofile as usize].as_mut_ptr(),
                    TRUE,
                ) != 0
                {
                    Goodbye();
                }
                nomeshes += 1;
            }
            _ => {
                Instructions();
                Goodbye();
            }
        }
        nofile += 1;
        if nofile < eg.nofilesin {
            printf(
                b"\nElmergrid loading data from another file:\n\0".as_ptr() as *const c_char,
            );
        } else {
            break;
        }
    }
    // NOTE: `goto redoelements` becomes loop-continue via the redo flag.
    'manip: loop {
        printf(
            b"\nElmergrid creating and manipulating meshes:\n\0".as_ptr() as *const c_char,
        );
        printf(
            b"-------------------------------------------\n\0".as_ptr() as *const c_char,
        );
        timer_show();
        if nogrids > nomeshes && outmethod != 1 {
            nomeshes = nogrids;
            k = 0;
            while k < nogrids {
                CreateCells(
                    grids.wrapping_offset(k as isize),
                    cell.as_mut_ptr().wrapping_offset(k as isize),
                    info,
                );
                CreateKnots(
                    grids.wrapping_offset(k as isize),
                    cell[k as usize],
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    0,
                    0,
                );
                boundaries[k as usize] =
                    malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                        as *mut BoundaryType;
                j = 0;
                while j < MAXBOUNDARIES as c_int {
                    (*boundaries[k as usize].wrapping_offset(j as isize)).created = FALSE;
                    (*boundaries[k as usize].wrapping_offset(j as isize)).nosides = FALSE;
                    j += 1;
                }
                if (*grids.wrapping_offset(k as isize)).noboundaries > 0 {
                    j = 0;
                    while j < (*grids.wrapping_offset(k as isize)).noboundaries {
                        if (*grids.wrapping_offset(k as isize)).boundsolid[j as usize] < 4 {
                            CreateBoundary(
                                cell[k as usize],
                                data.as_mut_ptr().wrapping_offset(k as isize),
                                boundaries[k as usize].wrapping_offset(j as isize),
                                (*grids.wrapping_offset(k as isize)).boundext[j as usize],
                                (*grids.wrapping_offset(k as isize)).boundint[j as usize],
                                1,
                                (*grids.wrapping_offset(k as isize)).boundtype[j as usize],
                                info,
                            );
                        } else {
                            CreatePoints(
                                cell[k as usize],
                                data.as_mut_ptr().wrapping_offset(k as isize),
                                boundaries[k as usize].wrapping_offset(j as isize),
                                (*grids.wrapping_offset(k as isize)).boundext[j as usize],
                                (*grids.wrapping_offset(k as isize)).boundint[j as usize],
                                (*grids.wrapping_offset(k as isize)).boundsolid[j as usize],
                                (*grids.wrapping_offset(k as isize)).boundtype[j as usize],
                                info,
                            );
                        }
                        j += 1;
                    }
                }
                k += 1;
            }
        }
        if data[0].dim < 3 {
            data[0].dim = GetCoordinateDimension(data.as_mut_ptr(), info);
        }
        k = 0;
        while k < nomeshes {
            if eg.discont == 0 && k < nogrids {
                j = 0;
                while j < (*grids.wrapping_offset(k as isize)).noboundaries {
                    if (*grids.wrapping_offset(k as isize)).boundsolid[j as usize] == 2 {
                        eg.discontbounds[eg.discont as usize] =
                            (*grids.wrapping_offset(k as isize)).boundtype[j as usize];
                        eg.discont += 1;
                    }
                    j += 1;
                }
            }
            if eg.discont != 0 {
                i = 1;
                while i <= eg.discont {
                    SetDiscontinuousBoundary(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        eg.discontbounds[(i - 1) as usize],
                        2,
                        info,
                    );
                    i += 1;
                }
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if nogrids != 0
                && (eg.triangles != 0
                    || (*grids.wrapping_offset(k as isize)).triangles == TRUE)
            {
                let criticalangle: Real = c_max_real(
                    eg.triangleangle,
                    (*grids.wrapping_offset(k as isize)).triangleangle,
                );
                ElementsToTriangles(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    criticalangle,
                    info,
                );
            }
            k += 1;
        }
        if eg.layers > 0 {
            k = 0;
            while k < nomeshes {
                CreateBoundaryLayer(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.layers,
                    eg.layerbounds.as_mut_ptr(),
                    eg.layernumber.as_mut_ptr(),
                    eg.layerratios.as_mut_ptr(),
                    eg.layerthickness.as_mut_ptr(),
                    eg.layerparents.as_mut_ptr(),
                    eg.layermove,
                    eg.layereps,
                    info,
                );
                k += 1;
            }
        } else if eg.layers < 0 {
            k = 0;
            while k < nomeshes {
                CreateBoundaryLayerDivide(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    abs(eg.layers),
                    eg.layerbounds.as_mut_ptr(),
                    eg.layernumber.as_mut_ptr(),
                    eg.layerratios.as_mut_ptr(),
                    eg.layerthickness.as_mut_ptr(),
                    eg.layerparents.as_mut_ptr(),
                    info,
                );
                k += 1;
            }
        }
        k = 0;
        while k < nogrids {
            if (*grids.wrapping_offset(k as isize)).rotatecurve != 0 {
                eg.rotatecurve = TRUE;
                eg.curvezet = (*grids.wrapping_offset(k as isize)).curvezet;
                eg.curverad = (*grids.wrapping_offset(k as isize)).curverad;
                eg.curveangle = (*grids.wrapping_offset(k as isize)).curveangle;
            }
            k += 1;
        }
        let mut redo = false;
        if outmethod != 1 && dim != 2 && eg.dim != 2 {
            j = c_max_int(nogrids, 1);
            k = 0;
            while k < j {
                if (*grids.wrapping_offset(k as isize)).dimension == 3
                    || (*grids.wrapping_offset(k as isize)).rotate != 0
                {
                    boundaries[j as usize] =
                        malloc((MAXBOUNDARIES as size_t) * c_size_of::<BoundaryType>())
                            as *mut BoundaryType;
                    i = 0;
                    while i < MAXBOUNDARIES as c_int {
                        (*boundaries[j as usize].wrapping_offset(i as isize)).created = FALSE;
                        i += 1;
                    }
                    CreateKnotsExtruded(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        grids.wrapping_offset(k as isize),
                        data.as_mut_ptr().wrapping_offset(j as isize),
                        boundaries[j as usize],
                        info,
                    );
                    if nogrids != 0 {
                        elements3d =
                            c_max_int(eg.elements3d, (*grids.wrapping_offset(k as isize)).wantedelems3d);
                        nodes3d =
                            c_max_int(eg.nodes3d, (*grids.wrapping_offset(k as isize)).wantednodes3d);
                        if elements3d != 0 {
                            if (abs(
                                data[j as usize].noelements - elements3d,
                            ) as Real)
                                / (1.0 * elements3d as Real)
                                > 0.01
                                && elementsredone < 5
                            {
                                (*grids.wrapping_offset(k as isize)).wantedelems =
                                    ((*grids.wrapping_offset(k as isize)).wantedelems as Real
                                        * pow(
                                            1.0 * elements3d as Real
                                                / data[j as usize].noelements as Real,
                                            2.0 / 3.0,
                                        )) as c_int;
                                elementsredone += 1;
                            } else {
                                elementsredone = 0;
                            }
                        } else if nodes3d != 0 {
                            if (abs(data[j as usize].noknots - nodes3d) as Real)
                                / (1.0 * nodes3d as Real)
                                > 0.01
                                && elementsredone < 5
                            {
                                (*grids.wrapping_offset(k as isize)).wantedelems =
                                    ((*grids.wrapping_offset(k as isize)).wantedelems as Real
                                        * pow(
                                            1.0 * nodes3d as Real
                                                / data[j as usize].noknots as Real,
                                            2.0 / 3.0,
                                        )) as c_int;
                                elementsredone += 1;
                            } else {
                                elementsredone = 0;
                            }
                        }
                        if elementsredone != 0 {
                            nomeshes = 0;
                            i = 0;
                            while i < nogrids {
                                SetElementDivision(
                                    grids.wrapping_offset(i as isize),
                                    eg.relh,
                                    info,
                                );
                                i += 1;
                            }
                            DestroyKnots(data.as_mut_ptr().wrapping_offset(j as isize));
                            DestroyKnots(data.as_mut_ptr().wrapping_offset(k as isize));
                            free(cell[k as usize] as *mut c_void);
                            if info != 0 {
                                printf(
                                    b"Iteration %d of elements number targeting %d in 2D\n\0"
                                        .as_ptr() as *const c_char,
                                    elementsredone,
                                    (*grids.wrapping_offset(k as isize)).wantedelems,
                                );
                            }
                            // NOTE: `goto redoelements`.
                            redo = true;
                            break;
                        }
                    }
                    data[k as usize] = data[j as usize];
                    boundaries[k as usize] = boundaries[j as usize];
                }
                k += 1;
            }
            if redo {
                continue 'manip;
            }
        }
        k = 0;
        while k < nomeshes {
            if eg.polar != 0 || data[k as usize].coordsystem == COORD_POLAR {
                if eg.polar == 0 {
                    eg.polarradius = (*grids.wrapping_offset(k as isize)).polarradius;
                }
                PolarCoordinates(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    eg.polarradius,
                    info,
                );
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if eg.cylinder != 0 || data[k as usize].coordsystem == COORD_CYL {
                CylinderCoordinates(data.as_mut_ptr().wrapping_offset(k as isize), info);
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            RotateTranslateScale(
                data.as_mut_ptr().wrapping_offset(k as isize),
                &mut eg,
                info,
            );
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if eg.rotatecurve != 0 {
                CylindricalCoordinateCurve(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    eg.curvezet,
                    eg.curverad,
                    eg.curveangle,
                );
            }
            k += 1;
        }
        if eg.unitemeshes != 0 {
            k = 1;
            while k < nomeshes {
                UniteMeshes(
                    data.as_mut_ptr(),
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[0],
                    boundaries[k as usize],
                    eg.unitenooverlap,
                    info,
                );
                k += 1;
            }
            nomeshes = 1;
            nogrids = 1;
        }
        if eg.clone[0] != 0 || eg.clone[1] != 0 || eg.clone[2] != 0 {
            k = 0;
            while k < nomeshes {
                CloneMeshes(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.clone.as_mut_ptr(),
                    eg.clonesize.as_mut_ptr(),
                    eg.cloneinds,
                    info,
                );
                k += 1;
            }
        }
        if eg.mirror[0] != 0 || eg.mirror[1] != 0 || eg.mirror[2] != 0 {
            k = 0;
            while k < nomeshes {
                MirrorMeshes(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.mirror.as_mut_ptr(),
                    FALSE,
                    eg.clonesize.as_mut_ptr(),
                    eg.mirrorbc,
                    info,
                );
                mergeeps = fabs(
                    eg.clonesize[0] + eg.clonesize[1] + eg.clonesize[2],
                ) * 1.0e-8;
                MergeElements(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.order,
                    eg.corder.as_mut_ptr(),
                    mergeeps,
                    FALSE,
                    TRUE,
                );
                k += 1;
            }
        }
        if nomeshes > 1 {
            strcpy(prefix.as_mut_ptr(), eg.filesout[0].as_ptr());
            k = 0;
            while k < nomeshes {
                sprintf(
                    eg.filesout[k as usize].as_mut_ptr(),
                    b"%s%d\0".as_ptr() as *const c_char,
                    prefix.as_ptr(),
                    k + 1,
                );
                k += 1;
            }
        }
        k = 0;
        while k < nomeshes {
            if nogrids != 0 && (*grids.wrapping_offset(k as isize)).reduceordermatmax != 0 {
                eg.reduce = TRUE;
                eg.reducemat1 = (*grids.wrapping_offset(k as isize)).reduceordermatmin;
                eg.reducemat2 = (*grids.wrapping_offset(k as isize)).reduceordermatmax;
            }
            if eg.reduce != 0 {
                ReduceElementOrder(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    eg.reducemat1,
                    eg.reducemat2,
                );
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if eg.increase != 0 {
                IncreaseElementOrder(data.as_mut_ptr().wrapping_offset(k as isize), TRUE);
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if eg.merge != 0 {
                MergeElements(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.order,
                    eg.corder.as_mut_ptr(),
                    eg.cmerge,
                    FALSE,
                    TRUE,
                );
            } else if eg.order == 3 {
                // NOTE: USE_METIS=1 (exe config) takes this branch; the
                // #else printf is compiled out exactly as in C.
                ReorderElementsMetis(data.as_mut_ptr().wrapping_offset(k as isize), TRUE);
            } else if eg.order != 0 {
                ReorderElements(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.order,
                    eg.corder.as_mut_ptr(),
                    TRUE,
                );
            }
            if eg.isoparam != 0 {
                IsoparametricElements(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    TRUE,
                    info,
                );
            }
            k += 1;
        }
        k = 0;
        while k < nomeshes {
            if eg.bulkbounds != 0 || eg.boundbounds != 0 {
                SideAndBulkBoundaries(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    &mut eg,
                    info,
                );
            }
            k += 1;
        }
        if eg.removelowdim != 0 {
            k = 0;
            while k < nomeshes {
                RemoveLowerDimensionalBoundaries(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    info,
                );
                k += 1;
            }
        }
        if eg.removeintbcs != 0 {
            k = 0;
            while k < nomeshes {
                RemoveInternalBoundaries(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    info,
                );
                k += 1;
            }
        }
        if eg.removeunused != 0 {
            k = 0;
            while k < nomeshes {
                RemoveUnusedNodes(data.as_mut_ptr().wrapping_offset(k as isize), info);
                k += 1;
            }
        }
        if eg.boundorder != 0 || eg.bcoffset != 0 {
            k = 0;
            while k < nomeshes {
                RenumberBoundaryTypes(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.boundorder,
                    eg.bcoffset,
                    info,
                );
                k += 1;
            }
        }
        if eg.bulkorder != 0 {
            k = 0;
            while k < nomeshes {
                RenumberMaterialTypes(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    info,
                );
                k += 1;
            }
        }
        if eg.sidemappings != 0 || eg.bulkmappings != 0 {
            k = 0;
            while k < nomeshes {
                SideAndBulkMappings(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    &mut eg,
                    info,
                );
                k += 1;
            }
        }
        if eg.coordinatemap[0] != 0 && eg.coordinatemap[1] != 0 && eg.coordinatemap[2] != 0 {
            let mut tmpcoord: [*mut Real; 3] = [core::ptr::null_mut(); 3];
            if info != 0 {
                printf(
                    b"Mapping coordinates with [%d %d %d]\n\0".as_ptr() as *const c_char,
                    eg.coordinatemap[0],
                    eg.coordinatemap[1],
                    eg.coordinatemap[2],
                );
            }
            k = 0;
            while k < nomeshes {
                tmpcoord[0] = data[k as usize].x;
                tmpcoord[1] = data[k as usize].y;
                tmpcoord[2] = data[k as usize].z;
                data[k as usize].x = tmpcoord[(eg.coordinatemap[0] - 1) as usize];
                data[k as usize].y = tmpcoord[(eg.coordinatemap[1] - 1) as usize];
                data[k as usize].z = tmpcoord[(eg.coordinatemap[2] - 1) as usize];
                if eg.coordinatemap[2] != 3 {
                    data[k as usize].dim = 3;
                }
                k += 1;
            }
        }
        k = 0;
        while k < nomeshes {
            let partoptim: c_int;
            let partbcoptim: c_int;
            let mut partopt: c_int;
            let mut partdual: c_int;
            if eg.metis == 1 {
                if info != 0 {
                    printf(
                        b"One Metis partition requested, enforcing serial mode\n\0".as_ptr()
                            as *const c_char,
                    );
                }
                eg.metis = 0;
            }
            if eg.partitions == 1 {
                if eg.connect == 0 {
                    if info != 0 {
                        printf(
                            b"One geometric partition requested, enforcing serial mode\n\0"
                                .as_ptr() as *const c_char,
                        );
                    }
                    eg.partitions = 0;
                }
            }
            partoptim = eg.partoptim;
            partbcoptim = eg.partbcoptim;
            partdual = eg.partdual;
            if eg.partitions != 0 || eg.metis != 0 {
                printf(b"\nElmergrid partitioning meshes:\n\0".as_ptr() as *const c_char);
                printf(b"------------------------------\n\0".as_ptr() as *const c_char);
                timer_show();
                partopt = eg.partopt;
                i = 1;
                while i <= eg.connect {
                    SetConnectedNodes(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        eg.connectbounds[(i - 1) as usize],
                        eg.connectboundsset[(i - 1) as usize],
                        info,
                    );
                    i += 1;
                }
                if eg.periodicdim[0] != 0 || eg.periodicdim[1] != 0 || eg.periodicdim[2] != 0 {
                    FindPeriodicNodes(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        eg.periodicdim.as_mut_ptr(),
                        info,
                    );
                }
                if eg.partitions != 0 {
                    if partopt == -1 {
                        partopt = partdual;
                    }
                    if partopt == 0 {
                        // NOTE: raw derivations hoisted: they cannot coexist
                        // with &mut eg in one call (E0499); the borrows end
                        // at the let statements.
                        let partdim = eg.partdim.as_mut_ptr();
                        let periodicdim = eg.periodicdim.as_mut_ptr();
                        let partcorder = eg.partcorder.as_mut_ptr();
                        PartitionSimpleElements(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            &mut eg,
                            boundaries[k as usize],
                            partdim,
                            periodicdim,
                            eg.partorder,
                            partcorder,
                            eg.parttol,
                            info,
                        );
                    } else if partopt == 2 {
                        PartitionSimpleElementsNonRecursive(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            eg.partdim.as_mut_ptr(),
                            eg.periodicdim.as_mut_ptr(),
                            info,
                        );
                    } else if partopt == 3 {
                        PartitionSimpleElementsRotational(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            eg.partdim.as_mut_ptr(),
                            eg.periodicdim.as_mut_ptr(),
                            info,
                        );
                    } else {
                        PartitionSimpleNodes(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            eg.partdim.as_mut_ptr(),
                            eg.periodicdim.as_mut_ptr(),
                            eg.partorder,
                            eg.partcorder.as_mut_ptr(),
                            eg.parttol,
                            info,
                        );
                    }
                }
                // NOTE: USE_METIS=1 (exe config); the C #else arm (no-metis
                // builds) is compiled out exactly as in C.
                if eg.metis != 0 {
                    if partopt < 0 || partopt > 4 {
                        printf(
                            b"Metis optional parameter should be in range [0,4], not %d\n\0"
                                .as_ptr() as *const c_char,
                            partopt,
                        );
                        bigerror(
                            b"Cannot perform partitioning\0".as_ptr() as *const c_char,
                        );
                    }
                    if partopt <= 1 && eg.connect != 0 {
                        printf(
                            b"Elemental Metis partition cannot deal with constraints!\n\0"
                                .as_ptr() as *const c_char,
                        );
                        printf(
                            b"Using Metis algorithms based on the dual graph\n\0".as_ptr()
                                as *const c_char,
                        );
                        partopt = 2;
                    }
                    if partopt <= 1 {
                        if partdual == 0 {
                            partdual = partopt;
                        }
                        // NOTE: C ignores the return (fail); same here.
                        let _fail: c_int = PartitionMetisMesh(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            &mut eg,
                            eg.metis,
                            partdual,
                            info,
                        );
                    } else {
                        PartitionMetisGraph(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            boundaries[k as usize],
                            &mut eg,
                            eg.metis,
                            partopt,
                            partdual,
                            info,
                        );
                    }
                }
                if data[k as usize].periodicexist != 0 {
                    FindPeriodicParents(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        info,
                    );
                }
                timer_show();
                if partbcoptim != 0 {
                    OptimizePartitioningAtBoundary(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        info,
                    );
                }
                OptimizePartitioning(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    (partoptim == 0) as c_int,
                    eg.partbw,
                    info,
                );
                if data[k as usize].periodicexist != 0 {
                    free_ivector(
                        data[k as usize].periodic,
                        1,
                        data[k as usize].noknots,
                    );
                    data[k as usize].periodicexist = FALSE;
                }
                if info != 0 {
                    printf(
                        b"Partitioning routines finished!\n\0".as_ptr() as *const c_char,
                    );
                }
            }
            timer_show();
            k += 1;
        }
        if eg.timeron != 0 {
            if info != 0 {
                printf(
                    b"Saving size info for timing purposes\n\0".as_ptr() as *const c_char,
                );
            }
            k = 0;
            while k < nomeshes {
                SaveSizeInfo(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    boundaries[k as usize],
                    eg.infofile.as_mut_ptr(),
                    info,
                );
                k += 1;
            }
        }
        if info != 0 {
            k = 0;
            while k < nomeshes {
                BoundingBox(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    k,
                    nomeshes,
                    info,
                );
                k += 1;
            }
        }
        if info != 0 {
            k = 0;
            while k < nomeshes {
                MeshPieces(
                    data.as_mut_ptr().wrapping_offset(k as isize),
                    k,
                    nomeshes,
                    info,
                );
                k += 1;
            }
        }
        if eg.nosave != 0 {
            Goodbye();
        }
        printf(
            b"\nElmergrid saving data with method %d:\n\0".as_ptr() as *const c_char,
            outmethod,
        );
        printf(b"-------------------------------------\n\0".as_ptr() as *const c_char);
        match outmethod {
            1 => {
                SaveElmergrid(grids, nogrids, eg.filesout[0].as_mut_ptr(), info);
            }
            2 => {
                k = 0;
                while k < nomeshes {
                    if data[k as usize].nopartitions > 1 {
                        SaveElmerInputPartitioned(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            boundaries[k as usize],
                            eg.filesout[k as usize].as_mut_ptr(),
                            eg.decimals,
                            eg.binary,
                            eg.parthalo.as_mut_ptr(),
                            eg.parthypre,
                            c_max_int(eg.partbcz, eg.partbcr),
                            eg.nooverwrite,
                            info,
                        );
                    } else {
                        SaveElmerInput(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            boundaries[k as usize],
                            eg.filesout[k as usize].as_mut_ptr(),
                            eg.decimals,
                            eg.binary,
                            eg.nooverwrite,
                            info,
                        );
                    }
                    k += 1;
                }
            }
            22 => {
                k = 0;
                while k < nomeshes {
                    SaveElmerInputFemBem(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        eg.filesout[k as usize].as_mut_ptr(),
                        eg.decimals,
                        info,
                    );
                    k += 1;
                }
            }
            3 => {
                k = 0;
                while k < nomeshes {
                    if data[k as usize].variables == 0 {
                        CreateVariable(
                            data.as_mut_ptr().wrapping_offset(k as isize),
                            1,
                            1,
                            0.0,
                            b"Number\0".as_ptr() as *const c_char,
                            FALSE,
                        );
                        i = 1;
                        while i <= data[k as usize].alldofs[1] {
                            *data[k as usize].dofs[1].wrapping_offset(i as isize) = i as Real;
                            i += 1;
                        }
                    }
                    SaveSolutionElmer(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        if eg.saveboundaries != 0 {
                            MAXBOUNDARIES as c_int
                        } else {
                            0
                        },
                        eg.filesout[k as usize].as_mut_ptr(),
                        eg.decimals,
                        info,
                    );
                    k += 1;
                }
            }
            4 => {
                k = 0;
                while k < nomeshes {
                    SaveMeshGmsh(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        if eg.saveboundaries != 0 {
                            MAXBOUNDARIES as c_int
                        } else {
                            0
                        },
                        eg.filesout[k as usize].as_mut_ptr(),
                        eg.decimals,
                        info,
                    );
                    k += 1;
                }
            }
            5 => {
                k = 0;
                while k < nomeshes {
                    SaveMeshVtu(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        boundaries[k as usize],
                        if eg.saveboundaries != 0 {
                            MAXBOUNDARIES as c_int
                        } else {
                            0
                        },
                        eg.filesout[k as usize].as_mut_ptr(),
                        eg.vtuone,
                        info,
                    );
                    k += 1;
                }
            }
            6 => {
                k = 0;
                while k < nomeshes {
                    SaveAbaqusInput(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        eg.filesout[k as usize].as_mut_ptr(),
                        info,
                    );
                    k += 1;
                }
            }
            7 => {
                k = 0;
                while k < nomeshes {
                    SaveFidapOutput(
                        data.as_mut_ptr().wrapping_offset(k as isize),
                        eg.filesout[k as usize].as_mut_ptr(),
                        info,
                        1,
                        data[k as usize].dofs[1],
                    );
                    k += 1;
                }
            }
            101 => {
                k = 0;
                while k < nogrids {
                    i = 0;
                    while i < (*grids.wrapping_offset(k as isize)).noboundaries {
                        if (*boundaries[k as usize].wrapping_offset(i as isize)).created == TRUE
                        {
                            sprintf(
                                prefix.as_mut_ptr(),
                                b"%s%d\0".as_ptr() as *const c_char,
                                eg.filesout[k as usize].as_ptr(),
                                i + 1,
                            );
                            SaveBoundary(
                                data.as_mut_ptr().wrapping_offset(k as isize),
                                boundaries[k as usize].wrapping_offset(i as isize),
                                prefix.as_mut_ptr(),
                                info,
                            );
                        }
                        i += 1;
                    }
                    k += 1;
                }
            }
            _ => {
                Instructions();
            }
        }
        timer_show();
        Goodbye();
    }
}
