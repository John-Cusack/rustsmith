// egconvert: third-party mesh format loaders (ElmerGrid lane, pair 3).
//
// Faithful C-ABI transcription of elmergrid/src/egconvert.c @ 9f6af2f85.
// Same contract as the pair-1/2 units: libc malloc/free discipline, static
// mut for C file-scope globals, raw-pointer indexing, all user-visible
// text through C printf/fprintf/sprintf/sscanf, every unsafe carries a
// SAFETY comment. C `Rvector/Ivector/Imatrix/Cvector` macros resolve to
// their lowercase providers (egutils.h:15-21).

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
    fn free_imatrix(m: *mut *mut c_int, nrl: c_int, nrh: c_int, ncl: c_int, nch: c_int);
    fn free_cvector(v: *mut c_char, nl: c_int, nh: c_int);
    fn next_int(start: *mut *mut c_char) -> c_int;
    fn next_int_n(start: *mut *mut c_char, n: c_int) -> c_int;
    fn next_real(start: *mut *mut c_char) -> Real;
    fn timer_activate(prefix: *const c_char);
    fn FileExists(filename: *mut c_char) -> c_int;
    fn StringToStrings(
        buf: *const c_char,
        args: *mut [c_char; 15],
        maxcnt: c_int,
        separator: c_char,
    ) -> c_int;
    fn StringToIntegerNoZero(
        buf: *const c_char,
        dest: *mut c_int,
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
    fn Maximum(vector: *mut Real, first: c_int, last: c_int) -> Real;
    // NOTE: egmesh-unit providers (ported in this same pair).
    fn AllocateBoundary(bound: *mut BoundaryType, size: c_int) -> c_int;
    fn AllocateKnots(data: *mut FemType);
    fn CreateVariable(
        data: *mut FemType,
        variable: c_int,
        unknowns: c_int,
        value: Real,
        variablename: *const c_char,
        eorder: c_int,
    ) -> c_int;
    fn GetElementDimension(elementtype: c_int) -> c_int;
    fn GetElementSide(
        element: c_int,
        side: c_int,
        normal: c_int,
        data: *mut FemType,
        ind: *mut c_int,
        sideelemtype: *mut c_int,
    );
    fn GetMaxElementType(data: *mut FemType) -> c_int;
    fn GetMinElementType(data: *mut FemType) -> c_int;
    fn ModifyUnsupportedElements(data: *mut FemType);
    fn NodesToBoundaryChain(
        data: *mut FemType,
        bound: *mut BoundaryType,
        bcinds: *mut c_int,
        bctags: *mut c_int,
        nbc: c_int,
        bccount: c_int,
        info: c_int,
    );
    fn InitializeKnots(data: *mut FemType);
    fn ElementsToBoundaryConditions(
        data: *mut FemType,
        bound: *mut BoundaryType,
        param: c_int,
        info: c_int,
    );
    // NOTE: egmesh-unit provider (ported in this same pair).
    fn CreateInverseTopology(data: *mut FemType, info: c_int) -> c_int;
}
// NOTE: C file-scope `GETLINE`/`GETLONGLINE` macros assign `ioptr`;
// write-only here.
static mut ioptr: *mut c_char = core::ptr::null_mut();
// SAFETY: C file-scope line counter (single-threaded utility).
static mut linenumber: c_int = 0;

// SAFETY: #-comment-skipping row reader; `goto newline` is continue.
unsafe fn Getrow(line1: *mut c_char, io: *mut FILE, upper: c_int) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut isend: c_int;
    let mut line0: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: read for the NULL check below.
    let mut charend: *mut c_char;
    i = 0;
    while i < MAXLINESIZE as c_int {
        line0[i as usize] = ' ' as c_char;
        i += 1;
    }
    loop {
        charend = fgets(line0.as_mut_ptr(), MAXLINESIZE as c_int, io);
        linenumber += 1;
        isend = (charend.is_null()) as c_int;
        if isend != 0 {
            return 1;
        }
        if line0[0] == '#' as c_char || line0[0] == '!' as c_char {
            continue;
        }
        if !strstr(line0.as_ptr(), b"#\0".as_ptr() as *const c_char).is_null() {
            continue;
        }
        if upper != 0 {
            i = 0;
            while i < MAXLINESIZE as c_int {
                *line1.wrapping_offset(i as isize) =
                    toupper(line0[i as usize] as c_int) as c_char;
                i += 1;
            }
        } else {
            i = 0;
            while i < MAXLINESIZE as c_int {
                *line1.wrapping_offset(i as isize) = line0[i as usize];
                i += 1;
            }
        }
        return 0;
    }
}

// SAFETY: Fortran-D-tolerant row reader; `goto newline` is continue.
unsafe fn GetrowDouble(line1: *mut c_char, io: *mut FILE) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut isend: c_int;
    let mut line0: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: read for the NULL check below.
    let mut charend: *mut c_char;
    i = 0;
    while i < MAXLINESIZE as c_int {
        line0[i as usize] = ' ' as c_char;
        i += 1;
    }
    loop {
        charend = fgets(line0.as_mut_ptr(), MAXLINESIZE as c_int, io);
        linenumber += 1;
        isend = (charend.is_null()) as c_int;
        if isend != 0 {
            return 1;
        }
        if line0[0] == '#' as c_char || line0[0] == '!' as c_char {
            continue;
        }
        if !strstr(line0.as_ptr(), b"#\0".as_ptr() as *const c_char).is_null() {
            continue;
        }
        i = 0;
        while i < MAXLINESIZE as c_int {
            // NOTE: the Fortran double is not recognized by C string operators.
            if line0[i as usize] == 'd' as c_char || line0[i as usize] == 'D' as c_char {
                *line1.wrapping_offset(i as isize) = 'e' as c_char;
            } else {
                *line1.wrapping_offset(i as isize) = line0[i as usize];
            }
            i += 1;
        }
        return 0;
    }
}

// SAFETY: comment-preserving row reader (no skipping).
unsafe fn Comsolrow(line1: *mut c_char, io: *mut FILE) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let isend: c_int;
    let mut line0: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: read for the NULL check below.
    let charend: *mut c_char;
    i = 0;
    while i < MAXLINESIZE as c_int {
        line0[i as usize] = ' ' as c_char;
        i += 1;
    }
    charend = fgets(line0.as_mut_ptr(), MAXLINESIZE as c_int, io);
    linenumber += 1;
    isend = (charend.is_null()) as c_int;
    if isend != 0 {
        return 1;
    }
    i = 0;
    while i < MAXLINESIZE as c_int {
        *line1.wrapping_offset(i as isize) = line0[i as usize];
        i += 1;
    }
    0
}

// SAFETY: pure family→corner-node-count map.
unsafe fn LinearNodes(elemtype: c_int) -> c_int {
    let fam2nodemap: [c_int; 9] = [0, 1, 2, 3, 4, 4, 5, 6, 8];
    let elemfamily: c_int;
    let nonodes: c_int;
    elemfamily = elemtype / 100;
    nonodes = fam2nodemap[elemfamily as usize];
    nonodes
}
// SAFETY: point-parent locator over valid data + bound; needs sized bound.
unsafe fn FindPointParents(
    data: *mut FemType,
    bound: *mut BoundaryType,
    boundarynodes: c_int,
    nodeindx: *mut c_int,
    boundindx: *mut c_int,
    mut info: c_int,
) {
    // NOTE: C forces `info = TRUE` on entry; the passed value is unread.
    let _ = info;
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: C declares `k` but only the helper uses it now; dropped.
    // NOTE: 0-init for the &mut handoff (GetElementSide overwrites).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemind: c_int;
    // NOTE: assigned unconditionally below before any use.
    let indx: *mut c_int;
    let mut boundarytype: c_int;
    let mut minboundary: c_int;
    let mut maxboundary: c_int;
    let mut minnode: c_int;
    let mut maxnode: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sideelem: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemsides: c_int;
    // NOTE: assigned at loop head before the by-value handoff.
    let mut side: c_int;
    // NOTE: 0-init for the &mut handoff (set in find_side before reads).
    let mut sidenodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in find_side before reads).
    let mut hit: c_int = 0;
    // NOTE: assigned in the loop before the helper handoff.
    let mut nohits: c_int;
    // NOTE: assigned unconditionally below before any use.
    let mut elemhits: *mut c_int;
    if boundarynodes == 0 {
        return;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        (*bound.wrapping_offset(j as isize)).created = FALSE;
        j += 1;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        (*bound.wrapping_offset(j as isize)).nosides = 0;
        j += 1;
    }
    AllocateBoundary(bound, boundarynodes);
    info = TRUE;
    sideelem = 0;
    maxboundary = *boundindx.wrapping_offset(1);
    minboundary = maxboundary;
    maxnode = *nodeindx.wrapping_offset(1);
    minnode = maxnode;
    i = 1;
    while i <= boundarynodes {
        if maxboundary < *boundindx.wrapping_offset(i as isize) {
            maxboundary = *boundindx.wrapping_offset(i as isize);
        }
        if minboundary > *boundindx.wrapping_offset(i as isize) {
            minboundary = *boundindx.wrapping_offset(i as isize);
        }
        if maxnode < *nodeindx.wrapping_offset(i as isize) {
            maxnode = *nodeindx.wrapping_offset(i as isize);
        }
        if minnode > *nodeindx.wrapping_offset(i as isize) {
            minnode = *nodeindx.wrapping_offset(i as isize);
        }
        i += 1;
    }
    if info != 0 {
        printf(
            b"Boundary types are in interval [%d, %d]\n\0".as_ptr() as *const c_char,
            minboundary,
            maxboundary,
        );
        printf(
            b"Boundary nodes are in interval [%d, %d]\n\0".as_ptr() as *const c_char,
            minnode,
            maxnode,
        );
    }
    // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
    indx = ivector(1, (*data).noknots);
    printf(
        b"Allocating hit table of size: %d\n\0".as_ptr() as *const c_char,
        (*data).noknots,
    );
    elemhits = ivector(1, (*data).noknots);
    i = 1;
    while i <= (*data).noknots {
        *elemhits.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    elemind = 1;
    while elemind <= (*data).noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(elemind as isize);
        elemsides = elemtype % 100;
        i = 0;
        while i < elemsides {
            j = *(*(*data).topology.wrapping_offset(elemind as isize)).wrapping_offset(i as isize);
            *elemhits.wrapping_offset(j as isize) += 1;
            i += 1;
        }
        elemind += 1;
    }
    boundarytype = minboundary;
    while boundarytype <= maxboundary {
        // NOTE: block-scoped C locals become a Rust block.
        {
            let mut boundfirst: c_int;
            // NOTE: 0-init for the &mut handoff (set in find_side before reads).
            let mut bchits: c_int = 0;
            // NOTE: 0-init for the &mut handoff (set in find_side before reads).
            let mut bcsame: c_int = 0;
            // NOTE: 0-init for the &mut handoff (GetElementSide overwrites).
            let mut sideelemtype2: c_int = 0;
            let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
            let mut minhits: c_int;
            boundfirst = 0;
            i = 1;
            while i <= (*data).noknots {
                *indx.wrapping_offset(i as isize) = 0;
                i += 1;
            }
            i = 1;
            while i <= boundarynodes {
                if *boundindx.wrapping_offset(i as isize) == boundarytype {
                    *indx.wrapping_offset(*nodeindx.wrapping_offset(i as isize) as isize) = TRUE;
                }
                i += 1;
            }
            elemind = 1;
            while elemind <= (*data).noelements {
                elemtype = *(*data).elementtypes.wrapping_offset(elemind as isize);
                j = LinearNodes(elemtype);
                nohits = 0;
                i = 0;
                while i < j {
                    if *indx.wrapping_offset(
                        *(*(*data).topology.wrapping_offset(elemind as isize))
                            .wrapping_offset(i as isize) as isize,
                    ) != 0
                    {
                        nohits += 1;
                    }
                    i += 1;
                }
                if nohits == 0 {
                    elemind += 1;
                    continue;
                }
                if elemtype > 800 {
                    minhits = 4;
                } else if elemtype > 500 {
                    minhits = 3;
                } else if elemtype > 300 {
                    minhits = 2;
                } else {
                    minhits = 1;
                }
                if nohits < minhits {
                    elemind += 1;
                    continue;
                }
                elemsides = elemtype / 100;
                if elemsides == 8 {
                    elemsides = 6;
                } else if elemsides == 5 {
                    elemsides = 4;
                } else if elemsides == 6 || elemsides == 7 {
                    elemsides = 5;
                }
                // NOTE: `if(0)` diagnostic omitted (runtime-dead).
                side = 0;
                while side < elemsides {
                    find_side(
                        data, bound, indx, &mut sideelem, &mut sideelemtype, &mut sideelemtype2,
                        &mut sideind, &mut sideind2, &mut elemhits, &mut bcsame, &mut bchits,
                        &mut boundfirst, &mut hit, &mut nohits, &mut sidenodes, elemind,
                        elemtype, elemsides, side, boundarytype,
                    );
                    side += 1;
                }
                elemind += 1;
            }
        }
        boundarytype += 1;
    }
    // NOTE: C `free_Ivector` is a macro for `free_ivector` (egutils.h:21).
    free_ivector(indx, 1, (*data).noknots);
    if info != 0 {
        printf(
            b"Found %d side elements formed by %d points.\n\0".as_ptr() as *const c_char,
            sideelem,
            boundarynodes,
        );
    }
    if sideelem != 0 {
        // NOTE: C `MIN` macro (egdef.h:96); arms pure.
        if sideelem < (*bound).nosides {
            (*bound).nosides = sideelem;
        }
    } else {
        if info != 0 {
            printf(b"Adding the points as 101 elements to the mesh instead!\n\0".as_ptr() as *const c_char);
        }
        // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
        (*bound).topology = imatrix(1, boundarynodes, 0, 0);
        (*bound).elementtypes = ivector(1, boundarynodes);
        i = 1;
        while i <= boundarynodes {
            *(*bound).types.wrapping_offset(i as isize) = *boundindx.wrapping_offset(i as isize);
            *(*bound).elementtypes.wrapping_offset(i as isize) = 101;
            *(*(*bound).topology.wrapping_offset(i as isize)).wrapping_offset(0) =
                *nodeindx.wrapping_offset(i as isize);
            i += 1;
        }
    }
}

// SAFETY: single-side parent resolution (split for sizing); the C `goto
// bcset` targets the loop end, i.e. plain continue.
#[allow(clippy::too_many_arguments)]
unsafe fn find_side(
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: *mut c_int,
    sideelem: &mut c_int,
    sideelemtype: &mut c_int,
    sideelemtype2: &mut c_int,
    sideind: &mut [c_int; MAXNODESD1],
    sideind2: &mut [c_int; MAXNODESD1],
    elemhits: &mut *mut c_int,
    bcsame: &mut c_int,
    bchits: &mut c_int,
    boundfirst: &mut c_int,
    hit: &mut c_int,
    nohits: &mut c_int,
    sidenodes: &mut c_int,
    elemind: c_int,
    elemtype: c_int,
    elemsides: c_int,
    side: c_int,
    boundarytype: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    GetElementSide(
        elemind,
        side,
        1,
        data,
        sideind.as_mut_ptr(),
        &mut (*sideelemtype),
    );
    *sidenodes = *sideelemtype % 100;
    // NOTE: `if(0)` diagnostics omitted (runtime-dead).
    *hit = TRUE;
    *nohits = 0;
    i = 0;
    while i < *sidenodes {
        if sideind[i as usize] <= 0 {
            *hit = FALSE;
        } else if sideind[i as usize] > (*data).noknots {
            *hit = FALSE;
        } else if *indx.wrapping_offset(sideind[i as usize] as isize) == 0 {
            *hit = FALSE;
        } else {
            *nohits += 1;
        }
        i += 1;
    }
    // NOTE: `if(0)` diagnostic omitted (runtime-dead).
    if *hit == TRUE {
        // NOTE: if all the points belong to more than one element there
        // may be another parent for the side element.
        *bcsame = FALSE;
        *bchits = TRUE;
        i = 0;
        while i < *sidenodes {
            if *(*elemhits).wrapping_offset(sideind[i as usize] as isize) < 2 {
                *bchits = FALSE;
            }
            i += 1;
        }
        if *bchits != 0 && *boundfirst != 0 {
            j = *boundfirst;
            while j <= *sideelem {
                if *(*bound).parent2.wrapping_offset(j as isize) != 0 {
                    j += 1;
                    continue;
                }
                GetElementSide(
                    *(*bound).parent.wrapping_offset(j as isize),
                    *(*bound).side.wrapping_offset(j as isize),
                    1,
                    data,
                    sideind2.as_mut_ptr(),
                    &mut (*sideelemtype2),
                );
                if *sideelemtype != *sideelemtype2 {
                    j += 1;
                    continue;
                }
                *bcsame = 0;
                i = 0;
                while i < *sidenodes {
                    k = 0;
                    while k < *sidenodes {
                        if sideind[i as usize] == sideind2[k as usize] {
                            *bcsame += 1;
                        }
                        k += 1;
                    }
                    i += 1;
                }
                if *bcsame == *sidenodes {
                    if *(*data).material.wrapping_offset(
                        *(*bound).parent.wrapping_offset(j as isize) as isize,
                    ) > *(*data).material.wrapping_offset(elemind as isize)
                    {
                        *(*bound).parent2.wrapping_offset(j as isize) =
                            *(*bound).parent.wrapping_offset(j as isize);
                        *(*bound).side2.wrapping_offset(j as isize) =
                            *(*bound).side.wrapping_offset(j as isize);
                        *(*bound).parent.wrapping_offset(j as isize) = elemind;
                        *(*bound).side.wrapping_offset(j as isize) = side;
                    } else {
                        *(*bound).parent2.wrapping_offset(j as isize) = elemind;
                        *(*bound).side2.wrapping_offset(j as isize) = side;
                    }
                    // NOTE: C `goto bcset` (loop end) is continue.
                    return;
                }
                j += 1;
            }
        }
        *sideelem += 1;
        if *sideelem > (*bound).nosides {
            printf(
                b"There are more side elements than allocated for (%d vs. %d)\n\0".as_ptr()
                    as *const c_char,
                *sideelem,
                (*bound).nosides,
            );
        }
        *(*bound).parent.wrapping_offset(*sideelem as isize) = elemind;
        *(*bound).side.wrapping_offset(*sideelem as isize) = side;
        *(*bound).parent2.wrapping_offset(*sideelem as isize) = 0;
        *(*bound).side2.wrapping_offset(*sideelem as isize) = 0;
        *(*bound).types.wrapping_offset(*sideelem as isize) = boundarytype;
        if *boundfirst == 0 {
            *boundfirst = *sideelem;
        }
        // NOTE: `bcset:` landing pad (C `goto` target).
    }
    let _ = (elemsides, elemtype);
}
// SAFETY: abaqus side→boundary fill over valid data + sized bound.
unsafe fn AbaqusSideInfoToBoundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nosides: c_int,
    sides: *mut c_int,
    parent: *mut c_int,
    bctypes: *mut c_int,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: `l`/`n` declared but never used in C; dropped.
    let noelements: c_int;
    let noknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemside: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemfamily: c_int;
    // NOTE: `if(0)`-only; dropped with its assignments.
    let mut elemtype: c_int;
    // NOTE: 0-init for the &mut handoff (GetElementSide overwrites).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let minelemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxelemtype: c_int;
    let mut sideind2: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let order8: [c_int; 6] = [4, 5, 0, 1, 2, 3];
    let order7: [c_int; 5] = [3, 4, 0, 1, 2];
    let order6: [c_int; 5] = [4, 0, 1, 2, 3];
    let order5: [c_int; 4] = [3, 0, 1, 2];
    let order4: [c_int; 4] = [0, 1, 2, 3];
    let order3: [c_int; 3] = [0, 1, 2];
    // NOTE: points at one of the tables above (C array decay).
    let mut faceorder: *const c_int;
    let mut zeroparent: c_int;
    let mut zerosides: c_int;
    // NOTE: dead C locals kept read-visible for audit (never read in C either).
    noelements = (*data).noelements;
    noknots = (*data).noknots;
    let _ = (&noelements, &noknots);
    if info != 0 {
        printf(
            b"Making side info of size %d to boundary elements\n\0".as_ptr() as *const c_char,
            nosides,
        );
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        (*bound.wrapping_offset(j as isize)).created = FALSE;
        j += 1;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        (*bound.wrapping_offset(j as isize)).nosides = 0;
        j += 1;
    }
    if nosides == 0 {
        return;
    }
    maxelemtype = GetMaxElementType(data);
    if info != 0 {
        printf(
            b"Leading bulk elementtype is %d\n\0".as_ptr() as *const c_char,
            maxelemtype,
        );
    }
    minelemtype = GetMinElementType(data);
    if info != 0 {
        printf(
            b"Trailing bulk elementtype is %d\n\0".as_ptr() as *const c_char,
            minelemtype,
        );
    }
    // NOTE: `if(0)` dimension queries omitted (runtime-dead as in C).
    AllocateBoundary(bound, nosides);
    // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
    (*bound).topology = imatrix(1, nosides, 0, MAXNODESD2 as c_int - 1);
    i = 1;
    while i <= nosides {
        j = 0;
        while j < MAXNODESD2 as c_int {
            *(*(*bound).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) = 0;
            j += 1;
        }
        i += 1;
    }
    j = 0;
    zeroparent = 0;
    zerosides = 0;
    i = 0;
    while i < nosides {
        if *sides.wrapping_offset(i as isize) == 0 {
            i += 1;
            continue;
        }
        if *parent.wrapping_offset(i as isize) <= 0 {
            zeroparent += 1;
            i += 1;
            continue;
        }
        if *sides.wrapping_offset(i as isize) <= 0 {
            zerosides += 1;
            i += 1;
            continue;
        }
        j += 1;
        *(*bound).parent.wrapping_offset(j as isize) = *parent.wrapping_offset(i as isize);
        elemtype = *(*data)
            .elementtypes
            .wrapping_offset(*parent.wrapping_offset(i as isize) as isize);
        elemfamily = elemtype / 100;
        if elemfamily == 8 {
            faceorder = order8.as_ptr();
        } else if elemfamily == 7 {
            faceorder = order7.as_ptr();
        } else if elemfamily == 6 {
            faceorder = order6.as_ptr();
        } else if elemfamily == 5 {
            faceorder = order5.as_ptr();
        } else if elemfamily == 4 {
            faceorder = order4.as_ptr();
        } else if elemfamily == 3 {
            faceorder = order3.as_ptr();
        } else {
            printf(
                b"Face order not implemented for family: %d\n\0".as_ptr() as *const c_char,
                elemfamily,
            );
            j -= 1;
            i += 1;
            continue;
        }
        elemside = *faceorder.wrapping_offset((*sides.wrapping_offset(i as isize) - 1) as isize);
        *(*bound).side.wrapping_offset(j as isize) = elemside;
        *(*bound).parent2.wrapping_offset(j as isize) = 0;
        *(*bound).side2.wrapping_offset(j as isize) = 0;
        *(*bound).types.wrapping_offset(j as isize) = *bctypes.wrapping_offset(i as isize);
        GetElementSide(
            *parent.wrapping_offset(i as isize),
            elemside,
            1,
            data,
            sideind2.as_mut_ptr(),
            &mut sideelemtype,
        );
        if sideelemtype == 0 {
            printf(
                b"sideele = %d %d\n\0".as_ptr() as *const c_char,
                sideelemtype,
                sideelemtype % 100,
            );
            j -= 1;
        } else {
            k = 0;
            while k < sideelemtype % 100 {
                *(*(*bound).topology.wrapping_offset(j as isize)).wrapping_offset(k as isize) =
                    sideind2[k as usize];
                k += 1;
            }
        }
        i += 1;
    }
    if zeroparent != 0 {
        printf(b"Number of zero parents: %d\n\0".as_ptr() as *const c_char, zeroparent);
    }
    if zerosides != 0 {
        printf(b"Number of zero sides: %d\n\0".as_ptr() as *const c_char, zerosides);
    }
    if info != 0 {
        printf(
            b"Setting side %d elements to be BCs finished!\n\0".as_ptr() as *const c_char,
            j,
        );
    }
    (*bound).nosides = j;
}

// SAFETY: fatal abaqus-input diagnostic (diverges via bigerror).
unsafe fn InpUntreated(cmd: *mut c_char) {
    printf(b"Untreated: %s\0".as_ptr() as *const c_char, cmd);
    bigerror(b"Could not read mesh from Abaqus inp file!\0".as_ptr() as *const c_char);
}

// SAFETY: ignored-command diagnostic.
unsafe fn InpIgnored(cmd: *mut c_char) {
    printf(b"Ignored: %s\0".as_ptr() as *const c_char, cmd);
}

// SAFETY: comment diagnostic.
unsafe fn InpComment(cmd: *mut c_char) {
    printf(b"Comment: %s\0".as_ptr() as *const c_char, cmd);
}
// SAFETY: abaqus .inp reader over caller-owned data/bound; C status codes.
// Two passes (count then fill) exactly as in C (`omstart`/`end` labels).
#[no_mangle]
pub unsafe extern "C" fn LoadAbaqusInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut noknots: c_int;
    let mut noelements: c_int;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut elemcode: c_int;
    let mut maxnodes: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut bodyid: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut maxelem: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut nodeoffset: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut mode: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut allocated: c_int;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut nvalue: c_int = 0;
    // NOTE: C declares `nvalue2` but never uses it; dropped.
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut maxknot: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut nosides: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut ncum: c_int = 0;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut boundarytype: c_int;
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut boundarynodes: c_int;
    // NOTE: C declares `cont` but never uses it; dropped.
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut bcind: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut nodeindx: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut boundindx: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut elemindx: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (strstr-guarded before reads).
    let mut pstr: *mut c_char = core::ptr::null_mut();
    // NOTE: null-init (strstr-guarded before reads).
    let mut pstr2: *mut c_char = core::ptr::null_mut();
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut i: c_int;
    // NOTE: C declares `j`/`k` but the helpers own them now; dropped.
    // NOTE: C declares `l` but never uses it; dropped.
    // NOTE: null-init (Ivector before guarded reads).
    let mut ind: *mut c_int = core::ptr::null_mut();
    let mut input: *mut FILE;
    let mut rvalues: [Real; MAXDOFS] = [0.0; MAXDOFS];
    // NOTE: ivector overwrites unconditionally below before any use.
    let mut ivalues: *mut c_int;
    let mut ivalues0: [c_int; MAXDOFS] = [0; MAXDOFS];
    // NOTE: assigned once before every read (C uninit decl shape).
    let ivaluessize: c_int;
    // NOTE: C declares `setmaterial` but never uses it; dropped.
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut debug: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut firstline: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut generate: c_int = 0;
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut side: c_int = 0;
    // NOTE: C declares `typ` but never uses it; dropped.
    // NOTE: reset before the &mut handoff (pass loop / pre-loop init).
    let mut newsurface: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut bcsides: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut bctypes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut bcparent: *mut c_int = core::ptr::null_mut();
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"inp\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadAbaqusInput: opening of the ABAQUS-file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(
        b"Reading input from ABAQUS input file %s.\n\0".as_ptr() as *const c_char,
        filename.as_ptr(),
    );
    InitializeKnots(data);
    allocated = FALSE;
    maxknot = 0;
    maxelem = 0;
    ivaluessize = MAXDOFS as c_int;
    // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
    ivalues = ivector(0, ivaluessize - 1);
    i = 0;
    while i < ivaluessize {
        *ivalues.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    // NOTE: the file gives no counts; read twice, register on 2nd pass.
    debug = FALSE;
    // NOTE: `omstart:` label (second pass rewinds below).
    loop {
        mode = 0;
        maxnodes = 0;
        noknots = 0;
        noelements = 0;
        nodeoffset = 0;
        elemcode = 0;
        boundarytype = 0;
        boundarynodes = 0;
        bodyid = 0;
        nosides = 0;
        bcind = 0;
        ivalues0[0] = 0;
        ivalues0[1] = 0;
        newsurface = 0;
        loop {
            // NOTE: commented-out GETLINE/END checks omitted (dead in C).
            if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                break;
            }
            // NOTE: commented-out END checks omitted (dead in C).
            if !strrchr(line.as_ptr(), '*' as c_int).is_null() {
                abaqus_star(
                    data, &mut mode, &mut elemcode, &mut elemnodes, &mut maxnodes,
                    &mut noknots, &mut noelements, &mut boundarytype, &mut bodyid,
                    &mut bcind, &mut newsurface, &mut side, &mut generate,
                    &mut firstline, &mut pstr, &mut pstr2, &mut allocated,
                    &mut entityname, line.as_mut_ptr(), input, info,
                );
            } else if mode != 0 {
                abaqus_data(
                    data, &mut mode, elemcode, elemnodes, &mut noknots, &mut noelements,
                    &mut maxknot, &mut maxelem, &mut boundarynodes, &mut boundarytype,
                    &mut bodyid, &mut nosides, &mut bcind, newsurface, &mut side,
                    generate, &mut firstline, &mut ncum, &mut nvalue, &mut ivalues,
                    ivaluessize, &mut ivalues0, &mut nodeindx, &mut boundindx,
                    &mut elemindx, &mut ind, &mut bcsides, &mut bctypes, &mut bcparent,
                    &mut rvalues, &mut entityname, &mut pstr, &mut debug,
                    nodeoffset, line.as_mut_ptr(), input, allocated, info,
                );
            }
        }
        // NOTE: `end:` landing pad (C `goto` target).
        if allocated == 0 {
            rewind(input);
            (*data).noknots = noknots;
            (*data).noelements = noelements;
            (*data).maxnodes = maxnodes;
            (*data).dim = 3;
            if info != 0 {
                printf(
                    b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                    maxnodes,
                );
            }
            AllocateKnots(data);
            if info != 0 {
                printf(
                    b"Number of boundary nodes: %d\n\0".as_ptr() as *const c_char,
                    boundarynodes,
                );
            }
            if boundarynodes > 0 {
                nodeindx = ivector(1, boundarynodes);
                boundindx = ivector(1, boundarynodes);
            }
            if info != 0 {
                printf(
                    b"Maximum temporal index array size: %d\n\0".as_ptr() as *const c_char,
                    ivaluessize,
                );
            }
            // NOTE: C `free_Ivector` is a macro for `free_ivector`.
            free_ivector(ivalues, 0, ivaluessize - 1);
            if info != 0 {
                printf(
                    b"Maximum element index in file: %d\n\0".as_ptr() as *const c_char,
                    maxelem,
                );
            }
            // NOTE: C `MAX` macro (egdef.h:97); arms pure.
            if maxelem < noelements {
                maxelem = noelements;
            }
            elemindx = ivector(1, maxelem);
            i = 1;
            while i <= maxelem {
                *elemindx.wrapping_offset(i as isize) = 0;
                i += 1;
            }
            if info != 0 {
                printf(
                    b"Maximum node index in file: %d\n\0".as_ptr() as *const c_char,
                    maxknot,
                );
            }
            if maxknot < noknots {
                maxknot = noknots;
            }
            ind = ivector(1, maxknot);
            i = 1;
            while i <= maxknot {
                *ind.wrapping_offset(i as isize) = 0;
                i += 1;
            }
            if nosides > 0 {
                bcsides = ivector(0, nosides - 1);
                bctypes = ivector(0, nosides - 1);
                bcparent = ivector(0, nosides - 1);
            }
            allocated = TRUE;
            // NOTE: C `goto omstart` (second pass).
            continue;
        }
        abaqus_tail(
            data, bound, noknots, noelements, maxknot, maxelem, nosides, boundarynodes,
            nodeindx, boundindx, elemindx, ind, bcsides, bctypes, bcparent,
            filename.as_mut_ptr(), input, info,
        );
        return 0;
    }
}
// SAFETY: abaqus `*`-command dispatch (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn abaqus_star(
    data: *mut FemType,
    mode: &mut c_int,
    elemcode: &mut c_int,
    elemnodes: &mut c_int,
    maxnodes: &mut c_int,
    noknots: &mut c_int,
    noelements: &mut c_int,
    boundarytype: &mut c_int,
    bodyid: &mut c_int,
    bcind: &mut c_int,
    newsurface: &mut c_int,
    side: &mut c_int,
    generate: &mut c_int,
    firstline: &mut c_int,
    pstr: &mut *mut c_char,
    pstr2: &mut *mut c_char,
    allocated: &mut c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    line: *mut c_char,
    _input: *mut FILE,
    info: c_int,
) {
    // NOTE: result-to-errors need no "else" (see C comment).
    if !strstr(line, b"NEWSET\0".as_ptr() as *const c_char).is_null() {
        InpUntreated(line);
    }
    if !strstr(line, b"NGEN\0".as_ptr() as *const c_char).is_null() {
        InpUntreated(line);
    }
    if !strstr(line, b"NFILL\0".as_ptr() as *const c_char).is_null() {
        InpUntreated(line);
    }
    if *mode == 2 && *allocated == 0 {
        printf(b"Number of nodes so far: %d\n\0".as_ptr() as *const c_char, *noknots);
    } else if *mode == 3 && *allocated == 0 {
        printf(
            b"Number of elements so far: %d\n\0".as_ptr() as *const c_char,
            *noelements,
        );
        // NOTE: commented-out ELMAT block omitted (dead in C).
    }
    if !strstr(line, b"**\0".as_ptr() as *const c_char).is_null() {
        if info != 0 && *allocated == 0 {
            InpComment(line);
        }
        *mode = 0;
    } else if !strstr(line, b"HEAD\0".as_ptr() as *const c_char).is_null() {
        *mode = 1;
    } else if !strstr(line, b"*NODE\0".as_ptr() as *const c_char).is_null() {
        if !strstr(line, b"NODE OUTPUT\0".as_ptr() as *const c_char).is_null() {
            *mode = 10;
        } else {
            if !strstr(line, b"SYSTEM=R\0".as_ptr() as *const c_char).is_null() {
                (*data).coordsystem = COORD_CART2;
            }
            if !strstr(line, b"SYSTEM=C\0".as_ptr() as *const c_char).is_null() {
                (*data).coordsystem = COORD_AXIS;
            }
            if !strstr(line, b"SYSTEM=P\0".as_ptr() as *const c_char).is_null() {
                (*data).coordsystem = COORD_POLAR;
            }
            *mode = 2;
        }
    } else if !strstr(line, b"*ELEMENT\0".as_ptr() as *const c_char).is_null() {
        if !strstr(line, b"ELEMENT OUTPUT\0".as_ptr() as *const c_char).is_null() {
            *mode = 10;
        } else {
            abaqus_elemcode(line, elemcode, elemnodes, maxnodes);
            *mode = 3;
            if *allocated != 0 && info != 0 {
                printf(
                    b"Loading elements of type %d starting from element %d to body %d.\n\0".as_ptr()
                        as *const c_char,
                    *elemcode,
                    *noelements,
                    *bodyid,
                );
            }
            if !strstr(line, b"ELSET=\0".as_ptr() as *const c_char).is_null() {
                *pstr = strstr(line, b"ELSET=\0".as_ptr() as *const c_char);
                *bodyid += 1;
                if *allocated != 0 {
                    if info != 0 {
                        printf(
                            b"Loading elements to body %d from ELSET %s\0".as_ptr()
                                as *const c_char,
                            *bodyid,
                            (*pstr).wrapping_offset(6),
                        );
                    }
                    sscanf(
                        (*pstr).wrapping_offset(6),
                        b"%s\0".as_ptr() as *const c_char,
                        entityname.as_mut_ptr(),
                    );
                    if (*data).bodyname[*bodyid as usize].is_null() {
                        (*data).bodyname[*bodyid as usize] = cvector(0, MAXNAMESIZE as c_int);
                    }
                    strcpy((*data).bodyname[*bodyid as usize], entityname.as_ptr());
                    (*data).bodynamesexist = TRUE;
                    (*data).boundarynamesexist = TRUE;
                }
            }
            *firstline = TRUE;
        }
    } else if !strstr(line, b"BOUNDARY\0".as_ptr() as *const c_char).is_null() {
        *boundarytype += 1;
        *mode = 4;
        if *allocated != 0 && info != 0 {
            printf(
                b"Treating keyword BOUNDARY for bc %d\n\0".as_ptr() as *const c_char,
                *boundarytype,
            );
        }
    } else if !strstr(line, b"SOLID SECTION\0".as_ptr() as *const c_char).is_null() {
        // NOTE: solid section may include ELSET.
        *mode = 10;
    } else if !strstr(line, b"MEMBRANE SECTION\0".as_ptr() as *const c_char).is_null() {
        // NOTE: solid section may include ELSET.
        *mode = 10;
    } else if !strstr(line, b"CLOAD\0".as_ptr() as *const c_char).is_null() {
        *boundarytype += 1;
        *mode = 4;
        if *allocated != 0 && info != 0 {
            printf(
                b"Treating keyword CLOAD for bc %d\n\0".as_ptr() as *const c_char,
                *boundarytype,
            );
        }
    } else if !strstr(line, b"NSET=\0".as_ptr() as *const c_char).is_null() {
        *pstr = strstr(line, b"NSET=\0".as_ptr() as *const c_char);
        if !strstr(line, b"ELSET=\0".as_ptr() as *const c_char).is_null() {
            // NOTE: skipping association of ELSET to NSET.
            *mode = 10;
        } else {
            *boundarytype += 1;
            *mode = 5;
            if *allocated != 0 && info != 0 {
                printf(
                    b"Treating keyword NSET for bc %d from: %s\0".as_ptr() as *const c_char,
                    *boundarytype,
                    (*pstr).wrapping_offset(5),
                );
            }
        }
    } else if !strstr(line, b"ELSET=\0".as_ptr() as *const c_char).is_null() {
        *pstr = strstr(line, b"ELSET=\0".as_ptr() as *const c_char);
        *mode = 6;
        *generate = FALSE;
        if !strstr(*pstr, b"GENERATE\0".as_ptr() as *const c_char).is_null() {
            *generate = TRUE;
        }
        *side = 0;
        *pstr = (*pstr).wrapping_offset(6);
        if !strstr(line, b"ELSET=_\0".as_ptr() as *const c_char).is_null() {
            *pstr = (*pstr).wrapping_offset(1);
            // NOTE: this numbering is true for Abaqus-to-Elmer hexas only!
            if !strstr(*pstr, b"_S1\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S1\0".as_ptr() as *const c_char);
                *side = 1;
            } else if !strstr(*pstr, b"_S2\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S2\0".as_ptr() as *const c_char);
                *side = 2;
            } else if !strstr(*pstr, b"_S3\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S3\0".as_ptr() as *const c_char);
                *side = 3;
            } else if !strstr(*pstr, b"_S4\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S4\0".as_ptr() as *const c_char);
                *side = 4;
            } else if !strstr(*pstr, b"_S5\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S5\0".as_ptr() as *const c_char);
                *side = 5;
            } else if !strstr(*pstr, b"_S6\0".as_ptr() as *const c_char).is_null() {
                *pstr2 = strstr(*pstr, b"_S6\0".as_ptr() as *const c_char);
                *side = 6;
            }
            if *side == 0 {
                printf(b"Could not determine side!\n\0".as_ptr() as *const c_char);
            }
        }
        if *side > 0 {
            // NOTE: C sets newsurface TRUE in the `if(1)` arm.
            *newsurface = TRUE;
            // NOTE: pstr2 is live here exactly as in C (side>0 implies a match).
            *(*pstr2).wrapping_offset(0) = '\n' as c_char;
            sscanf(*pstr, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
            if *allocated != 0 {
                if info != 0 {
                    printf(
                        b"Loading boundary set %d for side %d of %s\n\0".as_ptr()
                            as *const c_char,
                        *bcind + *newsurface,
                        *side,
                        entityname.as_ptr(),
                    );
                }
                let k = *bcind + *newsurface;
                if k > MAXBCS as c_int {
                    bigerror(b"Boundary set larger than MAXBCS!\0".as_ptr() as *const c_char);
                }
                if (*data).boundaryname[k as usize].is_null() {
                    (*data).boundaryname[k as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy((*data).boundaryname[k as usize], entityname.as_ptr());
                (*data).boundarynamesexist = TRUE;
            }
        } else {
            *bodyid += 1;
            sscanf(*pstr, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
            if *allocated != 0 {
                if info != 0 {
                    printf(
                        b"Loading element to body %d from %s\n\0".as_ptr() as *const c_char,
                        *bodyid,
                        entityname.as_ptr(),
                    );
                }
                if *bodyid > MAXBODIES as c_int {
                    bigerror(b"Body set larger than MAXBODIES!\0".as_ptr() as *const c_char);
                }
                if (*data).bodyname[*bodyid as usize].is_null() {
                    (*data).bodyname[*bodyid as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy((*data).bodyname[*bodyid as usize], entityname.as_ptr());
                (*data).bodynamesexist = TRUE;
            }
        }
    } else if !strstr(line, b"SURFACE\0".as_ptr() as *const c_char).is_null() {
        *pstr = strstr(line, b"SURFACE\0".as_ptr() as *const c_char);
        *bcind = *bcind + *newsurface;
        *newsurface = FALSE;
        *mode = 0;
    } else if !strstr(line, b"PART, NAME=\0".as_ptr() as *const c_char).is_null() {
        *pstr = strstr(line, b"PART, NAME=\0".as_ptr() as *const c_char);
        *bodyid += 1;
        *mode = 6;
        *generate = FALSE;
        if *allocated != 0 {
            if info != 0 {
                printf(
                    b"Loading part to body %d from %s\0".as_ptr() as *const c_char,
                    *bodyid,
                    (*pstr).wrapping_offset(11),
                );
            }
            sscanf(
                (*pstr).wrapping_offset(6),
                b"%s\0".as_ptr() as *const c_char,
                entityname.as_mut_ptr(),
            );
            if (*data).bodyname[*bodyid as usize].is_null() {
                (*data).bodyname[*bodyid as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy((*data).bodyname[*bodyid as usize], entityname.as_ptr());
            (*data).bodynamesexist = TRUE;
            (*data).boundarynamesexist = TRUE;
        }
    } else if !strstr(line, b"HWCOLOR\0".as_ptr() as *const c_char).is_null() {
        *pstr = strstr(line, b"HWCOLOR\0".as_ptr() as *const c_char);
        // NOTE: unused command.
        *mode = 0;
    } else {
        if *allocated == 0 {
            InpIgnored(line);
        }
        *mode = 0;
    }
}

// SAFETY: abaqus element-code map (pure table).
unsafe fn abaqus_elemcode(line: *mut c_char, elemcode: &mut c_int, elemnodes: &mut c_int, maxnodes: &mut c_int) {
    if !strstr(line, b"S3\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"STRI3\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"M3D3\0".as_ptr() as *const c_char).is_null()
    {
        *elemcode = 303;
    } else if !strstr(line, b"2D4\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"SP4\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"AX4\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"S4\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"CPE4\0".as_ptr() as *const c_char).is_null()
    {
        *elemcode = 404;
    } else if !strstr(line, b"2D8\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"AX8\0".as_ptr() as *const c_char).is_null()
        || !strstr(line, b"DS8\0".as_ptr() as *const c_char).is_null()
    {
        *elemcode = 408;
    } else if !strstr(line, b"3D4\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 504;
    } else if !strstr(line, b"3D10\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 510;
    } else if !strstr(line, b"3D5\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 605;
    } else if !strstr(line, b"3D6\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 706;
    } else if !strstr(line, b"3D15\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 715;
    } else if !strstr(line, b"3D8\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 808;
    } else if !strstr(line, b"3D20\0".as_ptr() as *const c_char).is_null() {
        *elemcode = 820;
    } else {
        printf(b"Unknown element code: %s\n\0".as_ptr() as *const c_char, line);
        bigerror(b"Cannot continue without all elements!\0".as_ptr() as *const c_char);
    }
    *elemnodes = *elemcode % 100;
    // NOTE: C `MAX` macro (egdef.h:97); arms pure.
    if *elemnodes > *maxnodes {
        *maxnodes = *elemnodes;
    }
}
// SAFETY: abaqus data-line modes 1-6,10 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn abaqus_data(
    data: *mut FemType,
    mode: &mut c_int,
    elemcode: c_int,
    elemnodes: c_int,
    noknots: &mut c_int,
    noelements: &mut c_int,
    maxknot: &mut c_int,
    maxelem: &mut c_int,
    boundarynodes: &mut c_int,
    boundarytype: &mut c_int,
    bodyid: &mut c_int,
    nosides: &mut c_int,
    bcind: &mut c_int,
    newsurface: c_int,
    side: &mut c_int,
    generate: c_int,
    firstline: &mut c_int,
    ncum: &mut c_int,
    nvalue: &mut c_int,
    ivalues: &mut *mut c_int,
    ivaluessize: c_int,
    ivalues0: &mut [c_int; MAXDOFS],
    nodeindx: &mut *mut c_int,
    boundindx: &mut *mut c_int,
    elemindx: &mut *mut c_int,
    ind: &mut *mut c_int,
    bcsides: &mut *mut c_int,
    bctypes: &mut *mut c_int,
    bcparent: &mut *mut c_int,
    rvalues: &mut [Real; MAXDOFS],
    entityname: &mut [c_char; MAXNAMESIZE],
    pstr: &mut *mut c_char,
    debug: &mut c_int,
    nodeoffset: c_int,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    let _ = (entityname, pstr, nodeoffset);
    // NOTE: C switch over mode 1-6,10.
    if *mode == 1 {
        if info != 0 {
            printf(b"Loading Abaqus input file:\n%s\0".as_ptr() as *const c_char, line);
        }
    } else if *mode == 2 {
        // NOTE: NODE.
        *nvalue = StringToReal(line, rvalues.as_mut_ptr(), MAXNODESD2 as c_int + 1, ',' as c_char);
        if *nvalue > 4 || *nvalue < 2 {
            printf(b"line: %s\n\0".as_ptr() as *const c_char, line);
            printf(b"Invalid nvalue = %d\n\0".as_ptr() as *const c_char, *nvalue);
        }
        i = (rvalues[0] + 0.5) as c_int;
        *noknots += 1;
        if allocated != 0 {
            if *debug != 0 && (i == 1 || i == *maxknot) {
                printf(
                    b"debug node: %i %d %.3le %.3le %.3le\n\0".as_ptr() as *const c_char,
                    i,
                    *noknots,
                    rvalues[1],
                    rvalues[2],
                    rvalues[3],
                );
            }
            // NOTE: C `MAX` macro (egdef.h:97); arms pure.
            if i < *noknots {
                i = *noknots;
            }
            if i <= 0 || i > *maxknot {
                printf(b"Invalid node index = %d\n\0".as_ptr() as *const c_char, i);
            } else {
                *(*ind).wrapping_offset(i as isize) = *noknots;
                if *nvalue >= 2 {
                    *(*data).x.wrapping_offset(*noknots as isize) = rvalues[1];
                }
                if *nvalue >= 3 {
                    *(*data).y.wrapping_offset(*noknots as isize) = rvalues[2];
                }
                if *nvalue >= 4 {
                    *(*data).z.wrapping_offset(*noknots as isize) = rvalues[3];
                }
            }
        } else if *maxknot < i {
            *maxknot = i;
        }
    } else if *mode == 3 {
        // NOTE: ELEMENT.
        *noelements += 1;
        *nvalue = StringToIntegerNoZero(line, *ivalues, elemnodes + 1, ',' as c_char);
        if allocated != 0 {
            if *debug != 0 && *firstline != 0 {
                printf(
                    b"debug elem: %d %d %d %d\n\0".as_ptr() as *const c_char,
                    *noelements,
                    *(*ivalues).wrapping_offset(0),
                    elemcode,
                    *bodyid,
                );
                printf(b"      topo:\0".as_ptr() as *const c_char);
                i = 0;
                while i < *nvalue - 1 {
                    printf(
                        b" %d\0".as_ptr() as *const c_char,
                        *(*ivalues).wrapping_offset((i + 1) as isize),
                    );
                    i += 1;
                }
                printf(b"\n\0".as_ptr() as *const c_char);
                *firstline = FALSE;
            }
            *(*elemindx).wrapping_offset(*(*ivalues).wrapping_offset(0) as isize) = *noelements;
            *(*data).elementtypes.wrapping_offset(*noelements as isize) = elemcode;
            *(*data).material.wrapping_offset(*noelements as isize) = *bodyid;
            i = 0;
            while i < *nvalue - 1 {
                *(*(*data).topology.wrapping_offset(*noelements as isize))
                    .wrapping_offset(i as isize) =
                    *(*ivalues).wrapping_offset((i + 1) as isize);
                i += 1;
            }
            if nodeoffset != 0 {
                i = 0;
                while i < *nvalue - 1 {
                    *(*(*data).topology.wrapping_offset(*noelements as isize))
                        .wrapping_offset(i as isize) += nodeoffset;
                    i += 1;
                }
            }
        } else if *maxelem < *(*ivalues).wrapping_offset(0) {
            *maxelem = *(*ivalues).wrapping_offset(0);
        }
        *ncum = *nvalue - 1;
        // NOTE: read 2nd line if needed.
        if *ncum < elemnodes {
            Getrow(line, input, TRUE);
            *nvalue = StringToIntegerNoZero(line, *ivalues, elemnodes - *ncum, ',' as c_char);
            if allocated != 0 {
                i = 0;
                while i < *nvalue {
                    *(*(*data).topology.wrapping_offset(*noelements as isize))
                        .wrapping_offset((*ncum + i) as isize) =
                        *(*ivalues).wrapping_offset(i as isize);
                    i += 1;
                }
            }
            *ncum = *ncum + *nvalue;
        }
        // NOTE: be prepared for 3rd line as well.
        if *ncum < elemnodes {
            Getrow(line, input, TRUE);
            *nvalue = StringToIntegerNoZero(line, *ivalues, elemnodes - *ncum, ',' as c_char);
            if allocated != 0 {
                i = 0;
                while i < *nvalue {
                    *(*(*data).topology.wrapping_offset(*noelements as isize))
                        .wrapping_offset((*ncum + i) as isize) =
                        *(*ivalues).wrapping_offset(i as isize);
                    i += 1;
                }
            }
            *ncum = *ncum + *nvalue;
        }
        if *ncum != elemnodes {
            printf(b"ncum = %d vs. %d\n\0".as_ptr() as *const c_char, *ncum, elemnodes);
        }
        if allocated != 0 {
            j = FALSE;
            i = 0;
            while i < elemnodes {
                if *(*(*data).topology.wrapping_offset(*noelements as isize))
                    .wrapping_offset(i as isize)
                    == 0
                {
                    j = TRUE;
                }
                i += 1;
            }
            if j != 0 {
                printf(b"zero in this element\n\0".as_ptr() as *const c_char);
                printf(
                    b"element = %d %d\n\0".as_ptr() as *const c_char,
                    *noelements,
                    elemnodes,
                );
                i = 0;
                while i < elemnodes {
                    printf(
                        b"%d \0".as_ptr() as *const c_char,
                        *(*(*data).topology.wrapping_offset(*noelements as isize))
                            .wrapping_offset(i as isize),
                    );
                    i += 1;
                }
                printf(b"\n\0".as_ptr() as *const c_char);
            }
        }
    } else if *mode == 4 {
        *nvalue = StringToInteger(line, *ivalues, 2, ',' as c_char);
        if *(*ivalues).wrapping_offset(0) == ivalues0[0]
            && *(*ivalues).wrapping_offset(1) != ivalues0[1]
        {
            return;
        }
        ivalues0[0] = *(*ivalues).wrapping_offset(0);
        ivalues0[1] = *(*ivalues).wrapping_offset(1);
        *boundarynodes += 1;
        if allocated != 0 {
            *(*nodeindx).wrapping_offset(*boundarynodes as isize) =
                *(*ivalues).wrapping_offset(0);
            *(*boundindx).wrapping_offset(*boundarynodes as isize) = *boundarytype;
        }
    } else if *mode == 5 {
        // NOTE: NSET.
        *nvalue = StringToIntegerNoZero(line, *ivalues, ivaluessize, ',' as c_char);
        if allocated != 0 {
            i = 0;
            while i < *nvalue {
                *boundarynodes += 1;
                *(*nodeindx).wrapping_offset(*boundarynodes as isize) =
                    *(*ivalues).wrapping_offset(i as isize);
                *(*boundindx).wrapping_offset(*boundarynodes as isize) = *boundarytype;
                // NOTE: `if(0)` diagnostic omitted (runtime-dead).
                i += 1;
            }
        } else {
            *boundarynodes += *nvalue;
        }
    } else if *mode == 6 {
        // NOTE: ELSET.
        *nvalue = StringToIntegerNoZero(line, *ivalues, ivaluessize, ',' as c_char);
        if generate != 0 {
            *nvalue = (*(*ivalues).wrapping_offset(1) - *(*ivalues).wrapping_offset(0))
                / *(*ivalues).wrapping_offset(2)
                + 1;
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        }
        if allocated != 0 {
            if generate != 0 {
                i = 0;
                j = *(*ivalues).wrapping_offset(0);
                while j <= *(*ivalues).wrapping_offset(1) {
                    if *side != 0 {
                        *(*bcsides).wrapping_offset((*nosides + i) as isize) = *side;
                        *(*bctypes).wrapping_offset((*nosides + i) as isize) =
                            *bcind + newsurface;
                        *(*bcparent).wrapping_offset((*nosides + i) as isize) =
                            *(*elemindx).wrapping_offset(j as isize);
                        i += 1;
                    } else {
                        *(*data).material.wrapping_offset(
                            *(*elemindx).wrapping_offset(j as isize) as isize,
                        ) = *bodyid;
                    }
                    j += *(*ivalues).wrapping_offset(2);
                }
            } else {
                i = 0;
                while i < *nvalue {
                    if *side != 0 {
                        *(*bcsides).wrapping_offset((*nosides + i) as isize) = *side;
                        *(*bctypes).wrapping_offset((*nosides + i) as isize) =
                            *bcind + newsurface;
                        *(*bcparent).wrapping_offset((*nosides + i) as isize) =
                            *(*elemindx).wrapping_offset(
                                *(*ivalues).wrapping_offset(i as isize) as isize,
                            );
                    } else {
                        j = *(*elemindx).wrapping_offset(
                            *(*ivalues).wrapping_offset(i as isize) as isize,
                        );
                        *(*data).material.wrapping_offset(j as isize) = *bodyid;
                    }
                    i += 1;
                }
            }
        }
        if *side != 0 {
            *nosides += *nvalue;
        }
    } else if *mode == 10 {
        // NOTE: doing nothing.
    } else {
        printf(b"Unknown case: %d\n\0".as_ptr() as *const c_char, *mode);
    }
}
// SAFETY: abaqus post-pass (renumber + boundaries + cleanup); C status 0.
#[allow(clippy::too_many_arguments)]
unsafe fn abaqus_tail(
    data: *mut FemType,
    bound: *mut BoundaryType,
    noknots: c_int,
    noelements: c_int,
    maxknot: c_int,
    maxelem: c_int,
    nosides: c_int,
    boundarynodes: c_int,
    nodeindx: *mut c_int,
    boundindx: *mut c_int,
    elemindx: *mut c_int,
    ind: *mut c_int,
    bcsides: *mut c_int,
    bctypes: *mut c_int,
    bcparent: *mut c_int,
    filename: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned at loop head before every read.
    let mut elemcode: c_int;
    // NOTE: reset before every use (pass loop / call return).
    let mut elemnodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut errcount: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut okcount: c_int;
    if info != 0 {
        printf(b"The mesh was loaded from file %s.\n\0".as_ptr() as *const c_char, filename);
    }
    // NOTE: abaqus numbering may skip; renumber 1..noknots.
    if noknots != maxknot {
        if info != 0 {
            printf(
                b"There are %d nodes but maximum index is %d.\n\0".as_ptr() as *const c_char,
                noknots,
                maxknot,
            );
            printf(b"Renumbering %d elements\n\0".as_ptr() as *const c_char, noelements);
        }
        errcount = 0;
        okcount = 0;
        j = 1;
        while j <= noelements {
            elemcode = *(*data).elementtypes.wrapping_offset(j as isize);
            elemnodes = elemcode % 100;
            i = 0;
            while i < elemnodes {
                k = *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize);
                if k <= 0 {
                    printf(
                        b"err elem ind: %d %d %d %d\n\0".as_ptr() as *const c_char,
                        j,
                        elemcode,
                        i,
                        k,
                    );
                    errcount += 1;
                } else {
                    *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                        *ind.wrapping_offset(k as isize);
                    okcount += 1;
                }
                i += 1;
            }
            j += 1;
        }
        if info != 0 {
            printf(
                b"There are %d positive and %d non-positive indexes in elements!\n\0".as_ptr()
                    as *const c_char,
                okcount,
                errcount,
            );
            printf(
                b"Renumbering %d nodes in node sets\n\0".as_ptr() as *const c_char,
                boundarynodes,
            );
        }
        errcount = 0;
        okcount = 0;
        j = 1;
        while j <= boundarynodes {
            k = *nodeindx.wrapping_offset(j as isize);
            if k <= 0 || k > maxknot {
                printf(b"err node set ind: %d %d\n\0".as_ptr() as *const c_char, j, k);
                errcount += 1;
            } else {
                *nodeindx.wrapping_offset(j as isize) = *ind.wrapping_offset(k as isize);
                okcount += 1;
            }
            j += 1;
        }
        printf(
            b"There are %d positive and %d non-positive indexes in node sets!\n\0".as_ptr()
                as *const c_char,
            okcount,
            errcount,
        );
    }
    {
        // NOTE: block-scoped C locals become a Rust block.
        let mut maxid: c_int;
        let mut mincnt: c_int;
        maxid = *(*data).material.wrapping_offset(1);
        mincnt = 0;
        i = 1;
        while i <= noelements {
            // NOTE: C `MAX` macro (egdef.h:97); arms pure.
            if maxid < *(*data).material.wrapping_offset(i as isize) {
                maxid = *(*data).material.wrapping_offset(i as isize);
            }
            if *(*data).material.wrapping_offset(i as isize) == 0 {
                mincnt += 1;
            }
            i += 1;
        }
        if mincnt != 0 {
            maxid += 1;
            if info != 0 {
                printf(
                    b"Setting %d unset elements to body index %d and name 'DefaulBody'\n\0".as_ptr()
                        as *const c_char,
                    mincnt,
                    maxid,
                );
            }
            i = 1;
            while i <= noelements {
                if *(*data).material.wrapping_offset(i as isize) == 0 {
                    *(*data).material.wrapping_offset(i as isize) = maxid;
                }
                i += 1;
            }
            if (*data).bodyname[maxid as usize].is_null() {
                (*data).bodyname[maxid as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy((*data).bodyname[maxid as usize], b"DefaultBody\0".as_ptr() as *const c_char);
            (*data).bodynamesexist = TRUE;
            (*data).boundarynamesexist = TRUE;
        }
        // NOTE: the underscore suggests special Abaqus commands; scrap that.
        i = 1;
        while i <= maxid {
            if !(*data).bodyname[i as usize].is_null() {
                // NOTE: `pstr` here is the outer pstr; use a fresh local.
                let hit: *mut c_char = strchr((*data).bodyname[i as usize], ',' as c_int);
                if !hit.is_null() {
                    *hit.wrapping_offset(0) = 0 as c_char;
                }
            }
            i += 1;
        }
    }
    if nosides != 0 {
        if info != 0 {
            printf(b"Creating boundary elements from side pointers!\n\0".as_ptr() as *const c_char);
        }
        AbaqusSideInfoToBoundary(data, bound, nosides, bcsides, bcparent, bctypes, info);
    } else if boundarynodes != 0 {
        if info != 0 {
            printf(b"Creating boundary elements from node set!\n\0".as_ptr() as *const c_char);
        }
        FindPointParents(data, bound, boundarynodes, nodeindx, boundindx, info);
        // NOTE: `if(0)` ElementsToBoundaryConditions omitted (dead in C).
    }
    free_ivector(ind, 1, maxknot);
    // NOTE: C `free_Ivector` is a macro for `free_ivector` (egutils.h:21).
    free_ivector(elemindx, 1, maxelem);
    if boundarynodes > 0 {
        if info != 0 {
            printf(
                b"Number of nodes in boundary sets: %d\n\0".as_ptr() as *const c_char,
                boundarynodes,
            );
        }
        // NOTE: deallocation kept although C muses about suppressing it.
        free_ivector(nodeindx, 1, boundarynodes);
        free_ivector(boundindx, 1, boundarynodes);
    }
    fclose(input);
    if info != 0 {
        printf(b"Finished reading ABAQUS input file\n\0".as_ptr() as *const c_char);
    }
}
// NOTE: C function-local statics hoisted (single-threaded utility).
static mut READABAQUS_MAXARGNO: c_int = 0;
static mut READABAQUS_MODE: c_int = 0;

// SAFETY: abaqus .fil field reader over a valid stream.
unsafe fn ReadAbaqusField(
    input: *mut FILE,
    buffer: *mut c_char,
    argtype: *mut c_int,
    argno: *mut c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut val: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let digits: c_int;
    val = fgetc(input);
    if val == EOF {
        return -1;
    }
    if val == '\n' as c_int {
        val = fgetc(input);
    }
    if val == '*' as c_int {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        if *argno != READABAQUS_MAXARGNO {
            printf(b"The previous field was of wrong length, debugging time!\n\0".as_ptr() as *const c_char);
        }
        *argno = 0;
        READABAQUS_MODE = 0;
        val = fgetc(input);
        if val == '\n' as c_int {
            val = fgetc(input);
        }
    }
    if val == 'I' as c_int {
        i = 0;
        while i < 2 {
            val = fgetc(input);
            if val == '\n' as c_int {
                val = fgetc(input);
            }
            *buffer.wrapping_offset(i as isize) = val as c_char;
            i += 1;
        }
        *buffer.wrapping_offset(2) = 0 as c_char;
        digits = atoi(buffer);
        i = 0;
        while i < digits {
            val = fgetc(input);
            if val == '\n' as c_int {
                val = fgetc(input);
            }
            *buffer.wrapping_offset(i as isize) = val as c_char;
            i += 1;
        }
        *buffer.wrapping_offset(digits as isize) = 0 as c_char;
        *argno += 1;
        *argtype = 1;
        if *argno == 1 {
            READABAQUS_MAXARGNO = atoi(buffer);
        }
        if *argno == 2 {
            READABAQUS_MODE = atoi(buffer);
        }
    } else if val == 'D' as c_int {
        i = 0;
        while i < 22 {
            val = fgetc(input);
            if val == '\n' as c_int {
                val = fgetc(input);
            }
            if val == 'D' as c_int {
                val = 'E' as c_int;
            }
            *buffer.wrapping_offset(i as isize) = val as c_char;
            i += 1;
        }
        *buffer.wrapping_offset(22) = 0 as c_char;
        *argno += 1;
        *argtype = 2;
    } else if val == 'A' as c_int {
        i = 0;
        while i < 8 {
            val = fgetc(input);
            if val == '\n' as c_int {
                val = fgetc(input);
            }
            *buffer.wrapping_offset(i as isize) = val as c_char;
            i += 1;
        }
        *buffer.wrapping_offset(8) = 0 as c_char;
        *argno += 1;
        *argtype = 3;
    } else {
        *buffer.wrapping_offset(0) = val as c_char;
        *buffer.wrapping_offset(1) = 0 as c_char;
        *argtype = 0;
    }
    READABAQUS_MODE
}
// SAFETY: abaqus .fil reader over caller-owned data; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadAbaqusOutput(
    data: *mut FemType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: `knotno = 0` follows before every use.
    let mut knotno: c_int;
    let mut elemno: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elset: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut secno: c_int;
    // NOTE: 0-init (ReadAbaqusField overwrites before reads).
    let mut argtype: c_int = 0;
    // NOTE: `argno = 0` follows before the &mut handoff.
    let mut argno: c_int;
    // NOTE: reset before every use (pass loop / call return).
    let mut mode: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut buffer: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut prevdog: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nodogs: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ignored: c_int;
    let input: *mut FILE;
    // NOTE: null-init (Ivector before guarded reads).
    let mut indx: *mut c_int = core::ptr::null_mut();
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"fil\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadAbaqusOutput: opening of the Abaqus-file '%s' wasn't successful !\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Reading input from ABAQUS output file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitializeKnots(data);
    allocated = FALSE;
    // NOTE: C zeroes `mode` here, but the ReadAbaqusField return
    // overwrites it before every read.
    knotno = 0;
    elemno = 0;
    nodogs = 0;
    prevdog = 0;
    argno = 0;
    elset = 0;
    secno = 0;
    ignored = 0;
    (*data).maxnodes = 9;
    (*data).dim = 3;
    loop {
        mode = ReadAbaqusField(input, buffer.as_mut_ptr(), &mut argtype, &mut argno);
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        // NOTE: C switch over mode; fallthrough arms merged below.
        if mode == -1 {
            break;
        } else if mode == 0 {
        } else if mode == 1921 {
            // NOTE: general info.
            if argno == 3 {
                printf(b"Reading output file for Abaqus %s\n\0".as_ptr() as *const c_char, buffer.as_ptr());
            } else if argno == 4 {
                printf(b"Created on %s\0".as_ptr() as *const c_char, buffer.as_ptr());
            } else if argno == 5 {
                printf(b"%s\0".as_ptr() as *const c_char, buffer.as_ptr());
            } else if argno == 6 {
                printf(b"%s\n\0".as_ptr() as *const c_char, buffer.as_ptr());
            } else if argno == 7 {
                (*data).noelements = atoi(buffer.as_ptr());
            } else if argno == 8 && allocated == FALSE {
                (*data).noknots = atoi(buffer.as_ptr());
                allocated = TRUE;
                AllocateKnots(data);
                // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
                indx = ivector(0, 2 * (*data).noknots);
                i = 1;
                while i <= 2 * (*data).noknots {
                    *indx.wrapping_offset(i as isize) = 0;
                    i += 1;
                }
            }
        } else if mode == 1900 {
            // NOTE: element definition.
            if argno == 3 {
                elemno = atoi(buffer.as_ptr());
            } else if argno == 4 {
                if !strstr(buffer.as_ptr(), b"2D4\0".as_ptr() as *const c_char).is_null()
                    || !strstr(buffer.as_ptr(), b"SP4\0".as_ptr() as *const c_char).is_null()
                    || !strstr(buffer.as_ptr(), b"AX4\0".as_ptr() as *const c_char).is_null()
                {
                    *(*data).elementtypes.wrapping_offset(elemno as isize) = 404;
                } else if !strstr(buffer.as_ptr(), b"2D8\0".as_ptr() as *const c_char).is_null()
                    || !strstr(buffer.as_ptr(), b"AX8\0".as_ptr() as *const c_char).is_null()
                    || !strstr(buffer.as_ptr(), b"S8R5\0".as_ptr() as *const c_char).is_null()
                {
                    *(*data).elementtypes.wrapping_offset(elemno as isize) = 408;
                } else if !strstr(buffer.as_ptr(), b"3D8\0".as_ptr() as *const c_char).is_null() {
                    *(*data).elementtypes.wrapping_offset(elemno as isize) = 808;
                } else {
                    printf(b"Unknown element code: %s\n\0".as_ptr() as *const c_char, buffer.as_ptr());
                }
            } else if argno >= 5 {
                *(*(*data).topology.wrapping_offset(elemno as isize))
                    .wrapping_offset((argno - 5) as isize) = atoi(buffer.as_ptr());
            }
        } else if mode == 1901 {
            // NOTE: node definition.
            if argno == 3 {
                knotno += 1;
                if atoi(buffer.as_ptr()) > 2 * (*data).noknots {
                    printf(b"LoadAbaqusOutput: allocate more space for indx.\n\0".as_ptr() as *const c_char);
                } else {
                    *indx.wrapping_offset(atoi(buffer.as_ptr()) as isize) = knotno;
                }
            }
            if argno == 4 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).x.wrapping_offset(knotno as isize)),
                );
            }
            if argno == 5 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).y.wrapping_offset(knotno as isize)),
                );
            }
            if argno == 6 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).z.wrapping_offset(knotno as isize)),
                );
            }
        } else if mode == 1933 {
            // NOTE: element set (falls through to 1934 in C).
            if argno == 3 {
                elset += 1;
                if (*data).bodyname[elset as usize].is_null() {
                    (*data).bodyname[elset as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy((*data).bodyname[elset as usize], buffer.as_ptr());
            }
            if argno > 3 {
                elemno = atoi(buffer.as_ptr());
                *(*data).material.wrapping_offset(elemno as isize) = elset;
            }
        } else if mode == 1934 {
            // NOTE: element set continuation.
            if argno > 3 {
                elemno = atoi(buffer.as_ptr());
                *(*data).material.wrapping_offset(elemno as isize) = elset;
            }
        } else if mode == 2001 {
            // NOTE: just ignore.
        } else if mode == 1 {
            if argno == 3 {
                knotno = *indx.wrapping_offset(atoi(buffer.as_ptr()) as isize);
            }
            if argno == 5 {
                secno = atoi(buffer.as_ptr());
            }
        } else if mode == 2 {
            if prevdog != mode {
                prevdog = mode;
                nodogs += 1;
                CreateVariable(data, nodogs, 1, 0.0, b"Temperature\0".as_ptr() as *const c_char, FALSE);
            }
        } else if mode == 11 {
            // NOTE: falls through to 12 in C with mode STILL 11, so the
            // 12-guard (`prevdog != mode`) fails and no Invariants
            // variable is created on this path; the shared body runs.
            if prevdog != mode {
                prevdog = mode;
                nodogs += 1;
                CreateVariable(data, nodogs, 3, 0.0, b"Stress\0".as_ptr() as *const c_char, FALSE);
            }
            if secno == 1 && argno == 3 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 2) as isize)),
                );
            }
            if secno == 1 && argno == 4 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 1) as isize)),
                );
            }
            if secno == 1 && argno == 5 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno) as isize)),
                );
            }
        } else if mode == 12 {
            if prevdog != mode {
                prevdog = mode;
                nodogs += 1;
                CreateVariable(data, nodogs, 3, 0.0, b"Invariants\0".as_ptr() as *const c_char, FALSE);
            }
            if secno == 1 && argno == 3 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 2) as isize)),
                );
            }
            if secno == 1 && argno == 4 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 1) as isize)),
                );
            }
            if secno == 1 && argno == 5 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno) as isize)),
                );
            }
        } else if mode == 101 {
            // NOTE: falls through to 102 in C with mode STILL 101, so the
            // 102-guard fails and no Velocity variable is created here.
            if prevdog != mode {
                prevdog = mode;
                nodogs += 1;
                CreateVariable(data, nodogs, 3, 0.0, b"Displacement\0".as_ptr() as *const c_char, FALSE);
            }
            if argno == 3 {
                knotno = *indx.wrapping_offset(atoi(buffer.as_ptr()) as isize);
            }
            if argno == 4 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 2) as isize)),
                );
            }
            if argno == 5 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 1) as isize)),
                );
            }
            if argno == 6 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno) as isize)),
                );
            }
        } else if mode == 102 {
            if prevdog != mode {
                prevdog = mode;
                nodogs += 1;
                CreateVariable(data, nodogs, 3, 0.0, b"Velocity\0".as_ptr() as *const c_char, FALSE);
            }
            if argno == 3 {
                knotno = *indx.wrapping_offset(atoi(buffer.as_ptr()) as isize);
            }
            if argno == 4 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 2) as isize)),
                );
            }
            if argno == 5 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno - 1) as isize)),
                );
            }
            if argno == 6 {
                sscanf(
                    buffer.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*(*data).dofs[nodogs as usize]
                        .wrapping_offset((3 * knotno) as isize)),
                );
            }
        } else if ignored != mode {
            printf(b"Record %d was ignored!\n\0".as_ptr() as *const c_char, mode);
            ignored = mode;
        }
    }
    // NOTE: `jump:` landing pad (C `goto` target).
    if info != 0 {
        printf(b"Renumbering elements\n\0".as_ptr() as *const c_char);
    }
    j = 1;
    while j <= (*data).noelements {
        i = 0;
        while i < *(*data).elementtypes.wrapping_offset(j as isize) % 100 {
            *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                *indx.wrapping_offset(
                    *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize)
                        as isize,
                );
            i += 1;
        }
        j += 1;
    }
    free_ivector(indx, 0, 2 * (*data).noknots);
    fclose(input);
    if info != 0 {
        printf(
            b"LoadAbacusInput: results were loaded from file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}
// SAFETY: nastran reader over caller-owned data; C status codes.
// Two passes (count then fill) exactly as in C (`omstart`/`end`).
#[no_mangle]
pub unsafe extern "C" fn LoadNastranInput(
    data: *mut FemType,
    _bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: reset before every use (pass loop / call return).
    let mut noknots: c_int;
    // NOTE: reset before every use (pass loop / call return).
    let mut noelements: c_int;
    // NOTE: reset before every use (pass loop).
    let mut maxnodes: c_int;
    let mut allocated: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxknot: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minknot: c_int;
    // NOTE: assigned in element arms before reads.
    let mut nodes: c_int;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (next_* overwrites via &mut before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (nastran_elem overwrites first).
    let mut k: c_int = 0;
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"nas\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadNastranInput: opening of the Nastran file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    if info != 0 {
        printf(
            b"Reading mesh from Nastran file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitializeKnots(data);
    allocated = FALSE;
    maxknot = 0;
    minknot = 10000;
    // NOTE: the file gives no counts; read twice, register on 2nd pass.
    // NOTE: `omstart:` label (second pass rewinds below).
    loop {
        maxnodes = 0;
        noknots = 0;
        noelements = 0;
        loop {
            // NOTE: commented-out GETLINE omitted (dead in C).
            if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                break;
            }
            if line[0] == '$' as c_char {
                if allocated == 0 {
                    InpComment(line.as_mut_ptr());
                }
            } else if !strstr(line.as_ptr(), b"GRID\0".as_ptr() as *const c_char).is_null() {
                noknots += 1;
                // NOTE: `if(0)` diagnostics omitted (runtime-dead).
                cp = line.as_mut_ptr().wrapping_offset(5);
                j = next_int(&mut cp);
                // NOTE: `if(0)` diagnostic omitted (runtime-dead).
                if allocated != 0 {
                    k = next_int(&mut cp);
                    let _ = k;
                    *(*data).x.wrapping_offset(noknots as isize) = next_real(&mut cp);
                    *(*data).y.wrapping_offset(noknots as isize) = next_real(&mut cp);
                } else {
                    if j > maxknot {
                        maxknot = j;
                    }
                    if j < minknot {
                        minknot = j;
                    }
                }
                if !strstr(line.as_ptr(), b"*\0".as_ptr() as *const c_char).is_null() {
                    Getrow(line.as_mut_ptr(), input, TRUE);
                }
                if allocated != 0 {
                    cp = line.as_mut_ptr().wrapping_offset(4);
                    *(*data).z.wrapping_offset(noknots as isize) = next_real(&mut cp);
                }
            } else if !strstr(line.as_ptr(), b"TETRA\0".as_ptr() as *const c_char).is_null() {
                noelements += 1;
                nodes = 4;
                if nodes > maxnodes {
                    maxnodes = nodes;
                }
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(noelements as isize) = 504;
                    nastran_elem(data, noelements, nodes, line.as_mut_ptr(), &mut cp, &mut k, 6);
                }
            } else if !strstr(line.as_ptr(), b"PYRAM\0".as_ptr() as *const c_char).is_null() {
                noelements += 1;
                nodes = 5;
                if nodes > maxnodes {
                    maxnodes = nodes;
                }
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(noelements as isize) = 605;
                    nastran_elem(data, noelements, nodes, line.as_mut_ptr(), &mut cp, &mut k, 6);
                }
            } else if !strstr(line.as_ptr(), b"PENTA\0".as_ptr() as *const c_char).is_null() {
                noelements += 1;
                nodes = 6;
                if nodes > maxnodes {
                    maxnodes = nodes;
                }
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(noelements as isize) = 706;
                    nastran_elem(data, noelements, nodes, line.as_mut_ptr(), &mut cp, &mut k, 6);
                }
            } else if !strstr(line.as_ptr(), b"CHEXA\0".as_ptr() as *const c_char).is_null() {
                noelements += 1;
                nodes = 8;
                if nodes > maxnodes {
                    maxnodes = nodes;
                }
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(noelements as isize) = 808;
                    nastran_elem(data, noelements, 6, line.as_mut_ptr(), &mut cp, &mut k, 5);
                }
                Getrow(line.as_mut_ptr(), input, TRUE);
                if allocated != 0 {
                    cp = line.as_mut_ptr().wrapping_offset(1);
                    j = 6;
                    while j < 8 {
                        // NOTE: C reuses `k` here (last value read above).
                        *(*(*data).topology.wrapping_offset(noelements as isize))
                            .wrapping_offset(j as isize) = k;
                        j += 1;
                    }
                }
            } else if !strstr(line.as_ptr(), b"ENDDAT\0".as_ptr() as *const c_char).is_null() {
                break;
            } else if allocated == 0 {
                InpIgnored(line.as_mut_ptr());
            }
        }
        // NOTE: `end:` landing pad (C `goto` target).
        if allocated == TRUE {
            if info != 0 {
                printf(
                    b"The mesh was loaded from file %s.\n\0".as_ptr() as *const c_char,
                    filename.as_ptr(),
                );
            }
            fclose(input);
            return 0;
        }
        rewind(input);
        (*data).noknots = noknots;
        (*data).noelements = noelements;
        (*data).maxnodes = maxnodes;
        (*data).dim = 3;
        printf(b"maxknot = %d  minknot = %d\n\0".as_ptr() as *const c_char, maxknot, minknot);
        if info != 0 {
            printf(
                b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr() as *const c_char,
                noknots,
                noelements,
                maxnodes,
            );
        }
        AllocateKnots(data);
        allocated = TRUE;
        // NOTE: C `goto omstart` (second pass).
    }
}

// SAFETY: one nastran element row (split for sizing).
unsafe fn nastran_elem(
    data: *mut FemType,
    noelements: c_int,
    nodes: c_int,
    line: *mut c_char,
    cp: &mut *mut c_char,
    k: &mut c_int,
    off: c_int,
) {
    let mut j: c_int;
    *cp = line.wrapping_offset(off as isize);
    *k = next_int(&mut (*cp));
    *(*data).material.wrapping_offset(noelements as isize) =
        next_int(&mut (*cp)) + 1;
    j = 0;
    while j < nodes {
        *(*(*data).topology.wrapping_offset(noelements as isize)).wrapping_offset(j as isize) =
            next_int(&mut (*cp));
        j += 1;
    }
}
// SAFETY: fidap node reorder over a valid element row.
unsafe fn ReorderFidapNodes(data: *mut FemType, element: c_int, nodes: c_int, typeflag: c_int) {
    let mut i: c_int;
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: assigned before every read (C uninit decl shape).
    let topology: *mut c_int;
    let mut dim: c_int;
    let order808: [c_int; 8] = [1, 2, 4, 3, 5, 6, 8, 7];
    let order408: [c_int; 8] = [1, 3, 5, 7, 2, 4, 6, 8];
    let order306: [c_int; 6] = [1, 3, 5, 2, 4, 6];
    let order203: [c_int; 3] = [1, 3, 2];
    let order605: [c_int; 5] = [1, 2, 4, 3, 5];
    dim = (*data).dim;
    if typeflag > 10 {
        dim -= 1;
    }
    *(*data).elementtypes.wrapping_offset(element as isize) = 101 * nodes;
    topology = *(*data).topology.wrapping_offset(element as isize);
    if dim == 1 {
        if nodes == 3 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 203;
            i = 0;
            while i < nodes {
                oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i < nodes {
                *topology.wrapping_offset(i as isize) =
                    oldtopology[(order203[i as usize] - 1) as usize];
                i += 1;
            }
        }
    } else if dim == 2 {
        if nodes == 6 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 306;
            i = 0;
            while i < nodes {
                oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i < nodes {
                *topology.wrapping_offset(i as isize) =
                    oldtopology[(order306[i as usize] - 1) as usize];
                i += 1;
            }
        } else if nodes == 8 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 408;
            i = 0;
            while i < nodes {
                oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i < nodes {
                *topology.wrapping_offset(i as isize) =
                    oldtopology[(order408[i as usize] - 1) as usize];
                i += 1;
            }
        }
    } else if dim == 3 {
        if nodes == 4 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 504;
        } else if nodes == 5 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 605;
            i = 0;
            while i < nodes {
                oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i < nodes {
                *topology.wrapping_offset(i as isize) =
                    oldtopology[(order605[i as usize] - 1) as usize];
                i += 1;
            }
        } else if nodes == 6 {
            *(*data).elementtypes.wrapping_offset(element as isize) = 706;
        } else if nodes == 8 {
            i = 0;
            while i < nodes {
                oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i < nodes {
                *topology.wrapping_offset(i as isize) =
                    oldtopology[(order808[i as usize] - 1) as usize];
                i += 1;
            }
        } else {
            printf(b"Unknown Fidap elementtype with %d nodes.\n\0".as_ptr() as *const c_char, nodes);
        }
    } else {
        printf(
            b"ReorderFidapNodes: unknown dimension (%d)\n\0".as_ptr() as *const c_char,
            (*data).dim,
        );
    }
}
// SAFETY: fidap neutral reader over caller-owned data; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadFidapInput(
    data: *mut FemType,
    boundaries: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: reset before every use (pass loop).
    let mut noknots: c_int;
    // NOTE: reset before every use (pass loop).
    let mut noelements: c_int;
    // NOTE: reset before every use (pass loop).
    let mut dim: c_int;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut novel: c_int = 0;
    // NOTE: reset before every use (pass loop).
    let mut maxnodes: c_int;
    // NOTE: reset before every use (pass loop).
    let mut mode: c_int;
    // NOTE: 0-init for the &mut handoff (renumber overwrites first).
    let mut maxknot: c_int = 0;
    // NOTE: assigned in the read loop before every read.
    let mut totelems: c_int;
    // NOTE: assigned in the read loop before every read.
    let mut entity: c_int;
    // NOTE: assigned in the read loop before every read.
    let mut maxentity: c_int;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: C declares `k` at top too, but only the handoff decl below is live; dropped.
    // NOTE: null-init (Ivector before guarded reads).
    let mut ind: *mut c_int = core::ptr::null_mut();
    // NOTE: C declares `geoflag`/`typeflag` here but only fidap_group uses them; dropped.
    let mut topology: *mut *mut c_int = core::ptr::null_mut();
    let mut input: *mut FILE;
    // NOTE: CreateVariable result overwrites below before any use.
    let mut vel: *mut Real;
    // NOTE: CreateVariable result overwrites below before any use.
    let mut temp: *mut Real;
    // NOTE: `nogroups = 0` follows before every use.
    let mut nogroups: c_int;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut knotno: c_int = 0;
    // NOTE: GETLINE overwrites before every read.
    let mut isio: *mut c_char;
    // NOTE: 0-init for the &mut handoff (set in fidap_group before reads).
    let mut k: c_int = 0;
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"fidap\0".as_ptr() as *const c_char,
    );
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"FDNEUT\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadFidapInput: opening of the Fidap-file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    InitializeKnots(data);
    entity = 0;
    mode = 0;
    noknots = 0;
    noelements = 0;
    dim = 0;
    maxnodes = 4;
    totelems = 0;
    maxentity = 0;
    nogroups = 0;
    let _ = nogroups;
    loop {
        // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr write-only here.
        isio = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        ioptr = isio;
        if isio.is_null() {
            break;
        }
        if !strstr(line.as_ptr(), b"END\0".as_ptr() as *const c_char).is_null() {
            break;
        }
        // NOTE: control information.
        if !strstr(line.as_ptr(), b"FIDAP NEUTRAL FILE\0".as_ptr() as *const c_char).is_null() {
            mode = 1;
        } else if !strstr(line.as_ptr(), b"NO. OF NODES\0".as_ptr() as *const c_char).is_null() {
            mode = 2;
        } else if !strstr(line.as_ptr(), b"TEMPERATURE/SPECIES FLAGS\0".as_ptr() as *const c_char).is_null() {
            mode = 3;
        } else if !strstr(line.as_ptr(), b"PRESSURE FLAGS\0".as_ptr() as *const c_char).is_null() {
            mode = 4;
        } else if !strstr(line.as_ptr(), b"NODAL COORDINATES\0".as_ptr() as *const c_char).is_null() {
            mode = 5;
        } else if !strstr(line.as_ptr(), b"BOUNDARY CONDITIONS\0".as_ptr() as *const c_char).is_null() {
            mode = 6;
        } else if !strstr(line.as_ptr(), b"ELEMENT GROUPS\0".as_ptr() as *const c_char).is_null() {
            mode = 7;
        } else if !strstr(line.as_ptr(), b"GROUP:\0".as_ptr() as *const c_char).is_null() {
            mode = 8;
        } else if !strstr(line.as_ptr(), b"VELOCITY\0".as_ptr() as *const c_char).is_null() {
            mode = 10;
        } else if !strstr(line.as_ptr(), b"TEMPERATURE\0".as_ptr() as *const c_char).is_null() {
            mode = 11;
        }
        // NOTE: `else if(0)` diagnostic omitted (runtime-dead as in C).
        // NOTE: C switch over mode.
        if mode == 1 {
            if info != 0 {
                printf(b"Loading FIDAP input file %s\n\0".as_ptr() as *const c_char, filename.as_ptr());
            }
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            if info != 0 {
                printf(b"Name of the case: %s\0".as_ptr() as *const c_char, line.as_ptr());
            }
            mode = 0;
        } else if mode == 2 {
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            sscanf(
                line.as_ptr(),
                b"%d%d%d%d%d\0".as_ptr() as *const c_char,
                &mut noknots,
                &mut noelements,
                &mut nogroups,
                &mut dim,
                &mut novel,
            );
            let _ = nogroups;
            (*data).noknots = noknots;
            (*data).noelements = noelements;
            (*data).maxnodes = maxnodes;
            (*data).dim = dim;
            mode = 0;
        } else if mode == 5 {
            if info != 0 {
                printf(
                    b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                    maxnodes,
                );
            }
            AllocateKnots(data);
            if info != 0 {
                printf(b"Reading the nodes\n\0".as_ptr() as *const c_char);
            }
            i = 1;
            while i <= noknots {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                if dim == 2 {
                    sscanf(
                        line.as_ptr(),
                        b"%d%le%le\0".as_ptr() as *const c_char,
                        &mut knotno,
                        &mut (*(*data).x.wrapping_offset(i as isize)),
                        &mut (*(*data).y.wrapping_offset(i as isize)),
                    );
                } else if dim == 3 {
                    sscanf(
                        line.as_ptr(),
                        b"%d%le%le%le\0".as_ptr() as *const c_char,
                        &mut knotno,
                        &mut (*(*data).x.wrapping_offset(i as isize)),
                        &mut (*(*data).y.wrapping_offset(i as isize)),
                        &mut (*(*data).z.wrapping_offset(i as isize)),
                    );
                }
                i += 1;
            }
        } else if mode == 8 {
            fidap_group(
                data, &mut mode, &mut entity, &mut maxentity, &mut totelems,
                &mut maxnodes, &mut topology, &mut entityname, &mut k,
                line.as_mut_ptr(), input, info, noelements,
            );
            mode = 0;
        } else if mode == 10 {
            dim = 3;
            if info != 0 {
                printf(b"Reading the velocity field\n\0".as_ptr() as *const c_char);
            }
            CreateVariable(data, 2, dim, 0.0, b"Velocity\0".as_ptr() as *const c_char, FALSE);
            vel = (*data).dofs[2];
            j = 1;
            while j <= noknots {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                if dim == 2 {
                    sscanf(
                        line.as_ptr(),
                        b"%le%le\0".as_ptr() as *const c_char,
                        &mut (*vel.wrapping_offset((2 * j - 1) as isize)),
                        &mut (*vel.wrapping_offset((2 * j) as isize)),
                    );
                }
                if dim == 3 {
                    sscanf(
                        line.as_ptr(),
                        b"%le%le%le\0".as_ptr() as *const c_char,
                        &mut (*vel.wrapping_offset((3 * j - 2) as isize)),
                        &mut (*vel.wrapping_offset((3 * j - 1) as isize)),
                        &mut (*vel.wrapping_offset((3 * j) as isize)),
                    );
                }
                j += 1;
            }
            mode = 0;
        } else if mode == 11 {
            if info != 0 {
                printf(b"Reading the temperature field\n\0".as_ptr() as *const c_char);
            }
            CreateVariable(data, 1, 1, 0.0, b"Temperature\0".as_ptr() as *const c_char, FALSE);
            temp = (*data).dofs[1];
            j = 1;
            while j <= noknots {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                sscanf(
                    line.as_ptr(),
                    b"%le\0".as_ptr() as *const c_char,
                    &mut (*temp.wrapping_offset(j as isize)),
                );
                j += 1;
            }
            mode = 0;
        }
        // NOTE: default arm is empty in C.
    }
    // NOTE: `end:` landing pad (C `goto` target).
    fidap_renumber(data, &mut maxknot, &mut ind, noelements, noknots, info);
    if maxentity > 0 {
        (*data).bodynamesexist = TRUE;
    }
    fclose(input);
    if info != 0 {
        printf(b"Finished reading the Fidap neutral file\n\0".as_ptr() as *const c_char);
    }
    ElementsToBoundaryConditions(data, boundaries, FALSE, TRUE);
    // NOTE: commented-out RenumberBoundaryTypes omitted (dead in C).
    0
}
// SAFETY: fidap GROUP arm (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn fidap_group(
    data: *mut FemType,
    mode: &mut c_int,
    entity: &mut c_int,
    maxentity: &mut c_int,
    totelems: &mut c_int,
    maxnodes: &mut c_int,
    topology: &mut *mut *mut c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    k: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
    noelements: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut val: c_int;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut group: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut elems: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut nodes: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut geoflag: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut typeflag: c_int = 0;
    // NOTE: next_* overwrites via &mut before reads.
    let mut cp: *mut c_char;
    let _ = mode;
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%d\0".as_ptr() as *const c_char,
        &mut group,
    );
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%d\0".as_ptr() as *const c_char,
        &mut elems,
    );
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%d\0".as_ptr() as *const c_char,
        &mut nodes,
    );
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%d\0".as_ptr() as *const c_char,
        &mut geoflag,
    );
    let _ = geoflag;
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%d\0".as_ptr() as *const c_char,
        &mut typeflag,
    );
    ioptr = fgets(line, MAXLINESIZE as c_int, input);
    i = 0;
    loop {
        val = *line.wrapping_offset(i as isize) as c_int;
        i += 1;
        if val == ':' as c_int {
            break;
        }
    }
    i += 1;
    sscanf(
        line.wrapping_offset(i as isize),
        b"%s\0".as_ptr() as *const c_char,
        entityname.as_mut_ptr(),
    );
    if nodes > *maxnodes {
        if info != 0 {
            printf(
                b"Allocating a %d-node topology matrix\n\0".as_ptr() as *const c_char,
                nodes,
            );
        }
        // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
        *topology = imatrix(1, noelements, 0, nodes - 1);
        if *totelems > 0 {
            j = 1;
            while j <= *totelems {
                i = 0;
                while i < *(*data).elementtypes.wrapping_offset(j as isize) % 100 {
                    *(*topology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                        *(*(*data).topology.wrapping_offset(j as isize))
                            .wrapping_offset(i as isize);
                    i += 1;
                }
                j += 1;
            }
        }
        // NOTE: C `free_Imatrix` is a macro for `free_imatrix`.
        free_imatrix((*data).topology, 1, (*data).noelements, 0, (*data).maxnodes - 1);
        (*data).maxnodes = nodes;
        *maxnodes = nodes;
        (*data).topology = *topology;
    }
    if info != 0 {
        printf(
            b"reading %d element topologies with %d nodes for %s\n\0".as_ptr() as *const c_char,
            elems,
            nodes,
            entityname.as_ptr(),
        );
    }
    *entity = 1;
    while *entity <= *maxentity {
        if (*data).bodyname[*entity as usize].is_null() {
            break;
        }
        *k = strcmp(entityname.as_ptr(), (*data).bodyname[*entity as usize]);
        if *k == 0 {
            break;
        }
        *entity += 1;
    }
    if *entity > *maxentity {
        *maxentity += 1;
        if (*data).bodyname[*entity as usize].is_null() {
            (*data).bodyname[*entity as usize] = cvector(0, MAXNAMESIZE as c_int);
        }
        strcpy((*data).bodyname[*entity as usize], entityname.as_ptr());
        if info != 0 {
            printf(b"Found new entity: %s\n\0".as_ptr() as *const c_char, entityname.as_ptr());
        }
    }
    i = *totelems + 1;
    while i <= *totelems + elems {
        ioptr = fgets(line, MAXLINESIZE as c_int, input);
        cp = line;
        j = next_int(&mut cp);
        let _ = j;
        j = 0;
        while j < nodes {
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) =
                next_int(&mut cp);
            j += 1;
        }
        ReorderFidapNodes(data, i, nodes, typeflag);
        if *(*data).elementtypes.wrapping_offset(i as isize) == 0 {
            printf(b"Elementtype is zero!\n\0".as_ptr() as *const c_char);
        }
        if *entity != 0 {
            *(*data).material.wrapping_offset(i as isize) = *entity;
        } else {
            *(*data).material.wrapping_offset(i as isize) = group;
        }
        i += 1;
    }
    *totelems += elems;
}

// SAFETY: fidap node renumber tail (split for sizing).
unsafe fn fidap_renumber(
    data: *mut FemType,
    maxknot: &mut c_int,
    ind: &mut *mut c_int,
    noelements: c_int,
    noknots: c_int,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: renumber the nodes.
    *maxknot = 0;
    i = 1;
    while i <= noelements {
        j = 0;
        while j < *(*data).elementtypes.wrapping_offset(i as isize) % 100 {
            if *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                > *maxknot
            {
                *maxknot = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
            }
            j += 1;
        }
        i += 1;
    }
    if *maxknot > noknots {
        if info != 0 {
            printf(
                b"Renumbering the nodes from 1 to %d\n\0".as_ptr() as *const c_char,
                noknots,
            );
        }
        *ind = ivector(1, *maxknot);
        i = 1;
        while i <= *maxknot {
            *(*ind).wrapping_offset(i as isize) = 0;
            i += 1;
        }
        i = 1;
        while i <= noelements {
            j = 0;
            while j < *(*data).elementtypes.wrapping_offset(i as isize) % 100 {
                *(*ind).wrapping_offset(
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                        as isize,
                ) = *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize);
                j += 1;
            }
            i += 1;
        }
        i = 0;
        j = 1;
        while j <= noknots {
            i += 1;
            while *(*ind).wrapping_offset(i as isize) == 0 {
                i += 1;
            }
            *(*ind).wrapping_offset(i as isize) = j;
            j += 1;
        }
        i = 1;
        while i <= noelements {
            j = 0;
            while j < *(*data).elementtypes.wrapping_offset(i as isize) % 100 {
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) =
                    *(*ind).wrapping_offset(
                        *(*(*data).topology.wrapping_offset(i as isize))
                            .wrapping_offset(j as isize) as isize,
                    );
                j += 1;
            }
            i += 1;
        }
    }
}
// SAFETY: ansys node reorder over valid old/new topology rows.
unsafe fn ReorderAnsysNodes(
    data: *mut FemType,
    oldtopology: *mut c_int,
    element: c_int,
    dim: c_int,
    nodes: c_int,
) {
    let mut i: c_int;
    // NOTE: assigned once before every read.
    let topology: *mut c_int;
    // NOTE: `elementtype = 0` follows before every read.
    let mut elementtype: c_int;
    let order820: [c_int; 20] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 17, 18, 19, 20, 13, 14, 15, 16,
    ];
    let order504: [c_int; 4] = [1, 2, 3, 5];
    let order306: [c_int; 6] = [1, 2, 3, 5, 6, 8];
    let order510: [c_int; 10] = [1, 2, 3, 5, 9, 10, 12, 17, 18, 19];
    let order613: [c_int; 13] = [1, 2, 3, 4, 5, 9, 10, 11, 12, 17, 18, 19, 20];
    let order706: [c_int; 6] = [1, 2, 3, 5, 6, 7];
    let order715: [c_int; 15] = [1, 2, 3, 5, 6, 7, 9, 10, 12, 17, 18, 19, 13, 14, 16];
    elementtype = 0;
    if dim == 3 {
        if nodes == 20 {
            if *oldtopology.wrapping_offset(2) == *oldtopology.wrapping_offset(3)
                && *oldtopology.wrapping_offset(4) == *oldtopology.wrapping_offset(5)
            {
                elementtype = 510;
            } else if *oldtopology.wrapping_offset(2) == *oldtopology.wrapping_offset(3)
                && *oldtopology.wrapping_offset(4) != *oldtopology.wrapping_offset(5)
            {
                elementtype = 715;
            } else if *oldtopology.wrapping_offset(4) == *oldtopology.wrapping_offset(5) {
                elementtype = 613;
            } else {
                elementtype = 820;
            }
        }
        if nodes == 8 {
            if *oldtopology.wrapping_offset(2) == *oldtopology.wrapping_offset(3)
                && *oldtopology.wrapping_offset(4) == *oldtopology.wrapping_offset(7)
                && *oldtopology.wrapping_offset(5) == *oldtopology.wrapping_offset(7)
                && *oldtopology.wrapping_offset(6) == *oldtopology.wrapping_offset(7)
            {
                elementtype = 504;
            } else if *oldtopology.wrapping_offset(2) == *oldtopology.wrapping_offset(3)
                && *oldtopology.wrapping_offset(6) == *oldtopology.wrapping_offset(7)
            {
                elementtype = 706;
            } else if *oldtopology.wrapping_offset(4) == *oldtopology.wrapping_offset(5) {
                elementtype = 605;
            } else {
                elementtype = 808;
            }
        }
        if nodes == 4 {
            elementtype = 504;
        }
        if nodes == 10 {
            elementtype = 510;
        }
    } else if dim == 2 {
        if nodes == 9 {
            elementtype = 408;
        }
        if nodes == 8 {
            if *oldtopology.wrapping_offset(3) == *oldtopology.wrapping_offset(6) {
                elementtype = 306;
            } else {
                elementtype = 408;
            }
        }
        if nodes == 4 {
            elementtype = 404;
        }
        if nodes == 10 {
            elementtype = 310;
        }
        if nodes == 6 {
            elementtype = 306;
        }
        if nodes == 3 {
            elementtype = 303;
        }
    } else if dim == 1 {
        if nodes == 4 {
            elementtype = 204;
        }
        if nodes == 3 {
            elementtype = 203;
        }
        if nodes == 2 {
            elementtype = 202;
        }
    }
    if elementtype == 0 {
        printf(
            b"Unknown elementtype in element %d (%d nodes, %d dim).\n\0".as_ptr() as *const c_char,
            element,
            nodes,
            dim,
        );
    }
    *(*data).elementtypes.wrapping_offset(element as isize) = elementtype;
    topology = *(*data).topology.wrapping_offset(element as isize);
    // NOTE: C switch over elementtype.
    if elementtype == 820 {
        i = 0;
        while i < elementtype % 100 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order820[i as usize] - 1) as isize);
            i += 1;
        }
    } else if elementtype == 504 {
        if nodes == 4 {
            i = 0;
            while i < elementtype % 100 {
                *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
                i += 1;
            }
        } else {
            i = 0;
            while i < elementtype % 100 {
                *topology.wrapping_offset(i as isize) =
                    *oldtopology.wrapping_offset((order504[i as usize] - 1) as isize);
                i += 1;
            }
        }
    } else if elementtype == 510 {
        i = 0;
        while i < elementtype % 100 {
            if *oldtopology.wrapping_offset(2) == *oldtopology.wrapping_offset(3) {
                *topology.wrapping_offset(i as isize) =
                    *oldtopology.wrapping_offset((order510[i as usize] - 1) as isize);
            } else {
                *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
            }
            i += 1;
        }
    } else if elementtype == 605 {
        i = 0;
        while i < elementtype % 100 {
            *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
            i += 1;
        }
    } else if elementtype == 613 {
        i = 0;
        while i < elementtype % 100 {
            if *oldtopology.wrapping_offset(4) == *oldtopology.wrapping_offset(5) {
                *topology.wrapping_offset(i as isize) =
                    *oldtopology.wrapping_offset((order613[i as usize] - 1) as isize);
            } else {
                *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
            }
            i += 1;
        }
    } else if elementtype == 306 {
        i = 0;
        while i < elementtype % 100 {
            if *oldtopology.wrapping_offset(3) == *oldtopology.wrapping_offset(6) {
                *topology.wrapping_offset(i as isize) =
                    *oldtopology.wrapping_offset((order306[i as usize] - 1) as isize);
            } else {
                *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
            }
            i += 1;
        }
    } else if elementtype == 706 {
        i = 0;
        while i < elementtype % 100 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order706[i as usize] - 1) as isize);
            i += 1;
        }
    } else if elementtype == 715 {
        i = 0;
        while i < elementtype % 100 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order715[i as usize] - 1) as isize);
            i += 1;
        }
    } else {
        i = 0;
        while i < elementtype % 100 {
            *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
            i += 1;
        }
    }
}
// SAFETY: ansys reader over caller-owned data/bound; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadAnsysInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut noknots: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut noelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut nosides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut sidetype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut currenttype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let indx: *mut c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let mut revindx: *mut c_int;
    let mut topology: [c_int; 100] = [0; 100];
    // NOTE: assigned in loop before every read.
    let mut ind: c_int;
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut l: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut imax: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut nodeindx: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut boundindx: *mut c_int = core::ptr::null_mut();
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut boundarynodes: c_int = 0;
    // NOTE: assigned once before every read.
    let noansystypes: c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let ansysdim: *mut c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let ansysnodes: *mut c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let ansystypes: *mut c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut boundarytypes: c_int = 0;
    // NOTE: assigned once before every read.
    let namesexist: c_int;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut maxside: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in ansys_boundary before reads).
    let mut sides: c_int = 0;
    // NOTE: assigned in loop before every read.
    let mut x: Real;
    // NOTE: assigned in loop before every read.
    let mut y: Real;
    // NOTE: 0-init (unobservable: read only under dim==3 which assigns).
    let mut z: Real = 0.0;
    let mut input: *mut FILE;
    // NOTE: `cp = line...` overwrites unconditionally below before any use.
    let mut cp: *mut c_char;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut text: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut text2: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    // NOTE: `ExportMesh.header`.
    sprintf(filename.as_mut_ptr(), b"%s.header\0".as_ptr() as *const c_char, prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadAnsysInput: The opening of the header-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Calculating Ansys elementtypes from %s\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    i = 0;
    loop {
        // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr write-only here.
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        if ioptr.is_null() {
            break;
        }
        i += 1;
    }
    noansystypes = i - 1;
    printf(
        b"There seems to be %d element types in file %s.\n\0".as_ptr() as *const c_char,
        noansystypes,
        filename.as_ptr(),
    );
    // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
    ansysdim = ivector(1, noansystypes);
    ansysnodes = ivector(1, noansystypes);
    ansystypes = ivector(1, noansystypes);
    rewind(input);
    i = 0;
    while i <= noansystypes {
        // NOTE: block-scoped C locals become a Rust block.
        {
            // NOTE: 0-init (sscanf overwrites before reads).
            let mut dummy1: Real = 0.0;
            // NOTE: 0-init (sscanf overwrites before reads).
            let mut dummy2: Real = 0.0;
            // NOTE: 0-init (sscanf overwrites before reads).
            let mut dummy3: Real = 0.0;
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            // NOTE: Ansys writes decimal points for integers too; read as reals.
            sscanf(
                line.as_ptr(),
                b"%le %le %le\0".as_ptr() as *const c_char,
                &mut dummy1,
                &mut dummy2,
                &mut dummy3,
            );
            if i == 0 {
                noelements = (dummy1 + 0.5) as c_int;
                noknots = (dummy2 + 0.5) as c_int;
                boundarytypes = (dummy3 + 0.5) as c_int;
            } else {
                *ansysdim.wrapping_offset(i as isize) = (dummy1 + 0.5) as c_int;
                *ansysnodes.wrapping_offset(i as isize) = (dummy2 + 0.5) as c_int;
                *ansystypes.wrapping_offset(i as isize) = (dummy3 + 0.5) as c_int;
            }
        }
        i += 1;
    }
    fclose(input);
    printf(
        b"Ansys file has %d elements, %d nodes and %d boundary types.\n\0".as_ptr()
            as *const c_char,
        noelements,
        noknots,
        boundarytypes,
    );
    // NOTE: `ExportMesh.names`.
    sprintf(filename.as_mut_ptr(), b"%s.names\0".as_ptr() as *const c_char, prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        namesexist = FALSE;
    } else {
        namesexist = TRUE;
    }
    if namesexist != 0 {
        printf(b"Using names of bodies and boundaries\n\0".as_ptr() as *const c_char);
    }
    // NOTE: `ExportMesh.node`.
    sprintf(filename.as_mut_ptr(), b"%s.node\0".as_ptr() as *const c_char, prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadAnsysInput: The opening of the nodes-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 2;
    }
    if info != 0 {
        printf(b"Calculating Ansys nodes from %s\n\0".as_ptr() as *const c_char, filename.as_ptr());
    }
    i = 0;
    loop {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        if ioptr.is_null() {
            break;
        }
        i += 1;
    }
    if info != 0 {
        printf(b"There seems to be %d nodes in file %s.\n\0".as_ptr() as *const c_char, i, filename.as_ptr());
    }
    if i != noknots {
        printf(b"Conflicting number of nodes %d vs %d!\n\0".as_ptr() as *const c_char, i, noknots);
    }
    // NOTE: make room and initialize the mesh.
    InitializeKnots(data);
    // NOTE: even 2D elements may form a 3D object!
    (*data).dim = 3;
    (*data).maxnodes = 1;
    i = 1;
    while i <= noansystypes {
        if *ansysdim.wrapping_offset(i as isize) > (*data).dim {
            (*data).dim = *ansysdim.wrapping_offset(i as isize);
        }
        if *ansysnodes.wrapping_offset(i as isize) > (*data).maxnodes {
            (*data).maxnodes = *ansysnodes.wrapping_offset(i as isize);
        }
        i += 1;
    }
    if (*data).maxnodes < 8 {
        (*data).maxnodes = 8;
    }
    (*data).noknots = noknots;
    (*data).noelements = noelements;
    if info != 0 {
        printf(
            b"Allocating for %d nodes and %d elements with max. %d nodes in %d-dim.\n\0".as_ptr()
                as *const c_char,
            noknots,
            noelements,
            (*data).maxnodes,
            (*data).dim,
        );
    }
    AllocateKnots(data);
    indx = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *indx.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    if info != 0 {
        printf(b"Loading %d Ansys nodes from %s\n\0".as_ptr() as *const c_char, noknots, filename.as_ptr());
    }
    rewind(input);
    i = 1;
    while i <= noknots {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        cp = line.as_mut_ptr();
        *indx.wrapping_offset(i as isize) = next_int(&mut cp);
        if *cp.wrapping_offset(0) == '.' as c_char {
            cp = cp.wrapping_offset(1);
        }
        x = next_real(&mut cp);
        if cp.is_null() {
            x = 0.0;
            y = 0.0;
            z = 0.0;
        } else {
            y = next_real(&mut cp);
            if cp.is_null() {
                y = 0.0;
                z = 0.0;
            } else if (*data).dim == 3 {
                z = next_real(&mut cp);
                if cp.is_null() {
                    z = 0.0;
                }
            }
        }
        *(*data).x.wrapping_offset(i as isize) = x;
        *(*data).y.wrapping_offset(i as isize) = y;
        if (*data).dim == 3 {
            *(*data).z.wrapping_offset(i as isize) = z;
        }
        i += 1;
    }
    fclose(input);
    // NOTE: reorder the indexes.
    maxindx = *indx.wrapping_offset(1);
    i = 1;
    while i <= noknots {
        if *indx.wrapping_offset(i as isize) > maxindx {
            maxindx = *indx.wrapping_offset(i as isize);
        }
        i += 1;
    }
    revindx = ivector(0, maxindx);
    if maxindx > noknots {
        printf(
            b"There are %d nodes with indexes up to %d.\n\0".as_ptr() as *const c_char,
            noknots,
            maxindx,
        );
        i = 1;
        while i <= maxindx {
            *revindx.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        i = 1;
        while i <= noknots {
            *revindx.wrapping_offset(*indx.wrapping_offset(i as isize) as isize) = i;
            i += 1;
        }
    } else {
        i = 1;
        while i <= noknots {
            *revindx.wrapping_offset(i as isize) = i;
            i += 1;
        }
    }
    // NOTE: `ExportMesh.elem`.
    sprintf(filename.as_mut_ptr(), b"%s.elem\0".as_ptr() as *const c_char, prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadAnsysInput: The opening of the element-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename.as_ptr(),
        );
        return 4;
    }
    if info != 0 {
        printf(
            b"Loading %d Ansys elements from %s\n\0".as_ptr() as *const c_char,
            noelements,
            filename.as_ptr(),
        );
    }
    j = 1;
    while j <= noelements {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        cp = line.as_mut_ptr();
        i = 0;
        while i < 8 {
            if noknots < 1000000 {
                ind = next_int(&mut cp);
            } else if noknots < 100000000 {
                ind = next_int_n(&mut cp, 8);
            } else {
                ind = next_int_n(&mut cp, 10);
            }
            if *cp.wrapping_offset(0) == '.' as c_char {
                cp = cp.wrapping_offset(1);
            }
            topology[i as usize] = *revindx.wrapping_offset(ind as isize);
            i += 1;
        }
        *(*data).material.wrapping_offset(j as isize) = next_int(&mut cp);
        currenttype = next_int(&mut cp);
        k = 1;
        while k <= noansystypes {
            if *ansystypes.wrapping_offset(k as isize) == currenttype {
                break;
            }
            k += 1;
        }
        if *ansystypes.wrapping_offset(k as isize) != currenttype {
            k = 1;
        }
        if *ansysnodes.wrapping_offset(k as isize) > 8 {
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            cp = line.as_mut_ptr();
            if *ansysnodes.wrapping_offset(k as isize) == 10
                && topology[2] != topology[3]
            {
                imax = 10;
            } else {
                imax = 20;
            }
            i = 8;
            while i < imax {
                if noknots < 1000000 {
                    ind = next_int(&mut cp);
                } else if noknots < 100000000 {
                    ind = next_int_n(&mut cp, 8);
                } else {
                    ind = next_int_n(&mut cp, 10);
                }
                if *cp.wrapping_offset(0) == '.' as c_char {
                    cp = cp.wrapping_offset(1);
                }
                topology[i as usize] = *revindx.wrapping_offset(ind as isize);
                i += 1;
            }
        }
        ReorderAnsysNodes(
            data,
            topology.as_mut_ptr(),
            j,
            *ansysdim.wrapping_offset(k as isize),
            *ansysnodes.wrapping_offset(k as isize),
        );
        j += 1;
    }
    fclose(input);
    // NOTE: C `return(5)` on a missing boundary file propagates out.
    let rc = ansys_boundary(
        data, bound, &mut nosides, &mut sidetype, &mut maxside, &mut sides,
        &mut nodeindx, &mut boundindx, &mut boundarynodes, &mut revindx,
        &mut text, &mut text2, &mut i, &mut j, &mut k, &mut l,
        filename.as_mut_ptr(), &mut input, prefix, info, namesexist,
    );
    if rc != 0 {
        return rc;
    }
    // NOTE: C `free_Ivector` is a macro for `free_ivector` (egutils.h:21).
    free_ivector(boundindx, 1, boundarynodes);
    free_ivector(nodeindx, 1, boundarynodes);
    if info != 0 {
        printf(b"Ansys mesh loaded successfully\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: ansys boundary + names tail (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn ansys_boundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nosides: &mut c_int,
    sidetype: &mut c_int,
    maxside: &mut c_int,
    sides: &mut c_int,
    nodeindx: &mut *mut c_int,
    boundindx: &mut *mut c_int,
    boundarynodes: &mut c_int,
    revindx: &mut *mut c_int,
    text: &mut [c_char; MAXNAMESIZE],
    text2: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    filename: *mut c_char,
    input: &mut *mut FILE,
    prefix: *mut c_char,
    info: c_int,
    namesexist: c_int,
) -> c_int {
    let _ = l;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    sprintf(filename, b"%s.boundary\0".as_ptr() as *const c_char, prefix);
    printf(b"Calculating nodes in file %s\n\0".as_ptr() as *const c_char, filename);
    *input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if (*input).is_null() {
        printf(
            b"LoadAnsysInput: The opening of the boundary-file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        // NOTE: C `return(5)` exits the whole loader; propagated to caller.
        return 5;
    }
    *j = 0;
    *i = 0;
    loop {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, *input);
        if ioptr.is_null() {
            break;
        }
        if strstr(line.as_ptr(), b"Boundary\0".as_ptr() as *const c_char).is_null() {
            *j += 1;
        }
        *i += 1;
    }
    *boundarynodes = *j;
    *nosides = 6 * *boundarynodes;
    if info != 0 {
        printf(
            b"There are %d boundary nodes, allocating %d elements\n\0".as_ptr() as *const c_char,
            *boundarynodes,
            *nosides,
        );
    }
    AllocateBoundary(bound, *nosides);
    *nodeindx = ivector(1, *boundarynodes);
    *boundindx = ivector(1, *boundarynodes);
    if info != 0 {
        printf(b"Loading Ansys boundary from %s\n\0".as_ptr() as *const c_char, filename);
    }
    *i = 1;
    while *i <= *boundarynodes {
        *(*nodeindx).wrapping_offset(*i as isize) = 0;
        *(*boundindx).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    rewind(*input);
    *maxside = 0;
    *j = 0;
    *i = 0;
    loop {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, *input);
        if ioptr.is_null() {
            break;
        }
        if !strstr(line.as_ptr(), b"Boundary\0".as_ptr() as *const c_char).is_null() {
            sscanf(line.as_ptr(), b"%d\0".as_ptr() as *const c_char, &mut (*sidetype));
            // NOTE: C `MAX` macro (egdef.h:97); arms pure.
            if *maxside < *sidetype {
                *maxside = *sidetype;
            }
        } else {
            *j += 1;
            sscanf(line.as_ptr(), b"%d\0".as_ptr() as *const c_char, &mut (*k));
            *(*nodeindx).wrapping_offset(*j as isize) =
                *(*revindx).wrapping_offset(*k as isize);
            if *(*nodeindx).wrapping_offset(*j as isize) == 0 {
                printf(
                    b"The boundary %dth node %d is not in index list\n\0".as_ptr()
                        as *const c_char,
                    *j,
                    *k,
                );
            }
            *(*boundindx).wrapping_offset(*j as isize) = *sidetype;
        }
        *i += 1;
    }
    printf(
        b"Found %d boundary nodes with %d as maximum side.\n\0".as_ptr() as *const c_char,
        *j,
        *maxside,
    );
    fclose(*input);
    FindPointParents(data, bound, *boundarynodes, *nodeindx, *boundindx, info);
    ansys_names(
        data, bound, maxside, text, text2, i, j, k, l, sides, filename, input, prefix, info,
        namesexist,
    );
    0
}
// SAFETY: ansys .names tail (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn ansys_names(
    data: *mut FemType,
    bound: *mut BoundaryType,
    maxside: &mut c_int,
    text: &mut [c_char; MAXNAMESIZE],
    text2: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    sides: &mut c_int,
    filename: *mut c_char,
    input: &mut *mut FILE,
    prefix: *mut c_char,
    info: c_int,
    namesexist: c_int,
) {
    // NOTE: block-scoped C locals become plain bindings (no outer shadow:
    // LoadAnsysInput declares no bcind).
    let mut bcind: c_int = 0;
    let mut bctypes: *mut c_int = core::ptr::null_mut();
    let mut bctypeused: *mut c_int = core::ptr::null_mut();
    let mut bcused: *mut c_int = core::ptr::null_mut();
    let mut newsides: c_int = 0;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    if namesexist == 0 {
        return;
    }
    (*data).bodynamesexist = TRUE;
    if (*bound).nosides != 0 {
        newsides = 0;
        bctypes = ivector(1, *maxside);
        bctypeused = ivector(1, *maxside);
        bcused = ivector(1, (*bound).nosides);
        *i = 1;
        while *i <= (*bound).nosides {
            *bcused.wrapping_offset(*i as isize) = FALSE;
            *i += 1;
        }
        (*data).boundarynamesexist = TRUE;
        *i = 1;
        while *i <= *maxside {
            *bctypeused.wrapping_offset(*i as isize) = FALSE;
            *i += 1;
        }
    }
    sprintf(filename, b"%s.names\0".as_ptr() as *const c_char, prefix);
    *input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    loop {
        if Getrow(line.as_mut_ptr(), *input, TRUE) != 0 {
            break;
        }
        sscanf(
            line.as_ptr(),
            b"%d%s%s%d\0".as_ptr() as *const c_char,
            &mut bcind,
            text.as_mut_ptr(),
            text2.as_mut_ptr(),
            &mut (*sides),
        );
        if !strstr(text2.as_ptr(), b"BODY\0".as_ptr() as *const c_char).is_null() {
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, *input);
            sscanf(
                line.as_ptr(),
                b"%d%d\0".as_ptr() as *const c_char,
                &mut (*j),
                &mut bcind,
            );
            if (*data).bodyname[bcind as usize].is_null() {
                (*data).bodyname[bcind as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy((*data).bodyname[bcind as usize], text.as_ptr());
        } else if !strstr(text2.as_ptr(), b"BOUNDARY\0".as_ptr() as *const c_char).is_null() {
            // NOTE: read the boundary groups belonging to a name.
            *i = 1;
            while *i <= *maxside {
                *bctypes.wrapping_offset(*i as isize) = 0;
                *i += 1;
            }
            *i = 1;
            while *i <= *sides {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, *input);
                sscanf(
                    line.as_ptr(),
                    b"%d%d\0".as_ptr() as *const c_char,
                    &mut (*j),
                    &mut bcind,
                );
                *bctypes.wrapping_offset(bcind as isize) = TRUE;
                *i += 1;
            }
            // NOTE: find 1st unused boundarytype.
            *i = 1;
            while *i <= *maxside {
                if *bctypes.wrapping_offset(*i as isize) != 0
                    && *bctypeused.wrapping_offset(*i as isize) == 0
                {
                    break;
                }
                *i += 1;
            }
            bcind = *i;
            *bctypeused.wrapping_offset(bcind as isize) = TRUE;
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            if bcind > MAXBCS as c_int {
                bigerror(b"bcind larger than MAXBCS!\0".as_ptr() as *const c_char);
            }
            if (*data).boundaryname[bcind as usize].is_null() {
                (*data).boundaryname[bcind as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy((*data).boundaryname[bcind as usize], text.as_ptr());
            // NOTE: check which of the BCs have already been named.
            *k = 0;
            *l = 0;
            *i = 1;
            while *i <= (*bound).nosides {
                *j = *(*bound).types.wrapping_offset(*i as isize);
                // NOTE: the bc is not given any name, hence it can't be a duplicate.
                if *bctypes.wrapping_offset(*j as isize) == 0 {
                    *i += 1;
                    continue;
                }
                if *bcused.wrapping_offset(*i as isize) == 0 {
                    *k += 1;
                    *bcused.wrapping_offset(*i as isize) = bcind;
                } else {
                    *l += 1;
                    if newsides == 0 {
                        AllocateBoundary(&mut (*bound.wrapping_offset(1)), (*bound).nosides);
                    }
                    newsides += 1;
                    *(*bound.wrapping_offset(1)).types.wrapping_offset(newsides as isize) = bcind;
                    *(*bound.wrapping_offset(1)).parent.wrapping_offset(newsides as isize) =
                        *(*bound).parent.wrapping_offset(*i as isize);
                    *(*bound.wrapping_offset(1)).side.wrapping_offset(newsides as isize) =
                        *(*bound).side.wrapping_offset(*i as isize);
                    *(*bound.wrapping_offset(1)).parent2.wrapping_offset(newsides as isize) =
                        *(*bound).parent2.wrapping_offset(*i as isize);
                    *(*bound.wrapping_offset(1)).side2.wrapping_offset(newsides as isize) =
                        *(*bound).side2.wrapping_offset(*i as isize);
                    *(*bound.wrapping_offset(1)).normal.wrapping_offset(newsides as isize) =
                        *(*bound).normal.wrapping_offset(*i as isize);
                }
                *i += 1;
            }
            if info != 0 && !(*data).boundaryname[bcind as usize].is_null() {
                printf(
                    b"There are %d boundary elements with name %s.\n\0".as_ptr()
                        as *const c_char,
                    *k + *l,
                    (*data).boundaryname[bcind as usize],
                );
            }
        }
    }
    fclose(*input);
    // NOTE: put the indexes of all conditions with the same name to be same.
    if (*bound).nosides != 0 {
        *i = 1;
        while *i <= (*bound).nosides {
            if *bcused.wrapping_offset(*i as isize) != 0 {
                *(*bound).types.wrapping_offset(*i as isize) =
                    *bcused.wrapping_offset(*i as isize);
            }
            *i += 1;
        }
        if newsides != 0 {
            (*bound.wrapping_offset(1)).nosides = newsides;
            if info != 0 {
                printf(
                    b"Created %d additional boundary elements to achieve unique naming.\n\0".as_ptr()
                        as *const c_char,
                    newsides,
                );
            }
        }
        // NOTE: C `free_Ivector` is a macro for `free_ivector` (egutils.h:21).
        free_ivector(bctypes, 1, *maxside);
        free_ivector(bctypeused, 1, *maxside);
        free_ivector(bcused, 1, (*bound).nosides);
    }
}
// SAFETY: fieldview node reorder over valid old topology row.
unsafe fn ReorderFieldviewNodes(
    data: *mut FemType,
    oldtopology: *mut c_int,
    element: c_int,
    dim: c_int,
    nodes: c_int,
) {
    let mut i: c_int;
    // NOTE: assigned once before every read.
    let topology: *mut c_int;
    // NOTE: `elementtype = 0` follows before every read.
    let mut elementtype: c_int;
    let order808: [c_int; 8] = [1, 2, 4, 3, 5, 6, 8, 7];
    let order706: [c_int; 6] = [1, 4, 6, 2, 3, 5];
    let order404: [c_int; 4] = [1, 2, 3, 4];
    elementtype = 0;
    if dim == 3 {
        if nodes == 8 {
            elementtype = 808;
        }
        if nodes == 6 {
            elementtype = 706;
        }
        if nodes == 4 {
            elementtype = 504;
        }
    } else if dim == 2 {
        if nodes == 4 {
            elementtype = 404;
        }
        if nodes == 3 {
            elementtype = 303;
        }
    }
    if elementtype == 0 {
        printf(
            b"Unknown elementtype in element %d (%d nodes, %d dim).\n\0".as_ptr()
                as *const c_char,
            element,
            nodes,
            dim,
        );
        bigerror(b"Cannot continue\0".as_ptr() as *const c_char);
    }
    *(*data).elementtypes.wrapping_offset(element as isize) = elementtype;
    topology = *(*data).topology.wrapping_offset(element as isize);
    i = 0;
    while i < elementtype % 100 {
        if elementtype == 808 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order808[i as usize] - 1) as isize);
        } else if elementtype == 706 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order706[i as usize] - 1) as isize);
        } else if elementtype == 404 {
            *topology.wrapping_offset(i as isize) =
                *oldtopology.wrapping_offset((order404[i as usize] - 1) as isize);
        } else {
            *topology.wrapping_offset(i as isize) = *oldtopology.wrapping_offset(i as isize);
        }
        i += 1;
    }
}

// SAFETY: fieldview reader over caller-owned data; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadFieldviewInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: reset before every use (pass loop).
    let mut noknots: c_int;
    // NOTE: reset before every use (pass loop).
    let mut noelements: c_int;
    // NOTE: reset before every use (pass loop).
    let mut maxnodes: c_int;
    // NOTE: reset before every use (pass loop).
    let mut mode: c_int;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (next_* overwrites via &mut before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in fieldview_elems before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in fieldview_elems before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in fieldview_elems before reads).
    let mut k: c_int = 0;
    let mut input: *mut FILE;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut x: Real = 0.0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut y: Real = 0.0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut z: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: GETLINE overwrites before every read.
    let mut isio: *mut c_char;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut nobound: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut nobulk: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxsidenodes: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut boundtypes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Imatrix before guarded reads).
    let mut boundtopos: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut boundnodes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut origtopology: *mut c_int = core::ptr::null_mut();
    input = fopen(prefix, b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"dat\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadFieldviewInput: opening of the Fieldview-file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    InitializeKnots(data);
    (*data).dim = 3;
    (*data).created = TRUE;
    mode = 0;
    noknots = 0;
    noelements = 0;
    maxnodes = 8;
    maxsidenodes = 4;
    maxindx = 0;
    (*data).maxnodes = maxnodes;
    loop {
        if mode == 0 {
            // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr write-only here.
            isio = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            ioptr = isio;
            if isio.is_null() {
                break;
            }
            if !strstr(line.as_ptr(), b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            // NOTE: control information.
            if !strstr(line.as_ptr(), b"FIELDVIEW\0".as_ptr() as *const c_char).is_null() {
                mode = 1;
            } else if !strstr(line.as_ptr(), b"CONSTANTS\0".as_ptr() as *const c_char).is_null() {
                mode = 2;
            } else if !strstr(line.as_ptr(), b"GRIDS\0".as_ptr() as *const c_char).is_null() {
                mode = 3;
            } else if !strstr(line.as_ptr(), b"Boundary Table\0".as_ptr() as *const c_char).is_null() {
                mode = 4;
            } else if !strstr(line.as_ptr(), b"Variable Names\0".as_ptr() as *const c_char).is_null() {
                mode = 5;
            } else if !strstr(line.as_ptr(), b"Nodes\0".as_ptr() as *const c_char).is_null() {
                mode = 6;
            } else if !strstr(line.as_ptr(), b"Boundary Faces\0".as_ptr() as *const c_char).is_null() {
                mode = 7;
            } else if !strstr(line.as_ptr(), b"Elements\0".as_ptr() as *const c_char).is_null() {
                mode = 8;
            } else if !strstr(line.as_ptr(), b"Variables\0".as_ptr() as *const c_char).is_null() {
                mode = 9;
            }
            // NOTE: `else if(0)` diagnostic omitted (runtime-dead as in C).
        }
        // NOTE: C switch over mode.
        if mode == 1 {
            printf(b"This is indeed a Fieldview input file.\n\0".as_ptr() as *const c_char);
            mode = 0;
        } else if mode == 2 {
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            mode = 0;
        } else if mode == 3 {
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            mode = 0;
        } else if mode == 4 {
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            mode = 0;
        } else if mode == 5 {
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            mode = 0;
        } else if mode == 6 {
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            sscanf(line.as_ptr(), b"%d\0".as_ptr() as *const c_char, &mut noknots);
            (*data).noknots = noknots;
            if info != 0 {
                printf(b"Loading %d node coordinates\n\0".as_ptr() as *const c_char, noknots);
            }
            // NOTE: C `Rvector` is a macro for `dvector` (egutils.h:15).
            (*data).x = dvector(1, noknots);
            (*data).y = dvector(1, noknots);
            (*data).z = dvector(1, noknots);
            i = 1;
            while i <= noknots {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                sscanf(
                    line.as_ptr(),
                    b"%le%le%le\0".as_ptr() as *const c_char,
                    &mut x,
                    &mut y,
                    &mut z,
                );
                *(*data).x.wrapping_offset(i as isize) = x;
                *(*data).y.wrapping_offset(i as isize) = y;
                *(*data).z.wrapping_offset(i as isize) = z;
                i += 1;
            }
            mode = 0;
        } else if mode == 7 {
            ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
            sscanf(line.as_ptr(), b"%d\0".as_ptr() as *const c_char, &mut nobound);
            if info != 0 {
                printf(
                    b"Loading %d boundary element definitions\n\0".as_ptr() as *const c_char,
                    nobound,
                );
            }
            boundtypes = ivector(1, nobound);
            // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
            boundtopos = imatrix(1, nobound, 0, maxsidenodes - 1);
            boundnodes = ivector(1, nobound);
            i = 1;
            while i <= nobound {
                ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                cp = line.as_mut_ptr();
                *boundtypes.wrapping_offset(i as isize) = next_int(&mut cp);
                maxsidenodes = next_int(&mut cp);
                j = 0;
                while j < maxsidenodes && !cp.is_null() {
                    *(*boundtopos.wrapping_offset(i as isize)).wrapping_offset(j as isize) =
                        next_int(&mut cp);
                    j += 1;
                }
                *boundnodes.wrapping_offset(i as isize) = j;
                i += 1;
            }
            mode = 0;
        } else if mode == 8 {
            fieldview_elems(
                data, &mut mode, &mut noelements, &mut maxnodes, &mut maxindx,
                &mut nobulk, &mut nobound, maxsidenodes, noknots, &mut boundtopos,
                &mut boundnodes, &mut boundtypes, &mut origtopology, &mut i, &mut j,
                &mut k, line.as_mut_ptr(), &mut cp, input, info,
            );
            mode = 0;
        } else if mode == 9 {
            printf(b"Variables\n\0".as_ptr() as *const c_char);
            mode = 0;
        }
        // NOTE: default arm is empty in C.
    }
    // NOTE: `end:` landing pad (C `goto` target).
    if maxindx != noknots {
        printf(
            b"The maximum index %d differs from the number of nodes %d !\n\0".as_ptr()
                as *const c_char,
            maxindx,
            noknots,
        );
    }
    ElementsToBoundaryConditions(data, bound, FALSE, TRUE);
    // NOTE: C never fcloses the fieldview file; mirrored (leak preserved).
    0
}
// SAFETY: fieldview bulk/boundary element section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn fieldview_elems(
    data: *mut FemType,
    mode: &mut c_int,
    noelements: &mut c_int,
    maxnodes: &mut c_int,
    maxindx: &mut c_int,
    nobulk: &mut c_int,
    nobound: &mut c_int,
    maxsidenodes: c_int,
    noknots: c_int,
    boundtopos: &mut *mut *mut c_int,
    boundnodes: &mut *mut c_int,
    boundtypes: &mut *mut c_int,
    origtopology: &mut *mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    line: *mut c_char,
    cp: &mut *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    let _ = mode;
    if info != 0 {
        printf(b"Loading bulk element definitions\n\0".as_ptr() as *const c_char);
    }
    if maxsidenodes == 4 {
        *noelements = noknots + *nobound;
    } else {
        *noelements = 6 * noknots + *nobound;
    }
    *origtopology = ivector(0, *maxnodes - 1);
    // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
    (*data).topology = imatrix(1, *noelements, 0, *maxnodes - 1);
    (*data).material = ivector(1, *noelements);
    (*data).elementtypes = ivector(1, *noelements);
    *i = 0;
    loop {
        ioptr = fgets(line, MAXLINESIZE as c_int, input);
        *cp = line;
        if !strstr(line, b"Variables\0".as_ptr() as *const c_char).is_null() {
            *mode = 9;
        }
        if *mode != 8 {
            break;
        }
        *i += 1;
        *k = next_int(&mut (*cp));
        if *k == 2 {
            *maxnodes = 8;
        } else if *k == 1 {
            *maxnodes = 4;
        }
        *(*data).material.wrapping_offset(*i as isize) = next_int(&mut (*cp));
        *j = 0;
        while *j < *maxnodes && !(*cp).is_null() {
            *(*origtopology).wrapping_offset(*j as isize) = next_int(&mut (*cp));
            if *maxindx < *(*origtopology).wrapping_offset(*j as isize) {
                *maxindx = *(*origtopology).wrapping_offset(*j as isize);
            }
            *j += 1;
        }
        ReorderFieldviewNodes(data, *origtopology, *i, 3, *j);
        if *nobulk + *nobound == *noelements + 1 {
            printf(b"Too few elements (%d) were allocated!!\n\0".as_ptr() as *const c_char, *noelements);
        }
    }
    *nobulk = *i;
    printf(b"Found %d bulk elements\n\0".as_ptr() as *const c_char, *nobulk);
    if *nobulk + *nobound > *noelements {
        printf(b"Too few elements (%d) were allocated!!\n\0".as_ptr() as *const c_char, *noelements);
    }
    printf(
        b"Allocated %.4lg %% too many elements\n\0".as_ptr() as *const c_char,
        *noelements as Real * 100.0 / (*nobulk + *nobound) as Real - 100.0,
    );
    *i = 1;
    while *i <= *nobound {
        ReorderFieldviewNodes(data, *(*boundtopos).wrapping_offset(*i as isize), *i + *nobulk, 2, *(*boundnodes).wrapping_offset(*i as isize));
        *(*data).material.wrapping_offset((*i + *nobulk) as isize) =
            *(*boundtypes).wrapping_offset(*i as isize);
        *i += 1;
    }
    *noelements = *nobulk + *nobound;
    (*data).noelements = *noelements;
}
// SAFETY: triangle reader over caller-owned data/bound; C status codes.
#[no_mangle]
pub unsafe extern "C" fn LoadTriangleInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut noknots: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut noelements: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut maxnodes: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut elematts: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut nodeatts: c_int = 0;
    // NOTE: 0-init (sscanf overwrites before reads).
    let mut dim: c_int = 0;
    // NOTE: assigned once before every read.
    let elementtype: c_int;
    // NOTE: 0-init for the &mut handoff (set in triangle_boundary).
    let mut bcmarkers: c_int = 0;
    // NOTE: 0-init for the &mut handoff (GetElementSide overwrites).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut offset: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bcinds: c_int;
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in triangle_boundary).
    let mut ii: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in triangle_boundary).
    let mut j: c_int = 0;
    // NOTE: `k = 0` follows before every use.
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut jmin: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut jmax: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut kmin: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut kmax: c_int;
    // NOTE: ivector overwrites unconditionally below before any use.
    let boundnodes: *mut c_int;
    let mut input: *mut FILE;
    // NOTE: `cp = line...` overwrites unconditionally below before any use.
    let mut cp: *mut c_char;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut elemfile: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut nodefile: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut polyfile: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (CreateInverseTopology before guarded reads).
    let mut invrow: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (CreateInverseTopology before guarded reads).
    let mut invcol: *mut c_int = core::ptr::null_mut();
    if info != 0 {
        printf(
            b"Loading mesh in Triangle format from file %s.*\n\0".as_ptr() as *const c_char,
            prefix,
        );
    }
    sprintf(nodefile.as_mut_ptr(), b"%s.node\0".as_ptr() as *const c_char, prefix);
    input = fopen(nodefile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadTriangleInput: The opening of the nodes file %s failed!\n\0".as_ptr()
                as *const c_char,
            nodefile.as_ptr(),
        );
        return 1;
    } else {
        printf(b"Loading nodes from file %s\n\0".as_ptr() as *const c_char, nodefile.as_ptr());
    }
    // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr write-only here.
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    sscanf(
        line.as_ptr(),
        b"%d %d %d %d\0".as_ptr() as *const c_char,
        &mut noknots,
        &mut dim,
        &mut nodeatts,
        &mut bcmarkers,
    );
    fclose(input);
    if dim != 2 {
        printf(
            b"LoadTriangleInput assumes that the space dimension is 2, not %d.\n\0".as_ptr()
                as *const c_char,
            dim,
        );
        return 2;
    }
    sprintf(elemfile.as_mut_ptr(), b"%s.ele\0".as_ptr() as *const c_char, prefix);
    input = fopen(elemfile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadTriangleInput: The opening of the element file %s failed!\n\0".as_ptr()
                as *const c_char,
            elemfile.as_ptr(),
        );
        return 3;
    } else {
        printf(b"Loading elements from file %s\n\0".as_ptr() as *const c_char, elemfile.as_ptr());
    }
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    sscanf(
        line.as_ptr(),
        b"%d %d %d\0".as_ptr() as *const c_char,
        &mut noelements,
        &mut maxnodes,
        &mut elematts,
    );
    let _ = elematts;
    fclose(input);
    InitializeKnots(data);
    (*data).dim = dim;
    (*data).maxnodes = maxnodes;
    (*data).noelements = noelements;
    (*data).noknots = noknots;
    elementtype = 300 + maxnodes;
    bcinds = FALSE;
    if info != 0 {
        printf(
            b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
            noknots,
            noelements,
        );
    }
    AllocateKnots(data);
    // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
    boundnodes = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *boundnodes.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    k = 0;
    jmax = 0;
    jmin = noknots;
    input = fopen(nodefile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    i = 1;
    while i <= noknots {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        cp = line.as_mut_ptr();
        j = next_int(&mut cp);
        if j != i {
            k += 1;
        }
        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
        if jmax < j {
            jmax = j;
        }
        if jmin > j {
            jmin = j;
        }
        *(*data).x.wrapping_offset(i as isize) = next_real(&mut cp);
        *(*data).y.wrapping_offset(i as isize) = next_real(&mut cp);
        j = 0;
        while j < nodeatts {
            next_real(&mut cp);
            j += 1;
        }
        if bcmarkers > 0 {
            *boundnodes.wrapping_offset(i as isize) = next_int(&mut cp);
        }
        i += 1;
    }
    fclose(input);
    if info != 0 {
        printf(
            b"LoadTriangleInput: Node index different from order index for %d nodes\n\0".as_ptr()
                as *const c_char,
            k,
        );
        printf(
            b"LoadTriangleInput: Node index ranged was [%d %d]\n\0".as_ptr() as *const c_char,
            jmin,
            jmax,
        );
    }
    offset = 0;
    if jmin == 0 {
        printf(b"LoadTriangleInput: Using offset 1 to because of C-type indexing!\n\0".as_ptr() as *const c_char);
        offset = 1;
    }
    k = 0;
    jmax = 0;
    jmin = noelements;
    kmax = 0;
    kmin = noknots;
    input = fopen(elemfile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
    i = 1;
    while i <= noelements {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
        cp = line.as_mut_ptr();
        *(*data).elementtypes.wrapping_offset(i as isize) = elementtype;
        j = next_int(&mut cp);
        if j != i {
            k += 1;
        }
        if jmin > j {
            jmin = j;
        }
        if jmax < j {
            jmax = j;
        }
        j = 0;
        while j < 3 {
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) =
                next_int(&mut cp);
            j += 1;
        }
        if maxnodes == 6 {
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4) =
                next_int(&mut cp);
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(5) =
                next_int(&mut cp);
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3) =
                next_int(&mut cp);
        }
        if offset != 0 {
            j = 0;
            while j < maxnodes {
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) +=
                    offset;
                j += 1;
            }
        }
        j = 0;
        while j < maxnodes {
            if kmax
                < *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
            {
                kmax = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
            }
            j += 1;
        }
        j = 0;
        while j < maxnodes {
            if kmin
                > *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
            {
                kmin = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
            }
            j += 1;
        }
        j = next_int(&mut cp);
        // NOTE: C `MAX` macro (egdef.h:97); arms pure.
        if 1 > j {
            *(*data).material.wrapping_offset(i as isize) = 1;
        } else {
            *(*data).material.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    fclose(input);
    if info != 0 {
        printf(
            b"LoadTriangleInput: Element index different from order index for %d nodes\n\0".as_ptr()
                as *const c_char,
            k,
        );
        printf(
            b"LoadTriangleInput: Element index ranged was [%d %d]\n\0".as_ptr() as *const c_char,
            jmin,
            jmax,
        );
        printf(
            b"LoadTriangleInput: Node indexes in elements range [%d %d] (with offset)\n\0".as_ptr()
                as *const c_char,
            kmin,
            kmax,
        );
    }
    sprintf(polyfile.as_mut_ptr(), b"%s.edge\0".as_ptr() as *const c_char, prefix);
    input = fopen(polyfile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadTriangleInput: The opening of the poly file %s failed!\n\0".as_ptr()
                as *const c_char,
            polyfile.as_ptr(),
        );
        return 1;
    } else {
        printf(b"Loading boundaries from file %s\n\0".as_ptr() as *const c_char, polyfile.as_ptr());
    }
    triangle_boundary(
        data, bound, &mut bcinds, &mut sideelemtype, &mut jmin, &mut jmax, &mut i,
        &mut ii, &mut j, &mut k, &mut invrow, &mut invcol, line.as_mut_ptr(),
        polyfile.as_mut_ptr(), input, info, noknots, offset,
    );
    // NOTE: `finish:` landing pad (C `goto` target).
    printf(b"Successfully read the mesh from the Triangle input file.\n\0".as_ptr() as *const c_char);
    0
}
// SAFETY: triangle .edge boundary section (split for sizing); the C
// `goto finish` on zero elements skips the block.
#[allow(clippy::too_many_arguments)]
unsafe fn triangle_boundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    bcinds: &mut c_int,
    sideelemtype: &mut c_int,
    jmin: &mut c_int,
    jmax: &mut c_int,
    i: &mut c_int,
    ii: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    invrow: &mut *mut c_int,
    invcol: &mut *mut c_int,
    line: *mut c_char,
    polyfile: *mut c_char,
    input: *mut FILE,
    info: c_int,
    noknots: c_int,
    offset: c_int,
) {
    // NOTE: block-scoped C locals become a Rust block.
    {
        // NOTE: 0-init (sscanf overwrites before reads).
        let mut bcelems: c_int = 0;
        // NOTE: 0-init (sscanf overwrites before reads).
        let mut markers: c_int = 0;
        // NOTE: 0-init (sscanf overwrites before reads).
        let mut ind1: c_int = 0;
        // NOTE: 0-init (sscanf overwrites before reads).
        let mut ind2: c_int = 0;
        // NOTE: `bctype = 1` follows before every use.
        let mut bctype: c_int;
        let mut j2: c_int;
        let mut k2: c_int;
        let mut hit: c_int;
        let elemsides: c_int;
        let mut sideind: [c_int; 2] = [0; 2];
        // NOTE: C declares `side` here but it is scoped to search+use below.
        // NOTE: assigned before every read (C `=0` decl init shape).
        let mut elemind: c_int = 0;
        bctype = 1;
        elemsides = 3;
        hit = FALSE;
        let _ = elemind;
        ioptr = fgets(line, MAXLINESIZE as c_int, input);
        sscanf(
            line,
            b"%d %d\0".as_ptr() as *const c_char,
            &mut bcelems,
            &mut markers,
        );
        // NOTE: C `goto finish` (skip block on zero elements).
        if bcelems != 0 {
            if info != 0 {
                printf(
                    b"Allocating for %d boundary elements.\n\0".as_ptr() as *const c_char,
                    bcelems,
                );
            }
            CreateInverseTopology(data, info);
            *invrow = (*data).invtopo.rows;
            *invcol = (*data).invtopo.cols;
            AllocateBoundary(bound, bcelems);
            *jmin = noknots;
            *jmax = 0;
            *i = 0;
            *ii = 1;
            while *ii <= bcelems {
                ioptr = fgets(line, MAXLINESIZE as c_int, input);
                if markers != 0 {
                    sscanf(
                        line,
                        b"%d %d %d %d\0".as_ptr() as *const c_char,
                        &mut (*j),
                        &mut ind1,
                        &mut ind2,
                        &mut bctype,
                    );
                } else {
                    sscanf(
                        line,
                        b"%d %d %d\0".as_ptr() as *const c_char,
                        &mut (*j),
                        &mut ind1,
                        &mut ind2,
                    );
                }
                if bctype == 0 {
                    *ii += 1;
                    continue;
                }
                *i += 1;
                bctype = abs(bctype);
                ind1 += offset;
                ind2 += offset;
                // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
                if *jmin > ind1 && ind1 < ind2 {
                    *jmin = ind1;
                } else if *jmin > ind2 {
                    *jmin = ind2;
                }
                if *jmax < ind1 && ind1 > ind2 {
                    *jmax = ind1;
                } else if *jmax < ind2 {
                    *jmax = ind2;
                }
                // NOTE: find an element which owns both the nodes.
                *j = *(*invrow).wrapping_offset((ind1 - 1) as isize);
                while *j < *(*invrow).wrapping_offset(ind1 as isize) {
                    *k = *(*invcol).wrapping_offset(*j as isize) + 1;
                    hit = FALSE;
                    j2 = *(*invrow).wrapping_offset((ind2 - 1) as isize);
                    while j2 < *(*invrow).wrapping_offset(ind2 as isize) {
                        k2 = *(*invcol).wrapping_offset(j2 as isize) + 1;
                        if *k == k2 {
                            hit = TRUE;
                            elemind = *k;
                            break;
                        }
                        j2 += 1;
                    }
                    if hit != 0 {
                        break;
                    }
                    *j += 1;
                }
                // NOTE: find the correct side of the triangular element.
                // `side` is scoped to search+use (C declares it wider but
                // every read is dominated by the search assignment).
                if hit != 0 {
                    *k = 0;
                    let mut side = 0;
                    while side < elemsides {
                        GetElementSide(
                            elemind,
                            side,
                            1,
                            data,
                            sideind.as_mut_ptr(),
                            &mut (*sideelemtype),
                        );
                        hit = FALSE;
                        if sideind[0] == ind1 && sideind[1] == ind2 {
                            hit = TRUE;
                        }
                        if sideind[0] == ind2 && sideind[1] == ind1 {
                            hit = TRUE;
                        }
                        if hit != 0 {
                            break;
                        }
                        side += 1;
                    }
                    if hit != 0 {
                        *(*bound).parent.wrapping_offset(*i as isize) = elemind;
                        *(*bound).side.wrapping_offset(*i as isize) = side;
                    } else {
                        *(*bound).parent.wrapping_offset(*i as isize) = 0;
                        *(*bound).side.wrapping_offset(*i as isize) = 0;
                        if *bcinds == 0 {
                            // NOTE: C `Imatrix` is a macro for `imatrix`.
                            (*bound).topology = imatrix(1, bcelems, 0, 1);
                            *bcinds = TRUE;
                        }
                        *(*(*bound).topology.wrapping_offset(*i as isize))
                            .wrapping_offset(0) = ind1;
                        *(*(*bound).topology.wrapping_offset(*i as isize))
                            .wrapping_offset(1) = ind2;
                    }
                } else {
                    *(*bound).parent.wrapping_offset(*i as isize) = 0;
                    *(*bound).side.wrapping_offset(*i as isize) = 0;
                    if *bcinds == 0 {
                        // NOTE: C `Imatrix` is a macro for `imatrix`.
                        (*bound).topology = imatrix(1, bcelems, 0, 1);
                        *bcinds = TRUE;
                    }
                    *(*(*bound).topology.wrapping_offset(*i as isize))
                        .wrapping_offset(0) = ind1;
                    *(*(*bound).topology.wrapping_offset(*i as isize))
                        .wrapping_offset(1) = ind2;
                }
                *(*bound).parent2.wrapping_offset(*i as isize) = 0;
                *(*bound).side2.wrapping_offset(*i as isize) = 0;
                *(*bound).types.wrapping_offset(*i as isize) = bctype;
                *k += 1;
                // NOTE: `if(0)` diagnostic omitted (runtime-dead).
                *ii += 1;
            }
            (*bound).nosides = *i;
            printf(
                b"LoadTriangleInput: Node indexes in boundary range [%d %d] (with offset)\n\0".as_ptr()
                    as *const c_char,
                *jmin,
                *jmax,
            );
        }
    }
    let _ = polyfile;
}
// SAFETY: medit reader over caller-owned data; C status codes.
// Two passes (count then fill) exactly as in C (`allocate`/`end`).
#[no_mangle]
pub unsafe extern "C" fn LoadMeditInput(
    data: *mut FemType,
    _bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut noknots: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut noelements: c_int = 0;
    // NOTE: reset before every use (pass loop).
    let mut maxnodes: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut dim: c_int = 0;
    // NOTE: `elementtype = 303` in TRIANGLES dominates the only read.
    let mut elementtype: c_int;
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    let mut input: *mut FILE;
    // NOTE: `cp = line...` overwrites unconditionally below before any use.
    let mut cp: *mut c_char;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut nodefile: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    sprintf(nodefile.as_mut_ptr(), b"%s.mesh\0".as_ptr() as *const c_char, prefix);
    if info != 0 {
        printf(
            b"Loading mesh in Medit format from file %s\n\0".as_ptr() as *const c_char,
            prefix,
        );
    }
    input = fopen(nodefile.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadMeditInput: The opening of the mesh file %s failed!\n\0".as_ptr()
                as *const c_char,
            nodefile.as_ptr(),
        );
        return 1;
    }
    allocated = FALSE;
    maxnodes = 0;
    // NOTE: `allocate:` label (second pass re-enters below).
    loop {
        if allocated != 0 {
            InitializeKnots(data);
            (*data).dim = dim;
            (*data).maxnodes = maxnodes;
            (*data).noelements = noelements;
            (*data).noknots = noknots;
            // NOTE: C sets `elementtype = 300 + maxnodes` here, but the only
            // read is dominated by `elementtype = 303` in TRIANGLES; dropped.
            if info != 0 {
                printf(
                    b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
                    noknots,
                    noelements,
                );
            }
            AllocateKnots(data);
            // NOTE: C reopens without a NULL check; mirrored exactly.
            input = fopen(nodefile.as_ptr(), b"r\0".as_ptr() as *const c_char);
        }
        loop {
            if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            if !strstr(line.as_ptr(), b"DIMENSION\0".as_ptr() as *const c_char).is_null() {
                if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                    break;
                }
                cp = line.as_mut_ptr();
                dim = next_int(&mut cp);
                printf(b"dim = %d %s\0".as_ptr() as *const c_char, dim, line.as_ptr());
            } else if !strstr(line.as_ptr(), b"VERTICES\0".as_ptr() as *const c_char).is_null() {
                printf(b"verts: %s\0".as_ptr() as *const c_char, line.as_ptr());
                if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                    break;
                }
                cp = line.as_mut_ptr();
                noknots = next_int(&mut cp);
                printf(b"noknots = %d %s\0".as_ptr() as *const c_char, noknots, line.as_ptr());
                i = 1;
                while i <= noknots {
                    // NOTE: C GETLINE macro (`ioptr=fgets(...)`); ioptr write-only.
                    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                    if allocated != 0 {
                        cp = line.as_mut_ptr();
                        *(*data).x.wrapping_offset(i as isize) = next_real(&mut cp);
                        *(*data).y.wrapping_offset(i as isize) = next_real(&mut cp);
                        if dim > 2 {
                            *(*data).z.wrapping_offset(i as isize) = next_real(&mut cp);
                        }
                    }
                    i += 1;
                }
            } else if !strstr(line.as_ptr(), b"TRIANGLES\0".as_ptr() as *const c_char).is_null() {
                if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                    break;
                }
                cp = line.as_mut_ptr();
                noelements = next_int(&mut cp);
                printf(b"noelements = %d %s\0".as_ptr() as *const c_char, noelements, line.as_ptr());
                elementtype = 303;
                if maxnodes < 3 {
                    maxnodes = 3;
                }
                i = 1;
                while i <= noelements {
                    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, input);
                    if allocated != 0 {
                        cp = line.as_mut_ptr();
                        *(*data).elementtypes.wrapping_offset(i as isize) = elementtype;
                        j = 0;
                        while j < 3 {
                            *(*(*data).topology.wrapping_offset(i as isize))
                                .wrapping_offset(j as isize) = next_int(&mut cp);
                            j += 1;
                        }
                        *(*data).material.wrapping_offset(i as isize) = next_int(&mut cp);
                    }
                    i += 1;
                }
            }
            // NOTE: `#if 0` unknown-command arm omitted (dead in C).
        }
        // NOTE: `end:` landing pad (C `goto` target).
        fclose(input);
        printf(b"ALLOCATED=%d\n\0".as_ptr() as *const c_char, allocated);
        if allocated == 0 {
            allocated = TRUE;
            // NOTE: C `goto allocate` (second pass).
            continue;
        }
        printf(b"Successfully read the mesh from the Medit input file.\n\0".as_ptr() as *const c_char);
        return 0;
    }
}
// SAFETY: gid reader over caller-owned data/bound; C status codes.
// Two passes (count then fill) exactly as in C (`omstart`/`end`).
#[no_mangle]
pub unsafe extern "C" fn LoadGidInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut noknots: c_int;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut noelements: c_int;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut maxnodes: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut foundsame: c_int = 0;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut mode: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut nosides: c_int;
    // NOTE: 0-init for the &mut handoff (GetElementSide overwrites).
    let mut sideelemtype: c_int = 0;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut boundarytype: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut parent: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut elemsides: c_int = 0;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut materialtype: c_int = 0;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut dim: c_int = 0;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut elemnodes: c_int = 0;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut elembasis: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut elemtype: c_int = 0;
    // NOTE: reset in the pass loop before the &mut handoff.
    let mut bulkdone: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut usedmax: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut hits: c_int = 0;
    // NOTE: `= 1000` pre-loop seed follows before every use.
    let mut minbulk: c_int;
    // NOTE: `= 0` pre-loop seed follows before every use.
    let mut maxbulk: c_int;
    // NOTE: `= 1000` pre-loop seed follows before every use.
    let mut minbound: c_int;
    // NOTE: `= 0` pre-loop seed follows before every use.
    let mut maxbound: c_int;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut label: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut debug: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut usedno: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Imatrix before guarded reads).
    let mut usedelem: *mut *mut c_int = core::ptr::null_mut();
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: `cp = line...` overwrites below, but the &mut handoff needs init.
    let mut cp: *mut c_char = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut n: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in helpers before reads).
    let mut ind: c_int = 0;
    let mut inds: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut input: *mut FILE;
    // NOTE: 0-init (next_real overwrites before reads).
    let mut x: Real = 0.0;
    // NOTE: 0-init (next_real overwrites before reads).
    let mut y: Real = 0.0;
    // NOTE: 0-init (next_real overwrites before reads).
    let mut z: Real = 0.0;
    debug = FALSE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"msh\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadGidInput: opening of the GID-file '%s' wasn't successful !\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(b"Reading mesh from GID file %s.\n\0".as_ptr() as *const c_char, filename.as_ptr());
    InitializeKnots(data);
    allocated = FALSE;
    // NOTE: the file gives no counts; read twice, register on 2nd pass.
    minbulk = 1000;
    minbound = 1000;
    maxbulk = 0;
    maxbound = 0;
    // NOTE: `omstart:` label (second pass rewinds below).
    loop {
        mode = 0;
        maxnodes = 0;
        noknots = 0;
        noelements = 0;
        boundarytype = 0;
        nosides = 0;
        bulkdone = FALSE;
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            // NOTE: one exclusive if/else chain in C; the command half
            // runs first and skips the mode half when it matches.
            if !gid_cmd(
                data, &mut mode, &mut dim, &mut elembasis, &mut elemnodes,
                &mut maxnodes, &mut elemtype, &mut bulkdone, &mut boundarytype,
                &mut usedno, &mut usedmax, &mut usedelem, &mut i, &mut j,
                &mut k, &mut n, &mut ind, &mut debug, line.as_mut_ptr(), allocated,
                info,
            ) {
                gid_data(
                    data, bound, mode, dim, elembasis, elemnodes, elemtype,
                    &mut noknots, &mut noelements, &mut nosides, &mut boundarytype,
                    &mut side, &mut parent, &mut elemsides, &mut materialtype,
                    &mut minbulk, &mut maxbulk, &mut minbound, &mut maxbound,
                    &mut label, &mut foundsame,
                    &mut hits, &mut usedno, &mut usedmax, &mut usedelem, &mut inds,
                    &mut sideind, &mut sideelemtype, &mut i, &mut j, &mut k,
                    &mut ind, &mut cp, line.as_mut_ptr(), input, &mut x, &mut y, &mut z,
                    allocated, debug, info,
                );
            }
        }
        // NOTE: `end:` landing pad (C `goto` target).
        if allocated == 0 {
            rewind(input);
            (*data).noknots = noknots;
            (*data).noelements = noelements;
            (*data).maxnodes = maxnodes;
            (*data).dim = dim;
            if info != 0 {
                printf(
                    b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                    maxnodes,
                );
                printf(
                    b"Initial material indexes are at interval %d to %d.\n\0".as_ptr()
                        as *const c_char,
                    minbulk,
                    maxbulk,
                );
            }
            AllocateKnots(data);
            if info != 0 {
                printf(
                    b"Allocating %d boundary elements\n\0".as_ptr() as *const c_char,
                    nosides,
                );
                printf(
                    b"Initial boundary indexes are at interval %d to %d.\n\0".as_ptr()
                        as *const c_char,
                    minbound,
                    maxbound,
                );
            }
            AllocateBoundary(bound, nosides);
            (*bound).nosides = nosides;
            (*bound).created = TRUE;
            // NOTE: C zeroes `nosides` here, but the pass-2 reset
            // NOTE: C resets `bulkdone` here, but the pass-2 reset
            // overwrites it before every read; dropped.
            // NOTE: C zeroes `boundarytype` here, but the pass-2 reset
            // overwrites it before every read; dropped.
            allocated = TRUE;
            // NOTE: C `goto omstart` (second pass).
            continue;
        }
        (*bound).nosides = nosides;
        // NOTE: C `free_Ivector`/`free_Imatrix` macros (egutils.h:21-22).
        free_ivector(usedno, 1, (*data).noknots);
        free_imatrix(usedelem, 1, (*data).noknots, 1, usedmax);
        if info != 0 {
            printf(b"The mesh was loaded from file %s.\n\0".as_ptr() as *const c_char, filename.as_ptr());
        }
        return 0;
    }
}
// SAFETY: gid command-line dispatch (split for sizing); true = matched.
#[allow(clippy::too_many_arguments)]
unsafe fn gid_cmd(
    data: *mut FemType,
    mode: &mut c_int,
    dim: &mut c_int,
    elembasis: &mut c_int,
    elemnodes: &mut c_int,
    maxnodes: &mut c_int,
    elemtype: &mut c_int,
    bulkdone: &mut c_int,
    boundarytype: &mut c_int,
    usedno: &mut *mut c_int,
    usedmax: &mut c_int,
    usedelem: &mut *mut *mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    n: &mut c_int,
    ind: &mut c_int,
    debug: &mut c_int,
    line: *mut c_char,
    allocated: c_int,
    _info: c_int,
) -> bool {
    if !strstr(line, b"MESH\0".as_ptr() as *const c_char).is_null() {
        if *debug != 0 {
            printf(b"MESH\n\0".as_ptr() as *const c_char);
        }
        if !strstr(line, b"dimension 3\0".as_ptr() as *const c_char).is_null() {
            *dim = 3;
        } else if !strstr(line, b"dimension 2\0".as_ptr() as *const c_char).is_null() {
            *dim = 2;
        } else {
            printf(b"Unknown dimension\n\0".as_ptr() as *const c_char);
        }
        if !strstr(line, b"ElemType Tetrahedra\0".as_ptr() as *const c_char).is_null() {
            *elembasis = 500;
        } else if !strstr(line, b"ElemType Triangle\0".as_ptr() as *const c_char).is_null() {
            *elembasis = 300;
        } else if !strstr(line, b"ElemType Linear\0".as_ptr() as *const c_char).is_null() {
            *elembasis = 200;
        } else {
            printf(b"Unknown elementtype: %s\n\0".as_ptr() as *const c_char, line);
        }
        if !strstr(line, b"Nnode 4\0".as_ptr() as *const c_char).is_null() {
            *elemnodes = 4;
        } else if !strstr(line, b"Nnode 3\0".as_ptr() as *const c_char).is_null() {
            *elemnodes = 3;
        } else if !strstr(line, b"Nnode 2\0".as_ptr() as *const c_char).is_null() {
            *elemnodes = 2;
        } else {
            printf(b"Unknown elementnode: %s\n\0".as_ptr() as *const c_char, line);
        }
        if *elemnodes > *maxnodes {
            *maxnodes = *elemnodes;
        }
        *elemtype = *elembasis + *elemnodes;
        *mode = 0;
        if *debug != 0 {
            printf(b"dim=%d elemtype=%d\n\0".as_ptr() as *const c_char, *dim, *elemtype);
        }
        return true;
    }
    if !strstr(line, b"Coordinates\0".as_ptr() as *const c_char).is_null() {
        if *debug != 0 {
            printf(b"Start coords\n\0".as_ptr() as *const c_char);
        }
        *mode = 1;
        return true;
    }
    if !strstr(line, b"end coordinates\0".as_ptr() as *const c_char).is_null() {
        if *debug != 0 {
            printf(b"End coords\n\0".as_ptr() as *const c_char);
        }
        *mode = 0;
        return true;
    }
    if !strstr(line, b"Elements\0".as_ptr() as *const c_char).is_null() {
        if *bulkdone != 0 {
            if *debug != 0 {
                printf(b"Start boundary elems\n\0".as_ptr() as *const c_char);
            }
            *mode = 3;
            *boundarytype += 1;
        } else {
            if *debug != 0 {
                printf(b"Start bulk elems\n\0".as_ptr() as *const c_char);
            }
            *mode = 2;
        }
        return true;
    }
    if !strstr(line, b"end elements\0".as_ptr() as *const c_char).is_null() {
        if *debug != 0 {
            printf(b"End elems\n\0".as_ptr() as *const c_char);
        }
        *mode = 0;
        if *bulkdone == 0 && allocated != 0 {
            *usedno = ivector(1, (*data).noknots);
            *j = 1;
            while *j <= (*data).noknots {
                *(*usedno).wrapping_offset(*j as isize) = 0;
                *j += 1;
            }
            *j = 1;
            while *j <= (*data).noelements {
                *n = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
                *i = 0;
                while *i < *n {
                    *ind = *(*(*data).topology.wrapping_offset(*j as isize))
                        .wrapping_offset(*i as isize);
                    *(*usedno).wrapping_offset(*ind as isize) += 1;
                    *i += 1;
                }
                *j += 1;
            }
            *usedmax = 0;
            *i = 1;
            while *i <= (*data).noknots {
                if *(*usedno).wrapping_offset(*i as isize) > *usedmax {
                    *usedmax = *(*usedno).wrapping_offset(*i as isize);
                }
                *i += 1;
            }
            *j = 1;
            while *j <= (*data).noknots {
                *(*usedno).wrapping_offset(*j as isize) = 0;
                *j += 1;
            }
            // NOTE: C `Imatrix` is a macro for `imatrix` (egutils.h).
            *usedelem = imatrix(1, (*data).noknots, 1, *usedmax);
            *j = 1;
            while *j <= (*data).noknots {
                *i = 1;
                while *i <= *usedmax {
                    *(*(*usedelem).wrapping_offset(*j as isize)).wrapping_offset(*i as isize) = 0;
                    *i += 1;
                }
                *j += 1;
            }
            *j = 1;
            while *j <= (*data).noelements {
                *n = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
                *i = 0;
                while *i < *n {
                    *ind = *(*(*data).topology.wrapping_offset(*j as isize))
                        .wrapping_offset(*i as isize);
                    *(*usedno).wrapping_offset(*ind as isize) += 1;
                    *k = *(*usedno).wrapping_offset(*ind as isize);
                    *(*(*usedelem).wrapping_offset(*ind as isize)).wrapping_offset(*k as isize) =
                        *j;
                    *i += 1;
                }
                *j += 1;
            }
        }
        *bulkdone = TRUE;
        return true;
    }
    if *mode == 0 {
        if *debug != 0 {
            printf(b"mode: %d %s\n\0".as_ptr() as *const c_char, *mode, line);
        }
        return true;
    }
    false
}

// SAFETY: gid data-line modes 1-3 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gid_data(
    data: *mut FemType,
    bound: *mut BoundaryType,
    mode: c_int,
    dim: c_int,
    _elembasis: c_int,
    elemnodes: c_int,
    elemtype: c_int,
    noknots: &mut c_int,
    noelements: &mut c_int,
    nosides: &mut c_int,
    boundarytype: &mut c_int,
    side: &mut c_int,
    parent: &mut c_int,
    elemsides: &mut c_int,
    materialtype: &mut c_int,
    minbulk: &mut c_int,
    maxbulk: &mut c_int,
    minbound: &mut c_int,
    maxbound: &mut c_int,
    label: &mut c_int,
    foundsame: &mut c_int,
    hits: &mut c_int,
    usedno: &mut *mut c_int,
    _usedmax: &mut c_int,
    usedelem: &mut *mut *mut c_int,
    inds: &mut [c_int; MAXNODESD2],
    sideind: &mut [c_int; MAXNODESD1],
    sideelemtype: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    ind: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    _input: *mut FILE,
    x: &mut Real,
    y: &mut Real,
    z: &mut Real,
    allocated: c_int,
    debug: c_int,
    _info: c_int,
) {
    if mode == 1 {
        *cp = line;
        *ind = next_int(&mut (*cp));
        if *ind > *noknots {
            *noknots = *ind;
        }
        *x = next_real(&mut (*cp));
        *y = next_real(&mut (*cp));
        if dim == 3 {
            *z = next_real(&mut (*cp));
        }
        if allocated != 0 {
            *(*data).x.wrapping_offset(*ind as isize) = *x;
            *(*data).y.wrapping_offset(*ind as isize) = *y;
            if dim == 3 {
                *(*data).z.wrapping_offset(*ind as isize) = *z;
            }
        }
    } else if mode == 2 {
        gid_bulk(
            data, elemnodes, elemtype, noelements, materialtype, minbulk, maxbulk,
            label, i, j, k, cp, line, allocated,
        );
    } else if mode == 3 {
        gid_side(
            data, bound, elemnodes, nosides, boundarytype, side, parent, elemsides,
            label, foundsame, hits, usedno, usedelem, minbound, maxbound, inds,
            sideind, sideelemtype, i, j, k, cp, line, allocated, debug,
        );
    }
}
// SAFETY: gid bulk-element rows (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gid_bulk(
    data: *mut FemType,
    elemnodes: c_int,
    elemtype: c_int,
    noelements: &mut c_int,
    materialtype: &mut c_int,
    minbulk: &mut c_int,
    maxbulk: &mut c_int,
    label: &mut c_int,
    i: &mut c_int,
    _j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    allocated: c_int,
) {
    // NOTE: `ind` is local to this arm in C.
    let ind: c_int;
    *cp = line;
    ind = next_int(&mut (*cp));
    if ind > *noelements {
        *noelements = ind;
    }
    *i = 0;
    while *i < elemnodes {
        *k = next_int(&mut (*cp));
        if allocated != 0 {
            *(*(*data).topology.wrapping_offset(ind as isize)).wrapping_offset(*i as isize) =
                *k;
            *(*data).elementtypes.wrapping_offset(ind as isize) = elemtype;
            *(*data).material.wrapping_offset(ind as isize) = 1;
        }
        *i += 1;
    }
    *label = next_int(&mut (*cp));
    if allocated != 0 {
        if *label != 0 {
            *materialtype = *label - *minbulk + 1;
        } else if *maxbulk != 0 {
            *materialtype = *maxbulk - *minbulk + 2;
        } else {
            *materialtype = 1;
        }
        *(*data).material.wrapping_offset(ind as isize) = *materialtype;
    } else {
        if *label > *maxbulk {
            *maxbulk = *label;
        }
        if *label < *minbulk {
            *minbulk = *label;
        }
    }
}
// SAFETY: gid boundary-side rows (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gid_side(
    data: *mut FemType,
    bound: *mut BoundaryType,
    elemnodes: c_int,
    nosides: &mut c_int,
    boundarytype: &mut c_int,
    side: &mut c_int,
    parent: &mut c_int,
    elemsides: &mut c_int,
    label: &mut c_int,
    foundsame: &mut c_int,
    hits: &mut c_int,
    usedno: &mut *mut c_int,
    usedelem: &mut *mut *mut c_int,
    minbound: &mut c_int,
    maxbound: &mut c_int,
    inds: &mut [c_int; MAXNODESD2],
    sideind: &mut [c_int; MAXNODESD1],
    sideelemtype: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    allocated: c_int,
    debug: c_int,
) {
    // NOTE: C stores `ind` here but never reads it; keep the cursor advance.
    *cp = line;
    next_int(&mut (*cp));
    *nosides += 1;
    *i = 0;
    while *i < elemnodes {
        *k = next_int(&mut (*cp));
        inds[*i as usize] = *k;
        *i += 1;
    }
    *label = next_int(&mut (*cp));
    if allocated == 0 {
        if *label != 0 {
            if *label > *maxbound {
                *maxbound = *label;
            }
            if *label < *minbound {
                *minbound = *label;
            }
        }
    }
    if allocated != 0 {
        if *label != 0 {
            *boundarytype = *label - *minbound + 1;
        } else if *maxbound != 0 {
            *boundarytype = *maxbound - *minbound + 2;
        } else {
            *boundarytype = 1;
        }
        *foundsame = FALSE;
        *i = 1;
        while *i <= *(*usedno).wrapping_offset(inds[0] as isize) {
            *parent =
                *(*(*usedelem).wrapping_offset(inds[0] as isize)).wrapping_offset(*i as isize);
            *elemsides =
                *(*data).elementtypes.wrapping_offset(*parent as isize) % 100;
            *side = 0;
            while *side < *elemsides {
                GetElementSide(
                    *parent,
                    *side,
                    1,
                    data,
                    sideind.as_mut_ptr(),
                    &mut (*sideelemtype),
                );
                if elemnodes != *sideelemtype % 100 {
                    printf(b"LoadGidMesh: bug?\n\0".as_ptr() as *const c_char);
                }
                *hits = 0;
                *j = 0;
                while *j < elemnodes {
                    *k = 0;
                    while *k < elemnodes {
                        if sideind[*j as usize] == inds[*k as usize] {
                            *hits += 1;
                        }
                        *k += 1;
                    }
                    *j += 1;
                }
                if *hits < elemnodes {
                    *side += 1;
                    continue;
                }
                if *foundsame == 0 {
                    *foundsame += 1;
                    *(*bound).parent.wrapping_offset(*nosides as isize) = *parent;
                    *(*bound).side.wrapping_offset(*nosides as isize) = *side;
                    *(*bound).parent2.wrapping_offset(*nosides as isize) = 0;
                    *(*bound).side2.wrapping_offset(*nosides as isize) = 0;
                    *(*bound).types.wrapping_offset(*nosides as isize) = *boundarytype;
                } else if *foundsame == 1 {
                    if *parent == *(*bound).parent.wrapping_offset(*nosides as isize) {
                        *side += 1;
                        continue;
                    }
                    *foundsame += 1;
                    *(*bound).parent2.wrapping_offset(*nosides as isize) = *parent;
                    *(*bound).side2.wrapping_offset(*nosides as isize) = *side;
                } else if *foundsame > 1 {
                    printf(
                        b"Boundary %d has more than 2 parents\n\0".as_ptr() as *const c_char,
                        *nosides,
                    );
                }
                *side += 1;
            }
            *i += 1;
        }
        if *foundsame == 0 {
            printf(b"Did not find parent for side %d\n\0".as_ptr() as *const c_char, *nosides);
            *nosides -= 1;
        } else if debug != 0 {
            printf(
                b"Parent of side %d is %d\n\0".as_ptr() as *const c_char,
                *nosides,
                *(*bound).parent.wrapping_offset(*nosides as isize),
            );
        }
    }
}
// SAFETY: comsol node reorder over a valid topo row.
unsafe fn ReorderComsolNodes(elementtype: c_int, topo: *mut c_int) {
    let mut i: c_int;
    let mut tmptopo: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let order404: [c_int; 4] = [1, 2, 4, 3];
    let order808: [c_int; 8] = [1, 2, 4, 3, 5, 6, 8, 7];
    let order605: [c_int; 5] = [1, 2, 4, 3, 5];
    // NOTE: C switch over elementtype.
    if elementtype == 404 {
        i = 0;
        while i < elementtype % 100 {
            tmptopo[i as usize] = *topo.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < elementtype % 100 {
            *topo.wrapping_offset(i as isize) =
                tmptopo[(order404[i as usize] - 1) as usize];
            i += 1;
        }
    } else if elementtype == 808 {
        i = 0;
        while i < elementtype % 100 {
            tmptopo[i as usize] = *topo.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < elementtype % 100 {
            *topo.wrapping_offset(i as isize) =
                tmptopo[(order808[i as usize] - 1) as usize];
            i += 1;
        }
    } else if elementtype == 605 {
        i = 0;
        while i < elementtype % 100 {
            tmptopo[i as usize] = *topo.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < elementtype % 100 {
            *topo.wrapping_offset(i as isize) =
                tmptopo[(order605[i as usize] - 1) as usize];
            i += 1;
        }
    }
    // NOTE: default arm is empty in C.
}

// SAFETY: comsol reader over caller-owned data/bound; C status codes.
// Two passes (count then fill) exactly as in C (`omstart`/`end`).
#[no_mangle]
pub unsafe extern "C" fn LoadComsolMesh(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: assigned in the pass loop before every read.
    let mut noknots: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut noelements: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut maxnodes: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut material: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut dim: c_int = 0;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut elemnodes: c_int = 0;
    // NOTE: `=0` decl inits kept (min/max seed before updates).
    let mut elembasis: c_int = 0;
    // NOTE: assigned in the pass loop before every read.
    let mut elemtype: c_int = 0;
    let debug: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut domains: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mindom: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minbc: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxdom: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxbc: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxlabel: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut elemdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in comsol_label before reads).
    let mut entitylen: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in comsol_label before reads).
    let mut entitydim: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut bclabel: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut domlabel: *mut c_int = core::ptr::null_mut();
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut n_label: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut offset: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bcoffset: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut domoffset: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut bcinfo: *mut c_int = core::ptr::null_mut();
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (next_* overwrites via &mut before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut i: c_int = 0;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in comsol_elems before reads).
    let mut k: c_int = 0;
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(
            prefix,
            filename.as_mut_ptr(),
            b"mphtxt\0".as_ptr() as *const c_char,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            if info != 0 {
                printf(
                    b"LoadComsolMesh: opening of the Comsol mesh file '%s' wasn't successful !\n\0".as_ptr()
                        as *const c_char,
                    filename.as_ptr(),
                );
            }
            return 1;
        }
    }
    if info != 0 {
        printf(
            b"Reading mesh from Comsol mesh file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitializeKnots(data);
    debug = FALSE;
    allocated = FALSE;
    mindom = 1000;
    minbc = 1000;
    maxdom = 0;
    maxlabel = 0;
    maxbc = 0;
    offset = 1;
    // NOTE: `omstart:` label (second pass rewinds below).
    loop {
        maxnodes = 0;
        noknots = 0;
        noelements = 0;
        material = 0;
        domains = 0;
        loop {
            if Comsolrow(line.as_mut_ptr(), input) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"# sdim\0".as_ptr() as *const c_char).is_null() {
                cp = line.as_mut_ptr();
                dim = next_int(&mut cp);
                if info != 0 && allocated == 0 {
                    printf(b"Dimension of mesh is: %d\n\0".as_ptr() as *const c_char, dim);
                }
            } else if !strstr(line.as_ptr(), b"# number of mesh points\0".as_ptr() as *const c_char).is_null()
                || !strstr(line.as_ptr(), b"# number of mesh vertices\0".as_ptr() as *const c_char).is_null()
            {
                cp = line.as_mut_ptr();
                noknots = next_int(&mut cp);
                if debug != 0 {
                    printf(b"noknots=%d\n\0".as_ptr() as *const c_char, noknots);
                }
            } else if !strstr(line.as_ptr(), b"# lowest mesh point index\0".as_ptr() as *const c_char).is_null()
                || !strstr(line.as_ptr(), b"# lowest mesh vertex index\0".as_ptr() as *const c_char).is_null()
            {
                cp = line.as_mut_ptr();
                offset = 1 - next_int(&mut cp);
                if debug != 0 {
                    printf(b"offset=%d\n\0".as_ptr() as *const c_char, offset);
                }
            } else if !strstr(line.as_ptr(), b"# type name\0".as_ptr() as *const c_char).is_null() {
                comsol_basis(line.as_ptr(), &mut elembasis);
            } else if !strstr(line.as_ptr(), b"# number of nodes per element\0".as_ptr() as *const c_char).is_null()
                || !strstr(line.as_ptr(), b"# number of vertices per element\0".as_ptr() as *const c_char).is_null()
            {
                cp = line.as_mut_ptr();
                elemnodes = next_int(&mut cp);
                if elemnodes > maxnodes {
                    maxnodes = elemnodes;
                }
                if debug != 0 {
                    printf(b"elemnodes=%d\n\0".as_ptr() as *const c_char, elemnodes);
                }
            } else if !strstr(line.as_ptr(), b"# Mesh point coordinates\0".as_ptr() as *const c_char).is_null()
                || !strstr(line.as_ptr(), b"# Mesh vertex coordinates\0".as_ptr() as *const c_char).is_null()
            {
                i = 1;
                while i <= noknots {
                    Comsolrow(line.as_mut_ptr(), input);
                    if allocated != 0 {
                        cp = line.as_mut_ptr();
                        *(*data).x.wrapping_offset(i as isize) = next_real(&mut cp);
                        *(*data).y.wrapping_offset(i as isize) = next_real(&mut cp);
                        if dim == 3 {
                            *(*data).z.wrapping_offset(i as isize) = next_real(&mut cp);
                        }
                    }
                    i += 1;
                }
            } else if !strstr(line.as_ptr(), b"# number of elements\0".as_ptr() as *const c_char).is_null() {
                comsol_elems(
                    data, &mut elemtype, &mut elemdim, &mut domains, &mut noelements,
                    elemnodes, elembasis, dim, &mut k, &mut i, &mut j, &mut cp,
                    line.as_mut_ptr(), input, offset, allocated, debug, info,
                );
            } else if !strstr(line.as_ptr(), b"# number of geometric entity indices\0".as_ptr() as *const c_char).is_null()
                || !strstr(line.as_ptr(), b"# number of domains\0".as_ptr() as *const c_char).is_null()
            {
                comsol_domains(
                    data, &mut domains, &mut material, &mut minbc, &mut maxbc,
                    &mut mindom, &mut maxdom, &mut bcinfo, maxlabel, noelements,
                    elemdim, elembasis, dim, &mut bcoffset, &mut domoffset,
                    &mut k, &mut i,
                    &mut cp, line.as_mut_ptr(), input, allocated, debug,
                );
            } else if !strstr(line.as_ptr(), b"# Label\0".as_ptr() as *const c_char).is_null()
                && strstr(line.as_ptr(), b"Union Selection\0".as_ptr() as *const c_char).is_null()
            {
                if comsol_label(
                    data, &mut n_label, &mut maxlabel, &mut entitylen, &mut entitydim,
                    &mut bclabel, &mut domlabel, &mut domoffset, &mut bcoffset,
                    &mut entityname, &mut i, &mut j, &mut k, &mut cp,
                    line.as_mut_ptr(), input, allocated, debug, dim, info,
                ) {
                    break;
                }
            } else if !strstr(line.as_ptr(), b"#\0".as_ptr() as *const c_char).is_null() {
                if debug != 0 {
                    printf(b"Unused command:  %s\0".as_ptr() as *const c_char, line.as_ptr());
                }
            }
        }
        // NOTE: `end:` landing pad (C `goto` target).
        if allocated == 0 {
            if noknots == 0 || noelements == 0 || maxnodes == 0 {
                printf(
                    b"Invalid mesh consists of %d knots and %d %d-node elements.\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                    maxnodes,
                );
                fclose(input);
                return 2;
            }
            rewind(input);
            if info != 0 {
                printf(
                    b"Comsol mesh consists of %d nodes and %d elements\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                );
            }
            (*data).noknots = noknots;
            (*data).noelements = noelements;
            (*data).maxnodes = maxnodes;
            (*data).dim = dim;
            n_label = 0;
            bcoffset = 1 - minbc;
            domoffset = 1 - mindom;
            if info != 0 {
                printf(
                    b"Original body index range is [%d,%d]\n\0".as_ptr() as *const c_char,
                    mindom,
                    maxdom,
                );
                printf(
                    b"Original bc index range is [%d,%d]\n\0".as_ptr() as *const c_char,
                    minbc,
                    maxbc,
                );
                if domoffset != 0 {
                    printf(b"Offset of body indexing set to start from one!\n\0".as_ptr() as *const c_char);
                }
                if bcoffset != 0 {
                    printf(b"Offset of BC indexing set to start from one!\n\0".as_ptr() as *const c_char);
                }
            }
            if maxlabel > 0 {
                if info != 0 {
                    printf(
                        b"Mesh has %d labels with physical names.\n\0".as_ptr() as *const c_char,
                        maxlabel,
                    );
                }
                // NOTE: allocate renumber tables geometric->physical.
                maxbc += 1;
                maxdom += 1;
                // NOTE: C `Ivector` is a macro for `ivector` (egutils.h:16).
                bclabel = ivector(minbc, maxbc);
                i = minbc;
                while i <= maxbc {
                    *bclabel.wrapping_offset(i as isize) = -1;
                    i += 1;
                }
                domlabel = ivector(mindom, maxdom);
                i = mindom;
                while i <= maxdom {
                    *domlabel.wrapping_offset(i as isize) = -1;
                    i += 1;
                }
                bcinfo = ivector(1, noelements);
                i = 1;
                while i <= noelements {
                    *bcinfo.wrapping_offset(i as isize) = FALSE;
                    i += 1;
                }
                // NOTE: bodies look like boundaries unless both names exist.
                (*data).bodynamesexist = TRUE;
                (*data).boundarynamesexist = TRUE;
            }
            if info != 0 {
                printf(
                    b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr()
                        as *const c_char,
                    noknots,
                    noelements,
                    maxnodes,
                );
            }
            AllocateKnots(data);
            allocated = TRUE;
            // NOTE: C `goto omstart` (second pass).
            continue;
        }
        fclose(input);
        // NOTE: perform remapping of entities.
        if maxlabel > 0 {
            if debug != 0 {
                i = minbc;
                while i <= maxbc {
                    if *bclabel.wrapping_offset(i as isize) > 0 {
                        printf(b"bc map: %d %d\n\0".as_ptr() as *const c_char, i, *bclabel.wrapping_offset(i as isize));
                    }
                    i += 1;
                }
                i = mindom;
                while i <= maxdom {
                    if *domlabel.wrapping_offset(i as isize) > 0 {
                        printf(
                            b"bulk map: %d %d\n\0".as_ptr() as *const c_char,
                            i,
                            *domlabel.wrapping_offset(i as isize),
                        );
                    }
                    i += 1;
                }
            }
            i = 1;
            while i <= (*data).noelements {
                j = *(*data).material.wrapping_offset(i as isize);
                if *bcinfo.wrapping_offset(i as isize) != 0 {
                    if *bclabel.wrapping_offset(j as isize) > -1 {
                        *(*data).material.wrapping_offset(i as isize) =
                            *bclabel.wrapping_offset(j as isize);
                    }
                } else if *domlabel.wrapping_offset(j as isize) > -1 {
                    *(*data).material.wrapping_offset(i as isize) =
                        *domlabel.wrapping_offset(j as isize);
                }
                i += 1;
            }
            // NOTE: C `free_Ivector` is a macro for `free_ivector`.
            free_ivector(bclabel, minbc, maxbc);
            free_ivector(domlabel, mindom, maxdom);
            free_ivector(bcinfo, 1, noelements);
        }
        if info != 0 {
            printf(b"Comsol mesh was loaded from file %s.\n\n\0".as_ptr() as *const c_char, filename.as_ptr());
        }
        ElementsToBoundaryConditions(data, bound, FALSE, TRUE);
        return 0;
    }
}
// SAFETY: comsol element-basis map (pure table).
unsafe fn comsol_basis(line: *const c_char, elembasis: &mut c_int) {
    if !strstr(line, b"vtx\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 100;
    } else if !strstr(line, b"edg\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 200;
    } else if !strstr(line, b"tri\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 300;
    } else if !strstr(line, b"quad\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 400;
    } else if !strstr(line, b"tet\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 500;
    } else if !strstr(line, b"pyr\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 600;
    } else if !strstr(line, b"prism\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 700;
    } else if !strstr(line, b"hex\0".as_ptr() as *const c_char).is_null() {
        *elembasis = 800;
    } else {
        printf(b"unknown element type = %s\0".as_ptr() as *const c_char, line);
        bigerror(b"Cannot continue!\n\0".as_ptr() as *const c_char);
    }
}

// SAFETY: comsol element block (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn comsol_elems(
    data: *mut FemType,
    elemtype: &mut c_int,
    elemdim: &mut c_int,
    domains: &mut c_int,
    noelements: &mut c_int,
    elemnodes: c_int,
    elembasis: c_int,
    dim: c_int,
    k: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    offset: c_int,
    allocated: c_int,
    debug: c_int,
    _info: c_int,
) {
    *cp = line;
    *k = next_int(&mut (*cp));
    Comsolrow(line, input);
    *elemtype = elemnodes + elembasis;
    *elemdim = GetElementDimension(*elemtype);
    if debug != 0 {
        printf(b"Loading %d elements of type %d\n\0".as_ptr() as *const c_char, *k, *elemtype);
    }
    *domains = *noelements;
    *i = 1;
    while *i <= *k {
        Comsolrow(line, input);
        if dim == 3 && elembasis < 300 {
            *i += 1;
            continue;
        }
        if dim == 2 && elembasis < 200 {
            *i += 1;
            continue;
        }
        *noelements = *noelements + 1;
        if allocated != 0 {
            *(*data).elementtypes.wrapping_offset(*noelements as isize) = *elemtype;
            *(*data).material.wrapping_offset(*noelements as isize) = 1;
            *cp = line;
            *j = 0;
            while *j < elemnodes {
                *(*(*data).topology.wrapping_offset(*noelements as isize))
                    .wrapping_offset(*j as isize) = next_int(&mut (*cp)) + offset;
                *j += 1;
            }
            ReorderComsolNodes(*elemtype, *(*data).topology.wrapping_offset(*noelements as isize));
        }
        *i += 1;
    }
}

// SAFETY: comsol domain/material block (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn comsol_domains(
    data: *mut FemType,
    domains: &mut c_int,
    material: &mut c_int,
    minbc: &mut c_int,
    maxbc: &mut c_int,
    mindom: &mut c_int,
    maxdom: &mut c_int,
    bcinfo: &mut *mut c_int,
    maxlabel: c_int,
    noelements: c_int,
    elemdim: c_int,
    elembasis: c_int,
    dim: c_int,
    bcoffset: &mut c_int,
    domoffset: &mut c_int,
    k: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    debug: c_int,
) {
    let _ = noelements;
    *cp = line;
    *k = next_int(&mut (*cp));
    Comsolrow(line, input);
    if debug != 0 {
        printf(b"Loading %d domains for the elements\n\0".as_ptr() as *const c_char, *k);
    }
    *i = 1;
    while *i <= *k {
        Comsolrow(line, input);
        if dim == 3 && elembasis < 300 {
            *i += 1;
            continue;
        }
        if dim == 2 && elembasis < 200 {
            *i += 1;
            continue;
        }
        *domains = *domains + 1;
        *cp = line;
        *material = next_int(&mut (*cp));
        if allocated != 0 {
            if elemdim < dim {
                if maxlabel > 0 {
                    *(*bcinfo).wrapping_offset(*domains as isize) = TRUE;
                }
                *(*data).material.wrapping_offset(*domains as isize) =
                    *material + *bcoffset;
            } else {
                *(*data).material.wrapping_offset(*domains as isize) =
                    *material + *domoffset;
            }
        } else if elemdim < dim {
            // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
            if *material < *minbc {
                *minbc = *material;
            }
            if *material > *maxbc {
                *maxbc = *material;
            }
        } else {
            if *material < *mindom {
                *mindom = *material;
            }
            if *material > *maxdom {
                *maxdom = *material;
            }
        }
        *i += 1;
    }
}
// SAFETY: comsol # Label arm (split for sizing); true = break read loop.
#[allow(clippy::too_many_arguments)]
unsafe fn comsol_label(
    data: *mut FemType,
    n_label: &mut c_int,
    maxlabel: &mut c_int,
    entitylen: &mut c_int,
    entitydim: &mut c_int,
    bclabel: &mut *mut c_int,
    domlabel: &mut *mut c_int,
    domoffset: &mut c_int,
    bcoffset: &mut c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    debug: c_int,
    dim: c_int,
    _info: c_int,
) -> bool {
    // NOTE: assigned in arms before every read.
    let mut ind: c_int;
    let mut ind1: c_int = 0;
    let mut n_ent: c_int = 0;
    *n_label += 1;
    // NOTE: C `MAX` macro (egdef.h:97); arms pure.
    if *maxlabel < *n_label {
        *maxlabel = *n_label;
    }
    if debug != 0 {
        printf(b"Reading Label %d\n\0".as_ptr() as *const c_char, *n_label);
    }
    if allocated != 0 {
        // NOTE: read the length of the label and proceed over the empty space.
        *cp = line;
        *entitylen = next_int(&mut (*cp));
        *cp = (*cp).wrapping_offset(1);
        strncpy(entityname.as_mut_ptr(), *cp, *entitylen as usize);
        entityname[*entitylen as usize] = 0 as c_char;
        if debug != 0 {
            printf(b"BoundaryName %d is: %s\n\0".as_ptr() as *const c_char, *n_label, entityname.as_ptr());
        }
    }
    *j = 0;
    *i = 1;
    while *i <= 4 {
        Comsolrow(line, input);
        if !strstr(line, b"# Geometry/mesh tag\0".as_ptr() as *const c_char).is_null() {
            *j += 1;
        }
        if !strstr(line, b"# Dimension\0".as_ptr() as *const c_char).is_null() {
            *cp = line;
            *entitydim = next_int(&mut (*cp));
            if debug != 0 {
                printf(b"Dimension of entity: %d\n\0".as_ptr() as *const c_char, *entitydim);
            }
            *j += 1;
        }
        if !strstr(line, b"# Number of entities\0".as_ptr() as *const c_char).is_null() {
            *j += 1;
            *cp = line;
            n_ent = next_int(&mut (*cp));
            if debug != 0 {
                printf(b"Number of entities: %d\n\0".as_ptr() as *const c_char, n_ent);
            }
        }
        if !strstr(line, b"# Entities\0".as_ptr() as *const c_char).is_null() {
            if debug != 0 {
                printf(b"Reading %d entities\n\0".as_ptr() as *const c_char, n_ent);
            }
            *j += 1;
            *k = 1;
            while *k <= n_ent {
                Comsolrow(line, input);
                if allocated != 0 {
                    *cp = line;
                    if *entitydim == dim {
                        ind = next_int(&mut (*cp)) + *domoffset;
                        if *k == 1 {
                            // NOTE: use the first entity to represent all.
                            ind1 = ind;
                            (*data).bodyname[ind1 as usize] = cvector(0, MAXNAMESIZE as c_int);
                            strncpy(
                                (*data).bodyname[ind1 as usize],
                                entityname.as_ptr(),
                                *entitylen as usize,
                            );
                        }
                        *(*domlabel).wrapping_offset(ind as isize) = ind1;
                        if debug != 0 {
                            printf(
                                b"Mapping bulk: %d %d %d\n\0".as_ptr() as *const c_char,
                                *n_label,
                                ind,
                                ind1,
                            );
                        }
                    } else if *entitydim == dim - 1 {
                        ind = next_int(&mut (*cp)) + *bcoffset;
                        if *k == 1 {
                            ind1 = ind;
                            (*data).boundaryname[ind1 as usize] =
                                cvector(0, MAXNAMESIZE as c_int);
                            strncpy(
                                (*data).boundaryname[ind1 as usize],
                                entityname.as_ptr(),
                                *entitylen as usize,
                            );
                        }
                        *(*bclabel).wrapping_offset(ind as isize) = ind1;
                        if debug != 0 {
                            printf(
                                b"Mapping bc: %d %d %d\n\0".as_ptr() as *const c_char,
                                *n_label,
                                ind,
                                ind1,
                            );
                        }
                    }
                }
                *k += 1;
            }
        }
        *i += 1;
    }
    if *j < 4 {
        if debug != 0 {
            printf(b"We should have number 4 keywords after label so something might be off!\n\0".as_ptr() as *const c_char);
        }
        return true;
    }
    false
}
// SAFETY: gmsh→elmer type map (pure table).
unsafe fn GmshToElmerType(gmshtype: c_int) -> c_int {
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut elmertype: c_int = 0;
    // NOTE: C switch over gmshtype.
    if gmshtype == 1 {
        elmertype = 202;
    } else if gmshtype == 2 {
        elmertype = 303;
    } else if gmshtype == 3 {
        elmertype = 404;
    } else if gmshtype == 4 {
        elmertype = 504;
    } else if gmshtype == 5 {
        elmertype = 808;
    } else if gmshtype == 6 {
        elmertype = 706;
    } else if gmshtype == 7 {
        elmertype = 605;
    } else if gmshtype == 8 {
        elmertype = 203;
    } else if gmshtype == 9 {
        elmertype = 306;
    } else if gmshtype == 10 {
        elmertype = 409;
    } else if gmshtype == 11 {
        elmertype = 510;
    } else if gmshtype == 12 {
        elmertype = 827;
    } else if gmshtype == 15 {
        elmertype = 101;
    } else if gmshtype == 16 {
        elmertype = 408;
    } else if gmshtype == 17 {
        elmertype = 820;
    } else if gmshtype == 18 {
        elmertype = 715;
    } else if gmshtype == 19 {
        elmertype = 613;
    } else if gmshtype == 21 {
        elmertype = 310;
    } else if gmshtype == 26 {
        elmertype = 204;
    } else if gmshtype == 36 {
        elmertype = 416;
    } else if gmshtype == 13 {
        // NOTE: supported by Gmsh but not by ElmerSolver.
        elmertype = 718;
    } else if gmshtype == 14 {
        elmertype = 614;
    } else if gmshtype == 20 {
        elmertype = 309;
    } else if gmshtype == 22 {
        elmertype = 312;
    } else if gmshtype == 24 {
        elmertype = 315;
    } else if gmshtype == 25 {
        elmertype = 320;
    } else if gmshtype == 27 {
        elmertype = 205;
    } else if gmshtype == 28 {
        elmertype = 206;
    } else if gmshtype == 29 {
        elmertype = 520;
    } else {
        printf(
            b"Gmsh element %d does not have an Elmer counterpart!\n\0".as_ptr() as *const c_char,
            gmshtype,
        );
    }
    elmertype
}

// SAFETY: gmsh node reorder over a valid topology row.
unsafe fn GmshToElmerIndx(elemtype: c_int, topology: *mut c_int) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let nodes: c_int;
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut reorder: c_int;
    // NOTE: null-init; read only under reorder (set in the matching arm).
    let mut porder: *const c_int = core::ptr::null();
    let order510: [c_int; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 8];
    let order614: [c_int; 14] = [0, 1, 2, 3, 4, 5, 8, 10, 6, 7, 9, 11, 12, 13];
    let order715: [c_int; 15] = [0, 1, 2, 3, 4, 5, 6, 9, 7, 12, 14, 13, 8, 10, 11];
    let order718: [c_int; 18] = [
        0, 1, 2, 3, 4, 5, 6, 9, 7, 12, 14, 13, 8, 10, 11, 15, 17, 16,
    ];
    let order820: [c_int; 20] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 13, 9, 10, 12, 14, 15, 16, 18, 19, 17,
    ];
    let order827: [c_int; 27] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 13, 9, 10, 12, 14, 15, 16, 18, 19, 17, 21, 23, 24,
        22, 20, 25, 26,
    ];
    reorder = FALSE;
    // NOTE: C switch over elemtype (613/614 share an arm).
    if elemtype == 510 {
        reorder = TRUE;
        porder = order510.as_ptr();
    } else if elemtype == 613 || elemtype == 614 {
        reorder = TRUE;
        porder = order614.as_ptr();
    } else if elemtype == 715 {
        reorder = TRUE;
        porder = order715.as_ptr();
    } else if elemtype == 718 {
        reorder = TRUE;
        porder = order718.as_ptr();
    } else if elemtype == 820 {
        reorder = TRUE;
        porder = order820.as_ptr();
    } else if elemtype == 827 {
        reorder = TRUE;
        porder = order827.as_ptr();
    }
    if reorder != 0 {
        nodes = elemtype % 100;
        i = 0;
        while i < nodes {
            oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < nodes {
            *topology.wrapping_offset(i as isize) =
                oldtopology[*porder.wrapping_offset(i as isize) as usize];
            i += 1;
        }
    }
}
// SAFETY: gmsh-1.0 loader (split for sizing); ports the goto-allocate shape.
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh1_alloc(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxindx: c_int,
    maxnodes: c_int,
    dim: c_int,
    i: &mut c_int,
    filename: *const c_char,
    input: &mut *mut FILE,
    allocated: c_int,
    _info: c_int,
) {
    if allocated != 0 {
        InitializeKnots(data);
        (*data).dim = dim;
        (*data).maxnodes = maxnodes;
        (*data).noelements = noelements;
        (*data).noknots = noknots;
        if _info != 0 {
            printf(
                b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
                noknots,
                noelements,
            );
        }
        AllocateKnots(data);
        if maxindx > noknots {
            *revindx = ivector(1, maxindx);
            *i = 1;
            while *i <= maxindx {
                *(*revindx).wrapping_offset(*i as isize) = 0;
                *i += 1;
            }
        }
        *input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    }
}

// SAFETY: gmsh-1.0 nodes section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh1_nodes(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: &mut c_int,
    maxindx: &mut c_int,
    dim: c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    Getrow(line, input, TRUE);
    *cp = line;
    *noknots = next_int(&mut (*cp));
    *i = 1;
    while *i <= *noknots {
        Getrow(line, input, TRUE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if allocated != 0 {
            if *maxindx > *noknots {
                *(*revindx).wrapping_offset(*j as isize) = *i;
            }
            *(*data).x.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            *(*data).y.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            if dim > 2 {
                *(*data).z.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            }
        } else {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *j > *maxindx {
                *maxindx = *j;
            }
        }
        *i += 1;
    }
    Getrow(line, input, TRUE);
    if strstr(line, b"$ENDNOD\0".as_ptr() as *const c_char).is_null() {
        printf(b"NOD section should end to string ENDNOD\n\0".as_ptr() as *const c_char);
        printf(b"%s\n\0".as_ptr() as *const c_char, line);
    }
}

// SAFETY: gmsh-1.0 elements section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh1_elems(
    data: *mut FemType,
    elemind: *mut c_int,
    elementtype: &mut c_int,
    maxelemtype: &mut c_int,
    noelements: &mut c_int,
    elemnodes: &mut c_int,
    maxnodes: &mut c_int,
    elemno: &mut c_int,
    gmshtype: &mut c_int,
    regphys: &mut c_int,
    regelem: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    Getrow(line, input, TRUE);
    *cp = line;
    *noelements = next_int(&mut (*cp));
    *i = 1;
    while *i <= *noelements {
        Getrow(line, input, TRUE);
        *cp = line;
        *elemno = next_int(&mut (*cp));
        *gmshtype = next_int(&mut (*cp));
        *regphys = next_int(&mut (*cp));
        *regelem = next_int(&mut (*cp));
        *elemnodes = next_int(&mut (*cp));
        if allocated != 0 {
            *elementtype = GmshToElmerType(*gmshtype);
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxelemtype < *elementtype {
                *maxelemtype = *elementtype;
            }
            *(*data).elementtypes.wrapping_offset(*i as isize) = *elementtype;
            *(*data).material.wrapping_offset(*i as isize) = *regphys;
            if *elemnodes != *elementtype % 100 {
                printf(
                    b"Conflict in elementtypes %d and number of nodes %d!\n\0".as_ptr()
                        as *const c_char,
                    *elementtype,
                    *elemnodes,
                );
            }
            *j = 0;
            while *j < *elemnodes {
                *elemind.wrapping_offset(*j as isize) = next_int(&mut (*cp));
                *j += 1;
            }
            GmshToElmerIndx(*elementtype, elemind);
            *j = 0;
            while *j < *elemnodes {
                *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) =
                    *elemind.wrapping_offset(*j as isize);
                *j += 1;
            }
        } else {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxnodes < *elemnodes {
                *maxnodes = *elemnodes;
            }
        }
        *i += 1;
    }
    Getrow(line, input, TRUE);
    if strstr(line, b"$ENDELM\0".as_ptr() as *const c_char).is_null() {
        printf(b"ELM section should end to string ENDELM\n\0".as_ptr() as *const c_char);
    }
}
// SAFETY: gmsh node renumber (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh1_renumber(
    data: *mut FemType,
    revindx: *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxindx: c_int,
    kind: *const c_char,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    if maxindx > noknots {
        printf(
            b"Renumbering the %s nodes from %d to %d\n\0".as_ptr() as *const c_char,
            kind,
            maxindx,
            noknots,
        );
        *i = 1;
        while *i <= noelements {
            *elementtype = *(*data).elementtypes.wrapping_offset(*i as isize);
            *elemnodes = *elementtype % 100;
            *j = 0;
            while *j < *elemnodes {
                *k = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                if *k <= 0 || *k > maxindx {
                    printf(b"index out of bounds %d\n\0".as_ptr() as *const c_char, *k);
                } else if *revindx.wrapping_offset(*k as isize) <= 0 {
                    printf(
                        b"unknown node %d %d in element %d\n\0".as_ptr() as *const c_char,
                        *k,
                        *revindx.wrapping_offset(*k as isize),
                        *i,
                    );
                } else {
                    *(*(*data).topology.wrapping_offset(*i as isize))
                        .wrapping_offset(*j as isize) =
                        *revindx.wrapping_offset(*k as isize);
                }
                *j += 1;
            }
            *i += 1;
        }
        free_ivector(revindx, 1, maxindx);
    }
}

/// Load the grid in Gmsh 1.0 format.
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGmshInput1(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noknots: c_int = 0;
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noelements: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnodes: c_int;
    let dim: c_int;
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut elementtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_nodes before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_nodes before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_renumber before reads).
    let mut k: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindx: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut elemno: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut gmshtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut regphys: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut regelem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_elems before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    let mut input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut allocated: c_int;
    input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadGmshInput: The opening of the mesh file %s failed!\n\0".as_ptr() as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in Gmsh format 1.0 from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = (*data).dim;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 0;
    // NOTE: C `allocate:` label + `goto allocate`.
    'allocate: loop {
        gmsh1_alloc(
            data, &mut revindx, noknots, noelements, maxindx, maxnodes, dim, &mut i,
            filename, &mut input, allocated, info,
        );
        loop {
            if Getrow(line.as_mut_ptr(), input, TRUE) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"END\0".as_ptr() as *const c_char).is_null() {
                break;
            }
            if !strstr(line.as_ptr(), b"$NOD\0".as_ptr() as *const c_char).is_null() {
                gmsh1_nodes(
                    data, &mut revindx, &mut noknots, &mut maxindx, dim, &mut i,
                    &mut j, &mut cp, line.as_mut_ptr(), input, allocated,
                );
            }
            if !strstr(line.as_ptr(), b"$ELM\0".as_ptr() as *const c_char).is_null() {
                gmsh1_elems(
                    data, elemind.as_mut_ptr(), &mut elementtype, &mut maxelemtype,
                    &mut noelements, &mut elemnodes, &mut maxnodes, &mut elemno,
                    &mut gmshtype, &mut regphys, &mut regelem, &mut i, &mut j,
                    &mut cp, line.as_mut_ptr(), input, allocated,
                );
            }
        }
        fclose(input);
        if allocated == 0 {
            allocated = TRUE;
            continue 'allocate;
        }
        break;
    }
    gmsh1_renumber(
        data, revindx, noknots, noelements, maxindx, b"Gmsh\0".as_ptr() as *const c_char,
        &mut elementtype, &mut elemnodes, &mut i, &mut j, &mut k,
    );
    ModifyUnsupportedElements(data);
    ElementsToBoundaryConditions(data, bound, FALSE, info);
    printf(b"Successfully read the mesh from the Gmsh input file.\n\0".as_ptr() as *const c_char);
    0
}
// SAFETY: gmsh-2 $MeshFormat section (split for sizing).
unsafe fn gmsh2_meshformat(
    gmshbinary: c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
) {
    let _ = cp;
    Getrow(line, input, FALSE);
    if gmshbinary != 0 {
        // NOTE: fread count unchecked in C (one checked instead).
        let mut one: c_int = 0;
        fread(
            &mut one as *mut c_int as *mut c_void,
            core::mem::size_of::<c_int>(),
            1,
            input,
        );
        if one != 1 {
            printf(b"Gmsh binary file needs to swap bytes, not implemented in ElmerGrid. Exiting!\n\0".as_ptr() as *const c_char);
            bigerror(b"Gmsh binary file needs to swap bytes, not implemented in ElmerGrid. Exiting!\0".as_ptr() as *const c_char);
        }
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndMeshFormat\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$MeshFormat section should end to string $EndMeshFormat:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-2 $Nodes section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_nodes(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: &mut c_int,
    maxindx: &mut c_int,
    dim: c_int,
    nodenum: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
) {
    let _ = dim;
    Getrow(line, input, FALSE);
    *cp = line;
    *noknots = next_int(&mut (*cp));
    *i = 1;
    while *i <= *noknots {
        // NOTE: assigned in arms before every read.
        let mut dataIn3: [c_double; 3] = [0.0; 3];
        if gmshbinary != 0 {
            *frcount = fread(
                &mut (*nodenum) as *mut c_int as *mut c_void,
                core::mem::size_of::<c_int>(),
                1,
                input,
            ) as c_int;
            if *frcount != 1 {
                printf(
                    b"23 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                    *frcount,
                    1,
                );
            }
            *frcount = fread(
                dataIn3.as_mut_ptr() as *mut c_void,
                core::mem::size_of::<c_double>(),
                3,
                input,
            ) as c_int;
            if *frcount != 3 {
                printf(
                    b"24 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                    *frcount,
                    3,
                );
            }
        } else {
            Getrow(line, input, FALSE);
            *cp = line;
            *nodenum = next_int(&mut (*cp));
            dataIn3[0] = next_real(&mut (*cp));
            dataIn3[1] = next_real(&mut (*cp));
            dataIn3[2] = next_real(&mut (*cp));
        }
        if allocated != 0 {
            if *maxindx > *noknots {
                *(*revindx).wrapping_offset(*nodenum as isize) = *i;
            }
            *(*data).x.wrapping_offset(*i as isize) = dataIn3[0];
            *(*data).y.wrapping_offset(*i as isize) = dataIn3[1];
            *(*data).z.wrapping_offset(*i as isize) = dataIn3[2];
        } else {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxindx < *nodenum {
                *maxindx = *nodenum;
            }
        }
        *i += 1;
    }
    if gmshbinary != 0 {
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndNodes\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Nodes section should end to string $EndNodes:\n%s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-2 binary per-element body (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_elem_bin(
    data: *mut FemType,
    elemind: *mut c_int,
    inElems: *mut c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    maxelemtype: &mut c_int,
    notags: &mut c_int,
    usetaggeom: &mut c_int,
    elemno: &mut c_int,
    gmshtype: &mut c_int,
    tagphys: &mut c_int,
    taggeom: &mut c_int,
    tagpart: &mut c_int,
    tagmat: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dataIn4: [c_int; 4];
    // NOTE: assigned before every read (C uninit decl shape).
    let numTags: c_int;
    // NOTE: block-scoped C local becomes a binding.
    let mut inTag: [c_int; 5] = [0; 5];
    dataIn4 = [0; 4];
    *frcount = fread(
        dataIn4.as_mut_ptr() as *mut c_void,
        core::mem::size_of::<c_int>(),
        4,
        input,
    ) as c_int;
    if *frcount != 4 {
        printf(
            b"25 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
            *frcount,
            4,
        );
    }
    *gmshtype = dataIn4[0];
    numTags = dataIn4[2];
    *elemno = dataIn4[3];
    *frcount = fread(
        inTag.as_mut_ptr() as *mut c_void,
        core::mem::size_of::<c_int>(),
        numTags as usize,
        input,
    ) as c_int;
    if *frcount != numTags {
        printf(
            b"26 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
            *frcount,
            numTags,
        );
    }
    *elementtype = GmshToElmerType(*gmshtype);
    *elemnodes = *elementtype % 100;
    *frcount = fread(
        inElems as *mut c_void,
        core::mem::size_of::<c_int>(),
        *elemnodes as usize,
        input,
    ) as c_int;
    if *frcount != *elemnodes {
        printf(
            b"27 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
            *frcount,
            *elemnodes,
        );
    }
    if allocated != 0 {
        *elemnodes = *elementtype % 100;
        *(*data).elementtypes.wrapping_offset(*i as isize) = *elementtype;
        *notags = numTags;
        if *notags > 0 {
            *tagphys = inTag[0];
        }
        if *notags > 1 {
            *taggeom = inTag[1];
        }
        if *notags > 2 {
            *tagpart = inTag[2];
        }
        if *tagphys != 0 {
            *tagmat = *tagphys;
        } else {
            *tagmat = *taggeom;
            *usetaggeom = TRUE;
        }
        *(*data).material.wrapping_offset(*i as isize) = *tagmat;
        *j = 0;
        while *j < *elemnodes {
            *elemind.wrapping_offset(*j as isize) = *inElems.wrapping_offset(*j as isize);
            *j += 1;
        }
        GmshToElmerIndx(*elementtype, elemind);
        *j = 0;
        while *j < *elemnodes {
            *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) =
                *elemind.wrapping_offset(*j as isize);
            *j += 1;
        }
    } else {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxelemtype < *elementtype {
            *maxelemtype = *elementtype;
        }
    }
}
// SAFETY: gmsh-2 ascii per-element body (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_elem_ascii(
    data: *mut FemType,
    elemind: *mut c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    maxelemtype: &mut c_int,
    notags: &mut c_int,
    usetaggeom: &mut c_int,
    elemno: &mut c_int,
    gmshtype: &mut c_int,
    tagphys: &mut c_int,
    taggeom: &mut c_int,
    tagpart: &mut c_int,
    tagmat: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    Getrow(line, input, FALSE);
    *cp = line;
    *elemno = next_int(&mut (*cp));
    *gmshtype = next_int(&mut (*cp));
    *elementtype = GmshToElmerType(*gmshtype);
    if allocated != 0 {
        *elemnodes = *elementtype % 100;
        *(*data).elementtypes.wrapping_offset(*i as isize) = *elementtype;
        *notags = next_int(&mut (*cp));
        if *notags > 0 {
            *tagphys = next_int(&mut (*cp));
        }
        if *notags > 1 {
            *taggeom = next_int(&mut (*cp));
        }
        if *notags > 2 {
            *tagpart = next_int(&mut (*cp));
        }
        *j = 4;
        while *j <= *notags {
            next_int(&mut (*cp));
            *j += 1;
        }
        if *tagphys != 0 {
            *tagmat = *tagphys;
        } else {
            *tagmat = *taggeom;
            *usetaggeom = TRUE;
        }
        *(*data).material.wrapping_offset(*i as isize) = *tagmat;
        *j = 0;
        while *j < *elemnodes {
            *elemind.wrapping_offset(*j as isize) = next_int(&mut (*cp));
            *j += 1;
        }
        GmshToElmerIndx(*elementtype, elemind);
        *j = 0;
        while *j < *elemnodes {
            *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) =
                *elemind.wrapping_offset(*j as isize);
            *j += 1;
        }
    } else {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxelemtype < *elementtype {
            *maxelemtype = *elementtype;
        }
    }
}
// SAFETY: gmsh-2 $Elements section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_elements(
    data: *mut FemType,
    elemind: *mut c_int,
    noelements: &mut c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    maxelemtype: &mut c_int,
    notags: &mut c_int,
    usetaggeom: &mut c_int,
    elemno: &mut c_int,
    gmshtype: &mut c_int,
    tagphys: &mut c_int,
    taggeom: &mut c_int,
    tagpart: &mut c_int,
    tagmat: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
) {
    Getrow(line, input, FALSE);
    *cp = line;
    *noelements = next_int(&mut (*cp));
    // NOTE: block-scoped C local becomes a binding.
    let mut inElems: [c_int; 126] = [0; 126];
    // NOTE: inElems covers the 125-node fourth-order hexahedron; larger
    // elements need the array grown and a recompile.
    if *elemnodes > 125 {
        printf(
            b"error: number of nodes per element of %d, exceeds 125\n\0".as_ptr() as *const c_char,
            *elemnodes,
        );
        printf(b"increase size of inElems and recompile. Exiting!\n\0".as_ptr() as *const c_char);
        bigerror(b"number of nodes per element exceeds 125. Exiting!\0".as_ptr() as *const c_char);
    }
    *i = 1;
    while *i <= *noelements {
        if gmshbinary != 0 {
            gmsh2_elem_bin(
                data, elemind, inElems.as_mut_ptr(), elementtype, elemnodes,
                maxelemtype, notags, usetaggeom, elemno, gmshtype, tagphys, taggeom,
                tagpart, tagmat, frcount, i, j, input, allocated,
            );
        } else {
            gmsh2_elem_ascii(
                data, elemind, elementtype, elemnodes, maxelemtype, notags,
                usetaggeom, elemno, gmshtype, tagphys, taggeom, tagpart, tagmat,
                i, j, cp, line, input, allocated,
            );
        }
        *i += 1;
    }
    if gmshbinary != 0 {
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndElements\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Elements section should end to string $EndElements:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-2 $PhysicalNames section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_physical(
    data: *mut FemType,
    nophysical: &mut c_int,
    physvolexist: &mut c_int,
    physsurfexist: &mut c_int,
    manifoldname: &[[c_char; 10]; 4],
    dim: c_int,
    gmshtype: &mut c_int,
    tagphys: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: section-local C decl becomes a binding.
    let mut entdim: c_int = dim;
    Getrow(line, input, FALSE);
    *cp = line;
    *nophysical = next_int(&mut (*cp));
    *i = 0;
    while *i < *nophysical {
        Getrow(line, input, FALSE);
        if allocated != 0 {
            *cp = line;
            *gmshtype = next_int(&mut (*cp));
            if *gmshtype < entdim - 1 {
                printf(b"Assuming maximum entity dim to be %d\n\0".as_ptr() as *const c_char, dim - 1);
                entdim -= 1;
            }
            *tagphys = next_int(&mut (*cp));
            if *gmshtype == entdim - 1 {
                *physsurfexist = TRUE;
                if *tagphys < MAXBCS as c_int {
                    if (*data).boundaryname[*tagphys as usize].is_null() {
                        (*data).boundaryname[*tagphys as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).boundaryname[*tagphys as usize],
                    );
                } else {
                    printf(
                        b"Index %d too high: ignoring physical %s %s\0".as_ptr() as *const c_char,
                        *tagphys,
                        manifoldname[*gmshtype as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            } else if *gmshtype == entdim {
                *physvolexist = TRUE;
                if *tagphys < MAXBODIES as c_int {
                    if (*data).bodyname[*tagphys as usize].is_null() {
                        (*data).bodyname[*tagphys as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).bodyname[*tagphys as usize],
                    );
                } else {
                    printf(
                        b"Index %d too high: ignoring physical %s %s\0".as_ptr() as *const c_char,
                        *tagphys,
                        manifoldname[*gmshtype as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            } else {
                printf(
                    b"Physical groups of dimension %d not supported in %d-dimensional mesh: ignoring group %d %s\0".as_ptr() as *const c_char,
                    *gmshtype,
                    dim,
                    *tagphys,
                    (*cp).wrapping_offset(1),
                );
            }
        }
        *i += 1;
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndPhysicalNames\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$EndPhysicalNames section should end to string $EndPhysicalNames:\n%s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-2 allocate pass (split for sizing); ports the goto-omstart shape.
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh2_alloc(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxindx: c_int,
    maxelemtype: c_int,
    dim: c_int,
    i: &mut c_int,
    input: *mut FILE,
    allocated: &mut c_int,
    info: c_int,
) {
    if *allocated == 0 {
        let maxnodes: c_int = maxelemtype % 100;
        InitializeKnots(data);
        (*data).dim = dim;
        (*data).maxnodes = maxnodes;
        (*data).noelements = noelements;
        (*data).noknots = noknots;
        if info != 0 {
            printf(
                b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
                noknots,
                noelements,
            );
        }
        AllocateKnots(data);
        if maxindx > noknots {
            *revindx = ivector(1, maxindx);
            *i = 1;
            while *i <= maxindx {
                *(*revindx).wrapping_offset(*i as isize) = 0;
                *i += 1;
            }
        }
        rewind(input);
        *allocated = TRUE;
    }
}

/// Load the grid in Gmsh 2 format (ascii or binary).
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGmshInput2(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    _usetaggeom: c_int,
    keeporphans: c_int,
    gmshbinary: c_int,
    info: c_int,
) -> c_int {
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noknots: c_int = 0;
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_physical before reads).
    let mut nophysical: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxnodes: c_int;
    let dim: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut notags: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_nodes before reads).
    let mut nodenum: c_int = 0;
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut elementtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_nodes before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh1_renumber before reads).
    let mut k: c_int = 0;
    let mut allocated: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindx: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut elemno: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut gmshtype: c_int = 0;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut tagphys: c_int = 0;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut taggeom: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut tagpart: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_elem_* before reads).
    let mut tagmat: c_int = 0;
    // NOTE: dead C local kept read-visible for audit.
    let verno: c_int = 0;
    let _ = verno;
    // NOTE: 0-init for the &mut handoff (set in gmsh2_nodes before reads).
    let mut frcount: c_int = 0;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physvolexist: c_int;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physsurfexist: c_int;
    let manifoldname: [[c_char; 10]; 4] = [
        [
            b'p' as c_char, b'o' as c_char, b'i' as c_char, b'n' as c_char,
            b't' as c_char, 0, 0, 0, 0, 0,
        ],
        [
            b'l' as c_char, b'i' as c_char, b'n' as c_char, b'e' as c_char, 0, 0, 0, 0,
            0, 0,
        ],
        [
            b's' as c_char, b'u' as c_char, b'r' as c_char, b'f' as c_char,
            b'a' as c_char, b'c' as c_char, b'e' as c_char, 0, 0, 0,
        ],
        [
            b'v' as c_char, b'o' as c_char, b'l' as c_char, b'u' as c_char,
            b'm' as c_char, b'e' as c_char, 0, 0, 0, 0,
        ],
    ];
    let input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: C overwrites the parameter with FALSE; bare decl (opacity-safe).
    let mut usetaggeom_local: c_int;
    input = fopen(filename, b"rb\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadGmshInput2: The opening of the mesh file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in Gmsh format 2 from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = (*data).dim;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 0;
    physvolexist = FALSE;
    physsurfexist = FALSE;
    usetaggeom_local = FALSE;
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"$End\0".as_ptr() as *const c_char).is_null() {
                continue;
            }
            if !strstr(line.as_ptr(), b"$MeshFormat\0".as_ptr() as *const c_char).is_null() {
                gmsh2_meshformat(gmshbinary, &mut cp, line.as_mut_ptr(), input);
            } else if !strstr(line.as_ptr(), b"$Nodes\0".as_ptr() as *const c_char).is_null() {
                gmsh2_nodes(
                    data, &mut revindx, &mut noknots, &mut maxindx, dim, &mut nodenum,
                    &mut frcount, &mut i, &mut cp, line.as_mut_ptr(), input, gmshbinary,
                    allocated,
                );
            } else if !strstr(line.as_ptr(), b"$Elements\0".as_ptr() as *const c_char).is_null()
            {
                gmsh2_elements(
                    data, elemind.as_mut_ptr(), &mut noelements, &mut elementtype,
                    &mut elemnodes, &mut maxelemtype, &mut notags, &mut usetaggeom_local,
                    &mut elemno, &mut gmshtype, &mut tagphys, &mut taggeom, &mut tagpart,
                    &mut tagmat, &mut frcount, &mut i, &mut j, &mut cp,
                    line.as_mut_ptr(), input, gmshbinary, allocated,
                );
            } else if !strstr(line.as_ptr(), b"$PhysicalNames\0".as_ptr() as *const c_char)
                .is_null()
            {
                gmsh2_physical(
                    data, &mut nophysical, &mut physvolexist, &mut physsurfexist,
                    &manifoldname, dim, &mut gmshtype, &mut tagphys, &mut i, &mut cp,
                    line.as_mut_ptr(), input, allocated,
                );
            } else if allocated == 0 {
                printf(b"Untreated command: %s\0".as_ptr() as *const c_char, line.as_ptr());
            }
        }
        if allocated != 0 {
            break;
        }
        gmsh2_alloc(
            data, &mut revindx, noknots, noelements, maxindx, maxelemtype, dim, &mut i,
            input, &mut allocated, info,
        );
    }
    let _ = maxnodes;
    gmsh1_renumber(
        data, revindx, noknots, noelements, maxindx, b"Gmsh\0".as_ptr() as *const c_char,
        &mut elementtype, &mut elemnodes, &mut i, &mut j, &mut k,
    );
    ModifyUnsupportedElements(data);
    ElementsToBoundaryConditions(data, bound, keeporphans, info);
    (*data).bodynamesexist = physvolexist;
    (*data).boundarynamesexist = physsurfexist;
    if info != 0 {
        printf(b"Successfully read the mesh from the Gmsh input file.\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: gmsh-4 $MeshFormat section (split for sizing).
unsafe fn gmsh4_meshformat(
    verno: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
) {
    Getrow(line, input, FALSE);
    *cp = line;
    *verno = next_int(&mut (*cp));
    if *verno != 4 {
        printf(
            b"Version number is not compatible with the parser: %d\n\0".as_ptr()
                as *const c_char,
            *verno,
        );
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndMeshFormat\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$MeshFormat section should end to string $EndMeshFormat:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}

// SAFETY: gmsh-4 $Nodes section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh4_nodes(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: &mut c_int,
    maxindx: &mut c_int,
    dim: c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) {
    // NOTE: block-scoped C locals become bindings (parsed, never read).
    let numEntityBlocks: c_int;
    let mut _tagEntity: c_int;
    let mut _dimEntity: c_int;
    let mut _parEntity: c_int;
    let mut numNodes: c_int;
    let mut ind: c_int;
    Getrow(line, input, FALSE);
    *cp = line;
    numEntityBlocks = next_int(&mut (*cp));
    *noknots = next_int(&mut (*cp));
    if allocated != 0 && info != 0 {
        printf(
            b"Reading %d nodes in %d blocks.\n\0".as_ptr() as *const c_char,
            *noknots,
            numEntityBlocks,
        );
    }
    *k = 0;
    *j = 1;
    while *j <= numEntityBlocks {
        Getrow(line, input, FALSE);
        *cp = line;
        _tagEntity = next_int(&mut (*cp));
        _dimEntity = next_int(&mut (*cp));
        _parEntity = next_int(&mut (*cp));
        numNodes = next_int(&mut (*cp));
        *i = 1;
        while *i <= numNodes {
            Getrow(line, input, FALSE);
            *cp = line;
            *k += 1;
            ind = next_int(&mut (*cp));
            if allocated != 0 {
                if *maxindx > *noknots {
                    *(*revindx).wrapping_offset(ind as isize) = *k;
                }
                *(*data).x.wrapping_offset(*k as isize) = next_real(&mut (*cp));
                *(*data).y.wrapping_offset(*k as isize) = next_real(&mut (*cp));
                if dim > 2 {
                    *(*data).z.wrapping_offset(*k as isize) = next_real(&mut (*cp));
                }
            } else {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if *maxindx < ind {
                    *maxindx = ind;
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndNodes\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Nodes section should end to string $EndNodes:\n%s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-4 $Entities section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh4_entities(
    tagmap: &mut *mut *mut c_int,
    tagsize: &mut c_int,
    maxtag: &mut [c_int; 4],
    usetaggeom: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    longline: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let numPoints: c_int;
    let numCurves: c_int;
    let numSurfaces: c_int;
    let numVolumes: c_int;
    let mut numEnt: c_int = 0;
    let mut tag: c_int;
    let mut tagdim: c_int;
    let mut nophys: c_int;
    let mut phystag: c_int;
    // NOTE: dead C locals kept read-visible for audit.
    let nobound: c_int = 0;
    let idum: c_int = 0;
    let mut _rdum: Real = 0.0;
    let _ = (nobound, idum, _rdum);
    *usetaggeom = FALSE;
    Getrow(line, input, FALSE);
    *cp = line;
    numPoints = next_int(&mut (*cp));
    numCurves = next_int(&mut (*cp));
    numSurfaces = next_int(&mut (*cp));
    numVolumes = next_int(&mut (*cp));
    if allocated != 0 {
        *tagsize = 0;
        tagdim = 0;
        while tagdim <= 3 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *tagsize < maxtag[tagdim as usize] {
                *tagsize = maxtag[tagdim as usize];
            }
            tagdim += 1;
        }
        if *tagsize > 0 {
            *tagmap = imatrix(0, 3, 1, *tagsize);
            *i = 0;
            while *i <= 3 {
                *j = 1;
                while *j <= *tagsize {
                    *(*(*tagmap).wrapping_offset(*i as isize)).wrapping_offset(*j as isize) = 0;
                    *j += 1;
                }
                *i += 1;
            }
        }
    }
    tagdim = 0;
    while tagdim <= 3 {
        if tagdim == 0 {
            numEnt = numPoints;
        } else if tagdim == 1 {
            numEnt = numCurves;
        } else if tagdim == 2 {
            numEnt = numSurfaces;
        } else if tagdim == 3 {
            numEnt = numVolumes;
        }
        if allocated == 0 {
            maxtag[tagdim as usize] = 0;
        } else if maxtag[tagdim as usize] > 0 {
            printf(
                b"Tag range for %d %dDIM entities is [%d %d]\n\0".as_ptr() as *const c_char,
                numEnt,
                tagdim,
                0,
                maxtag[tagdim as usize],
            );
        }
        if numEnt > 0 && allocated == 0 {
            printf(b"Reading %d entities in %dD\n\0".as_ptr() as *const c_char, numEnt, tagdim);
        }
        *i = 1;
        while *i <= numEnt {
            fgets(longline, LONGLINESIZE as c_int, input);
            if tagdim == 0 {
                *i += 1;
                continue;
            }
            *cp = longline;
            tag = next_int(&mut (*cp));
            if allocated == 0 {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxtag[tagdim as usize] < tag {
                    maxtag[tagdim as usize] = tag;
                }
            }
            *j = 1;
            while *j <= 6 {
                _rdum = next_real(&mut (*cp));
                *j += 1;
            }
            nophys = next_int(&mut (*cp));
            phystag = 0;
            if nophys > 0 {
                phystag = next_int(&mut (*cp));
            }
            if allocated != 0 {
                *(*(*tagmap).wrapping_offset(tagdim as isize)).wrapping_offset(tag as isize) =
                    phystag;
            }
            // NOTE: entity lines may exceed LONGLINESIZE; consume to newline.
            *j = 0;
            *k = 0;
            loop {
                *l = 0;
                while *l < LONGLINESIZE as c_int {
                    if *longline.wrapping_offset(*l as isize) == b'\n' as c_char {
                        *j = *l + 1;
                        break;
                    }
                    *l += 1;
                }
                if *j != 0 {
                    *j -= 1;
                    break;
                }
                *k += LONGLINESIZE as c_int;
                fgets(longline, LONGLINESIZE as c_int, input);
            }
            if *k > 0 && allocated == 0 {
                printf(
                    b"Entity line %d has length %d.\n\0".as_ptr() as *const c_char,
                    *i,
                    *k + *j,
                );
            }
            *i += 1;
        }
        tagdim += 1;
    }
    fgets(longline, LONGLINESIZE as c_int, input);
    if strstr(longline, b"$EndEntities\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Entities section should end to string $EndEntities:\n%s\n\0".as_ptr()
                as *const c_char,
            longline,
        );
    }
}
// SAFETY: gmsh-4 $Elements section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh4_elements(
    data: *mut FemType,
    elemind: *mut c_int,
    tagmap: *mut *mut c_int,
    tagsize: c_int,
    noelements: &mut c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    maxelemtype: &mut c_int,
    elemno: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let numEntityBlocks: c_int;
    let mut numElements: c_int;
    let mut tagEntity: c_int;
    let mut dimEntity: c_int;
    let mut typeEle: c_int;
    Getrow(line, input, FALSE);
    *cp = line;
    *k = 0;
    numEntityBlocks = next_int(&mut (*cp));
    *noelements = next_int(&mut (*cp));
    if allocated != 0 {
        printf(
            b"Reading %d elements in %d blocks.\n\0".as_ptr() as *const c_char,
            *noelements,
            numEntityBlocks,
        );
    }
    *j = 1;
    while *j <= numEntityBlocks {
        Getrow(line, input, FALSE);
        *cp = line;
        tagEntity = next_int(&mut (*cp));
        dimEntity = next_int(&mut (*cp));
        typeEle = next_int(&mut (*cp));
        numElements = next_int(&mut (*cp));
        *elementtype = GmshToElmerType(typeEle);
        *elemnodes = *elementtype % 100;
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxelemtype < *elementtype {
            *maxelemtype = *elementtype;
        }
        if allocated != 0 && tagsize > 0 {
            printf(
                b"Reading %d elements with tag %d of type %d\n\0".as_ptr() as *const c_char,
                numElements,
                tagEntity,
                *elementtype,
            );
            if tagsize > 0 {
                if *(*tagmap.wrapping_offset(dimEntity as isize)).wrapping_offset(tagEntity as isize)
                    != 0
                {
                    printf(
                        b"  Mapping mesh tag %d to physical tag %d in %dDIM\n\0".as_ptr()
                            as *const c_char,
                        tagEntity,
                        *(*tagmap
                            .wrapping_offset(dimEntity as isize))
                        .wrapping_offset(tagEntity as isize),
                        dimEntity,
                    );
                    tagEntity = *(*tagmap
                        .wrapping_offset(dimEntity as isize))
                    .wrapping_offset(tagEntity as isize);
                } else {
                    printf(
                        b"   Mesh tag %d is not associated to any physical tag!\n\0".as_ptr()
                            as *const c_char,
                        tagEntity,
                    );
                }
            }
        }
        *i = 1;
        while *i <= numElements {
            Getrow(line, input, FALSE);
            *cp = line;
            *k += 1;
            *elemno = next_int(&mut (*cp));
            if allocated != 0 {
                *(*data).elementtypes.wrapping_offset(*k as isize) = *elementtype;
                *(*data).material.wrapping_offset(*k as isize) = tagEntity;
                *l = 0;
                while *l < *elemnodes {
                    *elemind.wrapping_offset(*l as isize) = next_int(&mut (*cp));
                    *l += 1;
                }
                GmshToElmerIndx(*elementtype, elemind);
                *l = 0;
                while *l < *elemnodes {
                    *(*(*data).topology.wrapping_offset(*k as isize))
                        .wrapping_offset(*l as isize) = *elemind.wrapping_offset(*l as isize);
                    *l += 1;
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndElements\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Elements section should end to string $EndElements:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}
// SAFETY: gmsh-4 $PhysicalNames section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh4_physical(
    data: *mut FemType,
    nophysical: &mut c_int,
    physvolexist: &mut c_int,
    physsurfexist: &mut c_int,
    manifoldname: &[[c_char; 10]; 4],
    dim: c_int,
    gmshtype: &mut c_int,
    tagphys: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: section-local C decl becomes a binding.
    let mut entdim: c_int = dim;
    Getrow(line, input, FALSE);
    *cp = line;
    *nophysical = next_int(&mut (*cp));
    *i = 0;
    while *i < *nophysical {
        Getrow(line, input, FALSE);
        if allocated != 0 {
            *cp = line;
            *gmshtype = next_int(&mut (*cp));
            if *gmshtype < entdim - 1 {
                printf(b"Assuming maximum entity dim to be %d\n\0".as_ptr() as *const c_char, dim - 1);
                entdim -= 1;
            }
            *tagphys = next_int(&mut (*cp));
            if *gmshtype == entdim - 1 {
                *physsurfexist = TRUE;
                if *tagphys < MAXBCS as c_int {
                    if (*data).boundaryname[*tagphys as usize].is_null() {
                        (*data).boundaryname[*tagphys as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).boundaryname[*tagphys as usize],
                    );
                    printf(
                        b"Boundary name for physical group %d is: %s\n\0".as_ptr()
                            as *const c_char,
                        *tagphys,
                        (*data).boundaryname[*tagphys as usize],
                    );
                } else {
                    printf(
                        b"Index %d too high: ignoring physical %s %s\0".as_ptr() as *const c_char,
                        *tagphys,
                        manifoldname[*gmshtype as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            } else if *gmshtype == entdim {
                *physvolexist = TRUE;
                if *tagphys < MAXBODIES as c_int {
                    if (*data).bodyname[*tagphys as usize].is_null() {
                        (*data).bodyname[*tagphys as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).bodyname[*tagphys as usize],
                    );
                    printf(
                        b"Body name for physical group %d is: %s\n\0".as_ptr() as *const c_char,
                        *tagphys,
                        (*data).bodyname[*tagphys as usize],
                    );
                } else {
                    printf(
                        b"Index %d too high: ignoring physical %s %s\0".as_ptr() as *const c_char,
                        *tagphys,
                        manifoldname[*gmshtype as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            } else {
                printf(
                    b"Physical groups of dimension %d not supported in %d-dimensional mesh: ignoring group %d %s\0".as_ptr() as *const c_char,
                    *gmshtype,
                    dim,
                    *tagphys,
                    (*cp).wrapping_offset(1),
                );
            }
        }
        *i += 1;
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndPhysicalNames\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$EndPhysicalNames section should end to string $EndPhysicalNames:\n%s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
}

// SAFETY: gmsh-4 skippable data sections (split for sizing).
unsafe fn gmsh4_skip(
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
) {
    if !strstr(line, b"$Periodic\0".as_ptr() as *const c_char).is_null() {
        // NOTE: block-scoped C local becomes a binding.
        let numPeriodicLinks: c_int;
        if allocated != 0 {
            printf(b"Reading periodic links but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        Getrow(line, input, FALSE);
        *cp = line;
        numPeriodicLinks = next_int(&mut (*cp));
        *i = 1;
        while *i <= numPeriodicLinks {
            Getrow(line, input, FALSE);
            *i += 1;
        }
        Getrow(line, input, FALSE);
        if strstr(line, b"$EndPeriodic\0".as_ptr() as *const c_char).is_null() {
            printf(
                b"$Periodic section should end to string $EndPeriodic:\n%s\n\0".as_ptr()
                    as *const c_char,
                line,
            );
        }
    } else if !strstr(line, b"$PartitionedEntities\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading partitioned entities but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndPartitionedEntities\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$NodeData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading node data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndNodeData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$ElementData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading element data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndElementData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$ElementNodeData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading element node data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndElementNodeData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$GhostElements\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading ghost elements data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndGhostElements\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$InterpolationScheme\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading interpolation scheme but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndInterpolationScheme\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if allocated != 0 {
        printf(b"Untreated command: %s\0".as_ptr() as *const c_char, line);
    }
}
/// Load the grid in Gmsh 4 format.
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGmshInput4(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    _usetaggeom: c_int,
    keeporphans: c_int,
    info: c_int,
) -> c_int {
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noknots: c_int = 0;
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_physical before reads).
    let mut nophysical: c_int = 0;
    let maxnodes: c_int;
    let dim: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let notags: c_int = 0;
    let _ = notags;
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in gmsh4_elements before reads).
    let mut elementtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_nodes before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_nodes before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_nodes before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_elements before reads).
    let mut l: c_int = 0;
    let mut allocated: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindx: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_elements before reads).
    let mut elemno: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_physical before reads).
    let mut gmshtype: c_int = 0;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut tagphys: c_int = 0;
    // NOTE: dead C local kept read-visible for audit.
    let tagpart: c_int = 0;
    let _ = tagpart;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_elements before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let tagmat: c_int = 0;
    let _ = tagmat;
    // NOTE: 0-init for the &mut handoff (set in gmsh4_meshformat before reads).
    let mut verno: c_int = 0;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physvolexist: c_int;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physsurfexist: c_int;
    // NOTE: null-init (Imatrix before guarded reads).
    let mut tagmap: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut tagsize: c_int = 0;
    let mut maxtag: [c_int; 4];
    let manifoldname: [[c_char; 10]; 4] = [
        [
            b'p' as c_char, b'o' as c_char, b'i' as c_char, b'n' as c_char,
            b't' as c_char, 0, 0, 0, 0, 0,
        ],
        [
            b'l' as c_char, b'i' as c_char, b'n' as c_char, b'e' as c_char, 0, 0, 0, 0,
            0, 0,
        ],
        [
            b's' as c_char, b'u' as c_char, b'r' as c_char, b'f' as c_char,
            b'a' as c_char, b'c' as c_char, b'e' as c_char, 0, 0, 0,
        ],
        [
            b'v' as c_char, b'o' as c_char, b'l' as c_char, b'u' as c_char,
            b'm' as c_char, b'e' as c_char, 0, 0, 0, 0,
        ],
    ];
    let input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut longline: [c_char; LONGLINESIZE] = [0; LONGLINESIZE];
    // NOTE: C overwrites the parameter with TRUE; bare decl (opacity-safe).
    let mut usetaggeom_local: c_int;
    input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadGmshInput4: The opening of the mesh file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in Gmsh format 4 from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = (*data).dim;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 0;
    physvolexist = FALSE;
    physsurfexist = FALSE;
    usetaggeom_local = TRUE;
    maxtag = [0; 4];
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"$End\0".as_ptr() as *const c_char).is_null() {
                continue;
            }
            if !strstr(line.as_ptr(), b"$MeshFormat\0".as_ptr() as *const c_char).is_null() {
                gmsh4_meshformat(&mut verno, &mut cp, line.as_mut_ptr(), input);
            } else if !strstr(line.as_ptr(), b"$Nodes\0".as_ptr() as *const c_char).is_null() {
                gmsh4_nodes(
                    data, &mut revindx, &mut noknots, &mut maxindx, dim, &mut i,
                    &mut j, &mut k, &mut cp, line.as_mut_ptr(), input, allocated, info,
                );
            } else if !strstr(line.as_ptr(), b"$Entities\0".as_ptr() as *const c_char).is_null()
            {
                gmsh4_entities(
                    &mut tagmap, &mut tagsize, &mut maxtag, &mut usetaggeom_local,
                    &mut i, &mut j, &mut k, &mut l, &mut cp, line.as_mut_ptr(),
                    longline.as_mut_ptr(), input, allocated,
                );
            } else if !strstr(line.as_ptr(), b"$Elements\0".as_ptr() as *const c_char).is_null()
            {
                gmsh4_elements(
                    data, elemind.as_mut_ptr(), tagmap, tagsize, &mut noelements,
                    &mut elementtype, &mut elemnodes, &mut maxelemtype, &mut elemno,
                    &mut i, &mut j, &mut k, &mut l, &mut cp, line.as_mut_ptr(), input,
                    allocated,
                );
            } else if !strstr(line.as_ptr(), b"$PhysicalNames\0".as_ptr() as *const c_char)
                .is_null()
            {
                gmsh4_physical(
                    data, &mut nophysical, &mut physvolexist, &mut physsurfexist,
                    &manifoldname, dim, &mut gmshtype, &mut tagphys, &mut i, &mut cp,
                    line.as_mut_ptr(), input, allocated,
                );
            } else {
                gmsh4_skip(line.as_mut_ptr(), input, allocated, &mut i, &mut cp);
            }
        }
        if allocated != 0 {
            break;
        }
        if noelements == 0 {
            bigerror(b"No elements to load in Gmsh file!\0".as_ptr() as *const c_char);
        }
        if noknots == 0 {
            bigerror(b"No nodes to load in Gmsh file!\0".as_ptr() as *const c_char);
        }
        gmsh2_alloc(
            data, &mut revindx, noknots, noelements, maxindx, maxelemtype, dim, &mut i,
            input, &mut allocated, info,
        );
    }
    let _ = (maxnodes, usetaggeom_local);
    gmsh1_renumber(
        data, revindx, noknots, noelements, maxindx, b"Gmsh\0".as_ptr() as *const c_char,
        &mut elementtype, &mut elemnodes, &mut i, &mut j, &mut k,
    );
    ModifyUnsupportedElements(data);
    ElementsToBoundaryConditions(data, bound, keeporphans, info);
    (*data).bodynamesexist = physvolexist;
    (*data).boundarynamesexist = physsurfexist;
    if tagsize > 0 {
        free_imatrix(tagmap, 0, 3, 1, tagsize);
    }
    if info != 0 {
        printf(b"Successfully read the mesh from the Gmsh input file.\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: gmsh-4.1 $MeshFormat section (split for sizing).
unsafe fn gmsh41_meshformat(
    gmshbinary: c_int,
    frcount: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
) {
    Getrow(line, input, FALSE);
    if gmshbinary != 0 {
        // NOTE: fread count unchecked in C (one checked instead).
        let mut one: c_int = 0;
        *frcount = fread(
            &mut one as *mut c_int as *mut c_void,
            core::mem::size_of::<c_int>(),
            1,
            input,
        ) as c_int;
        if one != 1 {
            printf(b"Gmsh binary file needs to swap bytes, not implemented in ElmerGrid. Exiting!\n\0".as_ptr() as *const c_char);
            bigerror(b"Gmsh binary file needs to swap bytes, not implemented in ElmerGrid. Exiting!\0".as_ptr() as *const c_char);
        }
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndMeshFormat\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$MeshFormat section should end to string $EndMeshFormat:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}

// SAFETY: gmsh-4.1 $Nodes section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_nodes(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: &mut c_int,
    maxindx: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
    info: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let numEntityBlocks: c_int;
    let mut _tagEntity: c_int;
    let mut _dimEntity: c_int;
    let mut parTag: c_int = 0;
    let mut numNodes: c_int;
    let minNodeTag: c_int;
    let maxNodeTag: c_int;
    // NOTE: `=0` decl init kept (tagNode read in ascii arm before any loop).
    let mut tagNode: size_t = 0;
    if gmshbinary != 0 {
        // NOTE: block-scoped C local becomes a binding.
        let mut inputData: [size_t; 4] = [0; 4];
        *frcount = fread(
            inputData.as_mut_ptr() as *mut c_void,
            core::mem::size_of::<size_t>(),
            4,
            input,
        ) as c_int;
        if *frcount != 4 {
            printf(
                b"1 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                4,
            );
        }
        numEntityBlocks = inputData[0] as c_int;
        *noknots = inputData[1] as c_int;
        minNodeTag = inputData[2] as c_int;
        maxNodeTag = inputData[3] as c_int;
    } else {
        Getrow(line, input, FALSE);
        *cp = line;
        numEntityBlocks = next_int(&mut (*cp));
        *noknots = next_int(&mut (*cp));
        minNodeTag = next_int(&mut (*cp));
        maxNodeTag = next_int(&mut (*cp));
    }
    if allocated != 0 && info != 0 {
        printf(
            b"Reading %d nodes in %d blocks.\n\0".as_ptr() as *const c_char,
            *noknots,
            numEntityBlocks,
        );
    }
    *k = 0;
    *j = 1;
    while *j <= numEntityBlocks {
        if gmshbinary != 0 {
            // NOTE: block-scoped C locals become bindings.
            let mut blockData: [c_int; 3] = [0; 3];
            // NOTE: `=0` decl init kept (read before any overwrite).
            let mut numNodesInBlock: size_t = 0;
            *frcount = fread(
                blockData.as_mut_ptr() as *mut c_void,
                core::mem::size_of::<c_int>(),
                3,
                input,
            ) as c_int;
            if *frcount != 3 {
                printf(
                    b"2 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                    *frcount,
                    3,
                );
            }
            _dimEntity = blockData[0];
            _tagEntity = blockData[1];
            parTag = blockData[2];
            *frcount = fread(
                &mut numNodesInBlock as *mut size_t as *mut c_void,
                core::mem::size_of::<size_t>(),
                1,
                input,
            ) as c_int;
            if *frcount != 1 {
                printf(
                    b"3 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                    *frcount,
                    1,
                );
            }
            numNodes = numNodesInBlock as c_int;
        } else {
            Getrow(line, input, FALSE);
            *cp = line;
            _dimEntity = next_int(&mut (*cp));
            _tagEntity = next_int(&mut (*cp));
            parTag = next_int(&mut (*cp));
            numNodes = next_int(&mut (*cp));
        }
        // NOTE: `if(0)` debug print omitted (dead in C).
        *i = 1;
        while *i <= numNodes {
            if gmshbinary != 0 {
                *frcount = fread(
                    &mut tagNode as *mut size_t as *mut c_void,
                    core::mem::size_of::<size_t>(),
                    1,
                    input,
                ) as c_int;
                if *frcount != 1 {
                    printf(
                        b"4 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                            as *const c_char,
                        *frcount,
                        1,
                    );
                }
            } else {
                Getrow(line, input, FALSE);
                *cp = line;
                tagNode = next_int(&mut (*cp)) as size_t;
            }
            // NOTE: `if(0)` debug print omitted (dead in C).
            if allocated != 0 {
                if *maxindx > *noknots {
                    *(*revindx).wrapping_offset(tagNode as isize) = *k + *i;
                }
            } else {
                // NOTE: C MAX macro over (size_t, int) compares in size_t
                // domain (egdef.h:97); the int converts.
                if (*maxindx as size_t) < tagNode {
                    *maxindx = tagNode as c_int;
                }
            }
            *i += 1;
        }
        *i = 1;
        while *i <= numNodes {
            if gmshbinary != 0 {
                // NOTE: block-scoped C local becomes a binding.
                let mut xyz: [c_double; 3] = [0.0; 3];
                *frcount = fread(
                    xyz.as_mut_ptr() as *mut c_void,
                    core::mem::size_of::<c_double>(),
                    3,
                    input,
                ) as c_int;
                if *frcount != 3 {
                    printf(
                        b"5 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                            as *const c_char,
                        *frcount,
                        3,
                    );
                }
                if allocated != 0 {
                    *(*data).x.wrapping_offset((*k + *i) as isize) = xyz[0];
                    *(*data).y.wrapping_offset((*k + *i) as isize) = xyz[1];
                    *(*data).z.wrapping_offset((*k + *i) as isize) = xyz[2];
                }
            } else {
                Getrow(line, input, FALSE);
                *cp = line;
                if allocated != 0 {
                    *(*data).x.wrapping_offset((*k + *i) as isize) = next_real(&mut (*cp));
                    *(*data).y.wrapping_offset((*k + *i) as isize) = next_real(&mut (*cp));
                    *(*data).z.wrapping_offset((*k + *i) as isize) = next_real(&mut (*cp));
                }
            }
            *i += 1;
        }
        *k += numNodes;
        *j += 1;
    }
    if gmshbinary != 0 {
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndNodes\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Nodes section should end to string $EndNodes:\n%s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
    let _ = (parTag, minNodeTag, maxNodeTag);
}
// SAFETY: gmsh-4.1 $Entities counts + tagmap (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_entities_head(
    tagmap: &mut *mut *mut c_int,
    tagsize: &mut c_int,
    maxtag: &mut [c_int; 4],
    meshdim: c_int,
    numPoints: &mut c_int,
    numCurves: &mut c_int,
    numSurfaces: &mut c_int,
    numVolumes: &mut c_int,
    tagdim: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
    info: c_int,
) {
    if gmshbinary != 0 {
        // NOTE: block-scoped C local becomes a binding.
        let mut data: [size_t; 4] = [0; 4];
        *frcount = fread(
            data.as_mut_ptr() as *mut c_void,
            core::mem::size_of::<size_t>(),
            4,
            input,
        ) as c_int;
        if *frcount != 4 {
            printf(
                b"6 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                4,
            );
        }
        *numPoints = data[0] as c_int;
        *numCurves = data[1] as c_int;
        *numSurfaces = data[2] as c_int;
        *numVolumes = data[3] as c_int;
    } else {
        Getrow(line, input, FALSE);
        *cp = line;
        *numPoints = next_int(&mut (*cp));
        *numCurves = next_int(&mut (*cp));
        *numSurfaces = next_int(&mut (*cp));
        *numVolumes = next_int(&mut (*cp));
    }
    if allocated != 0 {
        *tagsize = 0;
        *tagdim = 0;
        while *tagdim <= meshdim {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *tagsize < maxtag[*tagdim as usize] {
                *tagsize = maxtag[*tagdim as usize];
            }
            *tagdim += 1;
        }
        if info != 0 {
            printf(
                b"Allocating lookup table for tags of size %d\n\0".as_ptr() as *const c_char,
                *tagsize,
            );
        }
        if *tagsize > 0 {
            *tagmap = imatrix(0, 3, 1, *tagsize);
            *i = 0;
            while *i <= 3 {
                *j = 1;
                while *j <= *tagsize {
                    *(*(*tagmap).wrapping_offset(*i as isize)).wrapping_offset(*j as isize) = 0;
                    *j += 1;
                }
                *i += 1;
            }
        }
    }
}
// SAFETY: gmsh-4.1 binary per-entity body (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_entity_bin(
    tagmap: &mut *mut *mut c_int,
    _maxtag: &mut [c_int; 4],
    _mintag: &mut [c_int; 4],
    maxreadtag: &mut [c_int; 4],
    minreadtag: &mut [c_int; 4],
    tag: &mut c_int,
    tagdim: c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut phystag: c_int = 0;
    // NOTE: `=0` decl init kept (nophys read before any overwrite).
    let mut nophys: size_t = 0;
    // NOTE: `=0` decl init kept (nobound read before any overwrite).
    let mut nobound: size_t = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut idum: c_int = 0;
    *frcount = fread(
        &mut (*tag) as *mut c_int as *mut c_void,
        core::mem::size_of::<c_int>(),
        1,
        input,
    ) as c_int;
    if *frcount != 1 {
        printf(
            b"7 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
            *frcount,
            1,
        );
    }
    if tagdim == 0 {
        // NOTE: block-scoped C local becomes a binding.
        let mut box3: [c_double; 3] = [0.0; 3];
        *frcount = fread(
            box3.as_mut_ptr() as *mut c_void,
            core::mem::size_of::<c_double>(),
            3,
            input,
        ) as c_int;
        if *frcount != 3 {
            printf(
                b"8 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                3,
            );
        }
        *frcount = fread(
            &mut nophys as *mut size_t as *mut c_void,
            core::mem::size_of::<size_t>(),
            1,
            input,
        ) as c_int;
        if *frcount != 1 {
            printf(
                b"9 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                1,
            );
        }
        if nophys > 0 {
            *frcount = fread(
                &mut phystag as *mut c_int as *mut c_void,
                core::mem::size_of::<c_int>(),
                nophys,
                input,
            ) as c_int;
            if *frcount != nophys as c_int {
                printf(
                    b"9 fread error, frcount = %d, not equal to %lu\n\0".as_ptr()
                        as *const c_char,
                    *frcount,
                    nophys,
                );
            }
        }
    } else {
        // NOTE: block-scoped C local becomes a binding.
        let mut box6: [c_double; 6] = [0.0; 6];
        *frcount = fread(
            box6.as_mut_ptr() as *mut c_void,
            core::mem::size_of::<c_double>(),
            6,
            input,
        ) as c_int;
        if *frcount != 6 {
            printf(
                b"10 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                6,
            );
        }
        *frcount = fread(
            &mut nophys as *mut size_t as *mut c_void,
            core::mem::size_of::<size_t>(),
            1,
            input,
        ) as c_int;
        if *frcount != 1 {
            printf(
                b"11 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                1,
            );
        }
        if nophys > 0 {
            *frcount = fread(
                &mut phystag as *mut c_int as *mut c_void,
                core::mem::size_of::<c_int>(),
                nophys,
                input,
            ) as c_int;
            if *frcount != nophys as c_int {
                printf(
                    b"12 fread error, frcount = %d, not equal to %lu\n\0".as_ptr()
                        as *const c_char,
                    *frcount,
                    nophys,
                );
            }
        }
        *frcount = fread(
            &mut nobound as *mut size_t as *mut c_void,
            core::mem::size_of::<size_t>(),
            1,
            input,
        ) as c_int;
        if *frcount != 1 {
            printf(
                b"13 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                1,
            );
        }
        // NOTE: block-scoped C local becomes a binding.
        let noboundint: c_int = nobound as c_int;
        if noboundint > 0 {
            *k = 0;
            while *k < noboundint {
                *frcount = fread(
                    &mut idum as *mut c_int as *mut c_void,
                    core::mem::size_of::<c_int>(),
                    1,
                    input,
                ) as c_int;
                if *frcount != 1 {
                    printf(
                        b"14 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                            as *const c_char,
                        *frcount,
                        1,
                    );
                }
                *k += 1;
            }
        }
    }
    if nophys > 0 {
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxreadtag[tagdim as usize] < phystag {
                maxreadtag[tagdim as usize] = phystag;
            }
            if minreadtag[tagdim as usize] == -1 {
                minreadtag[tagdim as usize] = phystag;
            } else {
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if phystag < minreadtag[tagdim as usize] {
                    minreadtag[tagdim as usize] = phystag;
                }
            }
        } else {
            *(*(*tagmap).wrapping_offset(tagdim as isize)).wrapping_offset(*tag as isize) =
                phystag;
        }
    }
    let _ = (i, j);
}
// SAFETY: gmsh-4.1 ascii per-entity body (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_entity_ascii(
    tagmap: &mut *mut *mut c_int,
    maxtag: &mut [c_int; 4],
    mintag: &mut [c_int; 4],
    maxreadtag: &mut [c_int; 4],
    minreadtag: &mut [c_int; 4],
    tag: &mut c_int,
    tagdim: c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    longline: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: `=0` decl init kept (nophys read before any overwrite).
    let nophys: size_t;
    // NOTE: assigned before every read (C uninit decl shape).
    let phystag: c_int;
    // NOTE: parsed, never read (bounding-box skip cursor).
    let mut _rdum: Real = 0.0;
    fgets(longline, LONGLINESIZE as c_int, input);
    *cp = longline;
    *tag = next_int(&mut (*cp));
    if allocated == 0 {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxtag[tagdim as usize] < *tag {
            maxtag[tagdim as usize] = *tag;
        }
        if mintag[tagdim as usize] == -1 {
            mintag[tagdim as usize] = *tag;
        } else {
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *tag < mintag[tagdim as usize] {
                mintag[tagdim as usize] = *tag;
            }
        }
    }
    if tagdim == 0 {
        *k = 3;
    } else {
        *k = 6;
    }
    *j = 1;
    while *j <= *k {
        _rdum = next_real(&mut (*cp));
        *j += 1;
    }
    nophys = next_int(&mut (*cp)) as size_t;
    if nophys > 0 {
        // NOTE: `if(0)` debug prints omitted (dead in C).
        phystag = next_int(&mut (*cp));
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxreadtag[tagdim as usize] < phystag {
                maxreadtag[tagdim as usize] = phystag;
            }
            if minreadtag[tagdim as usize] == -1 {
                minreadtag[tagdim as usize] = phystag;
            } else {
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if phystag < minreadtag[tagdim as usize] {
                    minreadtag[tagdim as usize] = phystag;
                }
            }
        } else {
            *(*(*tagmap).wrapping_offset(tagdim as isize)).wrapping_offset(*tag as isize) =
                phystag;
        }
    }
}
// SAFETY: gmsh-4.1 per-dimension entity loop (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_entities_dim(
    tagmap: &mut *mut *mut c_int,
    maxtag: &mut [c_int; 4],
    mintag: &mut [c_int; 4],
    maxreadtag: &mut [c_int; 4],
    minreadtag: &mut [c_int; 4],
    numEnt: c_int,
    tagdim: c_int,
    _meshdim: c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    tag: &mut c_int,
    cp: &mut *mut c_char,
    longline: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
) {
    *i = 1;
    while *i <= numEnt {
        if gmshbinary != 0 {
            gmsh41_entity_bin(
                tagmap, maxtag, mintag, maxreadtag, minreadtag, tag, tagdim, frcount,
                i, j, k, input, allocated,
            );
        } else {
            gmsh41_entity_ascii(
                tagmap, maxtag, mintag, maxreadtag, minreadtag, tag, tagdim, j, k,
                cp, longline, input, allocated,
            );
        }
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxtag[tagdim as usize] < *tag {
                maxtag[tagdim as usize] = *tag;
            }
            if mintag[tagdim as usize] == -1 {
                mintag[tagdim as usize] = *tag;
            } else {
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if *tag < mintag[tagdim as usize] {
                    mintag[tagdim as usize] = *tag;
                }
            }
        }
        if gmshbinary == 0 {
            // NOTE: entity lines may exceed LONGLINESIZE; consume to newline.
            *j = 0;
            *k = 0;
            loop {
                *l = 0;
                while *l < LONGLINESIZE as c_int {
                    if *longline.wrapping_offset(*l as isize) == b'\n' as c_char {
                        // NOTE: j stays positive even when newline is entry 0.
                        *j = *l + 1;
                        break;
                    }
                    *l += 1;
                }
                if *j != 0 {
                    *j -= 1;
                    break;
                }
                *k += LONGLINESIZE as c_int;
                fgets(longline, LONGLINESIZE as c_int, input);
                // NOTE: `if(0)` debug print omitted (dead in C).
                fflush(stdout);
            }
            if *k > 0 && allocated == 0 {
                printf(
                    b"Entity line %d has length %d.\n\0".as_ptr() as *const c_char,
                    *i,
                    *k + *j,
                );
            }
        }
        *i += 1;
    }
}
// SAFETY: gmsh-4.1 $Entities section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_entities(
    tagmap: &mut *mut *mut c_int,
    tagsize: &mut c_int,
    maxtag: &mut [c_int; 4],
    mintag: &mut [c_int; 4],
    maxreadtag: &mut [c_int; 4],
    minreadtag: &mut [c_int; 4],
    maxphystag: &[c_int; 4],
    minphystag: &mut [c_int; 4],
    tagoffset: &mut [c_int; 4],
    phystagoffset: &mut [c_int; 4],
    usetaggeom: &mut c_int,
    meshdim: c_int,
    numPoints: &mut c_int,
    numCurves: &mut c_int,
    numSurfaces: &mut c_int,
    numVolumes: &mut c_int,
    numEnt: &mut c_int,
    tag: &mut c_int,
    tagdim: &mut c_int,
    frcount: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    longline: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
    info: c_int,
) {
    *usetaggeom = FALSE;
    gmsh41_entities_head(
        tagmap, tagsize, maxtag, meshdim, numPoints, numCurves, numSurfaces,
        numVolumes, tagdim, frcount, i, j, cp, line, input, gmshbinary, allocated,
        info,
    );
    *tagdim = 0;
    while *tagdim <= 3 {
        if *tagdim == 0 {
            *numEnt = *numPoints;
        } else if *tagdim == 1 {
            *numEnt = *numCurves;
        } else if *tagdim == 2 {
            *numEnt = *numSurfaces;
        } else if *tagdim == 3 {
            *numEnt = *numVolumes;
        }
        if allocated != 0 && *numEnt > 0 && maxtag[*tagdim as usize] > 0 {
            if mintag[*tagdim as usize] == -1 {
                mintag[*tagdim as usize] = maxtag[*tagdim as usize];
            }
            if minphystag[*tagdim as usize] == -1 {
                minphystag[*tagdim as usize] = maxphystag[*tagdim as usize];
            }
            printf(
                b"Defined %d %dDIM entities with geometric tag range [%d %d]\n\0".as_ptr()
                    as *const c_char,
                *numEnt,
                *tagdim,
                mintag[*tagdim as usize],
                maxtag[*tagdim as usize],
            );
            if maxphystag[*tagdim as usize] > 0 || maxreadtag[*tagdim as usize] > 0 {
                printf(
                    b"   Physical given tag range is [%d %d]\n\0".as_ptr() as *const c_char,
                    minphystag[*tagdim as usize],
                    maxphystag[*tagdim as usize],
                );
                if maxreadtag[*tagdim as usize] != maxphystag[*tagdim as usize]
                    || minreadtag[*tagdim as usize] != minphystag[*tagdim as usize]
                {
                    if maxreadtag[*tagdim as usize] > 0 {
                        printf(
                            b"   Physical read tag range is [%d %d]\n\0".as_ptr()
                                as *const c_char,
                            minreadtag[*tagdim as usize],
                            maxreadtag[*tagdim as usize],
                        );
                    } else {
                        printf(b"   No physical tags read for real!\n\0".as_ptr() as *const c_char);
                    }
                    // NOTE: `if(0)` smallerror omitted (dead in C).
                }
            } else {
                // NOTE: `if(0)` printf omitted (dead in C).
            }
            if *tagdim > meshdim {
                printf(
                    b"We have %d %dDIM tags that are beyond mesh dimension %d!\n\0".as_ptr()
                        as *const c_char,
                    *numEnt,
                    *tagdim,
                    meshdim,
                );
            }
        }
        if *numEnt > 0 && allocated == 0 {
            printf(
                b"Reading %d entities in %dD\n\0".as_ptr() as *const c_char,
                *numEnt,
                *tagdim,
            );
        }
        gmsh41_entities_dim(
            tagmap, maxtag, mintag, maxreadtag, minreadtag, *numEnt, *tagdim, meshdim,
            frcount, i, j, k, l, tag, cp, longline, input, gmshbinary, allocated,
        );
        *tagdim += 1;
    }
    *tagdim = meshdim - 2;
    while *tagdim >= 0 {
        // NOTE: C MAX macro over (0, int) (egdef.h:97).
        let upper: c_int = if 0 > maxphystag[(*tagdim + 1) as usize] {
            0
        } else {
            maxphystag[(*tagdim + 1) as usize]
        };
        phystagoffset[*tagdim as usize] =
            phystagoffset[(*tagdim + 1) as usize] + upper;
        printf(
            b"Physical tag offset for %dD is %d\n\0".as_ptr() as *const c_char,
            *tagdim,
            phystagoffset[*tagdim as usize],
        );
        *tagdim -= 1;
    }
    if meshdim > 0 {
        tagoffset[meshdim as usize] = maxphystag[meshdim as usize];
        tagoffset[(meshdim - 1) as usize] = phystagoffset[0];
    }
    *tagdim = meshdim - 2;
    while *tagdim >= 0 {
        // NOTE: C MAX macro over (0, int) (egdef.h:97).
        let upper: c_int = if 0 > maxtag[(*tagdim + 1) as usize] {
            0
        } else {
            maxtag[(*tagdim + 1) as usize]
        };
        tagoffset[*tagdim as usize] = tagoffset[(*tagdim + 1) as usize] + upper;
        printf(
            b"Geometric tag offset for %dD is %d\n\0".as_ptr() as *const c_char,
            *tagdim,
            tagoffset[*tagdim as usize],
        );
        *tagdim -= 1;
    }
    if gmshbinary != 0 {
        fgets(longline, LONGLINESIZE as c_int, input);
    }
    fgets(longline, LONGLINESIZE as c_int, input);
    if strstr(longline, b"$EndEntities\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Entities section should end to string $EndEntities:\n%s\n\0".as_ptr()
                as *const c_char,
            longline,
        );
    }
}
// SAFETY: gmsh-4.1 $Elements section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_elements(
    data: *mut FemType,
    elemind: *mut c_int,
    tagmap: *mut *mut c_int,
    tagsize: c_int,
    tagoffset: &[c_int; 4],
    phystagoffset: &[c_int; 4],
    noelements: &mut c_int,
    meshdim: &mut c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    maxelemtype: &mut c_int,
    elemno: &mut c_int,
    frcount: &mut c_int,
    _i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    gmshbinary: c_int,
    allocated: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let numEntityBlocks: size_t;
    let minElementTag: size_t;
    let maxElementTag: size_t;
    let mut numElementsInBlock: size_t = 0;
    let mut dimEntity: c_int;
    let mut tagEntity: c_int;
    let mut typeEle: c_int;
    let mut numElements: c_int;
    // NOTE: block-scoped C locals become bindings (assigned in arms).
    let mut elementTag: size_t = 0;
    // NOTE: block-scoped C local becomes a binding.
    let mut nodeTag: [size_t; 126] = [0; 126];
    *k = 0;
    if gmshbinary != 0 {
        // NOTE: block-scoped C local becomes a binding.
        let mut data4: [size_t; 4] = [0; 4];
        // NOTE: block-scoped C local becomes a binding.
        let totalNumElements: size_t;
        *frcount = fread(
            data4.as_mut_ptr() as *mut c_void,
            core::mem::size_of::<size_t>(),
            4,
            input,
        ) as c_int;
        if *frcount != 4 {
            printf(
                b"17 fread error, frcount = %d, not equal to %d\n\0".as_ptr() as *const c_char,
                *frcount,
                4,
            );
        }
        numEntityBlocks = data4[0];
        totalNumElements = data4[1];
        *noelements = totalNumElements as c_int;
        minElementTag = data4[2];
        maxElementTag = data4[3];
    } else {
        Getrow(line, input, FALSE);
        *cp = line;
        numEntityBlocks = next_int(&mut (*cp)) as size_t;
        *noelements = next_int(&mut (*cp));
        minElementTag = next_int(&mut (*cp)) as size_t;
        maxElementTag = next_int(&mut (*cp)) as size_t;
    }
    if allocated != 0 {
        printf(
            b"Reading %d elements in %lu blocks.\n\0".as_ptr() as *const c_char,
            *noelements,
            numEntityBlocks,
        );
    }
    *j = 1;
    while *j <= numEntityBlocks as c_int {
        if gmshbinary != 0 {
            // NOTE: block-scoped C local becomes a binding.
            let mut data3: [c_int; 3] = [0; 3];
            *frcount = fread(
                data3.as_mut_ptr() as *mut c_void,
                core::mem::size_of::<c_int>(),
                3,
                input,
            ) as c_int;
            if *frcount != 3 {
                printf(
                    b"18 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                        as *const c_char,
                    *frcount,
                    3,
                );
            }
            dimEntity = data3[0];
            tagEntity = data3[1];
            typeEle = data3[2];
            *frcount = fread(
                &mut numElementsInBlock as *mut size_t as *mut c_void,
                core::mem::size_of::<size_t>(),
                1,
                input,
            ) as c_int;
            if *frcount != 1 {
                printf(
                    b"19 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                        as *const c_char,
                    *frcount,
                    1,
                );
            }
            numElements = numElementsInBlock as c_int;
        } else {
            Getrow(line, input, FALSE);
            *cp = line;
            dimEntity = next_int(&mut (*cp));
            tagEntity = next_int(&mut (*cp));
            typeEle = next_int(&mut (*cp));
            numElements = next_int(&mut (*cp));
        }
        *elementtype = GmshToElmerType(typeEle);
        if *elementtype == 0 {
            bigerror(
                b"Gmsh element type does not have an Elmer counterpart!\n\0".as_ptr()
                    as *const c_char,
            );
        }
        *elemnodes = *elementtype % 100;
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxelemtype < *elementtype {
            *maxelemtype = *elementtype;
        }
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            let edim: c_int = GetElementDimension(*elementtype);
            if *meshdim < edim {
                *meshdim = edim;
            }
        }
        if allocated != 0 {
            if tagsize > 0 {
                if *(*tagmap.wrapping_offset(dimEntity as isize))
                    .wrapping_offset(tagEntity as isize)
                    != 0
                {
                    printf(
                        b"   Mapping mesh tag %d to physical tag %d in %dDIM\n\0".as_ptr()
                            as *const c_char,
                        tagEntity,
                        *(*tagmap.wrapping_offset(dimEntity as isize))
                            .wrapping_offset(tagEntity as isize),
                        dimEntity,
                    );
                    tagEntity = *(*tagmap.wrapping_offset(dimEntity as isize))
                        .wrapping_offset(tagEntity as isize)
                        + phystagoffset[dimEntity as usize];
                } else {
                    tagEntity += tagoffset[dimEntity as usize];
                }
            } else {
                tagEntity += tagoffset[dimEntity as usize];
            }
        }
        // NOTE: nodeTag covers the 125-node fourth-order hexahedron; larger
        // elements need the array grown and a recompile.
        if *elemnodes > 125 {
            printf(
                b"error: number of nodes per element of %d, exceeds 125\n\0".as_ptr()
                    as *const c_char,
                *elemnodes,
            );
            printf(b"increase size of nodeTag and recompile. Exiting!\n\0".as_ptr() as *const c_char);
            bigerror(b"number of nodes per element exceeds 125. Exiting!\0".as_ptr() as *const c_char);
        }
        *m = 1;
        while *m <= numElements {
            if gmshbinary != 0 {
                *frcount = fread(
                    &mut elementTag as *mut size_t as *mut c_void,
                    core::mem::size_of::<size_t>(),
                    1,
                    input,
                ) as c_int;
                if *frcount != 1 {
                    printf(
                        b"20 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                            as *const c_char,
                        *frcount,
                        1,
                    );
                }
                *elemno = elementTag as c_int;
                *k += 1;
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(*k as isize) = *elementtype;
                    *(*data).material.wrapping_offset(*k as isize) = tagEntity;
                    *frcount = fread(
                        nodeTag.as_mut_ptr() as *mut c_void,
                        core::mem::size_of::<size_t>(),
                        *elemnodes as usize,
                        input,
                    ) as c_int;
                    if *frcount != *elemnodes {
                        printf(
                            b"21 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                                as *const c_char,
                            *frcount,
                            *elemnodes,
                        );
                    }
                    *l = 0;
                    while *l < *elemnodes {
                        *elemind.wrapping_offset(*l as isize) = nodeTag[*l as usize] as c_int;
                        *l += 1;
                    }
                    GmshToElmerIndx(*elementtype, elemind);
                    *l = 0;
                    while *l < *elemnodes {
                        *(*(*data).topology.wrapping_offset(*k as isize))
                            .wrapping_offset(*l as isize) =
                            *elemind.wrapping_offset(*l as isize);
                        *l += 1;
                    }
                } else {
                    *frcount = fread(
                        nodeTag.as_mut_ptr() as *mut c_void,
                        core::mem::size_of::<size_t>(),
                        *elemnodes as usize,
                        input,
                    ) as c_int;
                    if *frcount != *elemnodes {
                        printf(
                            b"22 fread error, frcount = %d, not equal to %d\n\0".as_ptr()
                                as *const c_char,
                            *frcount,
                            *elemnodes,
                        );
                    }
                }
            } else {
                Getrow(line, input, FALSE);
                *cp = line;
                *elemno = next_int(&mut (*cp));
                *k += 1;
                if allocated != 0 {
                    *(*data).elementtypes.wrapping_offset(*k as isize) = *elementtype;
                    *(*data).material.wrapping_offset(*k as isize) = tagEntity;
                    *l = 0;
                    while *l < *elemnodes {
                        *elemind.wrapping_offset(*l as isize) = next_int(&mut (*cp));
                        *l += 1;
                    }
                    GmshToElmerIndx(*elementtype, elemind);
                    *l = 0;
                    while *l < *elemnodes {
                        *(*(*data).topology.wrapping_offset(*k as isize))
                            .wrapping_offset(*l as isize) =
                            *elemind.wrapping_offset(*l as isize);
                        *l += 1;
                    }
                }
            }
            *m += 1;
        }
        *j += 1;
    }
    if gmshbinary != 0 {
        Getrow(line, input, FALSE);
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndElements\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$Elements section should end to string $EndElements:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
    let _ = (minElementTag, maxElementTag);
}
// SAFETY: gmsh-4.1 $PhysicalNames section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_physical(
    data: *mut FemType,
    nophysical: &mut c_int,
    physvolexist: &mut c_int,
    physsurfexist: &mut c_int,
    manifoldname: &[[c_char; 10]; 4],
    phystagoffset: &[c_int; 4],
    meshdim: c_int,
    tagdim: &mut c_int,
    tagphys: &mut c_int,
    maxphystag: &mut [c_int; 4],
    minphystag: &mut [c_int; 4],
    l: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    // NOTE: C `entdim = dim` write-only decl dropped (never read).
    Getrow(line, input, FALSE);
    *cp = line;
    *nophysical = next_int(&mut (*cp));
    *i = 0;
    while *i < *nophysical {
        Getrow(line, input, FALSE);
        *cp = line;
        *tagdim = next_int(&mut (*cp));
        *tagphys = next_int(&mut (*cp));
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxphystag[*tagdim as usize] < *tagphys {
                maxphystag[*tagdim as usize] = *tagphys;
            }
            if minphystag[*tagdim as usize] == -1 {
                minphystag[*tagdim as usize] = *tagphys;
            } else {
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if *tagphys < minphystag[*tagdim as usize] {
                    minphystag[*tagdim as usize] = *tagphys;
                }
            }
        } else if allocated != 0 {
            *l = phystagoffset[*tagdim as usize];
            if *tagdim < meshdim {
                *physsurfexist = TRUE;
                if *tagphys + *l < MAXBCS as c_int {
                    (*data).boundarynamesexist = TRUE;
                    if (*data).boundaryname[(*tagphys + *l) as usize].is_null() {
                        (*data).boundaryname[(*tagphys + *l) as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).boundaryname[(*tagphys + *l) as usize],
                    );
                    printf(
                        b"BC name for physical group %d is: %s\n\0".as_ptr() as *const c_char,
                        *tagphys,
                        (*data).boundaryname[(*tagphys + *l) as usize],
                    );
                } else {
                    printf(
                        b"Index %d too high: ignoring physical %s %s\0".as_ptr()
                            as *const c_char,
                        *tagphys,
                        manifoldname[*tagdim as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            } else if *tagdim == meshdim {
                *physvolexist = TRUE;
                if *tagphys < MAXBODIES as c_int {
                    (*data).bodynamesexist = TRUE;
                    if (*data).bodyname[*tagphys as usize].is_null() {
                        (*data).bodyname[*tagphys as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    sscanf(
                        *cp,
                        b" \"%[^\"]\"\0".as_ptr() as *const c_char,
                        (*data).bodyname[*tagphys as usize],
                    );
                    printf(
                        b"Body name for physical group %d is: %s\n\0".as_ptr() as *const c_char,
                        *tagphys,
                        (*data).bodyname[*tagphys as usize],
                    );
                } else {
                    printf(
                        b"Index %d too large: ignoring physical %s %s\0".as_ptr()
                            as *const c_char,
                        *tagphys,
                        manifoldname[*tagdim as usize].as_ptr(),
                        (*cp).wrapping_offset(1),
                    );
                }
            }
        }
        *i += 1;
    }
    Getrow(line, input, FALSE);
    if strstr(line, b"$EndPhysicalNames\0".as_ptr() as *const c_char).is_null() {
        printf(
            b"$PhysicalNames section should end to string $EndPhysicalNames:\n%s\n\0".as_ptr()
                as *const c_char,
            line,
        );
    }
}

// SAFETY: gmsh-4.1 skippable data sections (split for sizing).
unsafe fn gmsh41_skip(
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
) {
    if !strstr(line, b"$Periodic\0".as_ptr() as *const c_char).is_null() {
        // NOTE: block-scoped C local becomes a binding.
        let mut numPeriodicLinks: c_int;
        if allocated != 0 {
            printf(b"Reading periodic links but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        // NOTE: `if(1)` arm taken verbatim (dead `else` arm dropped).
        numPeriodicLinks = 0;
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndPeriodic\0".as_ptr() as *const c_char).is_null() {
                if allocated != 0 {
                    printf(
                        b"Number of lines for periodic stuff: %d\n\0".as_ptr() as *const c_char,
                        numPeriodicLinks,
                    );
                }
                break;
            }
            numPeriodicLinks += 1;
        }
    } else if !strstr(line, b"$PartitionedEntities\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading partitioned entities but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndPartitionedEntities\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$NodeData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading node data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndNodeData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$ElementData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading element data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndElementData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$ElementNodeData\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading element node data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndElementNodeData\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$GhostElements\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading ghost elements data but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndGhostElements\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if !strstr(line, b"$InterpolationScheme\0".as_ptr() as *const c_char).is_null() {
        if allocated != 0 {
            printf(b"Reading interpolation scheme but doing nothing with them!\n\0".as_ptr() as *const c_char);
        }
        loop {
            Getrow(line, input, FALSE);
            if !strstr(line, b"$EndInterpolationScheme\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
    } else if allocated != 0 {
        printf(b"Untreated command: %s\0".as_ptr() as *const c_char, line);
    }
    let _ = (i, cp);
}

// SAFETY: gmsh-4.1 node renumber with diagnostics (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn gmsh41_renumber(
    data: *mut FemType,
    revindx: *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxindx: c_int,
    elementtype: &mut c_int,
    elemnodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    if maxindx > noknots {
        // NOTE: block-scoped C locals become bindings.
        let mut cnt1: c_int;
        let mut cnt2: c_int;
        let mut cnt3: c_int;
        let mut cnt4: c_int;
        printf(
            b"Renumbering the Gmsh nodes from %d to %d\n\0".as_ptr() as *const c_char,
            maxindx,
            noknots,
        );
        cnt1 = 0;
        cnt2 = 0;
        cnt3 = 0;
        cnt4 = 0;
        *i = 1;
        while *i <= maxindx {
            if *revindx.wrapping_offset(*i as isize) != 0 {
                cnt1 += 1;
            }
            *i += 1;
        }
        printf(
            b"   Number of tagged nodes in reverse index table (out of %d): %d\n\0".as_ptr()
                as *const c_char,
            maxindx,
            cnt1,
        );
        *i = 1;
        while *i <= noelements {
            *elementtype = *(*data).elementtypes.wrapping_offset(*i as isize);
            *elemnodes = *elementtype % 100;
            *j = 0;
            while *j < *elemnodes {
                *k = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                if *k <= 0 || *k > maxindx {
                    if cnt2 < 10 {
                        printf(b"   index out of bounds %d\n\0".as_ptr() as *const c_char, *k);
                    }
                    if cnt2 == 10 {
                        printf(b"  ...\n\0".as_ptr() as *const c_char);
                    }
                    cnt2 += 1;
                } else if *revindx.wrapping_offset(*k as isize) <= 0 {
                    if cnt3 < 10 {
                        printf(
                            b"   node %d (local node %d in element %d of type %d) not found in revindx!\n\0".as_ptr() as *const c_char,
                            *k,
                            *j + 1,
                            *i,
                            *elementtype,
                        );
                    }
                    if cnt3 == 10 {
                        printf(b"  ...\n\0".as_ptr() as *const c_char);
                    }
                    cnt3 += 1;
                } else {
                    cnt4 += 1;
                    *(*(*data).topology.wrapping_offset(*i as isize))
                        .wrapping_offset(*j as isize) =
                        *revindx.wrapping_offset(*k as isize);
                }
                *j += 1;
            }
            *i += 1;
        }
        if cnt2 != 0 {
            printf(
                b"   Number of node indexes out of bounds [0 %d]: %d\n\0".as_ptr()
                    as *const c_char,
                maxindx,
                cnt2,
            );
        }
        if cnt3 != 0 {
            printf(
                b"   Number of node indexes not found in inverse index table: %d\n\0".as_ptr()
                    as *const c_char,
                cnt3,
            );
        }
        if cnt4 != 0 {
            printf(
                b"   Number of node indexes found in inverse index table: %d\n\0".as_ptr()
                    as *const c_char,
                cnt4,
            );
        }
        free_ivector(revindx, 1, maxindx);
    }
}
/// Load the grid in Gmsh 4.1 format (ascii or binary).
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGmshInput41(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    _usetaggeom: c_int,
    keeporphans: c_int,
    gmshbinary: c_int,
    info: c_int,
) -> c_int {
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noknots: c_int = 0;
    // NOTE: assigned in pass 1 before every read; bare decl (opacity-safe).
    let mut noelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_physical before reads).
    let mut nophysical: c_int = 0;
    let maxnodes: c_int;
    let dim: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let notags: c_int = 0;
    let _ = notags;
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in gmsh41_elements before reads).
    let mut elementtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_nodes before reads).
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_nodes before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_nodes before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_elements before reads).
    let mut m: c_int = 0;
    let mut allocated: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindx: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_elements before reads).
    let mut elemno: c_int = 0;
    // NOTE: dead C local kept read-visible for audit.
    let gmshtype: c_int = 0;
    let _ = gmshtype;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_physical before reads).
    let mut tagphys: c_int = 0;
    // NOTE: dead C local kept read-visible for audit.
    let tagpart: c_int = 0;
    let _ = tagpart;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_elements before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let tagmat: c_int = 0;
    let _ = tagmat;
    // NOTE: `=0` decl init kept (verno read before any overwrite).
    let verno: c_int = 0;
    let _ = verno;
    // NOTE: `=0` decl init kept (meshdim read before any overwrite).
    let mut meshdim: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut tagdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_nodes before reads).
    let mut frcount: c_int = 0;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physvolexist: c_int;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut physsurfexist: c_int;
    // NOTE: null-init (Imatrix before guarded reads).
    let mut tagmap: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut tagsize: c_int = 0;
    let mut maxtag: [c_int; 4] = [0; 4];
    let mut mintag: [c_int; 4] = [0; 4];
    let mut maxreadtag: [c_int; 4] = [0; 4];
    let mut minreadtag: [c_int; 4] = [0; 4];
    let mut maxphystag: [c_int; 4] = [0; 4];
    let mut minphystag: [c_int; 4] = [0; 4];
    let mut tagoffset: [c_int; 4] = [0; 4];
    let mut phystagoffset: [c_int; 4] = [0; 4];
    let manifoldname: [[c_char; 10]; 4] = [
        [
            b'p' as c_char, b'o' as c_char, b'i' as c_char, b'n' as c_char,
            b't' as c_char, 0, 0, 0, 0, 0,
        ],
        [
            b'l' as c_char, b'i' as c_char, b'n' as c_char, b'e' as c_char, 0, 0, 0, 0,
            0, 0,
        ],
        [
            b's' as c_char, b'u' as c_char, b'r' as c_char, b'f' as c_char,
            b'a' as c_char, b'c' as c_char, b'e' as c_char, 0, 0, 0,
        ],
        [
            b'v' as c_char, b'o' as c_char, b'l' as c_char, b'u' as c_char,
            b'm' as c_char, b'e' as c_char, 0, 0, 0, 0,
        ],
    ];
    let input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut longline: [c_char; LONGLINESIZE] = [0; LONGLINESIZE];
    // NOTE: C overwrites the parameter with TRUE; bare decl (opacity-safe).
    let mut usetaggeom_local: c_int;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut numPoints: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut numCurves: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut numSurfaces: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut numVolumes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut numEnt: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in gmsh41_entities before reads).
    let mut tag: c_int = 0;
    input = fopen(filename, b"rb\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"The opening of the mesh file %s failed!\n\0".as_ptr() as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in Gmsh format 4.1 from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = (*data).dim;
    meshdim = 0;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 0;
    physvolexist = FALSE;
    physsurfexist = FALSE;
    usetaggeom_local = TRUE;
    i = 0;
    while i < 4 {
        maxtag[i as usize] = -1;
        mintag[i as usize] = -1;
        minreadtag[i as usize] = -1;
        maxreadtag[i as usize] = -1;
        maxphystag[i as usize] = -1;
        minphystag[i as usize] = -1;
        tagoffset[i as usize] = 0;
        phystagoffset[i as usize] = 0;
        i += 1;
    }
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        if allocated != 0 {
            printf(b"Leading element dimension is %d\n\0".as_ptr() as *const c_char, meshdim);
        }
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"$End\0".as_ptr() as *const c_char).is_null() {
                continue;
            }
            if !strstr(line.as_ptr(), b"$MeshFormat\0".as_ptr() as *const c_char).is_null() {
                gmsh41_meshformat(gmshbinary, &mut frcount, line.as_mut_ptr(), input);
            } else if !strstr(line.as_ptr(), b"$Nodes\0".as_ptr() as *const c_char).is_null() {
                gmsh41_nodes(
                    data, &mut revindx, &mut noknots, &mut maxindx, &mut frcount,
                    &mut i, &mut j, &mut k, &mut cp, line.as_mut_ptr(), input,
                    gmshbinary, allocated, info,
                );
            } else if !strstr(line.as_ptr(), b"$Entities\0".as_ptr() as *const c_char).is_null()
            {
                gmsh41_entities(
                    &mut tagmap, &mut tagsize, &mut maxtag, &mut mintag, &mut maxreadtag,
                    &mut minreadtag, &maxphystag, &mut minphystag, &mut tagoffset,
                    &mut phystagoffset, &mut usetaggeom_local, meshdim, &mut numPoints,
                    &mut numCurves, &mut numSurfaces, &mut numVolumes, &mut numEnt,
                    &mut tag, &mut tagdim, &mut frcount, &mut i, &mut j, &mut k,
                    &mut l, &mut cp, line.as_mut_ptr(), longline.as_mut_ptr(), input,
                    gmshbinary, allocated, info,
                );
            } else if !strstr(line.as_ptr(), b"$Elements\0".as_ptr() as *const c_char).is_null()
            {
                gmsh41_elements(
                    data, elemind.as_mut_ptr(), tagmap, tagsize, &tagoffset,
                    &phystagoffset, &mut noelements, &mut meshdim, &mut elementtype,
                    &mut elemnodes, &mut maxelemtype, &mut elemno, &mut frcount,
                    &mut i, &mut j, &mut k, &mut l, &mut m, &mut cp,
                    line.as_mut_ptr(), input, gmshbinary, allocated,
                );
            } else if !strstr(line.as_ptr(), b"$PhysicalNames\0".as_ptr() as *const c_char)
                .is_null()
            {
                gmsh41_physical(
                    data, &mut nophysical, &mut physvolexist, &mut physsurfexist,
                    &manifoldname, &phystagoffset, meshdim, &mut tagdim, &mut tagphys,
                    &mut maxphystag, &mut minphystag, &mut l, &mut i, &mut cp,
                    line.as_mut_ptr(), input, allocated,
                );
            } else {
                gmsh41_skip(line.as_mut_ptr(), input, allocated, &mut i, &mut cp);
            }
        }
        if allocated != 0 {
            break;
        }
        if noelements == 0 {
            bigerror(b"No elements to load in Gmsh file!\0".as_ptr() as *const c_char);
        }
        if noknots == 0 {
            bigerror(b"No nodes to load in Gmsh file!\0".as_ptr() as *const c_char);
        }
        gmsh2_alloc(
            data, &mut revindx, noknots, noelements, maxindx, maxelemtype, dim, &mut i,
            input, &mut allocated, info,
        );
    }
    let _ = (maxnodes, usetaggeom_local, verno);
    gmsh41_renumber(
        data, revindx, noknots, noelements, maxindx, &mut elementtype, &mut elemnodes,
        &mut i, &mut j, &mut k,
    );
    ModifyUnsupportedElements(data);
    ElementsToBoundaryConditions(data, bound, keeporphans, info);
    (*data).bodynamesexist = physvolexist;
    (*data).boundarynamesexist = physsurfexist;
    if tagsize > 0 {
        free_imatrix(tagmap, 0, 3, 1, tagsize);
    }
    if info != 0 {
        printf(
            b"Successfully read the mesh from the Gmsh input file %s\n\0".as_ptr()
                as *const c_char,
            filename,
        );
    }
    0
}
/// Load the grid in a Gmsh format, dispatching on the $MeshFormat version.
///
/// # Safety
/// `data`, `bound` and `prefix` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGmshInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    keeporphans: c_int,
    dim: c_int,
    info: c_int,
) -> c_int {
    let mut input: *mut FILE;
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    // NOTE: `=0` decl init kept (errnum read before any overwrite).
    let mut errnum: c_int = 0;
    // NOTE: `=0` decl init kept; callees take it by value so it stays 0.
    let usetaggeom: c_int = 0;
    // NOTE: assigned in the flatten loops before every read.
    let mut i: c_int;
    // NOTE: keeporphans decides whether lower-order elements without a
    // higher-order entity survive; ElmerGUI dislikes keeping them.
    sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        sprintf(
            filename.as_mut_ptr(),
            b"%s.msh\0".as_ptr() as *const c_char,
            prefix,
        );
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadGmshInput: The opening of the mesh file %s failed!\n\0".as_ptr()
                    as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    Getrow(line.as_mut_ptr(), input, FALSE);
    if info != 0 {
        printf(
            b"Format chosen using the first line: %s\0".as_ptr() as *const c_char,
            line.as_ptr(),
        );
    }
    if !strstr(line.as_ptr(), b"$MeshFormat\0".as_ptr() as *const c_char).is_null() {
        // NOTE: gmsh writes the version as floating point; parse as double
        // so types 1 and 3 detect correctly.
        // NOTE: 0-init for the sscanf handoff (overwritten before reads).
        let mut gmshversion: c_double = 0.0;
        // NOTE: 0-init for the sscanf handoff (overwritten before reads).
        let mut gmshbinary: c_int = 0;
        // NOTE: 0-init for the sscanf handoff (overwritten before reads).
        let mut gmshsize: c_int = 0;
        Getrow(line.as_mut_ptr(), input, FALSE);
        fclose(input);
        if sscanf(
            line.as_ptr(),
            b"%lf %d %d\0".as_ptr() as *const c_char,
            &mut gmshversion,
            &mut gmshbinary,
            &mut gmshsize,
        ) != 3
        {
            printf(
                b"Gmsh input line: %s, expected but didn't receive three items on line. Exiting!\n\0".as_ptr() as *const c_char,
                line.as_ptr(),
            );
            printf(b"Input line: %s\n\0".as_ptr() as *const c_char, line.as_ptr());
            bigerror(b"Gmsh input line didn't receive the expected three items!\0".as_ptr() as *const c_char);
        }
        // NOTE: gmshversion is a float, hence the decimal thresholds.
        if gmshversion < 2.99 {
            if gmshbinary != 0 {
                printf(
                    b"Gmsh input file format is type %5.1f in binary.\n\0".as_ptr()
                        as *const c_char,
                    gmshversion,
                );
            } else {
                printf(
                    b"Gmsh input file format is type %5.1f in ASCII.\n\0".as_ptr()
                        as *const c_char,
                    gmshversion,
                );
            }
            errnum = LoadGmshInput2(
                data,
                bound,
                filename.as_mut_ptr(),
                usetaggeom,
                keeporphans,
                gmshbinary,
                info,
            );
        } else if gmshversion < 3.99 {
            printf(
                b"Gmsh input file of format type %5.1f, is not supported. Exiting!\n\0".as_ptr()
                    as *const c_char,
                gmshversion,
            );
            printf(b"Please use Gmsh 4 versions for output from Gmsh\n\0".as_ptr() as *const c_char);
            bigerror(b"Gmsh input file format is not supported!\0".as_ptr() as *const c_char);
        } else if gmshversion < 4.09 {
            printf(
                b"Gmsh input file format is type %5.1f in ASCII.\n\0".as_ptr()
                    as *const c_char,
                gmshversion,
            );
            errnum = LoadGmshInput4(
                data,
                bound,
                filename.as_mut_ptr(),
                usetaggeom,
                keeporphans,
                info,
            );
        } else if gmshversion < 5.0 {
            if gmshbinary != 0 {
                printf(
                    b"Gmsh input file format is type %5.1f in binary.\n\0".as_ptr()
                        as *const c_char,
                    gmshversion,
                );
            } else {
                printf(
                    b"Gmsh input file format is type %5.1f in ASCII.\n\0".as_ptr()
                        as *const c_char,
                    gmshversion,
                );
            }
            errnum = LoadGmshInput41(
                data,
                bound,
                filename.as_mut_ptr(),
                usetaggeom,
                keeporphans,
                gmshbinary,
                info,
            );
        } else {
            printf(
                b"Gmsh input file of format type %5.1f, is not supported. Exiting!\n\0".as_ptr()
                    as *const c_char,
                gmshversion,
            );
            printf(b"Please use Gmsh 4 versions for output from Gmsh\n\0".as_ptr() as *const c_char);
            bigerror(b"Gmsh input file format is not supported!\0".as_ptr() as *const c_char);
        }
    } else {
        printf(b"*****************************************************\n\0".as_ptr() as *const c_char);
        printf(b"The first line did not start with $MeshFormat, assuming Gmsh 1 format\n\0".as_ptr() as *const c_char);
        printf(b"This version of Gmsh format is no longer supported\n\0".as_ptr() as *const c_char);
        printf(b"Please use Gmsh 4 versions for output from Gmsh\n\0".as_ptr() as *const c_char);
        printf(b"*****************************************************\n\0".as_ptr() as *const c_char);
        errnum = LoadGmshInput1(data, bound, filename.as_mut_ptr(), info);
    }
    if info != 0 {
        if usetaggeom != 0 {
            printf(b"Using geometric numbering of entities\n\0".as_ptr() as *const c_char);
        } else {
            printf(b"Using physical numbering of entities\n\0".as_ptr() as *const c_char);
        }
    }
    if dim < 3 {
        i = 1;
        while i <= (*data).noknots {
            *(*data).z.wrapping_offset(i as isize) = 0.0;
            i += 1;
        }
    }
    if dim < 2 {
        i = 1;
        while i <= (*data).noknots {
            *(*data).y.wrapping_offset(i as isize) = 0.0;
            i += 1;
        }
    }
    errnum
}
// SAFETY: fvcom E3T element line (split for sizing).
unsafe fn fvcom_elem(
    data: *mut FemType,
    noelements: &mut c_int,
    i: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    allocated: c_int,
) {
    *noelements += 1;
    if allocated != 0 {
        *cp = line.wrapping_offset(4);
        *i = next_int(&mut (*cp));
        if *i != *noelements {
            printf(
                b"Invalid element number: %d %d\n\0".as_ptr() as *const c_char,
                *noelements,
                *i,
            );
        }
        *(*data).elementtypes.wrapping_offset(*i as isize) = 303;
        *k = 0;
        while *k < 3 {
            *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*k as isize) =
                next_int(&mut (*cp));
            *k += 1;
        }
        *(*data).material.wrapping_offset(*i as isize) = next_int(&mut (*cp));
    }
}

// SAFETY: fvcom ND node line (split for sizing).
unsafe fn fvcom_node(
    data: *mut FemType,
    noknots: &mut c_int,
    dim: c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    allocated: c_int,
) {
    *noknots += 1;
    if allocated != 0 {
        *cp = line.wrapping_offset(3);
        *i = next_int(&mut (*cp));
        if *i != *noknots {
            printf(
                b"Invalid node number: %d %d\n\0".as_ptr() as *const c_char,
                *noknots,
                *i,
            );
        }
        *(*data).x.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        *(*data).y.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        if dim > 2 {
            *(*data).z.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        }
    }
}

// SAFETY: fvcom NS boundary line (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn fvcom_bc(
    bcinds: &mut *mut c_int,
    bctags: &mut *mut c_int,
    bclines: &mut c_int,
    nbc: &mut c_int,
    nbc0: &mut c_int,
    bccount: &mut c_int,
    bc_id: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    allocated: c_int,
) {
    *bclines += 1;
    if allocated != 0 {
        *cp = line.wrapping_offset(3);
        *i = 0;
        while *i < 10 {
            *j = next_int(&mut (*cp));
            *nbc += 1;
            *(*bcinds).wrapping_offset(*nbc as isize) = abs(*j);
            if *j < 0 {
                *bccount += 1;
                *bc_id = next_int(&mut (*cp));
                *k = *nbc0;
                while *k <= *nbc {
                    *(*bctags).wrapping_offset(*k as isize) = *bc_id;
                    *k += 1;
                }
                *nbc0 = *nbc + 1;
                break;
            }
            *i += 1;
        }
    }
}

// SAFETY: fvcom allocate pass (split for sizing); ports the goto-omstart shape.
#[allow(clippy::too_many_arguments)]
unsafe fn fvcom_alloc(
    data: *mut FemType,
    bcinds: &mut *mut c_int,
    bctags: &mut *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxelemtype: c_int,
    dim: c_int,
    bclines: c_int,
    i: &mut c_int,
    input: *mut FILE,
    allocated: &mut c_int,
    info: c_int,
) {
    if *allocated == 0 {
        let maxnodes: c_int = maxelemtype % 100;
        InitializeKnots(data);
        (*data).dim = dim;
        (*data).maxnodes = maxnodes;
        (*data).noelements = noelements;
        (*data).noknots = noknots;
        if info != 0 {
            printf(
                b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
                noknots,
                noelements,
            );
        }
        AllocateKnots(data);
        printf(b"Number of BC lines: %d\n\0".as_ptr() as *const c_char, bclines);
        *bcinds = ivector(1, 10 * bclines);
        *bctags = ivector(1, 10 * bclines);
        *i = 1;
        while *i <= 10 * bclines {
            *(*bcinds).wrapping_offset(*i as isize) = 0;
            *(*bctags).wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
        rewind(input);
        *allocated = TRUE;
    }
}

/// Load the grid in FVCOM format.
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadFvcomMesh(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: assigned in the pass loop before every read.
    let mut noknots: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut noelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxnodes: c_int;
    let dim: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let _ = elemind;
    // NOTE: dead C local kept read-visible for audit.
    let elementtype: c_int = 0;
    let _ = elementtype;
    // NOTE: 0-init for the &mut handoff (set in fvcom_* before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in fvcom_bc before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in fvcom_elem before reads).
    let mut k: c_int = 0;
    let mut allocated: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let revindx: *mut c_int = core::ptr::null_mut();
    let _ = revindx;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxindx: c_int;
    let _ = maxindx;
    // NOTE: dead C local kept read-visible for audit.
    let elemnodes: c_int = 0;
    let _ = elemnodes;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxelemtype: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let elemtype0: c_int = 0;
    let _ = elemtype0;
    // NOTE: assigned in the pass loop before every read.
    let mut bclines: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let tagmat: c_int = 0;
    let _ = tagmat;
    // NOTE: assigned in the pass loop before every read.
    let mut bccount: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut bcinds: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut bctags: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned in the pass loop before every read.
    let mut nbc: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut nbc0: c_int;
    // NOTE: 0-init for the &mut handoff (set in fvcom_bc before reads).
    let mut bc_id: c_int = 0;
    let input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadFVCOMInput: The opening of the FVCOM mesh file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in FVCOM format from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = 2;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 303;
    // NOTE: C pre-loop `noelements = 0; bclines = 0;` dropped (pass resets).
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        noelements = 0;
        noknots = 0;
        nbc = 0;
        nbc0 = 1;
        bclines = 0;
        bccount = 0;
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if line[0] == 0 {
                break;
            }
            if memcmp(
                line.as_ptr() as *const c_void,
                b"E3T\0".as_ptr() as *const c_void,
                3,
            ) == 0
            {
                fvcom_elem(
                    data, &mut noelements, &mut i, &mut k, &mut cp,
                    line.as_mut_ptr(), allocated,
                );
            } else if memcmp(
                line.as_ptr() as *const c_void,
                b"ND\0".as_ptr() as *const c_void,
                2,
            ) == 0
            {
                fvcom_node(
                    data, &mut noknots, dim, &mut i, &mut cp, line.as_mut_ptr(),
                    allocated,
                );
            } else if memcmp(
                line.as_ptr() as *const c_void,
                b"NS\0".as_ptr() as *const c_void,
                2,
            ) == 0
            {
                fvcom_bc(
                    &mut bcinds, &mut bctags, &mut bclines, &mut nbc, &mut nbc0,
                    &mut bccount, &mut bc_id, &mut i, &mut j, &mut k, &mut cp,
                    line.as_mut_ptr(), allocated,
                );
            } else if memcmp(
                line.as_ptr() as *const c_void,
                b"MESH2D\0".as_ptr() as *const c_void,
                6,
            ) == 0
            {
                if allocated == 0 {
                    printf(b"Yes, we have MESH2D as we should\n\0".as_ptr() as *const c_char);
                }
            } else if memcmp(
                line.as_ptr() as *const c_void,
                b"MESHNAME\0".as_ptr() as *const c_void,
                8,
            ) == 0
            {
                if allocated == 0 {
                    printf(
                        b"Mesh name found but not used: %s\n\0".as_ptr() as *const c_char,
                        line.as_ptr().wrapping_offset(9),
                    );
                }
            }
        }
        if allocated != 0 {
            break;
        }
        fvcom_alloc(
            data, &mut bcinds, &mut bctags, noknots, noelements, maxelemtype, dim,
            bclines, &mut i, input, &mut allocated, info,
        );
    }
    let _ = (maxnodes, maxindx);
    printf(b"Number of different BCs: %d\n\0".as_ptr() as *const c_char, bccount);
    printf(b"Number of BC nodes: %d\n\0".as_ptr() as *const c_char, nbc);
    NodesToBoundaryChain(data, bound, bcinds, bctags, nbc, bccount, info);
    free_ivector(bcinds, 1, 10 * bclines);
    free_ivector(bctags, 1, 10 * bclines);
    if info != 0 {
        printf(b"Successfully read the mesh from the FVCOM file!\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: geo COORDINATES section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn geo_coords(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: &mut c_int,
    maxindx: &mut c_int,
    dim: c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    *i = 0;
    loop {
        Getrow(line, input, FALSE);
        if !strstr(line, b"END_COORDINATES\0".as_ptr() as *const c_char).is_null() {
            break;
        }
        *cp = line;
        *j = next_int(&mut (*cp));
        *i += 1;
        if allocated != 0 {
            if *maxindx > *noknots {
                *(*revindx).wrapping_offset(*j as isize) = *i;
            }
            *(*data).x.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            *(*data).y.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            if dim > 2 {
                *(*data).z.wrapping_offset(*i as isize) = next_real(&mut (*cp));
            }
        } else {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxindx < *j {
                *maxindx = *j;
            }
        }
    }
    *noknots = *i;
}

// SAFETY: geo ELEMENTS section (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn geo_elems(
    data: *mut FemType,
    elemind: *mut c_int,
    noelements: &mut c_int,
    maxelemtype: &mut c_int,
    elementtype: &mut c_int,
    elemtype0: c_int,
    tagmat: &mut c_int,
    elemnodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    *i = 0;
    *elementtype = elemtype0;
    *tagmat = 1;
    loop {
        Getrow(line, input, FALSE);
        if !strstr(line, b"END_ELEMENTS\0".as_ptr() as *const c_char).is_null() {
            break;
        }
        *cp = line;
        *j = next_int(&mut (*cp));
        *i += 1;
        if allocated != 0 {
            *elemnodes = *elementtype % 100;
            *(*data).elementtypes.wrapping_offset(*i as isize) = *elementtype;
            *(*data).material.wrapping_offset(*i as isize) = *tagmat;
            *k = 0;
            while *k < *elemnodes {
                *elemind.wrapping_offset(*k as isize) = next_int(&mut (*cp));
                *k += 1;
            }
            *k = 0;
            while *k < *elemnodes {
                *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*k as isize) = *elemind.wrapping_offset(*k as isize);
                *k += 1;
            }
        } else {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxelemtype < *elementtype {
                *maxelemtype = *elementtype;
            }
        }
    }
    *noelements = *i;
}

// SAFETY: geo allocate pass (split for sizing); ports the goto-omstart shape.
#[allow(clippy::too_many_arguments)]
unsafe fn geo_alloc(
    data: *mut FemType,
    revindx: &mut *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxindx: c_int,
    maxelemtype: c_int,
    dim: c_int,
    i: &mut c_int,
    input: *mut FILE,
    allocated: &mut c_int,
    info: c_int,
) {
    if *allocated == 0 {
        let maxnodes: c_int = maxelemtype % 100;
        InitializeKnots(data);
        (*data).dim = dim;
        (*data).maxnodes = maxnodes;
        (*data).noelements = noelements;
        (*data).noknots = noknots;
        if info != 0 {
            printf(
                b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
                noknots,
                noelements,
            );
        }
        AllocateKnots(data);
        if maxindx > noknots {
            *revindx = ivector(1, maxindx);
            *i = 1;
            while *i <= maxindx {
                *(*revindx).wrapping_offset(*i as isize) = 0;
                *i += 1;
            }
        }
        rewind(input);
        *allocated = TRUE;
    }
}

/// Load the grid in geo format (all-tets only).
///
/// # Safety
/// `data`, `bound` and `filename` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadGeoInput(
    data: *mut FemType,
    bound: *mut BoundaryType,
    filename: *mut c_char,
    info: c_int,
) -> c_int {
    let _ = bound;
    // NOTE: assigned in the pass loop before every read.
    let mut noknots: c_int = 0;
    // NOTE: assigned in the pass loop before every read.
    let mut noelements: c_int = 0;
    let maxnodes: c_int;
    let dim: c_int;
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in geo_elems before reads).
    let mut elementtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in geo_coords before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in geo_coords before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in geo_elems before reads).
    let mut k: c_int = 0;
    let mut allocated: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindx: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxindx: c_int;
    // NOTE: 0-init for the &mut handoff (set in geo_elems before reads).
    let mut elemnodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    // NOTE: `=0` decl init kept (elemtype0 read before any overwrite).
    let mut elemtype0: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in geo_elems before reads).
    let mut tagmat: c_int = 0;
    let input: *mut FILE;
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    input = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        printf(
            b"LoadGeoInput: The opening of the mesh file %s failed!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    if info != 0 {
        printf(
            b"Loading mesh in geo format from file %s\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    allocated = FALSE;
    dim = 3;
    maxnodes = 0;
    maxindx = 0;
    maxelemtype = 0;
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if line[0] == 0 {
                break;
            }
            if !strstr(line.as_ptr(), b"$End\0".as_ptr() as *const c_char).is_null() {
                continue;
            }
            if !strstr(line.as_ptr(), b"TYPES\0".as_ptr() as *const c_char).is_null() {
                if strstr(line.as_ptr(), b"ALL=TET04\0".as_ptr() as *const c_char).is_null() {
                    printf(b"Only all tets implemented at the moment!\n\0".as_ptr() as *const c_char);
                    return 1;
                }
                elemtype0 = 504;
                Getrow(line.as_mut_ptr(), input, FALSE);
            } else if !strstr(line.as_ptr(), b"COORDINATES\0".as_ptr() as *const c_char)
                .is_null()
            {
                geo_coords(
                    data, &mut revindx, &mut noknots, &mut maxindx, dim, &mut i,
                    &mut j, &mut cp, line.as_mut_ptr(), input, allocated,
                );
            } else if !strstr(line.as_ptr(), b"ELEMENTS\0".as_ptr() as *const c_char).is_null()
            {
                geo_elems(
                    data, elemind.as_mut_ptr(), &mut noelements, &mut maxelemtype,
                    &mut elementtype, elemtype0, &mut tagmat, &mut elemnodes, &mut i,
                    &mut j, &mut k, &mut cp, line.as_mut_ptr(), input, allocated,
                );
            } else if !strstr(line.as_ptr(), b"BOUNDARIES\0".as_ptr() as *const c_char)
                .is_null()
            {
                loop {
                    Getrow(line.as_mut_ptr(), input, FALSE);
                    if !strstr(line.as_ptr(), b"END_BOUNDARIES\0".as_ptr() as *const c_char)
                        .is_null()
                    {
                        break;
                    }
                    printf(b"Implement boundaries!\n\0".as_ptr() as *const c_char);
                }
            }
        }
        if allocated != 0 {
            break;
        }
        geo_alloc(
            data, &mut revindx, noknots, noelements, maxindx, maxelemtype, dim, &mut i,
            input, &mut allocated, info,
        );
    }
    let _ = maxnodes;
    gmsh1_renumber(
        data, revindx, noknots, noelements, maxindx, b"Geo\0".as_ptr() as *const c_char,
        &mut elementtype, &mut elemnodes, &mut i, &mut j, &mut k,
    );
    // NOTE: `if(0)` ElementsToBoundaryConditions omitted (dead in C).
    if info != 0 {
        printf(b"Successfully read the mesh from the Geo input file.\n\0".as_ptr() as *const c_char);
    }
    0
}
// SAFETY: universal→elmer type map (pure table).
unsafe fn UnvToElmerType(unvtype: c_int) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let elmertype: c_int;
    // NOTE: C switch over unvtype (multi-case arms grouped).
    if unvtype == 11 || unvtype == 21 {
        elmertype = 202;
    } else if unvtype == 22 || unvtype == 23 || unvtype == 24 {
        elmertype = 203;
    } else if unvtype == 41
        || unvtype == 51
        || unvtype == 61
        || unvtype == 74
        || unvtype == 81
        || unvtype == 91
    {
        elmertype = 303;
    } else if unvtype == 42
        || unvtype == 52
        || unvtype == 62
        || unvtype == 72
        || unvtype == 82
        || unvtype == 92
    {
        elmertype = 306;
    } else if unvtype == 43
        || unvtype == 53
        || unvtype == 63
        || unvtype == 73
        || unvtype == 93
    {
        elmertype = 310;
    } else if unvtype == 44
        || unvtype == 54
        || unvtype == 64
        || unvtype == 71
        || unvtype == 84
        || unvtype == 94
    {
        elmertype = 404;
    } else if unvtype == 45
        || unvtype == 46
        || unvtype == 56
        || unvtype == 66
        || unvtype == 76
        || unvtype == 96
    {
        elmertype = 408;
    } else if unvtype == 111 {
        elmertype = 504;
    } else if unvtype == 118 {
        elmertype = 510;
    } else if unvtype == 101 || unvtype == 112 {
        elmertype = 706;
    } else if unvtype == 102 || unvtype == 113 {
        elmertype = 715;
    } else if unvtype == 104 || unvtype == 115 {
        elmertype = 808;
    } else if unvtype == 105 || unvtype == 116 {
        elmertype = 820;
    } else {
        elmertype = 0;
        // NOTE: `if(0)` printf omitted (dead in C).
    }
    elmertype
}

// SAFETY: universal degenerate-element check (pure scan + report).
unsafe fn UnvRedundantIndexes(nonodes: c_int, ind: *mut c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: `=FALSE` decl init kept (read before any overwrite).
    let mut redundant: c_int = FALSE;
    i = 0;
    while i < nonodes {
        if *ind.wrapping_offset(i as isize) == 0 {
            redundant = TRUE;
        }
        j = i + 1;
        while j < nonodes {
            if *ind.wrapping_offset(i as isize) == *ind.wrapping_offset(j as isize) {
                redundant = TRUE;
            }
            j += 1;
        }
        i += 1;
    }
    if redundant != 0 {
        printf(b"Redundant element %d: \0".as_ptr() as *const c_char, nonodes);
        i = 0;
        while i < nonodes {
            printf(b" %d \0".as_ptr() as *const c_char, *ind.wrapping_offset(i as isize));
            i += 1;
        }
        printf(b"\n\0".as_ptr() as *const c_char);
    }
    redundant
}

// SAFETY: universal node reorder over a valid topology row.
unsafe fn UnvToElmerIndx(elemtype: c_int, topology: *mut c_int) {
    let mut i: c_int;
    let nodes: c_int;
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut reorder: c_int;
    // NOTE: null-init; read only under reorder (set in the matching arm).
    let mut porder: *const c_int = core::ptr::null();
    let order203: [c_int; 3] = [1, 3, 2];
    let order306: [c_int; 6] = [1, 3, 5, 2, 4, 6];
    let order510: [c_int; 10] = [1, 3, 5, 10, 2, 4, 6, 7, 8, 9];
    let order408: [c_int; 8] = [1, 3, 5, 7, 2, 4, 6, 8];
    let order820: [c_int; 20] = [
        1, 3, 5, 7, 13, 15, 17, 19, 2, 4, 6, 8, 9, 10, 11, 12, 14, 16, 18, 20,
    ];
    reorder = FALSE;
    // NOTE: C switch over elemtype.
    if elemtype == 203 {
        reorder = TRUE;
        porder = order203.as_ptr();
    } else if elemtype == 306 {
        reorder = TRUE;
        porder = order306.as_ptr();
    } else if elemtype == 510 {
        reorder = TRUE;
        porder = order510.as_ptr();
    } else if elemtype == 408 {
        reorder = TRUE;
        porder = order408.as_ptr();
    } else if elemtype == 820 {
        reorder = TRUE;
        porder = order820.as_ptr();
    }
    if reorder != 0 {
        nodes = elemtype % 100;
        i = 0;
        while i < nodes {
            oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < nodes {
            // NOTE: universal orders are 1-based (C subtracts one here).
            *topology.wrapping_offset(i as isize) =
                oldtopology[(*porder.wrapping_offset(i as isize) - 1) as usize];
            i += 1;
        }
    }
}
// SAFETY: universal node section 2411/781 (split for sizing); true = nextline.
#[allow(clippy::too_many_arguments)]
unsafe fn unv_nodes(
    data: *mut FemType,
    u2eind: *mut c_int,
    noknots: &mut c_int,
    maxnodeind: &mut c_int,
    reordernodes: &mut c_int,
    nodeind: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) -> bool {
    if allocated != 0 && info != 0 {
        printf(b"Reading node coordinates\n\0".as_ptr() as *const c_char);
    }
    loop {
        GetrowDouble(line, input);
        if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
            if allocated == 0 && info != 0 {
                printf(
                    b"There are %d nodes in the mesh\n\0".as_ptr() as *const c_char,
                    *noknots,
                );
            }
            return true;
        }
        *cp = line;
        *nodeind = next_int(&mut (*cp));
        // NOTE: three fields omitted (two coordinate systems and color).
        *noknots += 1;
        GetrowDouble(line, input);
        if allocated != 0 {
            if *reordernodes != 0 {
                if *u2eind.wrapping_offset(*nodeind as isize) != 0 {
                    printf(
                        b"Reordering node %d already set (%d vs. %d)\n\0".as_ptr()
                            as *const c_char,
                        *nodeind,
                        *u2eind.wrapping_offset(*nodeind as isize),
                        *noknots,
                    );
                } else {
                    *u2eind.wrapping_offset(*nodeind as isize) = *noknots;
                }
            }
            *cp = line;
            *(*data).x.wrapping_offset(*noknots as isize) = next_real(&mut (*cp));
            *(*data).y.wrapping_offset(*noknots as isize) = next_real(&mut (*cp));
            *(*data).z.wrapping_offset(*noknots as isize) = next_real(&mut (*cp));
        } else {
            if *nodeind != *noknots {
                *reordernodes = TRUE;
            }
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxnodeind < *nodeind {
                *maxnodeind = *nodeind;
            }
        }
    }
}

// SAFETY: universal element section 2412 (split for sizing); true = nextline.
#[allow(clippy::too_many_arguments)]
unsafe fn unv_elems2412(
    data: *mut FemType,
    u2eelem: *mut c_int,
    noelements: &mut c_int,
    maxnodes: &mut c_int,
    maxelem: &mut c_int,
    minphys: &mut c_int,
    maxphys: &mut c_int,
    poffset: c_int,
    reorderelements: &mut c_int,
    minelemtype: &mut c_int,
    maxelemtype: &mut c_int,
    elid: &mut c_int,
    unvtype: &mut c_int,
    physind: &mut c_int,
    matind: &mut c_int,
    colorind: &mut c_int,
    nonodes: &mut c_int,
    elmertype: &mut c_int,
    isbeam: &mut c_int,
    lines: &mut c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) -> bool {
    *minelemtype = c_int::MAX;
    *maxelemtype = 0;
    if allocated != 0 && info != 0 {
        printf(b"Reading element topologies\n\0".as_ptr() as *const c_char);
    }
    loop {
        Getrow(line, input, FALSE);
        if !strstr(line, b"-1\0".as_ptr() as *const c_char).is_null() {
            if info != 0 && allocated == 0 {
                printf(
                    b"Element type range in mesh [%d,%d]\n\0".as_ptr() as *const c_char,
                    *minelemtype,
                    *maxelemtype,
                );
            }
            return true;
        }
        *noelements += 1;
        *cp = line;
        *elid = next_int(&mut (*cp));
        *unvtype = next_int(&mut (*cp));
        *physind = next_int(&mut (*cp));
        *matind = next_int(&mut (*cp));
        *colorind = next_int(&mut (*cp));
        *nonodes = next_int(&mut (*cp));
        // NOTE: `if(0)` printf omitted (dead in C).
        *elmertype = UnvToElmerType(*unvtype);
        if *elmertype == 0 {
            printf(
                b"Unknown elementtype %d %d %d %d %d %d %d\n\0".as_ptr() as *const c_char,
                *noelements,
                *elid,
                *unvtype,
                *physind,
                *matind,
                *colorind,
                *nonodes,
            );
            printf(b"line %d: %s\n\0".as_ptr() as *const c_char, linenumber, line);
            bigerror(b"done\0".as_ptr() as *const c_char);
        }
        if allocated == 0 {
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *physind < *minphys {
                *minphys = *physind;
            }
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxphys < *physind {
                *maxphys = *physind;
            }
            if *maxnodes < *nonodes {
                *maxnodes = *nonodes;
            }
            if *elid != *noelements {
                *reorderelements = TRUE;
            }
            if *maxelem < *elid {
                *maxelem = *elid;
            }
        } else {
            *physind += poffset;
        }
        // NOTE: beam elements carry a spare zero row on top.
        *isbeam = (*elmertype / 100 == 2) as c_int;
        if *isbeam != 0 {
            Getrow(line, input, FALSE);
        }
        Getrow(line, input, FALSE);
        *cp = line;
        if *elmertype == 510 {
            *lines = 1;
        } else if *elmertype == 820 {
            *lines = 2;
        } else {
            *lines = 0;
        }
        if allocated != 0 {
            if *reorderelements != 0 {
                *u2eelem.wrapping_offset(*elid as isize) = *noelements;
            }
            // NOTE: `if(debug ...)` report omitted (debug is FALSE).
            if *elmertype % 100 != *nonodes {
                printf(
                    b"nonodes = %d elemtype = %d elid = %d\n\0".as_ptr() as *const c_char,
                    *nonodes,
                    *elmertype,
                    *elid,
                );
                *nonodes = *elmertype % 100;
            }
            *(*data).elementtypes.wrapping_offset(*noelements as isize) = *elmertype;
            *i = 0;
            while *i < *nonodes {
                if *lines > 0 && *i >= 8 && *i % 8 == 0 {
                    Getrow(line, input, FALSE);
                    *cp = line;
                }
                *(*(*data).topology.wrapping_offset(*noelements as isize))
                    .wrapping_offset(*i as isize) = next_int(&mut (*cp));
                *i += 1;
            }
            UnvRedundantIndexes(*nonodes, *(*data).topology.wrapping_offset(*noelements as isize));
            UnvToElmerIndx(*elmertype, *(*data).topology.wrapping_offset(*noelements as isize));
            // NOTE: physical or material property? kept as physical here.
            *(*data).material.wrapping_offset(*noelements as isize) = *physind;
        } else {
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *elmertype < *minelemtype {
                *minelemtype = *elmertype;
            }
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxelemtype < *elmertype {
                *maxelemtype = *elmertype;
            }
            *i = 1;
            while *i <= *lines {
                Getrow(line, input, FALSE);
                *i += 1;
            }
        }
    }
}
// SAFETY: universal coordinate-system section 2420 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unv_coordsys(
    scaling: &mut [Real; 4],
    doscaling: &mut c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let mut partuid: c_int = 0;
    // NOTE: parsed, never read.
    let mut _coordlabel: c_int = 0;
    let mut coordtype: c_int = 0;
    // NOTE: `=0.0` decl init kept (coeff read before any overwrite).
    let mut coeff: Real = 0.0;
    let _ = (partuid, coordtype, coeff);
    if allocated != 0 && info != 0 {
        printf(b"Reading Coordinate system information\n\0".as_ptr() as *const c_char);
    }
    Getrow(line, input, FALSE);
    if allocated == 0 {
        *cp = line;
        partuid = next_int(&mut (*cp));
        printf(b"Part UID = %d\n\0".as_ptr() as *const c_char, partuid);
    }
    Getrow(line, input, FALSE);
    if allocated == 0 {
        sscanf(line, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
        printf(b"Part name = %s\n\0".as_ptr() as *const c_char, entityname.as_ptr());
    }
    Getrow(line, input, FALSE);
    if allocated == 0 {
        *cp = line;
        _coordlabel = next_int(&mut (*cp));
        coordtype = next_int(&mut (*cp));
        if coordtype != 0 {
            printf(b"Coordinate system is not cartesian: %d\n\0".as_ptr() as *const c_char, coordtype);
            printf(b"Code some more if you want to consider this!\n\0".as_ptr() as *const c_char);
        }
    }
    Getrow(line, input, FALSE);
    if allocated == 0 {
        sscanf(line, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
        printf(b"Coord system name = %s\n\0".as_ptr() as *const c_char, entityname.as_ptr());
    }
    *i = 1;
    while *i <= 4 {
        Getrow(line, input, FALSE);
        if allocated == 0 {
            *cp = line;
            if (*cp).is_null() {
                printf(
                    b"Problem reading line %d for coordinate system\n\0".as_ptr()
                        as *const c_char,
                    *i,
                );
            }
            *j = 1;
            while *j <= 3 {
                coeff = next_real(&mut (*cp));
                if *i == *j {
                    scaling[*i as usize] = coeff;
                    if fabs(coeff) < 1.0e-20 {
                        printf(
                            b"Scaling for component %d too small %le\n\0".as_ptr()
                                as *const c_char,
                            *i,
                            coeff,
                        );
                    } else if fabs(coeff - 1.0) != 0.0 {
                        *doscaling = TRUE;
                        printf(
                            b"Scaling component %d by %le\n\0".as_ptr() as *const c_char,
                            *i,
                            coeff,
                        );
                    }
                } else if fabs(coeff) > 1.0e-20 {
                    printf(
                        b"Transformation matrix is not diagonal %d%d: %e\n\0".as_ptr()
                            as *const c_char,
                        *i,
                        *j,
                        coeff,
                    );
                    smallerror(b"Code some more...\0".as_ptr() as *const c_char);
                }
                *j += 1;
            }
        }
        *i += 1;
    }
    Getrow(line, input, FALSE);
    if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) != 0 {
        printf(
            b"Field 2420 should already be ending: %s\n\0".as_ptr() as *const c_char,
            line,
        );
    }
}

// SAFETY: universal element section 780 (split for sizing); true = nextline.
#[allow(clippy::too_many_arguments)]
unsafe fn unv_mode780(
    data: *mut FemType,
    u2eelem: *mut c_int,
    noelements: &mut c_int,
    maxnodes: &mut c_int,
    maxelem: &mut c_int,
    minphys: &mut c_int,
    maxphys: &mut c_int,
    poffset: c_int,
    reorderelements: &mut c_int,
    minelemtype: &mut c_int,
    maxelemtype: &mut c_int,
    elid: &mut c_int,
    unvtype: &mut c_int,
    physind: &mut c_int,
    matind: &mut c_int,
    colorind: &mut c_int,
    nonodes: &mut c_int,
    elmertype: &mut c_int,
    mode: c_int,
    i: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) -> bool {
    // NOTE: block-scoped C locals become bindings (parsed, never read).
    let mut _physind2: c_int = 0;
    let mut _matind2: c_int = 0;
    *maxelemtype = 0;
    *minelemtype = 1000;
    if allocated != 0 && info != 0 {
        printf(
            b"Reading element groups in mode %d\n\0".as_ptr() as *const c_char,
            mode,
        );
    }
    loop {
        Getrow(line, input, FALSE);
        if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
            return true;
        }
        *noelements += 1;
        *cp = line;
        *elid = next_int(&mut (*cp));
        *unvtype = next_int(&mut (*cp));
        *physind = next_int(&mut (*cp));
        _physind2 = next_int(&mut (*cp));
        *matind = next_int(&mut (*cp));
        _matind2 = next_int(&mut (*cp));
        *colorind = next_int(&mut (*cp));
        *nonodes = next_int(&mut (*cp));
        if allocated == 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxnodes < *nonodes {
                *maxnodes = *nonodes;
            }
            if *elid != *noelements {
                *reorderelements = TRUE;
            }
            if *maxelem < *elid {
                *maxelem = *elid;
            }
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *physind < *minphys {
                *minphys = *physind;
            }
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxphys < *physind {
                *maxphys = *physind;
            }
        } else {
            *physind += poffset;
        }
        if *unvtype == 11 || *unvtype == 21 {
            Getrow(line, input, FALSE);
        }
        Getrow(line, input, FALSE);
        *cp = line;
        if allocated != 0 {
            if *reorderelements != 0 {
                *u2eelem.wrapping_offset(*elid as isize) = *noelements;
            }
            *elmertype = UnvToElmerType(*unvtype);
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxelemtype < *elmertype {
                *maxelemtype = *elmertype;
            }
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *elmertype < *minelemtype {
                *minelemtype = *elmertype;
            }
            // NOTE: `if(debug ...)` report omitted (debug is FALSE).
            if *elmertype % 100 != *nonodes {
                printf(
                    b"nonodes = %d elemtype = %d elid = %d\n\0".as_ptr() as *const c_char,
                    *nonodes,
                    *elmertype,
                    *elid,
                );
                *nonodes = *elmertype % 100;
            }
            *(*data).elementtypes.wrapping_offset(*noelements as isize) = *elmertype;
            *i = 0;
            while *i < *nonodes {
                *(*(*data).topology.wrapping_offset(*noelements as isize))
                    .wrapping_offset(*i as isize) = next_int(&mut (*cp));
                *i += 1;
            }
            UnvRedundantIndexes(*nonodes, *(*data).topology.wrapping_offset(*noelements as isize));
            UnvToElmerIndx(*elmertype, *(*data).topology.wrapping_offset(*noelements as isize));
            // NOTE: physical or material property? kept as physical here.
            *(*data).material.wrapping_offset(*noelements as isize) = *physind;
        }
    }
}
// SAFETY: universal group sections 2467/2435 (split for sizing); true = nextline.
#[allow(clippy::too_many_arguments)]
unsafe fn unv_groups(
    data: *mut FemType,
    u2eelem: *mut c_int,
    group: &mut c_int,
    groupset: &mut c_int,
    goffset: c_int,
    mingroup: &mut c_int,
    maxgroup: &mut c_int,
    noconf1: &mut c_int,
    noconf2: &mut c_int,
    noelements: c_int,
    nopoints: &mut c_int,
    nogroup: &mut c_int,
    grouptype: &mut c_int,
    ind: &mut c_int,
    noentities: &mut c_int,
    maxelemtype: &mut c_int,
    minelemtype: &mut c_int,
    elemcode: &mut c_int,
    dummy: &mut c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    reorderelements: c_int,
    mode: c_int,
    i: &mut c_int,
    _j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
    info: c_int,
) -> bool {
    if allocated != 0 && info != 0 {
        printf(
            b"Reading element groups in mode %d\n\0".as_ptr() as *const c_char,
            mode,
        );
    }
    loop {
        Getrow(line, input, FALSE);
        if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
            return true;
        }
        *cp = line;
        *nogroup = next_int(&mut (*cp));
        *maxelemtype = 0;
        *minelemtype = 1000;
        *i = 1;
        while *i <= 6 {
            *dummy = next_int(&mut (*cp));
            *i += 1;
        }
        *noentities = next_int(&mut (*cp));
        if allocated == 0 {
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *nogroup < *mingroup {
                *mingroup = *nogroup;
            }
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxgroup < *nogroup {
                *maxgroup = *nogroup;
            }
        } else {
            *nogroup += goffset;
        }
        Getrow(line, input, FALSE);
        if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
            return true;
        }
        // NOTE: salome empty-group skip stays commented out (dead in C).
        *group += 1;
        *k = 0;
        if allocated != 0 {
            sscanf(line, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
            if (*data).bodyname[*nogroup as usize].is_null() {
                (*data).bodyname[*nogroup as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy((*data).bodyname[*nogroup as usize], entityname.as_ptr());
            (*data).bodynamesexist = TRUE;
            // NOTE: `if(0)` boundarynamesexist omitted (dead in C).
            if info != 0 {
                printf(
                    b"Reading %d:th group with index %d with %d entities: %s\n\0".as_ptr()
                        as *const c_char,
                    *group,
                    *nogroup,
                    *noentities,
                    entityname.as_ptr(),
                );
            }
        }
        if *noentities == 0 {
            Getrow(line, input, FALSE);
        }
        *i = 0;
        while *i < *noentities {
            if *i % 2 == 0 {
                Getrow(line, input, FALSE);
                if strncmp(line, b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
                    return true;
                }
                *cp = line;
            }
            *grouptype = next_int(&mut (*cp));
            *ind = next_int(&mut (*cp));
            *dummy = next_int(&mut (*cp));
            *dummy = next_int(&mut (*cp));
            if *ind == 0 {
                *i += 1;
                continue;
            }
            if *grouptype == 8 {
                if allocated != 0 {
                    if reorderelements != 0 {
                        *ind = *u2eelem.wrapping_offset(*ind as isize);
                    }
                    *elemcode = *(*data).elementtypes.wrapping_offset(*ind as isize);
                    // NOTE: C MAX macro (egdef.h:97); arms pure.
                    if *maxelemtype < *elemcode {
                        *maxelemtype = *elemcode;
                    }
                    // NOTE: C MIN macro (egdef.h:96); arms pure.
                    if *elemcode < *minelemtype {
                        *minelemtype = *elemcode;
                    }
                    if *(*data).material.wrapping_offset(*ind as isize) < 0 {
                        if *(*data).material.wrapping_offset(*ind as isize) == -*nogroup {
                            *noconf1 += 1;
                        } else {
                            *noconf2 += 1;
                        }
                    }
                    *(*data).material.wrapping_offset(*ind as isize) = -*nogroup;
                    *groupset += 1;
                }
            } else if *grouptype == 7 {
                *nopoints += 1;
                if allocated != 0 {
                    *elemcode = 101;
                    if *(*data).material.wrapping_offset((noelements + *nopoints) as isize) < 0
                    {
                        if *(*data).material.wrapping_offset((noelements + *nopoints) as isize)
                            == -*nogroup
                        {
                            *noconf1 += 1;
                        } else {
                            *noconf2 += 1;
                        }
                    }
                    *(*data).material.wrapping_offset((noelements + *nopoints) as isize) =
                        -*nogroup;
                    // NOTE: C MAX macro (egdef.h:97); arms pure.
                    if *maxelemtype < *elemcode {
                        *maxelemtype = *elemcode;
                    }
                    // NOTE: C MIN macro (egdef.h:96); arms pure.
                    if *elemcode < *minelemtype {
                        *minelemtype = *elemcode;
                    }
                    *(*data).elementtypes.wrapping_offset((noelements + *nopoints) as isize) =
                        *elemcode;
                    *(*(*data).topology.wrapping_offset((noelements + *nopoints) as isize))
                        .wrapping_offset(0) = *ind;
                    *groupset += 1;
                }
            } else {
                printf(b"unknown group type %d\n\0".as_ptr() as *const c_char, *grouptype);
            }
            *i += 1;
        }
        if allocated != 0 && info != 0 {
            printf(
                b"Element type range in group is [%d %d]\n\0".as_ptr() as *const c_char,
                *minelemtype,
                *maxelemtype,
            );
        }
    }
}
// SAFETY: universal allocate pass (split for sizing); ports the goto-omstart shape.
#[allow(clippy::too_many_arguments)]
unsafe fn unv_endalloc(
    data: *mut FemType,
    u2eind: &mut *mut c_int,
    u2eelem: &mut *mut c_int,
    noknots: c_int,
    noelements: c_int,
    nopoints: c_int,
    maxnodes: c_int,
    maxnodeind: c_int,
    maxelem: c_int,
    dim: c_int,
    reordernodes: c_int,
    reorderelements: c_int,
    mingroup: &mut c_int,
    maxgroup: &mut c_int,
    minphys: &mut c_int,
    maxphys: &mut c_int,
    goffset: &mut c_int,
    poffset: &mut c_int,
    totknots: &mut c_int,
    i: &mut c_int,
    input: *mut FILE,
    allocated: &mut c_int,
    info: c_int,
) -> bool {
    if *allocated != 0 {
        return false;
    }
    if reordernodes != 0 {
        if info != 0 {
            printf(
                b"Reordering %d nodes with indexes up to %d\n\0".as_ptr() as *const c_char,
                noknots,
                maxnodeind,
            );
        }
        *u2eind = ivector(1, maxnodeind);
        *i = 1;
        while *i <= maxnodeind {
            *(*u2eind).wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
    }
    if reorderelements != 0 {
        if info != 0 {
            printf(
                b"Reordering %d elements with indexes up to %d\n\0".as_ptr()
                    as *const c_char,
                noelements,
                maxelem,
            );
        }
        *u2eelem = ivector(1, maxelem);
        *i = 1;
        while *i <= maxelem {
            *(*u2eelem).wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
    }
    if noknots == 0 || noelements == 0 || maxnodes == 0 {
        printf(
            b"Invalid mesh consists of %d nodes and %d %d-node elements.\n\0".as_ptr()
                as *const c_char,
            noknots,
            noelements,
            maxnodes,
        );
        fclose(input);
        return true;
    }
    rewind(input);
    *totknots = noknots;
    (*data).noknots = noknots;
    (*data).noelements = noelements + nopoints;
    (*data).maxnodes = maxnodes;
    (*data).dim = dim;
    if info != 0 {
        printf(
            b"Allocating mesh with %d nodes and %d %d-node elements in %d dims.\n\0".as_ptr()
                as *const c_char,
            noknots,
            noelements,
            maxnodes,
            dim,
        );
    }
    AllocateKnots(data);
    *allocated = TRUE;
    // NOTE: group/physical offsets keep the two index spaces apart.
    if info != 0 {
        printf(
            b"Physical index interval is [%d,%d]\n\0".as_ptr() as *const c_char,
            *minphys,
            *maxphys,
        );
        if *maxgroup != 0 {
            printf(
                b"Group index interval is [%d,%d]\n\0".as_ptr() as *const c_char,
                *mingroup,
                *maxgroup,
            );
        }
    }
    if *mingroup == 0 {
        printf(b"Applying group offset to 1!\n\0".as_ptr() as *const c_char);
        *goffset = 1;
        *mingroup += 1;
        *maxgroup += 1;
    }
    if *minphys == 0 {
        printf(b"Applying physical entity offset to 1!\n\0".as_ptr() as *const c_char);
        *poffset = 1;
        *minphys += 1;
        *maxphys += 1;
    }
    true
}

// SAFETY: universal group/physical index reconciliation (split for sizing).
unsafe fn unv_physmap(
    data: *mut FemType,
    mingroup: c_int,
    maxgroup: c_int,
    minphys: c_int,
    maxphys: c_int,
    groupset: c_int,
    noelements: c_int,
    i: &mut c_int,
    j: &mut c_int,
    info: c_int,
) {
    if groupset == 0 {
        return;
    }
    // NOTE: block-scoped C locals become bindings.
    let i1: c_int;
    let mut i2: c_int;
    // NOTE: `=0` decl inits kept (k/k2 read before any overwrite).
    let mut k: c_int;
    let mut k2: c_int;
    // NOTE: dead C local kept read-visible for audit.
    let ioffset: c_int = 0;
    let _ = ioffset;
    // NOTE: block-scoped C local becomes a binding.
    let physmap: *mut c_int;
    if info != 0 {
        printf(
            b"Group given for %d elements out of %d\n\0".as_ptr() as *const c_char,
            groupset,
            noelements,
        );
    }
    // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
    if minphys < mingroup {
        i1 = minphys;
    } else {
        i1 = mingroup;
    }
    if maxphys > maxgroup {
        i2 = maxphys;
    } else {
        i2 = maxgroup;
    }
    // NOTE: plenty of room for new indexes.
    i2 = 2 * i2;
    physmap = ivector(i1, i2);
    *i = i1;
    while *i <= i2 {
        *physmap.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    // NOTE: group (negative index) wins over physical.
    *i = 1;
    while *i <= (*data).noelements {
        *j = *(*data).material.wrapping_offset(*i as isize);
        if *j < 0 {
            *physmap.wrapping_offset(-(*j) as isize) = -1;
        }
        *i += 1;
    }
    *i = 1;
    while *i <= (*data).noelements {
        *j = *(*data).material.wrapping_offset(*i as isize);
        if *j > 0 {
            if *physmap.wrapping_offset(*j as isize) == 0 {
                *physmap.wrapping_offset(*j as isize) = -2;
            }
            if *physmap.wrapping_offset(*j as isize) == -1 {
                if info != 0 {
                    printf(
                        b"Physical index and group index collide: %d\n\0".as_ptr()
                            as *const c_char,
                        *j,
                    );
                }
                *physmap.wrapping_offset(*j as isize) = -3;
            }
        }
        *i += 1;
    }
    *i = i1;
    while *i <= i2 {
        if *physmap.wrapping_offset(*i as isize) == -3 {
            // NOTE: take the first free index, tag it used with -4.
            *j = i1;
            while *j <= i2 && *physmap.wrapping_offset(*j as isize) != 0 {
                *j += 1;
            }
            *physmap.wrapping_offset(*i as isize) = *j;
            *physmap.wrapping_offset(*j as isize) = -4;
            if info != 0 {
                printf(
                    b"Replacing physical index %d with free index %d\n\0".as_ptr()
                        as *const c_char,
                    *i,
                    *j,
                );
            }
            // NOTE: drop the name remnant; the physical entity is unnamed.
            if !(*data).bodyname[*j as usize].is_null() {
                printf(
                    b"Removing name of an empty group: %s\n\0".as_ptr() as *const c_char,
                    (*data).bodyname[*j as usize],
                );
                free_cvector((*data).bodyname[*j as usize], 0, MAXNAMESIZE as c_int);
                (*data).bodyname[*j as usize] = core::ptr::null_mut();
            }
        }
        *i += 1;
    }
    k = 0;
    k2 = 0;
    *i = 1;
    while *i <= (*data).noelements {
        *j = *(*data).material.wrapping_offset(*i as isize);
        if *j < 0 {
            *(*data).material.wrapping_offset(*i as isize) = -*j;
        } else if *physmap.wrapping_offset(*j as isize) > 0 {
            *(*data).material.wrapping_offset(*i as isize) =
                *physmap.wrapping_offset(*j as isize);
            k2 += 1;
        } else {
            k += 1;
        }
        *i += 1;
    }
    if k != 0 {
        printf(
            b"Using original physical entity index for %d elements\n\0".as_ptr()
                as *const c_char,
            k,
        );
    }
    if k2 != 0 {
        printf(
            b"Using mapped physical entity index for %d elements\n\0".as_ptr()
                as *const c_char,
            k2,
        );
    }
    *i = i1;
    while *i <= i2 {
        *physmap.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *i = 1;
    while *i <= (*data).noelements {
        (*physmap.wrapping_offset(
            *(*data).material.wrapping_offset(*i as isize) as isize,
        )) += 1;
        *i += 1;
    }
    *i = i1;
    while *i <= i2 {
        *j = *physmap.wrapping_offset(*i as isize);
        if *j != 0 {
            if !(*data).bodyname[*i as usize].is_null() {
                printf(
                    b"Entity %d (%s) count is %d\n\0".as_ptr() as *const c_char,
                    *i,
                    (*data).bodyname[*i as usize],
                    *j,
                );
            } else {
                printf(b"Entity %d count is %d\n\0".as_ptr() as *const c_char, *i, *j);
            }
        }
        *i += 1;
    }
    free_ivector(physmap, i1, i2);
}

// SAFETY: universal post-pass (renumber, scale, boundaries; split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unv_finish(
    data: *mut FemType,
    bound: *mut BoundaryType,
    u2eind: *mut c_int,
    u2eelem: *mut c_int,
    noknots: c_int,
    noelements: c_int,
    maxnodeind: c_int,
    maxelem: c_int,
    reordernodes: c_int,
    reorderelements: c_int,
    doscaling: c_int,
    scaling: &[Real; 4],
    noconf1: c_int,
    noconf2: c_int,
    filename: *const c_char,
    i: &mut c_int,
    j: &mut c_int,
    info: c_int,
) {
    if noconf1 != 0 {
        printf(
            b"Group given multiple times with same group: %d\n\0".as_ptr() as *const c_char,
            noconf1,
        );
    }
    if noconf2 != 0 {
        printf(
            b"Group given multiple times but with different group index: %d\n\0".as_ptr()
                as *const c_char,
            noconf2,
        );
    }
    if reordernodes != 0 {
        printf(b"Reordering nodes continuously\n\0".as_ptr() as *const c_char);
        *j = 1;
        while *j <= noelements {
            *i = 0;
            while *i < *(*data).elementtypes.wrapping_offset(*j as isize) % 100 {
                *(*(*data).topology.wrapping_offset(*j as isize)).wrapping_offset(*i as isize) =
                    *u2eind.wrapping_offset(
                        *(*(*data).topology.wrapping_offset(*j as isize))
                            .wrapping_offset(*i as isize) as isize,
                    );
                *i += 1;
            }
            *j += 1;
        }
        free_ivector(u2eind, 1, maxnodeind);
    }
    if reorderelements != 0 {
        free_ivector(u2eelem, 1, maxelem);
    }
    if doscaling != 0 {
        // NOTE: block-scoped C local becomes a binding.
        let mut coord: *mut Real;
        *j = 1;
        while *j <= 3 {
            if *j == 1 {
                coord = (*data).x;
            } else if *j == 2 {
                coord = (*data).y;
            } else {
                coord = (*data).z;
            }
            if fabs(scaling[*j as usize] - 1.0) >= 1.0e-20 {
                *i = 1;
                while *i <= noknots {
                    *coord.wrapping_offset(*i as isize) *= scaling[*j as usize];
                    *i += 1;
                }
            }
            *j += 1;
        }
    }
    // NOTE: `if(FALSE)` nodal-order dump omitted (dead in C).
    // NOTE: bulk elements become boundary elements here.
    ElementsToBoundaryConditions(data, bound, TRUE, info);
    if info != 0 {
        printf(
            b"The Universal mesh was loaded from file %s.\n\0".as_ptr() as *const c_char,
            filename,
        );
    }
    let _ = noknots;
}
/// Load the grid in universal file format (common FE fields only).
///
/// # Safety
/// `data`, `bound` and `prefix` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadUniversalMesh(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: assigned in the pass loop before every read.
    let mut noknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totknots: c_int = 0;
    // NOTE: assigned in the pass loop before every read.
    let mut noelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemcode: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnodes: c_int;
    let mut allocated: c_int;
    let dim: c_int;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut ind: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut lines: c_int = 0;
    // NOTE: assigned in the pass loop before every read.
    let mut groupset: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut goffset: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut poffset: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut noconf1: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut noconf2: c_int;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut reordernodes: c_int;
    // NOTE: assigned before every read (C `=FALSE` decl init shape).
    let mut reorderelements: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut nogroups: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnodeind: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelem: c_int;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut elid: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut unvtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut elmertype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut group: c_int;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut grouptype: c_int = 0;
    // NOTE: assigned in the pass loop before every read.
    let mut mode: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut nopoints: c_int;
    // NOTE: 0-init for the &mut handoff (set in unv_nodes before reads).
    let mut nodeind: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut matind: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut physind: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut colorind: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int = 0;
    // NOTE: `=FALSE` decl init kept (read before any overwrite).
    let mut doscaling: c_int = FALSE;
    // NOTE: always FALSE; the `if(debug)` vector block is dead in C.
    let debug: c_int = FALSE;
    let _ = debug;
    // NOTE: assigned before every read (C `=INT_MAX` decl init shape).
    let mut mingroup: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut maxgroup: c_int;
    // NOTE: assigned before every read (C `=INT_MAX` decl init shape).
    let mut minphys: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut maxphys: c_int;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut nogroup: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut noentities: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut dummy: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_elems2412 before reads).
    let mut isbeam: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut u2eind: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut u2eelem: *mut c_int = core::ptr::null_mut();
    // NOTE: debug-only vector dropped (`if(debug)` is dead: debug=FALSE).
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_* before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in unv_groups before reads).
    let mut k: c_int = 0;
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut scaling: [Real; 4] = [0.0; 4];
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(prefix, filename.as_mut_ptr(), b"unv\0".as_ptr() as *const c_char);
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadUniversalMesh: opening of the universal mesh file '%s' wasn't successful !\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(
        b"Reading mesh from universal mesh file %s.\n\0".as_ptr() as *const c_char,
        filename.as_ptr(),
    );
    InitializeKnots(data);
    dim = 3;
    allocated = FALSE;
    reordernodes = FALSE;
    reorderelements = FALSE;
    // NOTE: C `debug = FALSE` dropped (decl init holds; never read).
    maxnodeind = 0;
    maxnodes = 0;
    maxelem = 0;
    mingroup = c_int::MAX;
    maxgroup = 0;
    minphys = c_int::MAX;
    maxphys = 0;
    groupset = 0;
    goffset = 0;
    poffset = 0;
    noconf1 = 0;
    noconf2 = 0;
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        // NOTE: module-global line counter reset per pass.
        linenumber = 0;
        if info != 0 {
            if allocated != 0 {
                printf(b"Second round for reading data\n\0".as_ptr() as *const c_char);
            } else {
                printf(b"First round for allocating data\n\0".as_ptr() as *const c_char);
            }
        }
        noknots = 0;
        noelements = 0;
        nogroups = 0;
        nopoints = 0;
        group = 0;
        mode = 0;
        // NOTE: C `for(;;)` + `nextline:` label; helpers return true for nextline.
        // NOTE: `if(0)` line dump omitted (dead in C).
        'readloop: loop {
            if strncmp(line.as_ptr(), b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 0;
            }
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if line[0] == 0 {
                break;
            }
            if strncmp(line.as_ptr(), b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 0;
            } else if strncmp(line.as_ptr(), b"  2411\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 2411;
            } else if strncmp(line.as_ptr(), b"  2412\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 2412;
            } else if strncmp(line.as_ptr(), b"  2420\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 2420;
            } else if strncmp(line.as_ptr(), b"  2467\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 2467;
            } else if strncmp(line.as_ptr(), b"  2435\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 2435;
            } else if strncmp(line.as_ptr(), b"   781\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 781;
            } else if strncmp(line.as_ptr(), b"   780\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 780;
            } else if strncmp(line.as_ptr(), b"   164\0".as_ptr() as *const c_char, 6) == 0 {
                mode = 164;
            } else if allocated != 0 && strncmp(line.as_ptr(), b"      \0".as_ptr() as *const c_char, 6) != 0 {
                printf(
                    b"Unknown mode line %d: %s\0".as_ptr() as *const c_char,
                    linenumber,
                    line.as_ptr(),
                );
            }
            // NOTE: `if(debug && mode)` report omitted (debug is FALSE).
            if mode == 2411 || mode == 781 {
                if unv_nodes(
                    data, u2eind, &mut noknots, &mut maxnodeind, &mut reordernodes,
                    &mut nodeind, &mut cp, line.as_mut_ptr(), input, allocated, info,
                ) {
                    continue 'readloop;
                }
            }
            if mode == 2412 {
                // NOTE: the "whole mesh" range print after the section is
                // unreachable in C (the section exits via goto nextline).
                if unv_elems2412(
                    data, u2eelem, &mut noelements, &mut maxnodes, &mut maxelem,
                    &mut minphys, &mut maxphys, poffset, &mut reorderelements,
                    &mut minelemtype, &mut maxelemtype, &mut elid, &mut unvtype,
                    &mut physind, &mut matind, &mut colorind, &mut nonodes,
                    &mut elmertype, &mut isbeam, &mut lines, &mut i, &mut cp,
                    line.as_mut_ptr(), input, allocated, info,
                ) {
                    continue 'readloop;
                }
            }
            if mode == 2420 {
                unv_coordsys(
                    &mut scaling, &mut doscaling, &mut entityname, &mut i, &mut j,
                    &mut cp, line.as_mut_ptr(), input, allocated, info,
                );
                continue 'readloop;
            }
            if mode == 780 {
                if unv_mode780(
                    data, u2eelem, &mut noelements, &mut maxnodes, &mut maxelem,
                    &mut minphys, &mut maxphys, poffset, &mut reorderelements,
                    &mut minelemtype, &mut maxelemtype, &mut elid, &mut unvtype,
                    &mut physind, &mut matind, &mut colorind, &mut nonodes,
                    &mut elmertype, mode, &mut i, &mut cp, line.as_mut_ptr(), input,
                    allocated, info,
                ) {
                    continue 'readloop;
                }
            }
            if mode == 2467 || mode == 2435 {
                if unv_groups(
                    data, u2eelem, &mut group, &mut groupset, goffset, &mut mingroup,
                    &mut maxgroup, &mut noconf1, &mut noconf2, noelements,
                    &mut nopoints, &mut nogroup, &mut grouptype, &mut ind,
                    &mut noentities, &mut maxelemtype, &mut minelemtype, &mut elemcode,
                    &mut dummy, &mut entityname, reorderelements, mode, &mut i,
                    &mut j, &mut k, &mut cp, line.as_mut_ptr(), input, allocated,
                    info,
                ) {
                    continue 'readloop;
                }
            }
            if mode == 164 {
                if allocated == 0 {
                    printf(b"Units dataset content is currently omitted!\n\0".as_ptr() as *const c_char);
                }
                loop {
                    Getrow(line.as_mut_ptr(), input, FALSE);
                    if strncmp(line.as_ptr(), b"    -1\0".as_ptr() as *const c_char, 6) == 0 {
                        continue 'readloop;
                    }
                }
            }
        }
        // NOTE: C `exit;` (bare function-pointer statement) is a no-op.
        // NOTE: `if(0)` done-reading print omitted (dead in C).
        if unv_endalloc(
            data, &mut u2eind, &mut u2eelem, noknots, noelements, nopoints, maxnodes,
            maxnodeind, maxelem, dim, reordernodes, reorderelements, &mut mingroup,
            &mut maxgroup, &mut minphys, &mut maxphys, &mut goffset, &mut poffset,
            &mut totknots, &mut i, input, &mut allocated, info,
        ) {
            if allocated == 0 {
                return 2;
            }
            continue;
        } else if allocated != 0 {
            break;
        }
    }
    fclose(input);
    unv_physmap(
        data, mingroup, maxgroup, minphys, maxphys, groupset, noelements, &mut i,
        &mut j, info,
    );
    unv_finish(
        data, bound, u2eind, u2eelem, noknots, noelements, maxnodeind, maxelem,
        reordernodes, reorderelements, doscaling, &scaling, noconf1, noconf2,
        filename.as_ptr(), &mut i, &mut j, info,
    );
    let _ = (nogroups, totknots);
    0
}
// SAFETY: cgsim ZONE knots block (split for sizing).
unsafe fn cgsim_knots(
    data: *mut FemType,
    noknots: c_int,
    thisknots: &mut c_int,
    savedofs: c_int,
    dummyreal: &mut Real,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    *i = 1;
    while *i <= *thisknots {
        Getrow(line, input, FALSE);
        if allocated != 0 {
            *cp = line;
            *(*data).x.wrapping_offset((noknots + *i) as isize) = next_real(&mut (*cp));
            *(*data).y.wrapping_offset((noknots + *i) as isize) = next_real(&mut (*cp));
            *(*data).z.wrapping_offset((noknots + *i) as isize) = 0.0;
            if savedofs == 1 {
                *j = 1;
                while *j <= 4 {
                    *dummyreal = next_real(&mut (*cp));
                    *j += 1;
                }
                *(*data).dofs[1].wrapping_offset((noknots + *i) as isize) =
                    next_real(&mut (*cp));
            } else if savedofs == 5 {
                *j = 1;
                while *j <= 5 {
                    *(*data).dofs[*j as usize]
                        .wrapping_offset((noknots + *i) as isize) = next_real(&mut (*cp));
                    *j += 1;
                }
            }
        }
        *i += 1;
    }
}

// SAFETY: cgsim ZONE elements block (split for sizing).
unsafe fn cgsim_elems(
    data: *mut FemType,
    inds: *mut c_int,
    noknots: c_int,
    noelements: c_int,
    thiselems: &mut c_int,
    thismat: c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    allocated: c_int,
) {
    *i = 1;
    while *i <= *thiselems {
        Getrow(line, input, FALSE);
        if allocated != 0 {
            *cp = line;
            *j = 0;
            while *j < 4 {
                *inds.wrapping_offset(*j as isize) = next_int(&mut (*cp));
                *j += 1;
            }
            *j = 0;
            while *j < 4 {
                *(*(*data).topology.wrapping_offset((noelements + *i) as isize))
                    .wrapping_offset(*j as isize) =
                    *inds.wrapping_offset(*j as isize) + noknots;
                *j += 1;
            }
            if *inds.wrapping_offset(2) == *inds.wrapping_offset(3) {
                *(*data).elementtypes.wrapping_offset((noelements + *i) as isize) = 303;
            } else {
                *(*data).elementtypes.wrapping_offset((noelements + *i) as isize) = 404;
            }
            *(*data).material.wrapping_offset((noelements + *i) as isize) = thismat;
        }
        *i += 1;
    }
}

/// Load the mesh from postprocessing format of CGsim.
///
/// # Safety
/// `data` and `prefix` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadCGsimMesh(
    data: *mut FemType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: assigned in the pass loop before every read.
    let mut noknots: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut noelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnodes: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut material: c_int;
    let mut allocated: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut dim: c_int;
    // NOTE: `=FALSE` decl init kept (read before any overwrite).
    let debug: c_int;
    // NOTE: assigned in the pass loop before every read.
    let mut thismat: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut thisknots: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut thiselems: c_int;
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char;
    // NOTE: 0-init for the &mut handoff (set in cgsim_* before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in cgsim_* before reads).
    let mut j: c_int = 0;
    let mut inds: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: `=FALSE` decl init kept (read before any overwrite).
    let savedofs: c_int;
    // NOTE: parsed, never read (dofs skip cursor).
    let mut _dummyreal: Real = 0.0;
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(prefix, filename.as_mut_ptr(), b"plt\0".as_ptr() as *const c_char);
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadCGsimMesh: opening of the CGsim mesh file '%s' wasn't successful !\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(
        b"Reading mesh from CGsim mesh file %s.\n\0".as_ptr() as *const c_char,
        filename.as_ptr(),
    );
    InitializeKnots(data);
    debug = FALSE;
    allocated = FALSE;
    savedofs = FALSE;
    // NOTE: C `omstart:` label + `goto omstart`; two passes then fall through.
    loop {
        maxnodes = 4;
        noknots = 0;
        noelements = 0;
        material = 0;
        dim = 2;
        thismat = 0;
        loop {
            if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
                break;
            }
            if line[0] == 0 {
                break;
            }
            cp = strstr(line.as_ptr(), b"ZONE\0".as_ptr() as *const c_char);
            if cp.is_null() {
                continue;
            }
            thismat += 1;
            cp = strstr(line.as_ptr(), b" N=\0".as_ptr() as *const c_char);
            cp = cp.wrapping_offset(3);
            thisknots = next_int(&mut cp);
            cp = strstr(line.as_ptr(), b",E=\0".as_ptr() as *const c_char);
            cp = cp.wrapping_offset(3);
            thiselems = next_int(&mut cp);
            if debug != 0 {
                printf(b"%s\0".as_ptr() as *const c_char, line.as_ptr());
                printf(
                    b"thismat = %d knots = %d elems = %d\n\0".as_ptr() as *const c_char,
                    thismat,
                    thisknots,
                    thiselems,
                );
            }
            cgsim_knots(
                data, noknots, &mut thisknots, savedofs, &mut _dummyreal, &mut i,
                &mut j, &mut cp, line.as_mut_ptr(), input, allocated,
            );
            cgsim_elems(
                data, inds.as_mut_ptr(), noknots, noelements, &mut thiselems,
                thismat, &mut i, &mut j, &mut cp, line.as_mut_ptr(), input,
                allocated,
            );
            noknots += thisknots;
            noelements += thiselems;
        }
        if allocated != 0 {
            break;
        }
        if noknots == 0 || noelements == 0 || maxnodes == 0 {
            printf(
                b"Invalid mesh consists of %d knots and %d %d-node elements.\n\0".as_ptr()
                    as *const c_char,
                noknots,
                noelements,
                maxnodes,
            );
            fclose(input);
            return 2;
        }
        rewind(input);
        (*data).noknots = noknots;
        (*data).noelements = noelements;
        (*data).maxnodes = maxnodes;
        (*data).dim = dim;
        if info != 0 {
            printf(
                b"Allocating for %d knots and %d %d-node elements.\n\0".as_ptr()
                    as *const c_char,
                noknots,
                noelements,
                maxnodes,
            );
        }
        AllocateKnots(data);
        if savedofs == 1 {
            CreateVariable(data, 1, 1, 0.0, b"Temperature\0".as_ptr() as *const c_char, FALSE);
        } else if savedofs == 5 {
            CreateVariable(data, 1, 1, 0.0, b"dTdX\0".as_ptr() as *const c_char, FALSE);
            CreateVariable(data, 2, 1, 0.0, b"dTdY\0".as_ptr() as *const c_char, FALSE);
            CreateVariable(data, 3, 1, 0.0, b"Qx\0".as_ptr() as *const c_char, FALSE);
            CreateVariable(data, 4, 1, 0.0, b"Qy\0".as_ptr() as *const c_char, FALSE);
            CreateVariable(
                data, 5, 1, 0.0, b"Temperature\0".as_ptr() as *const c_char, FALSE,
            );
        }
        allocated = TRUE;
    }
    fclose(input);
    let _ = material;
    if info != 0 {
        printf(
            b"The CGsim mesh was loaded from file %s.\n\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}
// SAFETY: flux node-count to elmer type (pure table).
unsafe fn FluxToElmerType(nonodes: c_int, dim: c_int) -> c_int {
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut elmertype: c_int = 0;
    if dim == 2 {
        // NOTE: C switch over nonodes.
        if nonodes == 3 {
            elmertype = 203;
        } else if nonodes == 6 {
            elmertype = 306;
        } else if nonodes == 8 {
            elmertype = 408;
        }
    }
    if elmertype == 0 {
        printf(
            b"FluxToElmerType could not deduce element type! (%d %d)\n\0".as_ptr()
                as *const c_char,
            nonodes,
            dim,
        );
    }
    elmertype
}

// SAFETY: flux element-topology mode 10 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn flux_elems(
    data: *mut FemType,
    noknots: c_int,
    noelements: c_int,
    maxnodes: c_int,
    dim: c_int,
    elmertype: &mut c_int,
    nonodes: &mut c_int,
    matind: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(
            b"Allocating mesh with %d nodes and %d %d-node elements in %d dims.\n\0".as_ptr()
                as *const c_char,
            noknots,
            noelements,
            maxnodes,
            dim,
        );
    }
    (*data).noknots = noknots;
    (*data).noelements = noelements;
    (*data).maxnodes = maxnodes;
    (*data).dim = dim;
    AllocateKnots(data);
    if info != 0 {
        printf(b"Reading %d element topologies\n\0".as_ptr() as *const c_char, noelements);
    }
    *i = 1;
    while *i <= noelements {
        Getrow(line, input, FALSE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if *i != *j {
            printf(
                b"It seems that reordering of elements should be performed! (%d %d)\n\0".as_ptr() as *const c_char,
                *i,
                *j,
            );
        }
        *nonodes = next_int(&mut (*cp));
        *matind = abs(next_int(&mut (*cp)));
        *elmertype = FluxToElmerType(*nonodes, dim);
        *(*data).elementtypes.wrapping_offset(*i as isize) = *elmertype;
        *(*data).material.wrapping_offset(*i as isize) = *matind;
        Getrow(line, input, FALSE);
        *cp = line;
        *k = 0;
        while *k < *nonodes {
            *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*k as isize) =
                next_int(&mut (*cp));
            *k += 1;
        }
        *i += 1;
    }
}

// SAFETY: flux node-coordinate mode 11 (split for sizing).
unsafe fn flux_nodes(
    data: *mut FemType,
    noknots: c_int,
    dim: c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(b"Reading %d element nodes\n\0".as_ptr() as *const c_char, noknots);
    }
    *i = 1;
    while *i <= noknots {
        Getrow(line, input, FALSE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if *i != *j {
            printf(
                b"It seems that reordering of nodes should be performed! (%d %d)\n\0".as_ptr() as *const c_char,
                *i,
                *j,
            );
        }
        *(*data).x.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        *(*data).y.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        if dim == 3 {
            *(*data).z.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        }
        *i += 1;
    }
}

// SAFETY: flux region-name mode 12 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn flux_regions(
    data: *mut FemType,
    noregions: c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(b"Reading %d names of regions\n\0".as_ptr() as *const c_char, noregions);
    }
    *i = 1;
    while *i <= noregions {
        Getrow(line, input, FALSE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if *i != *j {
            printf(
                b"It seems that reordering of regions should be performed! (%d %d)\n\0".as_ptr() as *const c_char,
                *i,
                *j,
            );
        }
        sscanf(*cp, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
        if (*data).bodyname[*i as usize].is_null() {
            (*data).bodyname[*i as usize] = cvector(0, MAXNAMESIZE as c_int);
        }
        strcpy((*data).bodyname[*i as usize], entityname.as_ptr());
        *i += 1;
    }
    (*data).bodynamesexist = TRUE;
    (*data).boundarynamesexist = TRUE;
}

/// Load the mesh from format of Flux Cedrat in TRA format.
///
/// # Safety
/// `data`, `bound` and `prefix` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadFluxMesh(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut noknots: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut noelements: c_int;
    // NOTE: `=8` decl init kept (read before any overwrite).
    let maxnodes: c_int;
    // NOTE: `=2` decl init kept (read before any overwrite).
    let dim: c_int;
    // NOTE: 0-init for the &mut handoff (set in flux_elems before reads).
    let mut elmertype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux_elems before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux_elems before reads).
    let mut matind: c_int = 0;
    // NOTE: `=0` decl init kept (noregions read before any overwrite).
    let mut noregions: c_int = 0;
    // NOTE: `=0` decl init kept (mode read before any overwrite).
    let mut mode: c_int;
    // NOTE: always FALSE; the `if(debug)` arms are dead in C.
    let debug: c_int = FALSE;
    let _ = debug;
    // NOTE: debug-only vector dropped (`if(debug)` is dead: debug=FALSE).
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in flux_* before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux_* before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux_elems before reads).
    let mut k: c_int = 0;
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(prefix, filename.as_mut_ptr(), b"TRA\0".as_ptr() as *const c_char);
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadFluxMesh: opening of the Flux mesh file '%s' wasn't successful !\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(
        b"Reading 2D mesh from Flux mesh file %s.\n\0".as_ptr() as *const c_char,
        filename.as_ptr(),
    );
    InitializeKnots(data);
    linenumber = 0;
    dim = 2;
    noknots = 0;
    noelements = 0;
    // NOTE: C `mode = 0` dropped (every pass reassigns before reads).
    maxnodes = 8;
    loop {
        // NOTE: `if(0)` line dump omitted (dead in C).
        if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
            break;
        }
        if line[0] == 0 {
            break;
        }
        if !strstr(line.as_ptr(), b"Number of nodes\0".as_ptr() as *const c_char).is_null() {
            mode = 1;
        } else if !strstr(line.as_ptr(), b"Total number of elements\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 2;
        } else if !strstr(line.as_ptr(), b"Total number of regions\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 3;
        } else if !strstr(line.as_ptr(), b"Description of elements\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 10;
        } else if !strstr(line.as_ptr(), b"Coordinates of the nodes\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 11;
        } else if !strstr(line.as_ptr(), b"Names of the regions\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 12;
        } else if !strstr(line.as_ptr(), b"Neighbouring element table\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 13;
        } else if !strstr(line.as_ptr(), b"List of boundary nodes\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 14;
        } else if !strstr(line.as_ptr(), b"Physical properties\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 15;
        } else if !strstr(line.as_ptr(), b"Boundary conditions\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 16;
        } else {
            // NOTE: `if(debug)` report omitted (debug is FALSE).
            mode = 0;
        }
        // NOTE: `if(debug && mode)` report omitted (debug is FALSE).
        // NOTE: C switch over mode.
        if mode == 1 {
            noknots = atoi(line.as_ptr());
        } else if mode == 2 {
            noelements = atoi(line.as_ptr());
        } else if mode == 3 {
            noregions = atoi(line.as_ptr());
        } else if mode == 10 {
            flux_elems(
                data, noknots, noelements, maxnodes, dim, &mut elmertype, &mut nonodes,
                &mut matind, &mut i, &mut j, &mut k, &mut cp, line.as_mut_ptr(),
                input, info,
            );
        } else if mode == 11 {
            flux_nodes(
                data, noknots, dim, &mut i, &mut j, &mut cp, line.as_mut_ptr(), input,
                info,
            );
        } else if mode == 12 {
            flux_regions(
                data, noregions, &mut entityname, &mut i, &mut j, &mut cp,
                line.as_mut_ptr(), input, info,
            );
        } else {
            // NOTE: `if(debug)` report omitted (debug is FALSE).
            // NOTE: dead `mode = 0` dropped (reassigned next pass).
        }
    }
    fclose(input);
    // NOTE: bulk elements become boundary elements here.
    ElementsToBoundaryConditions(data, bound, TRUE, info);
    if info != 0 {
        printf(
            b"The Flux mesh was loaded from file %s.\n\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}
// SAFETY: pf3 node reorder over a valid topology row.
unsafe fn PF3ToElmerPermuteNodes(elemtype: c_int, topology: *mut c_int) {
    let mut i: c_int;
    let nodes: c_int;
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut reorder: c_int;
    // NOTE: null-init; read only under reorder (set in the matching arm).
    let mut porder: *const c_int = core::ptr::null();
    // NOTE: `=TRUE` decl init kept (read before any overwrite).
    let debug: c_int = TRUE;
    let order303: [c_int; 3] = [3, 1, 2];
    let order306: [c_int; 6] = [3, 1, 2, 6, 4, 5];
    let order404: [c_int; 4] = [3, 4, 1, 2];
    let order408: [c_int; 8] = [3, 4, 1, 2, 7, 8, 5, 6];
    let order504: [c_int; 4] = [1, 2, 3, 4];
    let order510: [c_int; 10] = [1, 2, 3, 4, 5, 8, 6, 7, 10, 9];
    let order605: [c_int; 5] = [3, 2, 1, 4, 5];
    let order613: [c_int; 13] = [3, 2, 1, 4, 5, 7, 6, 9, 8, 12, 11, 10, 13];
    let order706: [c_int; 6] = [6, 4, 5, 3, 1, 2];
    let order715: [c_int; 15] = [6, 4, 5, 3, 1, 2, 12, 10, 11, 9, 7, 8, 15, 13, 14];
    let order808: [c_int; 8] = [7, 8, 5, 6, 3, 4, 1, 2];
    let order820: [c_int; 20] = [
        7, 8, 5, 6, 3, 4, 1, 2, 15, 16, 13, 14, 19, 20, 17, 18, 11, 12, 9, 10,
    ];
    reorder = FALSE;
    // NOTE: C switch over elemtype (101/202/203 are identity arms).
    if elemtype == 303 {
        reorder = TRUE;
        porder = order303.as_ptr();
    } else if elemtype == 306 {
        reorder = TRUE;
        porder = order306.as_ptr();
    } else if elemtype == 404 {
        reorder = TRUE;
        porder = order404.as_ptr();
    } else if elemtype == 408 {
        reorder = TRUE;
        porder = order408.as_ptr();
    } else if elemtype == 504 {
        reorder = TRUE;
        porder = order504.as_ptr();
    } else if elemtype == 510 {
        reorder = TRUE;
        porder = order510.as_ptr();
    } else if elemtype == 605 {
        reorder = TRUE;
        porder = order605.as_ptr();
    } else if elemtype == 613 {
        reorder = TRUE;
        porder = order613.as_ptr();
    } else if elemtype == 706 {
        reorder = TRUE;
        porder = order706.as_ptr();
    } else if elemtype == 715 {
        reorder = TRUE;
        porder = order715.as_ptr();
    } else if elemtype == 808 {
        reorder = TRUE;
        porder = order808.as_ptr();
    } else if elemtype == 820 {
        reorder = TRUE;
        porder = order820.as_ptr();
    } else if elemtype != 101 && elemtype != 202 && elemtype != 203 && debug != 0 {
        printf(
            b"Warning : Unknown element type: %d\n\0".as_ptr() as *const c_char,
            elemtype,
        );
    }
    if reorder != 0 {
        nodes = elemtype % 100;
        i = 0;
        while i < nodes {
            oldtopology[i as usize] = *topology.wrapping_offset(i as isize);
            i += 1;
        }
        i = 0;
        while i < nodes {
            // NOTE: pf3 orders are 1-based (C subtracts one here).
            *topology.wrapping_offset(i as isize) =
                oldtopology[(*porder.wrapping_offset(i as isize) - 1) as usize];
            i += 1;
        }
    }
}

// SAFETY: flux-3D node-count to elmer type (pure table).
unsafe fn FluxToElmerType3D(nonodes: c_int, dim: c_int) -> c_int {
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut elmertype: c_int = 0;
    if dim == 2 {
        // NOTE: C switch over nonodes.
        if nonodes == 3 {
            elmertype = 303;
        } else if nonodes == 4 {
            elmertype = 404;
        } else if nonodes == 6 {
            elmertype = 306;
        } else if nonodes == 8 {
            elmertype = 408;
        }
    }
    if dim == 3 {
        // NOTE: C switch over nonodes.
        if nonodes == 4 {
            elmertype = 504;
        } else if nonodes == 5 {
            elmertype = 605;
        } else if nonodes == 6 {
            elmertype = 706;
        } else if nonodes == 8 {
            elmertype = 808;
        } else if nonodes == 10 {
            elmertype = 510;
        } else if nonodes == 13 {
            elmertype = 613;
        } else if nonodes == 15 {
            elmertype = 715;
        } else if nonodes == 20 {
            elmertype = 820;
        }
    }
    if elmertype == 0 {
        printf(
            b"FluxToElmerType3D could not deduce element type! (%d %d)\n\0".as_ptr()
                as *const c_char,
            nonodes,
            dim,
        );
    }
    elmertype
}
// SAFETY: flux-3D element-topology mode 10 (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn flux3d_elems(
    data: *mut FemType,
    noknots: c_int,
    noelements: c_int,
    maxnodes: c_int,
    dim: c_int,
    maxlinenodes: c_int,
    elmertype: &mut c_int,
    nonodes: &mut c_int,
    matind: &mut c_int,
    dimplusone: &mut c_int,
    nodecnt: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(
            b"Allocating mesh with %d nodes and %d %d-node elements in %d dims.\n\0".as_ptr()
                as *const c_char,
            noknots,
            noelements,
            maxnodes,
            dim,
        );
    }
    (*data).noknots = noknots;
    (*data).noelements = noelements;
    (*data).maxnodes = maxnodes;
    (*data).dim = dim;
    AllocateKnots(data);
    if info != 0 {
        printf(b"Reading %d element topologies\n\0".as_ptr() as *const c_char, noelements);
    }
    *i = 1;
    while *i <= noelements {
        Getrow(line, input, FALSE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if *i != *j {
            printf(
                b"It seems that reordering of elements should be performed! (%d %d)\n\0".as_ptr() as *const c_char,
                *i,
                *j,
            );
        }
        next_int(&mut (*cp));
        next_int(&mut (*cp));
        *matind = next_int(&mut (*cp));
        *dimplusone = next_int(&mut (*cp));
        next_int(&mut (*cp));
        next_int(&mut (*cp));
        *nonodes = next_int(&mut (*cp));
        *elmertype = FluxToElmerType3D(*nonodes, *dimplusone - 1);
        *(*data).elementtypes.wrapping_offset(*i as isize) = *elmertype;
        *(*data).material.wrapping_offset(*i as isize) = *matind;
        Getrow(line, input, FALSE);
        *cp = line;
        *nodecnt = 0;
        *k = 0;
        while *k < *nonodes {
            if *nodecnt >= maxlinenodes {
                *nodecnt = 0;
                Getrow(line, input, FALSE);
                *cp = line;
            }
            *(*(*data).topology.wrapping_offset(*i as isize)).wrapping_offset(*k as isize) =
                next_int(&mut (*cp));
            *nodecnt += 1;
            *k += 1;
        }
        // NOTE: C indexes `topology[noelements]` here, not `topology[i]`.
        PF3ToElmerPermuteNodes(
            *elmertype,
            *(*data).topology.wrapping_offset(noelements as isize),
        );
        *i += 1;
    }
}

// SAFETY: flux-3D node-coordinate mode 11 (split for sizing).
unsafe fn flux3d_nodes(
    data: *mut FemType,
    noknots: c_int,
    i: &mut c_int,
    j: &mut c_int,
    cp: &mut *mut c_char,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(b"Reading %d element nodes\n\0".as_ptr() as *const c_char, noknots);
    }
    *i = 1;
    while *i <= noknots {
        Getrow(line, input, FALSE);
        *cp = line;
        *j = next_int(&mut (*cp));
        if *i != *j {
            printf(
                b"It seems that reordering of nodes should be performed! (%d %d)\n\0".as_ptr() as *const c_char,
                *i,
                *j,
            );
        }
        *(*data).x.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        *(*data).y.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        *(*data).z.wrapping_offset(*i as isize) = next_real(&mut (*cp));
        *i += 1;
    }
}
// SAFETY: flux-3D region-name mode 12 (split for sizing).
unsafe fn flux3d_regions(
    data: *mut FemType,
    noregions: c_int,
    entityname: &mut [c_char; MAXNAMESIZE],
    i: &mut c_int,
    line: *mut c_char,
    input: *mut FILE,
    info: c_int,
) {
    if info != 0 {
        printf(b"Reading %d names of regions\n\0".as_ptr() as *const c_char, noregions);
    }
    *i = 1;
    while *i <= noregions {
        Getrow(line, input, FALSE);
        // NOTE: skip over the section headers to the next name row.
        if !strstr(line, b"REGIONS SURFACIQUES\0".as_ptr() as *const c_char).is_null() {
            Getrow(line, input, FALSE);
        }
        if !strstr(line, b"REGIONS VOLUMIQUES\0".as_ptr() as *const c_char).is_null() {
            Getrow(line, input, FALSE);
        }
        sscanf(line, b"%s\0".as_ptr() as *const c_char, entityname.as_mut_ptr());
        if (*data).bodyname[*i as usize].is_null() {
            (*data).bodyname[*i as usize] = cvector(0, MAXNAMESIZE as c_int);
        }
        strcpy((*data).bodyname[*i as usize], entityname.as_ptr());
        *i += 1;
    }
    (*data).bodynamesexist = TRUE;
    (*data).boundarynamesexist = TRUE;
}
/// Load the mesh from format of Flux Cedrat in PF3 format.
///
/// # Safety
/// `data`, `bound` and `prefix` are valid; `data` fields initialized.
#[no_mangle]
pub unsafe extern "C" fn LoadFluxMesh3D(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut noknots: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut noelements: c_int;
    // NOTE: `=20` decl init kept (read before any overwrite).
    let maxnodes: c_int;
    // NOTE: `=3` decl init kept (read before any overwrite).
    let mut dim: c_int;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut elmertype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut matind: c_int = 0;
    // NOTE: `=0` decl init kept (noregions read before any overwrite).
    let mut noregions: c_int = 0;
    // NOTE: `=0` decl init kept (mode read before any overwrite).
    let mut mode: c_int;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut dimplusone: c_int = 0;
    // NOTE: `=12` decl init kept (nodes may span several lines).
    let maxlinenodes: c_int;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut nodecnt: c_int = 0;
    // NOTE: always FALSE; the `if(debug)` arms are dead in C.
    let debug: c_int = FALSE;
    let _ = debug;
    // NOTE: debug-only vector dropped (`if(debug)` is dead: debug=FALSE).
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    // NOTE: null-init (set on section entry before reads).
    let mut cp: *mut c_char = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in flux helpers before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux helpers before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in flux3d_elems before reads).
    let mut k: c_int = 0;
    let mut entityname: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    let mut input: *mut FILE;
    strcpy(filename.as_mut_ptr(), prefix);
    input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if input.is_null() {
        AddExtension(prefix, filename.as_mut_ptr(), b"PF3\0".as_ptr() as *const c_char);
        input = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
        if input.is_null() {
            printf(
                b"LoadFluxMesh3D: opening of the Flux mesh file '%s' wasn't successful !\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            return 1;
        }
    }
    printf(
        b"Reading 3D mesh from Flux mesh file %s.\n\0".as_ptr() as *const c_char,
        filename.as_ptr(),
    );
    InitializeKnots(data);
    linenumber = 0;
    dim = 3;
    noknots = 0;
    noelements = 0;
    // NOTE: C `mode = 0` dropped (every pass reassigns before reads).
    // NOTE: 20 nodes max (15?); topology rows may span several lines.
    maxnodes = 20;
    maxlinenodes = 12;
    loop {
        // NOTE: `if(0)` line dump omitted (dead in C).
        if Getrow(line.as_mut_ptr(), input, FALSE) != 0 {
            break;
        }
        if line[0] == 0 {
            break;
        }
        if !strstr(line.as_ptr(), b"==== DECOUPAGE  TERMINE\0".as_ptr() as *const c_char)
            .is_null()
        {
            break;
        }
        if !strstr(line.as_ptr(), b"NOMBRE DE DIMENSIONS DU DECOUPAGE\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 1;
        } else if !strstr(line.as_ptr(), b"NOMBRE  D'ELEMENTS\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 3;
        } else if !strstr(line.as_ptr(), b"NOMBRE DE POINTS\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 2;
        } else if !strstr(line.as_ptr(), b"NOMBRE DE REGIONS\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 4;
        } else if !strstr(
            line.as_ptr(),
            b"DESCRIPTEUR DE TOPOLOGIE DES ELEMENTS\0".as_ptr() as *const c_char,
        )
        .is_null()
        {
            mode = 10;
        } else if !strstr(line.as_ptr(), b"COORDONNEES DES NOEUDS\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 11;
        } else if !strstr(line.as_ptr(), b"NOMS DES REGIONS\0".as_ptr() as *const c_char)
            .is_null()
        {
            mode = 12;
        } else {
            // NOTE: `if(debug)` report omitted (debug is FALSE).
            mode = 0;
        }
        // NOTE: `if(debug && mode)` report omitted (debug is FALSE).
        // NOTE: C switch over mode.
        if mode == 1 {
            dim = atoi(line.as_ptr());
        } else if mode == 2 {
            // NOTE: skip the integration-point count; want the node total.
            if strstr(line.as_ptr(), b"NOMBRE DE POINTS D'INTEGRATION\0".as_ptr() as *const c_char)
                .is_null()
            {
                noknots = atoi(line.as_ptr());
            }
        } else if mode == 3 {
            i = atoi(line.as_ptr());
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if noelements < i {
                noelements = i;
            }
        } else if mode == 4 {
            i = atoi(line.as_ptr());
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if noregions < i {
                noregions = i;
            }
        } else if mode == 10 {
            flux3d_elems(
                data, noknots, noelements, maxnodes, dim, maxlinenodes, &mut elmertype,
                &mut nonodes, &mut matind, &mut dimplusone, &mut nodecnt, &mut i,
                &mut j, &mut k, &mut cp, line.as_mut_ptr(), input, info,
            );
        } else if mode == 11 {
            flux3d_nodes(
                data, noknots, &mut i, &mut j, &mut cp, line.as_mut_ptr(), input,
                info,
            );
        } else if mode == 12 {
            flux3d_regions(
                data, noregions, &mut entityname, &mut i, line.as_mut_ptr(), input,
                info,
            );
        } else {
            // NOTE: `if(debug)` report omitted (debug is FALSE).
            // NOTE: dead `mode = 0` dropped (reassigned next pass).
        }
    }
    fclose(input);
    // NOTE: bulk elements become boundary elements here.
    ElementsToBoundaryConditions(data, bound, TRUE, info);
    if info != 0 {
        printf(
            b"The Flux 3D mesh was loaded from file %s.\n\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}
