// Faithful Rust transcription of elmergrid/src/egexport.c @ 9f6af2f85.
// Workspace layout: shared declarations live in eg-prelude; this crate
// appends only the unit body.
// Config scope: exe-only unit (not in elmergrid_core), no USE_* references —
// single archive, spliced into the ElmerGrid object only.
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
    fn GetBoundaryElement(
        sideind: c_int,
        bound: *mut BoundaryType,
        data: *mut FemType,
        ind: *mut c_int,
        sideelemtype: *mut c_int,
    );
    fn GetElementDimension(elementtype: c_int) -> c_int;
}

// SAFETY: file export; filename buffer is caller-sized (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveAbaqusInput(
    data: *mut FemType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let (mut i, mut j);
    let (noknots, noelements, nonodes);
    let out: *mut FILE;
    noknots = (*data).noknots;
    noelements = (*data).noelements;
    nonodes = (*data).maxnodes;
    if nonodes != 4 && nonodes != 8 {
        printf(
            b"SaveAbaqusInput: not designed for %d-node elements\n\0".as_ptr() as *const c_char,
            nonodes,
        );
        return 1;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"inp\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving ABAQUS data to %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    fprintf(out, b"*HEADING\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"Abaqus input file created by ElmerGrid\n\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"*NODE, SYSTEM=\0".as_ptr() as *const c_char);
    if (*data).coordsystem == COORD_CART2 {
        fprintf(out, b"R\n\0".as_ptr() as *const c_char);
    } else if (*data).coordsystem == COORD_AXIS {
        fprintf(out, b"C\n\0".as_ptr() as *const c_char);
    } else if (*data).coordsystem == COORD_POLAR {
        fprintf(out, b"P\n\0".as_ptr() as *const c_char);
    }
    i = 1;
    while i <= noknots {
        fprintf(
            out,
            b"%8d, %12.4le, %12.4le, 0.0\n\0".as_ptr() as *const c_char,
            i,
            *(*data).x.wrapping_offset(i as isize),
            *(*data).y.wrapping_offset(i as isize),
        );
        i += 1;
    }
    fprintf(out, b"*ELEMENT,TYPE=\0".as_ptr() as *const c_char);
    if nonodes == 4 {
        fprintf(out, b"DC2D4 \0".as_ptr() as *const c_char);
    } else if nonodes == 8 {
        fprintf(out, b"DC2D8 \0".as_ptr() as *const c_char);
    } else {
        printf(
            b"SaveAbaqusInput: Not defined for %d-node elements\n\0".as_ptr() as *const c_char,
            nonodes,
        );
    }
    fprintf(out, b",ELSET=SETTI1\n\0".as_ptr() as *const c_char);
    i = 1;
    while i <= noelements {
        fprintf(out, b"%8d, \0".as_ptr() as *const c_char, i);
        j = 1;
        while j < nonodes {
            fprintf(
                out,
                b"%6d, \0".as_ptr() as *const c_char,
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset((j - 1) as isize),
            );
            j += 1;
        }
        fprintf(
            out,
            b"%6d\n\0".as_ptr() as *const c_char,
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset((nonodes - 1) as isize),
        );
        i += 1;
    }
    fprintf(
        out,
        b"*SOLID SECTION, ELSET=SETTI1, MATERIAL=MAT1\n\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"*MATERIAL, NAME=MAT1\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*CONDUCTIVITY\n\0".as_ptr() as *const c_char);
    fprintf(out, b"1.0\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*RESTART, WRITE, FREQUENCY=1\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*FILE FORMAT, ASCII\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*STEP\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*HEAT TRANSFER, STEADY STATE\n\0".as_ptr() as *const c_char);
    fprintf(out, b"*END STEP\n\0".as_ptr() as *const c_char);
    fclose(out);
    if info != 0 {
        printf(
            b"Wrote the mesh in ABAQUS restart format.\n\0".as_ptr() as *const c_char,
        );
    }
    0
}

// SAFETY: fixed-form triple for the C variadic tail. The toolchain's stable
// VaList exposes no argument reader, so a C-variadic *definition* cannot
// consume its tail; instead this defines the full reachable shape. Exact
// because: (a) the body clamps vctrs to <= 3 before any read, so only the
// first three vector slots are ever observable; (b) the sole tree call site
// (fempre.c) passes vctrs = 1 with one pointer, and the vctrs = 1 path reads
// only dofs[1]; (c) unread trailing fixed params are never touched. A future
// caller passing > 3 vectors behaves identically: C's va_arg would read only
// post-clamp slots too.
#[no_mangle]
pub unsafe extern "C" fn SaveFidapOutput(
    data: *mut FemType,
    prefix: *mut c_char,
    info: c_int,
    mut vctrs: c_int,
    vect1: *mut Real,
    vect2: *mut Real,
    vect3: *mut Real,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let (mut i, mut j, mut no, mut nogroup, mut material, mut cellelem);
    let (noknots, noelements, dim, mut nonodes);
    let mut elemcodes: [c_int; 51] = [0; 51];
    let mut mat: [c_int; 51] = [0; 51];
    let mut dofs: [*mut Real; MAXDOFS] = [core::ptr::null_mut(); MAXDOFS];
    let out: *mut FILE;
    if (*data).created == 0 {
        printf(
            b"You tried to save points that were never created.\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    printf(
        b"Saving results in FIDAP neutral format.\n\0".as_ptr() as *const c_char,
    );
    noknots = (*data).noknots;
    noelements = (*data).noelements;
    dim = (*data).dim;
    if vctrs > 3 {
        printf(
            b"SaveSolutionFidap: Maximum of 3 d.o.f.\n\0".as_ptr() as *const c_char,
        );
        vctrs = 3;
    }
    if vctrs > 0 {
        dofs[1] = vect1;
        dofs[2] = vect2;
        dofs[3] = vect3;
    }
    nogroup = 0;
    i = 1;
    while i <= MAT_MAXNUMBER {
        elemcodes[i as usize] = 0;
        mat[i as usize] = 0;
        i += 1;
    }
    j = 1;
    while j <= noelements {
        material = *(*data).material.wrapping_offset(j as isize);
        if material > 0 && material <= MAT_MAXNUMBER {
            mat[material as usize] += 1;
        }
        // SAFETY: C indexes elemcodes[material] unguarded (negative boundary
        // markers go out of bounds there too); wrapping_offset mirrors the
        // instruction instead of panicking where C would go silent-UB.
        *elemcodes.as_mut_ptr().wrapping_offset(material as isize) =
            *(*data).elementtypes.wrapping_offset(j as isize);
        j += 1;
    }
    i = 1;
    while i <= MAT_MAXNUMBER {
        if mat[i as usize] > 0 {
            nogroup += 1;
        }
        i += 1;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"fidap\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    fprintf(out, b"** FIDAP NEUTRAL FILE\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"Fidap input file created by ElmerGrid\n\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"VERSION %7.2f\n\0".as_ptr() as *const c_char, 7.52);
    fprintf(out, b" 1 Dec 96    12:00:00\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"%15s%15s%15s%15s%15s\n\0".as_ptr() as *const c_char,
        b"NO. OF NODES\0".as_ptr() as *const c_char,
        b"NO. ELEMENTS\0".as_ptr() as *const c_char,
        b"NO. ELT GROUPS\0".as_ptr() as *const c_char,
        b"NDFCD\0".as_ptr() as *const c_char,
        b"NDFVL\0".as_ptr() as *const c_char,
    );
    fprintf(
        out,
        b"%15d%15d%15d%15d%15d\n\0".as_ptr() as *const c_char,
        noknots,
        noelements,
        nogroup,
        dim,
        dim,
    );
    fprintf(
        out,
        b"%15s%15s%15s%15s\n\0".as_ptr() as *const c_char,
        b"STEADY/TRANS\0".as_ptr() as *const c_char,
        b"TURB. FLAG\0".as_ptr() as *const c_char,
        b"FREE SURF FLAG\0".as_ptr() as *const c_char,
        b"COMPR. FLAG\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"%15d%15d%15d%15d\n\0".as_ptr() as *const c_char, 0, 0, 0, 0);
    fprintf(out, b"%s\n\0".as_ptr() as *const c_char, b"TEMPERATURE/SPECIES FLAGS\0".as_ptr() as *const c_char);
    fprintf(out, b"%s\n\0".as_ptr() as *const c_char, b" 3 0 0 0 0 0 0 0 0 0 0 0 0 0 0\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"%s\n\0".as_ptr() as *const c_char,
        b"PRESSURE FLAGS - IDCTS, IPENY MPDF\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"%10d%10d%10d\n\0".as_ptr() as *const c_char, 0, 0, 1);
    fprintf(out, b"NODAL COORDINATES\n\0".as_ptr() as *const c_char);
    i = 1;
    while i <= noknots {
        fprintf(
            out,
            b"%10d%20.10e%20.10e\n\0".as_ptr() as *const c_char,
            i,
            *(*data).y.wrapping_offset(i as isize),
            *(*data).x.wrapping_offset(i as isize),
        );
        i += 1;
    }
    fprintf(out, b"BOUNDARY CONDITIONS\n\0".as_ptr() as *const c_char);
    fprintf(out, b"%10d%10d%10d%20.10e\n\0".as_ptr() as *const c_char, 0, 0, 5, 0.0);
    fprintf(out, b"ELEMENT GROUPS\n\0".as_ptr() as *const c_char);
    nogroup = 0;
    no = 1;
    while no <= MAT_MAXNUMBER {
        if mat[no as usize] > 0 {
            nonodes = elemcodes[no as usize] % 100;
            nogroup += 1;
            cellelem = mat[no as usize];
            fprintf(
                out,
                b"GROUP:    %5d ELEMENTS:%10d NODES:   %10d GEOMETRY:%5d TYPE:%4d\n\0".as_ptr()
                    as *const c_char,
                nogroup,
                cellelem,
                nonodes,
                1,
                1,
            );
            fprintf(
                out,
                b"ENTITY NAME:   %s%d\n\0".as_ptr() as *const c_char,
                b"material\0".as_ptr() as *const c_char,
                nogroup,
            );
            j = 1;
            while j <= noelements {
                if *(*data).material.wrapping_offset(j as isize) == no {
                    fprintf(out, b"%8d\n\0".as_ptr() as *const c_char, j);
                    i = 0;
                    while i < nonodes {
                        fprintf(
                            out,
                            b"%8d\0".as_ptr() as *const c_char,
                            *(*(*data).topology.wrapping_offset(j as isize))
                                .wrapping_offset(i as isize),
                        );
                        i += 1;
                    }
                    fprintf(out, b"\n\0".as_ptr() as *const c_char);
                }
                j += 1;
            }
        }
        no += 1;
    }
    fprintf(
        out,
        b"TIMESTEP: %5d TIME:     %15.7le INCRMNT: %15.7le\n\0".as_ptr() as *const c_char,
        1,
        1.0,
        1.0,
    );
    fprintf(out, b"VELOCITY\n\0".as_ptr() as *const c_char);
    if vctrs < 2 {
        i = 1;
        while i <= 2 * noknots {
            fprintf(out, b"%16.9e\0".as_ptr() as *const c_char, 0.0);
            if i % 5 == 0 {
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
            }
            i += 1;
        }
    } else {
        i = 1;
        while i <= 2 * noknots {
            fprintf(out, b"%16.9e\0".as_ptr() as *const c_char, *dofs[1].wrapping_offset(i as isize));
            if (2 * i - 1) % 5 == 0 {
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
            }
            fprintf(out, b"%16.9e\0".as_ptr() as *const c_char, *dofs[2].wrapping_offset(i as isize));
            if (2 * i) % 5 == 0 {
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
            }
            i += 1;
        }
    }
    if (2 * noknots) % 5 != 0 {
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    fprintf(out, b"TEMPERATURE\n\0".as_ptr() as *const c_char);
    if vctrs == 2 {
        i = 1;
        while i <= noknots {
            fprintf(out, b"%16.9e\0".as_ptr() as *const c_char, 0.0);
            if i % 5 == 0 {
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
            }
            i += 1;
        }
    } else {
        i = 1;
        while i <= noknots {
            fprintf(
                out,
                b"%16.9e\0".as_ptr() as *const c_char,
                *dofs[vctrs as usize].wrapping_offset(i as isize),
            );
            if i % 5 == 0 {
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
            }
            i += 1;
        }
    }
    if noknots % 5 != 0 {
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
    }
    fprintf(out, b"ENDOFTIMESTEP\n\0".as_ptr() as *const c_char);
    fclose(out);
    if info != 0 {
        printf(
            b"Results were saved in FIDAP neutral format to file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}
// SAFETY: pure mapping table, transcribed verbatim (incl. default printf).
unsafe fn ElmerToGmshType(elmertype: c_int) -> c_int {
    let mut gmshtype = 0;
    match elmertype {
        202 => gmshtype = 1,
        303 => gmshtype = 2,
        404 => gmshtype = 3,
        504 => gmshtype = 4,
        808 => gmshtype = 5,
        706 => gmshtype = 6,
        605 => gmshtype = 7,
        203 => gmshtype = 8,
        306 => gmshtype = 9,
        409 => gmshtype = 10,
        510 => gmshtype = 11,
        827 => gmshtype = 12,
        101 => gmshtype = 15,
        408 => gmshtype = 16,
        820 => gmshtype = 17,
        715 => gmshtype = 18,
        718 => gmshtype = 13,
        613 => gmshtype = 19,
        310 => gmshtype = 21,
        _ => {
            printf(
                b"Elmer element %d does not have an Gmsh counterpart!\n\0".as_ptr()
                    as *const c_char,
                elmertype,
            );
        }
    }
    gmshtype
}

// SAFETY: in-place index permutation over `nodes` entries (C contract).
unsafe fn ElmerToGmshIndx(elemtype: c_int, topology: *mut c_int) {
    let (mut i, nodes);
    let order510: [c_int; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 8];
    let order613: [c_int; 13] = [0, 1, 2, 3, 4, 5, 8, 10, 6, 7, 9, 11, 12];
    let order718: [c_int; 18] = [0, 1, 2, 3, 4, 5, 6, 9, 7, 8, 10, 11, 12, 14, 13, 15, 17, 16];
    let order820: [c_int; 20] =
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 9, 10, 12, 14, 15, 16, 18, 19, 17];
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut reorder = FALSE;
    let mut porder: *const c_int = core::ptr::null();
    match elemtype {
        510 => {
            reorder = TRUE;
            porder = order510.as_ptr();
        }
        613 => {
            reorder = TRUE;
            porder = order613.as_ptr();
        }
        715 | 718 => {
            reorder = TRUE;
            porder = order718.as_ptr();
        }
        820 => {
            reorder = TRUE;
            porder = order820.as_ptr();
        }
        _ => {}
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
            *topology.wrapping_offset(*porder.wrapping_offset(i as isize) as isize) =
                oldtopology[i as usize];
            i += 1;
        }
    }
}

// SAFETY: file export; runtime-built outstyle goes through C sprintf/fprintf.
#[no_mangle]
pub unsafe extern "C" fn SaveMeshGmsh(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nobound: c_int,
    prefix: *mut c_char,
    decimals: c_int,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut outstyle: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut usedbody: [c_int; MAXBODIES] = [0; MAXBODIES];
    let mut usedbc: [c_int; MAXBCS] = [0; MAXBCS];
    let (mut i, mut j, mut k, mut nonames, mut physent);
    let (mut material, noknots, noelements, bulkelems, mut sideelems): (
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    );
    let mut gmshtype: c_int;
    // NOTE: initialized for the compiler's definite-assignment proof at the
    // &mut handoff to GetBoundaryElement; every path reaching a read has
    // assigned it, so the initial value is unobservable.
    let mut elemtype: c_int = 0;
    let mut boundtype: c_int;
    let mut nodesd2: c_int;
    let mut elemind: c_int;
    let out: *mut FILE;
    if (*data).created == 0 {
        printf(
            b"SaveMeshGmsh: You tried to save points that were never created.\n\0".as_ptr()
                as *const c_char,
        );
        return 1;
    }
    sideelems = 0;
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created != 0 {
                sideelems += (*bound.wrapping_offset(j as isize)).nosides;
            }
            j += 1;
        }
    }
    noknots = (*data).noknots;
    bulkelems = (*data).noelements;
    if nobound != 0 {
        noelements = bulkelems + sideelems;
    } else {
        noelements = bulkelems;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"msh\0".as_ptr() as *const c_char,
    );
    if info != 0 {
        printf(
            b"Saving Gmsh mesh to %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 3;
    }
    fprintf(out, b"$MeshFormat\n\0".as_ptr() as *const c_char);
    fprintf(out, b"2.2 0 %ld\n\0".as_ptr() as *const c_char, 8 as c_long);
    fprintf(out, b"$EndMeshFormat\n\0".as_ptr() as *const c_char);
    if (*data).bodynamesexist != 0 || (*data).boundarynamesexist != 0 {
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
        fprintf(out, b"$PhysicalNames\n\0".as_ptr() as *const c_char);
        nonames = 0;
        if (*data).boundarynamesexist != 0 {
            j = 0;
            while j < MAXBOUNDARIES as c_int {
                if (*bound.wrapping_offset(j as isize)).created == 0 {
                    j += 1;
                    continue;
                }
                if (*bound.wrapping_offset(j as isize)).nosides == 0 {
                    j += 1;
                    continue;
                }
                i = 1;
                while i <= (*bound.wrapping_offset(j as isize)).nosides {
                    GetBoundaryElement(
                        i,
                        bound.wrapping_offset(j as isize),
                        data,
                        ind.as_mut_ptr(),
                        &mut elemtype as *mut c_int,
                    );
                    k = *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
                    // SAFETY: C guards k < MAXBCS only (negative k goes OOB
                    // there too); mirror the instruction, don't panic.
                    if k < MAXBCS as c_int {
                        *usedbc.as_mut_ptr().wrapping_offset(k as isize) =
                            GetElementDimension(elemtype);
                    }
                    i += 1;
                }
                j += 1;
            }
            i = 1;
            while i < MAXBCS as c_int {
                if !(*data).boundaryname[i as usize].is_null() && usedbc[i as usize] != 0 {
                    nonames += 1;
                }
                i += 1;
            }
        }
        if (*data).bodynamesexist != 0 {
            i = 1;
            while i <= noelements {
                elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
                k = *(*data).material.wrapping_offset(i as isize);
                // SAFETY: same OOB-mirror rationale as above.
                if k < MAXBODIES as c_int {
                    *usedbody.as_mut_ptr().wrapping_offset(k as isize) =
                        GetElementDimension(elemtype);
                }
                i += 1;
            }
            i = 1;
            while i < MAXBODIES as c_int {
                if !(*data).bodyname[i as usize].is_null() && usedbody[i as usize] != 0 {
                    nonames += 1;
                }
                i += 1;
            }
        }
        fprintf(out, b"%d\n\0".as_ptr() as *const c_char, nonames);
        if (*data).boundarynamesexist != 0 {
            i = 1;
            while i < MAXBCS as c_int {
                if !(*data).boundaryname[i as usize].is_null() && usedbc[i as usize] != 0 {
                    fprintf(
                        out,
                        b"%d %d \"%s\"\n\0".as_ptr() as *const c_char,
                        usedbc[i as usize],
                        i,
                        (*data).boundaryname[i as usize],
                    );
                }
                i += 1;
            }
        }
        if (*data).bodynamesexist != 0 {
            i = 1;
            while i < MAXBODIES as c_int {
                if !(*data).bodyname[i as usize].is_null() && usedbody[i as usize] != 0 {
                    fprintf(
                        out,
                        b"%d %d \"%s\"\n\0".as_ptr() as *const c_char,
                        usedbody[i as usize],
                        i,
                        (*data).bodyname[i as usize],
                    );
                }
                i += 1;
            }
        }
        fprintf(out, b"$EndPhysicalNames\n\0".as_ptr() as *const c_char);
    }
    if info != 0 {
        printf(
            b"Saving %d node coordinates.\n\0".as_ptr() as *const c_char,
            noknots,
        );
    }
    fprintf(out, b"$Nodes\n\0".as_ptr() as *const c_char);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, noknots);
    if (*data).dim == 1 {
        sprintf(
            outstyle.as_mut_ptr(),
            b"%%d %%.%dg 0 0\n\0".as_ptr() as *const c_char,
            decimals,
        );
        i = 1;
        while i <= noknots {
            fprintf(
                out,
                outstyle.as_ptr(),
                i,
                *(*data).x.wrapping_offset(i as isize),
            );
            i += 1;
        }
    } else if (*data).dim == 2 {
        sprintf(
            outstyle.as_mut_ptr(),
            b"%%d %%.%dg %%.%dg 0\n\0".as_ptr() as *const c_char,
            decimals,
            decimals,
        );
        i = 1;
        while i <= noknots {
            fprintf(
                out,
                outstyle.as_ptr(),
                i,
                *(*data).x.wrapping_offset(i as isize),
                *(*data).y.wrapping_offset(i as isize),
            );
            i += 1;
        }
    } else if (*data).dim == 3 {
        sprintf(
            outstyle.as_mut_ptr(),
            b"%%d %%.%dg %%.%dg %%.%dg\n\0".as_ptr() as *const c_char,
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
                *(*data).x.wrapping_offset(i as isize),
                *(*data).y.wrapping_offset(i as isize),
                *(*data).z.wrapping_offset(i as isize),
            );
            i += 1;
        }
    }
    fprintf(out, b"$EndNodes\n\0".as_ptr() as *const c_char);
    printf(b"Saving %d element topologies.\n\0".as_ptr() as *const c_char, bulkelems);
    fprintf(out, b"$Elements\n\0".as_ptr() as *const c_char);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, bulkelems + sideelems);
    i = 1;
    while i <= bulkelems {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        material = *(*data).material.wrapping_offset(i as isize);
        gmshtype = ElmerToGmshType(elemtype);
        physent = 0;
        if (*data).bodynamesexist != 0 && material < MAXBODIES as c_int {
            if !(*data).bodyname[material as usize].is_null()
                && usedbody[material as usize] != 0
            {
                physent = material;
            }
        }
        fprintf(
            out,
            b"%d %d %d %d %d\0".as_ptr() as *const c_char,
            i,
            gmshtype,
            2,
            physent,
            material,
        );
        nodesd2 = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
        j = 0;
        while j < nodesd2 {
            ind[j as usize] =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize);
            j += 1;
        }
        ElmerToGmshIndx(elemtype, ind.as_mut_ptr());
        j = 0;
        while j < nodesd2 {
            fprintf(out, b" %d\0".as_ptr() as *const c_char, ind[j as usize]);
            j += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    elemind = bulkelems;
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created == FALSE {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                GetBoundaryElement(
                    i,
                    bound.wrapping_offset(j as isize),
                    data,
                    ind.as_mut_ptr(),
                    &mut elemtype as *mut c_int,
                );
                boundtype = *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
                gmshtype = ElmerToGmshType(elemtype);
                elemind += 1;
                physent = 0;
                if (*data).boundarynamesexist != 0 && boundtype < MAXBCS as c_int {
                    if !(*data).boundaryname[boundtype as usize].is_null()
                        && usedbc[boundtype as usize] != 0
                    {
                        physent = boundtype;
                    }
                }
                fprintf(
                    out,
                    b"%d %d %d %d %d\0".as_ptr() as *const c_char,
                    elemind,
                    gmshtype,
                    2,
                    physent,
                    boundtype,
                );
                nodesd2 = elemtype % 100;
                ElmerToGmshIndx(elemtype, ind.as_mut_ptr());
                k = 0;
                while k < nodesd2 {
                    fprintf(out, b" %d\0".as_ptr() as *const c_char, ind[k as usize]);
                    k += 1;
                }
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
                i += 1;
            }
            j += 1;
        }
    }
    fprintf(out, b"$EndElements\n\0".as_ptr() as *const c_char);
    // NOTE: the `#if 0` dof-dump block is omitted exactly as the C
    // preprocessor does (it references undeclared locals).
    fclose(out);
    printf(b"SaveMeshGmsh: All done\n\0".as_ptr() as *const c_char);
    0
}
// SAFETY: pure mapping table; the case-101 fallthrough (1, then 202's 3) is
// kept explicit so the shape stays comparable with the C switch.
unsafe fn ElmerToVtkType(elmertype: c_int) -> c_int {
    let mut vtktype = 0;
    match elmertype {
        101 => {
            // C assigns vtktype = 1 here, then falls through into `case 202`
            // with no read between: net effect is 3, transcribed directly.
            vtktype = 3;
        }
        202 => vtktype = 3,
        203 => vtktype = 21,
        204 => vtktype = 35,
        303 => vtktype = 5,
        306 => vtktype = 22,
        310 => vtktype = 69,
        404 => vtktype = 9,
        408 => vtktype = 23,
        409 => vtktype = 28,
        416 => vtktype = 70,
        504 => vtktype = 10,
        510 => vtktype = 24,
        605 => vtktype = 14,
        613 => vtktype = 27,
        706 => vtktype = 13,
        715 => vtktype = 26,
        718 => vtktype = 32,
        808 => vtktype = 12,
        820 => vtktype = 25,
        827 => vtktype = 29,
        _ => {
            printf(
                b"Elmer element %d does not have an Vtk counterpart!\n\0".as_ptr()
                    as *const c_char,
                elmertype,
            );
        }
    }
    vtktype
}

// SAFETY: in-place index permutation over `nodes` entries (C contract).
// NOTE the C asymmetry is preserved: the reorder writes topology[i] =
// oldtopology[porder[i]] here (Gmsh variant writes topology[porder[i]]).
unsafe fn ElmerToVtkIndx(elemtype: c_int, topology: *mut c_int) {
    let (mut i, nodes);
    let order820: [c_int; 20] =
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 12, 13, 14, 15];
    let order827: [c_int; 27] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 12, 13, 14, 15, 23, 21, 20,
        22, 24, 25, 26,
    ];
    let mut oldtopology: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut reorder = FALSE;
    let mut porder: *const c_int = core::ptr::null();
    match elemtype {
        820 => {
            reorder = TRUE;
            porder = order820.as_ptr();
        }
        827 => {
            reorder = TRUE;
            porder = order827.as_ptr();
        }
        _ => {}
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

// SAFETY: file export; `if(0)` test scaffolding omitted (runtime-dead).
#[no_mangle]
pub unsafe extern "C" fn SaveMeshVtu(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nobound: c_int,
    prefix: *mut c_char,
    dummyzero: c_int,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let (mut i, mut j, mut k);
    let (noknots, noelements, bulkelems, mut sideelems): (c_int, c_int, c_int, c_int);
    let mut vtktype: c_int;
    // NOTE: same definite-assignment rationale as in SaveMeshGmsh above.
    let mut elemtype: c_int = 0;
    let mut boundtype: c_int;
    // NOTE: `nodesd2`/`elemind` are declared but never used in C either.
    let mut idoffset: c_int;
    let (di, mut maxbody, mut minbc);
    let (little_end, prec_bits, mut elemoffset);
    let out: *mut FILE;
    if dummyzero != 0 {
        di = 0;
    } else {
        di = 1;
    }
    if (*data).created == 0 {
        printf(
            b"SaveMeshVtk: You tried to save points that were never created.\n\0".as_ptr()
                as *const c_char,
        );
        return 1;
    }
    sideelems = 0;
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created != 0 {
                sideelems += (*bound.wrapping_offset(j as isize)).nosides;
            }
            j += 1;
        }
    }
    noknots = (*data).noknots;
    bulkelems = (*data).noelements;
    if nobound != 0 {
        noelements = bulkelems + sideelems;
    } else {
        noelements = bulkelems;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"vtu\0".as_ptr() as *const c_char,
    );
    if info != 0 {
        printf(
            b"Saving VTU mesh to %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 3;
    }
    maxbody = 0;
    i = 1;
    while i <= bulkelems {
        maxbody = c_max_int(maxbody, *(*data).material.wrapping_offset(i as isize));
        i += 1;
    }
    minbc = 0;
    j = 0;
    while j < nobound {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            boundtype =
                *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
            if minbc == 0 {
                minbc = boundtype;
            } else {
                minbc = c_min_int(minbc, boundtype);
            }
            i += 1;
        }
        j += 1;
    }
    idoffset = 0;
    if minbc != 0 && minbc <= maxbody {
        idoffset = 100;
        loop {
            if maxbody > idoffset {
                idoffset *= 10;
            } else {
                break;
            }
        }
        printf(
            b"Setting offset for boundaries: %d\n\0".as_ptr() as *const c_char,
            idoffset,
        );
    }
    little_end = FALSE;
    prec_bits = 64;
    if info != 0 {
        printf(
            b"Saving Elmer mesh in ascii VTU format\n\0".as_ptr() as *const c_char,
        );
    }
    fprintf(out, b"<?xml version=\"1.0\"?>\n\0".as_ptr() as *const c_char);
    if little_end != 0 {
        fprintf(
            out,
            b"<VTKFile type=\"UnstructuredGrid\" version=\"0.1\" byte_order=\"LittleEndian\">\n\0"
                .as_ptr() as *const c_char,
        );
    } else {
        fprintf(
            out,
            b"<VTKFile type=\"UnstructuredGrid\" version=\"0.1\" byte_order=\"BigEndian\">\n\0"
                .as_ptr() as *const c_char,
        );
    }
    fprintf(out, b"  <UnstructuredGrid>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"    <Piece NumberOfPoints=\"%d\"  NumberOfCells=\"%d\">\n\0".as_ptr() as *const c_char,
        noknots + dummyzero,
        noelements,
    );
    fprintf(out, b"      <PointData>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"NodeNumber\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    if dummyzero != 0 {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, 0);
    }
    i = 1;
    while i <= noknots {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, i);
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"      </PointData>\n\0".as_ptr() as *const c_char);
    printf(
        b"Saving cell data (Element numbers and Geometry Ids).\n\0".as_ptr() as *const c_char,
    );
    fprintf(out, b"      <CellData>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"ElementNumber\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    i = 1;
    while i <= noelements {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, i);
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"GeometryIds\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    i = 1;
    while i <= bulkelems {
        fprintf(out, b"%d \0".as_ptr() as *const c_char, *(*data).material.wrapping_offset(i as isize));
        i += 1;
    }
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created == FALSE {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                boundtype =
                    *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
                fprintf(out, b"%d \0".as_ptr() as *const c_char, idoffset + boundtype);
                i += 1;
            }
            j += 1;
        }
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    if (*data).partitionexist != 0 {
        fprintf(
            out,
            b"        <DataArray type=\"Int32\" Name=\"Partition\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
                .as_ptr() as *const c_char,
        );
        i = 1;
        while i <= bulkelems {
            fprintf(
                out,
                b"%d \0".as_ptr() as *const c_char,
                *(*data).elempart.wrapping_offset(i as isize),
            );
            i += 1;
        }
        if nobound != 0 {
            j = 0;
            while j < nobound {
                if (*bound.wrapping_offset(j as isize)).created == FALSE {
                    j += 1;
                    continue;
                }
                i = 1;
                while i <= (*bound.wrapping_offset(j as isize)).nosides {
                    k = *(*bound.wrapping_offset(j as isize)).parent.wrapping_offset(i as isize);
                    if k != 0 {
                        fprintf(
                            out,
                            b"%d \0".as_ptr() as *const c_char,
                            *(*data).elempart.wrapping_offset(k as isize),
                        );
                    } else {
                        fprintf(out, b"%d \0".as_ptr() as *const c_char, 0);
                    }
                    i += 1;
                }
                j += 1;
            }
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    }
    fprintf(out, b"      </CellData>\n\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving %d nodal coordinates\n\0".as_ptr() as *const c_char,
            noknots,
        );
    }
    fprintf(out, b"      <Points>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Float%d\" Name=\"ElementNumber\" NumberOfComponents=\"3\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
        prec_bits,
    );
    if dummyzero != 0 {
        if info != 0 {
            printf(
                b"Creating dummy duplicate for first index to allow numbering from one\n\0"
                    .as_ptr() as *const c_char,
            );
        }
        fprintf(
            out,
            b"%12.6le %12.6le %12.6le \0".as_ptr() as *const c_char,
            *(*data).x.wrapping_offset(1),
            *(*data).y.wrapping_offset(1),
            *(*data).z.wrapping_offset(1),
        );
    }
    i = 1;
    while i <= noknots {
        fprintf(
            out,
            b"%12.6le %12.6le %12.6le \0".as_ptr() as *const c_char,
            *(*data).x.wrapping_offset(i as isize),
            *(*data).y.wrapping_offset(i as isize),
            *(*data).z.wrapping_offset(i as isize),
        );
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"      </Points>\n\0".as_ptr() as *const c_char);
    printf(b"Saving %d element topologies.\n\0".as_ptr() as *const c_char, noelements);
    fprintf(out, b"      <Cells>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"connectivity\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    i = 1;
    while i <= bulkelems {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        j = 0;
        while j < elemtype % 100 {
            ind[j as usize] =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize);
            j += 1;
        }
        ElmerToVtkIndx(elemtype, ind.as_mut_ptr());
        j = 0;
        while j < elemtype % 100 {
            fprintf(out, b"%d \0".as_ptr() as *const c_char, ind[j as usize] - di);
            j += 1;
        }
        i += 1;
    }
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created == FALSE {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                GetBoundaryElement(
                    i,
                    bound.wrapping_offset(j as isize),
                    data,
                    ind.as_mut_ptr(),
                    &mut elemtype as *mut c_int,
                );
                k = 0;
                while k < elemtype % 100 {
                    fprintf(out, b"%d \0".as_ptr() as *const c_char, ind[k as usize] - di);
                    k += 1;
                }
                i += 1;
            }
            j += 1;
        }
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"offsets\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    elemoffset = 0;
    i = 1;
    while i <= bulkelems {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        elemoffset += elemtype % 100;
        fprintf(out, b"%d \0".as_ptr() as *const c_char, elemoffset);
        i += 1;
    }
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created == FALSE {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                GetBoundaryElement(
                    i,
                    bound.wrapping_offset(j as isize),
                    data,
                    ind.as_mut_ptr(),
                    &mut elemtype as *mut c_int,
                );
                elemoffset += elemtype % 100;
                fprintf(out, b"%d \0".as_ptr() as *const c_char, elemoffset);
                i += 1;
            }
            j += 1;
        }
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"        <DataArray type=\"Int32\" Name=\"types\" NumberOfComponents=\"1\" format=\"ascii\">\n\0"
            .as_ptr() as *const c_char,
    );
    i = 1;
    while i <= bulkelems {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        vtktype = ElmerToVtkType(elemtype);
        fprintf(out, b"%d \0".as_ptr() as *const c_char, vtktype);
        i += 1;
    }
    if nobound != 0 {
        j = 0;
        while j < nobound {
            if (*bound.wrapping_offset(j as isize)).created == FALSE {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                GetBoundaryElement(
                    i,
                    bound.wrapping_offset(j as isize),
                    data,
                    ind.as_mut_ptr(),
                    &mut elemtype as *mut c_int,
                );
                vtktype = ElmerToVtkType(elemtype);
                fprintf(out, b"%d \0".as_ptr() as *const c_char, vtktype);
                i += 1;
            }
            j += 1;
        }
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    fprintf(out, b"        </DataArray>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"      </Cells>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"    </Piece>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"  </UnstructuredGrid>\n\0".as_ptr() as *const c_char);
    fprintf(out, b"</VTKFile>\n\0".as_ptr() as *const c_char);
    fclose(out);
    if info != 0 {
        printf(
            b"Saving of Elmer mesh to VTK format finished\n\0".as_ptr() as *const c_char,
        );
    }
    0
}
