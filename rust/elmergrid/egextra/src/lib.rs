// Faithful Rust transcription of elmergrid/src/egextra.c @ 9f6af2f85.
// Workspace layout: shared declarations live in eg-prelude; this crate
// appends only the unit body.
// Config scope: exe-only unit (not in elmergrid_core), no USE_MATC /
// USE_METIS / HAVE_UNISTD_H references — single archive, spliced into the
// ElmerGrid object only.
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
    fn dvector(nl: c_int, nh: c_int) -> *mut c_double;
    fn ivector(nl: c_int, nh: c_int) -> *mut c_int;
    fn free_ivector(v: *mut c_int, nl: c_int, nh: c_int);
    fn smallerror(error_text: *const c_char);
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
    fn GetSideInfo(
        cell: *mut CellType,
        cellno: c_int,
        side: c_int,
        element: c_int,
        elemind: *mut c_int,
    ) -> c_int;
    fn GetMaxElementDimension(data: *mut FemType) -> c_int;
    fn InitializeKnots(data: *mut FemType);
    fn AllocateKnots(data: *mut FemType);
    fn CreateNewNodes(data: *mut FemType, order: *mut c_int, material: c_int, newknots: c_int)
    -> c_int;
    fn CreateVariable(
        data: *mut FemType,
        variable: c_int,
        unknowns: c_int,
        value: Real,
        variablename: *const c_char,
        eorder: c_int,
    ) -> c_int;
}

// NOTE: the C GETLINE macro assigns the file-scope `ioptr`; it is
// write-only in this unit (never read back). Kept for shape fidelity.
static mut ioptr: *mut c_char = core::ptr::null_mut();

// SAFETY: file export; cell rows address valid cells (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveCellInfo(
    grid: *mut GridType,
    cell: *mut CellType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut i: c_int;
    let out: *mut FILE;
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"cell\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    fprintf(
        out,
        b"%-6s %-6s %-6s %-6s %-6s %-6s %-6s %-6s %-6s\n\0".as_ptr() as *const c_char,
        b"1st\0".as_ptr() as *const c_char,
        b"2nd\0".as_ptr() as *const c_char,
        b"last\0".as_ptr() as *const c_char,
        b"level\0".as_ptr() as *const c_char,
        b"center\0".as_ptr() as *const c_char,
        b"mat\0".as_ptr() as *const c_char,
        b"xlin\0".as_ptr() as *const c_char,
        b"ylin\0".as_ptr() as *const c_char,
        b"num\0".as_ptr() as *const c_char,
    );
    i = 1;
    while i <= (*grid).nocells {
        fprintf(
            out,
            b"%-6d %-6d %-6d %-6d %-6d %-6d %-6d %-6d %-6d\n\0".as_ptr() as *const c_char,
            (*cell.wrapping_offset(i as isize)).left1st,
            (*cell.wrapping_offset(i as isize)).left2nd,
            (*cell.wrapping_offset(i as isize)).leftlast,
            (*cell.wrapping_offset(i as isize)).levelwidth,
            (*cell.wrapping_offset(i as isize)).leftcenter,
            (*cell.wrapping_offset(i as isize)).material,
            (*cell.wrapping_offset(i as isize)).xlinear,
            (*cell.wrapping_offset(i as isize)).ylinear,
            (*cell.wrapping_offset(i as isize)).numbering,
        );
        i += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"The cell information was saved to file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}

// SAFETY: file export; sideind entries index valid knots (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveBoundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut i: c_int;
    // NOTE: initialized for the &mut handoff to GetElementSide; the callee
    // overwrites before any read, so the value is unobservable (same
    // rationale as egexport's elemtype).
    let mut sideelemtype: c_int = 0;
    let out: *mut FILE;
    if (*bound).created == 0 {
        printf(
            b"SaveBoundary: You tried to save a nonexistent boundary.\n\0".as_ptr()
                as *const c_char,
        );
        return 1;
    }
    if (*bound).nosides == 0 {
        return 0;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"bound\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    i = 1;
    while i <= (*bound).nosides {
        GetElementSide(
            *(*bound).parent.wrapping_offset(i as isize),
            *(*bound).side.wrapping_offset(i as isize),
            *(*bound).normal.wrapping_offset(i as isize),
            data,
            sideind.as_mut_ptr(),
            &mut sideelemtype,
        );
        fprintf(
            out,
            b"%-12.4le %-12.4le %-12.4le %-12.4le\n\0".as_ptr() as *const c_char,
            *(*data).x.wrapping_offset(sideind[0] as isize),
            *(*data).x.wrapping_offset(sideind[1] as isize),
            *(*data).y.wrapping_offset(sideind[0] as isize),
            *(*data).y.wrapping_offset(sideind[1] as isize),
        );
        i += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"Boundary information was saved to file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}

// SAFETY: chain walk over valid sides (C contract); `goto restart/jump/
// theend` become structured loops with a found-flag.
#[no_mangle]
pub unsafe extern "C" fn CreateBoundaryChain(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) -> c_int {
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let (mut i, mut side, mut size, mut indfirst, mut indsecond): (c_int, c_int, c_int, c_int, c_int);
    // NOTE: &mut-handoff init (see SaveBoundary note above).
    let mut sideelemtype: c_int = 0;
    let mut setchain: c_int;
    if (*bound).created == FALSE {
        if info != 0 {
            printf(
                b"CreateBoundaryChain: boundary not created!\n\0".as_ptr() as *const c_char,
            );
        }
        return 1;
    }
    if (*bound).nosides < 2 {
        if info != 0 {
            printf(
                b"CreateBoundaryChain: there must be at least 2 sides!\n\0".as_ptr()
                    as *const c_char,
            );
        }
        return 2;
    }
    if (*bound).echain == TRUE {
        // NOTE: `if(0)` diagnostic omitted (runtime-dead).
        return 3;
    }
    setchain = FALSE;
    // NOTE: `restart:` label target.
    loop {
        GetElementSide(
            *(*bound).parent.wrapping_offset(1),
            *(*bound).side.wrapping_offset(1),
            *(*bound).normal.wrapping_offset(1),
            data,
            sideind.as_mut_ptr(),
            &mut sideelemtype,
        );
        GetElementSide(
            *(*bound).parent.wrapping_offset(2),
            *(*bound).side.wrapping_offset(2),
            *(*bound).normal.wrapping_offset(2),
            data,
            sideind2.as_mut_ptr(),
            &mut sideelemtype,
        );
        if sideind[1] == sideind2[0] {
            indfirst = sideind[0];
            indsecond = sideind[1];
        } else if sideind[0] == sideind2[1] {
            indfirst = sideind[1];
            indsecond = sideind[0];
        } else {
            printf(
                b"CreateBoundaryChain: Impossible case!\n\0".as_ptr() as *const c_char,
            );
            indfirst = sideind[0];
            indsecond = sideind[1];
        }
        if setchain != 0 {
            *(*bound).chain.wrapping_offset(0) = indfirst;
            *(*bound).chain.wrapping_offset(1) = indsecond;
        }
        size = 1;
        side = 1;
        // NOTE: `for(;;)` with `jump:` (found-flag) and `theend:` (break).
        loop {
            let mut found = false;
            i = side + 1;
            while i <= (*bound).nosides {
                GetElementSide(
                    *(*bound).parent.wrapping_offset(i as isize),
                    *(*bound).side.wrapping_offset(i as isize),
                    *(*bound).normal.wrapping_offset(i as isize),
                    data,
                    sideind.as_mut_ptr(),
                    &mut sideelemtype,
                );
                if sideind[0] == indsecond {
                    indsecond = sideind[1];
                    found = true;
                    break;
                } else if sideind[1] == indsecond {
                    indsecond = sideind[0];
                    found = true;
                    break;
                }
                i += 1;
            }
            if !found {
                i = side - 1;
                while i >= 1 {
                    GetElementSide(
                        *(*bound).parent.wrapping_offset(i as isize),
                        *(*bound).side.wrapping_offset(i as isize),
                        *(*bound).normal.wrapping_offset(i as isize),
                        data,
                        sideind.as_mut_ptr(),
                        &mut sideelemtype,
                    );
                    if sideind[0] == indsecond {
                        indsecond = sideind[1];
                        found = true;
                        break;
                    } else if sideind[1] == indsecond {
                        indsecond = sideind[0];
                        found = true;
                        break;
                    }
                    i -= 1;
                }
            }
            if !found {
                // NOTE: `goto theend`.
                break;
            }
            // NOTE: `jump:` body.
            size += 1;
            if setchain != 0 {
                *(*bound).chain.wrapping_offset(size as isize) = indsecond;
            }
            side = i;
            if indsecond == indfirst {
                // NOTE: `goto theend`.
                break;
            }
        }
        // NOTE: `theend:` label target.
        if setchain == FALSE {
            if size != (*bound).nosides {
                printf(
                    b"CreateBoundaryChain: the boundary is not continuous (%d vs. %d)\n\0"
                        .as_ptr() as *const c_char,
                    size,
                    (*bound).nosides,
                );
            } else {
                printf(
                    b"CreateBoundaryChain: the boundary is continuous with %d nodes.\n\0"
                        .as_ptr() as *const c_char,
                    size + 1,
                );
            }
            (*bound).chain = ivector(0, size);
            (*bound).echain = TRUE;
            (*bound).chainsize = size;
            setchain = TRUE;
            // NOTE: `goto restart` via loop-continue.
            continue;
        }
        break;
    }
    0
}
// SAFETY: file exports; chain/dof indexes address valid entries (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveBoundariesChain(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut filename2: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let (mut i, mut j, mut k, mut ind, mut length, mut col): (c_int, c_int, c_int, c_int, c_int, c_int);
    let mut out: *mut FILE;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created != 0
            && (*bound.wrapping_offset(j as isize)).nosides > 0
        {
            CreateBoundaryChain(data, bound.wrapping_offset(j as isize), info);
            length = (*bound.wrapping_offset(j as isize)).chainsize;
            if length < 2 {
                j += 1;
                continue;
            }
            sprintf(
                filename.as_mut_ptr(),
                b"%s%d%s\0".as_ptr() as *const c_char,
                prefix,
                j,
                b".side\0".as_ptr() as *const c_char,
            );
            out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
            i = 0;
            while i <= length {
                ind = *(*bound.wrapping_offset(j as isize)).chain.wrapping_offset(i as isize);
                fprintf(
                    out,
                    b"%-10.4le %-10.4le %-6d \0".as_ptr() as *const c_char,
                    *(*data).x.wrapping_offset(ind as isize),
                    *(*data).y.wrapping_offset(ind as isize),
                    ind,
                );
                k = 0;
                while k < MAXDOFS as c_int {
                    if (*data).edofs[k as usize] == 1 {
                        fprintf(
                            out,
                            b"%-10.4le  \0".as_ptr() as *const c_char,
                            *(*data).dofs[k as usize].wrapping_offset(ind as isize),
                        );
                    }
                    if (*data).edofs[k as usize] == 2 {
                        fprintf(
                            out,
                            b"%-10.4le  %-10.4le  \0".as_ptr() as *const c_char,
                            *(*data).dofs[k as usize].wrapping_offset((2 * ind - 1) as isize),
                            *(*data).dofs[k as usize].wrapping_offset((2 * ind) as isize),
                        );
                    }
                    k += 1;
                }
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
                i += 1;
            }
            fclose(out);
            sprintf(
                filename2.as_mut_ptr(),
                b"%s%d%s\0".as_ptr() as *const c_char,
                prefix,
                j,
                b".sidetxt\0".as_ptr() as *const c_char,
            );
            out = fopen(filename2.as_ptr(), b"w\0".as_ptr() as *const c_char);
            fprintf(
                out,
                b"Degrees of freedom in file %s are as follows:\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
            if (*bound).coordsystem == COORD_CART2 {
                fprintf(out, b"col1: X coordinate\n\0".as_ptr() as *const c_char);
                fprintf(out, b"col2: Y coordinate\n\0".as_ptr() as *const c_char);
            } else if (*bound).coordsystem == COORD_AXIS {
                fprintf(out, b"col1: R coordinate\n\0".as_ptr() as *const c_char);
                fprintf(out, b"col2: Z coordinate\n\0".as_ptr() as *const c_char);
            } else if (*bound).coordsystem == COORD_POLAR {
                fprintf(out, b"col1: R coordinate\n\0".as_ptr() as *const c_char);
                fprintf(out, b"col2: F coordinate\n\0".as_ptr() as *const c_char);
            }
            fprintf(out, b"col3: node indices\n\0".as_ptr() as *const c_char);
            col = 3;
            k = 0;
            while k < MAXDOFS as c_int {
                if (*data).edofs[k as usize] == 1 {
                    col += 1;
                    fprintf(
                        out,
                        b"col%d: %s\n\0".as_ptr() as *const c_char,
                        col,
                        (*data).dofname[k as usize].as_ptr(),
                    );
                }
                if (*data).edofs[k as usize] == 2 {
                    col += 1;
                    fprintf(
                        out,
                        b"col%d: %s1\n\0".as_ptr() as *const c_char,
                        col,
                        (*data).dofname[k as usize].as_ptr(),
                    );
                    col += 1;
                    fprintf(
                        out,
                        b"col%d: %s2\n\0".as_ptr() as *const c_char,
                        col,
                        (*data).dofname[k as usize].as_ptr(),
                    );
                }
                k += 1;
            }
            fclose(out);
            if info != 0 {
                printf(
                    b"Boundary info was saved to files %s and %s.\n\0".as_ptr() as *const c_char,
                    filename.as_ptr(),
                    filename2.as_ptr(),
                );
            }
        }
        j += 1;
    }
    0
}

// SAFETY: file export; GetSideInfo/side indexes address valid cells (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveBoundaryForm(
    data: *mut FemType,
    cell: *mut CellType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut elemind: [c_int; 2] = [0; 2];
    let (mut sideknots, mut elemno, mut side, mut more): (c_int, c_int, c_int, c_int);
    // NOTE: &mut-handoff init (see SaveBoundary note above).
    let mut sideelemtype: c_int = 0;
    let mut no: c_int;
    let out: *mut FILE;
    if (*data).created == FALSE {
        printf(
            b"SaveBoundaryForm: structure FemType not created\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    sideknots = 0;
    // NOTE: C stores more = FALSE here; the GetSideInfo assignment below
    // dominates every read, so the first store is dropped.
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"boundary\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    no = 1;
    while no <= (*data).nocells {
        side = 0;
        while side < 4 {
            if (*cell.wrapping_offset(no as isize)).material
                != (*cell.wrapping_offset(no as isize)).boundary[side as usize]
            {
                elemno = 0;
                loop {
                    elemno += 1;
                    sideknots += 1;
                    more = GetSideInfo(
                        cell,
                        no,
                        side,
                        elemno,
                        elemind.as_mut_ptr(),
                    );
                    GetElementSide(
                        elemind[0],
                        side,
                        1,
                        data,
                        sideind.as_mut_ptr(),
                        &mut sideelemtype,
                    );
                    fprintf(
                        out,
                        b"%-12.4le %-12.4le %-12.4le %-12.4le\n\0".as_ptr() as *const c_char,
                        *(*data).x.wrapping_offset(sideind[0] as isize),
                        *(*data).x.wrapping_offset(sideind[1] as isize),
                        *(*data).y.wrapping_offset(sideind[0] as isize),
                        *(*data).y.wrapping_offset(sideind[1] as isize),
                    );
                    if more == 0 {
                        break;
                    }
                }
            }
            side += 1;
        }
        no += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"%d boundaries between materials were saved to file %s.\n\0".as_ptr()
                as *const c_char,
            sideknots,
            filename.as_ptr(),
        );
    }
    0
}

// SAFETY: file export; knot indexes address valid nodes (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveBoundaryLine(
    data: *mut FemType,
    direction: c_int,
    c0: Real,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let (mut k, mut no, mut points): (c_int, c_int, c_int);
    let (mut c, mut c1, eps): (Real, Real, Real);
    let out: *mut FILE;
    if (*data).created == FALSE {
        printf(
            b"SaveBoundaryLine: structure FemType not created\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    eps = 1.0e-6;
    points = 0;
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"line\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    c1 = *(*data).x.wrapping_offset(1);
    no = 1;
    while no <= (*data).noknots {
        if direction > 0 {
            c = *(*data).x.wrapping_offset(no as isize);
        } else {
            c = *(*data).y.wrapping_offset(no as isize);
        }
        if fabs(c - c0) < fabs(c1 - c0) {
            c1 = c;
        }
        no += 1;
    }
    no = 1;
    while no <= (*data).noknots {
        if direction > 0 {
            c = *(*data).x.wrapping_offset(no as isize);
        } else {
            c = *(*data).y.wrapping_offset(no as isize);
        }
        if fabs(c - c1) < eps {
            if direction > 0 {
                fprintf(
                    out,
                    b"%-12.7le %-12.7le \0".as_ptr() as *const c_char,
                    c,
                    *(*data).y.wrapping_offset(no as isize),
                );
            } else {
                fprintf(
                    out,
                    b"%-12.7le %-12.7le \0".as_ptr() as *const c_char,
                    *(*data).x.wrapping_offset(no as isize),
                    c,
                );
            }
            k = 0;
            while k < MAXDOFS as c_int {
                if (*data).edofs[k as usize] == 1 {
                    fprintf(
                        out,
                        b"%-12.7le \0".as_ptr() as *const c_char,
                        *(*data).dofs[k as usize].wrapping_offset(no as isize),
                    );
                }
                if (*data).edofs[k as usize] == 2 {
                    fprintf(
                        out,
                        b"%-12.7le %-12.7le \0".as_ptr() as *const c_char,
                        *(*data).dofs[k as usize].wrapping_offset((2 * no - 1) as isize),
                        *(*data).dofs[k as usize].wrapping_offset((2 * no) as isize),
                    );
                }
                k += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
            points += 1;
        }
        no += 1;
    }
    if info != 0 {
        printf(
            b"Line (c=%.3lg) with %d nodes was saved to file %s.\n\0".as_ptr() as *const c_char,
            c1,
            points,
            filename.as_ptr(),
        );
    }
    fclose(out);
    0
}

// SAFETY: file export; side indexes address valid cells (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveSubcellForm(
    data: *mut FemType,
    cell: *mut CellType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut elemind: [c_int; 2] = [0; 2];
    let (mut more, mut sideknots, mut elemno, mut side, mut i): (c_int, c_int, c_int, c_int, c_int);
    let (mut no, mut nosidenodes): (c_int, c_int);
    // NOTE: &mut-handoff init (see SaveBoundary note above).
    let mut sideelemtype: c_int = 0;
    let out: *mut FILE;
    sideknots = 0;
    // NOTE: C stores more = FALSE here; dropped (GetSideInfo dominates).
    if (*data).created == FALSE {
        printf(
            b"SaveSubcellForm: structure FemType not created\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"subcell\0".as_ptr() as *const c_char,
    );
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    no = 1;
    while no <= (*data).nocells {
        side = 0;
        while side < 4 {
            if (*cell.wrapping_offset(no as isize)).boundary[side as usize] != 0 {
                elemno = 0;
                loop {
                    elemno += 1;
                    sideknots += 1;
                    more = GetSideInfo(
                        cell,
                        no,
                        side,
                        elemno,
                        elemind.as_mut_ptr(),
                    );
                    GetElementSide(
                        elemind[0],
                        side,
                        1,
                        data,
                        sideind.as_mut_ptr(),
                        &mut sideelemtype,
                    );
                    nosidenodes = sideelemtype % 100;
                    i = 0;
                    while i < nosidenodes {
                        fprintf(
                            out,
                            b"%-12.4le %-12.4le %-8d\n\0".as_ptr() as *const c_char,
                            *(*data).x.wrapping_offset(sideind[i as usize] as isize),
                            *(*data).y.wrapping_offset(sideind[i as usize] as isize),
                            sideind[i as usize],
                        );
                        i += 1;
                    }
                    if more == 0 {
                        break;
                    }
                }
            }
            side += 1;
        }
        no += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"There are %d sideknots in the elements.\n\0".as_ptr() as *const c_char,
            sideknots,
        );
    }
    if info != 0 {
        printf(
            b"The positions of the sideknots were saved in file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    0
}

// SAFETY: read-only inspection over valid elements (C contract).
#[no_mangle]
pub unsafe extern "C" fn InspectElement(data: *mut FemType, idx: c_int) -> c_int {
    let (elemtype, nonodes, mut i, mut j, mut k): (c_int, c_int, c_int, c_int, c_int);
    let (x, y, z): (*mut Real, *mut Real, *mut Real);
    let (mut ds, mut minds, mut xc, mut yc, mut zc): (Real, Real, Real, Real, Real);
    let (mut x0, mut y0, mut z0): (Real, Real, Real);
    let (mut ii, mut jj, mut mini, mut minj, mut kk): (c_int, c_int, c_int, c_int, c_int);
    elemtype = *(*data).elementtypes.wrapping_offset(idx as isize);
    printf(
        b"Inspecting element %d of type %d\n\0".as_ptr() as *const c_char,
        idx,
        elemtype,
    );
    nonodes = elemtype % 100;
    x = (*data).x;
    y = (*data).y;
    z = (*data).z;
    k = 0;
    while k < nonodes {
        kk = *(*(*data).topology.wrapping_offset(idx as isize)).wrapping_offset(k as isize);
        x0 = *x.wrapping_offset(kk as isize);
        y0 = *y.wrapping_offset(kk as isize);
        z0 = *z.wrapping_offset(kk as isize);
        mini = -1;
        minj = -1;
        minds = 1.0e20;
        i = 0;
        while i < nonodes {
            ii = *(*(*data).topology.wrapping_offset(idx as isize)).wrapping_offset(i as isize);
            j = i + 1;
            while j < nonodes {
                jj = *(*(*data).topology.wrapping_offset(idx as isize))
                    .wrapping_offset(j as isize);
                xc = (*x.wrapping_offset(ii as isize) + *x.wrapping_offset(jj as isize)) / 2.0;
                yc = (*y.wrapping_offset(ii as isize) + *y.wrapping_offset(jj as isize)) / 2.0;
                zc = (*z.wrapping_offset(ii as isize) + *z.wrapping_offset(jj as isize)) / 2.0;
                ds = sqrt(
                    pow(x0 - xc, 2.0) + pow(y0 - yc, 2.0) + pow(z0 - zc, 2.0),
                );
                if mini == -1 || ds < minds {
                    mini = i;
                    minj = j;
                    minds = ds;
                }
                j += 1;
            }
            i += 1;
        }
        printf(
            b"k=%d mini=%d minj=%d ds=%.3le\n\0".as_ptr() as *const c_char,
            k + 1,
            mini + 1,
            minj + 1,
            minds,
        );
        k += 1;
    }
    0
}
// SAFETY: file import; fscanf consumes input exactly as C (return codes
// ignored there too — hence _iostat); `goto end` becomes break.
#[no_mangle]
pub unsafe extern "C" fn LoadSolutionElmer(
    data: *mut FemType,
    results: c_int,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut line: [c_char; MAXLINESIZE] = [0; MAXLINESIZE];
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut text: [c_char; MAXNAMESIZE] = [0; MAXNAMESIZE];
    // NOTE: sscanf/fscanf targets carry an init that is dead by static
    // analysis (the calls overwrite before any read, and later by-value
    // stores dominate); the init exists solely for the &mut handoff
    // (E0381), so it is allowed rather than deleted.
    #[allow(unused_assignments)]
    let (mut noknots, mut noelements, mut novctrs): (c_int, c_int, c_int) = (0, 0, 0);
    let mut open: c_int;
    #[allow(unused_assignments)]
    let (mut timesteps, mut i, mut j, mut k): (c_int, c_int, c_int, c_int) = (0, 0, 0, 0);
    let mut grp: c_int;
    // NOTE: assigned by fscanf, never read (input consumption is the effect).
    let mut _iostat: c_int = 0;
    let mut _r: Real = 0.0;
    let inn: *mut FILE;
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"ep\0".as_ptr() as *const c_char,
    );
    inn = fopen(filename.as_ptr(), b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"LoadSolutionElmer: The opening of the Elmer-file %s wasn't successful!\n\0"
                .as_ptr() as *const c_char,
            filename.as_ptr(),
        );
        return 1;
    } else {
        printf(
            b"Loading Elmer data from %s\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    InitializeKnots(data);
    // NOTE: GETLINE macro (assigns the write-only static ioptr in C).
    ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, inn);
    sscanf(
        line.as_ptr(),
        b"%d %d %d %d\0".as_ptr() as *const c_char,
        &mut noknots,
        &mut noelements,
        &mut novctrs,
        &mut timesteps,
    );
    (*data).dim = 3;
    (*data).maxnodes = MAXNODESD2 as c_int;
    (*data).noknots = noknots;
    (*data).noelements = noelements;
    (*data).timesteps = timesteps;
    if timesteps > 1 {
        printf(
            b"LoadSolutionElmer: The subroutine may crash with %d timesteps\n\0".as_ptr()
                as *const c_char,
            timesteps,
        );
    }
    if timesteps < 1 {
        timesteps = 1;
    }
    if info != 0 {
        printf(
            b"Allocating for %d knots and %d elements.\n\0".as_ptr() as *const c_char,
            noknots,
            noelements,
        );
    }
    AllocateKnots(data);
    if results != 0 {
        if timesteps > 1 {
            (*data).times = dvector(0, timesteps - 1);
        }
        i = 1;
        while i <= novctrs {
            sprintf(text.as_mut_ptr(), b"var%d\0".as_ptr() as *const c_char, i);
            CreateVariable(data, i, timesteps, 0.0, text.as_ptr(), FALSE);
            i += 1;
        }
    }
    if info != 0 {
        printf(
            b"Reading %d coordinates.\n\0".as_ptr() as *const c_char,
            noknots,
        );
    }
    i = 1;
    while i <= noknots {
        ioptr = fgets(line.as_mut_ptr(), MAXLINESIZE as c_int, inn);
        sscanf(
            line.as_ptr(),
            b"%le %le %le\0".as_ptr() as *const c_char,
            &mut *(*data).x.wrapping_offset(i as isize),
            &mut *(*data).y.wrapping_offset(i as isize),
            &mut *(*data).z.wrapping_offset(i as isize),
        );
        i += 1;
    }
    if info != 0 {
        printf(
            b"Reading %d element topologies.\n\0".as_ptr() as *const c_char,
            noelements,
        );
    }
    grp = 0;
    open = FALSE;
    i = 1;
    while i <= noelements {
        _iostat = fscanf(
            inn,
            b"%s\0".as_ptr() as *const c_char,
            text.as_mut_ptr(),
        );
        if !strstr(text.as_ptr(), b"#group\0".as_ptr() as *const c_char).is_null() {
            grp += 1;
            printf(b"Starting a new element group\n\0".as_ptr() as *const c_char);
            _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
            _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
            open = TRUE;
        }
        if !strstr(text.as_ptr(), b"#end\0".as_ptr() as *const c_char).is_null() {
            printf(b"Ending an element group\n\0".as_ptr() as *const c_char);
            _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
            open = FALSE;
        }
        _iostat = fscanf(
            inn,
            b"%d\0".as_ptr() as *const c_char,
            &mut *(*data).elementtypes.wrapping_offset(i as isize),
        );
        *(*data).material.wrapping_offset(i as isize) = grp;
        j = 0;
        while j < *(*data).elementtypes.wrapping_offset(i as isize) % 100 {
            _iostat = fscanf(
                inn,
                b"%d\0".as_ptr() as *const c_char,
                &mut *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize),
            );
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) += 1;
            j += 1;
        }
        i += 1;
    }
    if open != 0 {
        loop {
            _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
            if !strstr(text.as_ptr(), b"#end\0".as_ptr() as *const c_char).is_null() {
                break;
            }
        }
        _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
        printf(b"Ending an element group\n\0".as_ptr() as *const c_char);
        // NOTE: C stores open = FALSE here, but `open` is never read after
        // this point; dropped for warning-freedom.
    }
    if results == 0 {
        return 0;
    }
    if info != 0 {
        printf(
            b"Reading %d degrees of freedom for %d knots.\n\0".as_ptr() as *const c_char,
            novctrs,
            noknots,
        );
    }
    if timesteps <= 1 {
        i = 1;
        while i <= noknots {
            j = 1;
            while j <= novctrs {
                _iostat = fscanf(
                    inn,
                    b"%le\0".as_ptr() as *const c_char,
                    &mut *(*data).dofs[j as usize].wrapping_offset(i as isize),
                );
                j += 1;
            }
            i += 1;
        }
    } else {
        k = 0;
        while k < timesteps {
            _iostat = fscanf(inn, b"%s\0".as_ptr() as *const c_char, text.as_mut_ptr());
            // NOTE: `goto end` (exits the k loop with the stream positioned).
            if _iostat < 0 {
                break;
            }
            _iostat = fscanf(
                inn,
                b"%d\0".as_ptr() as *const c_char,
                &mut i,
            );
            _iostat = fscanf(
                inn,
                b"%d\0".as_ptr() as *const c_char,
                &mut j,
            );
            _iostat = fscanf(
                inn,
                b"%le\0".as_ptr() as *const c_char,
                &mut _r,
            );
            // NOTE: `if(0)` diagnostic omitted (runtime-dead).
            i = 1;
            while i <= noknots {
                j = 1;
                while j <= novctrs {
                    _iostat = fscanf(
                        inn,
                        b"%le\0".as_ptr() as *const c_char,
                        &mut *(*data).dofs[j as usize]
                            .wrapping_offset((k * noknots + i) as isize),
                    );
                    j += 1;
                }
                i += 1;
            }
            k += 1;
        }
    }
    // NOTE: `end:` label target.
    (*data).timesteps = timesteps;
    fclose(inn);
    0
}

// SAFETY: file export; material/bc indexes mirror C's unguarded writes.
#[no_mangle]
pub unsafe extern "C" fn SaveSolutionElmer(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nobound: c_int,
    prefix: *mut c_char,
    decimals: c_int,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut outstyle: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut ind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let (mut material, noknots, noelements, bulkelems, mut novctrs): (
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    );
    let (mut sideelems, mut elemtype, mut boundtype): (c_int, c_int, c_int);
    // NOTE: &mut-handoff init (see SaveBoundary note above).
    let mut sideelemtype: c_int = 0;
    let (mut i, mut j, mut k, mut l, mut nodesd1, mut timesteps, mut nodesd2): (
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    );
    // NOTE: assigned by the partition branch before any read; the null init
    // C implies (indeterminate) is dropped for warning-freedom.
    let rpart: *mut Real;
    let out: *mut FILE;
    if (*data).created == 0 {
        printf(
            b"SaveSolutionElmer: You tried to save points that were never created.\n\0"
                .as_ptr() as *const c_char,
        );
        return 1;
    }
    if (*data).partitionexist != 0 {
        l = 0;
        loop {
            l += 1;
            if (*data).edofs[l as usize] == 0 {
                break;
            }
        }
        CreateVariable(
            data,
            l,
            1,
            0.0,
            b"Partition\0".as_ptr() as *const c_char,
            FALSE,
        );
        rpart = (*data).dofs[l as usize];
        i = 1;
        while i <= (*data).noknots {
            *rpart.wrapping_offset(i as isize) =
                1.0 * *(*data).nodepart.wrapping_offset(i as isize) as Real;
            i += 1;
        }
    }
    if (*data).variables == 0 {
        printf(
            b"SaveSolutionElmer: there are no dofs to save!\n\0".as_ptr() as *const c_char,
        );
        return 2;
    }
    sideelems = 0;
    if nobound != 0 {
        i = 0;
        while i < nobound {
            if (*bound.wrapping_offset(i as isize)).created != 0 {
                sideelems += (*bound.wrapping_offset(i as isize)).nosides;
            }
            i += 1;
        }
    }
    noknots = (*data).noknots;
    bulkelems = (*data).noelements;
    if nobound != 0 {
        noelements = bulkelems + sideelems;
    } else {
        noelements = bulkelems;
    }
    timesteps = (*data).timesteps;
    if timesteps < 1 {
        timesteps = 1;
    }
    novctrs = 0;
    i = 0;
    while i < MAXDOFS as c_int {
        if (*data).edofs[i as usize] == 1 {
            novctrs += 1;
        }
        if (*data).edofs[i as usize] == 2 {
            novctrs += 3;
        }
        if (*data).edofs[i as usize] == 3 {
            novctrs += 3;
        }
        i += 1;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"ep\0".as_ptr() as *const c_char,
    );
    if info != 0 {
        printf(
            b"Saving ElmerPost data to %s.\n\0".as_ptr() as *const c_char,
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
    fprintf(
        out,
        b"%d %d %d %d\0".as_ptr() as *const c_char,
        noknots,
        noelements,
        novctrs,
        timesteps,
    );
    i = 0;
    while i < MAXDOFS as c_int {
        if (*data).edofs[i as usize] == 1 {
            fprintf(
                out,
                b" scalar: %s\0".as_ptr() as *const c_char,
                (*data).dofname[i as usize].as_ptr(),
            );
        } else if (*data).edofs[i as usize] > 1 {
            fprintf(
                out,
                b" vector: %s\0".as_ptr() as *const c_char,
                (*data).dofname[i as usize].as_ptr(),
            );
        }
        i += 1;
    }
    fprintf(out, b"\n\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving %d node coordinates.\n\0".as_ptr() as *const c_char,
            noknots,
        );
    }
    sprintf(
        outstyle.as_mut_ptr(),
        b"%%.%dg %%.%dg %%.%dg\n\0".as_ptr() as *const c_char,
        decimals,
        decimals,
        decimals,
    );
    i = 1;
    while i <= noknots {
        fprintf(
            out,
            outstyle.as_ptr(),
            *(*data).x.wrapping_offset(i as isize),
            *(*data).y.wrapping_offset(i as isize),
            *(*data).z.wrapping_offset(i as isize),
        );
        i += 1;
    }
    printf(b"Saving %d bulk element topologies.\n\0".as_ptr() as *const c_char, bulkelems);
    i = 1;
    while i <= bulkelems {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        material = *(*data).material.wrapping_offset(i as isize);
        if (*data).bodynamesexist != 0 {
            fprintf(
                out,
                b"body_%d_%s %d \0".as_ptr() as *const c_char,
                material,
                (*data).bodyname[material as usize],
                elemtype,
            );
        } else if elemtype / 100 > 4 {
            fprintf(out, b"vol%d %d \0".as_ptr() as *const c_char, material, elemtype);
        } else if elemtype / 100 > 2 {
            fprintf(out, b"surf%d %d \0".as_ptr() as *const c_char, material, elemtype);
        } else if elemtype / 100 > 1 {
            fprintf(out, b"line%d %d \0".as_ptr() as *const c_char, material, elemtype);
        } else {
            fprintf(out, b"pnt%d %d \0".as_ptr() as *const c_char, material, elemtype);
        }
        nodesd2 = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
        j = 0;
        while j < nodesd2 {
            fprintf(
                out,
                b"%d \0".as_ptr() as *const c_char,
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize) - 1,
            );
            j += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    if nobound != 0 {
        printf(
            b"Saving %d side element topologies.\n\0".as_ptr() as *const c_char,
            sideelems,
        );
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
                    &mut sideelemtype,
                );
                boundtype =
                    *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
                if (*data).boundarynamesexist != 0 {
                    fprintf(
                        out,
                        b"bc_%d_%s %d \0".as_ptr() as *const c_char,
                        boundtype,
                        (*data).boundaryname[boundtype as usize],
                        sideelemtype,
                    );
                } else if sideelemtype / 100 > 2 {
                    fprintf(
                        out,
                        b"bcside%d %d \0".as_ptr() as *const c_char,
                        boundtype,
                        sideelemtype,
                    );
                } else if sideelemtype / 100 > 1 {
                    fprintf(
                        out,
                        b"bcline%d %d \0".as_ptr() as *const c_char,
                        boundtype,
                        sideelemtype,
                    );
                } else {
                    fprintf(out, b"bcpnt%d %d \0".as_ptr() as *const c_char, boundtype, sideelemtype);
                }
                nodesd1 = sideelemtype % 100;
                k = 0;
                while k < nodesd1 {
                    fprintf(out, b"%d \0".as_ptr() as *const c_char, ind[k as usize] - 1);
                    k += 1;
                }
                fprintf(out, b"\n\0".as_ptr() as *const c_char);
                i += 1;
            }
            j += 1;
        }
    }
    printf(
        b"Saving %d degrees of freedom for each knot.\n\0".as_ptr() as *const c_char,
        novctrs,
    );
    k = 0;
    while k < timesteps {
        i = 1;
        while i <= noknots {
            j = 0;
            while j < MAXDOFS as c_int {
                if (*data).edofs[j as usize] == 1 {
                    fprintf(
                        out,
                        b"%.6lg \0".as_ptr() as *const c_char,
                        *(*data).dofs[j as usize]
                            .wrapping_offset((k * noknots + i) as isize),
                    );
                }
                if (*data).edofs[j as usize] == 2 {
                    fprintf(
                        out,
                        b"%.6lg %.6lg 0.0 \0".as_ptr() as *const c_char,
                        *(*data).dofs[j as usize]
                            .wrapping_offset((2 * (k * noknots + i) - 1) as isize),
                        *(*data).dofs[j as usize]
                            .wrapping_offset((2 * (k * noknots + i)) as isize),
                    );
                }
                if (*data).edofs[j as usize] == 3 {
                    fprintf(
                        out,
                        b"%.6lg %.6lg %.6lg \0".as_ptr() as *const c_char,
                        *(*data).dofs[j as usize]
                            .wrapping_offset((3 * (k * noknots + i) - 2) as isize),
                        *(*data).dofs[j as usize]
                            .wrapping_offset((3 * (k * noknots + i) - 1) as isize),
                        *(*data).dofs[j as usize]
                            .wrapping_offset((3 * (k * noknots + i)) as isize),
                    );
                }
                j += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
            i += 1;
        }
        k += 1;
    }
    fclose(out);
    0
}
// SAFETY: trivial file export over valid structs (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveSizeInfo(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let (mut nosides, mut j): (c_int, c_int);
    let out: *mut FILE;
    if (*data).created == 0 {
        printf(
            b"You tried to save points that were never created.\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    nosides = 0;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        nosides += (*bound.wrapping_offset(j as isize)).nosides;
        j += 1;
    }
    AddExtension(
        prefix,
        filename.as_mut_ptr(),
        b"size\0".as_ptr() as *const c_char,
    );
    if info != 0 {
        printf(
            b"Saving size info into file %s.\n\0".as_ptr() as *const c_char,
            filename.as_ptr(),
        );
    }
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, (*data).noknots);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, (*data).noelements);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, nosides);
    fprintf(out, b"%d\n\0".as_ptr() as *const c_char, (*data).nopartitions);
    fclose(out);
    0
}

// SAFETY: read-only mesh walk; Ivector ranges address valid nodes.
#[no_mangle]
pub unsafe extern "C" fn MeshPieces(
    data: *mut FemType,
    nomesh: c_int,
    nomeshes: c_int,
    _info: c_int,
) -> c_int {
    let (mut i, mut j, mut k, n): (c_int, c_int, c_int, c_int);
    let (mut min_index, mut max_index, mut no_pieces, mut no_corners): (c_int, c_int, c_int, c_int);
    let mesh_piece: *mut c_int;
    // NOTE: C initializes PiecePerm to NULL; the store is never read (the
    // free runs only after the Ivector assignment), so it is dropped.
    let piece_perm: *mut c_int;
    let mut indexes: [c_int; 100] = [0; 100];
    // NOTE: C increments Loop but only reads it in a commented-out printf.
    let mut _loop: c_int;
    let mut ready: c_int;
    if nomeshes > 1 {
        printf(
            b"Calculate Mesh Pieces in mesh[%d] of [%d] meshes:\n\0".as_ptr() as *const c_char,
            nomesh,
            nomeshes,
        );
    } else {
        printf(b"Calculate Mesh Pieces in mesh:\n\0".as_ptr() as *const c_char);
    }
    n = (*data).noknots;
    mesh_piece = ivector(1, n);
    i = 1;
    while i <= n {
        *mesh_piece.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    i = 1;
    while i <= (*data).noelements {
        no_corners = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
        j = 0;
        while j < no_corners {
            *mesh_piece.wrapping_offset(
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                    as isize,
            ) = 1;
            j += 1;
        }
        i += 1;
    }
    j = 0;
    i = 1;
    while i <= n {
        if *mesh_piece.wrapping_offset(i as isize) > 0 {
            j += 1;
            *mesh_piece.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    if n > j {
        printf(
            b"Number of non-body (hanging) nodes in mesh is %d\n\0".as_ptr() as *const c_char,
            n - j,
        );
        printf(
            b"Consider running ElmerGrid with -autoclean command\n\0".as_ptr() as *const c_char,
        );
    }
    ready = FALSE;
    _loop = 0;
    while ready == 0 {
        ready = TRUE;
        i = 1;
        while i <= (*data).noelements {
            max_index = 0;
            min_index = n;
            no_corners = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
            j = 0;
            while j < no_corners {
                indexes[j as usize] =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize);
                j += 1;
            }
            j = 0;
            while j < no_corners {
                k = 0;
                while k < no_corners {
                    if max_index < *mesh_piece.wrapping_offset(indexes[k as usize] as isize) {
                        max_index = *mesh_piece.wrapping_offset(indexes[k as usize] as isize);
                    }
                    if min_index >= *mesh_piece.wrapping_offset(indexes[k as usize] as isize) {
                        min_index = *mesh_piece.wrapping_offset(indexes[k as usize] as isize);
                    }
                    k += 1;
                }
                if max_index > min_index {
                    *mesh_piece.wrapping_offset(indexes[j as usize] as isize) = min_index;
                    ready = FALSE;
                }
                j += 1;
            }
            i += 1;
        }
        _loop += 1;
    }
    max_index = 0;
    i = 1;
    while i <= n {
        if *mesh_piece.wrapping_offset(i as isize) > max_index {
            max_index = *mesh_piece.wrapping_offset(i as isize);
        }
        i += 1;
    }
    if max_index == 1 {
        no_pieces = 1;
    } else {
        no_pieces = 0;
        piece_perm = ivector(1, max_index);
        i = 1;
        while i <= max_index {
            *piece_perm.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        i = 1;
        while i <= n {
            j = *mesh_piece.wrapping_offset(i as isize);
            if j == 0 {
                i += 1;
                continue;
            }
            if *piece_perm.wrapping_offset(j as isize) == 0 {
                no_pieces += 1;
                *piece_perm.wrapping_offset(j as isize) = no_pieces;
            }
            i += 1;
        }
        free_ivector(piece_perm, 1, max_index);
    }
    if no_pieces == 1 {
        printf(
            b"There is a single piece in the mesh, so the mesh is conforming.\n\0".as_ptr()
                as *const c_char,
        );
    } else {
        printf(
            b"Number of separate pieces in mesh is %d\n\0".as_ptr() as *const c_char,
            no_pieces,
        );
        printf(
            b"The mesh is non-conforming. If not expecting a non-conforming\n\0".as_ptr()
                as *const c_char,
        );
        printf(b"mesh, then refer to the Elmer User Forum for help.\n\0".as_ptr() as *const c_char);
    }
    free_ivector(mesh_piece, 1, n);
    0
}
// SAFETY: mesh export with chdir round-trip; material/bc indexes mirror
// C's unguarded writes (see SaveFidapOutput note in egexport).
#[no_mangle]
pub unsafe extern "C" fn SaveElmerInputFemBem(
    data: *mut FemType,
    bound: *mut BoundaryType,
    prefix: *mut c_char,
    decimals: c_int,
    info: c_int,
) -> c_int {
    let mut filename: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut outstyle: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut directoryname: [c_char; MAXFILESIZE] = [0; MAXFILESIZE];
    let mut ind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut bodyperm: [c_int; MAXBODIES] = [0; MAXBODIES];
    let mut bcperm: [c_int; MAXBCS] = [0; MAXBCS];
    let mut bulktypes: [c_int; 828] = [0; 828];
    let mut sidetypes: [c_int; 828] = [0; 828];
    let (noknots, noelements, mut material): (c_int, c_int, c_int);
    let (mut sumsides, mut elemtype, fail, mut nobulkelements, mut bctype): (
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    );
    // NOTE: &mut-handoff init (see SaveBoundary note above).
    let mut sideelemtype: c_int = 0;
    let (mut nodesd1, mut nodesd2, newtype): (c_int, c_int, c_int);
    let (mut elemdim, maxelemdim, mut cdstat): (c_int, c_int, c_int);
    let (mut i, mut j, mut k, mut l, mut tottypes): (c_int, c_int, c_int, c_int, c_int);
    let mut out: *mut FILE;
    if (*data).created == 0 {
        printf(
            b"You tried to save points that were never created.\n\0".as_ptr() as *const c_char,
        );
        return 1;
    }
    if (*data).nodeconnectexist != 0 {
        smallerror(
            b"Connectivity data is not saved in the FEM/BEM version\0".as_ptr() as *const c_char,
        );
    }
    if (*data).nopartitions > 1 {
        smallerror(
            b"Partitioning data is not saved in the FEM/BEM version\0".as_ptr() as *const c_char,
        );
    }
    noelements = (*data).noelements;
    noknots = (*data).noknots;
    // NOTE: C stores sumsides = 0 here and again before the boundary loop;
    // the first store is never read (kept single for warning-freedom).
    i = 0;
    while i <= MAXELEMENTTYPE {
        bulktypes[i as usize] = 0;
        sidetypes[i as usize] = 0;
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
    cdstat = chdir(directoryname.as_ptr());
    if cdstat != 0 {
        // NOTE: MINGW32 undefined on this target; the mkdir(path,0750) arm
        // is compiled exactly as in C.
        fail = mkdir(directoryname.as_ptr(), 0o750);
        if fail != 0 {
            printf(
                b"Could not create a result directory!\n\0".as_ptr() as *const c_char,
            );
            return 1;
        } else {
            // NOTE: C stores the re-chdir status into cdstat, but it is
            // never read (the final chdir overwrites first); the call is
            // kept for its directory effect, the store dropped.
            chdir(directoryname.as_ptr());
        }
    } else {
        printf(b"Reusing an existing directory\n\0".as_ptr() as *const c_char);
    }
    sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, b"mesh.nodes\0".as_ptr() as *const c_char);
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving %d coordinates to %s.\n\0".as_ptr() as *const c_char,
            noknots,
            filename.as_ptr(),
        );
    }
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 2;
    }
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
    fclose(out);
    sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, b"mesh.elements\0".as_ptr() as *const c_char);
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 3;
    }
    maxelemdim = GetMaxElementDimension(data);
    nobulkelements = 0;
    i = 0;
    while i < MAXBODIES as c_int {
        bodyperm[i as usize] = FALSE;
        i += 1;
    }
    i = 1;
    while i <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        elemdim = GetElementDimension(elemtype);
        material = *(*data).material.wrapping_offset(i as isize);
        // SAFETY: C indexes bodyperm[material] unguarded (same OOB-mirror
        // rationale as egexport's elemcodes write).
        if elemdim == maxelemdim {
            *bodyperm.as_mut_ptr().wrapping_offset(material as isize) = 1;
        } else {
            *bodyperm.as_mut_ptr().wrapping_offset(material as isize) = -1;
        }
        i += 1;
    }
    j = 0;
    k = 0;
    i = 0;
    while i < MAXBODIES as c_int {
        if bodyperm[i as usize] > 0 {
            j += 1;
            bodyperm[i as usize] = j;
        }
        if bodyperm[i as usize] < 0 {
            k -= 1;
            bodyperm[i as usize] = k;
        }
        i += 1;
    }
    i = 1;
    while i <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        elemdim = GetElementDimension(elemtype);
        if elemdim < maxelemdim {
            i += 1;
            continue;
        }
        nobulkelements += 1;
        material = *(*data).material.wrapping_offset(i as isize);
        // SAFETY: OOB-mirror read (see write above).
        material = *bodyperm.as_mut_ptr().wrapping_offset(material as isize);
        fprintf(
            out,
            b"%d %d %d\0".as_ptr() as *const c_char,
            i,
            material,
            elemtype,
        );
        bulktypes[elemtype as usize] += 1;
        nodesd2 = elemtype % 100;
        j = 0;
        while j < nodesd2 {
            fprintf(
                out,
                b" %d\0".as_ptr() as *const c_char,
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize),
            );
            j += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"Saving %d (of %d) body elements to mesh.boundary\n\0".as_ptr() as *const c_char,
            nobulkelements,
            noelements,
        );
    }
    sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, b"mesh.boundary\0".as_ptr() as *const c_char);
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 4;
    }
    sumsides = 0;
    // NOTE: C stores newtype = 0 here; the `newtype = j` below dominates
    // every read, so the first store is dropped for warning-freedom.
    i = 0;
    while i < MAXBCS as c_int {
        bcperm[i as usize] = 0;
        i += 1;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            // SAFETY: OOB-mirror write (boundary types index bcperm raw in C).
            *bcperm.as_mut_ptr().wrapping_offset(
                *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize) as isize,
            ) = TRUE;
            i += 1;
        }
        j += 1;
    }
    j = 0;
    i = 0;
    while i < MAXBCS as c_int {
        if bcperm[i as usize] != 0 {
            j += 1;
            bcperm[i as usize] = j;
        }
        i += 1;
    }
    newtype = j;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        if (*bound.wrapping_offset(j as isize)).nosides == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            GetElementSide(
                *(*bound.wrapping_offset(j as isize)).parent.wrapping_offset(i as isize),
                *(*bound.wrapping_offset(j as isize)).side.wrapping_offset(i as isize),
                *(*bound.wrapping_offset(j as isize)).normal.wrapping_offset(i as isize),
                data,
                ind.as_mut_ptr(),
                &mut sideelemtype,
            );
            sumsides += 1;
            material = *(*bound.wrapping_offset(j as isize)).types.wrapping_offset(i as isize);
            // SAFETY: OOB-mirror read.
            material = *bcperm.as_mut_ptr().wrapping_offset(material as isize);
            fprintf(
                out,
                b"%d %d %d %d %d\0".as_ptr() as *const c_char,
                sumsides,
                material,
                *(*bound.wrapping_offset(j as isize)).parent.wrapping_offset(i as isize),
                *(*bound.wrapping_offset(j as isize)).parent2.wrapping_offset(i as isize),
                sideelemtype,
            );
            sidetypes[sideelemtype as usize] += 1;
            nodesd1 = sideelemtype % 100;
            l = 0;
            while l < nodesd1 {
                fprintf(out, b" %d\0".as_ptr() as *const c_char, ind[l as usize]);
                l += 1;
            }
            fprintf(out, b"\n\0".as_ptr() as *const c_char);
            i += 1;
        }
        j += 1;
    }
    i = 1;
    while i <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        elemdim = GetElementDimension(elemtype);
        if elemdim == maxelemdim {
            i += 1;
            continue;
        }
        sumsides += 1;
        material = *(*data).material.wrapping_offset(i as isize);
        // SAFETY: OOB-mirror read.
        bctype = abs(*bodyperm.as_mut_ptr().wrapping_offset(material as isize)) + newtype;
        fprintf(
            out,
            b"%d %d 0 0 %d\0".as_ptr() as *const c_char,
            sumsides,
            bctype,
            elemtype,
        );
        sidetypes[elemtype as usize] += 1;
        nodesd1 = elemtype % 100;
        l = 0;
        while l < nodesd1 {
            fprintf(
                out,
                b" %d\0".as_ptr() as *const c_char,
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(l as isize),
            );
            l += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"Saving %d boundary elements to mesh.boundary\n\0".as_ptr() as *const c_char,
            sumsides,
        );
    }
    tottypes = 0;
    i = 0;
    while i <= MAXELEMENTTYPE {
        if bulktypes[i as usize] != 0 || sidetypes[i as usize] != 0 {
            tottypes += 1;
        }
        i += 1;
    }
    sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, b"mesh.header\0".as_ptr() as *const c_char);
    out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
    if info != 0 {
        printf(
            b"Saving header info with %d elementtypes to %s.\n\0".as_ptr() as *const c_char,
            tottypes,
            filename.as_ptr(),
        );
    }
    if out.is_null() {
        printf(
            b"opening of file was not successful\n\0".as_ptr() as *const c_char,
        );
        return 4;
    }
    fprintf(
        out,
        b"%-6d %-6d %-6d\n\0".as_ptr() as *const c_char,
        noknots,
        nobulkelements,
        sumsides,
    );
    fprintf(out, b"%-6d\n\0".as_ptr() as *const c_char, tottypes);
    i = 0;
    while i <= MAXELEMENTTYPE {
        if bulktypes[i as usize] != 0 || sidetypes[i as usize] != 0 {
            fprintf(
                out,
                b"%-6d %-6d\n\0".as_ptr() as *const c_char,
                i,
                bulktypes[i as usize] + sidetypes[i as usize],
            );
        }
        i += 1;
    }
    fclose(out);
    if info != 0 {
        printf(
            b"Body and boundary numbers were permutated\n\0".as_ptr() as *const c_char,
        );
    }
    if (*data).boundarynamesexist != 0 || (*data).bodynamesexist != 0 {
        sprintf(filename.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, b"mesh.names\0".as_ptr() as *const c_char);
        out = fopen(filename.as_ptr(), b"w\0".as_ptr() as *const c_char);
        if info != 0 {
            printf(
                b"Saving names info to %s.\n\0".as_ptr() as *const c_char,
                filename.as_ptr(),
            );
        }
        if out.is_null() {
            printf(
                b"opening of file was not successful\n\0".as_ptr() as *const c_char,
            );
            return 5;
        }
        if (*data).bodynamesexist != 0 {
            fprintf(out, b"! ----- names for bodies -----\n\0".as_ptr() as *const c_char);
            i = 1;
            while i < MAXBODIES as c_int {
                if bodyperm[i as usize] > 0 {
                    fprintf(
                        out,
                        b"$ %s = %d\n\0".as_ptr() as *const c_char,
                        (*data).bodyname[i as usize],
                        bodyperm[i as usize],
                    );
                }
                i += 1;
            }
        }
        if (*data).boundarynamesexist != 0 {
            fprintf(
                out,
                b"! ----- names for boundaries -----\n\0".as_ptr() as *const c_char,
            );
            i = 1;
            while i < MAXBCS as c_int {
                if bcperm[i as usize] != 0 {
                    fprintf(
                        out,
                        b"$ %s = %d\n\0".as_ptr() as *const c_char,
                        (*data).boundaryname[i as usize],
                        bcperm[i as usize],
                    );
                }
                i += 1;
            }
            i = 1;
            while i < MAXBODIES as c_int {
                if bodyperm[i as usize] < 0 {
                    fprintf(
                        out,
                        b"$ %s = %d\n\0".as_ptr() as *const c_char,
                        (*data).bodyname[i as usize],
                        abs(bodyperm[i as usize]) + newtype,
                    );
                }
                i += 1;
            }
        }
        fclose(out);
    }
    cdstat = chdir(b"..\0".as_ptr() as *const c_char);
    // NOTE: C ignores the final chdir status; same here.
    let _ = cdstat;
    0
}
// SAFETY: pure index arithmetic over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn InspectVector(
    vector: *mut Real,
    first: c_int,
    last: c_int,
    min: *mut Real,
    max: *mut Real,
    mini: *mut c_int,
    maxi: *mut c_int,
) {
    let mut i: c_int;
    *min = *vector.wrapping_offset(first as isize);
    *mini = first;
    *max = *vector.wrapping_offset(first as isize);
    *maxi = first;
    i = first + 1;
    while i <= last {
        if *vector.wrapping_offset(i as isize) > *max {
            *max = *vector.wrapping_offset(i as isize);
            *maxi = i;
        }
        if *vector.wrapping_offset(i as isize) < *min {
            *min = *vector.wrapping_offset(i as isize);
            *mini = i;
        }
        i += 1;
    }
}

// SAFETY: pure scan over [first..last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn Steepest(vector: *mut Real, first: c_int, last: c_int) -> c_int {
    let mut i: c_int;
    let mut steep: c_int;
    let (mut aid, mut sub): (Real, Real);
    steep = -1;
    sub = 0.0;
    i = first;
    while i < last {
        aid = fabs(*vector.wrapping_offset((i + 1) as isize) - *vector.wrapping_offset(i as isize));
        if aid > sub {
            sub = aid;
            steep = i;
        }
        i += 1;
    }
    steep
}

// SAFETY: pure reduction over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn MeanVector(vector: *mut Real, first: c_int, last: c_int) -> Real {
    let mut sum: Real = 0.0;
    let mut i: c_int;
    i = first;
    while i <= last {
        sum += *vector.wrapping_offset(i as isize);
        i += 1;
    }
    sum / (last - first + 1) as Real
}

// SAFETY: as above with fabs.
#[no_mangle]
pub unsafe extern "C" fn AbsMeanVector(vector: *mut Real, first: c_int, last: c_int) -> Real {
    let mut sum: Real = 0.0;
    let mut i: c_int;
    i = first;
    while i <= last {
        sum += fabs(*vector.wrapping_offset(i as isize));
        i += 1;
    }
    sum / (last - first + 1) as Real
}

// SAFETY: pure reduction over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn DifferVector(
    vector1: *mut Real,
    vector2: *mut Real,
    first: c_int,
    last: c_int,
) -> Real {
    let (mut sum, eps): (Real, Real) = (0.0, 1.0e-50);
    let (mut i, n): (c_int, c_int);
    i = first;
    while i <= last {
        if fabs(
            *vector1.wrapping_offset(i as isize) + *vector2.wrapping_offset(i as isize),
        ) > eps
        {
            sum += fabs(
                2.0 * (*vector1.wrapping_offset(i as isize) - *vector2.wrapping_offset(i as isize))
                    / (*vector1.wrapping_offset(i as isize)
                        + *vector2.wrapping_offset(i as isize)),
            );
        }
        i += 1;
    }
    n = last - first + 1;
    sum / n as Real
}

// SAFETY: resampling write over vector2[0..=n2] (C contract: sized).
#[no_mangle]
pub unsafe extern "C" fn ReformVector(
    vector1: *mut Real,
    n1: c_int,
    vector2: *mut Real,
    n2: c_int,
) {
    let (mut i1, mut i2): (c_int, c_int);
    let (mut x1, mut d1): (Real, Real);
    i2 = 0;
    while i2 < n2 {
        x1 = (n1) as Real * ((i2) as Real / (n2) as Real);
        i1 = (x1) as c_int;
        d1 = x1 - (i1) as Real;
        *vector2.wrapping_offset(i2 as isize) = d1 * *vector1.wrapping_offset((i1 + 1) as isize)
            + (1.0 - d1) * *vector1.wrapping_offset(i1 as isize);
        i2 += 1;
    }
    *vector2.wrapping_offset(n2 as isize) = *vector1.wrapping_offset(n1 as isize);
}

// SAFETY: in-place linear rescale over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn AdjustVector(
    max: Real,
    min: Real,
    vector: *mut Real,
    first: c_int,
    last: c_int,
) {
    let mut i: c_int;
    let (mut oldmax, mut oldmin): (Real, Real);
    oldmax = *vector.wrapping_offset(first as isize);
    oldmin = oldmax;
    i = first + 1;
    while i <= last {
        if oldmax < *vector.wrapping_offset(i as isize) {
            oldmax = *vector.wrapping_offset(i as isize);
        }
        if oldmin > *vector.wrapping_offset(i as isize) {
            oldmin = *vector.wrapping_offset(i as isize);
        }
        i += 1;
    }
    i = first;
    while i <= last {
        *vector.wrapping_offset(i as isize) =
            (max - min) * (*vector.wrapping_offset(i as isize) - oldmin) / (oldmax - oldmin)
                + min;
        i += 1;
    }
}

// SAFETY: file import; fscanf return codes ignored as in C (_iostat).
#[no_mangle]
pub unsafe extern "C" fn ReadRealVector(
    vector: *mut Real,
    first: c_int,
    last: c_int,
    filename: *mut c_char,
) -> c_int {
    let (mut i, mut _iostat): (c_int, c_int);
    let inn: *mut FILE;
    let mut num: Real = 0.0;
    inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"The opening of the real vector file '%s' wasn't successful !\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    i = first;
    while i <= last {
        _iostat = fscanf(
            inn,
            b"%le\n\0".as_ptr() as *const c_char,
            &mut num,
        );
        *vector.wrapping_offset(i as isize) = num;
        i += 1;
    }
    fclose(inn);
    0
}

// SAFETY: file export over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveRealVector(
    vector: *mut Real,
    first: c_int,
    last: c_int,
    filename: *mut c_char,
) {
    let mut i: c_int;
    let out: *mut FILE;
    out = fopen(filename, b"w\0".as_ptr() as *const c_char);
    i = first;
    while i <= last {
        fprintf(out, b"%.6le\0".as_ptr() as *const c_char, *vector.wrapping_offset(i as isize));
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    fclose(out);
}

// SAFETY: file import; fscanf return codes ignored as in C (_iostat).
#[no_mangle]
pub unsafe extern "C" fn ReadIntegerVector(
    vector: *mut c_int,
    first: c_int,
    last: c_int,
    filename: *mut c_char,
) -> c_int {
    let (mut i, mut _iostat): (c_int, c_int);
    let inn: *mut FILE;
    let mut num: c_int = 0;
    inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"The opening of the int vector file '%s' wasn't successful !\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    i = first;
    while i <= last {
        _iostat = fscanf(
            inn,
            b"%d\n\0".as_ptr() as *const c_char,
            &mut num,
        );
        *vector.wrapping_offset(i as isize) = num;
        i += 1;
    }
    fclose(inn);
    0
}

// SAFETY: file export over [first..=last] (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveIntegerVector(
    vector: *mut c_int,
    first: c_int,
    last: c_int,
    filename: *mut c_char,
) {
    let mut i: c_int;
    let out: *mut FILE;
    out = fopen(filename, b"w\0".as_ptr() as *const c_char);
    i = first;
    while i <= last {
        fprintf(out, b"%d\0".as_ptr() as *const c_char, *vector.wrapping_offset(i as isize));
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        i += 1;
    }
    fclose(out);
}

// SAFETY: file import over the matrix range; fscanf codes ignored (_iostat).
#[no_mangle]
pub unsafe extern "C" fn ReadRealMatrix(
    matrix: *mut *mut Real,
    row_first: c_int,
    row_last: c_int,
    col_first: c_int,
    col_last: c_int,
    filename: *mut c_char,
) -> c_int {
    let (mut i, mut j, mut _iostat): (c_int, c_int, c_int);
    let inn: *mut FILE;
    let mut num: Real = 0.0;
    inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"The opening of the real matrix file '%s' wasn't successful!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return 1;
    }
    j = row_first;
    while j <= row_last {
        i = col_first;
        while i <= col_last {
            _iostat = fscanf(
                inn,
                b"%le\n\0".as_ptr() as *const c_char,
                &mut num,
            );
            *(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize) = num;
            i += 1;
        }
        j += 1;
    }
    fclose(inn);
    0
}

// SAFETY: file export over the matrix range (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveRealMatrix(
    matrix: *mut *mut Real,
    row_first: c_int,
    row_last: c_int,
    col_first: c_int,
    col_last: c_int,
    filename: *mut c_char,
) {
    let (mut i, mut j): (c_int, c_int);
    let out: *mut FILE;
    out = fopen(filename, b"w\0".as_ptr() as *const c_char);
    j = row_first;
    while j <= row_last {
        i = col_first;
        while i <= col_last {
            fprintf(
                out,
                b"%-14.6lg\0".as_ptr() as *const c_char,
                *(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize),
            );
            fprintf(out, b"\t\0".as_ptr() as *const c_char);
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        j += 1;
    }
    fclose(out);
}

// SAFETY: file import over the matrix range; fscanf codes ignored (_iostat).
// NOTE: C returns FALSE(0)/TRUE(1) here (unlike the vector readers above).
#[no_mangle]
pub unsafe extern "C" fn ReadIntegerMatrix(
    matrix: *mut *mut c_int,
    row_first: c_int,
    row_last: c_int,
    col_first: c_int,
    col_last: c_int,
    filename: *mut c_char,
) -> c_int {
    let (mut i, mut j, mut _iostat): (c_int, c_int, c_int);
    let inn: *mut FILE;
    let mut num: c_int = 0;
    inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"The opening of the int matrix file '%s' wasn't successful!\n\0".as_ptr()
                as *const c_char,
            filename,
        );
        return FALSE;
    }
    j = row_first;
    while j <= row_last {
        i = col_first;
        while i <= col_last {
            _iostat = fscanf(
                inn,
                b"%d\n\0".as_ptr() as *const c_char,
                &mut num,
            );
            *(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize) = num;
            i += 1;
        }
        j += 1;
    }
    fclose(inn);
    TRUE
}

// SAFETY: file export over the matrix range (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveIntegerMatrix(
    matrix: *mut *mut c_int,
    row_first: c_int,
    row_last: c_int,
    col_first: c_int,
    col_last: c_int,
    filename: *mut c_char,
) {
    let (mut i, mut j): (c_int, c_int);
    let out: *mut FILE;
    out = fopen(filename, b"w\0".as_ptr() as *const c_char);
    j = row_first;
    while j <= row_last {
        i = col_first;
        while i <= col_last {
            fprintf(
                out,
                b"%-8d\0".as_ptr() as *const c_char,
                *(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize),
            );
            fprintf(out, b"\t\0".as_ptr() as *const c_char);
            i += 1;
        }
        fprintf(out, b"\n\0".as_ptr() as *const c_char);
        j += 1;
    }
    fclose(out);
}

// SAFETY: file export over the matrix range (C contract).
#[no_mangle]
pub unsafe extern "C" fn SaveNonZeros(
    matrix: *mut *mut Real,
    row_first: c_int,
    row_last: c_int,
    col_first: c_int,
    col_last: c_int,
    filename: *mut c_char,
) {
    let (mut i, mut j): (c_int, c_int);
    let out: *mut FILE;
    let nearzero: Real = 1.0e-20;
    out = fopen(filename, b"w\0".as_ptr() as *const c_char);
    j = row_first;
    while j <= row_last {
        i = col_first;
        while i <= col_last {
            if fabs(*(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize)) > nearzero
            {
                fprintf(
                    out,
                    b"%d\t %d\t %-12.6le\n\0".as_ptr() as *const c_char,
                    j,
                    i,
                    *(*matrix.wrapping_offset(j as isize)).wrapping_offset(i as isize),
                );
            }
            i += 1;
        }
        j += 1;
    }
    fclose(out);
}

// SAFETY: file echo; LINELENGTH buffer is function-local (C contract).
#[no_mangle]
pub unsafe extern "C" fn EchoFile(filename: *mut c_char) -> c_int {
    let inn: *mut FILE;
    let mut line: [c_char; 100] = [0; 100];
    inn = fopen(filename, b"r\0".as_ptr() as *const c_char);
    if inn.is_null() {
        printf(
            b"Could not open file '%s'.\n\0".as_ptr() as *const c_char,
            filename,
        );
        return 1;
    }
    while !fgets(line.as_mut_ptr(), 100, inn).is_null() {
        printf(b"%s\0".as_ptr() as *const c_char, line.as_ptr());
    }
    fclose(inn);
    0
}

// SAFETY: point duplication over valid parents (C contract); the C leak of
// `order` (never freed on any path) is preserved — freeing would change
// allocator traffic, not behavior, and the corpus pins current behavior.
#[no_mangle]
pub unsafe extern "C" fn SetDiscontinuousPoints(
    data: *mut FemType,
    point: *mut PointType,
    material: c_int,
) -> c_int {
    let (mut i, mut ind, mut corner): (c_int, c_int, c_int);
    let (mut parent, mut new): (c_int, c_int);
    let newsuccess: c_int;
    let order: *mut c_int;
    if (*point).nopoints == FALSE {
        printf(
            b"SetDiscontinuousPoint: No points exists!\n\0".as_ptr() as *const c_char,
        );
        return 0;
    }
    order = ivector(1, (*data).noknots);
    i = 1;
    while i <= (*data).noknots {
        *order.wrapping_offset(i as isize) = i;
        i += 1;
    }
    new = 0;
    i = 0;
    while i < (*point).nopoints {
        parent = (*point).parent[i as usize];
        corner = (*point).corner[i as usize];
        ind = *(*(*data).topology.wrapping_offset(parent as isize)).wrapping_offset(corner as isize);
        if *order.wrapping_offset(ind as isize) >= 0 {
            new += 1;
            *order.wrapping_offset(ind as isize) = -new;
        }
        i += 1;
    }
    if new == 0 {
        return 0;
    }
    newsuccess = CreateNewNodes(data, order, material, new);
    newsuccess
}
