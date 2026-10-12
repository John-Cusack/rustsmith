// egmesh: structured-mesh construction and manipulation (ElmerGrid lane,
// pair 3). Body transcribed from elmergrid/src/egmesh.c @ 9f6af2f85; see
// contract notes in egconvert/src/lib.rs (same lane conventions).

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(static_mut_refs)]
#![allow(dead_code)]
#![allow(non_snake_case)]

use eg_prelude::*;

// ---- sibling-unit imports (declared in headers, resolved at final link) --
extern "C" {
    fn bigerror(error_text: *const c_char);
    fn smallerror(error_text: *const c_char);
    fn dvector(nl: c_int, nh: c_int) -> *mut Real;
    fn ivector(nl: c_int, nh: c_int) -> *mut c_int;
    fn cvector(nl: c_int, nh: c_int) -> *mut c_char;
    fn imatrix(nrl: c_int, nrh: c_int, ncl: c_int, nch: c_int) -> *mut *mut c_int;
    fn free_ivector(v: *mut c_int, nl: c_int, nh: c_int);
    fn free_imatrix(m: *mut *mut c_int, nrl: c_int, nrh: c_int, ncl: c_int, nch: c_int);
    fn free_dvector(v: *mut Real, nl: c_int, nh: c_int);
    fn free_cvector(v: *mut c_char, nl: c_int, nh: c_int);
    fn GetSideInfo(
        cell: *mut CellType,
        cellno: c_int,
        side: c_int,
        element: c_int,
        elemind: *mut c_int,
    ) -> c_int;
    fn GetElementIndex(cell: *mut CellType, i: c_int, j: c_int) -> c_int;
    fn SortIndex(n: c_int, key: *mut Real, ord: *mut c_int);
    fn SetElementDivisionCylinder(grid: *mut GridType, info: c_int);
    fn SetElementDivisionExtruded(grid: *mut GridType, info: c_int);
    fn next_int(start: *mut *mut c_char) -> c_int;
    fn next_real(start: *mut *mut c_char) -> Real;
    fn Minimum(vector: *mut Real, first: c_int, last: c_int) -> Real;
    fn Maximum(vector: *mut Real, first: c_int, last: c_int) -> Real;
    fn GetElementCoordinates(
        cell: *mut CellType,
        i: c_int,
        j: c_int,
        globalcoord: *mut Real,
        ind: *mut c_int,
    ) -> c_int;
}
/// Knot coordinates and topology of one element (standard formulation).
///
/// # Safety
/// `data`, `globalcoord`, `ind` valid; `data` knots stored.
#[no_mangle]
pub unsafe extern "C" fn GetElementInfo(
    element: c_int,
    data: *mut FemType,
    globalcoord: *mut Real,
    ind: *mut c_int,
    material: *mut c_int,
) {
    let mut i: c_int;
    let mut indi: c_int;
    let nodesd2: c_int = *(*data).elementtypes.wrapping_offset(element as isize) % 100;
    i = 0;
    while i < nodesd2 {
        indi = *(*(*data).topology.wrapping_offset(element as isize))
            .wrapping_offset(i as isize);
        *ind.wrapping_offset(i as isize) = indi;
        *globalcoord.wrapping_offset(i as isize) =
            *(*data).x.wrapping_offset(indi as isize);
        *globalcoord.wrapping_offset((i + nodesd2) as isize) =
            *(*data).y.wrapping_offset(indi as isize);
        i += 1;
    }
    *material = *(*data).material.wrapping_offset(element as isize);
}

/// Bulk dimension of an element type code.
///
/// # Safety
/// /// Takes no pointers; any code is accepted (unknown codes take the error path).
#[no_mangle]
pub unsafe extern "C" fn GetElementDimension(elementtype: c_int) -> c_int {
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut elemdim: c_int = 0;
    // NOTE: C switch over elementtype/100.
    if elementtype / 100 == 1 {
        elemdim = 0;
    } else if elementtype / 100 == 2 {
        elemdim = 1;
    } else if elementtype / 100 == 3 || elementtype / 100 == 4 {
        elemdim = 2;
    } else if elementtype / 100 == 5
        || elementtype / 100 == 6
        || elementtype / 100 == 7
        || elementtype / 100 == 8
    {
        elemdim = 3;
    } else {
        printf(
            c"GetElementDimension: unknown elementtype %d\n".as_ptr(),
            elementtype,
        );
    }
    elemdim
}

/// Largest element type code in the mesh.
///
/// # Safety
/// /// `data` valid with `noelements` readable element-type entries.
#[no_mangle]
pub unsafe extern "C" fn GetMaxElementType(data: *mut FemType) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelementtype: c_int;
    maxelementtype = *(*data).elementtypes.wrapping_offset(1);
    i = 1;
    while i <= (*data).noelements {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxelementtype < *(*data).elementtypes.wrapping_offset(i as isize) {
            maxelementtype = *(*data).elementtypes.wrapping_offset(i as isize);
        }
        i += 1;
    }
    maxelementtype
}

/// Smallest element type code in the mesh.
///
/// # Safety
/// /// `data` valid with `noelements` readable element-type entries.
#[no_mangle]
pub unsafe extern "C" fn GetMinElementType(data: *mut FemType) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minelementtype: c_int;
    minelementtype = *(*data).elementtypes.wrapping_offset(1);
    i = 1;
    while i <= (*data).noelements {
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if *(*data).elementtypes.wrapping_offset(i as isize) < minelementtype {
            minelementtype = *(*data).elementtypes.wrapping_offset(i as isize);
        }
        i += 1;
    }
    minelementtype
}

/// Bulk dimension of the largest element type in the mesh.
///
/// # Safety
/// /// `data` valid with `noelements` readable element-type entries.
#[no_mangle]
pub unsafe extern "C" fn GetMaxElementDimension(data: *mut FemType) -> c_int {
    let maxelementtype: c_int = GetMaxElementType(data);
    let elemdim: c_int = GetElementDimension(maxelementtype);
    elemdim
}

/// Largest body/material index in the mesh.
///
/// # Safety
/// /// `data` valid with `noelements` readable material entries.
#[no_mangle]
pub unsafe extern "C" fn GetMaxBodyIndex(data: *mut FemType) -> c_int {
    let mut i: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut maxind: c_int = 0;
    // NOTE: C `maxind = 0` dropped (decl init holds).
    i = 1;
    while i <= (*data).noelements {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxind < *(*data).material.wrapping_offset(i as isize) {
            maxind = *(*data).material.wrapping_offset(i as isize);
        }
        i += 1;
    }
    maxind
}

/// Largest boundary type index over all created boundaries.
///
/// # Safety
/// /// `bound` valid over `MAXBOUNDARIES` records.
#[no_mangle]
pub unsafe extern "C" fn GetMaxBCIndex(bound: *mut BoundaryType) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut maxind: c_int = 0;
    // NOTE: C `maxind = 0` dropped (decl init holds).
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
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxind
                < *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)
            {
                maxind = *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize);
            }
            i += 1;
        }
        j += 1;
    }
    maxind
}

/// Number of coordinate components actually used (by magnitude).
///
/// # Safety
/// /// `data` valid with `noknots` readable coordinate entries.
#[no_mangle]
pub unsafe extern "C" fn GetCoordinateDimension(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut coorddim: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut coordis: c_int;
    // NOTE: null-init (set per-component before reads).
    let mut coord: *mut Real;
    // NOTE: set per-component before every read.
    // NOTE: `=1.0e-20` decl init kept (read before any overwrite).
    let epsilon: Real = 1.0e-20;
    let noknots: c_int = (*data).noknots;
    coorddim = 0;
    j = 3;
    while j >= 1 {
        coordis = FALSE;
        if j == 1 {
            coord = (*data).x;
        } else if j == 2 {
            coord = (*data).y;
        } else {
            coord = (*data).z;
        }
        i = 1;
        while i <= noknots {
            if fabs(*coord.wrapping_offset(i as isize)) > epsilon {
                coordis = TRUE;
                break;
            }
            i += 1;
        }
        if coordis != 0 {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if coorddim < j {
                coorddim = j;
            }
        }
        j -= 1;
    }
    if info != 0 {
        printf(
            c"Coordinates defined in %d dimensions\n".as_ptr(),
            coorddim,
        );
    }
    coorddim
}
/// SAFETY: 2D side tables of GetElementSide (split for sizing).
unsafe fn element_side_2d(
    elemtype: c_int,
    elemind: *mut c_int,
    side: c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    i: &mut c_int,
) -> bool {
    let _ = i;
    if elemtype == 202 || elemtype == 203 || elemtype == 204 {
        *sideelemtype = 101;
        *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
    } else if elemtype == 303 {
        // NOTE: linear triangle.
        if side < 3 {
            *sideelemtype = 202;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
        } else if side < 6 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 3) as isize);
        }
    } else if elemtype == 306 {
        // NOTE: 2nd order triangle.
        if side < 3 {
            *sideelemtype = 203;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((side + 3) as isize);
        } else if side < 9 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 3) as isize);
        }
    } else if elemtype == 310 {
        // NOTE: 3rd order triangle.
        if side < 3 {
            *sideelemtype = 204;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((2 * side + 3) as isize);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((2 * side + 4) as isize);
        } else if side < 13 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 3) as isize);
        }
    } else if elemtype == 404 {
        // NOTE: linear quadrilateral.
        if side < 4 {
            *sideelemtype = 202;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
        } else if side < 8 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else if elemtype == 405 {
        if side < 4 {
            *sideelemtype = 202;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
        } else if side < 9 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else if elemtype == 408 {
        // NOTE: 2nd order quadrilateral.
        if side < 4 {
            *sideelemtype = 203;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((side + 4) as isize);
        } else if side < 12 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else if elemtype == 409 {
        if side < 4 {
            *sideelemtype = 203;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((side + 4) as isize);
        } else if side < 13 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else if elemtype == 412 {
        // NOTE: 3rd order quadrilateral.
        if side < 4 {
            *sideelemtype = 204;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((2 * side + 4) as isize);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((2 * side + 5) as isize);
        } else if side < 16 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else if elemtype == 416 {
        if side < 4 {
            *sideelemtype = 204;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset((2 * side + 4) as isize);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((2 * side + 5) as isize);
        } else if side < 20 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
        }
    } else {
        return false;
    }
    true
}
/// SAFETY: tet/pyramid side tables of GetElementSide (split for sizing).
unsafe fn element_side_tetpyr(
    elemtype: c_int,
    elemind: *mut c_int,
    side: c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    i: &mut c_int,
) -> bool {
    if elemtype == 504 {
        // NOTE: linear tetrahedron.
        if side < 4 {
            *sideelemtype = 303;
            if side < 3 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(3);
            }
            if side == 3 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset(0);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(2);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(1);
            }
        } else if side < 10 {
            *sideelemtype = 202;
            if side < 7 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(3);
            } else {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 7) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side - 6) % 3) as isize);
            }
        } else if side < 14 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 10) as isize);
        }
    } else if elemtype == 510 {
        // NOTE: 2nd order tetrahedron.
        if side < 4 {
            *sideelemtype = 306;
            if side < 3 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(3);
                *ind.wrapping_offset(3) = *elemind.wrapping_offset((4 + side) as isize);
                *ind.wrapping_offset(4) =
                    *elemind.wrapping_offset((7 + (side + 1) % 3) as isize);
                *ind.wrapping_offset(5) = *elemind.wrapping_offset((7 + side) as isize);
            } else if side == 3 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset(0);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(1);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(2);
                *ind.wrapping_offset(3) = *elemind.wrapping_offset(4);
                *ind.wrapping_offset(4) = *elemind.wrapping_offset(5);
                *ind.wrapping_offset(5) = *elemind.wrapping_offset(6);
            }
        } else if side < 10 {
            *sideelemtype = 203;
            if side < 7 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(3);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset((side + 3) as isize);
            } else {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 7) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side - 6) % 3) as isize);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset((side - 3) as isize);
            }
        } else if side < 20 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 10) as isize);
        }
    } else if elemtype == 605 {
        // NOTE: linear pyramid.
        if side < 4 {
            *sideelemtype = 303;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
        } else if side < 5 {
            *sideelemtype = 404;
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(*i as isize) = *elemind.wrapping_offset(*i as isize);
                *i += 1;
            }
        } else if side < 13 {
            *sideelemtype = 202;
            if side < 9 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 5) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side - 4) % 4) as isize);
            } else {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 9) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
            }
        } else if side < 18 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 13) as isize);
        }
    } else if elemtype == 613 {
        // NOTE: 2nd order pyramid.
        if side < 4 {
            *sideelemtype = 306;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) = *elemind.wrapping_offset(4);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((side + 5) as isize);
            *ind.wrapping_offset(4) =
                *elemind.wrapping_offset(((side + 1) % 4 + 9) as isize);
            *ind.wrapping_offset(5) = *elemind.wrapping_offset((side % 4 + 9) as isize);
        } else if side == 4 {
            *sideelemtype = 408;
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(*i as isize) = *elemind.wrapping_offset(*i as isize);
                *i += 1;
            }
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(((*i) + 4) as isize) =
                    *elemind.wrapping_offset((*i + 5) as isize);
                *i += 1;
            }
        } else if side < 13 {
            *sideelemtype = 203;
            if side < 9 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 5) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side - 4) % 4) as isize);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(side as isize);
            } else {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 9) as isize);
                *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
                *ind.wrapping_offset(2) = *elemind.wrapping_offset(side as isize);
            }
        } else if side < 26 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 13) as isize);
        }
    } else {
        return false;
    }
    true
}
/// SAFETY: wedge side tables of GetElementSide (split for sizing).
unsafe fn element_side_wedge(
    elemtype: c_int,
    elemind: *mut c_int,
    side: c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    i: &mut c_int,
) -> bool {
    // NOTE: 706/715/718 share the edge/node arms; face orders differ.
    if elemtype != 706 && elemtype != 715 && elemtype != 718 {
        return false;
    }
    if side < 3 {
        if elemtype == 706 {
            *sideelemtype = 404;
        } else if elemtype == 715 {
            *sideelemtype = 408;
        } else {
            *sideelemtype = 409;
        }
        *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
        *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 3) as isize);
        *ind.wrapping_offset(2) = *elemind.wrapping_offset(((side + 1) % 3 + 3) as isize);
        *ind.wrapping_offset(3) = *elemind.wrapping_offset((side + 3) as isize);
        if elemtype != 706 {
            *ind.wrapping_offset(4) = *elemind.wrapping_offset((6 + side) as isize);
            *ind.wrapping_offset(5) =
                *elemind.wrapping_offset((12 + (side + 1) % 3) as isize);
            *ind.wrapping_offset(6) = *elemind.wrapping_offset((9 + side) as isize);
            *ind.wrapping_offset(7) = *elemind.wrapping_offset((12 + side) as isize);
            if elemtype == 718 {
                *ind.wrapping_offset(8) = *elemind.wrapping_offset((15 + side) as isize);
            }
        }
    } else if side < 5 {
        *sideelemtype = 306;
        *i = 0;
        while *i < 3 {
            *ind.wrapping_offset(*i as isize) =
                *elemind.wrapping_offset((3 * (side - 3) + *i) as isize);
            if elemtype != 706 {
                *ind.wrapping_offset((*i + 3) as isize) =
                    *elemind.wrapping_offset((3 * (side - 3) + 6 + *i) as isize);
            }
            *i += 1;
        }
        // NOTE: linear wedge takes the 303 face arm instead.
        if elemtype == 706 {
            *sideelemtype = 303;
        }
    } else if side < 14 {
        *sideelemtype = 202;
        if side < 8 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 5) as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side - 4) % 3) as isize);
        }
        if side < 11 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((3 + side - 8) as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset((3 + (side - 7) % 3) as isize);
        } else {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 11) as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset((3 + side - 11) as isize);
        }
    } else if side < 20 {
        *sideelemtype = 101;
        *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 14) as isize);
    }
    true
}

/// SAFETY: brick side tables of GetElementSide (split for sizing).
unsafe fn element_side_brick(
    elemtype: c_int,
    elemind: *mut c_int,
    side: c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    i: &mut c_int,
) -> bool {
    if elemtype == 808 {
        // NOTE: linear brick.
        if side < 6 {
            *sideelemtype = 404;
            if side < 4 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
                *ind.wrapping_offset(1) =
                    *elemind.wrapping_offset(((side + 1) % 4) as isize);
                *ind.wrapping_offset(2) =
                    *elemind.wrapping_offset(((side + 1) % 4 + 4) as isize);
                *ind.wrapping_offset(3) = *elemind.wrapping_offset((side + 4) as isize);
            } else if side < 6 {
                *i = 0;
                while *i < 4 {
                    *ind.wrapping_offset(*i as isize) =
                        *elemind.wrapping_offset((4 * (side - 4) + *i) as isize);
                    *i += 1;
                }
            }
        } else if side < 18 {
            *sideelemtype = 202;
            if side < 10 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 6) as isize);
                *ind.wrapping_offset(1) =
                    *elemind.wrapping_offset(((side - 5) % 4) as isize);
            } else if side < 14 {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 6) as isize);
                *ind.wrapping_offset(1) =
                    *elemind.wrapping_offset(((side - 9) % 4 + 4) as isize);
            } else {
                *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 14) as isize);
                *ind.wrapping_offset(1) =
                    *elemind.wrapping_offset((side - 14 + 4) as isize);
            }
        } else if side < 26 {
            *sideelemtype = 101;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 18) as isize);
        }
    } else if elemtype == 820 {
        // NOTE: 2nd order brick.
        if side < 4 {
            *sideelemtype = 408;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) =
                *elemind.wrapping_offset(((side + 1) % 4 + 4) as isize);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((side + 4) as isize);
            *ind.wrapping_offset(4) = *elemind.wrapping_offset((8 + side) as isize);
            *ind.wrapping_offset(5) =
                *elemind.wrapping_offset((12 + (side + 1) % 4) as isize);
            *ind.wrapping_offset(6) = *elemind.wrapping_offset((16 + side) as isize);
            *ind.wrapping_offset(7) = *elemind.wrapping_offset((12 + side) as isize);
        } else if side < 6 {
            *sideelemtype = 408;
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(*i as isize) =
                    *elemind.wrapping_offset((4 * (side - 4) + *i) as isize);
                *i += 1;
            }
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(((*i) + 4) as isize) =
                    *elemind.wrapping_offset((8 * (side - 4) + 8 + *i) as isize);
                *i += 1;
            }
        }
    } else if elemtype == 827 {
        if side < 4 {
            *sideelemtype = 409;
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(((side + 1) % 4) as isize);
            *ind.wrapping_offset(2) =
                *elemind.wrapping_offset(((side + 1) % 4 + 4) as isize);
            *ind.wrapping_offset(3) = *elemind.wrapping_offset((side + 4) as isize);
            *ind.wrapping_offset(4) = *elemind.wrapping_offset((8 + side) as isize);
            *ind.wrapping_offset(5) =
                *elemind.wrapping_offset((12 + (side + 1) % 4) as isize);
            *ind.wrapping_offset(6) = *elemind.wrapping_offset((16 + side) as isize);
            *ind.wrapping_offset(7) = *elemind.wrapping_offset((12 + side) as isize);
            *ind.wrapping_offset(8) = *elemind.wrapping_offset((20 + side) as isize);
        } else {
            *sideelemtype = 409;
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(*i as isize) =
                    *elemind.wrapping_offset((4 * (side - 4) + *i) as isize);
                *i += 1;
            }
            *i = 0;
            while *i < 4 {
                *ind.wrapping_offset(((*i) + 4) as isize) =
                    *elemind.wrapping_offset((8 * (side - 4) + 8 + *i) as isize);
                *i += 1;
            }
            *ind.wrapping_offset(8) = *elemind.wrapping_offset((20 + side) as isize);
        }
    } else {
        return false;
    }
    true
}

/// Side topology of an element, with optional normal flip.
///
/// # Safety
/// `data`, `ind` valid; element in range.
#[no_mangle]
pub unsafe extern "C" fn GetElementSide(
    element: c_int,
    mut side: c_int,
    normal: c_int,
    data: *mut FemType,
    ind: *mut c_int,
    sideelemtype: *mut c_int,
) {
    let mut i: c_int = 0;
    let j: c_int;
    // NOTE: null-init (set from data before reads).
    let mut ind2: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: commented-out range check omitted (dead in C).
    let elemtype: c_int = *(*data).elementtypes.wrapping_offset(element as isize);
    let elemind: *mut c_int = *(*data).topology.wrapping_offset(element as isize);
    let sides: c_int = elemtype / 100;
    *sideelemtype = 0;
    if side < 0 && sides > 4 {
        side = -(side + 1);
    }
    // NOTE: C switch over elemtype, split by family.
    if !(element_side_2d(elemtype, elemind, side, ind, &mut *sideelemtype, &mut i)
        || element_side_tetpyr(elemtype, elemind, side, ind, &mut *sideelemtype, &mut i)
        || element_side_wedge(elemtype, elemind, side, ind, &mut *sideelemtype, &mut i)
        || element_side_brick(elemtype, elemind, side, ind, &mut *sideelemtype, &mut i))
    {
        printf(
            c"GetElementSide: unknown elementtype %d (elem=%d,side=%d)\n".as_ptr(),
            elemtype,
            element,
            side,
        );
        bigerror(c"Cannot continue".as_ptr());
    }
    if normal == -1
        && (*sideelemtype == 202
            || *sideelemtype == 203
            || *sideelemtype == 303
            || *sideelemtype == 404)
    {
            j = *sideelemtype / 100 - 1;
            i = 0;
            while i <= j {
                ind2[i as usize] = *ind.wrapping_offset(i as isize);
                i += 1;
            }
            i = 0;
            while i <= j {
                *ind.wrapping_offset(i as isize) = ind2[(j - i) as usize];
                i += 1;
            }
    }
}
/// Side topology from a boundary record (or the bulk element behind it).
///
/// # Safety
/// `bound`, `data`, `ind` valid.
#[no_mangle]
pub unsafe extern "C" fn GetBoundaryElement(
    sideind: c_int,
    bound: *mut BoundaryType,
    data: *mut FemType,
    ind: *mut c_int,
    sideelemtype: *mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let side: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let normal: c_int;
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let n: c_int;
    if sideind > (*bound).nosides {
        *sideelemtype = 0;
        printf(
            c"Side element index %d exceeds size of boundary (%d)\n".as_ptr(),
            sideind,
            (*bound).nosides,
        );
        return;
    }
    let element: c_int = *(*bound).parent.wrapping_offset(sideind as isize);
    // NOTE: commented-out GetElementSide probe omitted (dead in C).
    if element != 0 {
        side = *(*bound).side.wrapping_offset(sideind as isize);
        normal = *(*bound).normal.wrapping_offset(sideind as isize);
        GetElementSide(element, side, normal, data, ind, sideelemtype);
    } else {
        *sideelemtype = *(*bound).elementtypes.wrapping_offset(sideind as isize);
        n = *sideelemtype % 100;
        i = 0;
        while i < n {
            *ind.wrapping_offset(i as isize) = *(*(*bound)
                .topology
                .wrapping_offset(sideind as isize))
            .wrapping_offset(i as isize);
            i += 1;
        }
        // NOTE: `if(0)` dump omitted (dead in C).
    }
}

/// Number of faces of an element type code.
///
/// # Safety
/// /// Takes no pointers; any code is accepted (unknown codes take the error path).
#[no_mangle]
pub unsafe extern "C" fn GetElementFaces(elemtype: c_int) -> c_int {
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut elemfaces: c_int = 0;
    let basetype: c_int = elemtype / 100;
    // NOTE: C switch over basetype.
    if basetype == 1 {
        elemfaces = 0;
    } else if basetype == 2 {
        elemfaces = 2;
    } else if basetype == 3 {
        elemfaces = 3;
    } else if basetype == 4 || basetype == 5 {
        elemfaces = 4;
    } else if basetype == 6 || basetype == 7 {
        elemfaces = 5;
    } else if basetype == 8 {
        elemfaces = 6;
    } else {
        printf(
            c"GetElementFaces: Unknown elementtype %d\n".as_ptr(),
            elemfaces,
        );
    }
    elemfaces
}

/// Edge endpoints of an element, with mid-edge node for quadratics.
///
/// # Safety
/// `data`, `ind` valid; element in range.
#[no_mangle]
pub unsafe extern "C" fn GetElementGraph(
    element: c_int,
    edge: c_int,
    data: *mut FemType,
    ind: *mut c_int,
) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let evenodd: c_int;
    // NOTE: null-init (set from data before reads).
    let elemtype: c_int = *(*data).elementtypes.wrapping_offset(element as isize);
    let basetype: c_int = elemtype / 100;
    let elemnodes: c_int = elemtype % 100;
    let quadratic: c_int = (elemnodes > basetype) as c_int;
    let elemind: *mut c_int = *(*data).topology.wrapping_offset(element as isize);
    *ind.wrapping_offset(0) = 0;
    *ind.wrapping_offset(1) = 0;
    let side: c_int = if quadratic != 0 { edge / 2 } else { edge };
    // NOTE: C switch over basetype.
    if basetype == 2 {
        if side == 0 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(0);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(1);
        }
    } else if basetype == 3 {
        if side < 3 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset(((side + 1) % 3) as isize);
        }
    } else if basetype == 4 {
        if side < 4 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset(((side + 1) % 4) as isize);
        }
    } else if basetype == 5 {
        if side < 3 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset(((side + 1) % 3) as isize);
        } else if side < 6 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 3) as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(3);
        }
    } else if basetype == 6 {
        if side < 4 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset(((side + 1) % 4) as isize);
        } else if side < 8 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
        }
    } else if basetype == 7 {
        // NOTE: C switch over side.
        if side == 0 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(0);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(1);
        } else if side == 1 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(1);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(2);
        } else if side == 2 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(2);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(0);
        } else if side == 3 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(3);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
        } else if side == 4 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(4);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(5);
        } else if side == 5 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(5);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(3);
        } else if side == 6 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(0);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(3);
        } else if side == 7 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(1);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(4);
        } else if side == 8 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(2);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(5);
        }
    } else if basetype == 8 {
        if side < 4 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset(side as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset(((side + 1) % 4) as isize);
        } else if side < 8 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
            *ind.wrapping_offset(1) = *elemind.wrapping_offset(side as isize);
        } else if side < 12 {
            *ind.wrapping_offset(0) = *elemind.wrapping_offset((side - 4) as isize);
            *ind.wrapping_offset(1) =
                *elemind.wrapping_offset((4 + (side + 1) % 4) as isize);
        }
    }
    let hit: c_int = (*ind.wrapping_offset(0) != 0 || *ind.wrapping_offset(1) != 0) as c_int;
    if hit != 0 && quadratic != 0 {
        evenodd = edge - 2 * side;
        // NOTE: C switch over basetype.
        if basetype == 2 {
            *ind.wrapping_offset(evenodd as isize) = *elemind.wrapping_offset(2);
        } else if basetype == 3 {
            *ind.wrapping_offset(evenodd as isize) =
                *elemind.wrapping_offset((side + 3) as isize);
        } else if basetype == 4 || basetype == 5 {
            *ind.wrapping_offset(evenodd as isize) =
                *elemind.wrapping_offset((side + 4) as isize);
        } else if basetype == 6 {
            *ind.wrapping_offset(evenodd as isize) =
                *elemind.wrapping_offset((side + 5) as isize);
        } else if basetype == 7 {
            *ind.wrapping_offset(evenodd as isize) =
                *elemind.wrapping_offset((side + 6) as isize);
        } else if basetype == 8 {
            *ind.wrapping_offset(evenodd as isize) =
                *elemind.wrapping_offset((side + 8) as isize);
        }
    }
    hit
}

/// Maximum node-index spread over elements (optionally through a perm).
///
/// # Safety
/// `data`, `indx` valid.
#[no_mangle]
pub unsafe extern "C" fn CalculateIndexwidth(
    data: *mut FemType,
    indxis: c_int,
    indx: *mut c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut indexwidth: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut imax: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut imin: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut element: c_int;
    // NOTE: maximum bandwidth over elements.
    indexwidth = 0;
    element = 1;
    while element <= (*data).noelements {
        imin = (*data).noknots;
        imax = 0;
        nonodes = *(*data).elementtypes.wrapping_offset(element as isize) % 100;
        i = 0;
        while i < nonodes {
            ind = *(*(*data).topology.wrapping_offset(element as isize))
                .wrapping_offset(i as isize);
            if indxis != 0 {
                ind = *indx.wrapping_offset(ind as isize);
            }
            if ind == 0 {
                i += 1;
                continue;
            }
            if ind > imax {
                imax = ind;
            }
            if ind < imin {
                imin = ind;
            }
            i += 1;
        }
        if imax - imin > indexwidth {
            indexwidth = imax - imin;
        }
        element += 1;
    }
    if indxis == 0 {
        (*data).indexwidth = indexwidth;
    }
    indexwidth
}

/// Zero a FemType to freshly-initialized state.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn InitializeKnots(data: *mut FemType) {
    let mut i: c_int;
    (*data).timesteps = 0;
    (*data).noknots = 0;
    (*data).noelements = 0;
    (*data).coordsystem = COORD_CART2;
    (*data).numbering = NUMBER_XY;
    (*data).created = FALSE;
    (*data).variables = 0;
    (*data).maxnodes = 0;
    (*data).indexwidth = 0;
    (*data).noboundaries = 0;
    (*data).mapgeo = 1;
    (*data).nocorners = 0;
    (*data).boundarynamesexist = FALSE;
    (*data).bodynamesexist = FALSE;
    (*data).nodepermexist = FALSE;
    (*data).nopartitions = 1;
    (*data).partitionexist = FALSE;
    (*data).periodicexist = FALSE;
    (*data).nodeconnectexist = FALSE;
    (*data).elemconnectexist = FALSE;
    (*data).nodalexists = FALSE;
    // NOTE: commented-out invtopoexists omitted (dead in C).
    (*data).partitiontableexists = FALSE;
    (*data).maxpartitiontable = 0;
    (*data).invtopo.created = FALSE;
    (*data).nodalgraph2.created = FALSE;
    (*data).dualgraph.created = FALSE;
    i = 0;
    while i < MAXDOFS as c_int {
        (*data).edofs[i as usize] = 0;
        (*data).bandwidth[i as usize] = 0;
        strcpy((*data).dofname[i as usize].as_mut_ptr(), c"".as_ptr());
        i += 1;
    }
    i = 0;
    while i < MAXBODIES as c_int {
        (*data).bodyname[i as usize] = core::ptr::null_mut();
        i += 1;
    }
    i = 0;
    while i < MAXBCS as c_int {
        (*data).boundaryname[i as usize] = core::ptr::null_mut();
        i += 1;
    }
}

/// Allocate knot/element arrays from the counts in `data`.
///
/// # Safety
/// `data` valid; counts set.
#[no_mangle]
pub unsafe extern "C" fn AllocateKnots(data: *mut FemType) {
    let mut i: c_int;
    (*data).topology = imatrix(1, (*data).noelements, 0, (*data).maxnodes - 1);
    (*data).material = ivector(1, (*data).noelements);
    (*data).elementtypes = ivector(1, (*data).noelements);
    i = 1;
    while i <= (*data).noelements {
        *(*data).material.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    i = 1;
    while i <= (*data).noelements {
        *(*data).elementtypes.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    (*data).x = dvector(1, (*data).noknots);
    (*data).y = dvector(1, (*data).noknots);
    (*data).z = dvector(1, (*data).noknots);
    i = 1;
    while i <= (*data).noknots {
        *(*data).x.wrapping_offset(i as isize) = 0.0;
        *(*data).y.wrapping_offset(i as isize) = 0.0;
        *(*data).z.wrapping_offset(i as isize) = 0.0;
        i += 1;
    }
    (*data).created = TRUE;
}
// SAFETY: C function-local statics for the MovePoint* search cursors.
static mut MOVEPOINT_LINEAR_I: c_int = 0;
static mut MOVEPOINT_ANGLE_I: c_int = 0;
static mut MOVEPOINT_POWER_I: c_int = 0;
static mut MOVEPOINT_ARC_I: c_int = 0;

/// SAFETY: circular-arc boundary displacement (pure geometry on valid rows).
unsafe fn MovePointCircle(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x0: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y0: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut r: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut r1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut r2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut p: Real;
    r2 = fabs(*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
    if r2 > fabs(*lim.wrapping_offset(2) - *lim.wrapping_offset(1)) {
        r2 = fabs(*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
    }
    i = 0;
    while i < points / 2 {
        x0 = x - *coords.wrapping_offset((2 * i) as isize);
        r1 = fabs(*coords.wrapping_offset((2 * i + 1) as isize));
        if fabs(x0) >= r2 {
            i += 1;
            continue;
        }
        y0 = y - (*lim.wrapping_offset(1) + *coords.wrapping_offset((2 * i + 1) as isize));
        if y0 < 0.0 && *lim.wrapping_offset(0) > *lim.wrapping_offset(1) {
            i += 1;
            continue;
        }
        if y0 > 0.0 && *lim.wrapping_offset(2) < *lim.wrapping_offset(1) {
            i += 1;
            continue;
        }
        if fabs(y0) >= r2 {
            i += 1;
            continue;
        }
        r = sqrt(x0 * x0 + y0 * y0);
        if r < 1.0e-50 {
            i += 1;
            continue;
        }
        if fabs(x0) > fabs(y0) {
            p = fabs(x0) / r - 1.0;
            if fabs(x0) <= r1 {
                *dx += p * x0;
                *dy += p * y0;
            } else if fabs(x0) <= r2 {
                *dx += p * x0 * (r2 - fabs(x0)) / (r2 - r1);
                *dy += p * y0 * (r2 - fabs(x0)) / (r2 - r1);
            }
        } else {
            p = fabs(y0) / r - 1.0;
            if fabs(y0) <= r1 {
                *dx += p * x0;
                *dy += p * y0;
            } else if fabs(y0) <= r2 {
                *dx += p * x0 * (r2 - fabs(y0)) / (r2 - r1);
                *dy += p * y0 * (r2 - fabs(y0)) / (r2 - r1);
            }
        }
        i += 1;
    }
}

/// SAFETY: piecewise-linear boundary displacement (pure geometry on valid rows).
unsafe fn MovePointLinear(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let c: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let d: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        if x <= *coords.wrapping_offset(0) {
            d = *coords.wrapping_offset(1);
        } else if x >= *coords.wrapping_offset((points - 2) as isize) {
            d = *coords.wrapping_offset((points - 1) as isize);
        } else {
            MOVEPOINT_LINEAR_I = 1;
            while x > *coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I) as isize)
                && MOVEPOINT_LINEAR_I < points / 2 - 1
            {
                MOVEPOINT_LINEAR_I += 1;
            }
            c = (*coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I + 1) as isize)
                - *coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I - 1) as isize))
                / (*coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I) as isize)
                    - *coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I - 2) as isize));
            d = *coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I - 1) as isize)
                + c * (x - *coords.wrapping_offset((2 * MOVEPOINT_LINEAR_I - 2) as isize));
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = dx;
}

/// SAFETY: angular boundary displacement (pure geometry on valid rows).
unsafe fn MovePointAngle(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dz: *mut Real,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z1: Real;
    let degs: Real = FM_PI / 180.0;
    x1 = 0.0;
    z1 = 0.0;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        if x <= *coords.wrapping_offset(0) {
            x1 = x;
        } else {
            MOVEPOINT_ANGLE_I = 1;
            while x > *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I) as isize)
                && MOVEPOINT_ANGLE_I < points / 2
            {
                x1 += cos(
                    degs * *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 1) as isize),
                ) * (*coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I) as isize)
                    - *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 2) as isize));
                z1 += sin(
                    degs * *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 1) as isize),
                ) * (*coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I) as isize)
                    - *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 2) as isize));
                MOVEPOINT_ANGLE_I += 1;
            }
            x1 += cos(
                degs * *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 1) as isize),
            ) * (x - *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 2) as isize));
            z1 += sin(
                degs * *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 1) as isize),
            ) * (x - *coords.wrapping_offset((2 * MOVEPOINT_ANGLE_I - 2) as isize));
        }
        if y < *lim.wrapping_offset(1) {
            *dx += (x1 - x) * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
            *dz += z1 * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dx += (x1 - x) * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
            *dz += z1 * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = points;
}

/// SAFETY: sinusoidal boundary displacement (pure geometry on valid rows).
unsafe fn MovePointSinus(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let c: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let d: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        if x <= *coords.wrapping_offset(0) {
            d = 0.0;
        } else if x >= *coords.wrapping_offset(1) {
            d = *coords.wrapping_offset(3) * sin(*coords.wrapping_offset(2) * 2.0 * FM_PI);
        } else {
            c = *coords.wrapping_offset(2) * 2.0 * FM_PI
                / (*coords.wrapping_offset(1) - *coords.wrapping_offset(0));
            d = *coords.wrapping_offset(3) * sin(c * (x - *coords.wrapping_offset(0)));
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = (dx, points);
}
/// SAFETY: power-series boundary displacement (pure geometry on valid rows).
unsafe fn MovePointPowerSeries(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let n: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut d: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut t: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let u: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        t = x;
        if *coords.wrapping_offset(1) > *coords.wrapping_offset(0) {
            if t < *coords.wrapping_offset(0) {
                t = *coords.wrapping_offset(0);
            }
            if t > *coords.wrapping_offset(1) {
                t = *coords.wrapping_offset(1);
            }
        } else {
            if t > *coords.wrapping_offset(0) {
                t = *coords.wrapping_offset(0);
            }
            if t < *coords.wrapping_offset(1) {
                t = *coords.wrapping_offset(1);
            }
        }
        n = points - 2;
        u = (t - *coords.wrapping_offset(0))
            / (*coords.wrapping_offset(1) - *coords.wrapping_offset(0));
        d = 0.0;
        i = 0;
        while i < n {
            d += *coords.wrapping_offset((i + 2) as isize) * pow(u, i as c_double);
            i += 1;
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = dx;
}

/// SAFETY: Newton-basis power-series displacement (pure geometry on valid rows).
unsafe fn MovePointPowerSeries2(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let n: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut d: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut e: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut t: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let u: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut h: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        t = x;
        if *coords.wrapping_offset(1) > *coords.wrapping_offset(0) {
            if t < *coords.wrapping_offset(0) {
                t = *coords.wrapping_offset(0);
            }
            if t > *coords.wrapping_offset(1) {
                t = *coords.wrapping_offset(1);
            }
        } else {
            if t > *coords.wrapping_offset(0) {
                t = *coords.wrapping_offset(0);
            }
            if t < *coords.wrapping_offset(1) {
                t = *coords.wrapping_offset(1);
            }
        }
        n = points - 2;
        u = (t - *coords.wrapping_offset(0))
            / (*coords.wrapping_offset(1) - *coords.wrapping_offset(0));
        // NOTE: C `d = 0.0` dropped (overwritten by coords[2] below).
        d = *coords.wrapping_offset(2);
        if n >= 1 {
            d += u * *coords.wrapping_offset(3);
        }
        i = 2;
        while i < n {
            h = 1.0 / (i - 1) as Real;
            e = 1.0;
            j = 0;
            while j < i {
                e *= u - j as Real * h;
                j += 1;
            }
            d += *coords.wrapping_offset((i + 2) as isize) * e;
            i += 1;
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = dx;
}

/// SAFETY: power-law boundary displacement (pure geometry on valid rows).
unsafe fn MovePointPower(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let c: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let d: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        if x <= *coords.wrapping_offset(0) {
            d = *coords.wrapping_offset(1);
        } else if x >= *coords.wrapping_offset((points - 2) as isize) {
            d = *coords.wrapping_offset((points - 1) as isize);
        } else {
            MOVEPOINT_POWER_I = 1;
            while x > *coords.wrapping_offset((3 * MOVEPOINT_POWER_I) as isize)
                && MOVEPOINT_POWER_I < points / 3 - 1
            {
                MOVEPOINT_POWER_I += 1;
            }
            c = (*coords.wrapping_offset((3 * MOVEPOINT_POWER_I + 1) as isize)
                - *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 2) as isize))
                / pow(
                    *coords.wrapping_offset((3 * MOVEPOINT_POWER_I) as isize)
                        - *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 3) as isize),
                    *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 1) as isize),
                );
            d = *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 2) as isize)
                + c * pow(
                    x - *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 3) as isize),
                    *coords.wrapping_offset((3 * MOVEPOINT_POWER_I - 1) as isize),
                );
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = dx;
}

/// SAFETY: NACA airfoil displacement (pure geometry on valid rows).
unsafe fn MovePointNACAairfoil(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    let mut t: Real;
    if y < *lim.wrapping_offset(0) || y > *lim.wrapping_offset(2) {
        return;
    }
    if x < *coords.wrapping_offset(0) || x > *coords.wrapping_offset(1) {
        return;
    }
    // NOTE: `if(0)` trace prints omitted (dead in C).
    t = x;
    if *coords.wrapping_offset(1) > *coords.wrapping_offset(0) {
        if t < *coords.wrapping_offset(0) {
            t = *coords.wrapping_offset(0);
        }
        if t > *coords.wrapping_offset(1) {
            t = *coords.wrapping_offset(1);
        }
    } else {
        if t > *coords.wrapping_offset(0) {
            t = *coords.wrapping_offset(0);
        }
        if t < *coords.wrapping_offset(1) {
            t = *coords.wrapping_offset(1);
        }
    }
    let u: Real = (t - *coords.wrapping_offset(0))
        / (*coords.wrapping_offset(1) - *coords.wrapping_offset(0));
    let p: Real = 0.2969 * sqrt(u) - 0.1260 * u - 0.3537 * u * u + 0.2843 * u * u * u
        - 0.1015 * u * u * u * u;
    let d: Real = *coords.wrapping_offset(2) * (*coords.wrapping_offset(1) - *coords.wrapping_offset(0))
        * p
        / 0.2;
    if y < *lim.wrapping_offset(1) {
        *dy += d * (y - *lim.wrapping_offset(0))
            / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
    } else {
        *dy += d * (*lim.wrapping_offset(2) - y)
            / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
    }
    // NOTE: `if(0)` trace print omitted (dead in C).
    let _ = (dx, points);
}

/// SAFETY: circular-segment boundary displacement (pure geometry on valid rows).
unsafe fn MovePointArc(
    lim: *mut Real,
    points: c_int,
    coords: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let sx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let sy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let ss: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let r: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let rat: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let d: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let x0: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let y0: Real;
    if y > *lim.wrapping_offset(0) && y < *lim.wrapping_offset(2) {
        if x <= *coords.wrapping_offset(0) {
            d = *coords.wrapping_offset(1);
        } else if x >= *coords.wrapping_offset((points - 2) as isize) {
            d = *coords.wrapping_offset((points - 1) as isize);
        } else {
            MOVEPOINT_ARC_I = 1;
            while x > *coords.wrapping_offset((3 * MOVEPOINT_ARC_I) as isize)
                && MOVEPOINT_ARC_I < points / 3 - 1
            {
                MOVEPOINT_ARC_I += 1;
            }
            sx = 0.5
                * (*coords.wrapping_offset((3 * MOVEPOINT_ARC_I) as isize)
                    - *coords.wrapping_offset((3 * MOVEPOINT_ARC_I - 3) as isize));
            sy = 0.5
                * (*coords.wrapping_offset((3 * MOVEPOINT_ARC_I + 1) as isize)
                    - *coords.wrapping_offset((3 * MOVEPOINT_ARC_I - 2) as isize));
            r = *coords.wrapping_offset((3 * MOVEPOINT_ARC_I - 1) as isize);
            ss = sx * sx + sy * sy;
            rat = sqrt(1.0 / ss - 1.0 / (r * r)) * r;
            x0 = *coords.wrapping_offset((3 * MOVEPOINT_ARC_I - 3) as isize) + sx - sy * rat;
            y0 = *coords.wrapping_offset((3 * MOVEPOINT_ARC_I - 2) as isize) + sy + sx * rat;
            d = y0 - sqrt(r * r - (x - x0) * (x - x0)) * r / fabs(r);
        }
        if y < *lim.wrapping_offset(1) {
            *dy += d * (y - *lim.wrapping_offset(0))
                / (*lim.wrapping_offset(1) - *lim.wrapping_offset(0));
        } else {
            *dy += d * (*lim.wrapping_offset(2) - y)
                / (*lim.wrapping_offset(2) - *lim.wrapping_offset(1));
        }
    }
    let _ = dx;
}
/// SAFETY: mapping-type dispatch (positive/negative mode arm pairing).
#[allow(clippy::too_many_arguments)]
unsafe fn apply_mapping(
    mode: c_int,
    maplim: *mut Real,
    points: c_int,
    params: *mut Real,
    x: Real,
    y: Real,
    dx: *mut Real,
    dy: *mut Real,
    dz: *mut Real,
) {
    // NOTE: C switch over mode; negative modes swap (x,y) and (dx,dy).
    if mode == 1 {
        MovePointLinear(maplim, points, params, x, y, dx, dy);
    } else if mode == 2 {
        MovePointPower(maplim, points, params, x, y, dx, dy);
    } else if mode == 3 {
        MovePointArc(maplim, points, params, x, y, dx, dy);
    } else if mode == 4 {
        MovePointCircle(maplim, points, params, x, y, dx, dy);
    } else if mode == 5 {
        MovePointSinus(maplim, points, params, x, y, dx, dy);
    } else if mode == 6 {
        MovePointPowerSeries(maplim, points, params, x, y, dx, dy);
    } else if mode == 7 {
        MovePointPowerSeries2(maplim, points, params, x, y, dx, dy);
    } else if mode == 8 {
        MovePointAngle(maplim, points, params, x, y, dx, dz);
    } else if mode == 9 {
        MovePointNACAairfoil(maplim, points, params, x, y, dx, dy);
    } else if mode == -1 {
        MovePointLinear(maplim, points, params, y, x, dy, dx);
    } else if mode == -2 {
        MovePointPower(maplim, points, params, y, x, dy, dx);
    } else if mode == -3 {
        MovePointArc(maplim, points, params, y, x, dy, dx);
    } else if mode == -4 {
        MovePointCircle(maplim, points, params, y, x, dy, dx);
    } else if mode == -5 {
        MovePointSinus(maplim, points, params, y, x, dy, dx);
    } else if mode == -6 {
        MovePointPowerSeries(maplim, points, params, y, x, dy, dx);
    } else if mode == -7 {
        MovePointPowerSeries2(maplim, points, params, y, x, dy, dx);
    } else if mode == -8 {
        MovePointAngle(maplim, points, params, y, x, dy, dz);
    } else if mode == -9 {
        MovePointNACAairfoil(maplim, points, params, y, x, dy, dx);
    }
}

/// Build rectangular-grid knots, then apply boundary mappings.
///
/// # Safety
/// `grid`, `cell`, `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateKnots(
    grid: *mut GridType,
    cell: *mut CellType,
    data: *mut FemType,
    noknots: c_int,
    info: c_int,
) {
    let mut globalcoord: [Real; DIM as usize * MAXNODESD2] = [0.0; DIM as usize * MAXNODESD2];
    let mut maplim: [Real; 3 * MAXMAPPINGS] = [0.0; 3 * MAXMAPPINGS];
    // NOTE: `=0` decl init kept (material read before any overwrite).
    let mut material: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemind: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mode: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut level: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maplevel: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut celli: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cellj: c_int;
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut l: c_int;
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut size: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minsize: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxsize: Real;
    // NOTE: extern provider (native unit); declared at crate top.
    InitializeKnots(data);
    (*data).dim = (*grid).dimension;
    let dim: c_int = (*grid).dimension;
    let nonodes: c_int = (*grid).nonodes;
    (*data).maxnodes = (*grid).nonodes;
    (*data).nocells = (*grid).nocells;
    (*data).noelements = (*grid).noelements;
    (*data).coordsystem = (*grid).coordsystem;
    (*data).numbering = (*grid).numbering;
    (*data).indexwidth = (*grid).maxwidth;
    // NOTE: C MAX macro (egdef.h:97); arms pure.
    if (*data).noknots < noknots.max((*grid).noknots) {
        (*data).noknots = noknots.max((*grid).noknots);
    }
    (*data).noboundaries = (*grid).noboundaries;
    AllocateKnots(data);
    // NOTE: C first `minsize = 1.0e20` dropped (reset before size loop).
    let elemtype: c_int = if dim == 1 {
        (*grid).nonodes + 200
    } else {
        (*grid).nonodes + 400
    };
    i = 1;
    while i <= (*data).noelements {
        *(*data).elementtypes.wrapping_offset(i as isize) = elemtype;
        i += 1;
    }
    // NOTE: elements numbered the same way as knots.
    cellj = 1;
    while cellj <= (*grid).ycells {
        j = 1;
        while j <= (*grid).yelems[cellj as usize] {
            celli = 1;
            while celli <= (*grid).xcells {
                if (*grid).numbered[cellj as usize][celli as usize] != 0 {
                    k = (*grid).numbered[cellj as usize][celli as usize];
                    material = (*cell.wrapping_offset(k as isize)).material;
                    i = 1;
                    while i <= (*grid).xelems[celli as usize] {
                        elemind = GetElementCoordinates(
                            cell.wrapping_offset(k as isize),
                            i,
                            j,
                            globalcoord.as_mut_ptr(),
                            ind.as_mut_ptr(),
                        );
                        if (*data).noknots == (*grid).noknots {
                            l = 0;
                            while l < nonodes {
                                *(*(*data).topology.wrapping_offset(elemind as isize))
                                    .wrapping_offset(l as isize) = ind[l as usize];
                                *(*data).x.wrapping_offset(ind[l as usize] as isize) =
                                    globalcoord[l as usize];
                                *(*data).y.wrapping_offset(ind[l as usize] as isize) =
                                    globalcoord[(l + nonodes) as usize];
                                l += 1;
                            }
                            *(*data).material.wrapping_offset(elemind as isize) = material;
                        }
                        i += 1;
                    }
                }
                celli += 1;
            }
            j += 1;
        }
        cellj += 1;
    }
    // NOTE: map limits from the grid mapping lines.
    k = 0;
    while k < (*grid).mappings {
        j = (*grid).mappingline[k as usize];
        if (*grid).mappingtype[k as usize] > 0 {
            maplim[(3 * k + 1) as usize] = (*grid).y[j as usize];
        } else if (*grid).mappingtype[k as usize] < 0 {
            maplim[(3 * k + 1) as usize] = (*grid).x[j as usize];
        } else {
            k += 1;
            continue;
        }
        maplim[(3 * k) as usize] = maplim[(3 * k + 1) as usize]
            - (*grid).mappinglimits[(2 * k) as usize];
        maplim[(3 * k + 2) as usize] = maplim[(3 * k + 1) as usize]
            + (*grid).mappinglimits[(2 * k + 1) as usize];
        k += 1;
    }
    mode = 0;
    if (*grid).mappings != 0 {
        level = 0;
        while level < 10 {
            maplevel = FALSE;
            k = 0;
            while k < (*grid).mappings {
                if abs((*grid).mappingtype[k as usize] / 10) == level {
                    maplevel = TRUE;
                }
                k += 1;
            }
            if maplevel == FALSE {
                level += 1;
                continue;
            }
            if level >= 5 {
                (*data).dim = 3;
            }
            i = 1;
            while i <= (*data).noknots {
                x = *(*data).x.wrapping_offset(i as isize);
                y = *(*data).y.wrapping_offset(i as isize);
                dx = 0.0;
                dy = 0.0;
                dz = 0.0;
                k = 0;
                while k < (*grid).mappings {
                    mode = (*grid).mappingtype[k as usize] % 10;
                    apply_mapping(
                        mode,
                        maplim.as_mut_ptr().wrapping_offset((3 * k) as isize),
                        (*grid).mappingpoints[k as usize],
                        (*grid).mappingparams[k as usize],
                        x,
                        y,
                        &mut dx,
                        &mut dy,
                        &mut dz,
                    );
                    k += 1;
                }
                if mode == 8 || mode == -8 {
                    *(*data).x.wrapping_offset(i as isize) += dx;
                    *(*data).y.wrapping_offset(i as isize) += dy;
                    *(*data).z.wrapping_offset(i as isize) += dz;
                } else if level >= 5 {
                    *(*data).z.wrapping_offset(i as isize) += dx + dy;
                } else {
                    *(*data).x.wrapping_offset(i as isize) += dx;
                    *(*data).y.wrapping_offset(i as isize) += dy;
                }
                i += 1;
            }
            level += 1;
        }
    }
    minsize = 1.0e20;
    maxsize = 0.0;
    i = 1;
    while i <= (*data).noelements {
        GetElementInfo(i, data, globalcoord.as_mut_ptr(), ind.as_mut_ptr(), &mut material);
        dx = globalcoord[0] - globalcoord[1];
        dy = globalcoord[nonodes as usize] - globalcoord[(nonodes + 1) as usize];
        size = dx * dx + dy * dy;
        if size < minsize {
            minsize = size;
        }
        if size > maxsize {
            maxsize = size;
        }
        dx = globalcoord[0] - globalcoord[(nonodes - 1) as usize];
        dy = globalcoord[nonodes as usize] - globalcoord[(2 * nonodes - 1) as usize];
        size = dx * dx + dy * dy;
        if size < minsize {
            minsize = size;
        }
        if size > maxsize {
            maxsize = size;
        }
        i += 1;
    }
    (*data).maxsize = sqrt(maxsize);
    (*data).minsize = sqrt(minsize);
    if info != 0 {
        printf(
            c"Maximum elementsize is %.3e and minimum %.3e.\n".as_ptr(),
            (*data).maxsize,
            (*data).minsize,
        );
    }
}
/// Create (or verify) a named dof variable on the mesh.
///
/// # Safety
/// `data`, `dofname` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateVariable(
    data: *mut FemType,
    variable: c_int,
    unknowns: c_int,
    value: Real,
    dofname: *const c_char,
    eorder: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: `=FALSE` decl init kept (read before any overwrite).
    let info: c_int = FALSE;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut timesteps: c_int;
    let _ = eorder;
    if variable == 0 {
        return 0;
    }
    if (*data).created == FALSE {
        if info != 0 {
            printf(c"CreateVariable: Knots must first be created!\n".as_ptr());
        }
        return 1;
    }
    timesteps = (*data).timesteps;
    if timesteps < 1 {
        timesteps = 1;
    }
    if (*data).edofs[variable as usize] == 0 {
        (*data).variables += 1;
        (*data).edofs[variable as usize] = unknowns;
        (*data).alldofs[variable as usize] = unknowns * (*data).noknots;
        (*data).bandwidth[variable as usize] = unknowns * (*data).indexwidth;
        (*data).dofs[variable as usize] =
            dvector(1, timesteps * (*data).alldofs[variable as usize]);
        if info != 0 {
            printf(
                c"Created variable %s with %d dofs.\n".as_ptr(),
                dofname,
                (*data).alldofs[variable as usize],
            );
        }
        i = 1;
        while i <= (*data).alldofs[variable as usize] * timesteps {
            *(*data).dofs[variable as usize].wrapping_offset(i as isize) = value;
            i += 1;
        }
    } else if (*data).edofs[variable as usize] == unknowns {
        if info != 0 {
            printf(
                c"CreateVariable: Variable %d exists with correct number of dofs!\n".as_ptr(),
                variable,
            );
        }
    } else {
        if info != 0 {
            printf(
                c"CreateVariable: Variable %d exists with wrong number of dofs!\n".as_ptr(),
                variable,
            );
        }
        return 2;
    }
    strcpy((*data).dofname[variable as usize].as_mut_ptr(), dofname);
    0
}

/// Free all knot/element storage.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyKnots(data: *mut FemType) {
    let mut i: c_int;
    if (*data).created == 0 {
        return;
    }
    (*data).created = FALSE;
    i = 0;
    while i < MAXDOFS as c_int {
        if (*data).edofs[i as usize] != 0 {
            if (*data).edofs[i as usize] > 0 {
                free_dvector(
                    (*data).dofs[i as usize],
                    1,
                    (*data).alldofs[i as usize],
                );
            }
            (*data).edofs[i as usize] = 0;
        }
        i += 1;
    }
    free_imatrix(
        (*data).topology,
        1,
        (*data).noelements,
        0,
        (*data).maxnodes - 1,
    );
    free_ivector((*data).material, 1, (*data).noelements);
    free_ivector((*data).elementtypes, 1, (*data).noelements);
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).noknots = 0;
    (*data).noelements = 0;
    (*data).maxnodes = 0;
    if (*data).nocorners > 0 {
        free_ivector((*data).corners, 1, 2 * (*data).nocorners);
    }
}
/// SAFETY: CreateBoundary cell scan (split for sizing); ports startpoint passes.
#[allow(clippy::too_many_arguments)]
unsafe fn create_boundary_scan(
    cell: *mut CellType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    material1: c_int,
    material2: c_int,
    solidmat: c_int,
    boundarytype: c_int,
    dimsides: c_int,
    nosides: &mut c_int,
    no: &mut c_int,
    side: &mut c_int,
    elem: &mut c_int,
    more: &mut c_int,
    elemind: *mut c_int,
    times: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut setpoint: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidemat: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut thismat: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cellside: c_int;
    *nosides = 0;
    *no = 1;
    while *no <= (*data).nocells {
        *side = 0;
        while *side < dimsides {
            if (*data).dim == 1 {
                cellside = 3 - 2 * *side;
            } else {
                cellside = *side;
            }
            setpoint = FALSE;
            sidemat = (*cell.wrapping_offset(*no as isize)).boundary[cellside as usize];
            thismat = (*cell.wrapping_offset(*no as isize)).material;
            // NOTE: free BCs disallowed with negative keywords (comment only).
            if material1 >= 0 && material1 != sidemat {
                *side += 1;
                continue;
            }
            if material2 >= 0 && material2 != thismat {
                *side += 1;
                continue;
            }
            // NOTE: `#if 0` trace omitted (dead in C).
            if material2 == -((*side + 2) % 4 + 1) && sidemat == material1 && sidemat != thismat
            {
                setpoint = TRUE;
            }
            if material1 == -(*side + 1) && thismat == material2 && sidemat != thismat {
                setpoint = TRUE;
            }
            if material1 == MAT_BIGGER && sidemat > material2 {
                setpoint = TRUE;
            }
            if material1 == MAT_SMALLER && sidemat < material2 {
                setpoint = TRUE;
            }
            if material1 == MAT_ANYTHING && sidemat != material2 {
                setpoint = TRUE;
            }
            if material2 == MAT_BIGGER && thismat > material1 {
                setpoint = TRUE;
            }
            if material2 == MAT_SMALLER && thismat < material1 {
                setpoint = TRUE;
            }
            if material2 == MAT_ANYTHING && thismat != material1 {
                setpoint = TRUE;
            }
            if sidemat == material1 && thismat == material2 {
                setpoint = TRUE;
            }
            if setpoint == TRUE {
                // NOTE: `#if 0` trace omitted (dead in C).
                *elem = 0;
                loop {
                    *elem += 1;
                    *nosides += 1;
                    *more = GetSideInfo(cell, *no, *side, *elem, elemind);
                    // NOTE: `#if 0` trace omitted (dead in C).
                    if times != 0 {
                        // NOTE: solidmat side sets the surface properties.
                        *(*bound).parent.wrapping_offset(*nosides as isize) =
                            *elemind.wrapping_offset(0);
                        *(*bound).parent2.wrapping_offset(*nosides as isize) =
                            *elemind.wrapping_offset(1);
                        *(*bound).side.wrapping_offset(*nosides as isize) = *side;
                        *(*bound).side2.wrapping_offset(*nosides as isize) =
                            (*side + dimsides / 2) % dimsides;
                        *(*bound).types.wrapping_offset(*nosides as isize) = boundarytype;
                        if solidmat == FIRST {
                            *(*bound).material.wrapping_offset(*nosides as isize) = sidemat;
                            *(*bound).normal.wrapping_offset(*nosides as isize) = 1;
                        }
                        if solidmat == SECOND {
                            *(*bound).material.wrapping_offset(*nosides as isize) = thismat;
                            *(*bound).normal.wrapping_offset(*nosides as isize) = -1;
                        }
                    }
                    if *more == 0 {
                        break;
                    }
                }
            }
            *side += 1;
        }
        *no += 1;
    }
}

/// Boundary of sides separating two materials (two-pass count + fill).
///
/// # Safety
/// `cell`, `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateBoundary(
    cell: *mut CellType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    material1: c_int,
    material2: c_int,
    mut solidmat: c_int,
    boundarytype: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in the scanner before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the scanner before reads).
    let mut more: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the scanner before reads).
    let mut elem: c_int = 0;
    let mut elemind: [c_int; 2] = [0; 2];
    // NOTE: 0-init for the &mut handoff (set in the scanner before reads).
    let mut nosides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in the scanner before reads).
    let mut no: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut times: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut size: c_int;
    let _ = i;
    let dimsides: c_int = if (*data).dim == 1 { 2 } else { 4 };
    if (*bound).created == TRUE {
        if info != 0 {
            printf(c"CreateBoundary: You tried to recreate the boundary!\n".as_ptr());
        }
        return 1;
    }
    if (*data).created == 0 {
        if info != 0 {
            printf(c"CreateBoundary: You tried to create a boundary before the knots were made.".as_ptr());
        }
        return 2;
    }
    if material1 < 0 && material2 < 0 {
        if info != 0 {
            printf(c"CreateBoundary: the material arguments are both negative".as_ptr());
        }
        return 3;
    }
    times = 0;
    (*bound).created = FALSE;
    (*bound).nosides = 0;
    if solidmat >= 2 {
        solidmat -= 2;
    }
    // NOTE: C `startpoint:` label + `goto startpoint`.
    loop {
        create_boundary_scan(
            cell, data, bound, material1, material2, solidmat, boundarytype, dimsides,
            &mut nosides, &mut no, &mut side, &mut elem, &mut more,
            elemind.as_mut_ptr(), times,
        );
        if nosides == 0 {
            if info != 0 {
                printf(
                    c"No boundary between materials %d and %d exists.\n".as_ptr(),
                    material1,
                    material2,
                );
            }
            return 0;
        }
        if times == 0 {
            times += 1;
            // NOTE: allocation happens on the first pass only.
            (*bound).created = TRUE;
            (*bound).nosides = nosides;
            size = nosides;
            let _ = size;
            (*bound).coordsystem = (*data).coordsystem;
            (*bound).types = ivector(1, nosides);
            (*bound).side = ivector(1, nosides);
            (*bound).side2 = ivector(1, nosides);
            (*bound).material = ivector(1, nosides);
            (*bound).parent = ivector(1, nosides);
            (*bound).parent2 = ivector(1, nosides);
            (*bound).normal = ivector(1, nosides);
            (*bound).echain = FALSE;
            (*bound).ediscont = FALSE;
            continue;
        }
        break;
    }
    if info != 0 {
        printf(
            c"%d element sides between materials %d and %d were located to type %d.\n".as_ptr(),
            nosides,
            material1,
            material2,
            boundarytype,
        );
    }
    0
}

/// Allocate empty boundary arrays of `size` sides.
///
/// # Safety
/// `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn AllocateBoundary(bound: *mut BoundaryType, size: c_int) -> c_int {
    let mut i: c_int;
    if (*bound).created == TRUE {
        printf(c"AllocateBoundary: You tried to recreate the boundary!\n".as_ptr());
        return 1;
    }
    (*bound).created = TRUE;
    (*bound).nosides = size;
    (*bound).echain = FALSE;
    (*bound).ediscont = FALSE;
    (*bound).material = ivector(1, size);
    (*bound).side = ivector(1, size);
    (*bound).side2 = ivector(1, size);
    (*bound).parent = ivector(1, size);
    (*bound).parent2 = ivector(1, size);
    (*bound).types = ivector(1, size);
    (*bound).normal = ivector(1, size);
    i = 1;
    while i <= size {
        *(*bound).material.wrapping_offset(i as isize) = 0;
        *(*bound).side.wrapping_offset(i as isize) = 0;
        *(*bound).side2.wrapping_offset(i as isize) = 0;
        *(*bound).parent.wrapping_offset(i as isize) = 0;
        *(*bound).parent2.wrapping_offset(i as isize) = 0;
        *(*bound).types.wrapping_offset(i as isize) = 0;
        *(*bound).normal.wrapping_offset(i as isize) = 1;
        i += 1;
    }
    0
}

/// Free boundary arrays.
///
/// # Safety
/// `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyBoundary(bound: *mut BoundaryType) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut i: c_int;
    let _ = i;
    if (*bound).created == 0 {
        return 1;
    }
    let nosides: c_int = (*bound).nosides;
    if nosides == 0 {
        (*bound).created = FALSE;
        return 2;
    }
    free_ivector((*bound).material, 1, nosides);
    free_ivector((*bound).side, 1, nosides);
    free_ivector((*bound).side2, 1, nosides);
    free_ivector((*bound).parent, 1, nosides);
    free_ivector((*bound).parent2, 1, nosides);
    free_ivector((*bound).types, 1, nosides);
    free_ivector((*bound).normal, 1, nosides);
    (*bound).nosides = 0;
    (*bound).created = FALSE;
    // NOTE: `#if DEBUG` print omitted (DEBUG is 0 in C).
    0
}
/// SAFETY: CreatePoints corner dispatch (split for sizing).
unsafe fn points_corner_elem(
    cell: *mut CellType,
    no: c_int,
    corner: c_int,
) -> c_int {
    // NOTE: C if/else chain over the corner code.
    if corner == BOTLEFT {
        GetElementIndex(cell.wrapping_offset(no as isize), 1, 1)
    } else if corner == BOTRIGHT {
        GetElementIndex(
            cell.wrapping_offset(no as isize),
            (*cell.wrapping_offset(no as isize)).xelem,
            1,
        )
    } else if corner == TOPRIGHT {
        GetElementIndex(
            cell.wrapping_offset(no as isize),
            (*cell.wrapping_offset(no as isize)).xelem,
            (*cell.wrapping_offset(no as isize)).yelem,
        )
    } else if corner == TOPLEFT {
        GetElementIndex(
            cell.wrapping_offset(no as isize),
            1,
            (*cell.wrapping_offset(no as isize)).yelem,
        )
    } else {
        0
    }
}

/// Corner points of a material pair (two-pass count + fill).
///
/// # Safety
/// `cell`, `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn CreatePoints(
    cell: *mut CellType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    param1: c_int,
    param2: c_int,
    pointmode: c_int,
    pointtype: c_int,
    info: c_int,
) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut size: c_int;
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut no: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut corner: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut times: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut node: c_int;
    (*bound).created = FALSE;
    (*bound).nosides = 0;
    times = 0;
    // NOTE: C `omstart:` label + `goto omstart`.
    loop {
        i = 0;
        // NOTE: nodes divided by the two given materials.
        if pointmode == 4 {
            no = 1;
            while no <= (*data).nocells {
                if (*cell.wrapping_offset(no as isize)).material == param2 {
                    corner = 0;
                    while corner < 4 {
                        if (*cell.wrapping_offset(no as isize)).boundary[(4 + corner) as usize]
                            == param1
                        {
                            i += 1;
                            if times != 0 {
                                *(*bound).material.wrapping_offset(i as isize) = param2;
                                *(*bound).types.wrapping_offset(i as isize) = pointtype;
                                *(*bound).side.wrapping_offset(i as isize) = 4 + corner;
                                if corner == BOTLEFT {
                                    elem = GetElementIndex(
                                        cell.wrapping_offset(no as isize),
                                        1,
                                        1,
                                    );
                                } else if corner == BOTRIGHT {
                                    elem = GetElementIndex(
                                        cell.wrapping_offset(no as isize),
                                        (*cell.wrapping_offset(no as isize)).xelem,
                                        1,
                                    );
                                } else if corner == TOPRIGHT {
                                    elem = GetElementIndex(
                                        cell.wrapping_offset(no as isize),
                                        (*cell.wrapping_offset(no as isize)).xelem,
                                        (*cell.wrapping_offset(no as isize)).yelem,
                                    );
                                } else if corner == TOPLEFT {
                                    elem = GetElementIndex(
                                        cell.wrapping_offset(no as isize),
                                        1,
                                        (*cell.wrapping_offset(no as isize)).yelem,
                                    );
                                }
                                *(*bound).parent.wrapping_offset(i as isize) = elem;
                            }
                        }
                        corner += 1;
                    }
                }
                no += 1;
            }
        }
        if pointmode == 5 {
            corner = -1;
            no = 1;
            while no <= (*data).nocells && corner < 0 {
                if (*cell.wrapping_offset(no as isize)).xind - 1 == param1
                    && (*cell.wrapping_offset(no as isize)).yind - 1 == param2
                {
                    corner = BOTLEFT;
                } else if (*cell.wrapping_offset(no as isize)).xind == param1
                    && (*cell.wrapping_offset(no as isize)).yind - 1 == param2
                {
                    corner = BOTRIGHT;
                } else if (*cell.wrapping_offset(no as isize)).xind == param1
                    && (*cell.wrapping_offset(no as isize)).yind == param2
                {
                    corner = TOPRIGHT;
                } else if (*cell.wrapping_offset(no as isize)).xind - 1 == param1
                    && (*cell.wrapping_offset(no as isize)).yind == param2
                {
                    corner = TOPLEFT;
                }
                no += 1;
            }
            if corner >= 0 {
                i += 1;
                no -= 1;
            }
            if times != 0 {
                *(*bound).types.wrapping_offset(i as isize) = pointtype;
                *(*bound).side.wrapping_offset(i as isize) = 4 + corner;
                elem = points_corner_elem(cell, no, corner);
                *(*bound).parent.wrapping_offset(i as isize) = elem;
                node = *(*(*data).topology.wrapping_offset(elem as isize))
                    .wrapping_offset(corner as isize);
                if info != 0 {
                    printf(
                        c"Found node %d at (%.3lg, %.3lg)\n".as_ptr(),
                        node,
                        *(*data).x.wrapping_offset(node as isize),
                        *(*data).y.wrapping_offset(node as isize),
                    );
                }
            }
        }
        size = i;
        if times == 0 && size > 0 {
            AllocateBoundary(bound, size);
            times = 1;
            continue;
        }
        break;
    }
    if info != 0 {
        printf(
            c"Created %d new points of type %d in the corner of materials %d and %d.\n".as_ptr(),
            size,
            pointtype,
            param1,
            param2,
        );
    }
    FALSE
}
// SAFETY: C function-local statics for the discontinuous-boundary hit table.
static mut DISCONT_HITSEXIST: c_int = FALSE;
static mut DISCONT_HITSLENGTH: c_int = 0;
static mut DISCONT_HITS: *mut c_int = core::ptr::null_mut();

/// Duplicate knots flagged by `order` for a discontinuous boundary.
///
/// # Safety
/// `data`, `order` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateNewNodes(
    data: *mut FemType,
    order: *mut c_int,
    material: c_int,
    newknots: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    let mut l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut lmax: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    let mut newdofs: [*mut Real; MAXDOFS] = [core::ptr::null_mut(); MAXDOFS];
    let noknots: c_int = (*data).noknots;
    printf(
        c"Creating %d new nodes for discontinuous boundary.\n".as_ptr(),
        newknots,
    );
    // NOTE: room for the duplicated nodes.
    let newsize: c_int = noknots + newknots;
    let newx: *mut Real = dvector(1, newsize);
    let newy: *mut Real = dvector(1, newsize);
    let newz: *mut Real = dvector(1, newsize);
    let neworder: *mut c_int = ivector(1, newsize);
    j = 1;
    while j < MAXDOFS as c_int {
        if (*data).edofs[j as usize] != 0 {
            newdofs[j as usize] = dvector(1, (*data).edofs[j as usize] * newsize);
        }
        j += 1;
    }
    // NOTE: copy knots, duplicating the flagged ones.
    j = 0;
    i = 1;
    while i <= noknots {
        j += 1;
        *neworder.wrapping_offset(j as isize) = i;
        *newx.wrapping_offset(j as isize) = *(*data).x.wrapping_offset(i as isize);
        *newy.wrapping_offset(j as isize) = *(*data).y.wrapping_offset(i as isize);
        *newz.wrapping_offset(j as isize) = *(*data).z.wrapping_offset(i as isize);
        k = 1;
        while k < MAXDOFS as c_int {
            lmax = (*data).edofs[k as usize];
            if lmax != 0 {
                l = 1;
                while l <= lmax {
                    *newdofs[k as usize].wrapping_offset((lmax * (j - 1) + l) as isize) =
                        *(*data).dofs[k as usize]
                            .wrapping_offset((lmax * (i - 1) + l) as isize);
                    l += 1;
                }
            }
            k += 1;
        }
        if *order.wrapping_offset(i as isize) < 0 {
            j += 1;
            *neworder.wrapping_offset(j as isize) = -i;
            *newx.wrapping_offset(j as isize) = *(*data).x.wrapping_offset(i as isize);
            *newy.wrapping_offset(j as isize) = *(*data).y.wrapping_offset(i as isize);
            *newz.wrapping_offset(j as isize) = *(*data).z.wrapping_offset(i as isize);
            k = 1;
            while k < MAXDOFS as c_int {
                lmax = (*data).edofs[k as usize];
                if lmax != 0 {
                    l = 1;
                    while l <= lmax {
                        *newdofs[k as usize]
                            .wrapping_offset((lmax * (j - 1) + l) as isize) =
                            *(*data).dofs[k as usize]
                                .wrapping_offset((lmax * (i - 1) + l) as isize);
                        l += 1;
                    }
                }
                k += 1;
            }
        }
        i += 1;
    }
    // NOTE: old index for every new one.
    i = 1;
    while i <= newsize {
        if *neworder.wrapping_offset(i as isize) > 0 {
            if *order.wrapping_offset(*neworder.wrapping_offset(i as isize) as isize) < 0 {
                *order.wrapping_offset(*neworder.wrapping_offset(i as isize) as isize) = -i;
            } else {
                *order.wrapping_offset(*neworder.wrapping_offset(i as isize) as isize) = i;
            }
        }
        i += 1;
    }
    // NOTE: repoint the material-flagged elements at the duplicates.
    i = 1;
    while i <= (*data).noelements {
        nonodes = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
        j = 0;
        while j < nonodes {
            ind = *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize);
            if *(*data).material.wrapping_offset(i as isize) == material
                && *order.wrapping_offset(ind as isize) < 0
            {
                *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize) =
                    abs(*order.wrapping_offset(ind as isize)) + 1;
            } else {
                *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize) = abs(*order.wrapping_offset(ind as isize));
            }
            j += 1;
        }
        i += 1;
    }
    // NOTE: swap in the new vectors, freeing the old ones.
    free_dvector((*data).x, 1, noknots);
    free_dvector((*data).y, 1, noknots);
    free_dvector((*data).z, 1, noknots);
    k = 1;
    while k < MAXDOFS as c_int {
        if (*data).edofs[k as usize] != 0 {
            free_dvector((*data).dofs[k as usize], 1, noknots);
        }
        k += 1;
    }
    (*data).noknots = newsize;
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    k = 1;
    while k < MAXDOFS as c_int {
        if (*data).edofs[k as usize] != 0 {
            (*data).dofs[k as usize] = newdofs[k as usize];
            (*data).alldofs[k as usize] = (*data).edofs[k as usize] * (*data).noknots;
        }
        k += 1;
    }
    0
}
/// SAFETY: discontinuous double-side scan (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn discont_scan(
    data: *mut FemType,
    bound: *mut BoundaryType,
    boundtype: c_int,
    doublesides: &mut c_int,
    mat1old: &mut c_int,
    mat2old: &mut c_int,
    bc: &mut c_int,
    i: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut par1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut par2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mat1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mat2: c_int;
    *bc = 0;
    while *bc < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*bc as isize)).created == FALSE {
            *bc += 1;
            continue;
        }
        if (*bound.wrapping_offset(0)).nosides == 0 {
            *bc += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*bc as isize)).nosides {
            if *(*bound.wrapping_offset(*bc as isize))
                .types
                .wrapping_offset(*i as isize)
                == boundtype
            {
                par1 = *(*bound.wrapping_offset(*bc as isize))
                    .parent
                    .wrapping_offset(*i as isize);
                par2 = *(*bound.wrapping_offset(*bc as isize))
                    .parent2
                    .wrapping_offset(*i as isize);
                if par1 != 0 && par2 != 0 {
                    *doublesides += 1;
                    mat1 = *(*data).material.wrapping_offset(par1 as isize);
                    mat2 = *(*data).material.wrapping_offset(par2 as isize);
                    if *mat1old == 0 {
                        *mat1old = mat1;
                    } else if *mat1old != mat1 {
                        *mat1old = -mat1;
                    }
                    if *mat2old == 0 {
                        *mat2old = mat2;
                    } else if *mat2old != mat2 {
                        *mat2old = -mat2;
                    }
                }
            }
            *i += 1;
        }
        *bc += 1;
    }
}
/// SAFETY: discontinuous endnode hit counts (split for sizing).
unsafe fn discont_hits(
    data: *mut FemType,
    noelements: c_int,
    nonodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    endnodes: c_int,
) {
    // NOTE: C truncates the 1.1 double scale to int (same as `as`).
    if endnodes == 1 {
        if DISCONT_HITSEXIST == 0 {
            DISCONT_HITSLENGTH = (1.1 * (*data).noknots as Real) as c_int;
            DISCONT_HITS = ivector(1, DISCONT_HITSLENGTH);
            DISCONT_HITSEXIST = TRUE;
        } else if DISCONT_HITSLENGTH <= (*data).noknots {
            free_ivector(DISCONT_HITS, 1, DISCONT_HITSLENGTH);
            DISCONT_HITSLENGTH = (1.1 * (*data).noknots as Real) as c_int;
            DISCONT_HITS = ivector(1, DISCONT_HITSLENGTH);
        }
        *i = 1;
        while *i <= (*data).noknots {
            *DISCONT_HITS.wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
        *j = 1;
        while *j <= noelements {
            *nonodes = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
            *i = 0;
            while *i < *nonodes {
                (*DISCONT_HITS.wrapping_offset(
                    *(*(*data).topology.wrapping_offset(*j as isize))
                        .wrapping_offset(*i as isize) as isize,
                )) += 1;
                *i += 1;
            }
            *j += 1;
        }
    }
}

/// SAFETY: discontinuous node-flag pass (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn discont_order(
    data: *mut FemType,
    bound: *mut BoundaryType,
    boundtype: c_int,
    disconttype: c_int,
    endnodes: c_int,
    newnodes: &mut c_int,
    order: *mut c_int,
    sideind: *mut c_int,
    sideelemtype: &mut c_int,
    sidenodes: &mut c_int,
    parent: &mut c_int,
    side: &mut c_int,
    ind: &mut c_int,
    bc: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
) {
    *bc = 0;
    while *bc < MAXBOUNDARIES as c_int {
        *i = 1;
        while *i <= (*bound.wrapping_offset(*bc as isize)).nosides {
            if *(*bound.wrapping_offset(*bc as isize))
                .types
                .wrapping_offset(*i as isize)
                != boundtype
            {
                *i += 1;
                continue;
            }
            if (*bound.wrapping_offset(*bc as isize)).ediscont == 0 {
                (*bound.wrapping_offset(*bc as isize)).discont =
                    ivector(1, (*bound.wrapping_offset(*bc as isize)).nosides);
                *j = 1;
                while *j <= (*bound.wrapping_offset(*bc as isize)).nosides {
                    *(*bound.wrapping_offset(*bc as isize))
                        .discont
                        .wrapping_offset(*j as isize) = 0;
                    *j += 1;
                }
                (*bound.wrapping_offset(*bc as isize)).ediscont = TRUE;
            }
            *parent = *(*bound.wrapping_offset(*bc as isize))
                .parent2
                .wrapping_offset(*i as isize);
            if *parent == 0 {
                *i += 1;
                continue;
            }
            *side = *(*bound.wrapping_offset(*bc as isize))
                .side2
                .wrapping_offset(*i as isize);
            GetElementSide(*parent, *side, 1, data, sideind, sideelemtype);
            *sidenodes = *sideelemtype % 100;
            *(*bound.wrapping_offset(*bc as isize))
                .discont
                .wrapping_offset(*i as isize) = disconttype;
            *j = 0;
            while *j < *sidenodes {
                *ind = abs(*sideind.wrapping_offset(*j as isize));
                if endnodes == 2 {
                    if *order.wrapping_offset(*ind as isize) > 0 {
                        *newnodes += 1;
                        *order.wrapping_offset(*ind as isize) = -*newnodes;
                    }
                } else if endnodes == 0 {
                    if *order.wrapping_offset(*ind as isize) > 0 {
                        *order.wrapping_offset(*ind as isize) = 0;
                    } else if *order.wrapping_offset(*ind as isize) == 0 {
                        *newnodes += 1;
                        *order.wrapping_offset(*ind as isize) = -*newnodes;
                    }
                } else if endnodes == 1 {
                    if *order.wrapping_offset(*ind as isize) > 0 {
                        if *DISCONT_HITS.wrapping_offset(*ind as isize) < 4 {
                            *newnodes += 1;
                            *order.wrapping_offset(*ind as isize) = -*newnodes;
                        } else {
                            *order.wrapping_offset(*ind as isize) = 0;
                        }
                    } else if *order.wrapping_offset(*ind as isize) == 0 {
                        *newnodes += 1;
                        *order.wrapping_offset(*ind as isize) = -*newnodes;
                    }
                }
                *j += 1;
            }
            *i += 1;
        }
        if endnodes == 0 || endnodes == 1 {
            *i = 1;
            while *i <= (*data).noknots {
                if *order.wrapping_offset(*i as isize) == 0 {
                    *order.wrapping_offset(*i as isize) = *i;
                }
                *i += 1;
            }
        }
        *bc += 1;
    }
}
/// Duplicate boundary knots for discontinuous fields.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn SetDiscontinuousBoundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    mut boundtype: c_int,
    endnodes: c_int,
    info: c_int,
) -> c_int {
    // NOTE: 0-init for the &mut handoff (set in discont_* before reads).
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in discont_* before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut bc: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: 0-init for the &mut handoff (set in discont_* before reads.)
    let mut ind: c_int = 0;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: 0-init for the &mut handoff (set in discont_* before reads.)
    let mut side: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: 0-init for the &mut handoff (set in discont_* before reads.)
    let mut parent: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newnodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut doublesides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxtype: c_int;
    // NOTE: `=0` decl init kept (newbc read before any overwrite).
    let newbc: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sidenodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let disconttype: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mat1: c_int = 0;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mat2: c_int = 0;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut mat1old: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut mat2old: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let material: c_int;
    let _ = (mat1, mat2);
    if boundtype < 0 {
        newbc = TRUE;
        boundtype = -boundtype;
    } else {
        newbc = FALSE;
    }
    mat1old = 0;
    mat2old = 0;
    doublesides = 0;
    // NOTE: counts elements with both parents set.
    discont_scan(
        data, bound, boundtype, &mut doublesides, &mut mat1old, &mut mat2old,
        &mut bc, &mut i,
    );
    if doublesides == 0 {
        return 1;
    }
    if mat1old > 0 && mat2old > 0 {
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if mat1old < mat2old {
            material = mat1old;
        } else {
            material = mat2old;
        }
    } else if mat1old > 0 {
        material = mat1old;
    } else if mat2old > 0 {
        material = mat2old;
    } else {
        printf(c"SetDiscontinuousBoundary: impossible to make the boundary of several materials\n".as_ptr());
        return 2;
    }
    if info != 0 {
        printf(
            c"Creating discontinuous boundary between materials %d and %d\n".as_ptr(),
            mat1old,
            mat2old,
        );
        printf(
            c"New set of nodes will be created for material %d\n".as_ptr(),
            material,
        );
    }
    let noelements: c_int = (*data).noelements;
    let order: *mut c_int = ivector(1, (*data).noknots);
    i = 1;
    while i <= (*data).noknots {
        *order.wrapping_offset(i as isize) = i;
        i += 1;
    }
    // NOTE: endnodes share hits with the bulk count.
    discont_hits(data, noelements, &mut nonodes, &mut i, &mut j, endnodes);
    // NOTE: optional secondary boundary on the far side.
    if newbc != 0 {
        maxtype = 0;
        bc = 0;
        while bc < MAXBOUNDARIES as c_int {
            i = 1;
            while i <= (*bound.wrapping_offset(bc as isize)).nosides {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxtype
                    < *(*bound.wrapping_offset(bc as isize))
                        .types
                        .wrapping_offset(i as isize)
                {
                    maxtype = *(*bound.wrapping_offset(bc as isize))
                        .types
                        .wrapping_offset(i as isize);
                }
                if (*bound.wrapping_offset(bc as isize)).ediscont != 0
                    && maxtype
                        < *(*bound.wrapping_offset(bc as isize))
                            .discont
                            .wrapping_offset(i as isize)
                {
                        maxtype = *(*bound.wrapping_offset(bc as isize))
                            .discont
                            .wrapping_offset(i as isize);
                }
                i += 1;
            }
            bc += 1;
        }
        disconttype = maxtype + 1;
        if info != 0 {
            printf(
                c"Type of the other side of discontinuous boundary set to %d.\n".as_ptr(),
                disconttype,
            );
        }
    } else {
        disconttype = boundtype;
    }
    // NOTE: flag the duplicated nodes.
    newnodes = 0;
    discont_order(
        data, bound, boundtype, disconttype, endnodes, &mut newnodes, order,
        sideind.as_mut_ptr(), &mut sideelemtype, &mut sidenodes, &mut parent,
        &mut side, &mut ind, &mut bc, &mut i, &mut j,
    );
    if newnodes == 0 {
        return 3;
    }
    let newsuccess: c_int = CreateNewNodes(data, order, material, newnodes);
    newsuccess
}
/// Pair two boundaries as periodic (simple parent-offset scheme).
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn FindPeriodicBoundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    boundary1: c_int,
    boundary2: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut parent: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    let mut minp: [c_int; 2] = [0; 2];
    let mut maxp: [c_int; 2] = [0; 2];
    let mut bounds: [c_int; 2] = [0; 2];
    let mut dp: [c_int; 2] = [0; 2];
    let mut sumsides: [c_int; 2] = [0; 2];
    if (*bound).created == FALSE {
        printf(c"SetDiscontinuousBoundary: The boundary does not exist!\n".as_ptr());
        return 1;
    }
    if (*bound).nosides == 0 {
        return 0;
    }
    bounds[0] = boundary1;
    bounds[1] = boundary2;
    minp[0] = (*data).noknots + 1;
    minp[1] = (*data).noknots + 1;
    maxp[0] = 0;
    maxp[1] = 0;
    sumsides[0] = 0;
    sumsides[1] = 0;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            k = 0;
            while k <= 1 {
                if *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)
                    == bounds[k as usize]
                {
                    sumsides[k as usize] += 1;
                    // NOTE: C MAX macro (egdef.h:97); arms pure.
                    if maxp[k as usize]
                        < *(*bound.wrapping_offset(j as isize))
                            .parent
                            .wrapping_offset(i as isize)
                    {
                        maxp[k as usize] = *(*bound.wrapping_offset(j as isize))
                            .parent
                            .wrapping_offset(i as isize);
                    }
                    // NOTE: C MIN macro (egdef.h:96); arms pure.
                    if *(*bound.wrapping_offset(j as isize))
                        .parent
                        .wrapping_offset(i as isize)
                        < minp[k as usize]
                    {
                        minp[k as usize] = *(*bound.wrapping_offset(j as isize))
                            .parent
                            .wrapping_offset(i as isize);
                    }
                }
                k += 1;
            }
            i += 1;
        }
        j += 1;
    }
    k = 0;
    while k <= 1 {
        dp[k as usize] = maxp[k as usize] - minp[k as usize];
        if info != 0 {
            printf(
                c"Parents of boundary %d are on the interval [%d, %d]\n".as_ptr(),
                bounds[k as usize],
                minp[k as usize],
                maxp[k as usize],
            );
        }
        k += 1;
    }
    if dp[0] != dp[1] || sumsides[0] != sumsides[1] {
        printf(c"FindPeriodicBoundary: The simple scheme cannot handle these boundaries!\n".as_ptr());
        printf(
            c"dp=[%d %d]  sumsides=[%d %d]\n".as_ptr(),
            dp[0],
            dp[1],
            sumsides[0],
            sumsides[1],
        );
        return 1;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == FALSE {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            k = 0;
            while k <= 1 {
                if *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)
                    == bounds[k as usize]
                {
                    parent = *(*bound.wrapping_offset(j as isize))
                        .parent
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .parent2
                        .wrapping_offset(i as isize) =
                        *(*bound.wrapping_offset(j as isize))
                            .parent
                            .wrapping_offset(i as isize) - minp[k as usize]
                            + minp[((k + 1) % 2) as usize];
                    if (*bound.wrapping_offset(j as isize)).ediscont == 0 {
                        (*bound.wrapping_offset(j as isize)).discont = ivector(
                            1,
                            (*bound.wrapping_offset(j as isize)).nosides,
                        );
                        l = 1;
                        while l <= (*bound.wrapping_offset(j as isize)).nosides {
                            *(*bound.wrapping_offset(j as isize))
                                .discont
                                .wrapping_offset(l as isize) = 0;
                            l += 1;
                        }
                        (*bound.wrapping_offset(j as isize)).ediscont = TRUE;
                    }
                    *(*bound.wrapping_offset(j as isize))
                        .discont
                        .wrapping_offset(i as isize) = 2 + k;
                    elemtype =
                        *(*data).elementtypes.wrapping_offset(parent as isize);
                    if elemtype % 100 == 4 {
                        *(*bound.wrapping_offset(j as isize))
                            .side2
                            .wrapping_offset(i as isize) = (*(*bound.wrapping_offset(j as isize))
                            .side
                            .wrapping_offset(i as isize)
                            + 2)
                            % 4;
                    } else if elemtype % 100 == 8 {
                        if *(*bound.wrapping_offset(j as isize))
                            .side
                            .wrapping_offset(i as isize)
                            < 4
                        {
                            *(*bound.wrapping_offset(j as isize))
                                .side2
                                .wrapping_offset(i as isize) = (*(*bound.wrapping_offset(j as isize))
                                .side
                                .wrapping_offset(i as isize)
                                + 2)
                                % 4;
                        }
                        if *(*bound.wrapping_offset(j as isize))
                            .side
                            .wrapping_offset(i as isize)
                            >= 4
                        {
                            *(*bound.wrapping_offset(j as isize))
                                .side2
                                .wrapping_offset(i as isize) = 4
                                + (5 - *(*bound.wrapping_offset(j as isize))
                                    .side
                                    .wrapping_offset(i as isize));
                        }
                    }
                }
                k += 1;
            }
            i += 1;
        }
        j += 1;
    }
    if info != 0 {
        printf(c"Periodic boundaries were set with a simple scheme\n".as_ptr());
    }
    2
}

/// Flag nodes touching a boundary condition type.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn SetConnectedNodes(
    data: *mut FemType,
    bound: *mut BoundaryType,
    bctype: c_int,
    connecttype: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bc: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidenodes: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut nodesset: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut conflicts: c_int;
    conflicts = 0;
    nodesset = 0;
    bc = 0;
    while bc < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(bc as isize)).created == FALSE {
            bc += 1;
            continue;
        }
        if (*bound.wrapping_offset(bc as isize)).nosides == 0 {
            bc += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(bc as isize)).nosides {
            if bctype > 0 {
                if *(*bound.wrapping_offset(bc as isize))
                    .types
                    .wrapping_offset(i as isize)
                    != bctype
                {
                    i += 1;
                    continue;
                }
            } else if bctype == -1 {
                if *(*bound.wrapping_offset(bc as isize))
                    .parent
                    .wrapping_offset(i as isize)
                    == 0
                {
                    i += 1;
                    continue;
                }
            } else if bctype == -2 {
                if *(*bound.wrapping_offset(bc as isize))
                    .parent
                    .wrapping_offset(i as isize)
                    == 0
                    || *(*bound.wrapping_offset(bc as isize))
                        .parent2
                        .wrapping_offset(i as isize)
                        == 0
                {
                    i += 1;
                    continue;
                }
            } else if bctype == -3
                && (*(*bound.wrapping_offset(bc as isize))
                    .parent
                    .wrapping_offset(i as isize)
                    == 0
                    || *(*bound.wrapping_offset(bc as isize))
                        .parent2
                        .wrapping_offset(i as isize)
                        != 0)
            {
                i += 1;
                continue;
            }
            // NOTE: lazily create the node table.
            if (*data).nodeconnectexist == 0 {
                (*data).nodeconnect = ivector(1, (*data).noknots);
                k = 1;
                while k <= (*data).noknots {
                    *(*data).nodeconnect.wrapping_offset(k as isize) = 0;
                    k += 1;
                }
                (*data).nodeconnectexist = TRUE;
            }
            GetElementSide(
                *(*bound.wrapping_offset(bc as isize))
                    .parent
                    .wrapping_offset(i as isize),
                *(*bound.wrapping_offset(bc as isize))
                    .side
                    .wrapping_offset(i as isize),
                *(*bound.wrapping_offset(bc as isize))
                    .normal
                    .wrapping_offset(i as isize),
                data,
                sideind.as_mut_ptr(),
                &mut sideelemtype,
            );
            sidenodes = sideelemtype % 100;
            j = 0;
            while j < sidenodes {
                k = sideind[j as usize];
                if *(*data).nodeconnect.wrapping_offset(k as isize) != connecttype {
                    if *(*data).nodeconnect.wrapping_offset(k as isize) != 0 {
                        conflicts += 1;
                    }
                    *(*data).nodeconnect.wrapping_offset(k as isize) = connecttype;
                    nodesset += 1;
                }
                j += 1;
            }
            i += 1;
        }
        bc += 1;
    }
    if info != 0 {
        printf(
            c"Setting connectivity group %d for %d nodes on boundary %d\n".as_ptr(),
            connecttype,
            nodesset,
            bctype,
        );
    }
    if conflicts != 0 {
        printf(
            c"The were %d conflicts in the connectivity set %d\n".as_ptr(),
            conflicts,
            connecttype,
        );
    }
    0
}

/// Flag elements touching connected nodes (usable as a permutation).
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn SetConnectedElements(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut hit: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nohits: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut con: c_int;
    // NOTE: null-init (set from data before reads).
    if (*data).nodeconnectexist == 0 {
        printf(c"Cannot create connected elements without connected nodes!\n".as_ptr());
        return 1;
    }
    let nodeconnect: *mut c_int = (*data).nodeconnect;
    // NOTE: allocate the element table on first use.
    if (*data).elemconnectexist == 0 {
        printf(c"Created table for connected elements\n".as_ptr());
        (*data).elemconnect = ivector(1, (*data).noelements);
        k = 1;
        while k <= (*data).noelements {
            *(*data).elemconnect.wrapping_offset(k as isize) = 0;
            k += 1;
        }
        (*data).elemconnectexist = TRUE;
        // NOTE: elements with a connected node.
        nohits = 0;
        i = 1;
        while i <= (*data).noelements {
            nonodes = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
            hit = FALSE;
            j = 0;
            while j < nonodes {
                k = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
                con = *nodeconnect.wrapping_offset(k as isize);
                if con != 0 {
                    // NOTE: C MAX macro (egdef.h:97); arms pure.
                    if *(*data).elemconnect.wrapping_offset(i as isize) < con {
                        *(*data).elemconnect.wrapping_offset(i as isize) = con;
                    }
                    hit = TRUE;
                }
                j += 1;
            }
            if hit != 0 {
                nohits += 1;
            }
            i += 1;
        }
        if info != 0 {
            printf(
                c"Number of connected elements is %d (out of %d)\n".as_ptr(),
                nohits,
                (*data).noelements,
            );
        }
        (*data).elemconnectexist = nohits;
    }
    // NOTE: negate connections; unconnected become a permutation.
    if (*data).elemconnectexist != 0 {
        if info != 0 {
            printf(c"Use connected table as a permutation for creating dual graph!\n".as_ptr());
        }
        j = 0;
        i = 1;
        while i <= (*data).noelements {
            if *(*data).elemconnect.wrapping_offset(i as isize) != 0 {
                *(*data).elemconnect.wrapping_offset(i as isize) =
                    -abs(*(*data).elemconnect.wrapping_offset(i as isize));
            } else {
                j += 1;
                *(*data).elemconnect.wrapping_offset(i as isize) = j;
            }
            i += 1;
        }
    }
    0
}

/// Knots at material corners of the structured grid.
///
/// # Safety
/// `grid`, `cell`, `data` valid.
#[no_mangle]
pub unsafe extern "C" fn FindCorners(
    grid: *mut GridType,
    cell: *mut CellType,
    data: *mut FemType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cellno: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elem: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nocorners: c_int;
    nocorners = 0;
    allocated = FALSE;
    // NOTE: C `omstart:` label + `goto omstart`.
    loop {
        if nocorners > 0 {
            (*data).corners = ivector(1, 2 * nocorners);
            (*data).nocorners = nocorners;
            allocated = TRUE;
        }
        k = 0;
        i = 1;
        while i <= (*grid).xcells + 1 {
            j = 1;
            while j <= (*grid).ycells + 1 {
                if (*grid).structure[j as usize][i as usize]
                    == (*grid).structure[j as usize][(i - 1) as usize]
                    && (*grid).structure[(j - 1) as usize][i as usize]
                        == (*grid).structure[(j - 1) as usize][(i - 1) as usize]
                {
                    j += 1;
                    continue;
                }
                if (*grid).structure[j as usize][i as usize]
                    == (*grid).structure[(j - 1) as usize][i as usize]
                    && (*grid).structure[j as usize][(i - 1) as usize]
                        == (*grid).structure[(j - 1) as usize][(i - 1) as usize]
                {
                    j += 1;
                    continue;
                }
                // NOTE: (i,j) is a corner; find the owning cell.
                cellno = (*grid).numbered[j as usize][i as usize];
                if cellno != 0 {
                    elem = GetElementIndex(cell.wrapping_offset(cellno as isize), 1, 1);
                    ind = BOTLEFT;
                } else {
                    cellno = (*grid).numbered[j as usize][(i - 1) as usize];
                    if cellno != 0 {
                        elem = GetElementIndex(
                            cell.wrapping_offset(cellno as isize),
                            (*cell.wrapping_offset(cellno as isize)).xelem,
                            1,
                        );
                        ind = BOTRIGHT;
                    } else {
                        cellno = (*grid).numbered[(j - 1) as usize][i as usize];
                        if cellno != 0 {
                            elem = GetElementIndex(
                                cell.wrapping_offset(cellno as isize),
                                1,
                                (*cell.wrapping_offset(cellno as isize)).yelem,
                            );
                            ind = TOPLEFT;
                        } else {
                            cellno = (*grid).numbered[(j - 1) as usize][(i - 1) as usize];
                            if cellno != 0 {
                                elem = GetElementIndex(
                                    cell.wrapping_offset(cellno as isize),
                                    (*cell.wrapping_offset(cellno as isize)).xelem,
                                    (*cell.wrapping_offset(cellno as isize)).yelem,
                                );
                                ind = TOPRIGHT;
                            } else {
                                j += 1;
                                continue;
                            }
                        }
                    }
                }
                // NOTE: ind is the corner knot index.
                k += 1;
                if allocated == FALSE {
                    j += 1;
                    continue;
                }
                *(*data).corners.wrapping_offset((2 * k - 1) as isize) = elem;
                *(*data).corners.wrapping_offset((2 * k) as isize) = ind;
                j += 1;
            }
            i += 1;
        }
        nocorners = k;
        if nocorners == 0 {
            return 0;
        }
        if allocated == FALSE {
            continue;
        }
        break;
    }
    if info != 0 {
        printf(c"Found %d material corners.\n".as_ptr(), nocorners);
    }
    0
}
/// SAFETY: per-element angle scan + split decision (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn triangles_decide(
    data: *mut FemType,
    needed: *mut c_int,
    critangle: Real,
    elementtype: &mut c_int,
    newtype: &mut c_int,
    newnodes: &mut c_int,
    triangles: &mut c_int,
    maxangle: &mut Real,
    maxanglej: &mut c_int,
    evenodd: &mut c_int,
    newmaxnodes: &mut c_int,
    angles: *mut Real,
    i: c_int,
) -> bool {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds2: Real;
    *elementtype = *(*data).elementtypes.wrapping_offset(i as isize);
    // NOTE: largest-angle corner decides the split diagonal.
    *maxangle = 0.0;
    *maxanglej = 0;
    j = 0;
    while j < 4 {
        dx1 = *(*data).x.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize))
                .wrapping_offset(((j + 3) % 4) as isize) as isize,
        ) - *(*data).x.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                as isize,
        );
        dy1 = *(*data).y.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize))
                .wrapping_offset(((j + 3) % 4) as isize) as isize,
        ) - *(*data).y.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                as isize,
        );
        dx2 = *(*data).x.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize))
                .wrapping_offset(((j + 1) % 4) as isize) as isize,
        ) - *(*data).x.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                as isize,
        );
        dy2 = *(*data).y.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize))
                .wrapping_offset(((j + 1) % 4) as isize) as isize,
        ) - *(*data).y.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                as isize,
        );
        ds1 = sqrt(dx1 * dx1 + dy1 * dy1);
        ds2 = sqrt(dx2 * dx2 + dy2 * dy2);
        *angles.wrapping_offset(j as isize) =
            (180.0 / FM_PI) * acos((dx1 * dx2 + dy1 * dy2) / (ds1 * ds2));
        // NOTE: slightly favor splits at singly-used corners.
        if *needed.wrapping_offset(
            *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                as isize,
        ) == 1
        {
            *angles.wrapping_offset(j as isize) *= 1.001;
        }
        // NOTE: C `abs(bool)` is the identity on 0/1; kept verbatim.
        if abs((*angles.wrapping_offset(j as isize) > *maxangle) as c_int) != 0 {
            *maxangle = abs(*angles.wrapping_offset(j as isize) as c_int) as Real;
            *maxanglej = j;
        }
        j += 1;
    }
    *evenodd = *maxanglej % 2;
    // NOTE: below the critical angle the element is kept as is.
    if *maxangle < critangle {
        *triangles = 1;
        *newtype = *elementtype;
        *newnodes = *elementtype % 100;
    } else {
        // NOTE: C switch over elementtype.
        if *elementtype == 404 {
            *newtype = 303;
            *newnodes = 3;
            *triangles = 2;
        } else if *elementtype == 405 {
            *newtype = 303;
            *newnodes = 3;
            *triangles = 4;
        } else if *elementtype == 409 {
            *newtype = 306;
            *newnodes = 6;
            *triangles = 2;
        } else if *elementtype == 416 {
            *newtype = 310;
            *newnodes = 10;
            *triangles = 2;
        } else {
            printf(
                c"ElementsToTriangles: not implemented for elementtype %d\n".as_ptr(),
                *elementtype,
            );
            return false;
        }
    }
    // NOTE: C MAX macro (egdef.h:97); arms pure.
    if *newmaxnodes < *newnodes {
        *newmaxnodes = *newnodes;
    }
    true
}
/// SAFETY: per-element triangle fill (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn triangles_fill(
    data: *mut FemType,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    divisions: *mut c_int,
    division1: *mut c_int,
    elementtype: c_int,
    newtype: c_int,
    newnodes: c_int,
    triangles: c_int,
    evenodd: c_int,
    newelem: &mut c_int,
    i: c_int,
    j: &mut c_int,
) {
    *j = *division1.wrapping_offset(i as isize) + 1;
    while *j <= *division1.wrapping_offset(i as isize) + *divisions.wrapping_offset(i as isize)
    {
        *newelementtypes.wrapping_offset(*j as isize) = newtype;
        *newmaterial.wrapping_offset(*j as isize) =
            *(*data).material.wrapping_offset(i as isize);
        *j += 1;
    }
    *newelem = *division1.wrapping_offset(i as isize) + 1;
    if triangles == 1 {
        *j = 0;
        while *j < newnodes {
            *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(*j as isize) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(*j as isize);
            *j += 1;
        }
    } else {
        if elementtype == 404 || elementtype == 409 || elementtype == 416 {
            if evenodd != 0 {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(0) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(0);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(1) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(1);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(2) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(0) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(2);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(1) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(2) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(1);
            } else {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(0) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(1);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(1) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(2);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(2) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(0);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(0) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(1) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(0);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(2) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(2);
            }
        }
        if elementtype == 409 {
            if evenodd != 0 {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(7);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(6);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(5);
            } else {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(5);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(7);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(6);
            }
        }
        if elementtype == 416 {
            if evenodd != 0 {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(5);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(13);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(6) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(15);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(7) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(10);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(8) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(11);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(9) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(12);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(9);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(15);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(6) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(13);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(7) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(6);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(8) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(7);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(9) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(14);
            } else {
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(6);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(7);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(14);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(6) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(12);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(7) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(8) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(5);
                *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(9) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(13);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(3) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(10);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(4) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(11);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(5) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(12);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(6) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(14);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(7) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(8);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(8) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(9);
                *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(9) =
                    *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(15);
            }
        } else if elementtype == 405 {
            *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(0) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(0);
            *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(1) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(1);
            *(*newtopo.wrapping_offset(*newelem as isize)).wrapping_offset(2) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
            *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(0) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(1);
            *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(1) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(2);
            *(*newtopo.wrapping_offset((*newelem + 1) as isize)).wrapping_offset(2) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
            *(*newtopo.wrapping_offset((*newelem + 2) as isize)).wrapping_offset(0) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(2);
            *(*newtopo.wrapping_offset((*newelem + 2) as isize)).wrapping_offset(1) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3);
            *(*newtopo.wrapping_offset((*newelem + 2) as isize)).wrapping_offset(2) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
            *(*newtopo.wrapping_offset((*newelem + 3) as isize)).wrapping_offset(0) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(3);
            *(*newtopo.wrapping_offset((*newelem + 3) as isize)).wrapping_offset(1) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(0);
            *(*newtopo.wrapping_offset((*newelem + 3) as isize)).wrapping_offset(2) =
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(4);
        }
    }
}
/// SAFETY: triangle BC parent remap (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn triangles_remap(
    data: *mut FemType,
    data2: *mut FemType,
    bound: *mut BoundaryType,
    newtopo: *mut *mut c_int,
    divisions: *mut c_int,
    division1: *mut c_int,
    sideind: *mut c_int,
    sideind2: *mut c_int,
    sideelemtype: &mut c_int,
    isum: &mut c_int,
    side: &mut c_int,
    elem: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    i1: &mut c_int,
) {
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            *l = 1;
            while *l <= 2 {
                if *l == 1 {
                    *k = *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize);
                } else {
                    *k = *(*bound.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize);
                }
                if *k == 0 {
                    *l += 1;
                    continue;
                }
                if *l == 1 {
                    GetElementSide(
                        *(*bound.wrapping_offset(*j as isize))
                            .parent
                            .wrapping_offset(*i as isize),
                        *(*bound.wrapping_offset(*j as isize))
                            .side
                            .wrapping_offset(*i as isize),
                        *(*bound.wrapping_offset(*j as isize))
                            .normal
                            .wrapping_offset(*i as isize),
                        data,
                        sideind,
                        sideelemtype,
                    );
                } else {
                    GetElementSide(
                        *(*bound.wrapping_offset(*j as isize))
                            .parent2
                            .wrapping_offset(*i as isize),
                        *(*bound.wrapping_offset(*j as isize))
                            .side2
                            .wrapping_offset(*i as isize),
                        *(*bound.wrapping_offset(*j as isize))
                            .normal
                            .wrapping_offset(*i as isize),
                        data,
                        sideind,
                        sideelemtype,
                    );
                }
                if *sideelemtype / 100 != 2 {
                    printf(c"ElementsToTriangles: implemented only for BCs 202 and 203\n".as_ptr());
                    *l += 1;
                    continue;
                }
                *isum = 0;
                *side = 0;
                if *divisions.wrapping_offset(*k as isize) == 1 {
                    *elem = *division1.wrapping_offset(*k as isize) + 1;
                    *side = *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize);
                    *isum = 2;
                } else {
                    // NOTE: brute-force search for the new parent.
                    *elem = *division1.wrapping_offset(*k as isize) + 1;
                    while *elem
                        <= *division1.wrapping_offset(*k as isize)
                            + *divisions.wrapping_offset(*k as isize)
                    {
                        *isum = 0;
                        *i1 = 0;
                        while *i1 < 3 {
                            if *(*newtopo.wrapping_offset(*elem as isize))
                                .wrapping_offset(*i1 as isize)
                                == *sideind.wrapping_offset(0)
                            {
                                *isum += 1;
                            }
                            if *(*newtopo.wrapping_offset(*elem as isize))
                                .wrapping_offset(*i1 as isize)
                                == *sideind.wrapping_offset(1)
                            {
                                *isum += 1;
                            }
                            *i1 += 1;
                        }
                        if *isum != 2 {
                            *elem += 1;
                            continue;
                        }
                        *side = 0;
                        while *side < 3 {
                            GetElementSide(
                                *elem,
                                *side,
                                *(*bound.wrapping_offset(*j as isize))
                                    .normal
                                    .wrapping_offset(*i as isize),
                                data2,
                                sideind2,
                                sideelemtype,
                            );
                            *isum = 0;
                            *i1 = 0;
                            while *i1 < 2 {
                                if *sideind2.wrapping_offset(*i1 as isize)
                                    == *sideind.wrapping_offset(0)
                                {
                                    *isum += 1;
                                }
                                if *sideind2.wrapping_offset(*i1 as isize)
                                    == *sideind.wrapping_offset(1)
                                {
                                    *isum += 1;
                                }
                                *i1 += 1;
                            }
                            // NOTE: C `goto nextparent` becomes break-out.
                            if *isum == 2 {
                                break;
                            }
                            *side += 1;
                        }
                        if *isum == 2 {
                            break;
                        }
                        *elem += 1;
                    }
                }
                // NOTE: C `nextparent:` label.
                if *isum == 2 {
                    if *l == 1 {
                        *(*bound.wrapping_offset(*j as isize))
                            .parent
                            .wrapping_offset(*i as isize) = *elem;
                        *(*bound.wrapping_offset(*j as isize))
                            .side
                            .wrapping_offset(*i as isize) = *side;
                    }
                    if *l == 2 {
                        *(*bound.wrapping_offset(*j as isize))
                            .parent2
                            .wrapping_offset(*i as isize) = *elem;
                        *(*bound.wrapping_offset(*j as isize))
                            .side2
                            .wrapping_offset(*i as isize) = *side;
                    }
                } else {
                    printf(
                        c"Failed to find parent for side %d of %d (parent %d)\n".as_ptr(),
                        *i,
                        *j,
                        *k,
                    );
                }
                *l += 1;
            }
            *i += 1;
        }
        *j += 1;
    }
}

/// Split rectangular elements into triangles along the flattest diagonal.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn ElementsToTriangles(
    data: *mut FemType,
    bound: *mut BoundaryType,
    critangle: Real,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in triangles_fill before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i1: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut isum: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elementtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut triangles: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut newmaxnodes: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    let mut newtopo: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut newmaterial: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut newelementtypes: *mut c_int = core::ptr::null_mut();
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newnodes: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxanglej: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut evenodd: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut newelem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxangle: Real = 0.0;
    let mut angles: [Real; 4] = [0.0; 4];
    // NOTE: shallow working copy over the new arrays.
    let mut data2: FemType = core::mem::zeroed();
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    allocated = FALSE;
    let needed: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *needed.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    i = 1;
    while i <= noelements {
        let nonodes: c_int = *(*data).elementtypes.wrapping_offset(i as isize) / 100;
        j = 0;
        while j < nonodes {
            (*needed.wrapping_offset(
                *(*(*data).topology.wrapping_offset(i as isize)).wrapping_offset(j as isize)
                    as isize,
            )) += 1;
            j += 1;
        }
        i += 1;
    }
    let divisions: *mut c_int = ivector(1, noelements);
    let division1: *mut c_int = ivector(1, noelements);
    i = 1;
    while i <= noelements {
        *divisions.wrapping_offset(i as isize) = 0;
        *division1.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    // NOTE: split along the shorter diameter first.
    newelements = 0;
    // NOTE: C `newmaxnodes = 0` dropped (decl init holds).
    // NOTE: C `omstart:` label + `goto omstart`; count then fill.
    loop {
        i = 1;
        while i <= noelements {
            if !triangles_decide(
                data, needed, critangle, &mut elementtype, &mut newtype, &mut newnodes,
                &mut triangles, &mut maxangle, &mut maxanglej, &mut evenodd,
                &mut newmaxnodes, angles.as_mut_ptr(), i,
            ) {
                return 1;
            }
            if allocated == 0 {
                *divisions.wrapping_offset(i as isize) = triangles;
                *division1.wrapping_offset(i as isize) = newelements;
                newelements += triangles;
                i += 1;
                continue;
            }
            triangles_fill(
                data, newtopo, newmaterial, newelementtypes, divisions, division1,
                elementtype, newtype, newnodes, triangles, evenodd, &mut newelem, i,
                &mut j,
            );
            i += 1;
        }
        if allocated != 0 {
            break;
        }
        newtopo = imatrix(1, newelements, 0, newmaxnodes - 1);
        newmaterial = ivector(1, newelements);
        newelementtypes = ivector(1, newelements);
        allocated = TRUE;
        // NOTE: bitwise copy; fields rebound to the new arrays.
        data2 = core::ptr::read(data);
        data2.topology = newtopo;
        data2.material = newmaterial;
        data2.elementtypes = newelementtypes;
    }
    // NOTE: remap BC parents onto the new elements (brute force).
    triangles_remap(
        data, &mut data2, bound, newtopo, divisions, division1,
        sideind.as_mut_ptr(), sideind2.as_mut_ptr(), &mut sideelemtype, &mut isum,
        &mut side, &mut elem, &mut i, &mut j, &mut k, &mut l, &mut i1,
    );
    free_imatrix(
        (*data).topology,
        1,
        noelements,
        0,
        (*data).maxnodes - 1,
    );
    free_ivector((*data).material, 1, noelements);
    free_ivector((*data).elementtypes, 1, noelements);
    free_ivector(needed, 1, noknots);
    free_ivector(divisions, 1, noelements);
    free_ivector(division1, 1, noelements);
    (*data).topology = newtopo;
    (*data).elementtypes = newelementtypes;
    (*data).material = newmaterial;
    (*data).noelements = newelements;
    (*data).maxnodes = newmaxnodes;
    if info != 0 {
        printf(
            c"There are %d elements after triangularization (was %d)\n".as_ptr(),
            newelements,
            noelements,
        );
    }
    0
}
/// Polar-to-cartesian knot transform around radius `rad`.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn PolarCoordinates(data: *mut FemType, rad: Real, info: c_int) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut fii: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zet: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dr: Real;
    i = 1;
    while i <= (*data).noknots {
        zet = *(*data).x.wrapping_offset(i as isize);
        fii = FM_PI / 180.0 * *(*data).y.wrapping_offset(i as isize);
        dr = *(*data).z.wrapping_offset(i as isize);
        *(*data).z.wrapping_offset(i as isize) = zet;
        *(*data).x.wrapping_offset(i as isize) = (rad + dr) * cos(fii);
        *(*data).y.wrapping_offset(i as isize) = (rad + dr) * sin(fii);
        i += 1;
    }
    if info != 0 {
        printf(c"Making coordinate transformation from polar to cartesian\n".as_ptr());
    }
    0
}

/// Cylindrical-to-cartesian knot transform.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CylinderCoordinates(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: C `z` decl dropped (never touched in C either).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut rad: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut fii: Real;
    if (*data).dim == 3 {
        i = 1;
        while i <= (*data).noknots {
            _x = *(*data).x.wrapping_offset(i as isize);
            y = *(*data).y.wrapping_offset(i as isize);
            fii = FM_PI / 180.0 * *(*data).z.wrapping_offset(i as isize);
            rad = *(*data).x.wrapping_offset(i as isize);
            *(*data).x.wrapping_offset(i as isize) = rad * cos(fii);
            *(*data).y.wrapping_offset(i as isize) = rad * sin(fii);
            *(*data).z.wrapping_offset(i as isize) = y;
            i += 1;
        }
    } else {
        i = 1;
        while i <= (*data).noknots {
            rad = *(*data).x.wrapping_offset(i as isize);
            fii = FM_PI / 180.0 * *(*data).y.wrapping_offset(i as isize);
            *(*data).x.wrapping_offset(i as isize) = rad * cos(fii);
            *(*data).y.wrapping_offset(i as isize) = rad * sin(fii);
            i += 1;
        }
    }
    if info != 0 {
        printf(c"Making coordinate transformation from cylindrical to cartesian\n".as_ptr());
    }
    0
}
/// SAFETY: unite name-table mark pass (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unite_mark(
    data1: *mut FemType,
    data2: *mut FemType,
    bound1: *mut BoundaryType,
    bound2: *mut BoundaryType,
    bodynameis: *mut c_int,
    boundarynameis: *mut c_int,
    bodyused: *mut c_int,
    boundaryused: *mut c_int,
    mat: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    let _ = k;
    *i = 1;
    while *i <= (*data1).noelements {
        *mat = *(*data1).material.wrapping_offset(*i as isize);
        if *mat < MAXBODIES as c_int && *bodynameis.wrapping_offset(*mat as isize) == 0 {
            *bodynameis.wrapping_offset(*mat as isize) = -1;
            *bodyused.wrapping_offset(*mat as isize) = TRUE;
        }
        *i += 1;
    }
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound1.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        // NOTE: C reads `bound1[k].nosides` with k == 0 here.
        *i = 1;
        while *i <= (*bound1.wrapping_offset(0)).nosides {
            *mat = *(*bound1.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            if *mat < MAXBCS as c_int
                && *boundarynameis.wrapping_offset(*mat as isize) == 0
            {
                *boundarynameis.wrapping_offset(*mat as isize) = -1;
                *boundaryused.wrapping_offset(*mat as isize) = TRUE;
            }
            *i += 1;
        }
        *j += 1;
    }
    *i = 1;
    while *i <= (*data2).noelements {
        *mat = *(*data2).material.wrapping_offset(*i as isize);
        if *mat < MAXBODIES as c_int && *bodynameis.wrapping_offset(*mat as isize) == 0 {
            *bodynameis.wrapping_offset(*mat as isize) = *mat;
            *bodyused.wrapping_offset(*mat as isize) = TRUE;
            if (*data1).bodyname[*mat as usize].is_null() {
                (*data1).bodyname[*mat as usize] = cvector(0, MAXNAMESIZE as c_int);
            }
            strcpy(
                (*data1).bodyname[*mat as usize],
                (*data2).bodyname[*mat as usize],
            );
        }
        *i += 1;
    }
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound2.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound2.wrapping_offset(*j as isize)).nosides {
            *mat = *(*bound2.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            if *mat < MAXBCS as c_int
                && *boundarynameis.wrapping_offset(*mat as isize) == 0
            {
                *boundarynameis.wrapping_offset(*mat as isize) = *mat;
                *boundaryused.wrapping_offset(*mat as isize) = TRUE;
                if (*data1).boundaryname[*mat as usize].is_null() {
                    (*data1).boundaryname[*mat as usize] =
                        cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy(
                    (*data1).boundaryname[*mat as usize],
                    (*data2).boundaryname[*mat as usize],
                );
            }
            *i += 1;
        }
        *j += 1;
    }
}

/// SAFETY: unite conflict renumber pass (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unite_conflicts(
    data1: *mut FemType,
    data2: *mut FemType,
    bound2: *mut BoundaryType,
    bodynameis: *mut c_int,
    boundarynameis: *mut c_int,
    bodyused: *mut c_int,
    boundaryused: *mut c_int,
    mat: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    info: c_int,
) {
    *i = 1;
    while *i <= (*data2).noelements {
        *mat = *(*data2).material.wrapping_offset(*i as isize);
        if *mat < MAXBODIES as c_int && *bodynameis.wrapping_offset(*mat as isize) == -1 {
            *k = 1;
            while *k < MAXBODIES as c_int && *bodyused.wrapping_offset(*k as isize) != 0 {
                *k += 1;
            }
            if info != 0 {
                printf(
                    c"Renumbering body %d to %d\n".as_ptr(),
                    *mat,
                    *k,
                );
            }
            *bodynameis.wrapping_offset(*mat as isize) = *k;
            *bodyused.wrapping_offset(*k as isize) = TRUE;
            if !(*data2).bodyname[*mat as usize].is_null() {
                if (*data1).bodyname[*k as usize].is_null() {
                    (*data1).bodyname[*k as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy(
                    (*data1).bodyname[*k as usize],
                    (*data2).bodyname[*mat as usize],
                );
            }
        }
        *i += 1;
    }
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound2.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound2.wrapping_offset(*j as isize)).nosides {
            *mat = *(*bound2.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            if *mat < MAXBCS as c_int
                && *boundarynameis.wrapping_offset(*mat as isize) == -1
            {
                *k = 1;
                while *k < MAXBCS as c_int
                    && *boundaryused.wrapping_offset(*k as isize) != 0
                {
                    *k += 1;
                }
                if info != 0 {
                    printf(
                        c"Renumbering boundary %d to %d\n".as_ptr(),
                        *mat,
                        *k,
                    );
                }
                *boundarynameis.wrapping_offset(*mat as isize) = *k;
                *boundaryused.wrapping_offset(*k as isize) = TRUE;
                if !(*data2).boundaryname[*mat as usize].is_null() {
                    if (*data1).boundaryname[*k as usize].is_null() {
                        (*data1).boundaryname[*k as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    strcpy(
                        (*data1).boundaryname[*k as usize],
                        (*data2).boundaryname[*mat as usize],
                    );
                }
            }
            *i += 1;
        }
        *j += 1;
    }
}
/// SAFETY: unite nooverlap offset computation (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unite_offsets(
    data1: *mut FemType,
    data2: *mut FemType,
    bound1: *mut BoundaryType,
    bound2: *mut BoundaryType,
    bcoffset: &mut c_int,
    bodyoffset: &mut c_int,
    mat: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    info: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bcmax1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bcmin2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bodymax1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bodymin2: c_int;
    let _ = k;
    bcmax1 = 0;
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound1.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        // NOTE: C reads `bound1[k].nosides` with k == 0 here.
        *i = 1;
        while *i <= (*bound1.wrapping_offset(0)).nosides {
            *mat = *(*bound1.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if bcmax1 < *mat {
                bcmax1 = *mat;
            }
            *i += 1;
        }
        *j += 1;
    }
    bcmin2 = 1000;
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound2.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound2.wrapping_offset(*j as isize)).nosides {
            *mat = *(*bound2.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            // NOTE: C MIN macro (egdef.h:96); arms pure.
            if *mat < bcmin2 {
                bcmin2 = *mat;
            }
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: C MAX macro over (0, int) (egdef.h:97).
    if 0 > bcmax1 - bcmin2 + 1 {
        *bcoffset = 0;
    } else {
        *bcoffset = bcmax1 - bcmin2 + 1;
    }
    if info != 0 {
        printf(
            c"Max(bc1) is %d and Min(bc2) is %d, using BC offset %d for mesh 2!\n".as_ptr(),
            bcmax1,
            bcmin2,
            *bcoffset,
        );
    }
    bodymax1 = 0;
    *i = 1;
    while *i <= (*data1).noelements {
        *mat = *(*data1).material.wrapping_offset(*i as isize);
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if bodymax1 < *mat {
            bodymax1 = *mat;
        }
        *i += 1;
    }
    bodymin2 = 1000;
    *i = 1;
    while *i <= (*data2).noelements {
        *mat = *(*data2).material.wrapping_offset(*i as isize);
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if *mat < bodymin2 {
            bodymin2 = *mat;
        }
        *i += 1;
    }
    if 0 > bodymax1 - bodymin2 + 1 {
        *bodyoffset = 0;
    } else {
        *bodyoffset = bodymax1 - bodymin2 + 1;
    }
    if info != 0 {
        printf(
            c"Max(body1) is %d and Min(body2) is %d, using body offset %d for mesh 2!\n".as_ptr(),
            bodymax1,
            bodymin2,
            *bodyoffset,
        );
    }
}

/// SAFETY: unite boundary move + bulk merge (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn unite_merge(
    data1: *mut FemType,
    data2: *mut FemType,
    bound1: *mut BoundaryType,
    bound2: *mut BoundaryType,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    newz: *mut Real,
    bodynameis: *mut c_int,
    boundarynameis: *mut c_int,
    usenames: c_int,
    bcoffset: c_int,
    bodyoffset: c_int,
    noelements: c_int,
    noknots: c_int,
    maxnodes: c_int,
    mat: &mut c_int,
    nonodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound2.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *k = *j;
        while *k < MAXBOUNDARIES as c_int
            && (*bound1.wrapping_offset(*k as isize)).created == 0
        {
            *k += 1;
        }
        // NOTE: field-by-field move (discont/chain state not transferred).
        (*bound1.wrapping_offset(*k as isize)).created =
            (*bound2.wrapping_offset(*j as isize)).created;
        (*bound1.wrapping_offset(*k as isize)).nosides =
            (*bound2.wrapping_offset(*j as isize)).nosides;
        (*bound1.wrapping_offset(*k as isize)).coordsystem =
            (*bound2.wrapping_offset(*j as isize)).coordsystem;
        (*bound1.wrapping_offset(*k as isize)).side =
            (*bound2.wrapping_offset(*j as isize)).side;
        (*bound1.wrapping_offset(*k as isize)).side2 =
            (*bound2.wrapping_offset(*j as isize)).side2;
        (*bound1.wrapping_offset(*k as isize)).parent =
            (*bound2.wrapping_offset(*j as isize)).parent;
        (*bound1.wrapping_offset(*k as isize)).parent2 =
            (*bound2.wrapping_offset(*j as isize)).parent2;
        (*bound1.wrapping_offset(*k as isize)).material =
            (*bound2.wrapping_offset(*j as isize)).material;
        (*bound1.wrapping_offset(*k as isize)).echain =
            (*bound2.wrapping_offset(*j as isize)).echain;
        (*bound1.wrapping_offset(*k as isize)).types =
            (*bound2.wrapping_offset(*j as isize)).types;
        (*bound1.wrapping_offset(*k as isize)).normal =
            (*bound2.wrapping_offset(*j as isize)).normal;
        *i = 1;
        while *i <= (*bound1.wrapping_offset(*k as isize)).nosides {
            *( (*bound1.wrapping_offset(*k as isize))
                .parent
                .wrapping_offset(*i as isize) ) += (*data1).noelements;
            if *(*bound1.wrapping_offset(*k as isize))
                .parent2
                .wrapping_offset(*i as isize)
                != 0
            {
                *( (*bound1.wrapping_offset(*k as isize))
                    .parent2
                    .wrapping_offset(*i as isize) ) += (*data1).noelements;
            }
            *mat = *(*bound2.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            if usenames != 0 {
                if *mat < MAXBCS as c_int {
                    *(*bound1.wrapping_offset(*k as isize))
                        .types
                        .wrapping_offset(*i as isize) =
                        *boundarynameis.wrapping_offset(*mat as isize);
                }
            } else {
                *(*bound1.wrapping_offset(*k as isize))
                    .types
                    .wrapping_offset(*i as isize) = bcoffset + *mat;
            }
            *i += 1;
        }
        *j += 1;
    }
    (*data1).maxnodes = maxnodes;
    *i = 1;
    while *i <= (*data1).noknots {
        *newx.wrapping_offset(*i as isize) = *(*data1).x.wrapping_offset(*i as isize);
        *newy.wrapping_offset(*i as isize) = *(*data1).y.wrapping_offset(*i as isize);
        *newz.wrapping_offset(*i as isize) = *(*data1).z.wrapping_offset(*i as isize);
        *i += 1;
    }
    *i = 1;
    while *i <= (*data2).noknots {
        *newx.wrapping_offset((*i + (*data1).noknots) as isize) =
            *(*data2).x.wrapping_offset(*i as isize);
        *newy.wrapping_offset((*i + (*data1).noknots) as isize) =
            *(*data2).y.wrapping_offset(*i as isize);
        *newz.wrapping_offset((*i + (*data1).noknots) as isize) =
            *(*data2).z.wrapping_offset(*i as isize);
        *i += 1;
    }
    *i = 1;
    while *i <= (*data1).noelements {
        *mat = *(*data1).material.wrapping_offset(*i as isize);
        *newmaterial.wrapping_offset(*i as isize) = *mat;
        *newelementtypes.wrapping_offset(*i as isize) =
            *(*data1).elementtypes.wrapping_offset(*i as isize);
        *nonodes = *newelementtypes.wrapping_offset(*i as isize) % 100;
        *j = 0;
        while *j < *nonodes {
            *(*newtopo.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) =
                *(*(*data1).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
            *j += 1;
        }
        *i += 1;
    }
    *i = 1;
    while *i <= (*data2).noelements {
        *mat = *(*data2).material.wrapping_offset(*i as isize);
        *newelementtypes.wrapping_offset((*i + (*data1).noelements) as isize) =
            *(*data2).elementtypes.wrapping_offset(*i as isize);
        *nonodes =
            *newelementtypes.wrapping_offset((*i + (*data1).noelements) as isize) % 100;
        *j = 0;
        while *j < *nonodes {
            *(*newtopo.wrapping_offset((*i + (*data1).noelements) as isize))
                .wrapping_offset(*j as isize) = *(*(*data2).topology
                .wrapping_offset(*i as isize))
            .wrapping_offset(*j as isize)
                + (*data1).noknots;
            *j += 1;
        }
        if usenames != 0 {
            if *mat < MAXBODIES as c_int {
                *newmaterial.wrapping_offset((*i + (*data1).noelements) as isize) =
                    *bodynameis.wrapping_offset(*mat as isize);
            }
        } else {
            *newmaterial.wrapping_offset((*i + (*data1).noelements) as isize) =
                bodyoffset + *mat;
        }
        *i += 1;
    }
    let _ = (noelements, noknots);
}
/// Concatenate two meshes (with name-table reconciliation).
///
/// # Safety
/// `data1`, `data2`, `bound1`, `bound2` valid.
#[no_mangle]
pub unsafe extern "C" fn UniteMeshes(
    data1: *mut FemType,
    data2: *mut FemType,
    bound1: *mut BoundaryType,
    bound2: *mut BoundaryType,
    nooverlap: c_int,
    info: c_int,
) -> c_int {
    // NOTE: `=0` decl init kept (k read before any overwrite).
    let mut i: c_int = 0;
    // NOTE: `=0` decl init kept (j read before any overwrite).
    let mut j: c_int = 0;
    // NOTE: `=0` decl init kept (k read before any overwrite).
    let mut k: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: 0-init for the &mut handoff (set in unite_merge before reads.)
    let mut nonodes: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mat: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut bodynameis: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut boundarynameis: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let bodyused: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let boundaryused: *mut c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut bcoffset: c_int;
    // NOTE: assigned before every read (C `=0` decl init shape).
    let mut bodyoffset: c_int;
    // NOTE: C min/max offset locals live in unite_offsets now (dead here).
    let noknots: c_int = (*data1).noknots + (*data2).noknots;
    let noelements: c_int = (*data1).noelements + (*data2).noelements;
    // NOTE: C MAX macro (egdef.h:97); arms pure.
    let maxnodes: c_int = if (*data1).maxnodes < (*data2).maxnodes {
        (*data2).maxnodes
    } else {
        (*data1).maxnodes
    };
    if (*data2).dim > (*data1).dim {
        (*data1).dim = (*data2).dim;
    }
    // NOTE: `if(0)` size print omitted (dead in C).
    let usenames: c_int = (*data1).bodynamesexist | (*data1).boundarynamesexist;
    bcoffset = 0;
    bodyoffset = 0;
    if usenames != 0 {
        bodynameis = ivector(1, MAXBODIES as c_int);
        boundarynameis = ivector(1, MAXBCS as c_int);
        bodyused = ivector(1, MAXBODIES as c_int);
        boundaryused = ivector(1, MAXBCS as c_int);
        i = 1;
        while i <= MAXBODIES as c_int {
            *bodynameis.wrapping_offset(i as isize) = FALSE;
            *bodyused.wrapping_offset(i as isize) = FALSE;
            i += 1;
        }
        i = 1;
        while i <= MAXBCS as c_int {
            *boundarynameis.wrapping_offset(i as isize) = FALSE;
            *boundaryused.wrapping_offset(i as isize) = FALSE;
            i += 1;
        }
        // NOTE: keepers first, then conflicts renumbered.
        unite_mark(
            data1, data2, bound1, bound2, bodynameis, boundarynameis, bodyused,
            boundaryused, &mut mat, &mut i, &mut j, &mut k,
        );
        unite_conflicts(
            data1, data2, bound2, bodynameis, boundarynameis, bodyused,
            boundaryused, &mut mat, &mut i, &mut j, &mut k, info,
        );
    } else if nooverlap != 0 {
        unite_offsets(
            data1, data2, bound1, bound2, &mut bcoffset, &mut bodyoffset, &mut mat,
            &mut i, &mut j, &mut k, info,
        );
    }
    (*data1).maxnodes = maxnodes;
    let newtopo: *mut *mut c_int = imatrix(1, noelements, 0, maxnodes - 1);
    let newmaterial: *mut c_int = ivector(1, noelements);
    let newelementtypes: *mut c_int = ivector(1, noelements);
    let newx: *mut Real = dvector(1, noknots);
    let newy: *mut Real = dvector(1, noknots);
    let newz: *mut Real = dvector(1, noknots);
    unite_merge(
        data1, data2, bound1, bound2, newtopo, newmaterial, newelementtypes, newx,
        newy, newz, bodynameis, boundarynameis, usenames, bcoffset, bodyoffset,
        noelements, noknots, maxnodes, &mut mat, &mut nonodes, &mut i, &mut j,
        &mut k,
    );
    free_imatrix(
        (*data1).topology,
        1,
        (*data1).noelements,
        0,
        (*data1).maxnodes - 1,
    );
    free_ivector((*data1).material, 1, (*data1).noelements);
    free_dvector((*data1).x, 1, (*data1).noknots);
    free_dvector((*data1).y, 1, (*data1).noknots);
    free_dvector((*data1).z, 1, (*data1).noknots);
    free_imatrix(
        (*data2).topology,
        1,
        (*data2).noelements,
        0,
        (*data2).maxnodes - 1,
    );
    free_ivector((*data2).material, 1, (*data2).noelements);
    free_dvector((*data2).x, 1, (*data2).noknots);
    free_dvector((*data2).y, 1, (*data2).noknots);
    free_dvector((*data2).z, 1, (*data2).noknots);
    (*data1).noelements = noelements;
    (*data1).noknots = noknots;
    (*data1).topology = newtopo;
    (*data1).material = newmaterial;
    // NOTE: C leaks the old elementtypes (never freed); kept verbatim.
    (*data1).elementtypes = newelementtypes;
    (*data1).x = newx;
    (*data1).y = newy;
    (*data1).z = newz;
    if info != 0 {
        printf(
            c"Two meshes were united to one with %d nodes and %d elements.\n".as_ptr(),
            noknots,
            noelements,
        );
    }
    0
}
/// SAFETY: clone knot/element fill (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn clone_fill(
    data: *mut FemType,
    ncopies: *mut c_int,
    meshsize: *mut Real,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    newz: *mut Real,
    maxmaterial: &mut c_int,
    diffmats: c_int,
    nonodes: &mut c_int,
    ncopy: &mut c_int,
    ind: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
    info: c_int,
) {
    *l = 0;
    while *l < *ncopies.wrapping_offset(2) {
        *k = 0;
        while *k < *ncopies.wrapping_offset(1) {
            *j = 0;
            while *j < *ncopies.wrapping_offset(0) {
                *i = 1;
                while *i <= (*data).noknots {
                    *ncopy = *j
                        + *k * *ncopies.wrapping_offset(0)
                        + *l * *ncopies.wrapping_offset(0) * *ncopies.wrapping_offset(1);
                    *ind = *i + *ncopy * (*data).noknots;
                    *newx.wrapping_offset(*ind as isize) =
                        *(*data).x.wrapping_offset(*i as isize)
                            + *j as Real * *meshsize.wrapping_offset(0);
                    *newy.wrapping_offset(*ind as isize) =
                        *(*data).y.wrapping_offset(*i as isize)
                            + *k as Real * *meshsize.wrapping_offset(1);
                    *newz.wrapping_offset(*ind as isize) =
                        *(*data).z.wrapping_offset(*i as isize)
                            + *l as Real * *meshsize.wrapping_offset(2);
                    *i += 1;
                }
                *j += 1;
            }
            *k += 1;
        }
        *l += 1;
    }
    *maxmaterial = 0;
    if diffmats != 0 {
        *i = 1;
        while *i <= (*data).noelements {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxmaterial < *(*data).material.wrapping_offset(*i as isize) {
                *maxmaterial = *(*data).material.wrapping_offset(*i as isize);
            }
            *i += 1;
        }
        if info != 0 {
            printf(
                c"Material offset for cloning set to: %d\n".as_ptr(),
                *maxmaterial,
            );
        }
    }
    *l = 0;
    while *l < *ncopies.wrapping_offset(2) {
        *k = 0;
        while *k < *ncopies.wrapping_offset(1) {
            *j = 0;
            while *j < *ncopies.wrapping_offset(0) {
                *i = 1;
                while *i <= (*data).noelements {
                    *ncopy = *j
                        + *k * *ncopies.wrapping_offset(0)
                        + *l * *ncopies.wrapping_offset(1) * *ncopies.wrapping_offset(0);
                    *ind = *i + *ncopy * (*data).noelements;
                    *newmaterial.wrapping_offset(*ind as isize) =
                        *(*data).material.wrapping_offset(*i as isize)
                            + diffmats * *maxmaterial * *ncopy;
                    *newelementtypes.wrapping_offset(*ind as isize) =
                        *(*data).elementtypes.wrapping_offset(*i as isize);
                    *nonodes = *newelementtypes.wrapping_offset(*i as isize) % 100;
                    *m = 0;
                    while *m < *nonodes {
                        *(*newtopo.wrapping_offset(*ind as isize))
                            .wrapping_offset(*m as isize) = *(*(*data)
                            .topology
                            .wrapping_offset(*i as isize))
                        .wrapping_offset(*m as isize)
                            + *ncopy * (*data).noknots;
                        *m += 1;
                    }
                    *i += 1;
                }
                *j += 1;
            }
            *k += 1;
        }
        *l += 1;
    }
}

/// SAFETY: clone one boundary record (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn clone_boundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    ncopies: *mut c_int,
    bndr: c_int,
    nosides: c_int,
    maxmaterial: c_int,
    maxtype: c_int,
    diffmats: c_int,
    vparent: *mut c_int,
    vparent2: *mut c_int,
    vside: *mut c_int,
    vside2: *mut c_int,
    vtypes: *mut c_int,
    vmaterial: *mut c_int,
    vnormal: *mut c_int,
    vdiscont: *mut c_int,
    ncopy: &mut c_int,
    ind: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
) {
    *l = 0;
    while *l < *ncopies.wrapping_offset(2) {
        *k = 0;
        while *k < *ncopies.wrapping_offset(1) {
            *j = 0;
            while *j < *ncopies.wrapping_offset(0) {
                *i = 1;
                while *i <= (*bound.wrapping_offset(bndr as isize)).nosides {
                    *ncopy = *j
                        + *k * *ncopies.wrapping_offset(0)
                        + *l * *ncopies.wrapping_offset(1) * *ncopies.wrapping_offset(0);
                    *ind = *i + *ncopy * (*bound.wrapping_offset(bndr as isize)).nosides;
                    *vparent.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .parent
                            .wrapping_offset(*i as isize)
                            + *ncopy * (*data).noelements;
                    *vside.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .side
                            .wrapping_offset(*i as isize);
                    if *(*bound.wrapping_offset(bndr as isize))
                        .parent2
                        .wrapping_offset(*i as isize)
                        != 0
                    {
                        *vparent2.wrapping_offset(*ind as isize) =
                            *(*bound.wrapping_offset(bndr as isize))
                                .parent2
                                .wrapping_offset(*i as isize)
                                + *ncopy * (*data).noelements;
                        *vside2.wrapping_offset(*ind as isize) =
                            *(*bound.wrapping_offset(bndr as isize))
                                .side2
                                .wrapping_offset(*i as isize);
                    } else {
                        // NOTE: C assigns 0.0 (converts to 0).
                        *vparent2.wrapping_offset(*ind as isize) = 0;
                        *vside2.wrapping_offset(*ind as isize) = 0;
                    }
                    *vnormal.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .normal
                            .wrapping_offset(*i as isize);
                    if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
                        *vdiscont.wrapping_offset(*ind as isize) =
                            *(*bound.wrapping_offset(bndr as isize))
                                .discont
                                .wrapping_offset(*i as isize);
                    }
                    *vtypes.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .types
                            .wrapping_offset(*i as isize)
                            + diffmats * *ncopy * maxtype;
                    *vmaterial.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .material
                            .wrapping_offset(*i as isize)
                            + *ncopy * maxmaterial;
                    *i += 1;
                }
                *j += 1;
            }
            *k += 1;
        }
        *l += 1;
    }
    (*bound.wrapping_offset(bndr as isize)).nosides = nosides;
    // NOTE: old boundary vectors leak in C (replaced, never freed).
    (*bound.wrapping_offset(bndr as isize)).side = vside;
    (*bound.wrapping_offset(bndr as isize)).side2 = vside2;
    (*bound.wrapping_offset(bndr as isize)).parent = vparent;
    (*bound.wrapping_offset(bndr as isize)).parent2 = vparent2;
    (*bound.wrapping_offset(bndr as isize)).types = vtypes;
    (*bound.wrapping_offset(bndr as isize)).material = vmaterial;
    (*bound.wrapping_offset(bndr as isize)).normal = vnormal;
    if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
        (*bound.wrapping_offset(bndr as isize)).discont = vdiscont;
    }
    let _ = nosides;
}
/// Replicate the mesh on a copy grid (optionally offsetting materials).
///
/// # Safety
/// `data`, `bound`, `ncopies`, `meshsize` valid.
#[no_mangle]
pub unsafe extern "C" fn CloneMeshes(
    data: *mut FemType,
    bound: *mut BoundaryType,
    ncopies: *mut c_int,
    meshsize: *mut Real,
    diffmats: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads.)
    let mut j: c_int = 0;
    // NOTE: `=0` decl init kept (k read before any overwrite).
    let mut k: c_int = 0;
    // NOTE: `=0` decl init kept (l read before any overwrite).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads).
    let mut m: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads).
    let mut nonodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totcopies: c_int;
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads).
    let mut ind: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads).
    let mut maxmaterial: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxtype: c_int;
    // NOTE: 0-init for the &mut handoff (set in clone_fill before reads).
    let mut ncopy: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bndr: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nosides: c_int;
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    let mut maxcoord: [Real; 3] = [0.0; 3];
    let mut mincoord: [Real; 3] = [0.0; 3];
    // NOTE: null-init (Ivector before guarded reads).
    let mut vparent: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vparent2: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vside: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vside2: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vtypes: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vmaterial: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vnormal: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vdiscont: *mut c_int = core::ptr::null_mut();
    if info != 0 {
        printf(c"CloneMeshes: copying the mesh to a matrix\n".as_ptr());
    }
    if diffmats != 0 && info != 0 {
        printf(c"CloneMeshes: giving each new entity new material and bc indexes\n".as_ptr());
    }
    let origdim: c_int = (*data).dim;
    totcopies = 1;
    if *ncopies.wrapping_offset(2) > 1 {
        (*data).dim = 3;
    } else {
        *ncopies.wrapping_offset(2) = 1;
    }
    i = 0;
    while i < (*data).dim {
        if *ncopies.wrapping_offset(i as isize) > 1 {
            totcopies *= *ncopies.wrapping_offset(i as isize);
        }
        i += 1;
    }
    maxcoord[0] = *(*data).x.wrapping_offset(1);
    mincoord[0] = *(*data).x.wrapping_offset(1);
    maxcoord[1] = *(*data).y.wrapping_offset(1);
    mincoord[1] = *(*data).y.wrapping_offset(1);
    maxcoord[2] = *(*data).z.wrapping_offset(1);
    mincoord[2] = *(*data).z.wrapping_offset(1);
    i = 1;
    while i <= (*data).noknots {
        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
        if maxcoord[0] < *(*data).x.wrapping_offset(i as isize) {
            maxcoord[0] = *(*data).x.wrapping_offset(i as isize);
        }
        if *(*data).x.wrapping_offset(i as isize) < mincoord[0] {
            mincoord[0] = *(*data).x.wrapping_offset(i as isize);
        }
        if maxcoord[1] < *(*data).y.wrapping_offset(i as isize) {
            maxcoord[1] = *(*data).y.wrapping_offset(i as isize);
        }
        if *(*data).y.wrapping_offset(i as isize) < mincoord[1] {
            mincoord[1] = *(*data).y.wrapping_offset(i as isize);
        }
        if maxcoord[2] < *(*data).z.wrapping_offset(i as isize) {
            maxcoord[2] = *(*data).z.wrapping_offset(i as isize);
        }
        if *(*data).z.wrapping_offset(i as isize) < mincoord[2] {
            mincoord[2] = *(*data).z.wrapping_offset(i as isize);
        }
        i += 1;
    }
    i = 0;
    while i < origdim {
        if maxcoord[i as usize] - mincoord[i as usize] > *meshsize.wrapping_offset(i as isize) {
            *meshsize.wrapping_offset(i as isize) = maxcoord[i as usize] - mincoord[i as usize];
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"Meshsize to be copied: %lg %lg %lg\n".as_ptr(),
            *meshsize.wrapping_offset(0),
            *meshsize.wrapping_offset(1),
            *meshsize.wrapping_offset(2),
        );
    }
    let noknots: c_int = totcopies * (*data).noknots;
    let noelements: c_int = totcopies * (*data).noelements;
    let maxnodes: c_int = (*data).maxnodes;
    if info != 0 {
        printf(
            c"Copying the mesh to %d identical domains in %d-dim.\n".as_ptr(),
            totcopies,
            (*data).dim,
        );
    }
    (*data).maxnodes = maxnodes;
    let newtopo: *mut *mut c_int = imatrix(1, noelements, 0, maxnodes - 1);
    let newmaterial: *mut c_int = ivector(1, noelements);
    let newelementtypes: *mut c_int = ivector(1, noelements);
    let newx: *mut Real = dvector(1, noknots);
    let newy: *mut Real = dvector(1, noknots);
    let newz: *mut Real = dvector(1, noknots);
    clone_fill(
        data, ncopies, meshsize, newtopo, newmaterial, newelementtypes, newx, newy,
        newz, &mut maxmaterial, diffmats, &mut nonodes, &mut ncopy, &mut ind, &mut i,
        &mut j, &mut k, &mut l, &mut m, info,
    );
    maxtype = 0;
    if diffmats != 0 {
        j = 0;
        while j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(j as isize)).created == 0 {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxtype
                    < *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize)
                {
                    maxtype = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                }
                i += 1;
            }
            j += 1;
        }
        if info != 0 {
            printf(
                c"Boundary offset for cloning set to: %d\n".as_ptr(),
                maxtype,
            );
        }
    }
    bndr = 0;
    while bndr < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(bndr as isize)).created == 0 {
            bndr += 1;
            continue;
        }
        nosides = totcopies * (*bound.wrapping_offset(bndr as isize)).nosides;
        vparent = ivector(1, nosides);
        vparent2 = ivector(1, nosides);
        vside = ivector(1, nosides);
        vside2 = ivector(1, nosides);
        vmaterial = ivector(1, nosides);
        vtypes = ivector(1, nosides);
        vnormal = ivector(1, nosides);
        if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
            vdiscont = ivector(1, nosides);
            i = 1;
            while i <= nosides {
                *vdiscont.wrapping_offset(i as isize) = 0;
                i += 1;
            }
        }
        clone_boundary(
            data, bound, ncopies, bndr, nosides, maxmaterial, maxtype, diffmats,
            vparent, vparent2, vside, vside2, vtypes, vmaterial, vnormal, vdiscont,
            &mut ncopy, &mut ind, &mut i, &mut j, &mut k, &mut l,
        );
        bndr += 1;
    }
    free_imatrix(
        (*data).topology,
        1,
        (*data).noelements,
        0,
        (*data).maxnodes - 1,
    );
    free_ivector((*data).material, 1, (*data).noelements);
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).noelements = noelements;
    (*data).noknots = noknots;
    (*data).topology = newtopo;
    (*data).material = newmaterial;
    (*data).elementtypes = newelementtypes;
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    if (*data).bodynamesexist != 0 || (*data).boundarynamesexist != 0 {
        printf(c"Cloning cannot treat names yet, omitting treatment of names for now!\n".as_ptr());
        (*data).bodynamesexist = FALSE;
        (*data).boundarynamesexist = FALSE;
    }
    if info != 0 {
        printf(c"The mesh was copied to several identical meshes\n".as_ptr());
    }
    0
}
/// SAFETY: mirror knot/element fill (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn mirror_fill(
    data: *mut FemType,
    symmaxis: *mut c_int,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    newz: *mut Real,
    nonodes: &mut c_int,
    ind: &mut c_int,
    ind0: &mut c_int,
    elem0: &mut c_int,
    axis1: &mut c_int,
    axis2: &mut c_int,
    axis3: &mut c_int,
    i: &mut c_int,
    m: &mut c_int,
) {
    *ind0 = 0;
    *axis1 = 0;
    while *axis1 <= *symmaxis.wrapping_offset(0) {
        *axis2 = 0;
        while *axis2 <= *symmaxis.wrapping_offset(1) {
            *axis3 = 0;
            while *axis3 <= *symmaxis.wrapping_offset(2) {
                *i = 1;
                while *i <= (*data).noknots {
                    *ind = *i + *ind0;
                    *newx.wrapping_offset(*ind as isize) =
                        (1 - 2 * *axis1) as Real * *(*data).x.wrapping_offset(*i as isize);
                    *newy.wrapping_offset(*ind as isize) =
                        (1 - 2 * *axis2) as Real * *(*data).y.wrapping_offset(*i as isize);
                    *newz.wrapping_offset(*ind as isize) =
                        (1 - 2 * *axis3) as Real * *(*data).z.wrapping_offset(*i as isize);
                    *newmaterial.wrapping_offset(*ind as isize) =
                        *(*data).material.wrapping_offset(*i as isize);
                    *newelementtypes.wrapping_offset(*ind as isize) =
                        *(*data).elementtypes.wrapping_offset(*i as isize);
                    *i += 1;
                }
                *ind0 += (*data).noknots;
                *axis3 += 1;
            }
            *axis2 += 1;
        }
        *axis1 += 1;
    }
    *elem0 = 0;
    *ind0 = 0;
    *axis1 = 0;
    while *axis1 <= *symmaxis.wrapping_offset(0) {
        *axis2 = 0;
        while *axis2 <= *symmaxis.wrapping_offset(1) {
            *axis3 = 0;
            while *axis3 <= *symmaxis.wrapping_offset(2) {
                *i = 1;
                while *i <= (*data).noelements {
                    *ind = *i + *elem0;
                    *newmaterial.wrapping_offset(*ind as isize) =
                        *(*data).material.wrapping_offset(*i as isize);
                    *newelementtypes.wrapping_offset(*ind as isize) =
                        *(*data).elementtypes.wrapping_offset(*i as isize);
                    *nonodes = *newelementtypes.wrapping_offset(*i as isize) % 100;
                    *m = 0;
                    while *m < *nonodes {
                        *(*newtopo.wrapping_offset(*ind as isize))
                            .wrapping_offset(*m as isize) = *(*(*data)
                            .topology
                            .wrapping_offset(*i as isize))
                        .wrapping_offset(*m as isize)
                            + *ind0;
                        *m += 1;
                    }
                    *i += 1;
                }
                *elem0 += (*data).noelements;
                *ind0 += (*data).noknots;
                printf(c"elem0=%d ind0=%d\n".as_ptr(), *elem0, *ind0);
                *axis3 += 1;
            }
            *axis2 += 1;
        }
        *axis1 += 1;
    }
}

/// SAFETY: mirror one boundary record (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn mirror_boundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    symmaxis: *mut c_int,
    bndr: c_int,
    nosides: &mut c_int,
    diffmats: c_int,
    maxtype: c_int,
    symmbound: c_int,
    vparent: *mut c_int,
    vparent2: *mut c_int,
    vside: *mut c_int,
    vside2: *mut c_int,
    vtypes: *mut c_int,
    vmaterial: *mut c_int,
    vnormal: *mut c_int,
    vdiscont: *mut c_int,
    ind: &mut c_int,
    ind0: &mut c_int,
    elem0: &mut c_int,
    symmcount: &mut c_int,
    axis1: &mut c_int,
    axis2: &mut c_int,
    axis3: &mut c_int,
    i: &mut c_int,
) {
    *ind = 0;
    *symmcount = 0;
    *elem0 = 0;
    *ind0 = 0;
    *axis1 = 0;
    while *axis1 <= *symmaxis.wrapping_offset(0) {
        *axis2 = 0;
        while *axis2 <= *symmaxis.wrapping_offset(1) {
            *axis3 = 0;
            while *axis3 <= *symmaxis.wrapping_offset(2) {
                *i = 1;
                while *i <= (*bound.wrapping_offset(bndr as isize)).nosides {
                    if *(*bound.wrapping_offset(bndr as isize))
                        .types
                        .wrapping_offset(*i as isize)
                        == symmbound
                    {
                        *i += 1;
                        continue;
                    }
                    *ind += 1;
                    *vparent.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .parent
                            .wrapping_offset(*i as isize)
                            + *elem0;
                    *vparent2.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .parent2
                            .wrapping_offset(*i as isize)
                            + *elem0;
                    *vside.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .side
                            .wrapping_offset(*i as isize);
                    *vside2.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .side2
                            .wrapping_offset(*i as isize);
                    *vnormal.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .normal
                            .wrapping_offset(*i as isize);
                    if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
                        *vdiscont.wrapping_offset(*ind as isize) =
                            *(*bound.wrapping_offset(bndr as isize))
                                .discont
                                .wrapping_offset(*i as isize);
                    }
                    *vtypes.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .types
                            .wrapping_offset(*i as isize)
                            + diffmats * *symmcount * maxtype;
                    *vmaterial.wrapping_offset(*ind as isize) =
                        *(*bound.wrapping_offset(bndr as isize))
                            .material
                            .wrapping_offset(*i as isize);
                    *i += 1;
                }
                *symmcount += 1;
                *elem0 += (*data).noelements;
                *axis3 += 1;
            }
            *axis2 += 1;
        }
        *axis1 += 1;
    }
    *nosides = *ind;
    (*bound.wrapping_offset(bndr as isize)).nosides = *nosides;
    // NOTE: old boundary vectors leak in C (replaced, never freed).
    (*bound.wrapping_offset(bndr as isize)).side = vside;
    (*bound.wrapping_offset(bndr as isize)).side2 = vside2;
    (*bound.wrapping_offset(bndr as isize)).parent = vparent;
    (*bound.wrapping_offset(bndr as isize)).parent2 = vparent2;
    (*bound.wrapping_offset(bndr as isize)).types = vtypes;
    (*bound.wrapping_offset(bndr as isize)).material = vmaterial;
    (*bound.wrapping_offset(bndr as isize)).normal = vnormal;
    if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
        (*bound.wrapping_offset(bndr as isize)).discont = vdiscont;
    }
}
/// Mirror the mesh across symmetry axes and unite with the original.
///
/// # Safety
/// `data`, `bound`, `symmaxis`, `meshsize` valid.
#[no_mangle]
pub unsafe extern "C" fn MirrorMeshes(
    data: *mut FemType,
    bound: *mut BoundaryType,
    symmaxis: *mut c_int,
    mut diffmats: c_int,
    meshsize: *mut Real,
    symmbound: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut m: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut nonodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totcopies: c_int;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut ind: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bndr: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nosides: c_int;
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    let mut maxcoord: [Real; 3] = [0.0; 3];
    let mut mincoord: [Real; 3] = [0.0; 3];
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut ind0: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut elem0: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut axis1: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut axis2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_fill before reads).
    let mut axis3: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in mirror_boundary before reads).
    let mut symmcount: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vparent: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vparent2: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vside: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vside2: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vtypes: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vmaterial: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut vnormal: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init; only touched when ediscont is set.
    let mut vdiscont: *mut c_int = core::ptr::null_mut();
    printf(c"MirrorMeshes: making a symmetric mapping of the mesh\n".as_ptr());
    if *symmaxis.wrapping_offset(0) != 0 {
        *symmaxis.wrapping_offset(0) = 1;
    }
    if *symmaxis.wrapping_offset(1) != 0 {
        *symmaxis.wrapping_offset(1) = 1;
    }
    if *symmaxis.wrapping_offset(2) != 0 {
        *symmaxis.wrapping_offset(2) = 1;
    }
    if (*data).dim < 3 {
        *symmaxis.wrapping_offset(2) = 0;
    }
    maxcoord[0] = *(*data).x.wrapping_offset(1);
    mincoord[0] = *(*data).x.wrapping_offset(1);
    maxcoord[1] = *(*data).y.wrapping_offset(1);
    mincoord[1] = *(*data).y.wrapping_offset(1);
    maxcoord[2] = *(*data).z.wrapping_offset(1);
    mincoord[2] = *(*data).z.wrapping_offset(1);
    i = 1;
    while i <= (*data).noknots {
        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
        if maxcoord[0] < *(*data).x.wrapping_offset(i as isize) {
            maxcoord[0] = *(*data).x.wrapping_offset(i as isize);
        }
        if *(*data).x.wrapping_offset(i as isize) < mincoord[0] {
            mincoord[0] = *(*data).x.wrapping_offset(i as isize);
        }
        if maxcoord[1] < *(*data).y.wrapping_offset(i as isize) {
            maxcoord[1] = *(*data).y.wrapping_offset(i as isize);
        }
        if *(*data).y.wrapping_offset(i as isize) < mincoord[1] {
            mincoord[1] = *(*data).y.wrapping_offset(i as isize);
        }
        if maxcoord[2] < *(*data).z.wrapping_offset(i as isize) {
            maxcoord[2] = *(*data).z.wrapping_offset(i as isize);
        }
        if *(*data).z.wrapping_offset(i as isize) < mincoord[2] {
            mincoord[2] = *(*data).z.wrapping_offset(i as isize);
        }
        i += 1;
    }
    i = 0;
    while i < 3 {
        if maxcoord[i as usize] - mincoord[i as usize] > *meshsize.wrapping_offset(i as isize) {
            *meshsize.wrapping_offset(i as isize) = maxcoord[i as usize] - mincoord[i as usize];
        }
        i += 1;
    }
    if diffmats != 0 {
        diffmats = 1;
    }
    totcopies = 1;
    i = 0;
    while i < 3 {
        if *symmaxis.wrapping_offset(i as isize) != 0 {
            totcopies *= 2;
        }
        i += 1;
    }
    let noknots: c_int = totcopies * (*data).noknots;
    let noelements: c_int = totcopies * (*data).noelements;
    let maxnodes: c_int = (*data).maxnodes;
    printf(c"Mirroring the mesh to %d symmetrical domains.\n".as_ptr(), totcopies);
    (*data).maxnodes = maxnodes;
    let newtopo: *mut *mut c_int = imatrix(1, noelements, 0, maxnodes - 1);
    let newmaterial: *mut c_int = ivector(1, noelements);
    let newelementtypes: *mut c_int = ivector(1, noelements);
    let newx: *mut Real = dvector(1, noknots);
    let newy: *mut Real = dvector(1, noknots);
    let newz: *mut Real = dvector(1, noknots);
    mirror_fill(
        data, symmaxis, newtopo, newmaterial, newelementtypes, newx, newy, newz,
        &mut nonodes, &mut ind, &mut ind0, &mut elem0, &mut axis1, &mut axis2,
        &mut axis3, &mut i, &mut m,
    );
    maxtype = 0;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if maxtype
                < *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)
            {
                maxtype = *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize);
            }
            i += 1;
        }
        j += 1;
    }
    bndr = 0;
    while bndr < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(bndr as isize)).created == 0 {
            bndr += 1;
            continue;
        }
        nosides = totcopies * (*bound.wrapping_offset(bndr as isize)).nosides;
        vparent = ivector(1, nosides);
        vparent2 = ivector(1, nosides);
        vside = ivector(1, nosides);
        vside2 = ivector(1, nosides);
        vmaterial = ivector(1, nosides);
        vtypes = ivector(1, nosides);
        vnormal = ivector(1, nosides);
        if (*bound.wrapping_offset(bndr as isize)).ediscont != 0 {
            vdiscont = ivector(1, nosides);
            i = 1;
            while i <= nosides {
                *vdiscont.wrapping_offset(i as isize) = 0;
                i += 1;
            }
        }
        mirror_boundary(
            data, bound, symmaxis, bndr, &mut nosides, diffmats, maxtype, symmbound,
            vparent, vparent2, vside, vside2, vtypes, vmaterial, vnormal, vdiscont,
            &mut ind, &mut ind0, &mut elem0, &mut symmcount, &mut axis1, &mut axis2,
            &mut axis3, &mut i,
        );
        bndr += 1;
    }
    free_imatrix(
        (*data).topology,
        1,
        (*data).noelements,
        0,
        (*data).maxnodes - 1,
    );
    free_ivector((*data).material, 1, (*data).noelements);
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).noelements = noelements;
    (*data).noknots = noknots;
    (*data).topology = newtopo;
    (*data).material = newmaterial;
    (*data).elementtypes = newelementtypes;
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    if (*data).bodynamesexist != 0 || (*data).boundarynamesexist != 0 {
        printf(c"Mirroring cannot treat names yet, omitting treatment of names for now!\n".as_ptr());
        (*data).bodynamesexist = FALSE;
        (*data).boundarynamesexist = FALSE;
    }
    if info != 0 {
        printf(c"The mesh was copied to several identical meshes\n".as_ptr());
    }
    0
}
/// SAFETY: automatic neighbour-table build (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn reorder_auto_neighbours(
    data: *mut FemType,
    neighbours: *mut *mut c_int,
    cands: *mut c_int,
    noelements: c_int,
    maxnodes: c_int,
    nocands: &mut c_int,
    ind: &mut c_int,
    ind2: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    *j = 1;
    while *j <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
        nonodes = elemtype % 100;
        *nocands = 0;
        *i = 0;
        while *i < nonodes {
            *ind = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            if elemtype == 404 || elemtype == 303 {
                *nocands = 2;
                *cands.wrapping_offset(1) = (*i + 1) % nonodes;
                *cands.wrapping_offset(2) = (*i + nonodes - 1) % nonodes;
            } else if elemtype == 808 {
                *nocands = 3;
                if *i < 4 {
                    *cands.wrapping_offset(1) = (*i + 1) % 4;
                    *cands.wrapping_offset(2) = (*i + 3) % 4;
                    *cands.wrapping_offset(3) = *i + 4;
                } else {
                    *cands.wrapping_offset(1) = (*i - 4 + 1) % 4 + 4;
                    *cands.wrapping_offset(2) = (*i - 4 + 3) % 4 + 4;
                    *cands.wrapping_offset(3) = *i - 4;
                }
            }
            *k = 1;
            while *k <= *nocands {
                *ind2 = *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*cands.wrapping_offset(*k as isize) as isize);
                *l = 1;
                while *l <= maxnodes {
                    if *(*neighbours.wrapping_offset(*ind as isize))
                        .wrapping_offset(*l as isize)
                        == 0
                    {
                        break;
                    }
                    if *(*neighbours.wrapping_offset(*ind as isize))
                        .wrapping_offset(*l as isize)
                        == *ind2
                    {
                        *ind2 = 0;
                    }
                    *l += 1;
                }
                if *ind2 != 0 {
                    *(*neighbours.wrapping_offset(*ind as isize))
                        .wrapping_offset(*l as isize) = *ind2;
                }
                *k += 1;
            }
            *i += 1;
        }
        *j += 1;
    }
}

/// SAFETY: automatic greedy reorder sweep (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn reorder_auto_sweep(
    data: *mut FemType,
    neighbours: *mut *mut c_int,
    newrank: *mut c_int,
    newindx: *mut c_int,
    oldrank: *mut c_int,
    oldindx: *mut c_int,
    origindx: *mut c_int,
    noknots: c_int,
    iterations: c_int,
    minwidth: &mut c_int,
    indexwidth: &mut c_int,
    ind: &mut c_int,
    ind2: &mut c_int,
    indready: &mut c_int,
    iter: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    l: &mut c_int,
    info: c_int,
) {
    *iter = 1;
    while *iter <= iterations {
        if info != 0 {
            printf(c"Optimal topology testing %d\n".as_ptr(), *iter);
        }
        *i = 1;
        while *i <= noknots {
            *newrank.wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
        *ind = 0;
        *indready = 1;
        loop {
            if *indready > *ind {
                *l = noknots;
                while *l >= 1 {
                    // NOTE: C `if(j = oldindx[l])` assigns then tests.
                    *j = *oldindx.wrapping_offset(*l as isize);
                    if *j != 0 {
                        break;
                    }
                    *l -= 1;
                }
                if info != 0 {
                    printf(
                        c"Starting over from node %d when ind=%d indready=%d\n".as_ptr(),
                        *j,
                        *ind,
                        *indready,
                    );
                }
            } else {
                *j = *newindx.wrapping_offset(*indready as isize);
            }
            // NOTE: C `for(l=1;ind2=neighbours[j][l];l++)` stops at zero.
            *l = 1;
            loop {
                *ind2 = *(*neighbours.wrapping_offset(*j as isize))
                    .wrapping_offset(*l as isize);
                if *ind2 == 0 {
                    break;
                }
                if *newrank.wrapping_offset(*ind2 as isize) == 0 {
                    *ind += 1;
                    *newrank.wrapping_offset(*ind2 as isize) = *ind;
                    *newindx.wrapping_offset(*ind as isize) = *ind2;
                    *oldindx.wrapping_offset(
                        *oldrank.wrapping_offset(*ind2 as isize) as isize,
                    ) = 0;
                    *oldrank.wrapping_offset(*ind2 as isize) = 0;
                }
                *l += 1;
            }
            *indready += 1;
            if !(*ind < noknots) {
                break;
            }
        }
        *indexwidth = CalculateIndexwidth(data, TRUE, newrank);
        if info != 0 {
            printf(
                c"Indexwidth of the suggested node order is %d.\n".as_ptr(),
                *indexwidth,
            );
        }
        *i = 1;
        while *i <= noknots {
            *oldrank.wrapping_offset(*i as isize) = *newrank.wrapping_offset(*i as isize);
            *i += 1;
        }
        *i = 1;
        while *i <= noknots {
            *oldindx.wrapping_offset(*i as isize) = *newindx.wrapping_offset(*i as isize);
            *i += 1;
        }
        if *indexwidth < *minwidth {
            *i = 1;
            while *i <= noknots {
                *origindx.wrapping_offset(*i as isize) =
                    *newindx.wrapping_offset(*i as isize);
                *i += 1;
            }
            *minwidth = *indexwidth;
        }
        *iter += 1;
    }
}

/// SAFETY: greedy automatic node reorder (bandwidth reduction).
unsafe fn ReorderAutomatic(
    data: *mut FemType,
    _iterations: c_int,
    origindx: *mut c_int,
    corder: *mut Real,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: `=0` decl init kept (k read before any overwrite).
    let mut k: c_int = 0;
    // NOTE: `=0` decl init kept (l read before any overwrite).
    let mut l: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: main-scope nonodes unused (neighbour helper owns its copy).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minwidth: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut indexwidth: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nocands: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cantdo: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut indready: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut iter: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nolocal: c_int;
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real;
    // NOTE: nolocal/dx/dy/dz set in the sort loop before every read.
    // NOTE: C overwrites the parameter; shadowed to a constant.
    let iterations: c_int = 3;
    iter = 0;
    let maxnodes: c_int = 8;
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    cantdo = FALSE;
    j = 1;
    while j <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(j as isize);
        if elemtype != 404 && elemtype != 303 && elemtype != 808 {
            cantdo = elemtype;
        }
        j += 1;
    }
    if cantdo != 0 {
        printf(
            c"Automatic reordering not specified for elementtype %d\n".as_ptr(),
            cantdo,
        );
        return;
    }
    printf(c"Allocating...\n".as_ptr());
    let cands: *mut c_int = ivector(1, maxnodes);
    let localorder: *mut c_int = ivector(1, maxnodes);
    let localtmp: *mut c_int = ivector(1, maxnodes);
    let localdist: *mut Real = dvector(1, maxnodes);
    let neighbours: *mut *mut c_int = imatrix(1, noknots, 1, maxnodes);
    let newrank: *mut c_int = ivector(1, noknots);
    let oldrank: *mut c_int = ivector(1, noknots);
    let newindx: *mut c_int = ivector(1, noknots);
    let oldindx: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *oldindx.wrapping_offset(i as isize) = *origindx.wrapping_offset(i as isize);
        i += 1;
    }
    i = 1;
    while i <= noknots {
        *oldrank.wrapping_offset(*origindx.wrapping_offset(i as isize) as isize) = i;
        i += 1;
    }
    minwidth = CalculateIndexwidth(data, TRUE, oldrank);
    if info != 0 {
        printf(
            c"Indexwidth of the initial node order is %d.\n".as_ptr(),
            minwidth,
        );
    }
    j = 1;
    while j <= noknots {
        i = 1;
        while i <= maxnodes {
            *(*neighbours.wrapping_offset(j as isize)).wrapping_offset(i as isize) = 0;
            i += 1;
        }
        j += 1;
    }
    if info != 0 {
        printf(c"Initializing neighbours\n".as_ptr());
    }
    reorder_auto_neighbours(
        data, neighbours, cands, noelements, maxnodes, &mut nocands, &mut ind,
        &mut ind2, &mut i, &mut j, &mut k, &mut l,
    );
    // NOTE: `#if 0` neighbour dump omitted (dead in C).
    if info != 0 {
        printf(c"Reordering neighbours table\n".as_ptr());
    }
    j = 1;
    while j <= noknots {
        nolocal = 0;
        // NOTE: C `dz = 0.0` dropped (overwritten before any read).
        l = 1;
        while l <= maxnodes {
            ind = *(*neighbours.wrapping_offset(j as isize)).wrapping_offset(l as isize);
            if ind != 0 {
                nolocal += 1;
                *localtmp.wrapping_offset(l as isize) = ind;
                dx = *(*data).x.wrapping_offset(l as isize)
                    - *(*data).x.wrapping_offset(ind as isize);
                dy = *(*data).y.wrapping_offset(l as isize)
                    - *(*data).y.wrapping_offset(ind as isize);
                dz = *(*data).z.wrapping_offset(l as isize)
                    - *(*data).z.wrapping_offset(ind as isize);
                *localdist.wrapping_offset(l as isize) = *corder.wrapping_offset(0)
                    * fabs(dx)
                    + *corder.wrapping_offset(1) * fabs(dy)
                    + *corder.wrapping_offset(2) * fabs(dz);
            }
            l += 1;
        }
        SortIndex(nolocal, localdist, localorder);
        l = 1;
        while l <= nolocal {
            *(*neighbours.wrapping_offset(j as isize)).wrapping_offset(l as isize) =
                *localtmp.wrapping_offset(
                    *localorder.wrapping_offset(l as isize) as isize,
                );
            l += 1;
        }
        // NOTE: `#if 0` order dump omitted (dead in C).
        j += 1;
    }
    reorder_auto_sweep(
        data, neighbours, newrank, newindx, oldrank, oldindx, origindx, noknots,
        iterations, &mut minwidth, &mut indexwidth, &mut ind, &mut ind2, &mut indready,
        &mut iter, &mut i, &mut j, &mut l, info,
    );
    free_ivector(cands, 1, maxnodes);
    free_ivector(localorder, 1, maxnodes);
    free_ivector(localtmp, 1, maxnodes);
    free_dvector(localdist, 1, maxnodes);
    free_imatrix(neighbours, 1, noknots, 1, maxnodes);
    free_ivector(newrank, 1, noknots);
    free_ivector(oldrank, 1, noknots);
    free_ivector(newindx, 1, noknots);
    free_ivector(oldindx, 1, noknots);
}
/// Reorder knots/elements along a linear functional (or automatically).
///
/// # Safety
/// `data`, `bound`, `corder` valid.
#[no_mangle]
pub unsafe extern "C" fn ReorderElements(
    data: *mut FemType,
    bound: *mut BoundaryType,
    manual: c_int,
    corder: *mut Real,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let dz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let cx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let cy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let cz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let cbase: Real;
    let noelements: c_int = (*data).noelements;
    let oldnoelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let oldnoknots: c_int = (*data).noknots;
    if info != 0 {
        printf(
            c"Reordering %d knots and %d elements in %d-dimensions.\n".as_ptr(),
            noknots,
            noelements,
            (*data).dim,
        );
    }
    let length: c_int = if noelements > noknots {
        noelements
    } else {
        noknots
    };
    let arrange: *mut Real = dvector(1, length);
    let indx: *mut c_int = ivector(1, noknots);
    let revindx: *mut c_int = ivector(1, noknots);
    let elemindx: *mut c_int = ivector(1, noelements);
    let revelemindx: *mut c_int = ivector(1, noelements);
    if manual == 1 {
        cx = *corder.wrapping_offset(0);
        cy = *corder.wrapping_offset(1);
        cz = *corder.wrapping_offset(2);
    } else {
        // NOTE: block-scoped C locals become bindings.
        let mut xmin: Real;
        let mut xmax: Real;
        let mut ymin: Real;
        let mut ymax: Real;
        let mut zmin: Real;
        let mut zmax: Real;
        xmin = *(*data).x.wrapping_offset(1);
        xmax = *(*data).x.wrapping_offset(1);
        ymin = *(*data).y.wrapping_offset(1);
        ymax = *(*data).y.wrapping_offset(1);
        zmin = *(*data).z.wrapping_offset(1);
        zmax = *(*data).z.wrapping_offset(1);
        i = 1;
        while i <= (*data).noknots {
            // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
            if *(*data).x.wrapping_offset(i as isize) < xmin {
                xmin = *(*data).x.wrapping_offset(i as isize);
            }
            if xmax < *(*data).x.wrapping_offset(i as isize) {
                xmax = *(*data).x.wrapping_offset(i as isize);
            }
            if *(*data).y.wrapping_offset(i as isize) < ymin {
                ymin = *(*data).y.wrapping_offset(i as isize);
            }
            if ymax < *(*data).y.wrapping_offset(i as isize) {
                ymax = *(*data).y.wrapping_offset(i as isize);
            }
            if *(*data).z.wrapping_offset(i as isize) < zmin {
                zmin = *(*data).z.wrapping_offset(i as isize);
            }
            if zmax < *(*data).z.wrapping_offset(i as isize) {
                zmax = *(*data).z.wrapping_offset(i as isize);
            }
            i += 1;
        }
        dx = xmax - xmin;
        dy = ymax - ymin;
        dz = zmax - zmin;
        // NOTE: dominant-axis weighting; bool counts as 0/1 doubles.
        cbase = 100.0;
        cx = pow(
            cbase,
            (dx > dy) as c_int as Real + (dx > dz) as c_int as Real,
        );
        cy = pow(
            cbase,
            (dy > dx) as c_int as Real + (dy > dz) as c_int as Real,
        );
        // NOTE: C tests dz>dx twice here (not dz>dy); kept verbatim.
        cz = pow(
            cbase,
            (dz > dx) as c_int as Real + (dz > dx) as c_int as Real,
        );
        *corder.wrapping_offset(0) = cx;
        *corder.wrapping_offset(1) = cy;
        *corder.wrapping_offset(2) = cz;
    }
    if info != 0 {
        printf(
            c"Ordering with (%.3lg*x + %.3lg*y + %.3lg*z)\n".as_ptr(),
            cx,
            cy,
            cz,
        );
    }
    i = 1;
    while i <= noknots {
        *arrange.wrapping_offset(i as isize) = cx * *(*data).x.wrapping_offset(i as isize)
            + cy * *(*data).y.wrapping_offset(i as isize)
            + cz * *(*data).z.wrapping_offset(i as isize);
        i += 1;
    }
    SortIndex(noknots, arrange, indx);
    if manual == 2 {
        ReorderAutomatic(data, 0, indx, corder, TRUE);
    }
    i = 1;
    while i <= noknots {
        *revindx.wrapping_offset(*indx.wrapping_offset(i as isize) as isize) = i;
        i += 1;
    }
    j = 1;
    while j <= noelements {
        nonodes = *(*data).elementtypes.wrapping_offset(j as isize) % 100;
        *arrange.wrapping_offset(j as isize) = 0.0;
        i = 0;
        while i < nonodes {
            k = *(*(*data).topology.wrapping_offset(j as isize))
                .wrapping_offset(i as isize);
            *arrange.wrapping_offset(j as isize) += cx * *(*data).x.wrapping_offset(k as isize)
                + cy * *(*data).y.wrapping_offset(k as isize)
                + cz * *(*data).z.wrapping_offset(k as isize);
            i += 1;
        }
        j += 1;
    }
    SortIndex(noelements, arrange, elemindx);
    i = 1;
    while i <= noelements {
        *revelemindx.wrapping_offset(*elemindx.wrapping_offset(i as isize) as isize) = i;
        i += 1;
    }
    // NOTE: `#if 0` order dump omitted (dead in C).
    if info != 0 {
        printf(c"Moving knots to new positions\n".as_ptr());
    }
    let newx: *mut Real = dvector(1, (*data).noknots);
    let newy: *mut Real = dvector(1, (*data).noknots);
    let newz: *mut Real = dvector(1, (*data).noknots);
    i = 1;
    while i <= (*data).noknots {
        *newx.wrapping_offset(i as isize) =
            *(*data).x.wrapping_offset(*indx.wrapping_offset(i as isize) as isize);
        *newy.wrapping_offset(i as isize) =
            *(*data).y.wrapping_offset(*indx.wrapping_offset(i as isize) as isize);
        *newz.wrapping_offset(i as isize) =
            *(*data).z.wrapping_offset(*indx.wrapping_offset(i as isize) as isize);
        i += 1;
    }
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    if info != 0 {
        printf(c"Moving the elements to new positions\n".as_ptr());
    }
    let newtopology: *mut *mut c_int = imatrix(1, noelements, 0, (*data).maxnodes - 1);
    let newmaterial: *mut c_int = ivector(1, noelements);
    let newelementtypes: *mut c_int = ivector(1, noelements);
    j = 1;
    while j <= noelements {
        *newmaterial.wrapping_offset(j as isize) = *(*data)
            .material
            .wrapping_offset(*elemindx.wrapping_offset(j as isize) as isize);
        *newelementtypes.wrapping_offset(j as isize) = *(*data)
            .elementtypes
            .wrapping_offset(*elemindx.wrapping_offset(j as isize) as isize);
        nonodes = *newelementtypes.wrapping_offset(j as isize) % 100;
        i = 0;
        while i < nonodes {
            k = *(*(*data).topology
                .wrapping_offset(*elemindx.wrapping_offset(j as isize) as isize))
            .wrapping_offset(i as isize);
            *(*newtopology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                *revindx.wrapping_offset(k as isize);
            i += 1;
        }
        j += 1;
    }
    (*data).material = newmaterial;
    (*data).elementtypes = newelementtypes;
    (*data).topology = newtopology;
    printf(c"Moving the parents of the boundary nodes.\n".as_ptr());
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize) = *revelemindx.wrapping_offset(
                *(*bound.wrapping_offset(j as isize))
                    .parent
                    .wrapping_offset(i as isize) as isize,
            );
            if *(*bound.wrapping_offset(j as isize))
                .parent2
                .wrapping_offset(i as isize)
                != 0
            {
                *(*bound.wrapping_offset(j as isize))
                    .parent2
                    .wrapping_offset(i as isize) = *revelemindx.wrapping_offset(
                    *(*bound.wrapping_offset(j as isize))
                        .parent2
                        .wrapping_offset(i as isize) as isize,
                );
            }
            i += 1;
        }
        j += 1;
    }
    i = CalculateIndexwidth(data, FALSE, indx);
    printf(c"Indexwidth of the new node order is %d.\n".as_ptr(), i);
    free_dvector(arrange, 1, length);
    free_ivector(indx, 1, oldnoknots);
    free_ivector(revindx, 1, oldnoknots);
    free_ivector(elemindx, 1, oldnoelements);
    free_ivector(revelemindx, 1, oldnoelements);
}
/// Drop knots no element references, compacting the numbering.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn RemoveUnusedNodes(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut activeknots: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let indx: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *indx.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    j = 1;
    while j <= noelements {
        nonodes = *(*data).elementtypes.wrapping_offset(j as isize) % 100;
        i = 0;
        while i < nonodes {
            *indx.wrapping_offset(
                *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize)
                    as isize,
            ) = 1;
            i += 1;
        }
        j += 1;
    }
    activeknots = 0;
    j = 0;
    i = 1;
    while i <= noknots {
        if *indx.wrapping_offset(i as isize) != 0 {
            activeknots += 1;
            *indx.wrapping_offset(i as isize) = activeknots;
        } else {
            j += 1;
            if j <= 5 {
                printf(c"Unused node index in mesh: %d\n".as_ptr(), i);
            }
        }
        i += 1;
    }
    if noknots == activeknots {
        if info != 0 {
            printf(
                c"All %d nodes were used by the mesh elements\n".as_ptr(),
                noknots,
            );
        }
        return 1;
    }
    if info != 0 {
        printf(
            c"Removing %d unused nodes (out of %d) from the mesh\n".as_ptr(),
            noknots - activeknots,
            noknots,
        );
    }
    j = 1;
    while j <= noelements {
        nonodes = *(*data).elementtypes.wrapping_offset(j as isize) % 100;
        i = 0;
        while i < nonodes {
            *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                *indx.wrapping_offset(
                    *(*(*data).topology.wrapping_offset(j as isize))
                        .wrapping_offset(i as isize) as isize,
                );
            i += 1;
        }
        j += 1;
    }
    i = 1;
    while i <= noknots {
        j = *indx.wrapping_offset(i as isize);
        if j == 0 {
            i += 1;
            continue;
        }
        *(*data).x.wrapping_offset(j as isize) = *(*data).x.wrapping_offset(i as isize);
        *(*data).y.wrapping_offset(j as isize) = *(*data).y.wrapping_offset(i as isize);
        *(*data).z.wrapping_offset(j as isize) = *(*data).z.wrapping_offset(i as isize);
        i += 1;
    }
    (*data).noknots = activeknots;
    free_ivector(indx, 1, noknots);
    0
}
/// SAFETY: boundary type census + map init (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn renumber_bc_census(
    data: *mut FemType,
    bound: *mut BoundaryType,
    mapbc: *mut *mut c_int,
    sideind: *mut c_int,
    elemdim: &mut c_int,
    elemtype: &mut c_int,
    bctype: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
) {
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            GetElementSide(
                *(*bound.wrapping_offset(*j as isize))
                    .parent
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .normal
                    .wrapping_offset(*i as isize),
                data,
                sideind,
                elemtype,
            );
            if *elemtype == 0 {
                printf(
                    c"could not find boundary element: %d %d %d\n".as_ptr(),
                    *i,
                    *j,
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                );
            }
            *elemdim = GetElementDimension(*elemtype);
            *bctype = *(*bound.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
            // NOTE: `if(0)` trace omitted (dead in C).
            (*(*mapbc.wrapping_offset(*bctype as isize))
                .wrapping_offset(*elemdim as isize)) += 1;
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: `if(0)` map dump omitted (dead in C).
}

/// SAFETY: ordered boundary index assignment (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn renumber_bc_assign(
    data: *mut FemType,
    minbc: c_int,
    maxbc: c_int,
    mapbc: *mut *mut c_int,
    j: &mut c_int,
    isordered: &mut c_int,
    elemdim: &mut c_int,
    i: &mut c_int,
    isname: &mut c_int,
    havename: &mut c_int,
) {
    *j = 0;
    // NOTE: larger dimensions first, named entities first.
    *isordered = TRUE;
    *isname = 1;
    while *isname >= 0 {
        *elemdim = 2;
        while *elemdim >= 0 {
            *i = minbc;
            while *i <= maxbc {
                if *(*mapbc.wrapping_offset(*i as isize))
                    .wrapping_offset(*elemdim as isize)
                    == 0
                {
                    *i += 1;
                    continue;
                }
                // NOTE: named entities take the lowest indexes.
                *havename = FALSE;
                if (*data).boundarynamesexist != 0
                    && *i < MAXBCS as c_int
                    && !(*data).boundaryname[*i as usize].is_null()
                {
                    *havename = TRUE;
                }
                // NOTE: C `break` exits the i-loop on a name mismatch.
                if *havename != *isname {
                    break;
                }
                *j += 1;
                if *i == *j {
                    if *isname != 0 {
                        printf(
                            c"BC index unaltered %d in %d %dD elements of %s\n".as_ptr(),
                            *i,
                            *(*mapbc.wrapping_offset(*i as isize))
                                .wrapping_offset(*elemdim as isize),
                            *elemdim,
                            (*data).boundaryname[*i as usize],
                        );
                    } else {
                        printf(
                            c"BC index unaltered %d in %d %dD elements\n".as_ptr(),
                            *i,
                            *(*mapbc.wrapping_offset(*i as isize))
                                .wrapping_offset(*elemdim as isize),
                            *elemdim,
                        );
                    }
                } else {
                    *isordered = FALSE;
                    if *isname != 0 {
                        printf(
                            c"BC index changed %d -> %d in %d %dD elements of %s\n".as_ptr(),
                            *i,
                            *j,
                            *(*mapbc.wrapping_offset(*i as isize))
                                .wrapping_offset(*elemdim as isize),
                            *elemdim,
                            (*data).boundaryname[*i as usize],
                        );
                    } else {
                        printf(
                            c"BC index changed %d -> %d in %d %dD elements\n".as_ptr(),
                            *i,
                            *j,
                            *(*mapbc.wrapping_offset(*i as isize))
                                .wrapping_offset(*elemdim as isize),
                            *elemdim,
                        );
                    }
                }
                *(*mapbc.wrapping_offset(*i as isize)).wrapping_offset(*elemdim as isize) =
                    *j;
                *i += 1;
            }
            *elemdim -= 1;
        }
        *isname -= 1;
    }
    // NOTE: `if(0)` map dump omitted (dead in C).
}
/// Renumber boundary types by dimension (named first), plus optional offset.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn RenumberBoundaryTypes(
    data: *mut FemType,
    bound: *mut BoundaryType,
    renumber: c_int,
    bcoffset: c_int,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut doinit: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut isordered: c_int = 0;
    // NOTE: `=0` decl inits kept (minbc/maxbc read before any overwrite).
    let mut minbc: c_int = 0;
    // NOTE: `=0` decl inits kept (minbc/maxbc read before any overwrite).
    let mut maxbc: c_int = 0;
    // NOTE: null-init (Imatrix before guarded reads).
    let mapbc: *mut *mut c_int;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut elemdim: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut elemtype: c_int = 0;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bctype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut havename: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut isname: c_int = 0;
    // NOTE: scratch name table for non-unique mappings.
    let mut boundaryname0: [*mut c_char; MAXBCS] = [core::ptr::null_mut(); MAXBCS];
    if renumber != 0 {
        // NOTE: `if(0)` trace omitted (dead in C).
        doinit = TRUE;
        j = 0;
        while j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(j as isize)).created == 0 {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                if doinit != 0 {
                    maxbc = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                    minbc = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                    doinit = FALSE;
                }
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxbc
                    < *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize)
                {
                    maxbc = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                }
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)
                    < minbc
                {
                    minbc = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                }
                i += 1;
            }
            j += 1;
        }
        if doinit != 0 {
            return;
        }
        if info != 0 {
            printf(
                c"Initial boundary interval [%d,%d]\n".as_ptr(),
                minbc,
                maxbc,
            );
        }
        mapbc = imatrix(minbc, maxbc, 0, 2);
        i = minbc;
        while i <= maxbc {
            j = 0;
            while j <= 2 {
                *(*mapbc.wrapping_offset(i as isize)).wrapping_offset(j as isize) = 0;
                j += 1;
            }
            i += 1;
        }
        // NOTE: count into the allocated map.
        renumber_bc_census(
            data, bound, mapbc, sideind.as_mut_ptr(), &mut elemdim, &mut elemtype,
            &mut bctype, &mut i, &mut j,
        );
        renumber_bc_assign(
            data, minbc, maxbc, mapbc, &mut j, &mut isordered, &mut elemdim, &mut i,
            &mut isname, &mut havename,
        );
        if isordered != 0 {
            if info != 0 {
                printf(c"Numbering of boundary types is already ok\n".as_ptr());
            }
        } else {
            if info != 0 {
                printf(
                    c"Mapping boundary types from [%d %d] to [%d %d]\n".as_ptr(),
                    minbc,
                    maxbc,
                    1,
                    j,
                );
            }
            j = 0;
            while j < MAXBOUNDARIES as c_int {
                if (*bound.wrapping_offset(j as isize)).created == 0 {
                    j += 1;
                    continue;
                }
                i = 1;
                while i <= (*bound.wrapping_offset(j as isize)).nosides {
                    GetElementSide(
                        *(*bound.wrapping_offset(j as isize))
                            .parent
                            .wrapping_offset(i as isize),
                        *(*bound.wrapping_offset(j as isize))
                            .side
                            .wrapping_offset(i as isize),
                        *(*bound.wrapping_offset(j as isize))
                            .normal
                            .wrapping_offset(i as isize),
                        data,
                        sideind.as_mut_ptr(),
                        &mut elemtype,
                    );
                    elemdim = GetElementDimension(elemtype);
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize) = *(*mapbc.wrapping_offset(
                        *(*bound.wrapping_offset(j as isize))
                            .types
                            .wrapping_offset(i as isize) as isize,
                    ))
                    .wrapping_offset(elemdim as isize);
                    i += 1;
                }
                j += 1;
            }
            if (*data).boundarynamesexist != 0 {
                j = 0;
                while j < MAXBODIES as c_int {
                    boundaryname0[j as usize] = core::ptr::null_mut();
                    j += 1;
                }
                // NOTE: stash names that will move.
                j = minbc;
                while j <= maxbc.min(MAXBCS as c_int - 1) {
                    k = 0;
                    elemdim = 2;
                    while elemdim >= 0 {
                        k = *(*mapbc.wrapping_offset(j as isize))
                            .wrapping_offset(elemdim as isize);
                        if k != 0 {
                            break;
                        }
                        elemdim -= 1;
                    }
                    if k != 0 && !(*data).boundaryname[j as usize].is_null() {
                        boundaryname0[j as usize] = cvector(0, MAXNAMESIZE as c_int);
                        strcpy(boundaryname0[j as usize], (*data).boundaryname[j as usize]);
                        free_cvector((*data).boundaryname[j as usize], 0, MAXNAMESIZE as c_int);
                        (*data).boundaryname[j as usize] = core::ptr::null_mut();
                    }
                    j += 1;
                }
                j = minbc;
                while j <= maxbc.min(MAXBCS as c_int - 1) {
                    k = 0;
                    elemdim = 2;
                    while elemdim >= 0 {
                        k = *(*mapbc.wrapping_offset(j as isize))
                            .wrapping_offset(elemdim as isize);
                        if k != 0 {
                            break;
                        }
                        elemdim -= 1;
                    }
                    if k != 0 && !boundaryname0[j as usize].is_null() {
                        if (*data).boundaryname[k as usize].is_null() {
                            (*data).boundaryname[k as usize] =
                                cvector(0, MAXNAMESIZE as c_int);
                        }
                        strcpy((*data).boundaryname[k as usize], boundaryname0[j as usize]);
                    }
                    j += 1;
                }
            }
        }
        free_imatrix(mapbc, minbc, maxbc, 0, 2);
    }
    if bcoffset != 0 {
        if info != 0 {
            printf(c"Adding offset of %d to the BCs\n".as_ptr(), bcoffset);
        }
        j = 0;
        while j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(j as isize)).created == 0 {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                (*(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize)) += bcoffset;
                i += 1;
            }
            j += 1;
        }
        if (*data).boundarynamesexist != 0 {
            j = MAXBCS as c_int - bcoffset - 1;
            while j >= 0 {
                k = j + bcoffset;
                if (*data).boundaryname[k as usize].is_null() {
                    (*data).boundaryname[k as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy((*data).boundaryname[k as usize], (*data).boundaryname[j as usize]);
                j -= 1;
            }
        }
    }
}
/// Compress body/material indexes to 1..N (following names).
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn RenumberMaterialTypes(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) {
    let _ = bound;
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut doinit: c_int;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut minmat: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut maxmat: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: `if(0)` trace omitted (dead in C).
    let noelements: c_int = (*data).noelements;
    if noelements < 1 {
        printf(c"There are no elements to set!\n".as_ptr());
        return;
    }
    doinit = TRUE;
    j = 1;
    while j <= noelements {
        if doinit != 0 {
            maxmat = *(*data).material.wrapping_offset(j as isize);
            minmat = *(*data).material.wrapping_offset(j as isize);
            doinit = FALSE;
        }
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxmat < *(*data).material.wrapping_offset(j as isize) {
            maxmat = *(*data).material.wrapping_offset(j as isize);
        }
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if *(*data).material.wrapping_offset(j as isize) < minmat {
            minmat = *(*data).material.wrapping_offset(j as isize);
        }
        j += 1;
    }
    if info != 0 {
        printf(
            c"Initial body interval [%d,%d]\n".as_ptr(),
            minmat,
            maxmat,
        );
    }
    let mapmat: *mut c_int = ivector(minmat, maxmat);
    i = minmat;
    while i <= maxmat {
        *mapmat.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    j = 1;
    while j <= noelements {
        (*mapmat.wrapping_offset(*(*data).material.wrapping_offset(j as isize) as isize)) += 1;
        j += 1;
    }
    j = 0;
    i = minmat;
    while i <= maxmat {
        if *mapmat.wrapping_offset(i as isize) != 0 {
            j += 1;
            if i != j {
                printf(
                    c"body index changed %d -> %d in %d elements\n".as_ptr(),
                    i,
                    j,
                    *mapmat.wrapping_offset(i as isize),
                );
            }
            *mapmat.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    if maxmat - minmat >= j || minmat != 1 {
        if info != 0 {
            printf(
                c"Mapping material types from [%d %d] to [%d %d]\n".as_ptr(),
                minmat,
                maxmat,
                1,
                j,
            );
        }
        j = 1;
        while j <= noelements {
            *(*data).material.wrapping_offset(j as isize) = *mapmat.wrapping_offset(
                *(*data).material.wrapping_offset(j as isize) as isize,
            );
            j += 1;
        }
        if (*data).bodynamesexist != 0 {
            if info != 0 {
                printf(c"Mapping entity names to follow material indexes\n".as_ptr());
            }
            j = minmat;
            while j <= maxmat.min(MAXBODIES as c_int - 1) {
                k = *mapmat.wrapping_offset(j as isize);
                if k != 0 && !(*data).bodyname[j as usize].is_null() {
                    if (*data).bodyname[k as usize].is_null() {
                        (*data).bodyname[k as usize] = cvector(0, MAXNAMESIZE as c_int);
                    }
                    strcpy((*data).bodyname[k as usize], (*data).bodyname[j as usize]);
                }
                j += 1;
            }
        }
    } else if info != 0 {
        printf(c"Numbering of bodies is already ok\n".as_ptr());
    }
    free_ivector(mapmat, minmat, maxmat);
    if info != 0 {
        printf(c"Renumbering of material types completed!\n".as_ptr());
    }
}

/// Drop boundary sides more than one dimension below the bulk minimum.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn RemoveLowerDimensionalBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemdim: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _parent: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _side: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nosides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnosides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newnosides: c_int;
    if info != 0 {
        printf(c"Removing lower dimensional boundaries\n".as_ptr());
    }
    let noelements: c_int = (*data).noelements;
    if noelements < 1 {
        return 1;
    }
    elemtype = GetMaxElementType(data);
    let maxelemdim: c_int = GetElementDimension(elemtype);
    if info != 0 {
        printf(
            c"Maximum elementtype is %d and dimension %d\n".as_ptr(),
            elemtype,
            maxelemdim,
        );
    }
    elemtype = GetMinElementType(data);
    let minelemdim: c_int = GetElementDimension(elemtype);
    if info != 0 {
        printf(
            c"Minimum elementtype is %d and dimension %d\n".as_ptr(),
            elemtype,
            minelemdim,
        );
    }
    // NOTE: 1D bulk keeps every boundary side.
    if minelemdim < 2 {
        return 2;
    }
    oldnosides = 0;
    newnosides = 0;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        nosides = 0;
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            oldnosides += 1;
            _parent = *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize);
            _side = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            GetBoundaryElement(
                i,
                bound.wrapping_offset(j as isize),
                data,
                sideind.as_mut_ptr(),
                &mut sideelemtype,
            );
            // NOTE: superseded GetElementSide call stays commented (dead in C).
            elemdim = GetElementDimension(sideelemtype);
            // NOTE: 1D BCs of hybrid 2D/3D meshes are maintained.
            if minelemdim - elemdim > 1 {
                i += 1;
                continue;
            }
            nosides += 1;
            if nosides == i {
                i += 1;
                continue;
            }
            *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(nosides as isize) =
                *(*bound.wrapping_offset(j as isize))
                    .parent
                    .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .parent2
                .wrapping_offset(nosides as isize) =
                *(*bound.wrapping_offset(j as isize))
                    .parent2
                    .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .side2
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .side2
                .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .types
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .types
                .wrapping_offset(i as isize);
            i += 1;
        }
        (*bound.wrapping_offset(j as isize)).nosides = nosides;
        newnosides += nosides;
        j += 1;
    }
    if info != 0 {
        printf(
            c"Removed %d (out of %d) less than %dD boundary elements\n".as_ptr(),
            oldnosides - newnosides,
            oldnosides,
            minelemdim - 1,
        );
    }
    0
}

/// Drop boundary sides with parents on both sides.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn RemoveInternalBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut parent: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut parent2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nosides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnosides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newnosides: c_int;
    if info != 0 {
        printf(c"Removing internal boundaries\n".as_ptr());
    }
    if (*data).noelements < 1 {
        return 1;
    }
    oldnosides = 0;
    newnosides = 0;
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        nosides = 0;
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            oldnosides += 1;
            parent = *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize);
            parent2 = *(*bound.wrapping_offset(j as isize))
                .parent2
                .wrapping_offset(i as isize);
            if parent > 0 && parent2 > 0 {
                i += 1;
                continue;
            }
            nosides += 1;
            if nosides == i {
                i += 1;
                continue;
            }
            *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(nosides as isize) =
                *(*bound.wrapping_offset(j as isize))
                    .parent
                    .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .parent2
                .wrapping_offset(nosides as isize) =
                *(*bound.wrapping_offset(j as isize))
                    .parent2
                    .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .side2
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .side2
                .wrapping_offset(i as isize);
            *(*bound.wrapping_offset(j as isize))
                .types
                .wrapping_offset(nosides as isize) = *(*bound.wrapping_offset(j as isize))
                .types
                .wrapping_offset(i as isize);
            i += 1;
        }
        (*bound.wrapping_offset(j as isize)).nosides = nosides;
        newnosides += nosides;
        j += 1;
    }
    if info != 0 {
        printf(
            c"Removed %d (out of %d) internal boundary elements\n".as_ptr(),
            oldnosides - newnosides,
            oldnosides,
        );
    }
    0
}
// NOTE: C `#if 0` FindEdges omitted entirely (preprocessor-dead in C).

/// SAFETY: side-index set comparison (pure scan).
unsafe fn CompareIndexes(elemtype: c_int, ind1: *mut c_int, ind2: *mut c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: `=0` decl init kept (read before any overwrite).
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut hits: c_int;
    hits = 0;
    let nosides: c_int = elemtype / 100;
    i = 0;
    while i < nosides {
        j = 0;
        while j < nosides {
            if *ind1.wrapping_offset(i as isize) == *ind2.wrapping_offset(j as isize) {
                hits += 1;
            }
            j += 1;
        }
        i += 1;
    }
    let same: c_int = (hits == nosides) as c_int;
    same
}

/// SAFETY: side-index range for a bulk type/dim pair (split for sizing).
unsafe fn newbound_edge_range(
    elemtype: c_int,
    dim: c_int,
    lowerdim: c_int,
    minedge: &mut c_int,
    maxedge: &mut c_int,
) {
    if dim == 1 {
        *minedge = 0;
        *maxedge = elemtype / 100 - 1;
    } else if dim == 2 {
        if lowerdim == 1 {
            *minedge = 0;
            *maxedge = elemtype / 100 - 1;
        } else if lowerdim == 0 {
            *minedge = elemtype / 100;
            *maxedge = *minedge + 1;
        }
    } else if dim == 3 {
        if lowerdim == 2 {
            *minedge = 0;
            if elemtype / 100 == 5 {
                *maxedge = 3;
            } else if elemtype / 100 == 6 || elemtype / 100 == 7 {
                *maxedge = 4;
            } else if elemtype / 100 == 8 {
                *maxedge = 5;
            }
        } else if lowerdim == 1 {
            if elemtype / 100 == 8 {
                *minedge = 6;
                *maxedge = 17;
            } else if elemtype / 100 == 5 {
                *minedge = 4;
                *maxedge = 9;
            } else {
                printf(c"FindNewBoundaries: not implemented for all 3d boundaries\n".as_ptr());
            }
        } else if lowerdim == 0 && elemtype / 100 == 8 {
            *minedge = 18;
            *maxedge = 25;
        }
    }
}
/// Boundary sides whose nodes are all flagged in `boundnodes`.
///
/// # Safety
/// `data`, `bound`, `boundnodes` valid.
#[no_mangle]
pub unsafe extern "C" fn FindNewBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    boundnodes: *mut c_int,
    suggesttype: c_int,
    dimred: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut side: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut identical: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut element: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut lowerdim: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minedge: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxedge: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nosides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newbound: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind0: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype0: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut noboundnodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sameside: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    newtype = 0;
    allocated = FALSE;
    let dim: c_int = (*data).dim;
    if dimred != 0 {
        lowerdim = dim - dimred;
    } else {
        lowerdim = dim - 1;
    }
    let noknots: c_int = (*data).noknots;
    let noelements: c_int = (*data).noelements;
    noboundnodes = 0;
    newbound = 0;
    maxedge = 0;
    minedge = 0;
    i = 1;
    while i <= noknots {
        if *boundnodes.wrapping_offset(i as isize) != 0 {
            noboundnodes += 1;
        }
        i += 1;
    }
    if noboundnodes == 0 {
        printf(c"FindNewBoundaries: no nonzero entries in boundnodes vector!\n".as_ptr());
        return 1;
    } else if info != 0 {
        printf(
            c"There are %d nonzero entries in boundnodes vector!\n".as_ptr(),
            noboundnodes,
        );
    }
    // NOTE: C `omstart:` label + `goto omstart`; retry on lowerdim.
    loop {
        nosides = 0;
        element = 1;
        while element <= noelements {
            elemtype = *(*data).elementtypes.wrapping_offset(element as isize);
            newbound_edge_range(elemtype, dim, lowerdim, &mut minedge, &mut maxedge);
            side = minedge;
            while side <= maxedge {
                GetElementSide(
                    element,
                    side,
                    1,
                    data,
                    sideind.as_mut_ptr(),
                    &mut sideelemtype,
                );
                nonodes = sideelemtype % 100;
                identical = TRUE;
                i = 0;
                while i < nonodes {
                    if *boundnodes.wrapping_offset(
                        sideind[i as usize] as isize,
                    ) == 0
                    {
                        identical = FALSE;
                    }
                    i += 1;
                }
                if identical == 0 {
                    side += 1;
                    continue;
                }
                nosides += 1;
                if allocated != 0 {
                    // NOTE: merge with an identical recorded side.
                    // NOTE: fresh flag per side (C stale-sameside falls to record).
                    let mut foundsame: c_int = 0;
                    i = 1;
                    while i < nosides {
                        if *(*bound.wrapping_offset(newbound as isize))
                            .parent2
                            .wrapping_offset(i as isize)
                            != 0
                        {
                            i += 1;
                            continue;
                        }
                        GetElementSide(
                            *(*bound.wrapping_offset(newbound as isize))
                                .parent
                                .wrapping_offset(i as isize),
                            *(*bound.wrapping_offset(newbound as isize))
                                .side
                                .wrapping_offset(i as isize),
                            1,
                            data,
                            sideind0.as_mut_ptr(),
                            &mut sideelemtype0,
                        );
                        if sideelemtype0 != sideelemtype {
                            i += 1;
                            continue;
                        }
                        sameside = CompareIndexes(
                            sideelemtype,
                            sideind0.as_mut_ptr(),
                            sideind.as_mut_ptr(),
                        );
                        if sameside != 0 {
                            *(*bound.wrapping_offset(newbound as isize))
                                .parent2
                                .wrapping_offset(i as isize) = element;
                            *(*bound.wrapping_offset(newbound as isize))
                                .side2
                                .wrapping_offset(i as isize) = side;
                            nosides -= 1;
                            // NOTE: C `goto foundsameside` skips recording.
                            foundsame = TRUE;
                            break;
                        }
                        i += 1;
                    }
                    // NOTE: C falls through to `foundsameside: continue`.
                    if foundsame != 0 {
                        side += 1;
                        continue;
                    }
                    *(*bound.wrapping_offset(newbound as isize))
                        .parent
                        .wrapping_offset(nosides as isize) = element;
                    *(*bound.wrapping_offset(newbound as isize))
                        .side
                        .wrapping_offset(nosides as isize) = side;
                    // NOTE: C writes types twice; single verbatim write kept.
                    *(*bound.wrapping_offset(newbound as isize))
                        .types
                        .wrapping_offset(nosides as isize) = newtype;
                }
                side += 1;
            }
            element += 1;
        }
        if nosides != 0 {
            if allocated == 0 {
                newtype = suggesttype;
                j = 0;
                while j < MAXBOUNDARIES as c_int
                    && (*bound.wrapping_offset(j as isize)).created != 0
                {
                    newbound = j;
                    if suggesttype != 0 {
                        j += 1;
                        continue;
                    }
                    i = 1;
                    while i <= (*bound.wrapping_offset(j as isize)).nosides {
                        // NOTE: C MAX macro (egdef.h:97); arms pure.
                        if newtype
                            < *(*bound.wrapping_offset(j as isize))
                                .types
                                .wrapping_offset(i as isize)
                        {
                            newtype = *(*bound.wrapping_offset(j as isize))
                                .types
                                .wrapping_offset(i as isize);
                        }
                        i += 1;
                    }
                    j += 1;
                }
                newbound += 1;
                if suggesttype == 0 {
                    newtype += 1;
                }
                AllocateBoundary(bound.wrapping_offset(newbound as isize), nosides);
                allocated = TRUE;
                if info != 0 {
                    printf(
                        c"Allocating for %d sides of boundary %d\n".as_ptr(),
                        nosides,
                        newtype,
                    );
                }
                continue;
            }
            (*bound.wrapping_offset(newbound as isize)).nosides = nosides;
            if info != 0 {
                printf(
                    c"Found %d sides of dim %d to define boundary %d\n".as_ptr(),
                    nosides,
                    lowerdim,
                    newtype,
                );
            }
            i = 1;
            while i <= nosides {
                // NOTE: C `if(j = parent2[i])` assigns then tests.
                j = *(*bound.wrapping_offset(newbound as isize))
                    .parent2
                    .wrapping_offset(i as isize);
                if j != 0
                    && *(*bound.wrapping_offset(newbound as isize))
                        .parent
                        .wrapping_offset(i as isize)
                        > *(*bound.wrapping_offset(newbound as isize))
                            .parent2
                            .wrapping_offset(i as isize)
                {
                    *(*bound.wrapping_offset(newbound as isize))
                        .parent2
                        .wrapping_offset(i as isize) = *(*bound.wrapping_offset(newbound as isize))
                        .parent
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(newbound as isize))
                        .parent
                        .wrapping_offset(i as isize) = j;
                    j = *(*bound.wrapping_offset(newbound as isize))
                        .side2
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(newbound as isize))
                        .side2
                        .wrapping_offset(i as isize) = *(*bound.wrapping_offset(newbound as isize))
                        .side
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(newbound as isize))
                        .side
                        .wrapping_offset(i as isize) = j;
                }
                i += 1;
            }
            break;
        } else if lowerdim == 0 {
            printf(c"The nodes do not form a boundary!\n".as_ptr());
            return 2;
        } else {
            lowerdim -= 1;
            printf(
                c"The nodes do not form a boundary, trying with %d-dimensional elements.\n".as_ptr(),
                lowerdim,
            );
        }
    }
    0
}
/// SAFETY: bulk material visit counts + extrema (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn bulk_visit(
    data: *mut FemType,
    visited: *mut c_int,
    boundnodes: *mut c_int,
    mat1: c_int,
    elemdim: &mut c_int,
    maxnodes: &mut c_int,
    minnodes: &mut c_int,
    nonodes: &mut c_int,
    material: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    *i = 1;
    while *i <= (*data).noknots {
        *visited.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *i = 1;
    while *i <= (*data).noknots {
        *boundnodes.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *elemdim = 0;
    *i = 1;
    while *i <= (*data).noelements {
        *material = *(*data).material.wrapping_offset(*i as isize);
        if *material == mat1 {
            *nonodes = *(*data).elementtypes.wrapping_offset(*i as isize) % 100;
            *k = *(*data).elementtypes.wrapping_offset(*i as isize) / 100;
            if *k > *elemdim {
                *elemdim = *k;
            }
            *j = 0;
            while *j < *nonodes {
                *k = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                (*visited.wrapping_offset(*k as isize)) += 1;
                *j += 1;
            }
        }
        *i += 1;
    }
    *maxnodes = *visited.wrapping_offset(1);
    *minnodes = *visited.wrapping_offset(1);
    *i = 1;
    while *i <= (*data).noknots {
        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
        if *maxnodes < *visited.wrapping_offset(*i as isize) {
            *maxnodes = *visited.wrapping_offset(*i as isize);
        }
        if *visited.wrapping_offset(*i as isize) < *minnodes {
            *minnodes = *visited.wrapping_offset(*i as isize);
        }
        *i += 1;
    }
}

/// SAFETY: dihedral-angle filter for 2D bulk in 3D (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn bulk_angles(
    data: *mut FemType,
    visited: *mut c_int,
    anglesum: *mut Real,
    mat1: c_int,
    maxnodes: c_int,
    nonodes: &mut c_int,
    material: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dotprod: Real;
    let _ = maxnodes;
    *i = 1;
    while *i <= (*data).noknots {
        *anglesum.wrapping_offset(*i as isize) = 0.0;
        *i += 1;
    }
    *i = 1;
    while *i <= (*data).noelements {
        *material = *(*data).material.wrapping_offset(*i as isize);
        if *material == mat1 {
            *nonodes = *(*data).elementtypes.wrapping_offset(*i as isize) / 100;
            // NOTE: row pointer for the corner fan (C `ind`).
            let row: *mut c_int = *(*data).topology.wrapping_offset(*i as isize);
            if *nonodes == 3 || *nonodes == 4 {
                *k = 0;
                while *k < *nonodes {
                    dx1 = *(*data).x.wrapping_offset(
                        *row.wrapping_offset(((*k + 1) % *nonodes) as isize) as isize,
                    ) - *(*data).x.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    dy1 = *(*data).y.wrapping_offset(
                        *row.wrapping_offset(((*k + 1) % *nonodes) as isize) as isize,
                    ) - *(*data).y.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    dz1 = *(*data).z.wrapping_offset(
                        *row.wrapping_offset(((*k + 1) % *nonodes) as isize) as isize,
                    ) - *(*data).z.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    dx2 = *(*data).x.wrapping_offset(
                        *row.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                            as isize,
                    ) - *(*data).x.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    dy2 = *(*data).y.wrapping_offset(
                        *row.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                            as isize,
                    ) - *(*data).y.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    dz2 = *(*data).z.wrapping_offset(
                        *row.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                            as isize,
                    ) - *(*data).z.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    );
                    ds1 = sqrt(dx1 * dx1 + dy1 * dy1 + dz1 * dz1);
                    ds2 = sqrt(dx2 * dx2 + dy2 * dy2 + dz2 * dz2);
                    dotprod = dx1 * dx2 + dy1 * dy2 + dz1 * dz2;
                    *anglesum.wrapping_offset(
                        *row.wrapping_offset(*k as isize) as isize,
                    ) += acos(dotprod / (ds1 * ds2));
                    *k += 1;
                }
            }
        }
        *i += 1;
    }
    *j = 0;
    *i = 1;
    while *i <= (*data).noknots {
        *anglesum.wrapping_offset(*i as isize) /= 2.0 * FM_PI;
        if *anglesum.wrapping_offset(*i as isize) > 0.99 {
            *visited.wrapping_offset(*i as isize) = 0;
        }
        if *anglesum.wrapping_offset(*i as isize) > 1.01 {
            printf(
                c"FindBulkBoundary: surprisingly large angle %.3e in node %d\n".as_ptr(),
                *anglesum.wrapping_offset(*i as isize),
                *i,
            );
        }
        if *visited.wrapping_offset(*i as isize) != 0 {
            *j += 1;
        }
        *i += 1;
    }
    // NOTE: `if(0)` candidate print omitted (dead in C).
}
/// SAFETY: bulk mat2 option arms (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn bulk_options(
    data: *mut FemType,
    visited: *mut c_int,
    boundnodes: *mut c_int,
    mat1: c_int,
    mat2: c_int,
    maxnodes: c_int,
    eps: &mut Real,
    nonodes: &mut c_int,
    material: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) -> bool {
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut xmin: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut xmax: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut ymin: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut ymax: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut zmin: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: `=0.0` init unobservable (set on first visited knot).
    let mut zmax: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds: Real;
    // NOTE: C switch over the mat2 option.
    if mat2 == 0 {
        *i = 1;
        while *i <= (*data).noelements {
            *material = *(*data).material.wrapping_offset(*i as isize);
            if *material == mat1 {
                *i += 1;
                continue;
            }
            *nonodes = *(*data).elementtypes.wrapping_offset(*i as isize) % 100;
            *j = 0;
            while *j < *nonodes {
                *k = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                (*boundnodes.wrapping_offset(*k as isize)) += 1;
                *j += 1;
            }
            *i += 1;
        }
        *k = 1;
        while *k <= (*data).noknots {
            // NOTE: C spells the =0 arms separately; merged (identical bodies).
            if *visited.wrapping_offset(*k as isize) != 0
                && *visited.wrapping_offset(*k as isize)
                    >= *boundnodes.wrapping_offset(*k as isize)
                && *visited.wrapping_offset(*k as isize)
                    + *boundnodes.wrapping_offset(*k as isize)
                    < maxnodes
            {
                *boundnodes.wrapping_offset(*k as isize) = 1;
            } else {
                *boundnodes.wrapping_offset(*k as isize) = 0;
            }
            *k += 1;
        }
    } else if mat2 == -10 {
        *i = 1;
        while *i <= (*data).noknots {
            if *visited.wrapping_offset(*i as isize) != 0 {
                *boundnodes.wrapping_offset(*i as isize) = 1;
            }
            *i += 1;
        }
    } else if mat2 == -11 || mat2 == -12 || mat2 > 0 {
        *i = 1;
        while *i <= (*data).noelements {
            *material = *(*data).material.wrapping_offset(*i as isize);
            if *material == mat1 {
                *i += 1;
                continue;
            }
            if mat2 > 0 && *material != mat2 {
                *i += 1;
                continue;
            }
            if mat2 == -11 && *material < mat1 {
                *i += 1;
                continue;
            }
            if mat2 == -12 && *material > mat1 {
                *i += 1;
                continue;
            }
            *nonodes = *(*data).elementtypes.wrapping_offset(*i as isize) % 100;
            *j = 0;
            while *j < *nonodes {
                *k = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                if *visited.wrapping_offset(*k as isize) != 0 {
                    *boundnodes.wrapping_offset(*k as isize) = 1;
                }
                *j += 1;
            }
            *i += 1;
        }
    } else if mat2 >= -2 * (*data).dim && mat2 <= -1 {
        *j = TRUE;
        *i = 1;
        while *i <= (*data).noknots {
            if *visited.wrapping_offset(*i as isize) != 0 {
                if *j != 0 {
                    xmax = *(*data).x.wrapping_offset(*i as isize);
                    xmin = *(*data).x.wrapping_offset(*i as isize);
                    ymax = *(*data).y.wrapping_offset(*i as isize);
                    ymin = *(*data).y.wrapping_offset(*i as isize);
                    zmax = *(*data).z.wrapping_offset(*i as isize);
                    zmin = *(*data).z.wrapping_offset(*i as isize);
                    *j = FALSE;
                } else {
                    // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
                    if xmax < *(*data).x.wrapping_offset(*i as isize) {
                        xmax = *(*data).x.wrapping_offset(*i as isize);
                    }
                    if *(*data).x.wrapping_offset(*i as isize) < xmin {
                        xmin = *(*data).x.wrapping_offset(*i as isize);
                    }
                    if ymax < *(*data).y.wrapping_offset(*i as isize) {
                        ymax = *(*data).y.wrapping_offset(*i as isize);
                    }
                    if *(*data).y.wrapping_offset(*i as isize) < ymin {
                        ymin = *(*data).y.wrapping_offset(*i as isize);
                    }
                    if zmax < *(*data).z.wrapping_offset(*i as isize) {
                        zmax = *(*data).z.wrapping_offset(*i as isize);
                    }
                    if *(*data).z.wrapping_offset(*i as isize) < zmin {
                        zmin = *(*data).z.wrapping_offset(*i as isize);
                    }
                }
            }
            *i += 1;
        }
        ds = (xmax - xmin) * (xmax - xmin)
            + (ymax - ymin) * (ymax - ymin)
            + (zmax - zmin) * (zmax - zmin);
        ds = sqrt(ds);
        *eps = 1.0e-5 * ds;
        *i = 1;
        while *i <= (*data).noknots {
            if *visited.wrapping_offset(*i as isize) < maxnodes
                && *visited.wrapping_offset(*i as isize) != 0
            {
                if (*data).dim == 1
                    && (mat2 == -1 && fabs(*(*data).x.wrapping_offset(*i as isize) - xmin) < *eps
                        || mat2 == -2
                            && fabs(*(*data).x.wrapping_offset(*i as isize) - xmax) < *eps)
                {
                    // NOTE: C spells one arm per side; merged (identical bodies).
                    *boundnodes.wrapping_offset(*i as isize) = 1;
                }
                if (*data).dim >= 2
                    && (mat2 == -1
                        && fabs(*(*data).y.wrapping_offset(*i as isize) - ymin) < *eps
                        || mat2 == -3
                            && fabs(*(*data).y.wrapping_offset(*i as isize) - ymax) < *eps
                        || mat2 == -4
                            && fabs(*(*data).x.wrapping_offset(*i as isize) - xmin) < *eps
                        || mat2 == -2
                            && fabs(*(*data).x.wrapping_offset(*i as isize) - xmax) < *eps)
                {
                    // NOTE: C spells one arm per side; merged (identical bodies).
                    *boundnodes.wrapping_offset(*i as isize) = 1;
                }
                if (*data).dim >= 3
                    && (mat2 == -5
                        && fabs(*(*data).z.wrapping_offset(*i as isize) - zmin) < *eps
                        || mat2 == -6
                            && fabs(*(*data).z.wrapping_offset(*i as isize) - zmax) < *eps)
                {
                    // NOTE: C spells one arm per side; merged (identical bodies).
                    *boundnodes.wrapping_offset(*i as isize) = 1;
                }
            }
            *i += 1;
        }
    } else {
        printf(
            c"FindBulkBoundary: unknown option %d for finding a side\n".as_ptr(),
            mat2,
        );
        return false;
    }
    true
}

/// Flagged nodes between bulk materials (angle/box/material options).
///
/// # Safety
/// `data`, `boundnodes`, `noboundnodes` valid.
#[no_mangle]
pub unsafe extern "C" fn FindBulkBoundary(
    data: *mut FemType,
    mut mat1: c_int,
    mut mat2: c_int,
    boundnodes: *mut c_int,
    noboundnodes: *mut c_int,
    info: c_int,
) -> c_int {
    // NOTE: 0-init for the &mut handoff (set in bulk_* before reads.)
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_* before reads.)
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut maxnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut minnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut material: c_int = 0;
    // NOTE: `=1.0e-4` decl init kept (read before any overwrite).
    let mut eps: Real;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: 0-init for the &mut handoff (set in bulk_visit before reads).
    let mut elemdim: c_int = 0;
    // NOTE: null-init (Rvector before guarded reads).
    let anglesum: *mut Real;
    eps = 1.0e-4;
    *noboundnodes = 0;
    if mat1 < 1 && mat2 < 1 {
        printf(c"FindBulkBoundary: Either of the materials must be positive\n".as_ptr());
        return 1;
    } else if mat1 < 1 {
        i = mat1;
        mat1 = mat2;
        mat2 = i;
    }
    if info != 0 {
        printf(
            c"Finding nodes between bulk elements of material %d and %d\n".as_ptr(),
            mat1,
            mat2,
        );
    }
    let visited: *mut c_int = ivector(1, (*data).noknots);
    bulk_visit(
        data, visited, boundnodes, mat1, &mut elemdim, &mut maxnodes, &mut minnodes,
        &mut nonodes, &mut material, &mut i, &mut j, &mut k,
    );
    if elemdim == 3 || elemdim == 4 {
        anglesum = dvector(1, (*data).noknots);
        bulk_angles(
            data, visited, anglesum, mat1, maxnodes, &mut nonodes, &mut material,
            &mut i, &mut j, &mut k,
        );
        free_dvector(anglesum, 1, (*data).noknots);
    } else {
        i = 1;
        while i <= (*data).noknots {
            if *visited.wrapping_offset(i as isize) == maxnodes {
                *visited.wrapping_offset(i as isize) = 0;
            }
            i += 1;
        }
        if maxnodes < 2 {
            printf(
                c"FindBulkBoundary: Nodes must belong to more than %d elements.\n".as_ptr(),
                maxnodes,
            );
            return 2;
        }
    }
    if !bulk_options(
        data, visited, boundnodes, mat1, mat2, maxnodes, &mut eps, &mut nonodes,
        &mut material, &mut i, &mut j, &mut k,
    ) {
        return 2;
    }
    *noboundnodes = 0;
    i = 1;
    while i <= (*data).noknots {
        if *boundnodes.wrapping_offset(i as isize) != 0 {
            *noboundnodes += 1;
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"Located %d nodes at the interval between materials %d and %d\n".as_ptr(),
            *noboundnodes,
            mat1,
            mat2,
        );
    }
    free_ivector(visited, 1, (*data).noknots);
    0
}
/// SAFETY: boundary visit counts + corner angles (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn boun_visit(
    data: *mut FemType,
    bound: *mut BoundaryType,
    visited: *mut c_int,
    boundnodes: *mut c_int,
    anglesum: &mut *mut Real,
    sideind: *mut c_int,
    mat1: c_int,
    bounddim: &mut c_int,
    maxnodes: &mut c_int,
    minnodes: &mut c_int,
    nonodes: &mut c_int,
    nocorners: &mut c_int,
    elemtype: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds1: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dotprod: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut jj: c_int;
    *i = 1;
    while *i <= (*data).noknots {
        *visited.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *i = 1;
    while *i <= (*data).noknots {
        *boundnodes.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *bounddim = 0;
    // NOTE: tag knots of the other boundary.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            if *(*bound.wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize)
                == mat1
            {
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    sideind,
                    elemtype,
                );
                *nonodes = *elemtype % 100;
                *nocorners = *elemtype / 100;
                *k = 0;
                while *k < *nocorners {
                    (*visited.wrapping_offset(
                        *sideind.wrapping_offset(*k as isize) as isize,
                    )) += 1;
                    *k += 1;
                }
                *k = *nocorners;
                while *k < *nonodes {
                    (*visited.wrapping_offset(
                        *sideind.wrapping_offset(*k as isize) as isize,
                    )) -= 1;
                    *k += 1;
                }
                if *nocorners == 3 || *nocorners == 4 {
                    if *bounddim < 2 {
                        *anglesum = dvector(1, (*data).noknots);
                        *k = 1;
                        while *k <= (*data).noknots {
                            *(*anglesum).wrapping_offset(*k as isize) = 0.0;
                            *k += 1;
                        }
                        *bounddim = 2;
                    }
                    *nonodes = *nocorners;
                    *k = 0;
                    while *k < *nonodes {
                        dx1 = *(*data).x.wrapping_offset(
                            *sideind.wrapping_offset(((*k + 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).x.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        dy1 = *(*data).y.wrapping_offset(
                            *sideind.wrapping_offset(((*k + 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).y.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        dz1 = *(*data).z.wrapping_offset(
                            *sideind.wrapping_offset(((*k + 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).z.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        dx2 = *(*data).x.wrapping_offset(
                            *sideind.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).x.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        dy2 = *(*data).y.wrapping_offset(
                            *sideind.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).y.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        dz2 = *(*data).z.wrapping_offset(
                            *sideind.wrapping_offset(((*k + *nonodes - 1) % *nonodes) as isize)
                                as isize,
                        ) - *(*data).z.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        );
                        ds1 = sqrt(dx1 * dx1 + dy1 * dy1 + dz1 * dz1);
                        ds2 = sqrt(dx2 * dx2 + dy2 * dy2 + dz2 * dz2);
                        dotprod = dx1 * dx2 + dy1 * dy2 + dz1 * dz2;
                        *(*anglesum).wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        ) += acos(dotprod / (ds1 * ds2));
                        *k += 1;
                    }
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    *maxnodes = abs(*visited.wrapping_offset(1));
    *minnodes = abs(*visited.wrapping_offset(1));
    *i = 1;
    while *i <= (*data).noknots {
        jj = abs(*visited.wrapping_offset(*i as isize));
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxnodes < jj {
            *maxnodes = jj;
        }
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if jj < *minnodes {
            *minnodes = jj;
        }
        *i += 1;
    }
}
/// SAFETY: boundary mat2 option arms (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn boun_options(
    data: *mut FemType,
    bound: *mut BoundaryType,
    visited: *mut c_int,
    boundnodes: *mut c_int,
    elemind: *mut c_int,
    sideind: *mut c_int,
    mat1: c_int,
    mat2: c_int,
    maxnodes: c_int,
    eps: &mut Real,
    hits: &mut c_int,
    nonodes: &mut c_int,
    material: &mut c_int,
    elemtype: &mut c_int,
    l: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) -> bool {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ds: Real;
    // NOTE: C switch over the mat2 option.
    if mat2 == 0 {
        *k = 1;
        while *k <= (*data).noknots {
            if *visited.wrapping_offset(*k as isize) != 0 {
                *boundnodes.wrapping_offset(*k as isize) = 1;
            }
            *k += 1;
        }
    } else if mat2 == -11 || mat2 == -12 || mat2 == -10 || mat2 > 0 {
        *j = 0;
        while *j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(*j as isize)).created == 0 {
                *j += 1;
                continue;
            }
            *i = 1;
            while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
                *material = *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize);
                if *material == mat1 {
                    *i += 1;
                    continue;
                }
                if mat2 > 0 && *material != mat2 {
                    *i += 1;
                    continue;
                }
                if mat2 == -11 && *material < mat1 {
                    *i += 1;
                    continue;
                }
                if mat2 == -12 && *material > mat1 {
                    *i += 1;
                    continue;
                }
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    sideind,
                    elemtype,
                );
                *nonodes = *elemtype % 100;
                *k = 0;
                while *k < *nonodes {
                    *l = *sideind.wrapping_offset(*k as isize);
                    if *visited.wrapping_offset(*l as isize) != 0 {
                        *boundnodes.wrapping_offset(*l as isize) = 1;
                    }
                    *k += 1;
                }
                *i += 1;
            }
            *j += 1;
        }
    } else if mat2 >= -2 * (*data).dim && mat2 <= -1 {
        *j = 0;
        while *j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(*j as isize)).created == 0 {
                *j += 1;
                continue;
            }
            *i = 1;
            while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
                *material = *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize);
                if *material != mat1 {
                    *i += 1;
                    continue;
                }
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    sideind,
                    elemtype,
                );
                *nonodes = *elemtype % 100;
                *hits = 0;
                *k = 0;
                while *k < *nonodes {
                    *l = *sideind.wrapping_offset(*k as isize);
                    if *visited.wrapping_offset(*l as isize) < maxnodes {
                        *hits += 1;
                    }
                    *k += 1;
                }
                if *hits == 0 {
                    *i += 1;
                    continue;
                }
                *l = *sideind.wrapping_offset(0);
                xmax = *(*data).x.wrapping_offset(*l as isize);
                xmin = *(*data).x.wrapping_offset(*l as isize);
                ymax = *(*data).y.wrapping_offset(*l as isize);
                ymin = *(*data).y.wrapping_offset(*l as isize);
                zmax = *(*data).z.wrapping_offset(*l as isize);
                zmin = *(*data).z.wrapping_offset(*l as isize);
                *k = 1;
                while *k < *nonodes {
                    *l = *sideind.wrapping_offset(*k as isize);
                    // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
                    if xmax < *(*data).x.wrapping_offset(*l as isize) {
                        xmax = *(*data).x.wrapping_offset(*l as isize);
                    }
                    if *(*data).x.wrapping_offset(*l as isize) < xmin {
                        xmin = *(*data).x.wrapping_offset(*l as isize);
                    }
                    if ymax < *(*data).y.wrapping_offset(*l as isize) {
                        ymax = *(*data).y.wrapping_offset(*l as isize);
                    }
                    if *(*data).y.wrapping_offset(*l as isize) < ymin {
                        ymin = *(*data).y.wrapping_offset(*l as isize);
                    }
                    if zmax < *(*data).z.wrapping_offset(*l as isize) {
                        zmax = *(*data).z.wrapping_offset(*l as isize);
                    }
                    if *(*data).z.wrapping_offset(*l as isize) < zmin {
                        zmin = *(*data).z.wrapping_offset(*l as isize);
                    }
                    *k += 1;
                }
                ds = (xmax - xmin) * (xmax - xmin)
                    + (ymax - ymin) * (ymax - ymin)
                    + (zmax - zmin) * (zmax - zmin);
                ds = sqrt(ds);
                *eps = 1.0e-3 * ds;
                *k = 0;
                while *k < *nonodes {
                    *elemind.wrapping_offset(*k as isize) = 0;
                    *l = *sideind.wrapping_offset(*k as isize);
                    if *visited.wrapping_offset(*l as isize) == 0 {
                        *k += 1;
                        continue;
                    }
                    if (*data).dim == 1
                        && (mat2 == -1
                            && fabs(*(*data).x.wrapping_offset(*l as isize) - xmin) < *eps
                            || mat2 == -2
                                && fabs(*(*data).x.wrapping_offset(*l as isize) - xmax)
                                    < *eps)
                    {
                        // NOTE: C spells one arm per side; merged (identical bodies).
                        *boundnodes.wrapping_offset(*l as isize) = 1;
                    }
                    if (*data).dim >= 2
                        && (mat2 == -1
                            && fabs(*(*data).y.wrapping_offset(*l as isize) - ymin) < *eps
                            || mat2 == -3
                                && fabs(*(*data).y.wrapping_offset(*l as isize) - ymax)
                                    < *eps
                            || mat2 == -4
                                && fabs(*(*data).x.wrapping_offset(*l as isize) - xmin)
                                    < *eps
                            || mat2 == -2
                                && fabs(*(*data).x.wrapping_offset(*l as isize) - xmax)
                                    < *eps)
                    {
                        // NOTE: C spells one arm per side; merged (identical bodies).
                        *elemind.wrapping_offset(*l as isize) = 1;
                    }
                    if (*data).dim >= 3
                        && (mat2 == -5
                            && fabs(*(*data).z.wrapping_offset(*l as isize) - zmin) < *eps
                            || mat2 == -6
                                && fabs(*(*data).z.wrapping_offset(*l as isize) - zmax)
                                    < *eps)
                    {
                        // NOTE: C spells one arm per side; merged (identical bodies).
                        *elemind.wrapping_offset(*l as isize) = 1;
                    }
                    *k += 1;
                }
                if (*data).dim > 1 {
                    *hits = 0;
                    *k = 0;
                    while *k < *nonodes {
                        *hits += *elemind.wrapping_offset(*k as isize);
                        *k += 1;
                    }
                    if *hits > 1 {
                        *k = 0;
                        while *k < *nonodes {
                            if *elemind.wrapping_offset(*k as isize) != 0 {
                                *boundnodes.wrapping_offset(
                                    *sideind.wrapping_offset(*k as isize) as isize,
                                ) = 1;
                            }
                            *k += 1;
                        }
                    }
                }
                *i += 1;
            }
            *j += 1;
        }
    } else {
        printf(
            c"FindBoundaryBoundary: unknown option %d for finding a side\n".as_ptr(),
            mat2,
        );
        return false;
    }
    true
}
/// Flagged nodes between boundary element sets (angle/box/material options).
///
/// # Safety
/// `data`, `bound`, `boundnodes`, `noboundnodes` valid.
#[no_mangle]
pub unsafe extern "C" fn FindBoundaryBoundary(
    data: *mut FemType,
    bound: *mut BoundaryType,
    mut mat1: c_int,
    mut mat2: c_int,
    boundnodes: *mut c_int,
    noboundnodes: *mut c_int,
    info: c_int,
) -> c_int {
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads.)
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads.)
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut hits: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut nocorners: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut maxnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut minnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in boun_visit before reads).
    let mut material: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bounddim: c_int = 0;
    // NOTE: `=1.0e-4` decl init kept (read before any overwrite).
    let mut eps: Real;
    // NOTE: null-init (Rvector before guarded reads).
    let mut anglesum: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut sideind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut elemind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    // NOTE: 0-init for the &mut handoff (set in boun_options before reads).
    let mut l: c_int = 0;
    // NOTE: corner deltas live in boun_visit (dead here).
    eps = 1.0e-4;
    *noboundnodes = 0;
    if mat1 < 1 && mat2 < 1 {
        printf(c"FindBoundaryBoundary: Either of the boundaries must be positive\n".as_ptr());
        return 1;
    } else if mat1 < 1 {
        i = mat1;
        mat1 = mat2;
        mat2 = i;
    }
    if info != 0 {
        printf(
            c"Finding nodes between boundary elements of type %d and %d\n".as_ptr(),
            mat1,
            mat2,
        );
    }
    let visited: *mut c_int = ivector(1, (*data).noknots);
    boun_visit(
        data, bound, visited, boundnodes, &mut anglesum, sideind.as_mut_ptr(), mat1,
        &mut bounddim, &mut maxnodes, &mut minnodes, &mut nonodes, &mut nocorners,
        &mut elemtype, &mut i, &mut j, &mut k,
    );
    if info != 0 {
        printf(
            c"There are from %d to %d hits per node\n".as_ptr(),
            minnodes,
            maxnodes,
        );
    }
    if maxnodes < 2 {
        printf(
            c"FindBulkBoundary: Nodes must belong to more than %d elements.\n".as_ptr(),
            maxnodes,
        );
        return 2;
    }
    if bounddim == 2 {
        // NOTE: corner nodes with full angle are interior.
        i = 1;
        while i <= (*data).noknots {
            *anglesum.wrapping_offset(i as isize) /= 2.0 * FM_PI;
            if *anglesum.wrapping_offset(i as isize) > 0.99 {
                *visited.wrapping_offset(i as isize) = 0;
            }
            if *anglesum.wrapping_offset(i as isize) > 1.01 {
                printf(
                    c"FindBulkBoundary: surprisingly large angle %.3e in node %d\n".as_ptr(),
                    *anglesum.wrapping_offset(i as isize),
                    i,
                );
            }
            i += 1;
        }
        free_dvector(anglesum, 1, (*data).noknots);
        // NOTE: higher-order nodes with several hits are interior.
        k = 0;
        i = 1;
        while i <= (*data).noknots {
            if *visited.wrapping_offset(i as isize) == -1 {
                *visited.wrapping_offset(i as isize) = 1;
            } else if *visited.wrapping_offset(i as isize) < -1 {
                k += 1;
                *visited.wrapping_offset(i as isize) = 0;
            }
            i += 1;
        }
        if k != 0 && info != 0 {
            printf(
                c"Removed %d potential higher order side nodes from the list.\n".as_ptr(),
                k,
            );
        }
    }
    if bounddim == 1 {
        // NOTE: C reads visited[i] with the stale loop index here (UB in C).
        if *visited.wrapping_offset(i as isize) == maxnodes
            || *visited.wrapping_offset(i as isize) < 0
        {
            *visited.wrapping_offset(i as isize) = 0;
        }
    }
    if !boun_options(
        data, bound, visited, boundnodes, elemind.as_mut_ptr(), sideind.as_mut_ptr(),
        mat1, mat2, maxnodes, &mut eps, &mut hits, &mut nonodes, &mut material,
        &mut elemtype, &mut l, &mut i, &mut j, &mut k,
    ) {
        return 2;
    }
    *noboundnodes = 0;
    i = 1;
    while i <= (*data).noknots {
        if *boundnodes.wrapping_offset(i as isize) != 0 {
            *noboundnodes += 1;
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"Located %d nodes at the interval between boundaries %d and %d\n".as_ptr(),
            *noboundnodes,
            mat1,
            mat2,
        );
    }
    free_ivector(visited, 1, (*data).noknots);
    0
}
/// SAFETY: mid-edge node table + coordinates (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn increase_nodes(
    data: *mut FemType,
    newnodetable: *mut *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    newz: *mut Real,
    noknots: c_int,
    maxcon: c_int,
    _newknots: &mut c_int,
    con: &mut c_int,
    ind: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
) {
    // NOTE: count block lives in the caller (arrays unallocated yet).
    *i = 1;
    while *i <= noknots {
        *newx.wrapping_offset(*i as isize) = *(*data).x.wrapping_offset(*i as isize);
        *newy.wrapping_offset(*i as isize) = *(*data).y.wrapping_offset(*i as isize);
        *newz.wrapping_offset(*i as isize) = *(*data).z.wrapping_offset(*i as isize);
        *i += 1;
    }
    *i = 1;
    while *i <= noknots {
        *j = 0;
        while *j < maxcon {
            *con = *(*data).nodalgraph[*j as usize].wrapping_offset(*i as isize);
            *ind = *(*newnodetable.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            if *con != 0 && *ind != 0 {
                *newx.wrapping_offset(*ind as isize) = 0.5
                    * (*(*data).x.wrapping_offset(*i as isize)
                        + *(*data).x.wrapping_offset(*con as isize));
                *newy.wrapping_offset(*ind as isize) = 0.5
                    * (*(*data).y.wrapping_offset(*i as isize)
                        + *(*data).y.wrapping_offset(*con as isize));
                *newz.wrapping_offset(*ind as isize) = 0.5
                    * (*(*data).z.wrapping_offset(*i as isize)
                        + *(*data).z.wrapping_offset(*con as isize));
            }
            *j += 1;
        }
        *i += 1;
    }
}

/// Quadratic enrichment of linear bulk elements.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn IncreaseElementOrder(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut side: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut element: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut con: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut hit: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut node: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: null-init (Imatrix before guarded reads).
    let mut inds: [c_int; 2] = [0; 2];
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    if info != 0 {
        printf(c"Trying to increase the element order of current elements\n".as_ptr());
    }
    CreateNodalGraph(data, FALSE, info);
    let noknots: c_int = (*data).noknots;
    let noelements: c_int = (*data).noelements;
    let maxcon: c_int = (*data).nodalmaxconnections;
    maxnodes = 0;
    let newnodetable: *mut *mut c_int = imatrix(0, maxcon - 1, 1, noknots);
    i = 1;
    while i <= noknots {
        j = 0;
        while j < maxcon {
            *(*newnodetable.wrapping_offset(j as isize)).wrapping_offset(i as isize) = 0;
            j += 1;
        }
        i += 1;
    }
    newknots = 0;
    i = 1;
    while i <= noknots {
        j = 0;
        while j < maxcon {
            con = *(*data).nodalgraph[j as usize].wrapping_offset(i as isize);
            if con > i {
                newknots += 1;
                *(*newnodetable.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                    noknots + newknots;
            }
            j += 1;
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"There will be %d new nodes in the elements\n".as_ptr(),
            newknots,
        );
    }
    let newx: *mut Real = dvector(1, noknots + newknots);
    let newy: *mut Real = dvector(1, noknots + newknots);
    let newz: *mut Real = dvector(1, noknots + newknots);
    increase_nodes(
        data, newnodetable, newx, newy, newz, noknots, maxcon, &mut newknots, &mut con,
        &mut ind, &mut i, &mut j,
    );
    let maxelemtype: c_int = GetMaxElementType(data);
    // NOTE: C switch over maxelemtype.
    if maxelemtype <= 303 {
        maxnodes = 6;
    } else if maxelemtype == 404 {
        maxnodes = 8;
    } else if maxelemtype == 504 {
        maxnodes = 10;
    } else if maxelemtype == 605 {
        maxnodes = 13;
    } else if maxelemtype == 706 {
        maxnodes = 15;
    } else if maxelemtype == 808 {
        maxnodes = 20;
    } else {
        printf(c"Not implemented for elementtype %d\n".as_ptr(), maxelemtype);
        bigerror(c"IncreaseElementOrder: Cannot continue the subroutine".as_ptr());
    }
    if info != 0 {
        printf(
            c"New leading elementtype is %d\n".as_ptr(),
            100 * (maxelemtype / 100) + maxnodes,
        );
    }
    let newtopo: *mut *mut c_int = imatrix(1, noelements, 0, maxnodes - 1);
    element = 1;
    while element <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(element as isize);
        i = 0;
        while i < elemtype % 100 {
            *(*newtopo.wrapping_offset(element as isize)).wrapping_offset(i as isize) =
                *(*(*data).topology.wrapping_offset(element as isize))
                    .wrapping_offset(i as isize);
            i += 1;
        }
        element += 1;
    }
    element = 1;
    while element <= (*data).noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(element as isize);
        nonodes = *(*data).elementtypes.wrapping_offset(element as isize) % 100;
        side = 0;
        loop {
            hit = GetElementGraph(element, side, data, inds.as_mut_ptr());
            if hit == 0 {
                break;
            }
            if inds[0] > inds[1] {
                ind = inds[1];
                ind2 = inds[0];
            } else {
                ind = inds[0];
                ind2 = inds[1];
            }
            j = 0;
            while j < maxcon {
                con = *(*data).nodalgraph[j as usize].wrapping_offset(ind as isize);
                if con == ind2 {
                    node = *(*newnodetable.wrapping_offset(j as isize))
                        .wrapping_offset(ind as isize);
                    *(*newtopo.wrapping_offset(element as isize))
                        .wrapping_offset((nonodes + side) as isize) = node;
                }
                j += 1;
            }
            side += 1;
        }
        elemtype = 100 * (elemtype / 100) + nonodes + side;
        *(*data).elementtypes.wrapping_offset(element as isize) = elemtype;
        element += 1;
    }
    let stat: c_int = DestroyNodalGraph(data, info);
    let _ = stat;
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    // NOTE: C frees columns 0..maxnodes (not maxnodes-1).
    free_imatrix((*data).topology, 1, (*data).noelements, 0, (*data).maxnodes);
    free_imatrix(newnodetable, 0, maxcon - 1, 1, noknots);
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    (*data).topology = newtopo;
    (*data).noknots += newknots;
    (*data).maxnodes = maxnodes;
    if info != 0 {
        printf(c"Increased the element order from 1 to 2\n".as_ptr());
    }
    0
}
/// SAFETY: rectangular/cylindrical knot remap (split for sizing).
unsafe fn cylindrical_map(
    data: *mut FemType,
    r1: Real,
    r2: Real,
    rectangle: c_int,
    i: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut r: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut f: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut q: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mult: Real;
    *i = 1;
    while *i <= (*data).noknots {
        r = *(*data).x.wrapping_offset(*i as isize);
        z = *(*data).y.wrapping_offset(*i as isize);
        f = *(*data).z.wrapping_offset(*i as isize);
        *(*data).z.wrapping_offset(*i as isize) = z;
        if r >= r2 {
            *(*data).x.wrapping_offset(*i as isize) = cos(f) * r;
            *(*data).y.wrapping_offset(*i as isize) = sin(f) * r;
        } else if r <= r2 {
            mult = r / r1;
            if r > r1 {
                q = (r - r1) / (r2 - r1);
                r = r1;
            } else {
                q = -1.0;
            }
            if f <= 0.25 * FM_PI {
                *(*data).x.wrapping_offset(*i as isize) = r;
                *(*data).y.wrapping_offset(*i as isize) =
                    r1 * 4.0 * (f - 0.00 * FM_PI) / FM_PI;
            } else if f <= 0.75 * FM_PI {
                *(*data).y.wrapping_offset(*i as isize) = r;
                *(*data).x.wrapping_offset(*i as isize) =
                    -r1 * 4.0 * (f - 0.5 * FM_PI) / FM_PI;
            } else if f <= 1.25 * FM_PI {
                *(*data).x.wrapping_offset(*i as isize) = -r;
                *(*data).y.wrapping_offset(*i as isize) =
                    -r1 * 4.0 * (f - 1.0 * FM_PI) / FM_PI;
            } else if f <= 1.75 * FM_PI {
                *(*data).y.wrapping_offset(*i as isize) = -r;
                *(*data).x.wrapping_offset(*i as isize) =
                    r1 * 4.0 * (f - 1.5 * FM_PI) / FM_PI;
            } else {
                *(*data).x.wrapping_offset(*i as isize) = r;
                *(*data).y.wrapping_offset(*i as isize) =
                    r1 * 4.0 * (f - 2.0 * FM_PI) / FM_PI;
            }
            if rectangle == 0 && q > 0.0 {
                *(*data).x.wrapping_offset(*i as isize) = (1.0 - q)
                    * *(*data).x.wrapping_offset(*i as isize)
                    + q * cos(f) * r2;
                *(*data).y.wrapping_offset(*i as isize) = (1.0 - q)
                    * *(*data).y.wrapping_offset(*i as isize)
                    + q * sin(f) * r2;
            } else if rectangle != 0 && mult > 1.0 {
                *(*data).y.wrapping_offset(*i as isize) *= mult;
                *(*data).x.wrapping_offset(*i as isize) *= mult;
            }
        }
        *i += 1;
    }
}

/// SAFETY: duplicate-node merge after the remap (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn cylindrical_dedup(
    data: *mut FemType,
    candidatelist: *mut c_int,
    indx: *mut c_int,
    r1: Real,
    candidates: &mut c_int,
    nonodes1: &mut c_int,
    ind2: &mut c_int,
    hits: &mut c_int,
    trials: &mut c_int,
    tests: &mut c_int,
    eps: Real,
    i: &mut c_int,
    j: &mut c_int,
    j2: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real;
    *i = 1;
    while *i <= (*data).noknots {
        *indx.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *j = 1;
    while *j <= (*data).noelements {
        *nonodes1 = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
        *i = 0;
        while *i < *nonodes1 {
            *ind2 = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            *indx.wrapping_offset(*ind2 as isize) = *ind2;
            *i += 1;
        }
        *j += 1;
    }
    *candidates = 0;
    *i = 1;
    while *i <= (*data).noknots {
        if *indx.wrapping_offset(*i as isize) == 0 {
            *i += 1;
            continue;
        }
        x = *(*data).x.wrapping_offset(*i as isize);
        y = *(*data).y.wrapping_offset(*i as isize);
        if fabs(y) > r1 + eps {
            *i += 1;
            continue;
        }
        if fabs(x) > r1 + eps && fabs(y) > eps {
            *i += 1;
            continue;
        }
        if fabs(x) > eps
            && fabs(y) > eps
            && fabs(fabs(x) - r1) > eps
            && fabs(fabs(y) - r1) > eps
        {
            *i += 1;
            continue;
        }
        *candidates += 1;
        *candidatelist.wrapping_offset(*candidates as isize) = *i;
        *i += 1;
    }
    printf(
        c"%d/%d candidates for duplicate nodes.\n".as_ptr(),
        *candidates,
        (*data).noknots,
    );
    *hits = 0;
    *tests = 0;
    *trials = 0;
    *j = 1;
    while *j <= *candidates {
        ind1 = *indx.wrapping_offset(*candidatelist.wrapping_offset(*j as isize) as isize);
        x = *(*data).x.wrapping_offset(ind1 as isize);
        y = *(*data).y.wrapping_offset(ind1 as isize);
        z = *(*data).z.wrapping_offset(ind1 as isize);
        *j2 = *j + 1;
        while *j2 <= *candidates {
            *ind2 = *indx.wrapping_offset(*candidatelist.wrapping_offset(*j2 as isize) as isize);
            x2 = *(*data).x.wrapping_offset(*ind2 as isize);
            y2 = *(*data).y.wrapping_offset(*ind2 as isize);
            z2 = *(*data).z.wrapping_offset(*ind2 as isize);
            dx = x - x2;
            dy = y - y2;
            dz = z - z2;
            *tests += 1;
            if dx * dx + dy * dy + dz * dz < eps * eps && *ind2 != ind1 {
                *indx.wrapping_offset(*candidatelist.wrapping_offset(*j2 as isize) as isize) =
                    ind1;
                *hits += 1;
            }
            *j2 += 1;
        }
        *j += 1;
    }
    printf(c"Found %d double nodes in %d tests.\n".as_ptr(), *hits, *tests);
    *j = 1;
    while *j <= (*data).noelements {
        *nonodes1 = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
        *i = 0;
        while *i < *nonodes1 {
            *ind2 = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            if *ind2 != *indx.wrapping_offset(*ind2 as isize) {
                *trials += 1;
                *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*i as isize) =
                    *indx.wrapping_offset(*ind2 as isize);
            }
            *i += 1;
        }
        *j += 1;
    }
}
/// SAFETY: rectangular/cylindrical remap + dedup driver.
unsafe fn CylindricalCoordinateTransformation(
    data: *mut FemType,
    r1: Real,
    r2: Real,
    rectangle: c_int,
) {
    // NOTE: 0-init for the &mut handoff (set in cylindrical_* before reads.)
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in cylindrical_* before reads.)
    let mut j: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut j2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes1: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut hits: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut trials: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut tests: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut candidates: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: temporaries set in the helpers before every read.
    if rectangle != 0 {
        printf(
            c"Rectangular geometry with r1=%.4lg for %d nodes.\n".as_ptr(),
            r1,
            (*data).noknots,
        );
    } else {
        printf(
            c"Cylindrical geometry with r1=%.4lg r2=%.4lg for %d nodes.\n".as_ptr(),
            r1,
            r2,
            (*data).noknots,
        );
    }
    cylindrical_map(data, r1, r2, rectangle, &mut i);
    let eps: Real = 1.0e-3 * (*data).minsize;
    candidates = 0;
    let candidatelist: *mut c_int = ivector(1, (*data).noknots);
    let indx: *mut c_int = ivector(1, (*data).noknots);
    cylindrical_dedup(
        data, candidatelist, indx, r1, &mut candidates, &mut nonodes1, &mut ind2,
        &mut hits, &mut trials, &mut tests, eps, &mut i, &mut j, &mut j2,
    );
    free_ivector(indx, 1, (*data).noknots);
    free_ivector(candidatelist, 1, (*data).noknots);
    printf(c"Eliminated %d nodes from topology.\n".as_ptr(), trials);
}

/// SAFETY: cylindrical mesh quality improvement (pure geometry).
unsafe fn CylindricalCoordinateImprove(
    data: *mut FemType,
    mut factor: Real,
    r1: Real,
    r2: Real,
) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut r: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut q: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut q2: Real;
    printf(
        c"Cylindrical coordinate for r1=%.4lg and r2=%.4lg.\n".as_ptr(),
        r1,
        r2,
    );
    let eps: Real = 1.0e-10;
    let cmin: Real = 1.0 / (3.0 - sqrt(3.0));
    let cmax: Real = 1.0;
    // NOTE: manual clamp (C has no NaN trap; f64::clamp would panic on NaN
    // and pull float Debug fmt into the object, breaking the nm U gate).
    #[allow(clippy::manual_clamp)]
    if factor > 1.0 {
        factor = 1.0;
    } else if factor < 0.0 {
        factor = 0.0;
    }
    let c: Real = cmin + (cmax - cmin) * factor;
    if fabs(c - 1.0) < eps {
        return;
    }
    printf(
        c"Improving cylindrical mesh quality r1=%.4lg, r2=%.4lg and c=%.4lg\n".as_ptr(),
        r1,
        r2,
        c,
    );
    i = 1;
    while i <= (*data).noknots {
        x = *(*data).x.wrapping_offset(i as isize);
        y = *(*data).y.wrapping_offset(i as isize);
        r = sqrt(x * x + y * y);
        if r >= r2 {
            i += 1;
            continue;
        }
        if r < eps {
            i += 1;
            continue;
        }
        if fabs(x) <= r1 + eps && fabs(y) <= r1 + eps {
            if fabs(x) < fabs(y) {
                q = fabs(x / y);
                *(*data).x.wrapping_offset(i as isize) = (c * q + (1.0 - q)) * x;
                *(*data).y.wrapping_offset(i as isize) = (c * q + (1.0 - q)) * y;
            } else {
                q = fabs(y / x);
                *(*data).x.wrapping_offset(i as isize) = (c * q + (1.0 - q)) * x;
                *(*data).y.wrapping_offset(i as isize) = (c * q + (1.0 - q)) * y;
            }
        } else if fabs(x) < fabs(y) {
            q = fabs(x / y);
            q2 = (fabs(y) - r1) / (r2 * fabs(y / r) - r1);
            *(*data).x.wrapping_offset(i as isize) = (c * q + (1.0 - q)) * x * (1.0 - q2)
                + q2 * x;
            *(*data).y.wrapping_offset(i as isize) =
                (c * q + (1.0 - q)) * (1.0 - q2) * y + q2 * y;
        } else {
            q = fabs(y / x);
            q2 = (fabs(x) - r1) / (r2 * fabs(x / r) - r1);
            *(*data).x.wrapping_offset(i as isize) =
                (c * q + (1.0 - q)) * (1.0 - q2) * x + q2 * x;
            *(*data).y.wrapping_offset(i as isize) =
                (c * q + (1.0 - q)) * y * (1.0 - q2) + q2 * y;
        }
        i += 1;
    }
}

/// Bend a mesh around a cylindrical curve.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CylindricalCoordinateCurve(
    data: *mut FemType,
    zet: Real,
    rad: Real,
    angle: Real,
) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _y: Real;
    // NOTE: C `y` write-only here (only x/z bend); kept unread.
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut f: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z2: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x2: Real;
    printf(
        c"Cylindrical coordinate curve, zet=%.3lg  rad=%.3lg  angle=%.3lg\n".as_ptr(),
        zet,
        rad,
        angle,
    );
    let r0: Real = rad;
    let f0: Real = FM_PI * (angle / 180.0);
    let z0: Real = zet;
    let z1: Real = z0 + r0 * f0;
    i = 1;
    while i <= (*data).noknots {
        if (*data).dim == 2 {
            z = *(*data).x.wrapping_offset(i as isize);
            x = *(*data).y.wrapping_offset(i as isize);
        } else {
            x = *(*data).x.wrapping_offset(i as isize);
            _y = *(*data).y.wrapping_offset(i as isize);
            z = *(*data).z.wrapping_offset(i as isize);
        }
        if z <= z0 {
            i += 1;
            continue;
        }
        if z >= z1 {
            z2 = z0 + sin(f0) * (r0 + x) + cos(f0) * (z - z1);
            x2 = (cos(f0) - 1.0) * r0 + cos(f0) * x - sin(f0) * (z - z1);
        } else {
            f = (z - z0) / r0;
            z2 = z0 + sin(f) * (r0 + x);
            x2 = (cos(f) - 1.0) * r0 + cos(f) * x;
        }
        if (*data).dim == 2 {
            *(*data).x.wrapping_offset(i as isize) = z2;
            *(*data).y.wrapping_offset(i as isize) = x2;
        } else {
            *(*data).z.wrapping_offset(i as isize) = z2;
            *(*data).x.wrapping_offset(i as isize) = x2;
        }
        i += 1;
    }
}
/// SAFETY: cartesian variance split arms (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn separate_cartesian_side(
    bound: *mut BoundaryType,
    j: c_int,
    k: c_int,
    addtype: &mut c_int,
    used: &mut c_int,
    bc: *mut c_int,
    bcdim: *mut c_int,
    bclim: *mut Real,
    eps: Real,
    dx: Real,
    dy: Real,
    dz: Real,
    sx: Real,
    sy: Real,
    sideelemtype: c_int,
    i: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut hit: c_int;
    if sideelemtype < 300 && dz < eps {
        if dx < eps * dy {
            hit = FALSE;
            *i = 0;
            while *i < MAXBOUNDARIES as c_int && *bcdim.wrapping_offset(*i as isize) != 0 {
                if *bcdim.wrapping_offset(*i as isize) == 1
                    && fabs(*bclim.wrapping_offset(*i as isize) - sx) < eps * fabs(dy)
                {
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize) = *bc.wrapping_offset(*i as isize);
                    hit = TRUE;
                    break;
                }
                *i += 1;
            }
            if hit == 0 {
                if *used != 0 {
                    *addtype += 1;
                    printf(c"Adding new BC %d in Y-direction\n".as_ptr(), *addtype);
                    *bc.wrapping_offset(*i as isize) = *addtype;
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize) = *bc.wrapping_offset(*i as isize);
                } else {
                    *bc.wrapping_offset(*i as isize) = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize);
                }
                *bcdim.wrapping_offset(*i as isize) = 1;
                *bclim.wrapping_offset(*i as isize) = sx;
                *used = TRUE;
            }
        }
        if dy < eps * dx {
            hit = FALSE;
            *i = 0;
            while *i < MAXBOUNDARIES as c_int && *bcdim.wrapping_offset(*i as isize) != 0 {
                if *bcdim.wrapping_offset(*i as isize) == 2
                    && fabs(*bclim.wrapping_offset(*i as isize) - sy) < eps * fabs(dx)
                {
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize) = *bc.wrapping_offset(*i as isize);
                    hit = TRUE;
                    break;
                }
                *i += 1;
            }
            if hit == 0 {
                if *used != 0 {
                    *addtype += 1;
                    printf(c"Adding new BC %d in X-direction\n".as_ptr(), *addtype);
                    *bc.wrapping_offset(*i as isize) = *addtype;
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize) = *bc.wrapping_offset(*i as isize);
                } else {
                    *bc.wrapping_offset(*i as isize) = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize);
                }
                *bcdim.wrapping_offset(*i as isize) = 2;
                *bclim.wrapping_offset(*i as isize) = sy;
                *used = TRUE;
            }
        }
    } else if (dx < eps * dy && dx < eps * dz) || (dy < eps * dx && dy < eps * dz) {
        // NOTE: both arms empty in C (SeparateCartesianBoundaries).
    }
}

/// Split boundaries by cartesian direction of each side.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn SeparateCartesianBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut type_: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut addtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemsides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totsides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut used: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut hit: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: C sizes the scratch by MAXBOUNDARIES (roomy, kept verbatim).
    let mut sideind: [c_int; MAXBOUNDARIES] = [0; MAXBOUNDARIES];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sxx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut syy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut szz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real;
    let mut bclim: [Real; MAXBOUNDARIES] = [0.0; MAXBOUNDARIES];
    let mut bc: [c_int; MAXBOUNDARIES] = [0; MAXBOUNDARIES];
    let mut bcdim: [c_int; MAXBOUNDARIES] = [0; MAXBOUNDARIES];
    // NOTE: `=1.0e-4` decl init kept (read before any overwrite).
    let eps: Real = 1.0e-4;
    let _ = hit;
    maxtype = 0;
    totsides = 0;
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
            totsides += 1;
            k = 1;
            while k <= (*bound.wrapping_offset(j as isize)).nosides {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxtype
                    < *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize)
                {
                    maxtype = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize);
                }
                k += 1;
            }
            i += 1;
        }
        j += 1;
    }
    if info != 0 {
        printf(c"Maximum boundary type is %d\n".as_ptr(), maxtype);
        printf(c"Number of boundaries is %d\n".as_ptr(), totsides);
    }
    addtype = maxtype;
    type_ = 1;
    while type_ <= maxtype {
        i = 0;
        while i < MAXBOUNDARIES as c_int {
            bclim[i as usize] = 0.0;
            i += 1;
        }
        i = 0;
        while i < MAXBOUNDARIES as c_int {
            bc[i as usize] = 0;
            bcdim[i as usize] = 0;
            i += 1;
        }
        used = FALSE;
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
            k = 1;
            while k <= (*bound.wrapping_offset(j as isize)).nosides {
                if *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(k as isize)
                    != type_
                {
                    k += 1;
                    continue;
                }
                GetElementSide(
                    *(*bound.wrapping_offset(j as isize))
                        .parent
                        .wrapping_offset(k as isize),
                    *(*bound.wrapping_offset(j as isize))
                        .side
                        .wrapping_offset(k as isize),
                    *(*bound.wrapping_offset(j as isize))
                        .normal
                        .wrapping_offset(k as isize),
                    data,
                    sideind.as_mut_ptr(),
                    &mut sideelemtype,
                );
                sx = 0.0;
                sy = 0.0;
                sz = 0.0;
                sxx = 0.0;
                syy = 0.0;
                szz = 0.0;
                elemsides = sideelemtype % 100;
                // NOTE: variance of the side nodes per axis.
                l = 0;
                while l < elemsides {
                    x = *(*data).x.wrapping_offset(sideind[l as usize] as isize);
                    y = *(*data).y.wrapping_offset(sideind[l as usize] as isize);
                    z = *(*data).z.wrapping_offset(sideind[l as usize] as isize);
                    sx += x;
                    sy += y;
                    sz += z;
                    sxx += x * x;
                    syy += y * y;
                    szz += z * z;
                    l += 1;
                }
                sx /= elemsides as Real;
                sy /= elemsides as Real;
                sz /= elemsides as Real;
                sxx /= elemsides as Real;
                syy /= elemsides as Real;
                szz /= elemsides as Real;
                dx = sqrt(sxx - sx * sx);
                dy = sqrt(syy - sy * sy);
                dz = sqrt(szz - sz * sz);
                separate_cartesian_side(
                    bound, j, k, &mut addtype, &mut used, bc.as_mut_ptr(),
                    bcdim.as_mut_ptr(), bclim.as_mut_ptr(), eps, dx, dy, dz, sx, sy,
                    sideelemtype, &mut i,
                );
                k += 1;
            }
            j += 1;
        }
        type_ += 1;
    }
}
/// SAFETY: mainaxis vote pass over one boundary (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn separate_mainaxis_pass(
    data: *mut FemType,
    bound: *mut BoundaryType,
    j: c_int,
    maxtype: &mut c_int,
    axistype: *mut c_int,
    axishit: *mut c_int,
    done: c_int,
    eps: Real,
    sideelemtype: &mut c_int,
    sideind: *mut c_int,
    sx: &mut Real,
    sy: &mut Real,
    elemsides: &mut c_int,
    l: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sxx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut syy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut szz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real;
    *k = 1;
    while *k <= (*bound.wrapping_offset(j as isize)).nosides {
        GetElementSide(
            *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(*k as isize),
            *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(*k as isize),
            *(*bound.wrapping_offset(j as isize))
                .normal
                .wrapping_offset(*k as isize),
            data,
            sideind,
            sideelemtype,
        );
        *sx = 0.0;
        *sy = 0.0;
        sz = 0.0;
        sxx = 0.0;
        syy = 0.0;
        szz = 0.0;
        *elemsides = *sideelemtype % 100;
        // NOTE: variance of the side nodes per axis.
        *l = 0;
        while *l < *elemsides {
            x = *(*data).x.wrapping_offset(*sideind.wrapping_offset(*l as isize) as isize);
            y = *(*data).y.wrapping_offset(*sideind.wrapping_offset(*l as isize) as isize);
            z = *(*data).z.wrapping_offset(*sideind.wrapping_offset(*l as isize) as isize);
            *sx += x;
            *sy += y;
            sz += z;
            sxx += x * x;
            syy += y * y;
            szz += z * z;
            *l += 1;
        }
        *sx /= *elemsides as Real;
        *sy /= *elemsides as Real;
        sz /= *elemsides as Real;
        sxx /= *elemsides as Real;
        syy /= *elemsides as Real;
        szz /= *elemsides as Real;
        dx = sqrt(sxx - *sx * *sx);
        dy = sqrt(syy - *sy * *sy);
        dz = sqrt(szz - sz * sz);
        if dx < eps * dy && dx < eps * dz {
            if *sx > 0.0 {
                if done != 0 {
                    if *axistype.wrapping_offset(0) != 0 {
                        *(*bound.wrapping_offset(j as isize))
                            .types
                            .wrapping_offset(*k as isize) =
                            *maxtype + *axistype.wrapping_offset(0);
                    }
                } else {
                    (*axishit.wrapping_offset(0)) += 1;
                }
            }
            if *sx < 0.0 {
                if done != 0 {
                    if *axistype.wrapping_offset(1) != 0 {
                        *(*bound.wrapping_offset(j as isize))
                            .types
                            .wrapping_offset(*k as isize) =
                            *maxtype + *axistype.wrapping_offset(1);
                    }
                } else {
                    (*axishit.wrapping_offset(1)) += 1;
                }
            }
        } else if dy < eps * dx && dy < eps * dz {
            if *sy > 0.0 {
                if done != 0 {
                    if *axistype.wrapping_offset(2) != 0 {
                        *(*bound.wrapping_offset(j as isize))
                            .types
                            .wrapping_offset(*k as isize) =
                            *maxtype + *axistype.wrapping_offset(2);
                    }
                } else {
                    (*axishit.wrapping_offset(2)) += 1;
                }
            }
            if *sy < 0.0 {
                if done != 0 {
                    if *axistype.wrapping_offset(3) != 0 {
                        *(*bound.wrapping_offset(j as isize))
                            .types
                            .wrapping_offset(*k as isize) =
                            *maxtype + *axistype.wrapping_offset(3);
                    }
                } else {
                    (*axishit.wrapping_offset(3)) += 1;
                }
            }
        }
        *k += 1;
    }
}
/// Split rectangular boundaries by sign of the flat axis.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn SeparateMainaxisBoundaries(data: *mut FemType, bound: *mut BoundaryType) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut addtype: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemsides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut axistype: [c_int; 4] = [0; 4];
    let mut axishit: [c_int; 4] = [0; 4];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut axissum: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut axismax: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut done: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sx: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sy: Real = 0.0;
    // NOTE: `=1.0e-6` decl init kept (read before any overwrite).
    let eps: Real = 1.0e-6;
    maxtype = 0;
    addtype = 0;
    j = 0;
    while j < (*data).noboundaries {
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
            k = 1;
            while k <= (*bound.wrapping_offset(j as isize)).nosides {
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if maxtype
                    < *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize)
                {
                    maxtype = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(k as isize);
                }
                k += 1;
            }
            i += 1;
        }
        j += 1;
    }
    printf(c"Maximum boundary type is %d\n".as_ptr(), maxtype);
    // NOTE: `#if 0` legacy type-vector block omitted (dead in C).
    j = 0;
    while j < (*data).noboundaries {
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        if (*bound.wrapping_offset(j as isize)).nosides == 0 {
            j += 1;
            continue;
        }
        k = 0;
        while k < 4 {
            axishit[k as usize] = 0;
            k += 1;
        }
        done = 0;
        // NOTE: C `omstart:` label + `goto omstart`; vote then apply.
        loop {
            separate_mainaxis_pass(
                data, bound, j, &mut maxtype, axistype.as_mut_ptr(),
                axishit.as_mut_ptr(), done, eps, &mut sideelemtype,
                sideind.as_mut_ptr(), &mut sx, &mut sy, &mut elemsides, &mut l,
                &mut k,
            );
            // NOTE: rank the hit axes for new type indexes.
            if done == 0 {
                axissum = 0;
                    // NOTE: C `axismax = 0` dropped (reset in the loop below).
                k = 0;
                while k < 4 {
                    axissum += axishit[k as usize];
                    if axishit[k as usize] != 0 {
                        addtype += 1;
                    }
                    k += 1;
                }
                if axissum != 0 {
                    k = 0;
                    while k < 4 {
                        axismax = 0;
                        l = 0;
                        while l < 4 {
                            if axishit[l as usize] > axishit[axismax as usize] {
                                axismax = l;
                            }
                            l += 1;
                        }
                        axistype[axismax as usize] = k + 1;
                        axishit[axismax as usize] = -(k + 1);
                        k += 1;
                    }
                    if axissum == (*bound.wrapping_offset(j as isize)).nosides {
                        k = 0;
                        while k < 4 {
                            axistype[k as usize] -= 1;
                            k += 1;
                        }
                        addtype -= 1;
                    }
                    if addtype != 0 {
                        printf(
                            c"Separating %d rectangular boundaries from boundary %d.\n".as_ptr(),
                            addtype,
                            j,
                        );
                        done = 1;
                        continue;
                    } else {
                        // NOTE: C `done = 0` dropped (already zero).
                    }
                }
            }
            break;
        }
        maxtype += addtype;
        j += 1;
    }
    let _ = i;
}
/// SAFETY: extrude division + type detect + struct init (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_init(
    dataxy: *mut FemType,
    boundxy: *mut BoundaryType,
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: &mut *mut c_int,
    indxlength: &mut c_int,
    elemtype: &mut c_int,
    origtype: &mut c_int,
    nonodes3d: &mut c_int,
    nonodes2d: &mut c_int,
    layers: &mut c_int,
    newbounds: &mut c_int,
    usenames: &mut c_int,
    maxsidetype: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    size: &mut c_int,
    info: c_int,
) -> bool {
    if (*grid).rotate != 0 {
        SetElementDivisionCylinder(grid, info);
    } else if (*grid).dimension == 3 {
        SetElementDivisionExtruded(grid, info);
    } else {
        printf(c"CreateKnotsExtruded: unknown option!\n".as_ptr());
        return false;
    }
    InitializeKnots(data);
    (*data).dim = 3;
    *origtype = 0;
    *i = 1;
    while *i <= (*dataxy).noelements {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *origtype < *(*dataxy).elementtypes.wrapping_offset(*i as isize) {
            *origtype = *(*dataxy).elementtypes.wrapping_offset(*i as isize);
        }
        *i += 1;
    }
    // NOTE: C if/else chain over origtype.
    if *origtype == 202 {
        *elemtype = 404;
    } else if *origtype == 303 {
        *elemtype = 706;
    } else if *origtype == 404 {
        *elemtype = 808;
    } else if *origtype == 408 {
        *elemtype = 820;
    } else if *origtype == 409 {
        *elemtype = 827;
    } else {
        printf(
            c"CreateKnotsExtruded: not implemented for elementtypes %d!\n".as_ptr(),
            *origtype,
        );
        return false;
    }
    printf(
        c"Maximum elementtype %d extruded to type %d.\n".as_ptr(),
        *origtype,
        *elemtype,
    );
    *nonodes2d = *origtype % 100;
    (*data).maxnodes = *elemtype % 100;
    *nonodes3d = *elemtype % 100;
    if *nonodes3d <= 8 {
        *layers = 1;
    } else {
        *layers = 2;
    }
    // NOTE: initialize the 3D mesh structure.
    (*data).noknots = (*dataxy).noknots * (*layers * (*grid).totzelems + 1);
    (*data).noelements = (*dataxy).noelements * (*grid).totzelems;
    (*data).coordsystem = (*dataxy).coordsystem;
    (*data).numbering = (*dataxy).numbering;
    (*data).noboundaries = (*dataxy).noboundaries;
    (*data).maxsize = (*dataxy).maxsize;
    (*data).minsize = (*dataxy).minsize;
    (*data).partitionexist = FALSE;
    (*data).periodicexist = FALSE;
    (*data).nodeconnectexist = FALSE;
    (*data).elemconnectexist = FALSE;
    *usenames = (*dataxy).bodynamesexist | (*dataxy).boundarynamesexist;
    if *usenames != 0 {
        if (*grid).zmaterialmapexists != 0 {
            printf(c"Cannot extrude names when there is a given material mapping!\n".as_ptr());
            *usenames = FALSE;
        } else if info != 0 {
            printf(c"Trying to maintain entity names in extrusion\n".as_ptr());
        }
    }
    *maxsidetype = 0;
    AllocateKnots(data);
    *indxlength = (*data).noknots.max((*data).noelements);
    *indx = ivector(0, *indxlength);
    *i = 0;
    while *i <= *indxlength {
        *(*indx).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *newbounds = 0;
    if (*grid).dimension == 3 {
        *newbounds = (*grid).zcells + 1;
    } else if (*grid).rotate != 0 {
        if (*grid).rotateblocks < 4 {
            *newbounds = 4;
        }
        if (*grid).rotatecartesian != 0 {
            *newbounds += (*grid).rotateblocks;
        }
    }
    // NOTE: initialize the 3D boundary lists.
    *j = 0;
    while *j < (*data).noboundaries + *newbounds {
        if (*boundxy.wrapping_offset(*j as isize)).created != 0
            || *j >= (*data).noboundaries
        {
            *bound.wrapping_offset(*j as isize) = *boundxy.wrapping_offset(*j as isize);
            (*bound.wrapping_offset(*j as isize)).created = TRUE;
            (*bound.wrapping_offset(*j as isize)).nosides =
                (*boundxy.wrapping_offset(*j as isize)).nosides * (*grid).totzelems;
            *size = (*bound.wrapping_offset(*j as isize)).nosides;
            if *j >= (*data).noboundaries {
                *size = (*dataxy).noelements;
            }
            (*bound.wrapping_offset(*j as isize)).coordsystem = COORD_CART3;
            (*bound.wrapping_offset(*j as isize)).side = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).side2 = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).material = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).parent = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).parent2 = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).types = ivector(1, *size);
            (*bound.wrapping_offset(*j as isize)).normal = ivector(1, *size);
            *i = 1;
            while *i <= *size {
                *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .side2
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .parent
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .parent2
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .material
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .normal
                    .wrapping_offset(*i as isize) = 1;
                *i += 1;
            }
            (*bound.wrapping_offset(*j as isize)).echain = FALSE;
            (*bound.wrapping_offset(*j as isize)).ediscont = FALSE;
        }
        *j += 1;
    }
    if info != 0 {
        printf(c"Allocated for %d new BC lists\n".as_ptr(), *j);
    }
    true
}
/// SAFETY: extruded element topology fill (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_topology(
    dataxy: *mut FemType,
    grid: *mut GridType,
    data: *mut FemType,
    indx: *mut c_int,
    layers: c_int,
    knot0: &mut c_int,
    knot1: &mut c_int,
    knot2: &mut c_int,
    elem0: &mut c_int,
    level: &mut c_int,
    material_too_large: &mut c_int,
    nonodes2d: &mut c_int,
    material: &mut c_int,
    meanx: &mut Real,
    cellk: &mut c_int,
    kmax: &mut c_int,
    k: &mut c_int,
    element: &mut c_int,
    origtype: &mut c_int,
    elemtype: &mut c_int,
    i: &mut c_int,
) {
    *knot0 = 0;
    *knot1 = layers * (*dataxy).noknots;
    if layers == 2 {
        *knot2 = (*dataxy).noknots;
    } else {
        *knot2 = 0;
    }
    *elem0 = 0;
    *level = 0;
    *material_too_large = 0;
    // NOTE: element topology of the extruded mesh.
    *cellk = 1;
    while *cellk <= (*grid).zcells {
        *kmax = (*grid).zelems[*cellk as usize];
        *k = 1;
        while *k <= *kmax {
            *level += 1;
            *element = 1;
            while *element <= (*dataxy).noelements {
                *origtype = *(*dataxy).elementtypes.wrapping_offset(*element as isize);
                *nonodes2d = *origtype % 100;
                if *origtype == 202 {
                    *elemtype = 404;
                } else if *origtype == 303 {
                    *elemtype = 706;
                } else if *origtype == 404 {
                    *elemtype = 808;
                } else if *origtype == 408 {
                    *elemtype = 820;
                } else if *origtype == 409 {
                    *elemtype = 827;
                }
                if (*grid).zmaterialmapexists != 0 {
                    *material =
                        *(*dataxy).material.wrapping_offset(*element as isize);
                    if *material > (*grid).maxmaterial {
                        *material_too_large += 1;
                        *element += 1;
                        continue;
                    }
                    *material = *(*(*grid)
                        .zmaterialmap
                        .wrapping_offset(*cellk as isize))
                    .wrapping_offset(*material as isize);
                    if *material <= 0 {
                        *element += 1;
                        continue;
                    }
                } else {
                    if *(*dataxy).material.wrapping_offset(*element as isize)
                        < (*grid).zfirstmaterial[*cellk as usize]
                    {
                        *element += 1;
                        continue;
                    }
                    if *(*dataxy).material.wrapping_offset(*element as isize)
                        > (*grid).zlastmaterial[*cellk as usize]
                    {
                        *element += 1;
                        continue;
                    }
                    if (*grid).zmaterial[*cellk as usize] != 0 {
                        *material = (*grid).zmaterial[*cellk as usize];
                    } else {
                        *material =
                            *(*dataxy).material.wrapping_offset(*element as isize);
                    }
                }
                if (*grid).rotate != 0 {
                    *meanx = 0.0;
                    *i = 0;
                    while *i < *nonodes2d {
                        *meanx += *(*dataxy).x.wrapping_offset(
                            *(*(*dataxy).topology.wrapping_offset(*element as isize))
                                .wrapping_offset(*i as isize) as isize,
                        );
                        *i += 1;
                    }
                    *meanx = fabs(*meanx / *nonodes2d as Real);
                }
                if (*grid).rotate != 0 && *meanx < 0.0 {
                    *element += 1;
                    continue;
                }
                if (*grid).rotate != 0
                    && *cellk % 2 == 0
                    && *meanx < (*grid).rotateradius1
                {
                    *element += 1;
                    continue;
                }
                *elem0 += 1;
                // NOTE: new element order vector.
                *indx.wrapping_offset(
                    ((*level - 1) * (*dataxy).noelements + *element) as isize,
                ) = *elem0;
                *(*data).elementtypes.wrapping_offset(*elem0 as isize) = *elemtype;
                *(*data).material.wrapping_offset(*elem0 as isize) = *material;
                if *elemtype == 706 {
                    *i = 0;
                    while *i < 3 {
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset(*i as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot0;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 3) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot1;
                        *i += 1;
                    }
                } else if *elemtype == 808 {
                    *i = 0;
                    while *i < 4 {
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset(*i as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot0;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 4) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot1;
                        *i += 1;
                    }
                }
                if *elemtype == 820 || *elemtype == 827 {
                    *i = 0;
                    while *i < 4 {
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset(*i as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot0;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 4) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot1;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 8) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset((*i + 4) as isize)
                            + *knot0;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 12) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize)
                            + *knot2;
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((*i + 16) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset((*i + 4) as isize)
                            + *knot1;
                        *i += 1;
                    }
                }
                if *elemtype == 827 {
                    *i = 0;
                    while *i < 4 {
                        *(*(*data).topology.wrapping_offset(*elem0 as isize))
                            .wrapping_offset((20 + *i) as isize) = *(*(*dataxy)
                            .topology
                            .wrapping_offset(*element as isize))
                        .wrapping_offset((4 + *i) as isize)
                            + *knot2;
                        *i += 1;
                    }
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(24) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(8)
                        + *knot0;
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(25) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(8)
                        + *knot1;
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(26) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(8)
                        + *knot2;
                } else if *elemtype == 404 {
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(0) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(0)
                        + *knot0;
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(1) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(1)
                        + *knot0;
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(2) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(1)
                        + *knot1;
                    *(*(*data).topology.wrapping_offset(*elem0 as isize))
                        .wrapping_offset(3) = *(*(*dataxy)
                        .topology
                        .wrapping_offset(*element as isize))
                    .wrapping_offset(0)
                        + *knot1;
                }
                *element += 1;
            }
            *knot0 += layers * (*dataxy).noknots;
            *knot1 += layers * (*dataxy).noknots;
            *knot2 += layers * (*dataxy).noknots;
            *k += 1;
        }
        *cellk += 1;
    }
}
/// SAFETY: extruded knot coordinates + midnodes (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_coords(
    dataxy: *mut FemType,
    grid: *mut GridType,
    data: *mut FemType,
    layers: c_int,
    elemtype: c_int,
    knot0: &mut c_int,
    z: &mut Real,
    topo: &mut *mut c_int,
    cellk: &mut c_int,
    k: &mut c_int,
    element: &mut c_int,
    i: &mut c_int,
) {
    // NOTE: nodal coordinates of the extruded mesh.
    *knot0 = 0;
    *cellk = 1;
    while *cellk <= (*grid).zcells {
        if *cellk == 1 {
            *k = 0;
        } else {
            *k = 1;
        }
        while *k <= (*grid).zelems[*cellk as usize] {
            if (*grid).zlinear[*cellk as usize] != 0 {
                *z = (*grid).z[(*cellk - 1) as usize]
                    + *k as Real * (*grid).dz[*cellk as usize];
            } else if (*grid).zexpand[*cellk as usize] > 0.0 {
                *z = (*grid).z[(*cellk - 1) as usize]
                    + (*grid).dz[*cellk as usize]
                        * (1.0
                            - pow(
                                (*grid).zratios[*cellk as usize],
                                *k as Real,
                            ))
                        / (1.0 - (*grid).zratios[*cellk as usize]);
            } else if (*grid).zelems[*cellk as usize] <= 2 {
                *z = (*grid).z[(*cellk - 1) as usize]
                    + *k as Real * (*grid).dz[*cellk as usize];
            } else if *k <= (*grid).zelems[*cellk as usize] / 2 {
                *z = (*grid).z[(*cellk - 1) as usize]
                    + (*grid).dz[*cellk as usize]
                        * (1.0
                            - pow(
                                (*grid).zratios[*cellk as usize],
                                *k as Real,
                            ))
                        / (1.0 - (*grid).zratios[*cellk as usize]);
            } else {
                *z = (*grid).z[*cellk as usize]
                    - (*grid).dz[*cellk as usize]
                        * (1.0
                            - pow(
                                (*grid).zratios[*cellk as usize],
                                ((*grid).zelems[*cellk as usize] - *k) as Real,
                            ))
                        / (1.0 - (*grid).zratios[*cellk as usize]);
            }
            *i = 1;
            while *i <= (*dataxy).noknots {
                *(*data).x.wrapping_offset((*i + *knot0) as isize) =
                    *(*dataxy).x.wrapping_offset(*i as isize);
                *(*data).y.wrapping_offset((*i + *knot0) as isize) =
                    *(*dataxy).y.wrapping_offset(*i as isize);
                *(*data).z.wrapping_offset((*i + *knot0) as isize) = *z;
                *i += 1;
            }
            *knot0 += layers * (*dataxy).noknots;
            *k += 1;
        }
        *cellk += 1;
    }
    // NOTE: mid-edge nodes of quadratic extruded elements.
    if elemtype == 820 || elemtype == 827 {
        *element = 1;
        while *element <= (*data).noelements {
            *topo = *(*data).topology.wrapping_offset(*element as isize);
            *i = 0;
            while *i < 4 {
                *(*data).x.wrapping_offset(
                    *(*topo).wrapping_offset((*i + 12) as isize) as isize,
                ) = 0.5
                    * (*(*data).x.wrapping_offset(
                        *(*topo).wrapping_offset(*i as isize) as isize,
                    ) + *(*data).x.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 4) as isize) as isize,
                    ));
                *(*data).y.wrapping_offset(
                    *(*topo).wrapping_offset((*i + 12) as isize) as isize,
                ) = 0.5
                    * (*(*data).y.wrapping_offset(
                        *(*topo).wrapping_offset(*i as isize) as isize,
                    ) + *(*data).y.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 4) as isize) as isize,
                    ));
                *(*data).z.wrapping_offset(
                    *(*topo).wrapping_offset((*i + 12) as isize) as isize,
                ) = 0.5
                    * (*(*data).z.wrapping_offset(
                        *(*topo).wrapping_offset(*i as isize) as isize,
                    ) + *(*data).z.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 4) as isize) as isize,
                    ));
                *i += 1;
            }
            if elemtype == 827 {
                *i = 0;
                while *i < 4 {
                    *(*data).x.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 20) as isize) as isize,
                    ) = 0.5
                        * (*(*data).x.wrapping_offset(
                            *(*topo).wrapping_offset((12 + *i) as isize) as isize,
                        ) + *(*data).x.wrapping_offset(
                            *(*topo).wrapping_offset((12 + (*i + 1) % 4) as isize) as isize,
                        ));
                    *(*data).y.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 20) as isize) as isize,
                    ) = 0.5
                        * (*(*data).y.wrapping_offset(
                            *(*topo).wrapping_offset((12 + *i) as isize) as isize,
                        ) + *(*data).y.wrapping_offset(
                            *(*topo).wrapping_offset((12 + (*i + 1) % 4) as isize) as isize,
                        ));
                    *(*data).z.wrapping_offset(
                        *(*topo).wrapping_offset((*i + 20) as isize) as isize,
                    ) = 0.5
                        * (*(*data).z.wrapping_offset(
                            *(*topo).wrapping_offset((12 + *i) as isize) as isize,
                        ) + *(*data).z.wrapping_offset(
                            *(*topo).wrapping_offset((12 + (*i + 1) % 4) as isize) as isize,
                        ));
                    *i += 1;
                }
                *(*data).x.wrapping_offset(*(*topo).wrapping_offset(26) as isize) = 0.5
                    * (*(*data).x.wrapping_offset(*(*topo).wrapping_offset(0) as isize)
                        + *(*data).x.wrapping_offset(*(*topo).wrapping_offset(6) as isize));
                *(*data).y.wrapping_offset(*(*topo).wrapping_offset(26) as isize) = 0.5
                    * (*(*data).y.wrapping_offset(*(*topo).wrapping_offset(0) as isize)
                        + *(*data).y.wrapping_offset(*(*topo).wrapping_offset(6) as isize));
                *(*data).z.wrapping_offset(*(*topo).wrapping_offset(26) as isize) = 0.5
                    * (*(*data).z.wrapping_offset(*(*topo).wrapping_offset(0) as isize)
                        + *(*data).z.wrapping_offset(*(*topo).wrapping_offset(6) as isize));
            }
            *element += 1;
        }
    }
}
/// SAFETY: extruded 2D boundary conditions (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_bcs(
    dataxy: *mut FemType,
    boundxy: *mut BoundaryType,
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: *mut c_int,
    elemtype: c_int,
    cummaxsidetype: &mut c_int,
    sidetype: &mut c_int,
    minsidetype: &mut c_int,
    maxsidetype: &mut c_int,
    side: &mut c_int,
    level: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    material: &mut c_int,
    material2: &mut c_int,
    ind1: &mut c_int,
    ind2: &mut c_int,
    cellk: &mut c_int,
    k: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    info: c_int,
) {
    *cummaxsidetype = 0;
    *sidetype = 0;
    // NOTE: sides with mismatched post-extrusion materials are dropped.
    *j = 0;
    while *j < (*data).noboundaries {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *maxsidetype = 0;
        *minsidetype = c_int::MAX;
        *side = 0;
        *level = 0;
        *cellk = 1;
        while *cellk <= (*grid).zcells {
            *k = 1;
            while *k <= (*grid).zelems[*cellk as usize] {
                *level += 1;
                *i = 1;
                while *i <= (*boundxy.wrapping_offset(*j as isize)).nosides {
                    // NOTE: parent element and material per level.
                    *ind1 = (*level - 1) * (*dataxy).noelements
                        + *(*boundxy.wrapping_offset(*j as isize))
                            .parent
                            .wrapping_offset(*i as isize);
                    *parent = *indx.wrapping_offset(*ind1 as isize);
                    if *parent != 0 {
                        *material =
                            *(*data).material.wrapping_offset(*parent as isize);
                    } else {
                        *material = 0;
                    }
                    if *(*boundxy.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize)
                        != 0
                    {
                        *ind2 = (*level - 1) * (*dataxy).noelements
                            + *(*boundxy.wrapping_offset(*j as isize))
                                .parent2
                                .wrapping_offset(*i as isize);
                        *parent2 = *indx.wrapping_offset(*ind2 as isize);
                    } else {
                        *parent2 = 0;
                    }
                    if *parent2 != 0 {
                        *material2 =
                            *(*data).material.wrapping_offset(*parent2 as isize);
                    } else {
                        *material2 = 0;
                    }
                    if (*parent != 0 || *parent2 != 0) && *material != *material2 {
                        *side += 1;
                        // NOTE: `if(!parent & !parent2)` always false here; kept.
                        if (*parent == 0) && (*parent2 == 0) {
                            printf(
                                c"no parent = %d %d %d %d %d\n".as_ptr(),
                                *parent,
                                *parent2,
                                *ind1,
                                *ind2,
                                *level,
                            );
                        }
                        *sidetype = *(*boundxy.wrapping_offset(*j as isize))
                            .types
                            .wrapping_offset(*i as isize);
                        *(*bound.wrapping_offset(*j as isize))
                            .types
                            .wrapping_offset(*side as isize) = *sidetype;
                        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
                        if *maxsidetype < *sidetype {
                            *maxsidetype = *sidetype;
                        }
                        if *sidetype < *minsidetype {
                            *minsidetype = *sidetype;
                        }
                        if *parent != 0 {
                            *(*bound.wrapping_offset(*j as isize))
                                .parent
                                .wrapping_offset(*side as isize) = *parent;
                            *(*bound.wrapping_offset(*j as isize))
                                .parent2
                                .wrapping_offset(*side as isize) = *parent2;
                            *(*bound.wrapping_offset(*j as isize))
                                .side
                                .wrapping_offset(*side as isize) = *(*boundxy
                                .wrapping_offset(*j as isize))
                            .side
                            .wrapping_offset(*i as isize);
                            *(*bound.wrapping_offset(*j as isize))
                                .side2
                                .wrapping_offset(*side as isize) = *(*boundxy
                                .wrapping_offset(*j as isize))
                            .side2
                            .wrapping_offset(*i as isize);
                            *(*bound.wrapping_offset(*j as isize))
                                .material
                                .wrapping_offset(*side as isize) = *material;
                        } else {
                            *(*bound.wrapping_offset(*j as isize))
                                .parent
                                .wrapping_offset(*side as isize) = *parent2;
                            *(*bound.wrapping_offset(*j as isize))
                                .parent2
                                .wrapping_offset(*side as isize) = *parent;
                            *(*bound.wrapping_offset(*j as isize))
                                .side
                                .wrapping_offset(*side as isize) = *(*boundxy
                                .wrapping_offset(*j as isize))
                            .side2
                            .wrapping_offset(*i as isize);
                            *(*bound.wrapping_offset(*j as isize))
                                .side2
                                .wrapping_offset(*side as isize) = *(*boundxy
                                .wrapping_offset(*j as isize))
                            .side
                            .wrapping_offset(*i as isize);
                            *(*bound.wrapping_offset(*j as isize))
                                .material
                                .wrapping_offset(*side as isize) = *material2;
                        }
                        // NOTE: 1D sides use the 3-convention.
                        if elemtype == 404 {
                            if *(*bound.wrapping_offset(*j as isize))
                                .side
                                .wrapping_offset(*side as isize)
                                == 0
                            {
                                *(*bound.wrapping_offset(*j as isize))
                                    .side
                                    .wrapping_offset(*side as isize) = 3;
                            }
                            if *(*bound.wrapping_offset(*j as isize))
                                .side2
                                .wrapping_offset(*side as isize)
                                == 0
                            {
                                *(*bound.wrapping_offset(*j as isize))
                                    .side2
                                    .wrapping_offset(*side as isize) = 3;
                            }
                        }
                    }
                    *i += 1;
                }
                *k += 1;
            }
            *cellk += 1;
        }
        (*bound.wrapping_offset(*j as isize)).nosides = *side;
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *cummaxsidetype < *maxsidetype {
            *cummaxsidetype = *maxsidetype;
        }
        if info != 0 {
            if *side != 0 {
                printf(
                    c"Extruded BCs list %d of types [%d,%d] has %d elements.\n".as_ptr(),
                    *j,
                    *minsidetype,
                    *maxsidetype,
                    *side,
                );
            } else {
                printf(c"Extruded BCs list %d has no elements!\n".as_ptr(), *j);
            }
        }
        *j += 1;
    }
}
// NOTE: C `#define MAXNEWBC 200` local to CreateKnotsExtruded.
const MAXNEWBC: c_int = 200;
/// SAFETY: one layer-level BC fill (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_layer_fill(
    dataxy: *mut FemType,
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: *mut c_int,
    indxlength: c_int,
    usenames: c_int,
    layerbcoffset: c_int,
    bcset: c_int,
    bclevel: c_int,
    cellk: c_int,
    level: c_int,
    maxsidetype: &mut c_int,
    minsidetype: &mut c_int,
    refmaterial1: *mut c_int,
    refmaterial2: *mut c_int,
    refsidetype: *mut c_int,
    sideind: *mut c_int,
    sideelemtype: &mut c_int,
    sidetype: &mut c_int,
    swap: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    material: &mut c_int,
    material2: &mut c_int,
    ind1: &mut c_int,
    ind2: &mut c_int,
    side: &mut c_int,
    meanx: &mut Real,
    meany: &mut Real,
    origtype: &mut c_int,
    elemtype: &mut c_int,
    nonodes2d: &mut c_int,
    i: &mut c_int,
    _j: &mut c_int,
    _k: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
    n: &mut c_int,
) {
    *i = 1;
    while *i <= (*dataxy).noelements {
        *origtype = *(*dataxy).elementtypes.wrapping_offset(*i as isize);
        *nonodes2d = *origtype % 100;
        if *origtype == 202 {
            *elemtype = 404;
        } else if *origtype == 303 {
            *elemtype = 706;
        } else if *origtype == 404 {
            *elemtype = 808;
        } else if *origtype == 408 {
            *elemtype = 820;
        } else if *origtype == 409 {
            *elemtype = 827;
        }
        // NOTE: parents on either side of the layer interface.
        *ind1 = (level - 2) * (*dataxy).noelements + *i;
        if *ind1 < 1 {
            *parent = 0;
        } else {
            *parent = *indx.wrapping_offset(*ind1 as isize);
        }
        *ind2 = (level - 1) * (*dataxy).noelements + *i;
        if *ind2 > indxlength {
            *parent2 = 0;
        } else {
            *parent2 = *indx.wrapping_offset(*ind2 as isize);
        }
        // NOTE: single-sided interface swaps to parent-first.
        if *parent == 0 && *parent2 != 0 {
            *parent = *parent2;
            *parent2 = 0;
            *swap = 1;
        } else {
            *swap = 0;
        }
        if *parent == 0 {
            *i += 1;
            continue;
        }
        // NOTE: materials on either side.
        *material = *(*data).material.wrapping_offset(*parent as isize);
        if *parent2 != 0 {
            *material2 = *(*data).material.wrapping_offset(*parent2 as isize);
        } else {
            *material2 = 0;
        }
        if (*grid).rotatecartesian != 0 && *material2 == 0 {
            if *origtype == 303 {
                GetElementSide(*parent, 4 - *swap, 1, data, sideind, sideelemtype);
            } else {
                GetElementSide(*parent, 5 - *swap, 1, data, sideind, sideelemtype);
            }
            *meanx = 0.0;
            *meany = 0.0;
            if cellk % 4 == 2 {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *meanx += *(*data).y.wrapping_offset(
                        *sideind.wrapping_offset(*l as isize) as isize,
                    );
                    *meany += *(*data).z.wrapping_offset(
                        *sideind.wrapping_offset(*l as isize) as isize,
                    );
                    *l += 1;
                }
            } else {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *meanx += *(*data).x.wrapping_offset(
                        *sideind.wrapping_offset(*l as isize) as isize,
                    );
                    *meany += *(*data).z.wrapping_offset(
                        *sideind.wrapping_offset(*l as isize) as isize,
                    );
                    *l += 1;
                }
            }
            *meanx = fabs(*meanx) / (*sideelemtype % 100) as Real;
            *meany = fabs(*meany) / (*sideelemtype % 100) as Real;
            if fabs(*meanx - (*grid).rotateradius1) > 1.0e-12 {
                *material2 = *material;
            } else {
                *m = 0;
                while *m < (*grid).xcells
                    && (*grid).x[*m as usize] + 1.0e-12 < *meanx
                {
                    *m += 1;
                }
                *n = 0;
                while *n < (*grid).ycells
                    && (*grid).y[*n as usize] + 1.0e-12 < *meany
                {
                    *n += 1;
                }
                *material2 = (*grid).structure[*n as usize][(*m + 1) as usize];
            }
        }
        // NOTE: a BC exists only across differing materials.
        if *material != *material2 {
            *side += 1;
            (*bound.wrapping_offset(bcset as isize)).nosides = *side;
            *(*bound.wrapping_offset(bcset as isize))
                .parent
                .wrapping_offset(*side as isize) = *parent;
            *(*bound.wrapping_offset(bcset as isize))
                .parent2
                .wrapping_offset(*side as isize) = *parent2;
            *(*bound.wrapping_offset(bcset as isize))
                .material
                .wrapping_offset(*side as isize) = *material;
            if *origtype == 303 {
                *(*bound.wrapping_offset(bcset as isize))
                    .side
                    .wrapping_offset(*side as isize) = 4 - *swap;
                *(*bound.wrapping_offset(bcset as isize))
                    .side2
                    .wrapping_offset(*side as isize) = 3 + *swap;
            } else {
                *(*bound.wrapping_offset(bcset as isize))
                    .side
                    .wrapping_offset(*side as isize) = 5 - *swap;
                *(*bound.wrapping_offset(bcset as isize))
                    .side2
                    .wrapping_offset(*side as isize) = 4 + *swap;
            }
            // NOTE: deterministic vs continuous BC numbering.
            if layerbcoffset != 0 {
                *sidetype = bclevel * layerbcoffset + *(*dataxy).material.wrapping_offset(*i as isize);
                *(*bound.wrapping_offset(bcset as isize))
                    .types
                    .wrapping_offset(*side as isize) = *sidetype;
                // NOTE: C MAX macro (egdef.h:97); arms pure.
                if *maxsidetype < *sidetype {
                    *maxsidetype = *sidetype;
                }
                // NOTE: C MIN macro (egdef.h:96); arms pure.
                if *sidetype < *minsidetype {
                    *minsidetype = *sidetype;
                }
            } else {
                *m = 0;
                while *m < MAXNEWBC as c_int {
                    if *refmaterial1.wrapping_offset(*m as isize) == *material
                        && *refmaterial2.wrapping_offset(*m as isize) == *material2
                    {
                        break;
                    } else if *refmaterial1.wrapping_offset(*m as isize) == 0
                        && *refmaterial2.wrapping_offset(*m as isize) == 0
                    {
                        *refmaterial1.wrapping_offset(*m as isize) = *material;
                        *refmaterial2.wrapping_offset(*m as isize) = *material2;
                        *sidetype += 1;
                        // NOTE: C MAX macro (egdef.h:97); arms pure.
                        if *maxsidetype < *sidetype {
                            *maxsidetype = *sidetype;
                        }
                        // NOTE: C MIN macro (egdef.h:96); arms pure.
                        if *sidetype < *minsidetype {
                            *minsidetype = *sidetype;
                        }
                        *refsidetype.wrapping_offset(*m as isize) = *sidetype;
                        break;
                    } else if *m == MAXNEWBC as c_int - 1 {
                        printf(
                            c"Layer includes more than %d new BCs!\n".as_ptr(),
                            MAXNEWBC as c_int,
                        );
                    }
                    *m += 1;
                }
                *l = *refsidetype.wrapping_offset(*m as isize);
                *(*bound.wrapping_offset(bcset as isize))
                    .types
                    .wrapping_offset(*side as isize) = *l;
                if usenames != 0 {
                    if (*data).boundaryname[*l as usize].is_null() {
                        (*data).boundaryname[*l as usize] =
                            cvector(0, MAXNAMESIZE as c_int);
                    }
                    if bclevel == 1 {
                        sprintf(
                            (*data).boundaryname[*l as usize],
                            c"%s%s".as_ptr(),
                            (*dataxy).bodyname
                                [*(*dataxy).material.wrapping_offset(*i as isize) as usize],
                            c"_Start".as_ptr(),
                        );
                    } else if cellk == (*grid).zcells {
                        sprintf(
                            (*data).boundaryname[*l as usize],
                            c"%s%s".as_ptr(),
                            (*dataxy).bodyname
                                [*(*dataxy).material.wrapping_offset(*i as isize) as usize],
                            c"_End".as_ptr(),
                        );
                    } else {
                        sprintf(
                            (*data).boundaryname[*l as usize],
                            c"%s%s%d".as_ptr(),
                            (*dataxy).bodyname
                                [*(*dataxy).material.wrapping_offset(*i as isize) as usize],
                            c"_Level".as_ptr(),
                            bclevel,
                        );
                    }
                }
            }
        }
        *i += 1;
    }
}
/// SAFETY: inter-layer BC driver with redo (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_layers(
    dataxy: *mut FemType,
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: *mut c_int,
    indxlength: c_int,
    usenames: c_int,
    layerbcoffset: c_int,
    bcset: &mut c_int,
    _cummaxsidetype: &mut c_int,
    sidetype: &mut c_int,
    maxsidetype: &mut c_int,
    minsidetype: &mut c_int,
    bclevel: &mut c_int,
    refmaterial1: *mut c_int,
    refmaterial2: *mut c_int,
    refsidetype: *mut c_int,
    sideind: *mut c_int,
    sideelemtype: &mut c_int,
    swap: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    material: &mut c_int,
    material2: &mut c_int,
    ind1: &mut c_int,
    ind2: &mut c_int,
    side: &mut c_int,
    level: &mut c_int,
    meanx: &mut Real,
    meany: &mut Real,
    origtype: &mut c_int,
    elemtype: &mut c_int,
    nonodes2d: &mut c_int,
    cellk: &mut c_int,
    k: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
    n: &mut c_int,
    info: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut swap_loc: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut redo: c_int;
    let _ = swap_loc;
    *bcset = (*dataxy).noboundaries - 1;
    if (*grid).layerbcoffset == 0 {
        *sidetype = *maxsidetype;
    }
    // NOTE: BCs between layers.
    if (*grid).dimension == 3 || (*grid).rotatecartesian != 0 {
        *side = 0;
        *level = 0;
        *bclevel = 0;
        // NOTE: extruded cells.
        *cellk = 1;
        while *cellk <= (*grid).zcells {
            redo = FALSE;
            // NOTE: C `redolayer:` label + `goto redolayer`.
            'redolayer: loop {
                *maxsidetype = 0;
                *minsidetype = c_int::MAX;
                // NOTE: element layers within the cell.
                *k = 1;
                while *k <= (*grid).zelems[*cellk as usize] {
                    *level += 1;
                    if *k != 1
                        && !(*cellk == (*grid).zcells
                            && *k == (*grid).zelems[*cellk as usize])
                    {
                        *k += 1;
                        continue;
                    }
                    // NOTE: last single-layer cell yields two BCs.
                    if *cellk == (*grid).zcells && *k == (*grid).zelems[*cellk as usize] {
                        if (*grid).zelems[*cellk as usize] == 1 {
                            redo = TRUE;
                        } else {
                            *level += 1;
                        }
                    }
                    if (*grid).rotatecartesian != 0 && *cellk % 2 == 1 {
                        *k += 1;
                        continue;
                    }
                    if (*grid).rotatecartesian != 0 && *k != 1 {
                        *k += 1;
                        continue;
                    }
                    // NOTE: deterministic vs continuous BC numbering init.
                    if layerbcoffset == 0 {
                        *m = 0;
                        while *m < MAXNEWBC {
                            *refmaterial1.wrapping_offset(*m as isize) = 0;
                            *refmaterial2.wrapping_offset(*m as isize) = 0;
                            *refsidetype.wrapping_offset(*m as isize) = 0;
                            *m += 1;
                        }
                    }
                    *side = 0;
                    *bcset += 1;
                    *bclevel += 1;
                    *maxsidetype = 0;
                    *minsidetype = c_int::MAX;
                    extrude_layer_fill(
                        dataxy, grid, data, bound, indx, indxlength, usenames,
                        layerbcoffset, *bcset, *bclevel, *cellk, *level, maxsidetype,
                        minsidetype, refmaterial1, refmaterial2, refsidetype, sideind,
                        sideelemtype, sidetype, swap, parent, parent2, material,
                        material2, ind1, ind2, side, meanx, meany, origtype, elemtype,
                        nonodes2d, i, j, k, l, m, n,
                    );
                    if info != 0 {
                        if *side != 0 {
                            printf(
                                c"Layer BCs list %d of types [%d,%d] has %d elements.\n".as_ptr(),
                                *bcset,
                                *minsidetype,
                                *maxsidetype,
                                *side,
                            );
                        } else {
                            printf(
                                c"Layer BCs list %d has no elements!\n".as_ptr(),
                                *bcset,
                            );
                        }
                    }
                    if redo == TRUE {
                        continue 'redolayer;
                    }
                    *k += 1;
                }
                break;
            }
            *cellk += 1;
        }
    }
}
/// SAFETY: partial-rotation symmetry BCs (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_symmetry(
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    sideind: *mut c_int,
    sideelemtype: &mut c_int,
    bcset: &mut c_int,
    sidetype: c_int,
    element: &mut c_int,
    side: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    info: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let mut _o: c_int = 0;
    let mut _p: c_int = 0;
    // NOTE: write-only counters (never read in C either).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxradi: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut addtype: c_int;
    // NOTE: `=1.0e-3` decl init kept (read before any overwrite).
    let eps: Real = 1.0e-3;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut fii: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut rad: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut meanrad: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxrad: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xc: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut yc: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dfii: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut fii0: Real;
    let mut rads: [Real; 4] = [0.0; 4];
    let mut fiis: [Real; 4] = [0.0; 4];
    *bcset += 1;
    _o = 0;
    _p = 0;
    let blocks: c_int = (*grid).rotateblocks;
    *element = 1;
    while *element <= (*data).noelements {
        *side = 0;
        while *side < 6 {
            GetElementSide(*element, *side, 1, data, sideind, sideelemtype);
            meanrad = 0.0;
            maxrad = 0.0;
            maxradi = 0;
            *j = *sideelemtype / 100;
            if *j == 1 {
                break;
            }
            *i = 0;
            while *i < *j {
                xc = *(*data).x.wrapping_offset(*sideind.wrapping_offset(*i as isize) as isize);
                yc = *(*data).y.wrapping_offset(*sideind.wrapping_offset(*i as isize) as isize);
                rad = sqrt(yc * yc + xc * xc);
                fii = 2.0 * atan2(yc, xc) / FM_PI;
                rads[*i as usize] = rad;
                fiis[*i as usize] = fii;
                if rad > maxrad {
                    maxrad = rad;
                    maxradi = *i;
                }
                meanrad += rad / *j as Real;
                *i += 1;
            }
            fii0 = fiis[maxradi as usize];
            dfii = 0.0;
            *i = 0;
            while *i < 4 {
                if rads[*i as usize] > eps * maxrad
                    && fabs(fiis[*i as usize] - fii0) > dfii
                {
                    dfii = fabs(fiis[*i as usize] - fii0);
                }
                *i += 1;
            }
            if dfii > eps {
                *side += 1;
                continue;
            }
            addtype = -1;
            // NOTE: zero-angle BCs.
            if fabs(fii0) < eps {
                _o += 1;
                if meanrad < (*grid).rotateradius2 {
                    addtype = 0;
                } else {
                    addtype = 2;
                }
            } else if fabs(fii0 - blocks as Real) < eps {
                // NOTE: BCs for angles 90, 180 or 270.
                _p += 1;
                if meanrad < (*grid).rotateradius2 {
                    addtype = 1;
                } else {
                    addtype = 3;
                }
            }
            if addtype >= 0 {
                (*bound.wrapping_offset((*bcset + addtype) as isize)).nosides += 1;
                *k = (*bound.wrapping_offset((*bcset + addtype) as isize)).nosides;
                *(*bound.wrapping_offset((*bcset + addtype) as isize))
                    .side
                    .wrapping_offset(*k as isize) = *side;
                *(*bound.wrapping_offset((*bcset + addtype) as isize))
                    .parent
                    .wrapping_offset(*k as isize) = *element;
                *(*bound.wrapping_offset((*bcset + addtype) as isize))
                    .types
                    .wrapping_offset(*k as isize) = sidetype + addtype + 1;
            }
            *side += 1;
        }
        *element += 1;
    }
    addtype = 0;
    while addtype < 4 {
        *l = *bcset + addtype;
        if (*bound.wrapping_offset(*l as isize)).nosides == 0 {
            (*bound.wrapping_offset(*l as isize)).created = FALSE;
        } else {
            (*bound.wrapping_offset(*l as isize)).created = TRUE;
            if info != 0 {
                if (*bound.wrapping_offset(*l as isize)).nosides != 0 {
                    printf(
                        c"Symmetry BCs list %d of type %d has %d elements.\n".as_ptr(),
                        *l,
                        sidetype + addtype + 1,
                        (*bound.wrapping_offset(*l as isize)).nosides,
                    );
                } else {
                    printf(
                        c"Symmetry BCs list %d has no elements!\n".as_ptr(),
                        *l,
                    );
                }
            }
        }
        addtype += 1;
    }
    *bcset += 4;
    let _ = blocks;
}
/// SAFETY: renumber used knots densely (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn extrude_compact(
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    indx: *mut c_int,
    newx: &mut *mut Real,
    newy: &mut *mut Real,
    newz: &mut *mut Real,
    corder: *mut Real,
    nonodes3d: &mut c_int,
    element: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    info: c_int,
) {
    // NOTE: used-knot census.
    *i = 1;
    while *i <= (*data).noknots {
        *indx.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *element = 1;
    while *element <= (*data).noelements {
        *nonodes3d = *(*data).elementtypes.wrapping_offset(*element as isize) % 100;
        *i = 0;
        while *i < *nonodes3d {
            *indx.wrapping_offset(
                *(*(*data).topology.wrapping_offset(*element as isize))
                    .wrapping_offset(*i as isize) as isize,
            ) = 1;
            *i += 1;
        }
        *element += 1;
    }
    *j = 0;
    *i = 1;
    while *i <= (*data).noknots {
        if *indx.wrapping_offset(*i as isize) != 0 {
            *j += 1;
            *indx.wrapping_offset(*i as isize) = *j;
        }
        *i += 1;
    }
    if *j < (*data).noknots {
        printf(
            c"%d original nodes moved to %d new ones.\n".as_ptr(),
            (*data).noknots,
            *j,
        );
        *newx = dvector(1, *j);
        *i = 1;
        while *i <= (*data).noknots {
            *(*newx).wrapping_offset(*indx.wrapping_offset(*i as isize) as isize) =
                *(*data).x.wrapping_offset(*i as isize);
            *i += 1;
        }
        *newy = (*data).x;
        (*data).x = *newx;
        *i = 1;
        while *i <= (*data).noknots {
            *(*newy).wrapping_offset(*indx.wrapping_offset(*i as isize) as isize) =
                *(*data).y.wrapping_offset(*i as isize);
            *i += 1;
        }
        *newz = (*data).y;
        (*data).y = *newy;
        *i = 1;
        while *i <= (*data).noknots {
            *(*newz).wrapping_offset(*indx.wrapping_offset(*i as isize) as isize) =
                *(*data).z.wrapping_offset(*i as isize);
            *i += 1;
        }
        free_dvector((*data).z, 1, (*data).noknots);
        (*data).z = *newz;
        (*data).noknots = *j;
        *element = 1;
        while *element <= (*data).noelements {
            *nonodes3d =
                *(*data).elementtypes.wrapping_offset(*element as isize) % 100;
            *i = 0;
            while *i < *nonodes3d {
                *(*(*data).topology.wrapping_offset(*element as isize))
                    .wrapping_offset(*i as isize) = *indx.wrapping_offset(
                    *(*(*data).topology.wrapping_offset(*element as isize))
                        .wrapping_offset(*i as isize) as isize,
                );
                *i += 1;
            }
            *element += 1;
        }
    }
    if (*grid).rotate != 0 {
        ReorderElements(data, bound, FALSE, corder, info);
        CylindricalCoordinateImprove(
            data,
            (*grid).rotateimprove,
            (*grid).rotateradius1,
            (*grid).rotateradius2,
        );
        // NOTE: `if(0 && ...)` curve call omitted (dead in C).
        if (*grid).rotatecartesian != 0 {
            SeparateMainaxisBoundaries(data, bound);
        }
        printf(
            c"Created %d elements and %d nodes by rotation of %d degrees.\n".as_ptr(),
            (*data).noelements,
            (*data).noknots,
            90 * (*grid).rotateblocks,
        );
    } else if (*grid).dimension == 3 && info != 0 {
        printf(
            c"Created %d elements and %d nodes by extruding the 2D geometry\n".as_ptr(),
            (*data).noelements,
            (*data).noknots,
        );
    }
}

/// SAFETY: constant-helicity twist (split for sizing).
unsafe fn extrude_helicity(
    grid: *mut GridType,
    data: *mut FemType,
    i: &mut c_int,
    info: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let helicity: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut fii: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxz: Real;
    // NOTE: constant helicity twist on request.
    if (*grid).zhelicityexists != 0 {
        helicity = (FM_PI / 180.0) * (*grid).zhelicity;
        minz = *(*data).z.wrapping_offset(1);
        maxz = *(*data).z.wrapping_offset(1);
        *i = 1;
        while *i <= (*data).noknots {
            // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
            if *(*data).z.wrapping_offset(*i as isize) < minz {
                minz = *(*data).z.wrapping_offset(*i as isize);
            }
            if maxz < *(*data).z.wrapping_offset(*i as isize) {
                maxz = *(*data).z.wrapping_offset(*i as isize);
            }
            *i += 1;
        }
        *i = 1;
        while *i <= (*data).noknots {
            x = *(*data).x.wrapping_offset(*i as isize);
            y = *(*data).y.wrapping_offset(*i as isize);
            z = *(*data).z.wrapping_offset(*i as isize);
            fii = helicity * (z - minz) / (maxz - minz);
            *(*data).x.wrapping_offset(*i as isize) = cos(fii) * x - sin(fii) * y;
            *(*data).y.wrapping_offset(*i as isize) = sin(fii) * x + cos(fii) * y;
            *i += 1;
        }
        if info != 0 {
            printf(
                c"Applied helicity of %12.5le degrees\n".as_ptr(),
                (*grid).zhelicity,
            );
        }
    }
}
/// Extrude (or rotate) a 2D mesh into 3D, with boundaries.
///
/// # Safety
/// `dataxy`, `boundxy`, `grid`, `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateKnotsExtruded(
    dataxy: *mut FemType,
    boundxy: *mut BoundaryType,
    grid: *mut GridType,
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) {
    let mut i: c_int = 0;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut m: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut n: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut knot0: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut knot1: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut knot2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elem0: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut size: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut kmax: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: main-scope noknots unused (helpers own their copies).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut origtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonodes3d: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonodes2d: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bclevel: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bcset: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut cellk: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut element: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut level: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut layers: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut material_too_large: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut material: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut material2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind1: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind2: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut indx: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut topo: *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sidetype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut minsidetype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxsidetype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut cummaxsidetype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newbounds: c_int = 0;
    let mut refmaterial1: [c_int; MAXNEWBC as usize] = [0; MAXNEWBC as usize];
    let mut refmaterial2: [c_int; MAXNEWBC as usize] = [0; MAXNEWBC as usize];
    let mut refsidetype: [c_int; MAXNEWBC as usize] = [0; MAXNEWBC as usize];
    // NOTE: assigned before every read (C uninit decl shape).
    let mut indxlength: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut z: Real = 0.0;
    // NOTE: null-init (Rvector before guarded reads).
    let mut newx: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (Rvector before guarded reads).
    let mut newy: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (Rvector before guarded reads).
    let mut newz: *mut Real = core::ptr::null_mut();
    let mut corder: [Real; 3] = [0.0; 3];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut meanx: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut meany: Real = 0.0;
    // NOTE: assigned before every read (C `=0` decl init shape).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut usenames: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut swap: c_int = 0;
    if !extrude_init(
        dataxy, boundxy, grid, data, bound, &mut indx, &mut indxlength, &mut elemtype,
        &mut origtype, &mut nonodes3d, &mut nonodes2d, &mut layers, &mut newbounds,
        &mut usenames, &mut maxsidetype, &mut i, &mut j, &mut size, info,
    ) {
        return;
    }
    extrude_topology(
        dataxy, grid, data, indx, layers, &mut knot0, &mut knot1, &mut knot2,
        &mut elem0, &mut level, &mut material_too_large, &mut nonodes2d, &mut material,
        &mut meanx, &mut cellk, &mut kmax, &mut k, &mut element, &mut origtype,
        &mut elemtype, &mut i,
    );
    (*data).noelements = elem0;
    printf(
        c"Extruded mesh has %d elements in %d levels.\n".as_ptr(),
        elem0,
        level,
    );
    printf(
        c"Simple extrusion would have %d elements\n".as_ptr(),
        level * (*dataxy).noelements,
    );
    if material_too_large > 0 {
        printf(
            c"Material index exceeded %d the size of material permutation table (%d)!\n".as_ptr(),
            material_too_large,
            (*grid).maxmaterial,
        );
        printf(c"Give the max material with > Extruded Max Material < , if needed\n".as_ptr());
    }
    if elem0 == 0 {
        bigerror(c"No use to continue with zero elements!".as_ptr());
    }
    extrude_coords(
        dataxy, grid, data, layers, elemtype, &mut knot0, &mut z, &mut topo,
        &mut cellk, &mut k, &mut element, &mut i,
    );
    // NOTE: cylindrical rotation of the extruded mesh.
    if (*grid).rotate != 0 {
        CylindricalCoordinateTransformation(
            data,
            (*grid).rotateradius1,
            (*grid).rotateradius2,
            (*grid).rotatecartesian,
        );
    }
    extrude_bcs(
        dataxy, boundxy, grid, data, bound, indx, elemtype, &mut cummaxsidetype,
        &mut sidetype, &mut minsidetype, &mut maxsidetype, &mut side, &mut level,
        &mut parent, &mut parent2, &mut material, &mut material2, &mut ind1,
        &mut ind2, &mut cellk, &mut k, &mut i, &mut j, info,
    );
    bcset = (*dataxy).noboundaries - 1;
    if usenames != 0 {
        i = 1;
        while i < MAXBODIES as c_int {
            if !(*dataxy).bodyname[i as usize].is_null() {
                if (*data).bodyname[i as usize].is_null() {
                    (*data).bodyname[i as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy((*data).bodyname[i as usize], (*dataxy).bodyname[i as usize]);
            }
            i += 1;
        }
        i = 1;
        while i < MAXBCS as c_int {
            if !(*dataxy).boundaryname[i as usize].is_null() {
                if (*data).boundaryname[i as usize].is_null() {
                    (*data).boundaryname[i as usize] = cvector(0, MAXNAMESIZE as c_int);
                }
                strcpy(
                    (*data).boundaryname[i as usize],
                    (*dataxy).boundaryname[i as usize],
                );
            }
            i += 1;
        }
        (*data).bodynamesexist = TRUE;
        (*data).boundarynamesexist = TRUE;
    }
    // NOTE: BCs between layers, numbered by appearance.
    let layerbcoffset: c_int = (*grid).layerbcoffset;
    if (*grid).layeredbc != 0 {
        extrude_layers(
            dataxy, grid, data, bound, indx, indxlength, usenames, layerbcoffset,
            &mut bcset, &mut cummaxsidetype, &mut sidetype, &mut maxsidetype,
            &mut minsidetype, &mut bclevel, refmaterial1.as_mut_ptr(),
            refmaterial2.as_mut_ptr(), refsidetype.as_mut_ptr(), sideind.as_mut_ptr(),
            &mut sideelemtype, &mut swap, &mut parent, &mut parent2, &mut material,
            &mut material2, &mut ind1, &mut ind2, &mut side, &mut level, &mut meanx,
            &mut meany, &mut origtype, &mut elemtype, &mut nonodes2d, &mut cellk,
            &mut k, &mut i, &mut j, &mut l, &mut m, &mut n, info,
        );
    }
    // NOTE: symmetry BCs for partial rotation.
    bcset += 1;
    if (*grid).rotate != 0 && (*grid).rotateblocks < 4 {
        extrude_symmetry(
            grid, data, bound, sideind.as_mut_ptr(), &mut sideelemtype, &mut bcset,
            sidetype, &mut element, &mut side, &mut i, &mut j, &mut k, &mut l, info,
        );
    }
    (*data).noboundaries = bcset + 1;
    extrude_compact(
        grid, data, bound, indx, &mut newx, &mut newy, &mut newz, corder.as_mut_ptr(),
        &mut nonodes3d, &mut element, &mut i, &mut j, info,
    );
    free_ivector(indx, 0, indxlength);
    extrude_helicity(grid, data, &mut i, info);
    let _ = (topo, newbounds, size, kmax);
}
/// Drop mid-edge nodes of elements in a material interval.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn ReduceElementOrder(data: *mut FemType, matmin: c_int, matmax: c_int) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut element: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut material: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemcode1: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemcode2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxnode: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut reduced: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    let indx: *mut c_int = ivector(0, (*data).noknots);
    i = 0;
    while i <= (*data).noknots {
        *indx.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    reduced = 0;
    element = 1;
    while element <= (*data).noelements {
        elemcode1 = *(*data).elementtypes.wrapping_offset(element as isize);
        material = *(*data).material.wrapping_offset(element as isize);
        elemcode2 = elemcode1;
        if material >= matmin && material <= matmax {
            elemcode2 = 101 * (elemcode1 / 100);
        }
        // NOTE: center-node types collapse to the plain ones.
        if elemcode2 == 505 {
            elemcode2 = 504;
        } else if elemcode2 == 606 {
            elemcode2 = 605;
        } else if elemcode2 == 707 {
            elemcode2 = 706;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
        if elemcode2 < elemcode1 {
            reduced += 1;
        }
        maxnode = elemcode2 % 100;
        i = 0;
        while i < maxnode {
            *indx.wrapping_offset(
                *(*(*data).topology.wrapping_offset(element as isize))
                    .wrapping_offset(i as isize) as isize,
            ) = 1;
            i += 1;
        }
        *(*data).elementtypes.wrapping_offset(element as isize) = elemcode2;
        element += 1;
    }
    printf(
        c"The element order is reduced in %d elements at interval [%d,%d]\n".as_ptr(),
        reduced,
        matmin,
        matmax,
    );
    j = 0;
    i = 1;
    while i <= (*data).noknots {
        if *indx.wrapping_offset(i as isize) != 0 {
            j += 1;
            *indx.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    printf(c"%d original nodes moved to %d new ones.\n".as_ptr(), (*data).noknots, j);
    let newx: *mut Real = dvector(1, j);
    let newy: *mut Real = dvector(1, j);
    let newz: *mut Real = dvector(1, j);
    i = 1;
    while i <= (*data).noknots {
        *newx.wrapping_offset(*indx.wrapping_offset(i as isize) as isize) =
            *(*data).x.wrapping_offset(i as isize);
        *newy.wrapping_offset(*indx.wrapping_offset(i as isize) as isize) =
            *(*data).y.wrapping_offset(i as isize);
        *newz.wrapping_offset(*indx.wrapping_offset(i as isize) as isize) =
            *(*data).z.wrapping_offset(i as isize);
        i += 1;
    }
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    (*data).noknots = j;
    element = 1;
    while element <= (*data).noelements {
        maxnode = *(*data).elementtypes.wrapping_offset(element as isize) % 100;
        i = 0;
        while i < maxnode {
            *(*(*data).topology.wrapping_offset(element as isize))
                .wrapping_offset(i as isize) = *indx.wrapping_offset(
                *(*(*data).topology.wrapping_offset(element as isize))
                    .wrapping_offset(i as isize) as isize,
            );
            i += 1;
        }
        element += 1;
    }
}

/// SAFETY: close-knot merge scan (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn merge_scan(
    data: *mut FemType,
    mergeindx: *mut c_int,
    doubles: *mut c_int,
    noknots: c_int,
    newnoknots: &mut c_int,
    cx: Real,
    cy: Real,
    cz: Real,
    eps: Real,
    dx: &mut Real,
    dy: &mut Real,
    dz: &mut Real,
    dist: &mut Real,
    i: &mut c_int,
    j: &mut c_int,
) {
    *dz = 0.0;
    *i = 1;
    while *i < noknots {
        if *mergeindx.wrapping_offset(*i as isize) != 0 {
            *i += 1;
            continue;
        }
        *j = *i + 1;
        while *j <= noknots {
            if *mergeindx.wrapping_offset(*j as isize) != 0 {
                *j += 1;
                continue;
            }
            *dx = *(*data).x.wrapping_offset(*i as isize)
                - *(*data).x.wrapping_offset(*j as isize);
            *dy = *(*data).y.wrapping_offset(*i as isize)
                - *(*data).y.wrapping_offset(*j as isize);
            *dz = *(*data).z.wrapping_offset(*i as isize)
                - *(*data).z.wrapping_offset(*j as isize);
            if fabs(cx * *dx + cy * *dy + cz * *dz) > eps {
                break;
            }
            *dist = *dx * *dx + *dy * *dy + *dz * *dz;
            if *dist < eps * eps {
                *doubles.wrapping_offset(*i as isize) = TRUE;
                *doubles.wrapping_offset(*j as isize) = TRUE;
                *mergeindx.wrapping_offset(*j as isize) = -*i;
                *newnoknots -= 1;
            }
            *j += 1;
        }
        *i += 1;
    }
}

/// Merge knots closer than `eps` (after reordering along `corder`).
///
/// # Safety
/// `data`, `bound`, `corder` valid.
#[no_mangle]
pub unsafe extern "C" fn MergeElements(
    data: *mut FemType,
    bound: *mut BoundaryType,
    manual: c_int,
    corder: *mut Real,
    eps: Real,
    mergebounds: c_int,
    info: c_int,
) {
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set in merge_scan before reads.)
    let mut j: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    // NOTE: C `l` write-only here (never read); dropped.
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newnoknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dx: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dy: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dz: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dist: Real = 0.0;
    ReorderElements(data, bound, manual, corder, TRUE);
    // NOTE: known order along corder[] speeds up the search.
    cx = *corder.wrapping_offset(0);
    cy = *corder.wrapping_offset(1);
    cz = *corder.wrapping_offset(2);
    // NOTE: normalize for the projection test.
    let cdist: Real = sqrt(cx * cx + cy * cy + cz * cz);
    cx /= cdist;
    cy /= cdist;
    cz /= cdist;
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    newnoknots = noknots;
    let mergeindx: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *mergeindx.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    let doubles: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *doubles.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    if info != 0 {
        printf(c"Merging nodes close (%.3lg) to one another.\n".as_ptr(), eps);
    }
    merge_scan(
        data, mergeindx, doubles, noknots, &mut newnoknots, cx, cy, cz, eps, &mut dx,
        &mut dy, &mut dz, &mut dist, &mut i, &mut j,
    );
    if mergebounds != 0 {
        MergeBoundaries(data, bound, doubles, info);
    }
    j = 0;
    i = 1;
    while i <= noknots {
        if *mergeindx.wrapping_offset(i as isize) == 0 {
            j += 1;
            *mergeindx.wrapping_offset(i as isize) = j;
        }
        i += 1;
    }
    i = 1;
    while i <= noknots {
        if *mergeindx.wrapping_offset(i as isize) < 0 {
            *mergeindx.wrapping_offset(i as isize) =
                *mergeindx.wrapping_offset(-*mergeindx.wrapping_offset(i as isize) as isize);
        }
        i += 1;
    }
    printf(c"%d original nodes merged to %d new nodes.\n".as_ptr(), noknots, newnoknots);
    let newx: *mut Real = dvector(1, newnoknots);
    let newy: *mut Real = dvector(1, newnoknots);
    let newz: *mut Real = dvector(1, newnoknots);
    i = 1;
    while i <= noknots {
        *newx.wrapping_offset(*mergeindx.wrapping_offset(i as isize) as isize) =
            *(*data).x.wrapping_offset(i as isize);
        *newy.wrapping_offset(*mergeindx.wrapping_offset(i as isize) as isize) =
            *(*data).y.wrapping_offset(i as isize);
        *newz.wrapping_offset(*mergeindx.wrapping_offset(i as isize) as isize) =
            *(*data).z.wrapping_offset(i as isize);
        i += 1;
    }
    free_dvector((*data).x, 1, (*data).noknots);
    free_dvector((*data).y, 1, (*data).noknots);
    free_dvector((*data).z, 1, (*data).noknots);
    (*data).x = newx;
    (*data).y = newy;
    (*data).z = newz;
    // NOTE: `#if 0` trace omitted (dead in C).
    j = 1;
    while j <= noelements {
        nonodes = *(*data).elementtypes.wrapping_offset(j as isize) % 100;
        i = 0;
        while i < nonodes {
            k = *(*(*data).topology.wrapping_offset(j as isize))
                .wrapping_offset(i as isize);
            *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                *mergeindx.wrapping_offset(k as isize);
            i += 1;
        }
        j += 1;
    }
    (*data).noknots = newnoknots;
    free_ivector(mergeindx, 1, noknots);
    if info != 0 {
        printf(c"Merging of nodes is complete.\n".as_ptr());
    }
}

/// Drop boundary sides whose every node merged away.
///
/// # Safety
/// `data`, `bound`, `doubles` valid.
#[no_mangle]
pub unsafe extern "C" fn MergeBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    doubles: *mut c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i2: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totsides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut newsides: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidenodes: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut side: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut parent: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    totsides = 0;
    newsides = 0;
    if info != 0 {
        printf(c"Eliminating boundaries at joined nodes\n".as_ptr());
    }
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
        i2 = 0;
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            parent = *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize);
            side = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            GetElementSide(parent, side, 1, data, sideind.as_mut_ptr(), &mut sideelemtype);
            sidenodes = sideelemtype % 100;
            l = 0;
            k = 0;
            while k < sidenodes {
                if *doubles.wrapping_offset(sideind[k as usize] as isize) != 0 {
                    l += 1;
                }
                k += 1;
            }
            if l < sidenodes {
                i2 += 1;
                if i != i2 {
                    *(*bound.wrapping_offset(j as isize))
                        .parent
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .parent
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .parent2
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .parent2
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .side
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .side
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .side2
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .side2
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .types
                        .wrapping_offset(i as isize);
                    *(*bound.wrapping_offset(j as isize))
                        .normal
                        .wrapping_offset(i2 as isize) = *(*bound.wrapping_offset(j as isize))
                        .normal
                        .wrapping_offset(i as isize);
                }
            }
            i += 1;
        }
        totsides += (*bound.wrapping_offset(j as isize)).nosides;
        newsides += i2;
        (*bound.wrapping_offset(j as isize)).nosides = i2;
        if i2 == 0 {
            (*bound.wrapping_offset(j as isize)).created = FALSE;
        }
        j += 1;
    }
    if info != 0 {
        printf(
            c"Eliminated %d boundaries from original set of %d.\n".as_ptr(),
            totsides - newsides,
            totsides,
        );
    }
    0
}
/// SAFETY: straighten mid-edge nodes of one element (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn isoparam_elem(
    x: *mut Real,
    y: *mut Real,
    topo: *mut c_int,
    bcindx: *mut c_int,
    elemtype: c_int,
    i: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let xmean: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let ymean: Real;
    if elemtype == 306 {
        *i = 0;
        while *i < 3 {
            if *bcindx.wrapping_offset(
                *topo.wrapping_offset((*i + 3) as isize) as isize,
            ) == 0
            {
                *x.wrapping_offset(*topo.wrapping_offset((*i + 3) as isize) as isize) = 0.5
                    * (*x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        ));
                *y.wrapping_offset(*topo.wrapping_offset((*i + 3) as isize) as isize) = 0.5
                    * (*y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        ));
            }
            *i += 1;
        }
    } else if elemtype == 310 {
        *i = 0;
        while *i < 3 {
            if *bcindx.wrapping_offset(
                *topo.wrapping_offset((2 * *i + 3) as isize) as isize,
            ) == 0
            {
                *x.wrapping_offset(*topo.wrapping_offset((2 * *i + 3) as isize) as isize) =
                    (2.0 * *x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 1.0 * *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        )) / 3.0;
                *x.wrapping_offset(*topo.wrapping_offset((2 * *i + 4) as isize) as isize) =
                    (1.0 * *x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 2.0 * *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        )) / 3.0;
                *y.wrapping_offset(*topo.wrapping_offset((2 * *i + 3) as isize) as isize) =
                    (2.0 * *y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 1.0 * *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        )) / 3.0;
                *y.wrapping_offset(*topo.wrapping_offset((2 * *i + 4) as isize) as isize) =
                    (1.0 * *y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 2.0 * *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 3) as isize) as isize,
                        )) / 3.0;
            }
            *i += 1;
        }
        *x.wrapping_offset(*topo.wrapping_offset(9) as isize) =
            (*x.wrapping_offset(*topo.wrapping_offset(0) as isize)
                + *x.wrapping_offset(*topo.wrapping_offset(1) as isize)
                + *x.wrapping_offset(*topo.wrapping_offset(2) as isize))
                / 3.0;
        *y.wrapping_offset(*topo.wrapping_offset(9) as isize) =
            (*y.wrapping_offset(*topo.wrapping_offset(0) as isize)
                + *y.wrapping_offset(*topo.wrapping_offset(1) as isize)
                + *y.wrapping_offset(*topo.wrapping_offset(2) as isize))
                / 3.0;
    } else if elemtype == 408 || elemtype == 409 {
        *i = 0;
        while *i < 4 {
            if *bcindx.wrapping_offset(
                *topo.wrapping_offset((*i + 4) as isize) as isize,
            ) == 0
            {
                *x.wrapping_offset(*topo.wrapping_offset((*i + 4) as isize) as isize) = 0.5
                    * (*x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        ));
                *y.wrapping_offset(*topo.wrapping_offset((*i + 4) as isize) as isize) = 0.5
                    * (*y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        ));
            }
            *i += 1;
        }
        if elemtype == 409 {
            *x.wrapping_offset(*topo.wrapping_offset(8) as isize) = 0.25
                * (*x.wrapping_offset(*topo.wrapping_offset(0) as isize)
                    + *x.wrapping_offset(*topo.wrapping_offset(1) as isize)
                    + *x.wrapping_offset(*topo.wrapping_offset(2) as isize)
                    + *x.wrapping_offset(*topo.wrapping_offset(3) as isize));
            *y.wrapping_offset(*topo.wrapping_offset(8) as isize) = 0.25
                * (*y.wrapping_offset(*topo.wrapping_offset(0) as isize)
                    + *y.wrapping_offset(*topo.wrapping_offset(1) as isize)
                    + *y.wrapping_offset(*topo.wrapping_offset(2) as isize)
                    + *y.wrapping_offset(*topo.wrapping_offset(3) as isize));
        }
    } else if elemtype == 412 || elemtype == 416 {
        *i = 0;
        while *i < 4 {
            if *bcindx.wrapping_offset(
                *topo.wrapping_offset((2 * *i + 4) as isize) as isize,
            ) == 0
            {
                *x.wrapping_offset(*topo.wrapping_offset((2 * *i + 4) as isize) as isize) =
                    (2.0 * *x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 1.0 * *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        )) / 3.0;
                *x.wrapping_offset(*topo.wrapping_offset((2 * *i + 5) as isize) as isize) =
                    (1.0 * *x.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 2.0 * *x.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        )) / 3.0;
                *y.wrapping_offset(*topo.wrapping_offset((2 * *i + 4) as isize) as isize) =
                    (2.0 * *y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 1.0 * *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        )) / 3.0;
                *y.wrapping_offset(*topo.wrapping_offset((2 * *i + 5) as isize) as isize) =
                    (1.0 * *y.wrapping_offset(*topo.wrapping_offset(*i as isize) as isize)
                        + 2.0 * *y.wrapping_offset(
                            *topo.wrapping_offset(((*i + 1) % 4) as isize) as isize,
                        )) / 3.0;
            }
            *i += 1;
        }
        if elemtype == 416 {
            xmean = (*x.wrapping_offset(*topo.wrapping_offset(0) as isize)
                + *x.wrapping_offset(*topo.wrapping_offset(1) as isize)
                + *x.wrapping_offset(*topo.wrapping_offset(2) as isize)
                + *x.wrapping_offset(*topo.wrapping_offset(3) as isize))
                / 4.0;
            ymean = (*y.wrapping_offset(*topo.wrapping_offset(0) as isize)
                + *y.wrapping_offset(*topo.wrapping_offset(1) as isize)
                + *y.wrapping_offset(*topo.wrapping_offset(2) as isize)
                + *y.wrapping_offset(*topo.wrapping_offset(3) as isize))
                / 4.0;
            *i = 0;
            while *i < 4 {
                *x.wrapping_offset(*topo.wrapping_offset((11 + *i) as isize) as isize) =
                    (2.0 * xmean + 1.0 * *x.wrapping_offset(*i as isize)) / 3.0;
                *y.wrapping_offset(*topo.wrapping_offset((11 + *i) as isize) as isize) =
                    (2.0 * ymean + 1.0 * *y.wrapping_offset(*i as isize)) / 3.0;
                *i += 1;
            }
        }
    } else {
        printf(
            c"IsoparametricElements: Not implemented for elementtype %d\n".as_ptr(),
            elemtype,
        );
    }
}
/// Center mid-edge nodes not on a boundary (isoparametric fix-up).
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn IsoparametricElements(
    data: *mut FemType,
    bound: *mut BoundaryType,
    bcstoo: c_int,
    info: c_int,
) {
    let _ = bcstoo;
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut element: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut side: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidenodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (set from data before reads).
    let mut topo: *mut c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: null-init (set from data before reads).
    // NOTE: C `z` write-only here (x/y alias the rows).
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let x: *mut Real = (*data).x;
    let y: *mut Real = (*data).y;
    // NOTE: C `z = data->z` dropped (never read).
    let bcindx: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *bcindx.wrapping_offset(i as isize) = FALSE;
        i += 1;
    }
    j = 0;
    while j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(j as isize)).created == 0 {
            j += 1;
            continue;
        }
        i = 1;
        while i <= (*bound.wrapping_offset(j as isize)).nosides {
            element = *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize);
            side = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            GetElementSide(element, side, 1, data, sideind.as_mut_ptr(), &mut sideelemtype);
            sidenodes = sideelemtype % 100;
            k = 0;
            while k < sidenodes {
                *bcindx.wrapping_offset(sideind[k as usize] as isize) = TRUE;
                k += 1;
            }
            i += 1;
        }
        j += 1;
    }
    j = 1;
    while j <= noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(j as isize);
        topo = *(*data).topology.wrapping_offset(j as isize);
        isoparam_elem(x, y, topo, bcindx, elemtype, &mut i);
        j += 1;
    }
    if info != 0 {
        printf(c"The elements were forced to be isoparametric\n".as_ptr());
    }
}

/// Downgrade solver-unsupported center-node elements.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn ModifyUnsupportedElements(data: *mut FemType) {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elementtype: c_int;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let cnt718: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut cnt614: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut cnt409: c_int = 0;
    // NOTE: `=0` decl inits kept (read before any overwrite).
    let mut cnt827: c_int = 0;
    i = 1;
    while i <= (*data).noelements {
        elementtype = *(*data).elementtypes.wrapping_offset(i as isize);
        if elementtype == 718 {
            // NOTE: support introduced; the downgrade never runs.
            break;
        } else if elementtype == 614 {
            *(*data).elementtypes.wrapping_offset(i as isize) = 613;
            cnt614 += 1;
        }
        i += 1;
    }
    if cnt718 != 0 || cnt614 != 0 {
        printf(c"WARNING: ElmerSolver does not currently support all elements in mesh, dropping center nodes!\n".as_ptr());
        i = 1;
        while i <= (*data).noelements {
            elementtype = *(*data).elementtypes.wrapping_offset(i as isize);
            if elementtype == 409 {
                *(*data).elementtypes.wrapping_offset(i as isize) = 408;
                cnt409 += 1;
            } else if elementtype == 827 {
                *(*data).elementtypes.wrapping_offset(i as isize) = 820;
                cnt827 += 1;
            }
            i += 1;
        }
        if cnt718 != 0 {
            printf(
                c"Element type 718 not supported, dropping centernodes for %d elements\n".as_ptr(),
                cnt718,
            );
        }
        if cnt614 != 0 {
            printf(
                c"Element type 614 not supported, dropping centernodes for %d elements\n".as_ptr(),
                cnt614,
            );
        }
        if cnt827 != 0 {
            printf(
                c"Dropping centernodes in %d elements of type 827 as well!\n".as_ptr(),
                cnt827,
            );
        }
        if cnt409 != 0 {
            printf(
                c"Dropping centernodes in %d elements of type 409 as well!\n".as_ptr(),
                cnt409,
            );
        }
    }
}
/// SAFETY: multi-dimension tag uniqueness pass (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn etbc_tags(
    data: *mut FemType,
    noelements: c_int,
    maxelemdim: c_int,
    elemdim: &mut c_int,
    tagcount: &mut *mut *mut c_int,
    mintag: &mut c_int,
    maxtag: &mut c_int,
    tag: &mut c_int,
    overlap: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    info: c_int,
) {
    *mintag = -1;
    *maxtag = -1;
    *tag = -1;
    *overlap = -1;
    if info != 0 {
        printf(c"Checking that different dimensions have unique boundary tags!\n".as_ptr());
    }
    *k = 0;
    while *k <= 2 {
        *i = 1;
        while *i <= noelements {
            *elemdim = GetElementDimension(*(*data).elementtypes.wrapping_offset(*i as isize));
            *tag = *(*data).material.wrapping_offset(*i as isize);
            // NOTE: tag interval over all elements.
            if *k == 0 {
                if *maxtag == -1 {
                    *mintag = *tag;
                    *maxtag = *tag;
                } else {
                    // NOTE: C MIN macro (egdef.h:96); arms pure.
                    if *tag < *mintag {
                        *mintag = *tag;
                    }
                    // NOTE: C MAX macro (egdef.h:97); arms pure.
                    if *maxtag < *tag {
                        *maxtag = *tag;
                    }
                }
            } else if *k == 1 {
                // NOTE: per-dimension tag counts.
                (*(*tagcount.wrapping_offset(*elemdim as isize))
                    .wrapping_offset(*tag as isize)) += 1;
            } else if *k == 2 && *elemdim < maxelemdim {
                // NOTE: new tags for lower dimensions.
                *(*data).material.wrapping_offset(*i as isize) =
                    *(*tagcount.wrapping_offset(*elemdim as isize))
                        .wrapping_offset(*tag as isize);
            }
            *i += 1;
        }
        if *k == 0 {
            if info != 0 {
                printf(
                    c"Tag interval for boundaries: [%d %d]\n".as_ptr(),
                    *mintag,
                    *maxtag,
                );
            }
            *tagcount = imatrix(0, maxelemdim, *mintag, *maxtag);
            *i = 0;
            while *i <= maxelemdim {
                *j = *mintag;
                while *j <= *maxtag {
                    *(*tagcount.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) = 0;
                    *j += 1;
                }
                *i += 1;
            }
        } else if *k == 1 {
            *j = *mintag;
            while *j <= *maxtag {
                *overlap = 0;
                *i = 0;
                while *i < maxelemdim {
                    if *(*tagcount.wrapping_offset(*i as isize))
                        .wrapping_offset(*j as isize)
                        != 0
                    {
                        *overlap += 1;
                    }
                    *i += 1;
                }
                if *overlap > 1 {
                    break;
                }
                *j += 1;
            }
            if *overlap > 1 {
                if info != 0 {
                    printf(c"We have an overlap, applying offsets!\n".as_ptr());
                }
                *tag = 0;
                *i = maxelemdim - 1;
                while *i >= 0 {
                    *j = *mintag;
                    while *j <= *maxtag {
                        if *(*tagcount.wrapping_offset(*i as isize))
                            .wrapping_offset(*j as isize)
                            != 0
                        {
                            if *tag < *j {
                                *tag = *j;
                            } else {
                                *tag += 1;
                                if info != 0 {
                                    printf(
                                        c"Replacing tag in %d-dim %d -> %d\n".as_ptr(),
                                        *i,
                                        *j,
                                        *tag,
                                    );
                                }
                            }
                            *(*tagcount.wrapping_offset(*i as isize))
                                .wrapping_offset(*j as isize) = *tag;
                        }
                        *j += 1;
                    }
                    *i -= 1;
                }
            } else {
                if info != 0 {
                    printf(c"No overlap, no offsets needed!\n".as_ptr());
                }
                break;
            }
        } else if *k == 2 && info != 0 {
            printf(c"Renumbered tags for boundary elements!\n".as_ptr());
        }
        *k += 1;
    }
}

/// SAFETY: move flags + inverse-topology build (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn etbc_invtopo(
    data: *mut FemType,
    bound: *mut BoundaryType,
    moveelement: &mut *mut c_int,
    possible: &mut *mut c_int,
    invtopo: &mut *mut *mut c_int,
    noelements: c_int,
    noknots: c_int,
    maxpossible: &mut c_int,
    sideelements: &mut c_int,
    maxelemdim: c_int,
    elemtype: &mut c_int,
    elemind: &mut c_int,
    sideelemdim: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    info: c_int,
) {
    *moveelement = ivector(1, noelements);
    *sideelements = 0;
    *i = 1;
    while *i <= noelements {
        *(*moveelement).wrapping_offset(*i as isize) = FALSE;
        *sideelemdim = GetElementDimension(*(*data).elementtypes.wrapping_offset(*i as isize));
        *(*moveelement).wrapping_offset(*i as isize) = maxelemdim - *sideelemdim;
        if *(*moveelement).wrapping_offset(*i as isize) != 0 {
            *sideelements += 1;
        }
        *i += 1;
    }
    if info != 0 {
        printf(
            c"There are %d (out of %d) lower dimensional elements.\n".as_ptr(),
            *sideelements,
            noelements,
        );
    }
    if *sideelements == 0 {
        return;
    }
    AllocateBoundary(bound, *sideelements);
    *possible = ivector(1, noknots);
    *i = 1;
    while *i <= noknots {
        *(*possible).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *elemind = 1;
    while *elemind <= (*data).noelements {
        *elemtype = *(*data).elementtypes.wrapping_offset(*elemind as isize);
        if *elemtype < 200 {
            *elemind += 1;
            continue;
        }
        *i = 0;
        while *i < *elemtype % 100 {
            *j = *(*(*data).topology.wrapping_offset(*elemind as isize)).wrapping_offset(*i as isize);
            (*(*possible).wrapping_offset(*j as isize)) += 1;
            *i += 1;
        }
        *elemind += 1;
    }
    *j = 1;
    *maxpossible = *(*possible).wrapping_offset(1);
    *i = 1;
    while *i <= noknots {
        if *maxpossible < *(*possible).wrapping_offset(*i as isize) {
            *maxpossible = *(*possible).wrapping_offset(*i as isize);
            *j = *i;
        }
        *i += 1;
    }
    if info != 0 {
        printf(
            c"Node %d belongs to maximum of %d elements\n".as_ptr(),
            *j,
            *maxpossible,
        );
    }
    *invtopo = imatrix(1, noknots, 1, *maxpossible);
    *i = 1;
    while *i <= noknots {
        *j = 1;
        while *j <= *maxpossible {
            *(*invtopo.wrapping_offset(*i as isize)).wrapping_offset(*j as isize) = 0;
            *j += 1;
        }
        *i += 1;
    }
    *elemind = 1;
    while *elemind <= (*data).noelements {
        *elemtype = *(*data).elementtypes.wrapping_offset(*elemind as isize);
        if *elemtype < 200 {
            *elemind += 1;
            continue;
        }
        *i = 0;
        while *i < *elemtype % 100 {
            *k = *(*(*data).topology.wrapping_offset(*elemind as isize)).wrapping_offset(*i as isize);
            *l = 1;
            while *(*invtopo.wrapping_offset(*k as isize)).wrapping_offset(*l as isize) != 0 {
                *l += 1;
            }
            *(*invtopo.wrapping_offset(*k as isize)).wrapping_offset(*l as isize) = *elemind;
            *i += 1;
        }
        *elemind += 1;
    }
}
/// SAFETY: parent search for one lower-dimensional element (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn etbc_match(
    data: *mut FemType,
    bound: *mut BoundaryType,
    moveelement: *mut c_int,
    invtopo: *mut *mut c_int,
    maxpossible: c_int,
    movenames: c_int,
    elemind: c_int,
    loopdim: c_int,
    sideelem: &mut c_int,
    sameelem: &mut c_int,
    sideelemtype: &mut c_int,
    sideelemtype2: &mut c_int,
    sidenodes: &mut c_int,
    sidenodes2: &mut c_int,
    sideind: *mut c_int,
    sideind2: *mut c_int,
    material: &mut c_int,
    elemtype: &mut c_int,
    elemdim: &mut c_int,
    sideelemdim: &mut c_int,
    elemind2: &mut c_int,
    side: &mut c_int,
    hit: &mut c_int,
    same: &mut c_int,
    elemhits: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    l: &mut c_int,
    debug: c_int,
    foundtwo: &mut c_int,
) {
    *foundtwo = FALSE;
    *same = FALSE;
    *sideelemtype = *(*data).elementtypes.wrapping_offset(elemind as isize);
    // NOTE: only the right-dimension elements are checked.
    *sideelemdim = GetElementDimension(*sideelemtype);
    if *sideelemdim != loopdim {
        return;
    }
    *sidenodes = *sideelemtype % 100;
    *i = 0;
    while *i < *sidenodes {
        *sideind.wrapping_offset(*i as isize) = *(*(*data)
            .topology
            .wrapping_offset(elemind as isize))
        .wrapping_offset(*i as isize);
        *i += 1;
    }
    *elemhits = 0;
    // NOTE: candidates share the first side node.
    *l = 1;
    while *l <= maxpossible {
        *elemind2 = *(*invtopo.wrapping_offset(*sideind.wrapping_offset(0) as isize))
            .wrapping_offset(*l as isize);
        if *elemind2 == 0 {
            *l += 1;
            continue;
        }
        // NOTE: the parent must stay a bulk element.
        if *moveelement.wrapping_offset(*elemind2 as isize) != 0 {
            *l += 1;
            continue;
        }
        *elemtype = *(*data).elementtypes.wrapping_offset(*elemind2 as isize);
        *elemdim = GetElementDimension(*elemtype);
        // NOTE: owner must be higher-dimensional.
        if *elemdim <= *sideelemdim {
            *l += 1;
            continue;
        }
        *hit = 0;
        *i = 0;
        while *i < *sidenodes {
            *j = 0;
            while *j < *elemtype % 100 {
                if *sideind.wrapping_offset(*i as isize)
                    == *(*(*data).topology.wrapping_offset(*elemind2 as isize))
                        .wrapping_offset(*j as isize)
                {
                    *hit += 1;
                }
                *j += 1;
            }
            *i += 1;
        }
        if *hit < *sidenodes {
            *l += 1;
            continue;
        }
        if *hit > *sidenodes {
            printf(
                c"Strange: elemhits %d vs. elemnodes %d\n".as_ptr(),
                *hit,
                *sidenodes,
            );
        }
        if *hit >= *sidenodes {
            *elemhits += 1;
        }
        *side = 0;
        while *side <= 100 {
            GetElementSide(*elemind2, *side, 1, data, sideind2, sideelemtype2);
            if *sideelemtype2 == 0 {
                break;
            }
            if *sideelemtype2 < 300 && *sideelemtype > 300 {
                break;
            }
            if *sideelemtype2 < 200 && *sideelemtype > 200 {
                break;
            }
            if *sideelemtype2 != *sideelemtype {
                *side += 1;
                continue;
            }
            *sidenodes2 = *sideelemtype2 % 100;
            if *sidenodes != *sidenodes2 {
                *side += 1;
                continue;
            }
            if *sidenodes2 == 1 && *sidenodes > 1 {
                break;
            }
            *hit = 0;
            *i = 0;
            while *i < *sidenodes {
                *j = 0;
                while *j < *sidenodes2 {
                    if *sideind.wrapping_offset(*i as isize)
                        == *sideind2.wrapping_offset(*j as isize)
                    {
                        *hit += 1;
                    }
                    *j += 1;
                }
                *i += 1;
            }
            // NOTE: `if(0)` trace omitted (dead in C).
            if *hit == *sidenodes {
                break;
            }
            if *sideelemtype != *sideelemtype2 {
                printf(
                    c"Hits in element after mismatch: %d vs. %d\n".as_ptr(),
                    *sideelemtype,
                    *sideelemtype2,
                );
                *side += 1;
                continue;
            }
            *side += 1;
        }
        if *sidenodes == 1 && *hit == 0 {
            printf(
                c"elemind = %d sideind %d vs. ".as_ptr(),
                elemind,
                *sideind.wrapping_offset(0),
            );
            *j = 0;
            while *j < *sidenodes2 {
                printf(c"%d ".as_ptr(), *sideind2.wrapping_offset(*j as isize));
                *j += 1;
            }
            printf(c"\n".as_ptr());
        }
        if *hit < *sidenodes || *sideelemtype2 == 0 {
            // NOTE: `if(0)` trace omitted (dead in C).
            *l += 1;
            continue;
        }
        if *same != 0 {
            *sameelem += 1;
            *(*bound).parent2.wrapping_offset(*sideelem as isize) = *elemind2;
            *(*bound).side2.wrapping_offset(*sideelem as isize) = *side;
            // NOTE: `if(debug)` trace omitted (debug is FALSE).
            // NOTE: C `goto foundtwo` continues the elemind loop.
            *foundtwo = TRUE;
            return;
        } else {
            *sideelem += 1;
            *same = TRUE;
            // NOTE: `if(debug)` trace omitted (debug is FALSE).
            *(*bound).parent.wrapping_offset(*sideelem as isize) = *elemind2;
            *(*bound).side.wrapping_offset(*sideelem as isize) = *side;
            *(*bound).parent2.wrapping_offset(*sideelem as isize) = 0;
            *(*bound).side2.wrapping_offset(*sideelem as isize) = 0;
            *material = *(*data).material.wrapping_offset(elemind as isize);
            *(*bound).types.wrapping_offset(*sideelem as isize) = *material;
            if *sidenodes == 2
                && (*sideind.wrapping_offset(0) - *sideind.wrapping_offset(1))
                    * (*sideind2.wrapping_offset(0) - *sideind2.wrapping_offset(1))
                    < 0
            {
                *(*bound).normal.wrapping_offset(*sideelem as isize) = -1;
            }
            if movenames != 0 {
                (*data).boundarynamesexist = TRUE;
                if *material < MAXBODIES as c_int
                    && *material < MAXBCS as c_int
                    && (*data).boundaryname[*material as usize].is_null()
                {
                    (*data).boundaryname[*material as usize] =
                        cvector(0, MAXNAMESIZE as c_int);
                    if !(*data).bodyname[*material as usize].is_null() {
                        strcpy(
                            (*data).boundaryname[*material as usize],
                            (*data).bodyname[*material as usize],
                        );
                        free_cvector(
                            (*data).bodyname[*material as usize],
                            0,
                            MAXNAMESIZE as c_int,
                        );
                        (*data).bodyname[*material as usize] = core::ptr::null_mut();
                    } else {
                        sprintf(
                            (*data).boundaryname[*material as usize],
                            c"body%d".as_ptr(),
                            *material,
                        );
                    }
                }
                if *material < MAXBCS as c_int
                    && (*data).boundaryname[*material as usize].is_null()
                {
                    (*data).boundaryname[*material as usize] =
                        cvector(0, MAXNAMESIZE as c_int);
                    if strncmp(
                        (*data).boundaryname[*material as usize],
                        c"body".as_ptr(),
                        4,
                    ) == 0
                    {
                        strncpy(
                            (*data).boundaryname[*material as usize],
                            c"bnry".as_ptr(),
                            4,
                        );
                    }
                } else {
                    printf(
                        c"Boundary index %d exceeds the maximum allocated space for names %d\n".as_ptr(),
                        *material,
                        MAXBCS as c_int,
                    );
                }
            }
            // NOTE: two parents only when two dims below the maximum.
            if *moveelement.wrapping_offset(elemind as isize) > 1 {
                // NOTE: C `goto foundtwo` continues the elemind loop.
                *foundtwo = TRUE;
                return;
            }
        }
        *l += 1;
    }
    let _ = (debug, elemhits);
}
/// SAFETY: orphan + notfound handling for one element (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn etbc_orphan(
    data: *mut FemType,
    _bound: *mut BoundaryType,
    moveelement: *mut c_int,
    notfounds: &mut *mut c_int,
    notfound: &mut c_int,
    lowdimbulk: &mut c_int,
    removed: &mut c_int,
    retainorphans: c_int,
    elemind: c_int,
    same: c_int,
    sideelemtype: c_int,
    sidenodes: c_int,
    sideind: *mut c_int,
    elemhits: c_int,
    i: &mut c_int,
    debug: c_int,
) {
    if same == 0 {
        // NOTE: unparented DIM-1 elements stay bulk or list as notfound.
        if retainorphans != 0 {
            if *moveelement.wrapping_offset(elemind as isize) == 1 {
                *moveelement.wrapping_offset(elemind as isize) = 0;
                *lowdimbulk += 1;
                // NOTE: `if(debug)` trace omitted (debug is FALSE).
            } else {
                if *notfound == 0 {
                    *notfounds = ivector(1, (*data).noelements);
                    *i = 1;
                    while *i <= (*data).noelements {
                        *(*notfounds).wrapping_offset(*i as isize) = FALSE;
                        *i += 1;
                    }
                }
                *notfound += 1;
                *(*notfounds).wrapping_offset(elemind as isize) = TRUE;
                // NOTE: `if(0)` dump omitted (dead in C).
                // NOTE: `if(debug)` trace omitted (debug is FALSE).
            }
        }
    } else {
        // NOTE: `if(debug)` trace omitted (debug is FALSE).
        *moveelement.wrapping_offset(elemind as isize) = -1;
        *removed += 1;
    }
    let _ = (sideelemtype, sidenodes, sideind, elemhits, debug);
}

/// SAFETY: bulk reorder + boundary repoint + teardown (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn etbc_reorder(
    data: *mut FemType,
    bound: *mut BoundaryType,
    moveelement: *mut c_int,
    parentorder: &mut *mut c_int,
    possible: *mut c_int,
    invtopo: *mut *mut c_int,
    notfounds: *mut c_int,
    notfound: c_int,
    noelements: c_int,
    noknots: c_int,
    maxpossible: c_int,
    sideelem: &mut c_int,
    sideelements: c_int,
    sideelemtype2: &mut c_int,
    sideind2: *mut c_int,
    lowdimbulk: c_int,
    removed: c_int,
    retainorphans: c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    info: c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elem: c_int;
    if *sideelem != sideelements {
        printf(c"Studied %d lower dimensional elements\n".as_ptr(), sideelements);
        printf(c"Defined %d side elements\n".as_ptr(), *sideelem);
        printf(
            c"Defined %d lower dimensional bulk elements\n".as_ptr(),
            lowdimbulk,
        );
        (*bound).nosides = *sideelem;
        printf(
            c"Removing %d lower dimensional elements from the element list\n".as_ptr(),
            removed,
        );
        if notfound != 0 {
            // NOTE: `if(0)` banner omitted (dead in C).
            if retainorphans != 0 {
                printf(
                    c"Adding %d elements to boundary without parent information\n".as_ptr(),
                    notfound,
                );
                (*bound).elementtypes = ivector(*sideelem + 1, sideelements);
                *i = *sideelem + 1;
                while *i <= sideelements {
                    *(*bound).elementtypes.wrapping_offset(*i as isize) = 0;
                    *i += 1;
                }
                (*bound).topology = imatrix(*sideelem + 1, sideelements, 0, MAXNODESD2 as c_int - 1);
                elem = 1;
                while elem <= (*data).noelements {
                    if *notfounds.wrapping_offset(elem as isize) == 0 {
                        elem += 1;
                        continue;
                    }
                    *sideelem += 1;
                    *j = *(*data).elementtypes.wrapping_offset(elem as isize);
                    *(*bound).elementtypes.wrapping_offset(*sideelem as isize) = *j;
                    *i = 0;
                    while *i < *j % 100 {
                        *(*(*bound).topology.wrapping_offset(*sideelem as isize))
                            .wrapping_offset(*i as isize) = *(*(*data)
                            .topology
                            .wrapping_offset(elem as isize))
                        .wrapping_offset(*i as isize);
                        *i += 1;
                    }
                    // NOTE: debug offset marks synthesized types.
                    *(*bound).types.wrapping_offset(*sideelem as isize) =
                        *(*data).material.wrapping_offset(elem as isize) + 10;
                    *(*bound).parent.wrapping_offset(*sideelem as isize) = 0;
                    elem += 1;
                }
                (*bound).nosides = *sideelem;
            } else {
                printf(
                    c"Removing %d lower dimensional elements without parent information\n".as_ptr(),
                    notfound,
                );
            }
        }
    } else {
        printf(c"Found correctly %d side elements.\n".as_ptr(), *sideelem);
    }
    // NOTE: compact the remaining bulk elements.
    *parentorder = ivector(1, noelements);
    *i = 1;
    while *i <= noelements {
        *(*parentorder).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *j = 0;
    *i = 1;
    while *i <= noelements {
        if *moveelement.wrapping_offset(*i as isize) == 0 {
            *k = *(*data).elementtypes.wrapping_offset(*i as isize);
            *j += 1;
            *(*parentorder).wrapping_offset(*i as isize) = *j;
            // NOTE: `if(debug)` trace omitted (debug is FALSE).
            if *i != *j {
                *(*data).material.wrapping_offset(*j as isize) =
                    *(*data).material.wrapping_offset(*i as isize);
                *(*data).elementtypes.wrapping_offset(*j as isize) =
                    *(*data).elementtypes.wrapping_offset(*i as isize);
                *l = 0;
                while *l < *k % 100 {
                    *(*(*data).topology.wrapping_offset(*j as isize))
                        .wrapping_offset(*l as isize) = *(*(*data)
                        .topology
                        .wrapping_offset(*i as isize))
                    .wrapping_offset(*l as isize);
                    *l += 1;
                }
            }
        }
        *i += 1;
    }
    (*data).noelements = *j;
    if info != 0 {
        printf(
            c"Parent elements were reordered up to index %d.\n".as_ptr(),
            *j,
        );
    }
    // NOTE: repoint boundaries at the compacted bulk.
    *i = 1;
    while *i <= (*bound).nosides {
        if *(*bound).parent.wrapping_offset(*i as isize) == 0 {
            *i += 1;
            continue;
        }
        if *(*parentorder)
            .wrapping_offset(*(*bound).parent.wrapping_offset(*i as isize) as isize)
            == 0
        {
            printf(
                c"Zero reorder: %d %d %d\n".as_ptr(),
                *i,
                *(*bound).parent.wrapping_offset(*i as isize),
                *(*bound).side.wrapping_offset(*i as isize),
            );
            bigerror(c"Sorry folks!".as_ptr());
        }
        if *(*bound).parent.wrapping_offset(*i as isize) != 0 {
            *(*bound).parent.wrapping_offset(*i as isize) = *(*parentorder).wrapping_offset(
                *(*bound).parent.wrapping_offset(*i as isize) as isize,
            );
        }
        if *(*bound).parent2.wrapping_offset(*i as isize) != 0 {
            *(*bound).parent2.wrapping_offset(*i as isize) = *(*parentorder).wrapping_offset(
                *(*bound).parent2.wrapping_offset(*i as isize) as isize,
            );
        }
        GetElementSide(
            *(*bound).parent.wrapping_offset(*i as isize),
            *(*bound).side.wrapping_offset(*i as isize),
            1,
            data,
            sideind2,
            sideelemtype2,
        );
        // NOTE: `if(0)` GetBoundaryElement probe omitted (dead in C).
        *i += 1;
    }
    if info != 0 {
        printf(
            c"Moved %d elements (out of %d) to new positions\n".as_ptr(),
            *j,
            noelements,
        );
    }
    free_ivector(*parentorder, 1, noelements);
    free_ivector(moveelement, 1, noelements);
    free_ivector(possible, 1, noknots);
    free_imatrix(invtopo, 1, noknots, 1, maxpossible);
    if notfound != 0 {
        free_ivector(notfounds, 1, noelements);
    }
    // NOTE: `if(debug)` trace omitted (debug is FALSE).
}
/// Split lower-dimensional bulk elements into boundary sides.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn ElementsToBoundaryConditions(
    data: *mut FemType,
    bound: *mut BoundaryType,
    retainorphans: c_int,
    info: c_int,
) {
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads.)
    let mut i: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads.)
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemind: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemind2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelem: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sameelem: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let elemsides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hit: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut same: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut minelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sidenodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sidenodes2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut material: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut moveelement: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut parentorder: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut possible: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Imatrix before guarded reads).
    let mut invtopo: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxpossible: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let maxelemsides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut twiceelem: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: always FALSE; the `if(debug)` traces are dead in C.
    let debug: c_int = FALSE;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let unmoved: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut removed: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemhits: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut loopdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut lowdimbulk: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut notfound: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut notfounds: *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: tag-table pass over multi-dimension meshes.
    let mut tagcount: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads).
    let mut mintag: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads).
    let mut maxtag: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads).
    let mut tag: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in etbc_tags before reads).
    let mut overlap: c_int = 0;
    // NOTE: 0-init for the foundtwo gate (set in etbc_match before reads).
    let mut foundtwo: c_int = 0;
    let _ = (
        elemsides, maxelemsides, twiceelem, unmoved, elemhits, loopdim, minelemtype,
        elemind, sideelemdim, debug,
    );
    if info != 0 {
        printf(c"Moving bulk elements to boundary elements\n".as_ptr());
        // NOTE: `if(0)` orphan trace omitted (dead in C).
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
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let movenames: c_int = ((*data).bodynamesexist != 0 && (*data).boundarynamesexist == 0) as c_int;
    if (*data).bodynamesexist != 0 && info != 0 {
        if movenames != 0 {
            printf(c"Moving boundarynames together with elements\n".as_ptr());
        } else {
            printf(c"Assuming that boundaries names are already Ok!\n".as_ptr());
        }
    }
    let maxelemtype: c_int = GetMaxElementType(data);
    if info != 0 {
        printf(c"Leading bulk elementtype is %d\n".as_ptr(), maxelemtype);
    }
    minelemtype = GetMinElementType(data);
    if info != 0 {
        printf(c"Trailing bulk elementtype is %d\n".as_ptr(), minelemtype);
    }
    let maxelemdim: c_int = GetElementDimension(maxelemtype);
    let minelemdim: c_int = GetElementDimension(minelemtype);
    if maxelemdim - minelemdim == 0 {
        if info != 0 {
            printf(c"No lower dimensional elements present!\n".as_ptr());
        }
        return;
    }
    if maxelemdim - minelemdim > 1 {
        etbc_tags(
            data, noelements, maxelemdim, &mut elemdim, &mut tagcount, &mut mintag,
            &mut maxtag, &mut tag, &mut overlap, &mut i, &mut j, &mut k, info,
        );
        free_imatrix(tagcount, 0, maxelemdim, mintag, maxtag);
    }
    etbc_invtopo(
        data, bound, &mut moveelement, &mut possible, &mut invtopo, noelements, noknots,
        &mut maxpossible, &mut sideelements, maxelemdim, &mut elemtype, &mut elemind,
        &mut sideelemdim, &mut i, &mut j, &mut k, &mut l, info,
    );
    if sideelements == 0 {
        return;
    }
    sideelem = 0;
    sameelem = 0;
    twiceelem = 0;
    // NOTE: `debug` is FALSE; traces dead.
    // NOTE: candidates from high dimension down.
    loopdim = maxelemdim - 1;
    while loopdim >= 0 {
        // NOTE: `if(debug)` trace omitted (debug is FALSE).
        elemind = 1;
        while elemind <= (*data).noelements {
            if *moveelement.wrapping_offset(elemind as isize) == 0 {
                elemind += 1;
                continue;
            }
            etbc_match(
                data, bound, moveelement, invtopo, maxpossible, movenames, elemind,
                loopdim, &mut sideelem, &mut sameelem, &mut sideelemtype,
                &mut sideelemtype2, &mut sidenodes, &mut sidenodes2,
                sideind.as_mut_ptr(), sideind2.as_mut_ptr(), &mut material,
                &mut elemtype, &mut elemdim, &mut sideelemdim, &mut elemind2,
                &mut side, &mut hit, &mut same, &mut elemhits, &mut i, &mut j,
                &mut l, debug, &mut foundtwo,
            );
            // NOTE: C `foundtwo:` label continues the elemind loop.
            if foundtwo == 0 {
                etbc_orphan(
                data, bound, moveelement, &mut notfounds, &mut notfound,
                &mut lowdimbulk, &mut removed, retainorphans, elemind, same,
                sideelemtype, sidenodes, sideind.as_mut_ptr(), elemhits, &mut i,
                debug,
            );
            }
            elemind += 1;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
        loopdim -= 1;
    }
    if twiceelem != 0 {
        printf(c"Found %d sides that were multiply given\n".as_ptr(), twiceelem);
    }
    if sameelem != 0 {
        printf(
            c"Found %d side elements that have two parents.\n".as_ptr(),
            sameelem,
        );
    }
    etbc_reorder(
        data, bound, moveelement, &mut parentorder, possible, invtopo, notfounds,
        notfound, noelements, noknots, maxpossible, &mut sideelem, sideelements,
        &mut sideelemtype2, sideind2.as_mut_ptr(), lowdimbulk, removed, retainorphans,
        &mut i, &mut j, &mut k, &mut l, info,
    );
}
/// Apply configured side/material index mappings.
///
/// # Safety
/// `data`, `bound`, `eg` valid.
#[no_mangle]
pub unsafe extern "C" fn SideAndBulkMappings(
    data: *mut FemType,
    bound: *mut BoundaryType,
    eg: *mut ElmergridType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut currenttype: c_int;
    if (*eg).sidemappings != 0 {
        if info != 0 {
            printf(
                c"Renumbering boundary types with %d mappings\n".as_ptr(),
                (*eg).sidemappings,
            );
        }
        l = 0;
        while l < (*eg).sidemappings {
            if info != 0 {
                printf(
                    c"Setting boundary types between %d and %d to %d\n".as_ptr(),
                    (*eg).sidemap[(3 * l) as usize],
                    (*eg).sidemap[(3 * l + 1) as usize],
                    (*eg).sidemap[(3 * l + 2) as usize],
                );
            }
            l += 1;
        }
        j = 0;
        while j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(j as isize)).created == 0 {
                j += 1;
                continue;
            }
            i = 1;
            while i <= (*bound.wrapping_offset(j as isize)).nosides {
                currenttype = *(*bound.wrapping_offset(j as isize))
                    .types
                    .wrapping_offset(i as isize);
                if currenttype != 0 {
                    l = 0;
                    while l < (*eg).sidemappings {
                        if currenttype >= (*eg).sidemap[(3 * l) as usize]
                            && currenttype <= (*eg).sidemap[(3 * l + 1) as usize]
                        {
                            *(*bound.wrapping_offset(j as isize))
                                .types
                                .wrapping_offset(i as isize) =
                                (*eg).sidemap[(3 * l + 2) as usize];
                            currenttype = -1;
                        }
                        l += 1;
                    }
                }
                i += 1;
            }
            j += 1;
        }
        if info != 0 {
            printf(c"Renumbering boundary types finished\n".as_ptr());
        }
    }
    if (*eg).bulkmappings != 0 {
        if info != 0 {
            printf(
                c"Renumbering bulk types with %d mappings\n".as_ptr(),
                (*eg).bulkmappings,
            );
        }
        l = 0;
        while l < (*eg).bulkmappings {
            if info != 0 {
                printf(
                    c"Setting material types between %d and %d to %d\n".as_ptr(),
                    (*eg).bulkmap[(3 * l) as usize],
                    (*eg).bulkmap[(3 * l + 1) as usize],
                    (*eg).bulkmap[(3 * l + 2) as usize],
                );
            }
            l += 1;
        }
        j = 1;
        while j <= (*data).noelements {
            currenttype = *(*data).material.wrapping_offset(j as isize);
            l = 0;
            while l < (*eg).bulkmappings {
                if currenttype >= (*eg).bulkmap[(3 * l) as usize]
                    && currenttype <= (*eg).bulkmap[(3 * l + 1) as usize]
                {
                    *(*data).material.wrapping_offset(j as isize) =
                        (*eg).bulkmap[(3 * l + 2) as usize];
                    currenttype = -1;
                }
                l += 1;
            }
            j += 1;
        }
        if info != 0 {
            printf(c"Renumbering material indexes finished\n".as_ptr());
        }
    }
    0
}

/// Create boundaries from configured bulk/boundary node sets.
///
/// # Safety
/// `data`, `bound`, `eg` valid.
#[no_mangle]
pub unsafe extern "C" fn SideAndBulkBoundaries(
    data: *mut FemType,
    bound: *mut BoundaryType,
    eg: *mut ElmergridType,
    info: c_int,
) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut l: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut noboundnodes: c_int = 0;
    let boundnodes: *mut c_int = ivector(1, (*data).noknots);
    if (*eg).bulkbounds != 0 {
        l = 0;
        while l < (*eg).bulkbounds {
            FindBulkBoundary(
                data,
                (*eg).bulkbound[(3 * l) as usize],
                (*eg).bulkbound[(3 * l + 1) as usize],
                boundnodes,
                &mut noboundnodes,
                info,
            );
            FindNewBoundaries(
                data,
                bound,
                boundnodes,
                (*eg).bulkbound[(3 * l + 2) as usize],
                1,
                info,
            );
            l += 1;
        }
    }
    if (*eg).boundbounds != 0 {
        l = 0;
        while l < (*eg).boundbounds {
            FindBoundaryBoundary(
                data,
                bound,
                (*eg).boundbound[(3 * l) as usize],
                (*eg).boundbound[(3 * l + 1) as usize],
                boundnodes,
                &mut noboundnodes,
                info,
            );
            FindNewBoundaries(
                data,
                bound,
                boundnodes,
                (*eg).boundbound[(3 * l + 2) as usize],
                2,
                info,
            );
            l += 1;
        }
    }
    free_ivector(boundnodes, 1, (*data).noknots);
    0
}
/// SAFETY: parent search for one node-chain link (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn chain_match(
    data: *mut FemType,
    bound: *mut BoundaryType,
    invtopo: *mut *mut c_int,
    maxpossible: c_int,
    sideind: *mut c_int,
    sideind2: *mut c_int,
    sidenodes: c_int,
    sideelemtype: c_int,
    material: c_int,
    sideelem: &mut c_int,
    sameelem: &mut c_int,
    sideelemtype2: &mut c_int,
    side: &mut c_int,
    hit: &mut c_int,
    same: &mut c_int,
    elemhits: &mut c_int,
    elemtype: &mut c_int,
    elemind2: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    l: &mut c_int,
    foundtwo: &mut c_int,
) {
    *foundtwo = FALSE;
    *same = FALSE;
    *elemhits = 0;
    // NOTE: candidate parents via the inverse topology.
    *l = 1;
    while *l <= maxpossible {
        *elemind2 = *(*invtopo.wrapping_offset(*sideind.wrapping_offset(0) as isize))
            .wrapping_offset(*l as isize);
        if *elemind2 == 0 {
            *l += 1;
            continue;
        }
        *elemtype = *(*data).elementtypes.wrapping_offset(*elemind2 as isize);
        *hit = 0;
        *i = 0;
        while *i < sidenodes {
            *j = 0;
            while *j < *elemtype % 100 {
                if *sideind.wrapping_offset(*i as isize)
                    == *(*(*data).topology.wrapping_offset(*elemind2 as isize))
                        .wrapping_offset(*j as isize)
                {
                    *hit += 1;
                }
                *j += 1;
            }
            *i += 1;
        }
        // NOTE: all side nodes must hit.
        if *hit < sidenodes {
            *l += 1;
            continue;
        }
        *elemhits += 1;
        // NOTE: locate the matching side.
        *side = 0;
        while *side < 3 {
            GetElementSide(*elemind2, *side, 1, data, sideind2, sideelemtype2);
            if *sideelemtype2 != sideelemtype {
                printf(c"This should not happen!\n".as_ptr());
            }
            *hit = 0;
            *i = 0;
            while *i < sidenodes {
                *j = 0;
                while *j < sidenodes {
                    if *sideind.wrapping_offset(*i as isize)
                        == *sideind2.wrapping_offset(*j as isize)
                    {
                        *hit += 1;
                    }
                    *j += 1;
                }
                *i += 1;
            }
            if *hit < sidenodes {
                *side += 1;
                continue;
            }
            if *same != 0 {
                // NOTE: second parent found.
                *sameelem += 1;
                *(*bound).parent2.wrapping_offset(*sideelem as isize) = *elemind2;
                *(*bound).side2.wrapping_offset(*sideelem as isize) = *side;
                // NOTE: C `goto foundtwo` continues the bci loop.
                *foundtwo = TRUE;
                return;
            } else {
                // NOTE: first parent for this link.
                *sideelem += 1;
                *same = TRUE;
                *(*bound).parent.wrapping_offset(*sideelem as isize) = *elemind2;
                *(*bound).side.wrapping_offset(*sideelem as isize) = *side;
                *(*bound).parent2.wrapping_offset(*sideelem as isize) = 0;
                *(*bound).side2.wrapping_offset(*sideelem as isize) = 0;
                *(*bound).types.wrapping_offset(*sideelem as isize) = material;
                if sidenodes == 2
                    && (*sideind.wrapping_offset(0) - *sideind.wrapping_offset(1))
                        * (*sideind2.wrapping_offset(0) - *sideind2.wrapping_offset(1))
                        < 0
                {
                    *(*bound).normal.wrapping_offset(*sideelem as isize) = -1;
                }
            }
            *side += 1;
        }
        *l += 1;
    }
}

/// Boundary chain of 202 sides from ordered boundary nodes.
///
/// # Safety
/// `data`, `bound`, `bcinds`, `bctags` valid.
#[no_mangle]
pub unsafe extern "C" fn NodesToBoundaryChain(
    data: *mut FemType,
    bound: *mut BoundaryType,
    bcinds: *mut c_int,
    bctags: *mut c_int,
    nbc: c_int,
    bccount: c_int,
    info: c_int,
) {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemind: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemind2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelem: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sameelem: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let elemsides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hit: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut same: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let minelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let sidenodes2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let elemdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut material: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxpossible: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let sideelemdim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemhits: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut bci: c_int;
    // NOTE: 0-init for the foundtwo gate (set in chain_match before reads).
    let mut foundtwo: c_int = 0;
    let _ = (
        elemsides, minelemtype, elemdim, sidenodes2, sideelemdim,
    );
    if info != 0 {
        printf(c"Creating boundary elements from boundary nodes\n".as_ptr());
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
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let sideelements: c_int = nbc - bccount;
    printf(c"Expected number of BC elements: %d\n".as_ptr(), sideelements);
    AllocateBoundary(bound, sideelements);
    // NOTE: node-use counts for the inverse topology.
    let possible: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *possible.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    elemind = 1;
    while elemind <= (*data).noelements {
        i = 0;
        while i < *(*data).elementtypes.wrapping_offset(elemind as isize) % 100 {
            j = *(*(*data).topology.wrapping_offset(elemind as isize))
                .wrapping_offset(i as isize);
            (*possible.wrapping_offset(j as isize)) += 1;
            i += 1;
        }
        elemind += 1;
    }
    j = 1;
    maxpossible = *possible.wrapping_offset(1);
    i = 1;
    while i <= noknots {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxpossible < *possible.wrapping_offset(i as isize) {
            maxpossible = *possible.wrapping_offset(i as isize);
            j = i;
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"Node %d belongs to maximum of %d elements\n".as_ptr(),
            j,
            maxpossible,
        );
    }
    // NOTE: node-to-element table.
    let invtopo: *mut *mut c_int = imatrix(1, noknots, 1, maxpossible);
    i = 1;
    while i <= noknots {
        j = 1;
        while j <= maxpossible {
            *(*invtopo.wrapping_offset(i as isize)).wrapping_offset(j as isize) = 0;
            j += 1;
        }
        i += 1;
    }
    elemind = 1;
    while elemind <= (*data).noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(elemind as isize);
        i = 0;
        while i < elemtype % 100 {
            k = *(*(*data).topology.wrapping_offset(elemind as isize))
                .wrapping_offset(i as isize);
            // NOTE: first unset slot wins.
            l = 1;
            while *(*invtopo.wrapping_offset(k as isize)).wrapping_offset(l as isize) != 0 {
                l += 1;
            }
            *(*invtopo.wrapping_offset(k as isize)).wrapping_offset(l as isize) = elemind;
            i += 1;
        }
        elemind += 1;
    }
    sideelem = 0;
    sameelem = 0;
    let twiceelem: c_int = 0;
    // NOTE: links of two consecutive chain nodes become 202 sides.
    let sidenodes: c_int = 2;
    let sideelemtype: c_int = 202;
    bci = 1;
    while bci < nbc {
        // NOTE: links across differing tags are skipped.
        if *bctags.wrapping_offset(bci as isize)
            != *bctags.wrapping_offset((bci + 1) as isize)
        {
            bci += 1;
            continue;
        }
        sideind[0] = *bcinds.wrapping_offset(bci as isize);
        sideind[1] = *bcinds.wrapping_offset((bci + 1) as isize);
        material = *bctags.wrapping_offset(bci as isize);
        chain_match(
            data, bound, invtopo, maxpossible, sideind.as_mut_ptr(),
            sideind2.as_mut_ptr(), sidenodes, sideelemtype, material, &mut sideelem,
            &mut sameelem, &mut sideelemtype2, &mut side, &mut hit, &mut same,
            &mut elemhits, &mut elemtype, &mut elemind2, &mut i, &mut j, &mut l,
            &mut foundtwo,
        );
        // NOTE: C `foundtwo:` label continues the bci loop.
        // NOTE: chain links with differing tags are skipped inside match.
        bci += 1;
    }
    if twiceelem != 0 {
        printf(c"Found %d sides that were multiply given\n".as_ptr(), twiceelem);
    }
    if sameelem != 0 {
        printf(
            c"Found %d side elements that have two parents.\n".as_ptr(),
            sameelem,
        );
    }
    if sideelem == sideelements {
        printf(c"Found correctly %d side elements.\n".as_ptr(), sideelem);
    } else {
        printf(
            c"Found %d side elements, could have found %d\n".as_ptr(),
            sideelem,
            sideelements,
        );
    }
    (*bound).nosides = sideelem;
    free_ivector(possible, 1, noknots);
    free_imatrix(invtopo, 1, noknots, 1, maxpossible);
    let _ = (noelements, foundtwo);
}
/// SAFETY: 2D periodic match for one bottom node (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn periodic_match2d(
    data: *mut FemType,
    indxper: *mut c_int,
    revindtop: *mut c_int,
    revindbot: *mut c_int,
    topn: c_int,
    botn: c_int,
    dim: c_int,
    eps: Real,
    tothits: &mut c_int,
    hit: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    i2: &mut c_int,
    j2: &mut c_int,
    dist: &mut Real,
) {
    *i = 1;
    while *i <= botn {
        *j = *revindbot.wrapping_offset(*i as isize);
        *hit = FALSE;
        *i2 = 1;
        while *i2 <= topn {
            *j2 = *revindtop.wrapping_offset(*i2 as isize);
            if dim == 1 {
                *dist = fabs(
                    *(*data).y.wrapping_offset(*j as isize)
                        - *(*data).y.wrapping_offset(*j2 as isize),
                );
            } else {
                *dist = fabs(
                    *(*data).x.wrapping_offset(*j as isize)
                        - *(*data).x.wrapping_offset(*j2 as isize),
                );
            }
            if *dist < eps {
                *hit = TRUE;
                // NOTE: C `goto hit2d` breaks the search.
                break;
            }
            *i2 += 1;
        }
        // NOTE: C `hit2d:` label.
        if *hit != 0 {
            *tothits += 1;
            if *indxper.wrapping_offset(*j as isize) == *j {
                *indxper.wrapping_offset(*j2 as isize) = *j;
            } else if *indxper.wrapping_offset(
                *indxper.wrapping_offset(*j as isize) as isize,
            ) == *indxper.wrapping_offset(*j as isize)
            {
                *indxper.wrapping_offset(*j2 as isize) =
                    *indxper.wrapping_offset(*j as isize);
            } else {
                printf(c"unknown 2d case!\n".as_ptr());
            }
        } else {
            printf(
                c"Couldn't find a periodic counterpart for node %d at [%.3lg %.3lg]]\n".as_ptr(),
                *j,
                *(*data).x.wrapping_offset(*j as isize),
                *(*data).y.wrapping_offset(*j as isize),
            );
        }
        *i += 1;
    }
}

/// SAFETY: 3D periodic match for one bottom node (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn periodic_match3d(
    data: *mut FemType,
    indxper: *mut c_int,
    revindtop: *mut c_int,
    revindbot: *mut c_int,
    topn: c_int,
    botn: c_int,
    dim: c_int,
    coordmax: Real,
    coordmin: Real,
    eps: Real,
    tothits: &mut c_int,
    hit: &mut c_int,
    dx: &mut Real,
    dy: &mut Real,
    dz: &mut Real,
    epsmin: &mut Real,
    i: &mut c_int,
    j: &mut c_int,
    i2: &mut c_int,
    j2: &mut c_int,
) {
    *dx = 0.0;
    *dy = 0.0;
    *dz = 0.0;
    *i = 1;
    while *i <= botn {
        *j = *revindbot.wrapping_offset(*i as isize);
        *hit = FALSE;
        *epsmin = coordmax - coordmin;
        *i2 = 1;
        while *i2 <= topn {
            *j2 = *revindtop.wrapping_offset(*i2 as isize);
            if dim == 1 {
                *dy = *(*data).y.wrapping_offset(*j as isize)
                    - *(*data).y.wrapping_offset(*j2 as isize);
                *dz = *(*data).z.wrapping_offset(*j as isize)
                    - *(*data).z.wrapping_offset(*j2 as isize);
            } else if dim == 2 {
                *dx = *(*data).x.wrapping_offset(*j as isize)
                    - *(*data).x.wrapping_offset(*j2 as isize);
                *dz = *(*data).z.wrapping_offset(*j as isize)
                    - *(*data).z.wrapping_offset(*j2 as isize);
            } else {
                *dx = *(*data).x.wrapping_offset(*j as isize)
                    - *(*data).x.wrapping_offset(*j2 as isize);
                *dy = *(*data).y.wrapping_offset(*j as isize)
                    - *(*data).y.wrapping_offset(*j2 as isize);
            }
            if *dx * *dx + *dy * *dy + *dz * *dz < eps * eps {
                *hit = TRUE;
                // NOTE: C `goto hit3d` breaks the search.
                break;
            }
            *i2 += 1;
        }
        // NOTE: C `hit3d:` label.
        if *hit != 0 {
            *tothits += 1;
            *indxper.wrapping_offset(*j2 as isize) =
                *indxper.wrapping_offset(*j as isize);
        } else {
            printf(
                c"The periodic counterpart for node %d was not found!\n".as_ptr(),
                *j,
            );
        }
        *i += 1;
    }
    let _ = epsmin;
}
/// Periodic knot pairs along flagged dimensions.
///
/// # Safety
/// `data`, `periodicdim` valid.
#[no_mangle]
pub unsafe extern "C" fn FindPeriodicNodes(
    data: *mut FemType,
    periodicdim: *mut c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut j2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dim: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hit: c_int = 0;
    // NOTE: `=0` decl init kept (read before any overwrite).
    let mut tothits: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut botn: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut topn: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindtop: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut revindbot: *mut c_int;
    // NOTE: `=1.0e-5`-scale decl init kept (eps set per dim).
    let mut eps: Real;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dist: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dx: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dy: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dz: Real = 0.0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut coordmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut coordmin: Real;
    // NOTE: null-init (set from data before reads).
    let mut coord: *mut Real;
    // NOTE: null-init (Rvector before guarded reads).
    let mut toparr: *mut Real;
    // NOTE: null-init (Rvector before guarded reads).
    let mut botarr: *mut Real;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut epsmin: Real = 0.0;
    if (*data).dim < 3 {
        *periodicdim.wrapping_offset(2) = 0;
    }
    if *periodicdim.wrapping_offset(0) == 0
        && *periodicdim.wrapping_offset(1) == 0
        && *periodicdim.wrapping_offset(2) == 0
    {
        return 1;
    }
    if (*data).periodicexist != 0 {
        printf(c"FindPeriodicNodes: Subroutine is called for second time?\n".as_ptr());
        return 2;
    }
    let noknots: c_int = (*data).noknots;
    tothits = 0;
    (*data).periodicexist = TRUE;
    let indxper: *mut c_int = ivector(1, noknots);
    (*data).periodic = indxper;
    let topbot: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *indxper.wrapping_offset(i as isize) = i;
        i += 1;
    }
    dim = 1;
    while dim <= 3 {
        if *periodicdim.wrapping_offset((dim - 1) as isize) == 0 {
            dim += 1;
            continue;
        }
        if info != 0 {
            printf(
                c"Finding periodic nodes in direction %d\n".as_ptr(),
                dim,
            );
        }
        if dim == 1 {
            coord = (*data).x;
        } else if dim == 2 {
            coord = (*data).y;
        } else {
            coord = (*data).z;
        }
        coordmax = *coord.wrapping_offset(1);
        coordmin = *coord.wrapping_offset(1);
        i = 1;
        while i <= (*data).noknots {
            // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
            if coordmax < *coord.wrapping_offset(i as isize) {
                coordmax = *coord.wrapping_offset(i as isize);
            }
            if *coord.wrapping_offset(i as isize) < coordmin {
                coordmin = *coord.wrapping_offset(i as isize);
            }
            i += 1;
        }
        if info != 0 {
            printf(
                c"Coordinate in dimension %d is at the interval [%.3lg, %.3lg]\n".as_ptr(),
                dim,
                coordmin,
                coordmax,
            );
        }
        if coordmax - coordmin < 1.0e-10 {
            dim += 1;
            continue;
        }
        eps = 1.0e-5 * (coordmax - coordmin);
        topn = 0;
        botn = 0;
        i = 1;
        while i <= (*data).noknots {
            if fabs(*coord.wrapping_offset(i as isize) - coordmax) < eps {
                topn += 1;
                *topbot.wrapping_offset(i as isize) = topn;
            } else if fabs(*coord.wrapping_offset(i as isize) - coordmin) < eps {
                botn += 1;
                *topbot.wrapping_offset(i as isize) = -botn;
            } else {
                *topbot.wrapping_offset(i as isize) = 0;
            }
            i += 1;
        }
        if topn != botn {
            printf(
                c"There should be equal number of top and bottom nodes (%d vs. %d)!\n".as_ptr(),
                topn,
                botn,
            );
            return 3;
        } else if info != 0 {
            printf(c"Looking for %d periodic nodes\n".as_ptr(), topn);
        }
        toparr = dvector(1, topn);
        botarr = dvector(1, botn);
        revindtop = ivector(1, topn);
        revindbot = ivector(1, botn);
        topn = 0;
        botn = 0;
        i = 1;
        while i <= noknots {
            j = *topbot.wrapping_offset(i as isize);
            if j > 0 {
                topn += 1;
                *revindtop.wrapping_offset(topn as isize) = i;
            } else if j < 0 {
                j = abs(j);
                botn += 1;
                *revindbot.wrapping_offset(botn as isize) = i;
            }
            i += 1;
        }
        if (*data).dim == 2 {
            periodic_match2d(
                data, indxper, revindtop, revindbot, topn, botn, dim, eps,
                &mut tothits, &mut hit, &mut i, &mut j, &mut i2, &mut j2,
                &mut dist,
            );
        } else if (*data).dim == 3 {
            periodic_match3d(
                data, indxper, revindtop, revindbot, topn, botn, dim, coordmax,
                coordmin, eps, &mut tothits, &mut hit, &mut dx, &mut dy, &mut dz,
                &mut epsmin, &mut i, &mut j, &mut i2, &mut j2,
            );
        }
        free_dvector(toparr, 1, topn);
        free_dvector(botarr, 1, botn);
        free_ivector(revindtop, 1, topn);
        free_ivector(revindbot, 1, botn);
        dim += 1;
    }
    if info != 0 {
        printf(c"Found all in all %d periodic nodes.\n".as_ptr(), tothits);
    }
    free_ivector(topbot, 1, noknots);
    0
}
/// SAFETY: periodic element table, counted then filled (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn periodic_parents_table(
    data: *mut FemType,
    indexper: *mut c_int,
    periodichits: *mut c_int,
    periodicparents: &mut *mut *mut c_int,
    maxhits: &mut c_int,
    nodes: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: C `setparents:` label + `goto setparents`; count then fill.
    loop {
        *j = 1;
        while *j <= (*data).noelements {
            *nodes = *(*data).elementtypes.wrapping_offset(*j as isize) % 100;
            *i = 0;
            while *i < *nodes {
                *k = *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*i as isize);
                if *k != *indexper.wrapping_offset(*k as isize) {
                    (*periodichits.wrapping_offset(*k as isize)) += 1;
                    if *maxhits > 0 {
                        *(*periodicparents.wrapping_offset(*k as isize)).wrapping_offset(
                            *periodichits.wrapping_offset(*k as isize) as isize,
                        ) = *j;
                    }
                }
                *i += 1;
            }
            *j += 1;
        }
        if *maxhits != 0 {
            break;
        }
        *i = 1;
        while *i <= (*data).noknots {
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxhits < *periodichits.wrapping_offset(*i as isize) {
                *maxhits = *periodichits.wrapping_offset(*i as isize);
            }
            *i += 1;
        }
        printf(
            c"Maximum number of elements associated with periodic nodes is %d\n".as_ptr(),
            *maxhits,
        );
        *periodicparents = imatrix(1, (*data).noknots, 1, *maxhits);
        *i = 1;
        while *i <= (*data).noknots {
            *periodichits.wrapping_offset(*i as isize) = 0;
            *j = 1;
            while *j <= *maxhits {
                *(*periodicparents.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize) = 0;
                *j += 1;
            }
            *i += 1;
        }
    }
}

/// SAFETY: second-parent search for one boundary side (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn periodic_parent_match(
    data: *mut FemType,
    bound: *mut BoundaryType,
    indexper: *mut c_int,
    periodictarget: *mut c_int,
    periodichits: *mut c_int,
    periodicparents: *mut *mut c_int,
    sideind: *mut c_int,
    sideind2: *mut c_int,
    sidenodes: c_int,
    sideelemtype: c_int,
    parent: c_int,
    j: c_int,
    i: c_int,
    totsides: &mut c_int,
    newsides: &mut c_int,
    hits: &mut c_int,
    hits2: &mut c_int,
    targetnode: &mut c_int,
    parent2: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    l2: &mut c_int,
    k2: &mut c_int,
    side: &mut c_int,
    elemtype: &mut c_int,
    elemsides: &mut c_int,
    sideelemtype2: &mut c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let mut found: c_int = FALSE;
    *totsides += 1;
    // NOTE: the matching parent shares the periodic node set.
    *l = 0;
    while *l < sidenodes {
        *targetnode = *periodictarget.wrapping_offset(*sideind.wrapping_offset(*l as isize) as isize);
        if *targetnode == 0 {
            *l += 1;
            continue;
        }
        *l2 = 1;
        while *l2 <= *periodichits.wrapping_offset(*targetnode as isize) {
            // NOTE: block-scoped C locals become bindings.
            let mut side_loc: c_int;
            // NOTE: block-scoped C locals become bindings.
            // NOTE: block-scoped C locals become bindings.
            *parent2 = *(*periodicparents.wrapping_offset(*targetnode as isize))
                .wrapping_offset(*l2 as isize);
            if parent == *parent2 {
                *l2 += 1;
                continue;
            }
            let elemtype_loc: c_int = *(*data).elementtypes.wrapping_offset(*parent2 as isize);
            let elemsides_loc: c_int = GetElementFaces(elemtype_loc);
            side_loc = 0;
            while side_loc < elemsides_loc {
                GetElementSide(*parent2, side_loc, 1, data, sideind2, sideelemtype2);
                if sideelemtype != *sideelemtype2 {
                    side_loc += 1;
                    continue;
                }
                *hits = 0;
                *k = 0;
                while *k < sidenodes {
                    *k2 = 0;
                    while *k2 < sidenodes {
                        if *indexper.wrapping_offset(
                            *sideind.wrapping_offset(*k as isize) as isize,
                        ) == *indexper.wrapping_offset(
                            *sideind2.wrapping_offset(*k2 as isize) as isize,
                        ) {
                            *hits += 1;
                            break;
                        }
                        *k2 += 1;
                    }
                    *k += 1;
                }
                // NOTE: C `goto found` breaks all three loops.
                if *hits == sidenodes {
                    found = TRUE;
                    break;
                }
                side_loc += 1;
            }
            if found != 0 {
                *side = side_loc;
                *elemtype = elemtype_loc;
                *elemsides = elemsides_loc;
                break;
            }
            *l2 += 1;
        }
        if found != 0 {
            break;
        }
        *l += 1;
    }
    // NOTE: C `found:` label.
    if *hits == sidenodes {
        *newsides += 1;
        // NOTE: `if(0)` trace omitted (dead in C).
        *(*bound.wrapping_offset(j as isize))
            .parent2
            .wrapping_offset(i as isize) = -*parent2;
    } else {
        printf(
            c"Could not find a periodic counterpart: %d/%d/%d\n".as_ptr(),
            j,
            i,
            parent,
        );
        printf(c"ind = %d ".as_ptr(), *sideind.wrapping_offset(0));
        *k = 1;
        while *k < sidenodes {
            printf(c"%d ".as_ptr(), *sideind.wrapping_offset(*k as isize));
            *k += 1;
        }
        printf(c"\n".as_ptr());
    }
    let _ = (hits2, side);
}
/// Secondary periodic parents for boundary sides.
///
/// # Safety
/// `data`, `bound` valid.
#[no_mangle]
pub unsafe extern "C" fn FindPeriodicParents(
    data: *mut FemType,
    bound: *mut BoundaryType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut totsides: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut newsides: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sidenodes: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxhits: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hits: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hits2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut targets: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut mappings: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut targetnode: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut parent: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent2: c_int;
    let mut sideind: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    let mut sideind2: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: null-init (Imatrix before guarded reads).
    let mut periodicparents: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (set from data before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemsides: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype2: c_int = 0;
    totsides = 0;
    newsides = 0;
    targets = 0;
    parent2 = 0;
    if info != 0 {
        printf(c"Finding secondary periodic parents for boundary elements\n".as_ptr());
    }
    if (*data).periodicexist == 0 {
        printf(c"FindPeriodicParents: Periodic nodes are not defined\n".as_ptr());
        return 2;
    }
    let indexper: *mut c_int = (*data).periodic;
    // NOTE: periodic-node reverse map.
    let noknots: c_int = (*data).noknots;
    let periodictarget: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *periodictarget.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    mappings = 0;
    i = 1;
    while i <= noknots {
        j = *indexper.wrapping_offset(i as isize);
        if j != i {
            mappings += 1;
            *periodictarget.wrapping_offset(j as isize) = i;
        }
        i += 1;
    }
    // NOTE: `if(0)` index dump omitted (dead in C).
    if info != 0 {
        printf(
            c"Number of potential periodic mappings is %d\n".as_ptr(),
            mappings,
        );
    }
    i = 1;
    while i <= noknots {
        if *periodictarget.wrapping_offset(i as isize) != 0 {
            targets += 1;
        }
        i += 1;
    }
    if info != 0 {
        printf(
            c"Number of potential periodic targets is %d\n".as_ptr(),
            targets,
        );
    }
    // NOTE: per-node associated-element counts.
    maxhits = 0;
    let periodichits: *mut c_int = ivector(1, noknots);
    i = 1;
    while i <= noknots {
        *periodichits.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    periodic_parents_table(
        data, indexper, periodichits, &mut periodicparents, &mut maxhits, &mut nodes,
        &mut i, &mut j, &mut k,
    );
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
            // NOTE: settled second parents are skipped.
            if *(*bound.wrapping_offset(j as isize))
                .parent2
                .wrapping_offset(i as isize)
                != 0
            {
                i += 1;
                continue;
            }
            parent = *(*bound.wrapping_offset(j as isize))
                .parent
                .wrapping_offset(i as isize);
            // NOTE: `if(0)` trace omitted (dead in C).
            side = *(*bound.wrapping_offset(j as isize))
                .side
                .wrapping_offset(i as isize);
            GetElementSide(
                parent,
                side,
                1,
                data,
                sideind.as_mut_ptr(),
                &mut sideelemtype,
            );
            sidenodes = sideelemtype % 100;
            // NOTE: one target plus mapped-or-target rest.
            hits = 0;
            hits2 = 0;
            k = 0;
            while k < sidenodes {
                l = sideind[k as usize];
                if *periodictarget.wrapping_offset(l as isize) != 0 {
                    hits += 1;
                } else if *indexper.wrapping_offset(l as isize) != l {
                    hits2 += 1;
                }
                k += 1;
            }
            if hits == 0 || hits + hits2 < sidenodes {
                i += 1;
                continue;
            }
            // NOTE: `if(0)` traces omitted (dead in C).
            periodic_parent_match(
                data, bound, indexper, periodictarget, periodichits, periodicparents,
                sideind.as_mut_ptr(), sideind2.as_mut_ptr(), sidenodes, sideelemtype,
                parent, j, i, &mut totsides, &mut newsides, &mut hits, &mut hits2,
                &mut targetnode, &mut parent2, &mut k, &mut l, &mut l2, &mut k2,
                &mut side, &mut elemtype, &mut elemsides, &mut sideelemtype2,
            );
            // NOTE: C `found:` label continues the side loop.
            i += 1;
        }
        j += 1;
    }
    free_ivector(periodictarget, 1, noknots);
    free_ivector(periodichits, 1, noknots);
    free_imatrix(periodicparents, 1, noknots, 1, maxhits);
    if info != 0 {
        printf(
            c"Found %d secondary parents for %d potential sides.\n".as_ptr(),
            newsides,
            totsides,
        );
    }
    0
}
/// SAFETY: boundary-layer census over extrudable sides (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_count(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layernumber: *mut c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    maxelemtype: c_int,
    layernode: *mut c_int,
    noknots: &mut c_int,
    nonewnodes: &mut c_int,
    nonewelements: &mut c_int,
    maxbc: &mut c_int,
    order: &mut c_int,
    midpoints: &mut c_int,
    dolayer: &mut c_int,
    use2: &mut c_int,
    parent: &mut c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    noelements: c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
) {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    *nonewnodes = 0;
    *nonewelements = 0;
    *maxbc = 0;
    // NOTE: active layers over created boundaries.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            *dolayer = FALSE;
            *parent = *(*bound.wrapping_offset(*j as isize))
                .parent
                .wrapping_offset(*i as isize);
            *use2 = FALSE;
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxbc
                < *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
            {
                *maxbc = *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize);
            }
            *k = 0;
            while *k < nolayers {
                if *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
                    == *layerbounds.wrapping_offset(*k as isize)
                {
                    if checkmaterials != 0 {
                        if *layerparents.wrapping_offset(*k as isize) < 0 {
                            *k += 1;
                            continue;
                        }
                        if *(*data).material.wrapping_offset(*parent as isize)
                            == *layerparents.wrapping_offset(*k as isize)
                        {
                            *dolayer = *k + 1;
                        } else {
                            // NOTE: assignment-in-condition from C, bound before the test.
                            *parent = *(*bound.wrapping_offset(*j as isize))
                                .parent2
                                .wrapping_offset(*i as isize);
                            if *parent != 0
                                && *(*data).material.wrapping_offset(*parent as isize)
                                    == *layerparents.wrapping_offset(*k as isize)
                            {
                                *use2 = TRUE;
                                *dolayer = *k + 1;
                            }
                        }
                    } else {
                        *dolayer = *k + 1;
                    }
                }
                *k += 1;
            }
            if *dolayer == 0 {
                *i += 1;
                continue;
            }
            // NOTE: extrudable side found.
            if *use2 != 0 {
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side2
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    ind,
                    sideelemtype,
                );
            } else {
                GetElementSide(
                    *parent,
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    ind,
                    sideelemtype,
                );
            }
            *nonewelements += *layernumber.wrapping_offset((*dolayer - 1) as isize);
            *midpoints = FALSE;
            if *sideelemtype == 202 {
                *order = 1;
            } else if *sideelemtype == 203 {
                *order = 2;
                if maxelemtype > 408 {
                    *midpoints = TRUE;
                }
            }
            *l = 0;
            while *l < *sideelemtype % 100 {
                // NOTE: first touch flags the layer base.
                if *layernode.wrapping_offset(
                    *ind.wrapping_offset(*l as isize) as isize,
                ) == 0
                {
                    *layernode.wrapping_offset(
                        *ind.wrapping_offset(*l as isize) as isize,
                    ) = -(*noknots + *nonewnodes);
                    if *l < *sideelemtype / 100 || *midpoints != 0 {
                        *nonewnodes += *order * *layernumber.wrapping_offset((*dolayer - 1) as isize);
                    } else {
                        *nonewnodes += *layernumber.wrapping_offset((*dolayer - 1) as isize);
                    }
                } else {
                    *layernode.wrapping_offset(
                        *ind.wrapping_offset(*l as isize) as isize,
                    ) = abs(*layernode.wrapping_offset(
                        *ind.wrapping_offset(*l as isize) as isize,
                    ));
                }
                *l += 1;
            }
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: higher-order mids leave the corner list.
    if maxelemtype % 100 > 4 {
        *j = 1;
        while *j <= noelements {
            elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            *i = elemtype / 100;
            while *i < elemtype % 100 {
                *k = *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*i as isize);
                *layernode.wrapping_offset(*k as isize) =
                    abs(*layernode.wrapping_offset(*k as isize));
                *i += 1;
            }
            *j += 1;
        }
    }
}
/// SAFETY: layer end-node connection search (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_ends(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    layernode: *mut c_int,
    noknots: c_int,
    endbcs: &mut c_int,
    endparents: &mut *mut c_int,
    endtypes: &mut *mut c_int,
    endnodes: &mut *mut c_int,
    endnodes2: &mut *mut c_int,
    endneighbours: &mut *mut c_int,
    maxbc: &mut c_int,
    dolayer: &mut c_int,
    parent: &mut c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    dx: &mut Real,
    dy: &mut Real,
    rectfactor: Real,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    i2: &mut c_int,
    i3: &mut c_int,
    info: c_int,
) {
    // NOTE: negative flags mark new boundary end nodes.
    *endbcs = 0;
    *i = 1;
    while *i <= noknots {
        if *layernode.wrapping_offset(*i as isize) < 0 {
            *endbcs += 1;
        }
        *i += 1;
    }
    if *endbcs != 0 {
        *endparents = ivector(1, *endbcs);
        *endtypes = ivector(1, *endbcs);
        *endnodes = ivector(1, *endbcs);
        *endnodes2 = ivector(1, *endbcs);
        *endneighbours = ivector(1, 2 * *endbcs);
        *i = 1;
        while *i <= *endbcs {
            *(*endparents).wrapping_offset(*i as isize) = 0;
            *(*endtypes).wrapping_offset(*i as isize) = 0;
            *(*endnodes).wrapping_offset(*i as isize) = 0;
            *(*endnodes2).wrapping_offset(*i as isize) = 0;
            *i += 1;
        }
        *endbcs = 0;
        *i = 1;
        while *i <= noknots {
            if *layernode.wrapping_offset(*i as isize) < 0 {
                *endbcs += 1;
                *(*endparents).wrapping_offset(*endbcs as isize) = *i;
            }
            *i += 1;
        }
    }
    // NOTE: connect each end node to an existing boundary if any.
    *i2 = 1;
    while *i2 <= *endbcs {
        *j = 0;
        while *j < MAXBOUNDARIES as c_int {
            if (*bound.wrapping_offset(*j as isize)).created == 0 {
                *j += 1;
                continue;
            }
            *i = 1;
            while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    ind,
                    sideelemtype,
                );
                // NOTE: the node must sit on this side.
                *dolayer = FALSE;
                *i3 = 0;
                while *i3 < *sideelemtype % 100 {
                    if *ind.wrapping_offset(*i3 as isize)
                        == *(*endparents).wrapping_offset(*i2 as isize)
                    {
                        *dolayer = TRUE;
                        break;
                    }
                    *i3 += 1;
                }
                if *dolayer == 0 {
                    *i += 1;
                    continue;
                }
                // NOTE: parent material must match a layer.
                *dolayer = FALSE;
                if checkmaterials != 0 {
                    *k = 0;
                    while *k < nolayers {
                        if *layerparents.wrapping_offset(*k as isize) < 0 {
                            *k += 1;
                            continue;
                        }
                        *parent = *(*bound.wrapping_offset(*j as isize))
                            .parent
                            .wrapping_offset(*i as isize);
                        if *(*data).material.wrapping_offset(*parent as isize)
                            == *layerparents.wrapping_offset(*k as isize)
                        {
                            *dolayer = TRUE;
                        } else {
                            *parent = *(*bound.wrapping_offset(*j as isize))
                                .parent2
                                .wrapping_offset(*i as isize);
                            if *parent != 0
                                && *(*data).material.wrapping_offset(*parent as isize)
                                    == *layerparents.wrapping_offset(*k as isize)
                            {
                                *dolayer = TRUE;
                            }
                        }
                        *k += 1;
                    }
                }
                if *dolayer == 0 {
                    *i += 1;
                    continue;
                }
                // NOTE: extruded boundaries donate neighbours instead.
                *dolayer = FALSE;
                *k = 0;
                while *k < nolayers {
                    if *layerparents.wrapping_offset(*k as isize) < 0 {
                        *k += 1;
                        continue;
                    }
                    if *(*bound.wrapping_offset(*j as isize))
                        .types
                        .wrapping_offset(*i as isize)
                        == *layerbounds.wrapping_offset(*k as isize)
                    {
                        *dolayer = TRUE;
                    }
                    *k += 1;
                }
                if *dolayer != 0 {
                    *(*endneighbours).wrapping_offset((2 * *i2 - 1) as isize) =
                        *ind.wrapping_offset((1 - *i3) as isize);
                    *i += 1;
                    continue;
                }
                *(*endtypes).wrapping_offset(*i2 as isize) = *(*bound
                    .wrapping_offset(*j as isize))
                .types
                .wrapping_offset(*i as isize);
                *dx = fabs(
                    *(*data).x.wrapping_offset(*ind.wrapping_offset(0) as isize)
                        - *(*data).x.wrapping_offset(*ind.wrapping_offset(1) as isize),
                );
                *dy = fabs(
                    *(*data).y.wrapping_offset(*ind.wrapping_offset(0) as isize)
                        - *(*data).y.wrapping_offset(*ind.wrapping_offset(1) as isize),
                );
                if *dx < rectfactor * *dy && *dy < rectfactor * *dx {
                    *(*endnodes).wrapping_offset(*i2 as isize) =
                        *ind.wrapping_offset(*i3 as isize);
                    if *sideelemtype % 100 > 2 {
                        *(*endnodes2).wrapping_offset(*i2 as isize) =
                            *ind.wrapping_offset(2);
                    }
                    *(*endneighbours).wrapping_offset((2 * *i2) as isize) =
                        *ind.wrapping_offset((1 - *i3) as isize);
                }
                if info != 0 {
                    printf(
                        c"Found an existing boundary %d for the single node %d %d\n".as_ptr(),
                        *(*bound.wrapping_offset(*j as isize))
                            .types
                            .wrapping_offset(*i as isize),
                        *(*endparents).wrapping_offset(*i2 as isize),
                        *(*endnodes).wrapping_offset(*i2 as isize),
                    );
                }
                // NOTE: C `goto foundbc` breaks both loops.
                break;
            }
            // NOTE: inner break lands here; outer continues unless found.
            if *(*endtypes).wrapping_offset(*i2 as isize) != 0
                || *(*endneighbours).wrapping_offset((2 * *i2 - 1) as isize) != 0
            {
                break;
            }
            *j += 1;
        }
        // NOTE: C `foundbc:` label.
        // NOTE: fresh type for unconnected end nodes.
        if *(*endtypes).wrapping_offset(*i2 as isize) == 0 {
            *maxbc += 1;
            *(*endtypes).wrapping_offset(*i2 as isize) = *maxbc;
        }
        *i2 += 1;
    }
}
/// SAFETY: enlarged-array allocation + old-data copy (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_alloc(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layernumber: *mut c_int,
    maxelemtype: c_int,
    endbcs: c_int,
    newbc: &mut c_int,
    noknots: &mut c_int,
    noelements: &mut c_int,
    oldnoknots: &mut c_int,
    oldnoelements: &mut c_int,
    nonewnodes: c_int,
    nonewelements: c_int,
    newtopo: &mut *mut *mut c_int,
    newmaterial: &mut *mut c_int,
    newelementtypes: &mut *mut c_int,
    newx: &mut *mut Real,
    newy: &mut *mut Real,
    newz: &mut *mut Real,
    elemwidth: &mut *mut Real,
    herit: &mut *mut c_int,
    topomap: &mut *mut c_int,
    inside: &mut *mut c_int,
    elemindx: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    info: c_int,
) {
    // NOTE: first free boundary slot for end BCs.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *newbc = *j;
            (*bound.wrapping_offset(*j as isize)).nosides = 0;
            break;
        }
        *j += 1;
    }
    // NOTE: deepest layer stack sizes the end BC.
    *i = 0;
    *k = 0;
    while *k < nolayers {
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *i < *layernumber.wrapping_offset(*k as isize) {
            *i = *layernumber.wrapping_offset(*k as isize);
        }
        *k += 1;
    }
    if endbcs != 0 {
        if info != 0 {
            printf(
                c"Allocating for additional %d boundary elements into bc %d.\n".as_ptr(),
                (*bound.wrapping_offset(*newbc as isize)).nosides,
                *newbc,
            );
        }
        AllocateBoundary(bound.wrapping_offset(*newbc as isize), *i * endbcs);
        (*bound.wrapping_offset(*newbc as isize)).created = FALSE;
        (*bound.wrapping_offset(*newbc as isize)).nosides = 0;
    }
    // NOTE: sizes of the enlarged mesh.
    *noknots = (*data).noknots + nonewnodes;
    *noelements = (*data).noelements + nonewelements;
    *oldnoelements = (*data).noelements;
    *oldnoknots = (*data).noknots;
    if info != 0 {
        printf(
            c"Creating additional %d elements and %d nodes.\n".as_ptr(),
            nonewelements,
            nonewnodes,
        );
        printf(
            c"Boundary layer mesh has %d elements and %d nodes.\n".as_ptr(),
            *noelements,
            *noknots,
        );
    }
    // NOTE: triangles grow an extra corner node.
    if maxelemtype <= 303 {
        (*data).maxnodes = 4;
    } else if maxelemtype == 306 {
        (*data).maxnodes = 8;
    }
    // NOTE: room for the enlarged data set.
    *newtopo = imatrix(1, *noelements, 0, (*data).maxnodes - 1);
    *newmaterial = ivector(1, *noelements);
    *newelementtypes = ivector(1, *noelements);
    *newx = dvector(1, *noknots);
    *newy = dvector(1, *noknots);
    *newz = dvector(1, *noknots);
    *i = 1;
    while *i <= *noknots {
        *(*newz).wrapping_offset(*i as isize) = 0.0;
        *i += 1;
    }
    *elemwidth = dvector(1, nonewelements);
    *i = 1;
    while *i <= nonewelements {
        *(*elemwidth).wrapping_offset(*i as isize) = 0.0;
        *i += 1;
    }
    *herit = ivector(1, *noknots);
    *i = 1;
    while *i <= *oldnoknots {
        *(*herit).wrapping_offset(*i as isize) = *i;
        *i += 1;
    }
    *i = *oldnoknots + 1;
    while *i <= *noknots {
        *(*herit).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    // NOTE: old topology first.
    *j = 1;
    while *j <= (*data).noelements {
        *(*newmaterial).wrapping_offset(*j as isize) =
            *(*data).material.wrapping_offset(*j as isize);
        *(*newelementtypes).wrapping_offset(*j as isize) =
            *(*data).elementtypes.wrapping_offset(*j as isize);
        *i = 0;
        while *i < *(*data).elementtypes.wrapping_offset(*j as isize) % 100 {
            *(*(*newtopo).wrapping_offset(*j as isize)).wrapping_offset(*i as isize) =
                *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*i as isize);
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: old nodes first (z comes from the layer fill).
    *i = 1;
    while *i <= (*data).noknots {
        *(*newx).wrapping_offset(*i as isize) = *(*data).x.wrapping_offset(*i as isize);
        *(*newy).wrapping_offset(*i as isize) = *(*data).y.wrapping_offset(*i as isize);
        *i += 1;
    }
    *topomap = ivector(1, *noknots);
    *i = 1;
    while *i <= *noknots {
        *(*topomap).wrapping_offset(*i as isize) = *i;
        *i += 1;
    }
    *inside = ivector(1, *noelements);
    *i = 1;
    while *i <= *noelements {
        *(*inside).wrapping_offset(*i as isize) = FALSE;
        *i += 1;
    }
    // NOTE: new elements grow after the old ones.
    *elemindx = (*data).noelements;
}
/// SAFETY: per-side layer node/element creation (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_grow(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layernumber: *mut c_int,
    layerratios: *mut Real,
    layerthickness: *mut Real,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    layernode: *mut c_int,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    elemwidth: *mut Real,
    herit: *mut c_int,
    topomap: *mut c_int,
    inside: *mut c_int,
    endparents: *mut c_int,
    endtypes: *mut c_int,
    endneighbours: *mut c_int,
    endbcs: c_int,
    newbc: c_int,
    oldnoelements: c_int,
    elemindx: &mut c_int,
    dolayer: &mut c_int,
    use2: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    order: &mut c_int,
    midpoints: &mut c_int,
    nodeindx: &mut c_int,
    ind: *mut c_int,
    sidebc: *mut c_int,
    sideelemtype: &mut c_int,
    elemtype: &mut c_int,
    n: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
    ds: &mut Real,
    q: &mut Real,
    p: &mut Real,
    ratio: &mut Real,
    dx: &mut Real,
    dy: &mut Real,
    e1x: &mut Real,
    e1y: &mut Real,
    e2x: &mut Real,
    e2y: &mut Real,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    i2: &mut c_int,
    i3: &mut c_int,
) {
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            *dolayer = FALSE;
            *parent = *(*bound.wrapping_offset(*j as isize))
                .parent
                .wrapping_offset(*i as isize);
            *parent2 = *(*bound.wrapping_offset(*j as isize))
                .parent2
                .wrapping_offset(*i as isize);
            *use2 = FALSE;
            *k = 0;
            while *k < nolayers {
                if *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
                    == *layerbounds.wrapping_offset(*k as isize)
                {
                    if checkmaterials != 0 {
                        if *layerparents.wrapping_offset(*k as isize) < 0 {
                            *k += 1;
                            continue;
                        }
                        if *(*data).material.wrapping_offset(*parent as isize)
                            == *layerparents.wrapping_offset(*k as isize)
                        {
                            *dolayer = *k + 1;
                        } else if *parent2 != 0 {
                            *l = *parent;
                            *parent = *parent2;
                            *parent2 = *l;
                            if *(*data).material.wrapping_offset(*parent as isize)
                                == *layerparents.wrapping_offset(*k as isize)
                            {
                                *use2 = TRUE;
                                *dolayer = *k + 1;
                            }
                        }
                    } else {
                        *dolayer = *k + 1;
                    }
                }
                *k += 1;
            }
            if *dolayer == 0 {
                *i += 1;
                continue;
            }
            if *use2 != 0 {
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent2
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side2
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    ind,
                    sideelemtype,
                );
            } else {
                GetElementSide(
                    *(*bound.wrapping_offset(*j as isize))
                        .parent
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .side
                        .wrapping_offset(*i as isize),
                    *(*bound.wrapping_offset(*j as isize))
                        .normal
                        .wrapping_offset(*i as isize),
                    data,
                    ind,
                    sideelemtype,
                );
            }
            *inside.wrapping_offset(*parent as isize) = 1;
            if *sideelemtype == 202 {
                *order = 1;
            } else if *sideelemtype == 203 {
                *order = 2;
            }
            // NOTE: end nodes seed side BCs on the new boundary.
            *i2 = 0;
            while *i2 < *sideelemtype % 100 {
                *sidebc.wrapping_offset(*i2 as isize) = FALSE;
                if *i2 < 2
                    && *layernode.wrapping_offset(
                        *ind.wrapping_offset(*i2 as isize) as isize,
                    ) < 0
                {
                    *layernode.wrapping_offset(
                        *ind.wrapping_offset(*i2 as isize) as isize,
                    ) = abs(*layernode.wrapping_offset(
                        *ind.wrapping_offset(*i2 as isize) as isize,
                    ));
                    *sidebc.wrapping_offset(*i2 as isize) = TRUE;
                }
                *i2 += 1;
            }
            // NOTE: outward surface normal.
            *dy = -(*(*data).x.wrapping_offset(
                *ind.wrapping_offset(1) as isize,
            ) - *(*data).x.wrapping_offset(*ind.wrapping_offset(0) as isize));
            *dx = *(*data).y.wrapping_offset(*ind.wrapping_offset(1) as isize)
                - *(*data).y.wrapping_offset(*ind.wrapping_offset(0) as isize);
            *ds = sqrt(*dx * *dx + *dy * *dy);
            *dx /= *ds;
            *dy /= *ds;
            *n = *layernumber.wrapping_offset((*dolayer - 1) as isize);
            *ds = -*layerthickness.wrapping_offset((*dolayer - 1) as isize);
            *l = 0;
            while *l < *n {
                *elemindx += 1;
                *newmaterial.wrapping_offset(*elemindx as isize) =
                    *(*data).material.wrapping_offset(*parent as isize);
                *inside.wrapping_offset(*elemindx as isize) = 1;
                if *n <= 1
                    || fabs(
                        *layerratios.wrapping_offset((*dolayer - 1) as isize) - 1.0,
                    ) < 0.001
                {
                    *q = (1.0 * (*l + 1) as Real) / *n as Real;
                    *elemwidth.wrapping_offset((*elemindx - oldnoelements) as isize) =
                        *ds / *n as Real;
                } else {
                    *ratio = pow(
                        *layerratios.wrapping_offset((*dolayer - 1) as isize),
                        -1.0 / (*n - 1) as Real,
                    );
                    *q = (1.0 - pow(*ratio, (*l + 1) as Real))
                        / (1.0 - pow(*ratio, *n as Real));
                    *p = (1.0 - pow(*ratio, *l as Real))
                        / (1.0 - pow(*ratio, *n as Real));
                    *elemwidth.wrapping_offset((*elemindx - oldnoelements) as isize) =
                        (*q - *p) * *ds;
                }
                *m = 0;
                while *m < *sideelemtype % 100 {
                    // NOTE: side BCs on the fresh boundary.
                    if *sidebc.wrapping_offset(*m as isize) != 0 {
                        (*bound.wrapping_offset(newbc as isize)).nosides += 1;
                        *i2 = (*bound.wrapping_offset(newbc as isize)).nosides;
                        *(*bound.wrapping_offset(newbc as isize))
                            .parent
                            .wrapping_offset(*i2 as isize) = *elemindx;
                        *(*bound.wrapping_offset(newbc as isize))
                            .parent2
                            .wrapping_offset(*i2 as isize) = 0;
                        *(*bound.wrapping_offset(newbc as isize))
                            .side
                            .wrapping_offset(*i2 as isize) = 3 - 2 * *m;
                        *(*bound.wrapping_offset(newbc as isize))
                            .side2
                            .wrapping_offset(*i2 as isize) = 0;
                        *i3 = 1;
                        while *i3 <= endbcs {
                            if *ind.wrapping_offset(*m as isize)
                                == *endparents.wrapping_offset(*i3 as isize)
                            {
                                *(*bound.wrapping_offset(newbc as isize))
                                    .types
                                    .wrapping_offset(*i2 as isize) =
                                    *endtypes.wrapping_offset(*i3 as isize);
                                *endneighbours.wrapping_offset((2 * *i3 - 1) as isize) =
                                    *layernode.wrapping_offset(
                                        *ind.wrapping_offset(*m as isize) as isize,
                                    ) + 1;
                                break;
                            }
                            *i3 += 1;
                        }
                    }
                    // NOTE: new node coordinates along the normal.
                    if *m < 2 {
                        *nodeindx = *layernode.wrapping_offset(
                            *ind.wrapping_offset(*m as isize) as isize,
                        ) + *order * (*l + 1);
                    } else {
                        *nodeindx = *layernode.wrapping_offset(
                            *ind.wrapping_offset(*m as isize) as isize,
                        ) + (1 + *midpoints) * (*l + 1);
                    }
                    *e1x = *dx * *q * *ds;
                    *e1y = *dy * *q * *ds;
                    // NOTE: joined nodes average the normals.
                    if *herit.wrapping_offset(*nodeindx as isize) != 0 {
                        *e2x = *newx.wrapping_offset(*nodeindx as isize)
                            - *(*data).x.wrapping_offset(
                                *ind.wrapping_offset(*m as isize) as isize,
                            );
                        *e2y = *newy.wrapping_offset(*nodeindx as isize)
                            - *(*data).y.wrapping_offset(
                                *ind.wrapping_offset(*m as isize) as isize,
                            );
                        *p = (*e1x * *e2x + *e1y * *e2y)
                            / (sqrt(*e1x * *e1x + *e1y * *e1y)
                                * sqrt(*e2x * *e2x + *e2y * *e2y));
                        *newx.wrapping_offset(*nodeindx as isize) += *e1x - *p * *e2x;
                        *newy.wrapping_offset(*nodeindx as isize) += *e1y - *p * *e2y;
                    } else {
                        *herit.wrapping_offset(*nodeindx as isize) =
                            *ind.wrapping_offset(*m as isize);
                        *newx.wrapping_offset(*nodeindx as isize) = *(*data)
                            .x
                            .wrapping_offset(
                                *ind.wrapping_offset(*m as isize) as isize
                            )
                            + *e1x;
                        *newy.wrapping_offset(*nodeindx as isize) = *(*data)
                            .y
                            .wrapping_offset(
                                *ind.wrapping_offset(*m as isize) as isize
                            )
                            + *e1y;
                    }
                    *m += 1;
                }
                // NOTE: bulk element topology of the layer.
                if *l == 0 {
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(3) =
                        *ind.wrapping_offset(0);
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(2) =
                        *ind.wrapping_offset(1);
                    if *order == 2 {
                        *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(6) =
                            *ind.wrapping_offset(2);
                    }
                } else {
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(3) =
                        *layernode.wrapping_offset(*ind.wrapping_offset(0) as isize)
                            + *order * *l;
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(2) =
                        *layernode.wrapping_offset(*ind.wrapping_offset(1) as isize)
                            + *order * *l;
                    if *order == 2 {
                        *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(6) =
                            *layernode.wrapping_offset(*ind.wrapping_offset(2) as isize)
                                + (*midpoints + 1) * *l;
                    }
                }
                *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(0) =
                    *layernode.wrapping_offset(*ind.wrapping_offset(0) as isize)
                        + *order * (*l + 1);
                *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(1) =
                    *layernode.wrapping_offset(*ind.wrapping_offset(1) as isize)
                        + *order * (*l + 1);
                if *order == 2 {
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(7) =
                        *layernode.wrapping_offset(*ind.wrapping_offset(0) as isize)
                            + *order * *l
                            + 1;
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(5) =
                        *layernode.wrapping_offset(*ind.wrapping_offset(1) as isize)
                            + *order * *l
                            + 1;
                    *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(4) =
                        *layernode.wrapping_offset(*ind.wrapping_offset(2) as isize)
                            + (*midpoints + 1) * (*l + 1);
                    if *midpoints != 0 {
                        *(*newtopo.wrapping_offset(*elemindx as isize)).wrapping_offset(8) =
                            *layernode.wrapping_offset(*ind.wrapping_offset(2) as isize)
                                + 2 * *l
                                + 1;
                    }
                }
                if *order == 1 {
                    *newelementtypes.wrapping_offset(*elemindx as isize) = 404;
                } else if *midpoints != 0 {
                    *newelementtypes.wrapping_offset(*elemindx as isize) = 409;
                } else {
                    *newelementtypes.wrapping_offset(*elemindx as isize) = 408;
                }
                if *l == *n - 1 && *parent2 != 0 {
                    *elemtype = *(*data).elementtypes.wrapping_offset(*parent2 as isize);
                    *inside.wrapping_offset(*parent2 as isize) = 2;
                    *i2 = 0;
                    while *i2 < *elemtype % 100 {
                        *i3 = 0;
                        while *i3 < *sideelemtype % 100 {
                            if *(*(*data).topology.wrapping_offset(*parent2 as isize))
                                .wrapping_offset(*i2 as isize)
                                == *ind.wrapping_offset(*i3 as isize)
                            {
                                if *i3 < 2 {
                                    *topomap.wrapping_offset(
                                        *ind.wrapping_offset(*i3 as isize) as isize,
                                    ) = *layernode.wrapping_offset(
                                        *ind.wrapping_offset(*i3 as isize) as isize,
                                    ) + *order * *n;
                                } else {
                                    *topomap.wrapping_offset(
                                        *ind.wrapping_offset(*i3 as isize) as isize,
                                    ) = *layernode.wrapping_offset(
                                        *ind.wrapping_offset(*i3 as isize) as isize,
                                    ) + (*midpoints + 1) * *n;
                                }
                            }
                            *i3 += 1;
                        }
                        *i2 += 1;
                    }
                }
                *l += 1;
            }
            // NOTE: the BC now points at the fresh boundary.
            if *use2 != 0 {
                *(*bound.wrapping_offset(*j as isize))
                    .side2
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .parent2
                    .wrapping_offset(*i as isize) = *elemindx;
            } else {
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize) = 0;
                *(*bound.wrapping_offset(*j as isize))
                    .parent
                    .wrapping_offset(*i as isize) = *elemindx;
            }
            *i += 1;
        }
        *j += 1;
    }
}
/// SAFETY: outside-element marking + topology remap (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_outside(
    data: *mut FemType,
    nolayers: c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    newtopo: *mut *mut c_int,
    topomap: *mut c_int,
    inside: *mut c_int,
    noelements: c_int,
    oldnoelements: c_int,
    elemtype: &mut c_int,
    dolayer: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    // NOTE: block-scoped C local becomes a binding.
    let inside2: *mut c_int = ivector(1, (*data).noknots);
    *i = 1;
    while *i <= (*data).noknots {
        *inside2.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    // NOTE: nodes of outside elements seed the flood.
    *j = 1;
    while *j <= noelements {
        if *inside.wrapping_offset(*j as isize) == 2 {
            *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            *i = 0;
            while *i < *elemtype / 100 {
                *inside2.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*i as isize)
                        as isize,
                ) = TRUE;
                *i += 1;
            }
        }
        *j += 1;
    }
    // NOTE: elements sharing two outside nodes join them.
    *j = 1;
    while *j <= noelements {
        if *inside.wrapping_offset(*j as isize) == 0 {
            *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            *k = 0;
            *i = 0;
            while *i < *elemtype / 100 {
                if *inside2.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*i as isize)
                        as isize,
                ) != 0
                {
                    *k += 1;
                }
                *i += 1;
            }
            if *k > 1 {
                *inside.wrapping_offset(*j as isize) = 2;
            }
        }
        *j += 1;
    }
    free_ivector(inside2, 1, (*data).noknots);
    // NOTE: inactive materials count as outside.
    if checkmaterials != 0 {
        *j = 1;
        while *j <= oldnoelements {
            *dolayer = FALSE;
            *k = 0;
            while *k < nolayers {
                if *(*data).material.wrapping_offset(*j as isize)
                    == *layerparents.wrapping_offset(*k as isize)
                {
                    *dolayer = TRUE;
                }
                *k += 1;
            }
            if *dolayer == 0 {
                if *inside.wrapping_offset(*j as isize) == 1 {
                    printf(
                        c"Element %d of material %d should be in the inside\n".as_ptr(),
                        *j,
                        *(*data).material.wrapping_offset(*j as isize),
                    );
                }
                *inside.wrapping_offset(*j as isize) = 2;
            }
            *j += 1;
        }
    }
    // NOTE: outside elements follow the compacted map.
    *j = 1;
    while *j <= noelements {
        if *inside.wrapping_offset(*j as isize) == 2 {
            *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            *i = 0;
            while *i < *elemtype % 100 {
                *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*i as isize) =
                    *topomap.wrapping_offset(
                        *(*(*data).topology.wrapping_offset(*j as isize))
                            .wrapping_offset(*i as isize) as isize,
                    );
                *i += 1;
            }
        }
        *j += 1;
    }
}
/// SAFETY: smoother fixed-node setup (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_filter_setup(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    newx: *mut Real,
    newy: *mut Real,
    herit: *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    oldnoknots: c_int,
    noknots: c_int,
    noelements: c_int,
    rectfactor: Real,
    endbcs: c_int,
    endnodes: *mut c_int,
    endnodes2: *mut c_int,
    fixedx: *mut c_int,
    fixedy: *mut c_int,
    _weights: *mut Real,
    aidx: *mut Real,
    aidy: *mut Real,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    dolayer: &mut c_int,
    use2: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    elemtype: &mut c_int,
    l: &mut c_int,
    dx: &mut Real,
    dy: &mut Real,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
) {
    *i = 1;
    while *i <= noknots {
        *fixedx.wrapping_offset(*i as isize) = 0;
        *fixedy.wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    // NOTE: non-layer materials pin both components.
    if checkmaterials != 0 {
        *j = 1;
        while *j <= noelements {
            *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            *dolayer = FALSE;
            *k = 0;
            while *k < nolayers {
                if *(*data).material.wrapping_offset(*j as isize)
                    == *layerparents.wrapping_offset(*k as isize)
                {
                    *dolayer = TRUE;
                }
                *k += 1;
            }
            *i = 0;
            while *i < *elemtype / 100 {
                *ind.wrapping_offset(0) = *(*(*data).topology.wrapping_offset(*j as isize))
                    .wrapping_offset(*i as isize);
                if *dolayer != 0 && *fixedx.wrapping_offset(*ind.wrapping_offset(0) as isize) % 2 == 0
                {
                    (*fixedx.wrapping_offset(*ind.wrapping_offset(0) as isize)) += 1;
                }
                if *dolayer == 0 && *fixedx.wrapping_offset(*ind.wrapping_offset(0) as isize) < 2
                {
                    (*fixedx.wrapping_offset(*ind.wrapping_offset(0) as isize)) += 2;
                }
                *i += 1;
            }
            *j += 1;
        }
        *i = 1;
        while *i <= noknots {
            if *fixedx.wrapping_offset(*i as isize) == 2 {
                *fixedy.wrapping_offset(*i as isize) = 2;
            } else {
                *fixedx.wrapping_offset(*i as isize) = 0;
            }
            *i += 1;
        }
    }
    // NOTE: straight BC sides pin the normal component.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            GetElementSide(
                *(*bound.wrapping_offset(*j as isize))
                    .parent
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .normal
                    .wrapping_offset(*i as isize),
                data,
                ind,
                sideelemtype,
            );
            *dx = fabs(
                *newx.wrapping_offset(*ind.wrapping_offset(0) as isize)
                    - *newx.wrapping_offset(*ind.wrapping_offset(1) as isize),
            );
            *dy = fabs(
                *newy.wrapping_offset(*ind.wrapping_offset(0) as isize)
                    - *newy.wrapping_offset(*ind.wrapping_offset(1) as isize),
            );
            if *dx > rectfactor * *dy {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *fixedy.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *l += 1;
                }
            } else if *dy > rectfactor * *dx {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *fixedx.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *l += 1;
                }
            } else {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *fixedy.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *fixedx.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *l += 1;
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: active layer BCs pin both components.
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            *dolayer = FALSE;
            *parent = *(*bound.wrapping_offset(*j as isize))
                .parent
                .wrapping_offset(*i as isize);
            *parent2 = *(*bound.wrapping_offset(*j as isize))
                .parent2
                .wrapping_offset(*i as isize);
            *use2 = FALSE;
            GetElementSide(
                *(*bound.wrapping_offset(*j as isize))
                    .parent
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .normal
                    .wrapping_offset(*i as isize),
                data,
                ind,
                sideelemtype,
            );
            *k = 0;
            while *k < nolayers {
                if *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
                    == *layerbounds.wrapping_offset(*k as isize)
                {
                    if checkmaterials != 0 {
                        if *layerparents.wrapping_offset(*k as isize) < 0 {
                            *k += 1;
                            continue;
                        }
                        if *(*data).material.wrapping_offset(*parent as isize)
                            == *layerparents.wrapping_offset(*k as isize)
                        {
                            *dolayer = *k + 1;
                        } else if *parent2 != 0 {
                            *l = *parent;
                            *parent = *parent2;
                            *parent2 = *l;
                            if *(*data).material.wrapping_offset(*parent as isize)
                                == *layerparents.wrapping_offset(*k as isize)
                            {
                                *use2 = TRUE;
                                *dolayer = *k + 1;
                            }
                        }
                    } else {
                        *dolayer = *k + 1;
                    }
                }
                *k += 1;
            }
            if *dolayer != 0 {
                *l = 0;
                while *l < *sideelemtype % 100 {
                    *fixedy.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *fixedx.wrapping_offset(*ind.wrapping_offset(*l as isize) as isize) = TRUE;
                    *l += 1;
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    // NOTE: triple end nodes float free.
    *j = 1;
    while *j <= endbcs {
        *k = *endnodes.wrapping_offset(*j as isize);
        if *k != 0 {
            *fixedx.wrapping_offset(*k as isize) = FALSE;
            *fixedy.wrapping_offset(*k as isize) = FALSE;
        }
        *k = *endnodes2.wrapping_offset(*j as isize);
        if *k != 0 {
            *fixedx.wrapping_offset(*k as isize) = FALSE;
            *fixedy.wrapping_offset(*k as isize) = FALSE;
        }
        *j += 1;
    }
    // NOTE: anchor the fixed rows.
    *i = 1;
    while *i <= noknots {
        if *fixedx.wrapping_offset(*i as isize) != 0 {
            if *i <= oldnoknots {
                *aidx.wrapping_offset(*i as isize) = *oldx.wrapping_offset(*i as isize);
                *newx.wrapping_offset(*i as isize) = *oldx.wrapping_offset(*i as isize);
            } else {
                *aidx.wrapping_offset(*i as isize) =
                    *oldx.wrapping_offset(*herit.wrapping_offset(*i as isize) as isize);
                *newx.wrapping_offset(*i as isize) =
                    *oldx.wrapping_offset(*herit.wrapping_offset(*i as isize) as isize);
            }
        }
        if *fixedy.wrapping_offset(*i as isize) != 0 {
            if *i <= oldnoknots {
                *aidy.wrapping_offset(*i as isize) = *oldy.wrapping_offset(*i as isize);
                *newy.wrapping_offset(*i as isize) = *oldy.wrapping_offset(*i as isize);
            } else {
                *aidy.wrapping_offset(*i as isize) =
                    *oldy.wrapping_offset(*herit.wrapping_offset(*i as isize) as isize);
                *newy.wrapping_offset(*i as isize) =
                    *oldy.wrapping_offset(*herit.wrapping_offset(*i as isize) as isize);
            }
        }
        *i += 1;
    }
}
/// SAFETY: one Laplacian smoother sweep (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_filter_sweep(
    data: *mut FemType,
    oldtopo: *mut *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    elemwidth: *mut Real,
    fixedx: *mut c_int,
    fixedy: *mut c_int,
    weights: *mut Real,
    aidx: *mut Real,
    aidy: *mut Real,
    newx: *mut Real,
    newy: *mut Real,
    endnodes: *mut c_int,
    endneighbours: *mut c_int,
    herit: *mut c_int,
    endbcs: c_int,
    oldnoknots: c_int,
    oldnoelements: c_int,
    noknots: c_int,
    noelements: c_int,
    maxelemtype: c_int,
    layereps: Real,
    maxerror: &mut Real,
    minds: &mut Real,
    method: c_int,
    fii: &mut Real,
    ds: &mut Real,
    ds2: &mut Real,
    dx: &mut Real,
    dy: &mut Real,
    dx2: &mut Real,
    dy2: &mut Real,
    dolayer: &mut c_int,
    elemtype: &mut c_int,
    nonodes: &mut c_int,
    ind1: &mut c_int,
    ind2: &mut c_int,
    ind3: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    i2: &mut c_int,
    i3: &mut c_int,
    iter: c_int,
) {
    *maxerror = 0.0;
    *minds = 1.0e10;
    *j = 1;
    while *j <= noknots {
        *weights.wrapping_offset(*j as isize) = 0.0;
        if *fixedx.wrapping_offset(*j as isize) == 0 {
            *aidx.wrapping_offset(*j as isize) = *newx.wrapping_offset(*j as isize);
            *newx.wrapping_offset(*j as isize) = 0.0;
        }
        if *fixedy.wrapping_offset(*j as isize) == 0 {
            *aidy.wrapping_offset(*j as isize) = *newy.wrapping_offset(*j as isize);
            *newy.wrapping_offset(*j as isize) = 0.0;
        }
        *j += 1;
    }
    *j = 1;
    while *j <= noelements {
        *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
        *nonodes = *elemtype / 100;
        *i = 0;
        while *i < *nonodes {
            *i2 = (*i + 1) % *nonodes;
            *i3 = (*i + 2) % *nonodes;
            *ind1 = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            *ind2 = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i2 as isize);
            *ind3 = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i3 as isize);
            if *j <= oldnoelements {
                *dx = *oldx.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i2 as isize)
                        as isize,
                ) - *oldx.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i as isize)
                        as isize,
                );
                *dy = *oldy.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i2 as isize)
                        as isize,
                ) - *oldy.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i as isize)
                        as isize,
                );
                *ds = sqrt(*dx * *dx + *dy * *dy);
            } else {
                *ds = fabs(*elemwidth.wrapping_offset((*j - oldnoelements) as isize));
            }
            if *ds < *minds {
                *minds = *ds;
            }
            if *j <= oldnoelements {
                *dx2 = *oldx.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i2 as isize)
                        as isize,
                ) - *oldx.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i3 as isize)
                        as isize,
                );
                *dy2 = *oldy.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i2 as isize)
                        as isize,
                ) - *oldy.wrapping_offset(
                    *(*oldtopo.wrapping_offset(*j as isize)).wrapping_offset(*i3 as isize)
                        as isize,
                );
                *ds2 = sqrt(*dx2 * *dx2 + *dy2 * *dy2);
            } else {
                *ds2 = fabs(*elemwidth.wrapping_offset((*j - oldnoelements) as isize));
            }
            if *j <= oldnoelements && *ds * *ds2 < 1.0e-50 {
                printf(
                    c"problem elem %d and nodes %d (%d %d)\n".as_ptr(),
                    *j,
                    *i2,
                    *i,
                    *i3,
                );
                printf(c"dist ds=%.3e ds2=%.3e\n".as_ptr(), *ds, *ds2);
                printf(
                    c"coord: %.3e %.3e\n".as_ptr(),
                    *oldx.wrapping_offset(
                        *(*oldtopo.wrapping_offset(*j as isize))
                            .wrapping_offset(*i2 as isize) as isize,
                    ),
                    *oldy.wrapping_offset(
                        *(*oldtopo.wrapping_offset(*j as isize))
                            .wrapping_offset(*i2 as isize) as isize,
                    ),
                );
                *i += 1;
                continue;
            }
            // NOTE: angle-weighted (method 2) edge contributions.
            if method.abs() == 2 && *j <= oldnoelements {
                *fii = acos((*dx * *dx2 + *dy * *dy2) / (*ds * *ds2)) / (FM_PI / 2.0);
            } else if method.abs() == 3 && *j <= oldnoelements {
                *fii = acos((*dx * *dx2 + *dy * *dy2) / (*ds * *ds2));
                *fii = sin(*fii);
            } else {
                *fii = 1.0;
            }
            // NOTE: difficult triple nodes borrow one-sided.
            *dolayer = FALSE;
            *k = 1;
            while *k <= endbcs {
                if *ind2 == *endnodes.wrapping_offset(*k as isize) {
                    *dolayer = *k;
                }
                *k += 1;
            }
            if *dolayer != 0 {
                *k = 1;
                while *k <= 2 {
                    if *endneighbours.wrapping_offset((2 * (*dolayer - 1) + *k) as isize)
                        == *ind1
                    {
                        *weights.wrapping_offset(*ind2 as isize) += *fii / *ds;
                        if *fixedx.wrapping_offset(*ind2 as isize) == 0 {
                            *newx.wrapping_offset(*ind2 as isize) +=
                                *aidx.wrapping_offset(*ind1 as isize) * *fii / *ds;
                        }
                        if *fixedy.wrapping_offset(*ind2 as isize) == 0 {
                            *newy.wrapping_offset(*ind2 as isize) +=
                                *aidy.wrapping_offset(*ind1 as isize) * *fii / *ds;
                        }
                    }
                    *k += 1;
                }
                *k = 1;
                while *k <= 2 {
                    if *endneighbours.wrapping_offset((2 * (*dolayer - 1) + *k) as isize)
                        == *ind3
                    {
                        *weights.wrapping_offset(*ind2 as isize) += *fii / *ds2;
                        if *fixedx.wrapping_offset(*ind2 as isize) == 0 {
                            *newx.wrapping_offset(*ind2 as isize) +=
                                *aidx.wrapping_offset(*ind3 as isize) * *fii / *ds2;
                        }
                        if *fixedy.wrapping_offset(*ind2 as isize) == 0 {
                            *newy.wrapping_offset(*ind2 as isize) +=
                                *aidy.wrapping_offset(*ind3 as isize) * *fii / *ds2;
                        }
                    }
                    *k += 1;
                }
            } else {
                if *ind2 <= oldnoknots
                    || *herit.wrapping_offset(*ind1 as isize)
                        == *herit.wrapping_offset(*ind2 as isize)
                {
                    *weights.wrapping_offset(*ind2 as isize) += *fii / *ds;
                    if *fixedx.wrapping_offset(*ind2 as isize) == 0 {
                        *newx.wrapping_offset(*ind2 as isize) +=
                            *aidx.wrapping_offset(*ind1 as isize) * *fii / *ds;
                    }
                    if *fixedy.wrapping_offset(*ind2 as isize) == 0 {
                        *newy.wrapping_offset(*ind2 as isize) +=
                            *aidy.wrapping_offset(*ind1 as isize) * *fii / *ds;
                    }
                }
                if *ind2 <= oldnoknots
                    || *herit.wrapping_offset(*ind3 as isize)
                        == *herit.wrapping_offset(*ind2 as isize)
                {
                    *weights.wrapping_offset(*ind2 as isize) += *fii / *ds2;
                    if *fixedx.wrapping_offset(*ind2 as isize) == 0 {
                        *newx.wrapping_offset(*ind2 as isize) +=
                            *aidx.wrapping_offset(*ind3 as isize) * *fii / *ds2;
                    }
                    if *fixedy.wrapping_offset(*ind2 as isize) == 0 {
                        *newy.wrapping_offset(*ind2 as isize) +=
                            *aidy.wrapping_offset(*ind3 as isize) * *fii / *ds2;
                    }
                }
            }
            *i += 1;
        }
        *j += 1;
    }
    let _ = (herit, endneighbours, maxelemtype, layereps, iter);
}
/// SAFETY: sweep normalization + error (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_sweep_tail(
    newx: *mut Real,
    newy: *mut Real,
    weights: *mut Real,
    aidx: *mut Real,
    aidy: *mut Real,
    fixedx: *mut c_int,
    fixedy: *mut c_int,
    nonlin: *mut c_int,
    noknots: c_int,
    maxelemtype: c_int,
    layereps: Real,
    maxerror: &mut Real,
    minds: Real,
    dx: &mut Real,
    dy: &mut Real,
    ds: &mut Real,
    iter: c_int,
    j: &mut c_int,
) {
    if maxelemtype % 100 > 4 {
        *j = 1;
        while *j <= noknots {
            if *nonlin.wrapping_offset(*j as isize) != 0 {
                *j += 1;
                continue;
            }
            if *weights.wrapping_offset(*j as isize) > 1.0e-50 {
                if *fixedx.wrapping_offset(*j as isize) == 0 {
                    *newx.wrapping_offset(*j as isize) /=
                        *weights.wrapping_offset(*j as isize);
                }
                if *fixedy.wrapping_offset(*j as isize) == 0 {
                    *newy.wrapping_offset(*j as isize) /=
                        *weights.wrapping_offset(*j as isize);
                }
            } else if iter == 1 {
                printf(c"no weight for index %d\n".as_ptr(), *j);
            }
            *dx = *newx.wrapping_offset(*j as isize) - *aidx.wrapping_offset(*j as isize);
            *dy = *newy.wrapping_offset(*j as isize) - *aidy.wrapping_offset(*j as isize);
            *ds = *dx * *dx + *dy * *dy;
            if *ds > *maxerror {
                *maxerror = *ds;
            }
            *j += 1;
        }
    } else {
        *j = 1;
        while *j <= noknots {
            if *fixedx.wrapping_offset(*j as isize) == 0 {
                *newx.wrapping_offset(*j as isize) /= *weights.wrapping_offset(*j as isize);
            }
            if *fixedy.wrapping_offset(*j as isize) == 0 {
                *newy.wrapping_offset(*j as isize) /= *weights.wrapping_offset(*j as isize);
            }
            *dx = *newx.wrapping_offset(*j as isize) - *aidx.wrapping_offset(*j as isize);
            *dy = *newy.wrapping_offset(*j as isize) - *aidy.wrapping_offset(*j as isize);
            *ds = *dx * *dx + *dy * *dy;
            if *ds > *maxerror {
                *maxerror = *ds;
            }
            *j += 1;
        }
    }
    *maxerror = sqrt(*maxerror) / minds;
    let _ = layereps;
}

/// SAFETY: higher-order midnodes follow corners (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_midnodes(
    data: *mut FemType,
    newtopo: *mut *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    fixedx: *mut c_int,
    fixedy: *mut c_int,
    noelements: c_int,
    oldnoelements: c_int,
    maxelemtype: c_int,
    elemtype: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    info: c_int,
    filtered: c_int,
) {
    if maxelemtype % 100 <= 4 {
        return;
    }
    if filtered != 0 && info != 0 {
        printf(c"Marking the higher order nodes\n".as_ptr());
    }
    if filtered == 0 {
        if info != 0 {
            printf(c"Making the higher order nodes to lie in between\n".as_ptr());
        }
        *j = oldnoelements + 1;
        while *j <= noelements {
            *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
            if *elemtype % 100 <= *elemtype / 100 {
                *j += 1;
                continue;
            }
            if *elemtype == 408 || *elemtype == 409 {
                if *elemtype == 409 {
                    *newx.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                    ) = 0.0;
                    *newy.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                    ) = 0.0;
                }
                *k = 0;
                while *k < 4 {
                    // NOTE: unfiltered pass always centers.
                    *newx.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                            as isize,
                    ) = 0.5
                        * (*newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 4) as isize)
                                as isize,
                        ));
                    *newy.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                            as isize,
                    ) = 0.5
                        * (*newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 4) as isize)
                                as isize,
                        ));
                    if *elemtype == 409 {
                        *newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                        ) += 0.25
                            * *newx.wrapping_offset(
                                *(*newtopo.wrapping_offset(*j as isize))
                                    .wrapping_offset(*k as isize) as isize,
                            );
                        *newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                        ) += 0.25
                            * *newy.wrapping_offset(
                                *(*newtopo.wrapping_offset(*j as isize))
                                    .wrapping_offset(*k as isize) as isize,
                            );
                    }
                    *k += 1;
                }
            }
            *j += 1;
        }
        return;
    }
    // NOTE: filtered pass centers 306/408/409 mids unless fixed.
    *j = 1;
    while *j <= noelements {
        *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
        if *elemtype % 100 <= *elemtype / 100 {
            *j += 1;
            continue;
        }
        if *elemtype == 306 {
            *k = 0;
            while *k < 3 {
                if *fixedx.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 3) as isize)
                        as isize,
                ) == 0
                {
                    *newx.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 3) as isize)
                            as isize,
                    ) = 0.5
                        * (*newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 3) as isize)
                                as isize,
                        ));
                }
                if *fixedy.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 3) as isize)
                        as isize,
                ) == 0
                {
                    *newy.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 3) as isize)
                            as isize,
                    ) = 0.5
                        * (*newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 3) as isize)
                                as isize,
                        ));
                }
                *k += 1;
            }
        } else if *elemtype == 408 || *elemtype == 409 {
            if *elemtype == 409 {
                *newx.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                ) = 0.0;
                *newy.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                ) = 0.0;
            }
            *k = 0;
            while *k < 4 {
                if *fixedx.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                        as isize,
                ) == 0
                {
                    *newx.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                            as isize,
                    ) = 0.5
                        * (*newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 4) as isize)
                                as isize,
                        ));
                }
                if *fixedy.wrapping_offset(
                    *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                        as isize,
                ) == 0
                {
                    *newy.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset((*k + 4) as isize)
                            as isize,
                    ) = 0.5
                        * (*newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        ) + *newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize))
                                .wrapping_offset(((*k + 1) % 4) as isize)
                                as isize,
                        ));
                }
                if *elemtype == 409 {
                    *newx.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                    ) += 0.25
                        * *newx.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        );
                    *newy.wrapping_offset(
                        *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(8) as isize,
                    ) += 0.25
                        * *newy.wrapping_offset(
                            *(*newtopo.wrapping_offset(*j as isize)).wrapping_offset(*k as isize)
                                as isize,
                        );
                }
                *k += 1;
            }
        } else {
            printf(c"Unknown elementtype %d\n".as_ptr(), *elemtype);
        }
        *j += 1;
    }
}
/// Extruded viscous boundary layers along flagged sides.
///
/// # Safety
/// `data`, `bound`, layer tables valid.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn CreateBoundaryLayer(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layernumber: *mut c_int,
    layerratios: *mut Real,
    layerthickness: *mut Real,
    layerparents: *mut c_int,
    mut maxfilters: c_int,
    mut layereps: Real,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut m: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut n: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut i3: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxbc: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut newbc: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut noknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut noelements: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemindx: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nodeindx: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnoknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnoelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldmaxnodes: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonewnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonewelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dolayer: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut order: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut midpoints: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut checkmaterials: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent2: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut use2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut second: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dx: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dy: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ds: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ratio: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut q: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut p: Real = 0.0;
    // NOTE: `=1.0e2` decl init kept (read before any overwrite).
    let rectfactor: Real = 1.0e2;
    // NOTE: null-init (Rvector before guarded reads).
    let mut newx: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (Rvector before guarded reads).
    let mut newy: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (Rvector before guarded reads).
    let mut newz: *mut Real = core::ptr::null_mut();
    // NOTE: null-init (set from data before reads).
    let mut oldx: *mut Real;
    // NOTE: null-init (set from data before reads).
    let mut oldy: *mut Real;
    // NOTE: null-init (Rvector before guarded reads).
    let mut elemwidth: *mut Real = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut e1x: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut e1y: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut e2x: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut e2y: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut sidebc: [c_int; MAXNODESD1] = [0; MAXNODESD1];
    // NOTE: null-init (Ivector before guarded reads).
    let mut layernode: *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut newelementtypes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Imatrix before guarded reads).
    let mut newtopo: *mut *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Imatrix before guarded reads).
    let mut oldtopo: *mut *mut c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut topomap: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut newmaterial: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut herit: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut inside: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut nonlin: *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut endbcs: c_int = 0;
    // NOTE: null-init (Ivector before guarded reads).
    let mut endparents: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut endtypes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut endnodes: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut endnodes2: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut endneighbours: *mut c_int = core::ptr::null_mut();
    // NOTE: filter locals live in the filter block below.
    // NOTE: `if(0)` trace omitted (dead in C).
    if maxfilters == 0 {
        maxfilters = 1000;
    }
    if layereps < 1.0e-20 {
        layereps = 1.0e-3;
    }
    midpoints = FALSE;
    order = 1;
    // NOTE: dead store in C too (dim unused below).
    let _dim: c_int = (*data).dim;
    let maxelemtype: c_int = GetMaxElementType(data);
    if maxelemtype > 409 {
        printf(c"Subroutine implemented only up to 2nd degree in 2D!\n".as_ptr());
        bigerror(c"Cannot continue".as_ptr());
    }
    if info != 0 {
        printf(c"Largest elementtype is %d\n".as_ptr(), maxelemtype);
    }
    second = FALSE;
    checkmaterials = FALSE;
    k = 0;
    while k < nolayers {
        if *layerparents.wrapping_offset(k as isize) != 0 {
            checkmaterials = TRUE;
        }
        k += 1;
    }
    // NOTE: C `omstart:` label + `goto omstart`; negative-material rerun.
    loop {
        oldnoelements = (*data).noelements;
        oldnoknots = (*data).noknots;
        noelements = (*data).noelements;
        noknots = (*data).noknots;
        oldmaxnodes = (*data).maxnodes;
        layernode = ivector(1, oldnoknots);
        i = 1;
        while i <= oldnoknots {
            *layernode.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        layer_count(
            data, bound, nolayers, layerbounds, layernumber, layerparents,
            checkmaterials, maxelemtype, layernode, &mut noknots, &mut nonewnodes,
            &mut nonewelements, &mut maxbc, &mut order, &mut midpoints, &mut dolayer,
            &mut use2, &mut parent, ind.as_mut_ptr(), &mut sideelemtype, noelements,
            &mut i, &mut j, &mut k, &mut l,
        );
        if nonewelements == 0 {
            if info != 0 {
                printf(c"Found no active boundary layers!\n".as_ptr());
            }
            return 0;
        }
        layer_ends(
            data, bound, nolayers, layerbounds, layerparents, checkmaterials,
            layernode, noknots, &mut endbcs, &mut endparents, &mut endtypes,
            &mut endnodes, &mut endnodes2, &mut endneighbours, &mut maxbc,
            &mut dolayer, &mut parent, ind.as_mut_ptr(), &mut sideelemtype, &mut dx,
            &mut dy, rectfactor, &mut i, &mut j, &mut k, &mut i2, &mut i3, info,
        );
        layer_alloc(
            data, bound, nolayers, layernumber, maxelemtype, endbcs, &mut newbc,
            &mut noknots, &mut noelements, &mut oldnoknots, &mut oldnoelements,
            nonewnodes, nonewelements, &mut newtopo, &mut newmaterial,
            &mut newelementtypes, &mut newx, &mut newy, &mut newz, &mut elemwidth,
            &mut herit, &mut topomap, &mut inside, &mut elemindx, &mut i, &mut j,
            &mut k, info,
        );
        layer_grow(
            data, bound, nolayers, layerbounds, layernumber, layerratios,
            layerthickness, layerparents, checkmaterials, layernode, newtopo,
            newmaterial, newelementtypes, newx, newy, elemwidth, herit, topomap,
            inside, endparents, endtypes, endneighbours, endbcs, newbc,
            oldnoelements, &mut elemindx, &mut dolayer, &mut use2, &mut parent,
            &mut parent2, &mut order, &mut midpoints, &mut nodeindx,
            ind.as_mut_ptr(), sidebc.as_mut_ptr(), &mut sideelemtype, &mut elemtype,
            &mut n, &mut l, &mut m, &mut ds, &mut q, &mut p, &mut ratio, &mut dx,
            &mut dy, &mut e1x, &mut e1y, &mut e2x, &mut e2y, &mut i, &mut j,
            &mut k, &mut i2, &mut i3,
        );
        layer_outside(
            data, nolayers, layerparents, checkmaterials, newtopo, topomap, inside,
            noelements, oldnoelements, &mut elemtype, &mut dolayer, &mut i, &mut j,
            &mut k,
        );
        // NOTE: swap to the enlarged arrays, freeing the old rows.
        oldx = (*data).x;
        oldy = (*data).y;
        oldtopo = (*data).topology;
        (*data).noelements = noelements;
        (*data).noknots = noknots;
        (*data).x = newx;
        (*data).y = newy;
        (*data).z = newz;
        free_ivector((*data).elementtypes, 1, oldnoelements);
        (*data).elementtypes = newelementtypes;
        free_ivector((*data).material, 1, oldnoelements);
        (*data).material = newmaterial;
        (*data).topology = newtopo;
        // NOTE: higher-order corner flags for the smoother.
        if maxelemtype % 100 > 4 {
            if info != 0 {
                printf(c"Marking the higher order nodes\n".as_ptr());
            }
            nonlin = ivector(1, noknots);
            i = 1;
            while i <= noknots {
                *nonlin.wrapping_offset(i as isize) = FALSE;
                i += 1;
            }
            j = 1;
            while j <= noelements {
                elemtype = *(*data).elementtypes.wrapping_offset(j as isize);
                i = elemtype / 100;
                while i < elemtype % 100 {
                    k = *(*(*data).topology.wrapping_offset(j as isize))
                        .wrapping_offset(i as isize);
                    *nonlin.wrapping_offset(k as isize) = TRUE;
                    i += 1;
                }
                j += 1;
            }
        }
        if maxfilters != 0 {
            layer_filter(
                data, bound, nolayers, layerbounds, layerparents, checkmaterials,
                oldtopo, oldx, oldy, elemwidth, herit, nonlin, endnodes, endnodes2,
                endneighbours, endbcs, ind.as_mut_ptr(), maxfilters, layereps,
                rectfactor, maxelemtype, oldnoelements, oldnoknots, noknots,
                noelements, newx, newy, newtopo, &mut elemtype, &mut nonodes,
                &mut sideelemtype, &mut dolayer, &mut use2, &mut parent,
                &mut parent2, &mut i, &mut j, &mut k, &mut l, info,
            );
        }
        if layer_finish(
            data, bound, nolayers, layerparents, checkmaterials, newtopo, newx,
            newy, layernode, oldtopo, oldx, oldy, oldnoelements, oldnoknots,
            oldmaxnodes, noelements, noknots, newbc, maxelemtype, &mut elemtype,
            &mut j, &mut k, &mut second, info,
        ) {
            continue;
        }
        break;
    }
    0
}
/// SAFETY: Laplacian smoother driver (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_filter(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    oldtopo: *mut *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    elemwidth: *mut Real,
    herit: *mut c_int,
    nonlin: *mut c_int,
    endnodes: *mut c_int,
    endnodes2: *mut c_int,
    endneighbours: *mut c_int,
    endbcs: c_int,
    ind: *mut c_int,
    maxfilters: c_int,
    layereps: Real,
    rectfactor: Real,
    maxelemtype: c_int,
    oldnoelements: c_int,
    oldnoknots: c_int,
    noknots: c_int,
    noelements: c_int,
    newx: *mut Real,
    newy: *mut Real,
    newtopo: *mut *mut c_int,
    elemtype: &mut c_int,
    nonodes: &mut c_int,
    sideelemtype: &mut c_int,
    dolayer: &mut c_int,
    use2: &mut c_int,
    parent: &mut c_int,
    parent2: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    info: c_int,
) {
    // NOTE: block-scoped C locals become bindings.
    let mut iter: c_int;
    let mut ind1: c_int = 0;
    let mut ind2: c_int = 0;
    let mut ind3: c_int = 0;
    let mut maxerror: Real = 0.0;
    let mut minds: Real = 0.0;
    let mut dx2: Real = 0.0;
    let mut dy2: Real = 0.0;
    let mut ds2: Real = 0.0;
    let mut fii: Real = 0.0;
    let mut dx: Real = 0.0;
    let mut dy: Real = 0.0;
    let mut ds: Real = 0.0;
    let mut l_loc: c_int = 0;
    let m_loc: c_int = 0;
    let n_loc: c_int = 0;
    let mut i2_loc: c_int = 0;
    let mut i3_loc: c_int = 0;
    // NOTE: angle-weighted smoothing, method 2 preferred.
    let method: c_int = 2;
    if info != 0 {
        printf(c"Filtering the mesh to meet the original geometry\n".as_ptr());
    }
    let fixedx: *mut c_int = ivector(1, noknots);
    let fixedy: *mut c_int = ivector(1, noknots);
    let weights: *mut Real = dvector(1, noknots);
    let aidx: *mut Real = dvector(1, noknots);
    let aidy: *mut Real = dvector(1, noknots);
    layer_filter_setup(
        data, bound, nolayers, layerbounds, layerparents, checkmaterials, newx,
        newy, herit, oldx, oldy, oldnoknots, noknots, noelements, rectfactor,
        endbcs, endnodes, endnodes2, fixedx, fixedy, weights, aidx, aidy, ind,
        sideelemtype, dolayer, use2, parent, parent2, elemtype, &mut l_loc, &mut dx,
        &mut dy, i, j, k,
    );
    // NOTE: fixed-node census.
    *j = 0;
    *i = 1;
    while *i <= noknots {
        if *fixedx.wrapping_offset(*i as isize) != 0 {
            *j += 1;
        }
        *i += 1;
    }
    if info != 0 {
        printf(c"Number of fixed nodes in x-direction is %d\n".as_ptr(), *j);
    }
    *j = 0;
    *i = 1;
    while *i <= noknots {
        if *fixedy.wrapping_offset(*i as isize) != 0 {
            *j += 1;
        }
        *i += 1;
    }
    if info != 0 {
        printf(c"Number of fixed nodes in y-direction is %d\n".as_ptr(), *j);
    }
    iter = 1;
    while iter <= maxfilters {
        layer_filter_sweep(
            data, oldtopo, oldx, oldy, elemwidth, fixedx, fixedy, weights, aidx,
            aidy, newx, newy, endnodes, endneighbours, herit, endbcs, oldnoknots,
            oldnoelements, noknots, noelements, maxelemtype, layereps, &mut maxerror,
            &mut minds, method, &mut fii, &mut ds, &mut ds2, &mut dx, &mut dy,
            &mut dx2, &mut dy2, dolayer, elemtype, nonodes, &mut ind1, &mut ind2,
            &mut ind3, i, j, k, &mut i2_loc, &mut i3_loc, iter,
        );
        layer_sweep_tail(
            newx, newy, weights, aidx, aidy, fixedx, fixedy, nonlin, noknots,
            maxelemtype, layereps, &mut maxerror, minds, &mut dx, &mut dy, &mut ds,
            iter, j,
        );
        if maxerror < layereps {
            break;
        }
        iter += 1;
    }
    if info != 0 {
        printf(
            c"Filtered the new node coordinates %d times with final error %.3e.\n".as_ptr(),
            iter - 1,
            maxerror,
        );
    }
    // NOTE: filtered higher-order mids follow corners.
    layer_midnodes(
        data, newtopo, newx, newy, fixedx, fixedy, noelements, oldnoelements,
        maxelemtype, elemtype, j, k, info, 1,
    );
    free_ivector(fixedx, 1, noknots);
    free_ivector(fixedy, 1, noknots);
    free_dvector(aidx, 1, noknots);
    free_dvector(aidy, 1, noknots);
    free_dvector(weights, 1, noknots);
    let _ = (l, m_loc, n_loc, i2_loc, i3_loc, l_loc, ds2, fii);
}
/// SAFETY: post-filter teardown + negative-material rerun (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn layer_finish(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerparents: *mut c_int,
    checkmaterials: c_int,
    newtopo: *mut *mut c_int,
    newx: *mut Real,
    newy: *mut Real,
    layernode: *mut c_int,
    oldtopo: *mut *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    oldnoelements: c_int,
    oldnoknots: c_int,
    oldmaxnodes: c_int,
    noelements: c_int,
    noknots: c_int,
    newbc: c_int,
    maxelemtype: c_int,
    elemtype: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    second: &mut c_int,
    info: c_int,
) -> bool {
    if (*bound.wrapping_offset(newbc as isize)).nosides > 0 {
        (*bound.wrapping_offset(newbc as isize)).created = TRUE;
    }
    // NOTE: unfiltered higher-order mids always center.
    layer_midnodes(
        data, newtopo, newx, newy, core::ptr::null_mut(), core::ptr::null_mut(),
        noelements, oldnoelements, maxelemtype, elemtype, j, k, info, 0,
    );
    // NOTE: `#if 0` reorder omitted (dead in C).
    free_imatrix(oldtopo, 1, oldnoelements, 0, oldmaxnodes - 1);
    free_ivector(layernode, 1, oldnoknots);
    free_dvector(oldx, 1, oldnoknots);
    free_dvector(oldy, 1, oldnoknots);
    if info != 0 {
        printf(c"Boundary layers created successfully.\n".as_ptr());
    }
    if checkmaterials != 0 && *second == 0 {
        *k = 0;
        while *k < nolayers {
            if *layerparents.wrapping_offset(*k as isize) < 0 {
                *second = TRUE;
            }
            *layerparents.wrapping_offset(*k as isize) =
                -*layerparents.wrapping_offset(*k as isize);
            *k += 1;
        }
        if *second != 0 {
            if info != 0 {
                printf(c"\nPerforming boundary layer generation again for negative materials\n".as_ptr());
            }
            // NOTE: C `goto omstart` reruns the whole pass.
            return true;
        }
    }
    let _ = (noknots, noelements);
    false
}
/// SAFETY: divide census + enlarged allocation (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn divide_census(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layernumber: *mut c_int,
    layerratios: *mut Real,
    layerthickness: *mut Real,
    maxelemtype: &mut c_int,
    layernode: &mut *mut c_int,
    sharednode: &mut *mut c_int,
    maxbc: &mut c_int,
    elemtype: &mut c_int,
    nlayer: &mut c_int,
    slayer: &mut Real,
    qlayer: &mut Real,
    order: &mut c_int,
    midpoints: &mut c_int,
    dolayer: &mut c_int,
    parent: &mut c_int,
    ind: *mut c_int,
    sideelemtype: &mut c_int,
    bcnodes: &mut c_int,
    maxcon: &mut c_int,
    nonewelements: &mut c_int,
    oldnoelements: &mut c_int,
    oldnoknots: &mut c_int,
    oldmaxnodes: &mut c_int,
    dim: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    l: &mut c_int,
    info: c_int,
) -> bool {
    *dim = (*data).dim;
    *maxelemtype = GetMaxElementType(data);
    if *maxelemtype > 409 {
        printf(c"Subroutine implemented only up to 2nd degree!\n".as_ptr());
        return false;
    }
    if info != 0 {
        printf(c"Largest elementtype is %d\n".as_ptr(), *maxelemtype);
    }
    *oldnoelements = (*data).noelements;
    *oldnoknots = (*data).noknots;
    *oldmaxnodes = (*data).maxnodes;
    *layernode = ivector(1, *oldnoknots);
    *i = 1;
    while *i <= *oldnoknots {
        *(*layernode).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    *sharednode = ivector(1, *oldnoknots);
    *i = 1;
    while *i <= *oldnoknots {
        *(*sharednode).wrapping_offset(*i as isize) = 0;
        *i += 1;
    }
    // NOTE: surface node census over active layers.
    *maxbc = 0;
    *qlayer = 0.0;
    *slayer = 0.0;
    *nlayer = 0;
    *j = 0;
    while *j < MAXBOUNDARIES as c_int {
        if (*bound.wrapping_offset(*j as isize)).created == 0 {
            *j += 1;
            continue;
        }
        *i = 1;
        while *i <= (*bound.wrapping_offset(*j as isize)).nosides {
            *dolayer = FALSE;
            *parent = *(*bound.wrapping_offset(*j as isize))
                .parent
                .wrapping_offset(*i as isize);
            // NOTE: C MAX macro (egdef.h:97); arms pure.
            if *maxbc
                < *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
            {
                *maxbc = *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize);
            }
            *k = 0;
            while *k < nolayers {
                if *(*bound.wrapping_offset(*j as isize))
                    .types
                    .wrapping_offset(*i as isize)
                    == *layerbounds.wrapping_offset(*k as isize)
                {
                    *nlayer = *layernumber.wrapping_offset(*k as isize);
                    *slayer = *layerthickness.wrapping_offset(*k as isize);
                    *qlayer = *layerratios.wrapping_offset(*k as isize);
                    *dolayer = TRUE;
                }
                *k += 1;
            }
            if *dolayer == 0 {
                *i += 1;
                continue;
            }
            GetElementSide(
                *parent,
                *(*bound.wrapping_offset(*j as isize))
                    .side
                    .wrapping_offset(*i as isize),
                *(*bound.wrapping_offset(*j as isize))
                    .normal
                    .wrapping_offset(*i as isize),
                data,
                ind,
                sideelemtype,
            );
            *midpoints = FALSE;
            if *sideelemtype == 202 {
                *order = 1;
            } else if *sideelemtype == 203 {
                *order = 2;
                if *maxelemtype > 408 {
                    *midpoints = TRUE;
                }
            }
            *l = 0;
            while *l < *sideelemtype % 100 {
                (*(*layernode).wrapping_offset(
                    *ind.wrapping_offset(*l as isize) as isize,
                )) += 1;
                *l += 1;
            }
            *i += 1;
        }
        *j += 1;
    }
    if *slayer > 1.0 || *slayer < 1.0e-20 {
        *slayer = 1.0;
    }
    *bcnodes = 0;
    *maxcon = 0;
    *i = 1;
    while *i <= (*data).noknots {
        if *(*layernode).wrapping_offset(*i as isize) != 0 {
            *bcnodes += 1;
        }
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if *maxcon < *(*layernode).wrapping_offset(*i as isize) {
            *maxcon = *(*layernode).wrapping_offset(*i as isize);
        }
        *i += 1;
    }
    if info != 0 {
        printf(
            c"Found %d new nodes in the boundary layers!\n".as_ptr(),
            *bcnodes,
        );
    }
    if *bcnodes == 0 {
        return false;
    }
    if info != 0 {
        printf(c"There are %d connections at maximum\n".as_ptr(), *maxcon);
    }
    // NOTE: triangles grow an extra corner node.
    if *maxelemtype <= 303 {
        (*data).maxnodes = 4;
    } else if *maxelemtype == 306 {
        (*data).maxnodes = 8;
    }
    // NOTE: conservative new-element estimate.
    *nonewelements = 0;
    *j = 1;
    while *j <= (*data).noelements {
        *dolayer = 0;
        *elemtype = *(*data).elementtypes.wrapping_offset(*j as isize);
        *i = 0;
        while *i < *elemtype % 100 {
            *k = *(*(*data).topology.wrapping_offset(*j as isize))
                .wrapping_offset(*i as isize);
            if *(*layernode).wrapping_offset(*k as isize) != 0 {
                *(*sharednode).wrapping_offset(*k as isize) += 1;
                *dolayer += 1;
            }
            *i += 1;
        }
        if *dolayer != 0 {
            *nonewelements += *nlayer;
            if *dolayer != 2 {
                *nonewelements += *nlayer + 1;
            }
        }
        *j += 1;
    }
    printf(c"There will %d new elements\n".as_ptr(), *nonewelements);
    true
}
/// SAFETY: shared edge-pair node lookup (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn divide_edge_nodes(
    oldx: *mut Real,
    oldy: *mut Real,
    newx: *mut Real,
    newy: *mut Real,
    edgepairs: *mut *mut c_int,
    basenode: *mut c_int,
    topnode: *mut c_int,
    baseind: *mut c_int,
    x0: *mut Real,
    y0: *mut Real,
    dx: *mut Real,
    dy: *mut Real,
    nlayer: c_int,
    slayer: Real,
    qlayer: Real,
    q: &mut Real,
    ratio: &mut Real,
    nonewnodes: c_int,
    noknots: &mut c_int,
    i: &mut c_int,
    k: &mut c_int,
) {
    *k = 0;
    while *k <= 1 {
        *i = 1;
        while *i <= nonewnodes {
            if *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(1) == 0 {
                break;
            }
            if *basenode.wrapping_offset(*k as isize)
                == *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(1)
                && *topnode.wrapping_offset(*k as isize)
                    == *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(2)
            {
                break;
            }
            *i += 1;
        }
        if *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(1) == 0 {
            *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(1) =
                *basenode.wrapping_offset(*k as isize);
            *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(*k as isize);
            *baseind.wrapping_offset(*k as isize) = *noknots;
            *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(3) =
                *baseind.wrapping_offset(*k as isize);
            *noknots += nlayer;
        } else {
            // NOTE: `if(0)` trace omitted (dead in C).
            *baseind.wrapping_offset(*k as isize) =
                *(*edgepairs.wrapping_offset(*i as isize)).wrapping_offset(3);
        }
        *x0.wrapping_offset(*k as isize) = *oldx.wrapping_offset(
            *basenode.wrapping_offset(*k as isize) as isize,
        );
        *y0.wrapping_offset(*k as isize) = *oldy.wrapping_offset(
            *basenode.wrapping_offset(*k as isize) as isize,
        );
        *dx.wrapping_offset(*k as isize) = *oldx.wrapping_offset(
            *topnode.wrapping_offset(*k as isize) as isize,
        ) - *x0.wrapping_offset(*k as isize);
        *dy.wrapping_offset(*k as isize) = *oldy.wrapping_offset(
            *topnode.wrapping_offset(*k as isize) as isize,
        ) - *y0.wrapping_offset(*k as isize);
        *i = 1;
        while *i <= nlayer {
            if nlayer <= 1 || fabs(qlayer - 1.0) < 0.001 {
                *q = (1.0 * *i as Real) / (nlayer + 1) as Real;
            } else {
                *ratio = pow(qlayer, 1.0 / nlayer as Real);
                *q = (1.0 - pow(*ratio, 1.0 * *i as Real))
                    / (1.0 - pow(*ratio, 1.0 + nlayer as Real));
            }
            *q *= slayer;
            *newx.wrapping_offset(
                (*baseind.wrapping_offset(*k as isize) + *i) as isize,
            ) = *x0.wrapping_offset(*k as isize) + *q * *dx.wrapping_offset(*k as isize);
            *newy.wrapping_offset(
                (*baseind.wrapping_offset(*k as isize) + *i) as isize,
            ) = *y0.wrapping_offset(*k as isize) + *q * *dy.wrapping_offset(*k as isize);
            *i += 1;
        }
        *k += 1;
    }
}
/// SAFETY: quadrilateral divide arm (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn divide_quad(
    _data: *mut FemType,
    oldtopo: *mut *mut c_int,
    layernode: *mut c_int,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    newx: *mut Real,
    newy: *mut Real,
    edgepairs: *mut *mut c_int,
    basenode: *mut c_int,
    topnode: *mut c_int,
    baseind: *mut c_int,
    x0: *mut Real,
    y0: *mut Real,
    dx: *mut Real,
    dy: *mut Real,
    nlayer: c_int,
    slayer: Real,
    qlayer: Real,
    q: &mut Real,
    ratio: &mut Real,
    nonewnodes: c_int,
    noknots: &mut c_int,
    j: c_int,
    elemsides: c_int,
    elemhits: &mut c_int,
    elemdone: &mut c_int,
    elemind: &mut c_int,
    side: &mut c_int,
    goforit: &mut c_int,
    k: &mut c_int,
    i: &mut c_int,
) {
    *elemdone = FALSE;
    *side = 0;
    while *side < elemsides {
        *goforit = FALSE;
        if (*elemhits == 2 || *elemhits == 3)
            && *layernode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize)
                    as isize,
            ) != 0
            && *layernode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize))
                    .wrapping_offset(((*side + 1) % elemsides) as isize)
                    as isize,
            ) != 0
        {
            *goforit = TRUE;
        }
        if *elemhits == 1
            && *layernode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize)
                    as isize,
            ) != 0
        {
            *goforit = TRUE;
        }
        if *goforit == 0 {
            *side += 1;
            continue;
        }
        // NOTE: corner with three hits exposes the free node.
        if *elemhits == 3 {
            *k = 0;
            while *k < 4 {
                if *layernode.wrapping_offset(
                    *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*k as isize)
                        as isize,
                ) == 0
                {
                    break;
                }
                *k += 1;
            }
            // NOTE: `if(0)` trace omitted (dead in C).
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
            *topnode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*k as isize);
            *topnode.wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*k as isize);
        } else if *elemhits == 2 {
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
            *topnode.wrapping_offset(0) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 3) % elemsides) as isize);
            *topnode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize);
        } else if *elemhits == 1 {
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *topnode.wrapping_offset(0) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 3) % elemsides) as isize);
            *topnode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
        }
        divide_edge_nodes(
            oldx, oldy, newx, newy, edgepairs, basenode, topnode, baseind, x0, y0,
            dx, dy, nlayer, slayer, qlayer, q, ratio, nonewnodes, noknots, i, k,
        );
        // NOTE: 0th layer element reuses the slot.
        if *elemhits == 1 {
            *newelementtypes.wrapping_offset(j as isize) = 303;
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(0) =
                *basenode.wrapping_offset(0);
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + 1;
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(2) =
                *baseind.wrapping_offset(0) + 1;
        } else if *elemhits == 3 && *elemdone != 0 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 404;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize))
                .wrapping_offset(*side as isize) = *basenode.wrapping_offset(0);
            *(*newtopo.wrapping_offset(*elemind as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize) =
                *basenode.wrapping_offset(1);
            *(*newtopo.wrapping_offset(*elemind as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize) =
                *baseind.wrapping_offset(1) + 1;
            *(*newtopo.wrapping_offset(*elemind as isize))
                .wrapping_offset(((*side + 3) % elemsides) as isize) =
                *baseind.wrapping_offset(0) + 1;
        } else {
            *(*newtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize) =
                *baseind.wrapping_offset(1) + 1;
            *(*newtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 3) % elemsides) as isize) =
                *baseind.wrapping_offset(0) + 1;
        }
        *i = 1;
        while *i < nlayer {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 404;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + *i;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + *i;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *baseind.wrapping_offset(1) + *i + 1;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(3) =
                *baseind.wrapping_offset(0) + *i + 1;
            *i += 1;
        }
        // NOTE: top layer element.
        if *elemhits == 3 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 303;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(0);
        } else if *elemhits == 2 || *elemhits == 1 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 404;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(1);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(3) =
                *topnode.wrapping_offset(0);
        }
        // NOTE: extra corner triangle for single hits.
        if *elemhits == 1 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 303;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *topnode.wrapping_offset(1);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize))
                    .wrapping_offset(((*side + 2) % elemsides) as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(0);
        }
        *elemdone = TRUE;
        *side += 1;
    }
    if *elemdone == 0 {
        printf(
            c"cannot handle quadrilaterals with %d hits\n".as_ptr(),
            *elemhits,
        );
    }
}
/// SAFETY: triangle divide arm (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn divide_tri(
    _data: *mut FemType,
    oldtopo: *mut *mut c_int,
    layernode: *mut c_int,
    sharednode: *mut c_int,
    newtopo: *mut *mut c_int,
    newmaterial: *mut c_int,
    newelementtypes: *mut c_int,
    oldx: *mut Real,
    oldy: *mut Real,
    newx: *mut Real,
    newy: *mut Real,
    edgepairs: *mut *mut c_int,
    basenode: *mut c_int,
    topnode: *mut c_int,
    baseind: *mut c_int,
    x0: *mut Real,
    y0: *mut Real,
    dx: *mut Real,
    dy: *mut Real,
    nlayer: c_int,
    slayer: Real,
    qlayer: Real,
    q: &mut Real,
    ratio: &mut Real,
    nonewnodes: c_int,
    noknots: &mut c_int,
    j: c_int,
    elemsides: c_int,
    elemhits: &mut c_int,
    elemdone: &mut c_int,
    elemind: &mut c_int,
    side: &mut c_int,
    goforit: &mut c_int,
    k: &mut c_int,
    i: &mut c_int,
) {
    *elemdone = FALSE;
    *side = 0;
    while *side < elemsides {
        *goforit = FALSE;
        if *elemhits == 2 {
            if *layernode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize)
                    as isize,
            ) != 0
                && *layernode.wrapping_offset(
                    *(*oldtopo.wrapping_offset(j as isize))
                        .wrapping_offset(((*side + 1) % elemsides) as isize)
                        as isize,
                ) != 0
            {
                *goforit = TRUE;
            }
        } else if *elemhits == 1 {
            if *layernode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize)
                    as isize,
            ) != 0
            {
                *goforit = TRUE;
            }
        } else if *elemhits == 3 {
            if *sharednode.wrapping_offset(
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize)
                    as isize,
            ) == 1
            {
                *goforit = TRUE;
            }
            printf(c"The boundary layer creation for certain corner triangles is omitted\n".as_ptr());
            *goforit = FALSE;
        }
        if *goforit == 0 {
            *side += 1;
            continue;
        }
        if *elemhits == 3 {
            printf(
                c"Special node %d in corner %d\n".as_ptr(),
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize),
                *side,
            );
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *topnode.wrapping_offset(0) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize);
            *topnode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
        } else if *elemhits == 2 {
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
            *topnode.wrapping_offset(0) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize);
            *topnode.wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(((*side + 2) % elemsides) as isize);
        } else if *elemhits == 1 {
            *basenode.wrapping_offset(0) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *basenode.wrapping_offset(1) =
                *(*oldtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize);
            *topnode.wrapping_offset(0) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % elemsides) as isize);
            *topnode.wrapping_offset(1) = *(*oldtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % elemsides) as isize);
        }
        divide_edge_nodes(
            oldx, oldy, newx, newy, edgepairs, basenode, topnode, baseind, x0, y0,
            dx, dy, nlayer, slayer, qlayer, q, ratio, nonewnodes, noknots, i, k,
        );
        // NOTE: 0th element reuses the slot.
        if *elemhits == 1 || *elemhits == 3 {
            *newelementtypes.wrapping_offset(j as isize) = 303;
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(0) =
                *basenode.wrapping_offset(0);
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + 1;
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(2) =
                *baseind.wrapping_offset(0) + 1;
        } else if *elemhits == 2 {
            *newelementtypes.wrapping_offset(j as isize) = 404;
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(*side as isize) =
                *basenode.wrapping_offset(0);
            *(*newtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 1) % 4) as isize) = *basenode.wrapping_offset(1);
            *(*newtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 2) % 4) as isize) = *baseind.wrapping_offset(1) + 1;
            *(*newtopo.wrapping_offset(j as isize))
                .wrapping_offset(((*side + 3) % 4) as isize) = *baseind.wrapping_offset(0) + 1;
        }
        *i = 1;
        while *i < nlayer {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 404;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + *i;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + *i;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *baseind.wrapping_offset(1) + *i + 1;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(3) =
                *baseind.wrapping_offset(0) + *i + 1;
            *i += 1;
        }
        // NOTE: top element.
        if *elemhits == 1 || *elemhits == 3 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 404;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(1);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(3) =
                *topnode.wrapping_offset(0);
        } else if *elemhits == 2 {
            *elemind += 1;
            *newelementtypes.wrapping_offset(*elemind as isize) = 303;
            *newmaterial.wrapping_offset(*elemind as isize) =
                *newmaterial.wrapping_offset(j as isize);
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(0) =
                *baseind.wrapping_offset(0) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(1) =
                *baseind.wrapping_offset(1) + nlayer;
            *(*newtopo.wrapping_offset(*elemind as isize)).wrapping_offset(2) =
                *topnode.wrapping_offset(1);
        }
        *elemdone = TRUE;
        *side += 1;
    }
    if *elemdone == 0 {
        printf(
            c"cannot handle triangles with %d hits\n".as_ptr(),
            *elemhits,
        );
    }
}
/// Boundary layers by splitting near-wall elements.
///
/// # Safety
/// `data`, `bound`, layer tables valid.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn CreateBoundaryLayerDivide(
    data: *mut FemType,
    bound: *mut BoundaryType,
    nolayers: c_int,
    layerbounds: *mut c_int,
    layernumber: *mut c_int,
    layerratios: *mut Real,
    layerthickness: *mut Real,
    _layerparents: *mut c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int = 0;
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dim: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxbc: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dolayer: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut parent: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nlayer: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut sideelemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemind: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut side: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut noelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut noknots: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnoknots: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldnoelements: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut oldmaxnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut nonewelements: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut maxcon: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemsides: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemdone: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut midpoints: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut order: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut bcnodes: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemhits: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut goforit: c_int = 0;
    let mut ind: [c_int; MAXNODESD2] = [0; MAXNODESD2];
    let mut baseind: [c_int; 2] = [0; 2];
    let mut topnode: [c_int; 2] = [0; 2];
    let mut basenode: [c_int; 2] = [0; 2];
    // NOTE: null-init (Ivector before guarded reads).
    let mut layernode: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Imatrix before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    let mut sharednode: *mut c_int = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dx: [Real; 2] = [0.0; 2];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut dy: [Real; 2] = [0.0; 2];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut x0: [Real; 2] = [0.0; 2];
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut y0: [Real; 2] = [0.0; 2];
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (Rvector before guarded reads).
    // NOTE: null-init (set from data before reads).
    // NOTE: null-init (set from data before reads).
    // NOTE: null-init (set from data before reads).
    let mut _oldz: *mut Real = core::ptr::null_mut();
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut slayer: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut qlayer: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ratio: Real = 0.0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut q: Real = 0.0;
    let _ = dim;
    if !divide_census(
        data, bound, nolayers, layerbounds, layernumber, layerratios, layerthickness,
        &mut maxelemtype, &mut layernode, &mut sharednode, &mut maxbc,
        &mut elemtype, &mut nlayer, &mut slayer, &mut qlayer, &mut order,
        &mut midpoints, &mut dolayer, &mut parent, ind.as_mut_ptr(), &mut sideelemtype,
        &mut bcnodes, &mut maxcon, &mut nonewelements, &mut oldnoelements,
        &mut oldnoknots, &mut oldmaxnodes, &mut dim, &mut i, &mut j, &mut k,
        &mut l, info,
    ) {
        return 0;
    }
    // NOTE: edge-pair table bounds the new nodes.
    let nonewnodes: c_int = 2 * nonewelements;
    let edgepairs: *mut *mut c_int = imatrix(1, nonewnodes, 1, 3);
    j = 1;
    while j <= nonewnodes {
        *(*edgepairs.wrapping_offset(j as isize)).wrapping_offset(1) = 0;
        *(*edgepairs.wrapping_offset(j as isize)).wrapping_offset(2) = 0;
        *(*edgepairs.wrapping_offset(j as isize)).wrapping_offset(3) = 0;
        j += 1;
    }
    // NOTE: old arrays are aliased, then replaced.
    oldnoelements = (*data).noelements;
    oldnoknots = (*data).noknots;
    let oldtopo: *mut *mut c_int = (*data).topology;
    let oldx: *mut Real = (*data).x;
    let oldy: *mut Real = (*data).y;
    _oldz = (*data).z;
    noknots = oldnoknots + nonewnodes;
    noelements = oldnoelements + nonewelements;
    if info != 0 {
        printf(
            c"Creating additional %d elements and %d nodes.\n".as_ptr(),
            nonewelements,
            nonewnodes,
        );
        printf(
            c"Boundary layer mesh has at maximum %d elements and %d nodes.\n".as_ptr(),
            noelements,
            noknots,
        );
    }
    // NOTE: room for the enlarged data set.
    let newtopo: *mut *mut c_int = imatrix(1, noelements, 0, (*data).maxnodes - 1);
    let newmaterial: *mut c_int = ivector(1, noelements);
    let newelementtypes: *mut c_int = ivector(1, noelements);
    let newx: *mut Real = dvector(1, noknots);
    let newy: *mut Real = dvector(1, noknots);
    let newz: *mut Real = dvector(1, noknots);
    // NOTE: old topology first.
    j = 1;
    while j <= (*data).noelements {
        *newmaterial.wrapping_offset(j as isize) =
            *(*data).material.wrapping_offset(j as isize);
        *newelementtypes.wrapping_offset(j as isize) =
            *(*data).elementtypes.wrapping_offset(j as isize);
        i = 0;
        while i < *(*data).elementtypes.wrapping_offset(j as isize) % 100 {
            *(*newtopo.wrapping_offset(j as isize)).wrapping_offset(i as isize) =
                *(*(*data).topology.wrapping_offset(j as isize))
                    .wrapping_offset(i as isize);
            i += 1;
        }
        j += 1;
    }
    // NOTE: old nodes first.
    i = 1;
    while i <= (*data).noknots {
        *newx.wrapping_offset(i as isize) = *(*data).x.wrapping_offset(i as isize);
        *newy.wrapping_offset(i as isize) = *(*data).y.wrapping_offset(i as isize);
        *newz.wrapping_offset(i as isize) = *(*data).z.wrapping_offset(i as isize);
        i += 1;
    }
    noelements = (*data).noelements;
    elemind = noelements;
    noknots = (*data).noknots;
    // NOTE: split hit elements by type.
    j = 1;
    while j <= (*data).noelements {
        elemhits = 0;
        elemtype = *(*data).elementtypes.wrapping_offset(j as isize);
        elemsides = elemtype % 100;
        i = 0;
        while i < elemsides {
            if *layernode.wrapping_offset(
                *(*(*data).topology.wrapping_offset(j as isize)).wrapping_offset(i as isize)
                    as isize,
            ) != 0
            {
                elemhits += 1;
            }
            i += 1;
        }
        if elemhits == 0 {
            j += 1;
            continue;
        }
        if elemtype == 404 {
            divide_quad(
                data, oldtopo, layernode, newtopo, newmaterial, newelementtypes,
                oldx, oldy, newx, newy, edgepairs, basenode.as_mut_ptr(),
                topnode.as_mut_ptr(), baseind.as_mut_ptr(), x0.as_mut_ptr(),
                y0.as_mut_ptr(), dx.as_mut_ptr(), dy.as_mut_ptr(), nlayer, slayer,
                qlayer, &mut q, &mut ratio, nonewnodes, &mut noknots, j, elemsides,
                &mut elemhits, &mut elemdone, &mut elemind, &mut side, &mut goforit,
                &mut k, &mut i,
            );
        } else if elemtype == 303 {
            divide_tri(
                data, oldtopo, layernode, sharednode, newtopo, newmaterial,
                newelementtypes, oldx, oldy, newx, newy, edgepairs,
                basenode.as_mut_ptr(), topnode.as_mut_ptr(), baseind.as_mut_ptr(),
                x0.as_mut_ptr(), y0.as_mut_ptr(), dx.as_mut_ptr(), dy.as_mut_ptr(),
                nlayer, slayer, qlayer, &mut q, &mut ratio, nonewnodes, &mut noknots,
                j, elemsides, &mut elemhits, &mut elemdone, &mut elemind, &mut side,
                &mut goforit, &mut k, &mut i,
            );
        } else {
            printf(c"Not implemented for element %d\n".as_ptr(), elemtype);
        }
        j += 1;
    }
    noelements = elemind;
    (*data).x = newx;
    (*data).y = newy;
    (*data).topology = newtopo;
    (*data).material = newmaterial;
    (*data).elementtypes = newelementtypes;
    (*data).noknots = noknots;
    (*data).noelements = elemind;
    printf(
        c"The created boundary layer mesh has at %d elements and %d nodes.\n".as_ptr(),
        noelements,
        noknots,
    );
    0
}
/// SAFETY: affine mesh transform (scale/rotate/translate/center).
///
/// # Safety
/// `data`, `eg` valid.
#[no_mangle]
pub unsafe extern "C" fn RotateTranslateScale(
    data: *mut FemType,
    eg: *mut ElmergridType,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut x: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut y: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut z: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut yz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut yx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cx: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cy: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut cz: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmax: Real;
    if (*eg).scale != 0 {
        if info != 0 {
            printf(
                c"Scaling mesh with vector [%.3lg %.3lg %.3lg]\n".as_ptr(),
                (*eg).cscale[0],
                (*eg).cscale[1],
                (*eg).cscale[2],
            );
        }
        i = 1;
        while i <= (*data).noknots {
            *(*data).x.wrapping_offset(i as isize) *= (*eg).cscale[0];
            *(*data).y.wrapping_offset(i as isize) *= (*eg).cscale[1];
            *(*data).z.wrapping_offset(i as isize) *= (*eg).cscale[2];
            i += 1;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
    }
    if (*eg).rotate != 0 {
        if info != 0 {
            printf(
                c"Rotating mesh with degrees [%.3lg %.3lg %.3lg]\n".as_ptr(),
                (*eg).crotate[0],
                (*eg).crotate[1],
                (*eg).crotate[2],
            );
        }
        cx = FM_PI * (*eg).crotate[0] / 180.0;
        cy = FM_PI * (*eg).crotate[1] / 180.0;
        cz = FM_PI * (*eg).crotate[2] / 180.0;
        i = 1;
        while i <= (*data).noknots {
            x = *(*data).x.wrapping_offset(i as isize);
            y = *(*data).y.wrapping_offset(i as isize);
            z = *(*data).z.wrapping_offset(i as isize);
            xz = x * cos(cz) + y * sin(cz);
            yz = -x * sin(cz) + y * cos(cz);
            if fabs(cx) > 1.0e-8 || fabs(cy) > 1.0e-8 {
                yx = yz * cos(cx) + z * sin(cx);
                zx = -yz * sin(cx) + z * cos(cx);
                zy = zx * cos(cy) + xz * sin(cy);
                xy = -zx * sin(cy) + xz * cos(cy);
                *(*data).x.wrapping_offset(i as isize) = xy;
                *(*data).y.wrapping_offset(i as isize) = yx;
                *(*data).z.wrapping_offset(i as isize) = zy;
            } else {
                *(*data).x.wrapping_offset(i as isize) = xz;
                *(*data).y.wrapping_offset(i as isize) = yz;
            }
            i += 1;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
    }
    if (*eg).translate != 0 {
        if info != 0 {
            printf(
                c"Translating the mesh with vector [%.3lg %.3lg %.3lg]\n".as_ptr(),
                (*eg).ctranslate[0],
                (*eg).ctranslate[1],
                (*eg).ctranslate[2],
            );
        }
        i = 1;
        while i <= (*data).noknots {
            *(*data).x.wrapping_offset(i as isize) += (*eg).ctranslate[0];
            *(*data).y.wrapping_offset(i as isize) += (*eg).ctranslate[1];
            *(*data).z.wrapping_offset(i as isize) += (*eg).ctranslate[2];
            i += 1;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
    }
    if (*eg).center != 0 {
        xmin = *(*data).x.wrapping_offset(1);
        xmax = *(*data).x.wrapping_offset(1);
        ymin = *(*data).y.wrapping_offset(1);
        ymax = *(*data).y.wrapping_offset(1);
        zmin = *(*data).z.wrapping_offset(1);
        zmax = *(*data).z.wrapping_offset(1);
        i = 1;
        while i <= (*data).noknots {
            // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
            if xmax < *(*data).x.wrapping_offset(i as isize) {
                xmax = *(*data).x.wrapping_offset(i as isize);
            }
            if *(*data).x.wrapping_offset(i as isize) < xmin {
                xmin = *(*data).x.wrapping_offset(i as isize);
            }
            if ymax < *(*data).y.wrapping_offset(i as isize) {
                ymax = *(*data).y.wrapping_offset(i as isize);
            }
            if *(*data).y.wrapping_offset(i as isize) < ymin {
                ymin = *(*data).y.wrapping_offset(i as isize);
            }
            if zmax < *(*data).z.wrapping_offset(i as isize) {
                zmax = *(*data).z.wrapping_offset(i as isize);
            }
            if *(*data).z.wrapping_offset(i as isize) < zmin {
                zmin = *(*data).z.wrapping_offset(i as isize);
            }
            i += 1;
        }
        cx = 0.5 * (xmin + xmax);
        cy = 0.5 * (ymin + ymax);
        cz = 0.5 * (zmin + zmax);
        if info != 0 {
            printf(
                c"Setting new center to %.3e %.3e %.3e\n".as_ptr(),
                cx,
                cy,
                cz,
            );
        }
        i = 1;
        while i <= (*data).noknots {
            *(*data).x.wrapping_offset(i as isize) -= cx;
            *(*data).y.wrapping_offset(i as isize) -= cy;
            *(*data).z.wrapping_offset(i as isize) -= cz;
            i += 1;
        }
    }
    0
}
/// SAFETY: single connection insert with row growth (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn nodal_link(
    data: *mut FemType,
    ind: c_int,
    ind2: c_int,
    noknots: c_int,
    maxcon: &mut c_int,
    totcon: &mut c_int,
    hit: &mut c_int,
    l: &mut c_int,
    m: &mut c_int,
) {
    *hit = FALSE;
    *l = 0;
    while *l < *maxcon {
        if *(*data).nodalgraph[*l as usize].wrapping_offset(ind as isize) == ind2 {
            *hit = TRUE;
        }
        if *(*data).nodalgraph[*l as usize].wrapping_offset(ind as isize) == 0 {
            break;
        }
        *l += 1;
    }
    if *hit == 0 {
        if *l >= *maxcon {
            (*data).nodalgraph[*maxcon as usize] = ivector(1, noknots);
            *m = 1;
            while *m <= noknots {
                *(*data).nodalgraph[*maxcon as usize].wrapping_offset(*m as isize) = 0;
                *m += 1;
            }
            *maxcon += 1;
        }
        *(*data).nodalgraph[*l as usize].wrapping_offset(ind as isize) = ind2;
        *totcon += 1;
    }
}

/// Nodal adjacency graph (edge or full-element connectivity).
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateNodalGraph(
    data: *mut FemType,
    full: c_int,
    info: c_int,
) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut m: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totcon: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut hit: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut ind2: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxcon: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut percon: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut edge: c_int;
    printf(c"Creating a nodal graph of the finite element mesh\n".as_ptr());
    if (*data).nodalexists != 0 {
        printf(c"The nodal graph already exists!\n".as_ptr());
        smallerror(c"Nodal graph not done".as_ptr());
        return 1;
    }
    maxcon = 0;
    totcon = 0;
    percon = 0;
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    i = 1;
    while i <= noelements {
        // NOTE: edge-only connectivity unless full.
        if full == 0 {
            let mut inds: [c_int; 2] = [0; 2];
            edge = 0;
            loop {
                if GetElementGraph(i, edge, data, inds.as_mut_ptr()) == 0 {
                    break;
                }
                ind = inds[0];
                ind2 = inds[1];
                nodal_link(data, ind, ind2, noknots, &mut maxcon, &mut totcon, &mut hit, &mut l, &mut m);
                // NOTE: symmetric connection too.
                nodal_link(data, ind2, ind, noknots, &mut maxcon, &mut totcon, &mut hit, &mut l, &mut m);
                edge += 1;
            }
        } else {
            // NOTE: all within-element connections.
            nonodes = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
            j = 0;
            while j < nonodes {
                ind = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
                k = 0;
                while k < nonodes {
                    ind2 = *(*(*data).topology.wrapping_offset(i as isize))
                        .wrapping_offset(k as isize);
                    if ind == ind2 {
                        k += 1;
                        continue;
                    }
                    nodal_link(data, ind, ind2, noknots, &mut maxcon, &mut totcon, &mut hit, &mut l, &mut m);
                    k += 1;
                }
                j += 1;
            }
        }
        i += 1;
    }
    // NOTE: periodic knots link to their images.
    if (*data).periodicexist != 0 {
        ind = 1;
        while ind <= noknots {
            ind2 = *(*data).periodic.wrapping_offset(ind as isize);
            if ind == ind2 {
                ind += 1;
                continue;
            }
            nodal_link(data, ind, ind2, noknots, &mut maxcon, &mut totcon, &mut hit, &mut l, &mut m);
            // NOTE: C counts only fresh periodic links.
            if hit == 0 {
                percon += 1;
            }
            ind += 1;
        }
    }
    (*data).nodalmaxconnections = maxcon;
    (*data).nodalexists = TRUE;
    if info != 0 {
        printf(
            c"There are at maximum %d connections in nodal graph.\n".as_ptr(),
            maxcon,
        );
        printf(
            c"There are at all in all %d connections in nodal graph.\n".as_ptr(),
            totcon,
        );
        if percon != 0 {
            printf(
                c"There are %d periodic connections in nodal graph.\n".as_ptr(),
                percon,
            );
        }
    }
    0
}

/// Free the nodal adjacency graph.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyNodalGraph(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    if (*data).nodalexists == 0 {
        printf(c"You tried to destroy a non-existing nodal graph\n".as_ptr());
        return 1;
    }
    let maxcon: c_int = (*data).nodalmaxconnections;
    let noknots: c_int = (*data).noknots;
    i = 0;
    while i < maxcon {
        free_ivector((*data).nodalgraph[i as usize], 1, noknots);
        i += 1;
    }
    (*data).nodalmaxconnections = 0;
    (*data).nodalexists = FALSE;
    if info != 0 {
        printf(c"The nodal graph was destroyed\n".as_ptr());
    }
    0
}
/// SAFETY: CSR inverse topology (nodes -> elements).
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateInverseTopology(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    let mut j: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut k: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let _l: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let _m: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut _elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut nonodes: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ind: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minneeded: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxneeded: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut step: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totcon: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    let mut rows: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut cols: *mut c_int = core::ptr::null_mut();
    // NOTE: CSR struct borrowed for the fill.
    let invtopo: *mut CRSType = &mut (*data).invtopo;
    if (*invtopo).created != 0 {
        // NOTE: `if(0)` trace omitted (dead in C).
        return 0;
    }
    printf(c"Creating an inverse topology of the finite element mesh\n".as_ptr());
    let noelements: c_int = (*data).noelements;
    let noknots: c_int = (*data).noknots;
    let neededby: *mut c_int = ivector(1, noknots);
    totcon = 0;
    step = 1;
    while step <= 2 {
        i = 1;
        while i <= noknots {
            *neededby.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        i = 1;
        while i <= noelements {
            nonodes = *(*data).elementtypes.wrapping_offset(i as isize) % 100;
            j = 0;
            while j < nonodes {
                ind = *(*(*data).topology.wrapping_offset(i as isize))
                    .wrapping_offset(j as isize);
                if step == 1 {
                    (*neededby.wrapping_offset(ind as isize)) += 1;
                    totcon += 1;
                } else {
                    k = *rows.wrapping_offset((ind - 1) as isize)
                        + *neededby.wrapping_offset(ind as isize);
                    *cols.wrapping_offset(k as isize) = i - 1;
                    (*neededby.wrapping_offset(ind as isize)) += 1;
                }
                j += 1;
            }
            i += 1;
        }
        if step == 1 {
            rows = ivector(0, noknots);
            *rows.wrapping_offset(0) = 0;
            i = 1;
            while i <= noknots {
                *rows.wrapping_offset(i as isize) = *rows.wrapping_offset((i - 1) as isize)
                    + *neededby.wrapping_offset(i as isize);
                i += 1;
            }
            cols = ivector(0, totcon - 1);
            i = 0;
            while i < totcon {
                *cols.wrapping_offset(i as isize) = 0;
                i += 1;
            }
            (*invtopo).cols = cols;
            (*invtopo).rows = rows;
            (*invtopo).colsize = totcon;
            (*invtopo).rowsize = noknots;
            (*invtopo).created = TRUE;
        }
        step += 1;
    }
    minneeded = *neededby.wrapping_offset(1);
    maxneeded = *neededby.wrapping_offset(1);
    i = 1;
    while i <= noknots {
        // NOTE: C MIN/MAX macros (egdef.h:96-97); arms pure.
        if *neededby.wrapping_offset(i as isize) < minneeded {
            minneeded = *neededby.wrapping_offset(i as isize);
        }
        if maxneeded < *neededby.wrapping_offset(i as isize) {
            maxneeded = *neededby.wrapping_offset(i as isize);
        }
        i += 1;
    }
    free_ivector(neededby, 1, noknots);
    if info != 0 {
        printf(
            c"There are from %d to %d connections in the inverse topology.\n".as_ptr(),
            minneeded,
            maxneeded,
        );
        printf(
            c"Each node is in average in %.3f elements\n".as_ptr(),
            1.0 * totcon as Real / noknots as Real,
        );
    }
    0
}
/// SAFETY: dual-graph CSR build, counted then filled (split for sizing).
#[allow(clippy::too_many_arguments)]
unsafe fn dual_build(
    data: *mut FemType,
    elemconnect: *mut c_int,
    neededby: *mut c_int,
    dualrow: *mut c_int,
    dualcol: *mut c_int,
    invrow: *mut c_int,
    invcol: *mut c_int,
    dualsize: c_int,
    noelements: c_int,
    unconnected: c_int,
    allocated: c_int,
    totcon: &mut c_int,
    dcon: &mut c_int,
    dualmaxcon: &mut c_int,
    dualmaxelem: &mut c_int,
    orphanelements: &mut c_int,
    elemtype: &mut c_int,
    nonodes: &mut c_int,
    ind: &mut c_int,
    ci: &mut c_int,
    ci2: &mut c_int,
    i2: &mut c_int,
    step: &mut c_int,
    i: &mut c_int,
    j: &mut c_int,
    k: &mut c_int,
    showgraph: c_int,
) {
    *totcon = 0;
    *i = 1;
    while *i <= noelements {
        // NOTE: `if(showgraph)` trace omitted (always FALSE).
        *dcon = 0;
        *elemtype = *(*data).elementtypes.wrapping_offset(*i as isize);
        *nonodes = *(*data).elementtypes.wrapping_offset(*i as isize) % 100;
        if unconnected != 0 {
            *ci = *elemconnect.wrapping_offset(*i as isize);
            if *ci < 0 {
                *i += 1;
                continue;
            }
        } else {
            *ci = *i;
        }
        if allocated != 0 {
            *dualrow.wrapping_offset((*ci - 1) as isize) = *totcon;
        }
        // NOTE: `if(0)` trace omitted (dead in C).
        *step = 1;
        while *step <= 2 {
            *j = 0;
            while *j < *nonodes {
                *ind = *(*(*data).topology.wrapping_offset(*i as isize))
                    .wrapping_offset(*j as isize);
                // NOTE: `if(0)` trace omitted (dead in C).
                *k = *invrow.wrapping_offset((*ind - 1) as isize);
                while *k < *invrow.wrapping_offset(*ind as isize) {
                    *i2 = *invcol.wrapping_offset(*k as isize) + 1;
                    if *i2 == *i {
                        *k += 1;
                        continue;
                    }
                    if unconnected != 0 {
                        *ci2 = *elemconnect.wrapping_offset(*i2 as isize);
                        if *ci2 < 0 {
                            *k += 1;
                            continue;
                        }
                    } else {
                        *ci2 = *i2;
                    }
                    // NOTE: first pass marks, second clears for reuse.
                    if *step == 1 {
                        if *neededby.wrapping_offset(*ci2 as isize) != 0 {
                            *k += 1;
                            continue;
                        }
                        *neededby.wrapping_offset(*ci2 as isize) = TRUE;
                        // NOTE: `if(0)` trace omitted (dead in C).
                        if allocated != 0 {
                            *dualcol.wrapping_offset(*totcon as isize) = *ci2 - 1;
                        }
                        *dcon += 1;
                        *totcon += 1;
                        if *dcon > *dualmaxcon {
                            *dualmaxcon = *dcon;
                            *dualmaxelem = *i;
                        }
                    } else {
                        *neededby.wrapping_offset(*ci2 as isize) = FALSE;
                    }
                    *k += 1;
                }
                *j += 1;
            }
            *step += 1;
        }
        if *dcon == 0 && allocated != 0 {
            *orphanelements += 1;
        }
        *i += 1;
    }
    if allocated != 0 {
        *dualrow.wrapping_offset(dualsize as isize) = *totcon;
    }
    let _ = (showgraph, dualmaxelem);
}
/// SAFETY: element dual graph in CSR form.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn CreateDualGraph(
    data: *mut FemType,
    mut unconnected: c_int,
    info: c_int,
) -> c_int {
    // NOTE: assigned before every read (C uninit decl shape).
    let mut totcon: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut dcon: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut elemtype: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut nonodes: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut i: c_int;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut j: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let mut k: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set below before reads).
    let _l: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut i2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let _m: c_int;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut ind: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let _hit: c_int;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut ci: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut ci2: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dualmaxcon: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let _invmaxcon: c_int;
    // NOTE: always FALSE; the `if(showgraph)` traces are dead in C.
    let showgraph: c_int = FALSE;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut freeelements: c_int;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut step: c_int = 0;
    // NOTE: 0-init for the &mut handoff (set in dual_build before reads).
    let mut orphanelements: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let _stat: c_int;
    // NOTE: null-init (set from data before reads).
    let mut elemconnect: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    // NOTE: null-init (Ivector before guarded reads).
    let mut dualrow: *mut c_int = core::ptr::null_mut();
    // NOTE: null-init (Ivector before guarded reads).
    let mut dualcol: *mut c_int = core::ptr::null_mut();
    // NOTE: `=0` decl init kept (read on the fill pass).
    let mut dualsize: c_int = 0;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut dualmaxelem: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut allocated: c_int;
    // NOTE: null-init (set from data before reads).
    // NOTE: null-init (set from data before reads).
    // NOTE: CSR struct borrowed for the fill.
    if info != 0 {
        printf(c"Creating a dual graph of the finite element mesh\n".as_ptr());
    }
    let dualgraph: *mut CRSType = &mut (*data).dualgraph;
    if (*dualgraph).created != 0 {
        printf(c"The dual graph already exists!\n".as_ptr());
        return 1;
    }
    CreateInverseTopology(data, info);
    let noelements: c_int = (*data).noelements;
    let _noknots: c_int = (*data).noknots;
    freeelements = noelements;
    orphanelements = 0;
    // NOTE: unconnected-only graphs skip connected elements.
    if unconnected != 0 {
        if info != 0 {
            printf(c"Removing connected nodes from the dual graph\n".as_ptr());
        }
        if (*data).nodeconnectexist != 0 {
            if info != 0 {
                printf(c"Creating connected elements list from the connected nodes\n".as_ptr());
            }
            SetConnectedElements(data, info);
        }
        if (*data).elemconnectexist != 0 {
            elemconnect = (*data).elemconnect;
            freeelements -= (*data).elemconnectexist;
        } else {
            unconnected = FALSE;
        }
        if info != 0 {
            printf(c"List of unconnected elements created\n".as_ptr());
        }
    }
    // NOTE: showgraph is FALSE; traces dead.
    (*data).dualexists = TRUE;
    dualmaxcon = 0;
    dualmaxelem = 0;
    let invrow: *mut c_int = (*data).invtopo.rows;
    let invcol: *mut c_int = (*data).invtopo.cols;
    // NOTE: seen-markers for the counted pass.
    let neededby: *mut c_int = ivector(1, freeelements);
    i = 1;
    while i <= freeelements {
        *neededby.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    allocated = FALSE;
    // NOTE: C `omstart:` label + `goto omstart`; count then fill.
    loop {
        dual_build(
            data, elemconnect, neededby, dualrow, dualcol, invrow, invcol, dualsize,
            noelements, unconnected, allocated, &mut totcon, &mut dcon, &mut dualmaxcon,
            &mut dualmaxelem, &mut orphanelements, &mut elemtype, &mut nonodes,
            &mut ind, &mut ci, &mut ci2, &mut i2, &mut step, &mut i, &mut j,
            &mut k, showgraph,
        );
        if allocated != 0 {
            break;
        }
        dualsize = freeelements;
        if info != 0 {
            printf(
                c"Allocating for the dual graph for %d with %d connections\n".as_ptr(),
                dualsize,
                totcon,
            );
        }
        dualrow = ivector(0, dualsize);
        i = 1;
        while i <= dualsize {
            *dualrow.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        dualcol = ivector(0, totcon - 1);
        i = 0;
        while i < totcon {
            *dualcol.wrapping_offset(i as isize) = 0;
            i += 1;
        }
        (*dualgraph).cols = dualcol;
        (*dualgraph).rows = dualrow;
        (*dualgraph).rowsize = dualsize;
        (*dualgraph).colsize = totcon;
        (*dualgraph).created = TRUE;
        allocated = TRUE;
    }
    if orphanelements != 0 && info != 0 {
        printf(
            c"There are %d elements in the dual mesh that are not connected!\n".as_ptr(),
            orphanelements,
        );
        if unconnected != 0 {
            printf(c"The orphan elements are likely caused by the hybrid partitioning\n".as_ptr());
        }
    }
    // NOTE: `#if 0` range dumps omitted (dead in C).
    if info != 0 {
        printf(
            c"There are at maximum %d connections in dual graph (in element %d).\n".as_ptr(),
            dualmaxcon,
            dualmaxelem,
        );
        printf(
            c"There are at all in all %d connections in dual graph.\n".as_ptr(),
            totcon,
        );
        printf(
            c"Average connection per active element in dual graph is %.3f\n".as_ptr(),
            1.0 * totcon as Real / freeelements as Real,
        );
    }
    free_ivector(neededby, 1, freeelements);
    // NOTE: `if(0)` inverse-topology teardown omitted (dead in C).
    0
}
/// SAFETY: CSR teardown (note the inverted C guard, kept verbatim).
///
/// # Safety
/// `sp` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyCRSMatrix(sp: *mut CRSType) -> c_int {
    // NOTE: C returns early when created is set (inverted guard, verbatim).
    if (*sp).created != 0 {
        // NOTE: `if(0)` trace omitted (dead in C).
        return 1;
    }
    free_ivector((*sp).rows, 0, (*sp).rowsize);
    free_ivector((*sp).cols, 0, (*sp).colsize - 1);
    (*sp).rowsize = 0;
    (*sp).colsize = 0;
    (*sp).created = FALSE;
    0
}

/// Free the inverse topology via the CSR teardown.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyInverseTopology(data: *mut FemType, info: c_int) -> c_int {
    let _ = info;
    let stat: c_int = DestroyCRSMatrix(&mut (*data).invtopo);
    stat
}

/// Free the dual graph via the CSR teardown.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn DestroyDualGraph(data: *mut FemType, info: c_int) -> c_int {
    let _ = info;
    let stat: c_int = DestroyCRSMatrix(&mut (*data).dualgraph);
    stat
}

/// Element-type census of the mesh.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn MeshTypeStatistics(data: *mut FemType, info: c_int) -> c_int {
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut elemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut maxelemtype: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut minelemtype: c_int;
    // NOTE: null-init (Ivector before guarded reads).
    maxelemtype = *(*data).elementtypes.wrapping_offset(1);
    minelemtype = *(*data).elementtypes.wrapping_offset(1);
    i = 1;
    while i <= (*data).noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        // NOTE: C MAX macro (egdef.h:97); arms pure.
        if maxelemtype < elemtype {
            maxelemtype = elemtype;
        }
        // NOTE: C MIN macro (egdef.h:96); arms pure.
        if elemtype < minelemtype {
            minelemtype = elemtype;
        }
        i += 1;
    }
    let elemtypes: *mut c_int = ivector(minelemtype, maxelemtype);
    i = minelemtype;
    while i <= maxelemtype {
        *elemtypes.wrapping_offset(i as isize) = 0;
        i += 1;
    }
    i = 1;
    while i <= (*data).noelements {
        elemtype = *(*data).elementtypes.wrapping_offset(i as isize);
        (*elemtypes.wrapping_offset(elemtype as isize)) += 1;
        i += 1;
    }
    if info != 0 {
        printf(c"Number of different elementtypes\n".as_ptr());
        i = minelemtype;
        while i <= maxelemtype {
            if *elemtypes.wrapping_offset(i as isize) != 0 {
                printf(
                    c"\t%d\t%d\n".as_ptr(),
                    i,
                    *elemtypes.wrapping_offset(i as isize),
                );
            }
            i += 1;
        }
    }
    free_ivector(elemtypes, minelemtype, maxelemtype);
    0
}

/// Knot bounding box with an SI-unit sanity notice.
///
/// # Safety
/// `data` valid.
#[no_mangle]
pub unsafe extern "C" fn BoundingBox(
    data: *mut FemType,
    nomesh: c_int,
    nomeshes: c_int,
    info: c_int,
) -> c_int {
    let _ = info;
    let mut i: c_int;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut xmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut ymax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmin: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut zmax: Real;
    // NOTE: assigned before every read (C uninit decl shape).
    let mut sidemax: Real;
    xmin = *(*data).x.wrapping_offset(1);
    xmax = *(*data).x.wrapping_offset(1);
    ymin = *(*data).y.wrapping_offset(1);
    ymax = *(*data).y.wrapping_offset(1);
    zmin = *(*data).z.wrapping_offset(1);
    zmax = *(*data).z.wrapping_offset(1);
    i = 1;
    while i <= (*data).noknots {
        // NOTE: C MAX/MIN macros (egdef.h:96-97); arms pure.
        if xmax < *(*data).x.wrapping_offset(i as isize) {
            xmax = *(*data).x.wrapping_offset(i as isize);
        }
        if *(*data).x.wrapping_offset(i as isize) < xmin {
            xmin = *(*data).x.wrapping_offset(i as isize);
        }
        if ymax < *(*data).y.wrapping_offset(i as isize) {
            ymax = *(*data).y.wrapping_offset(i as isize);
        }
        if *(*data).y.wrapping_offset(i as isize) < ymin {
            ymin = *(*data).y.wrapping_offset(i as isize);
        }
        if zmax < *(*data).z.wrapping_offset(i as isize) {
            zmax = *(*data).z.wrapping_offset(i as isize);
        }
        if *(*data).z.wrapping_offset(i as isize) < zmin {
            zmin = *(*data).z.wrapping_offset(i as isize);
        }
        i += 1;
    }
    // NOTE: C MAX macro over differences (egdef.h:97).
    sidemax = xmax - xmin;
    if sidemax < ymax - ymin {
        sidemax = ymax - ymin;
    }
    if sidemax < zmax - zmin {
        sidemax = zmax - zmin;
    }
    if nomeshes > 1 {
        printf(
            c"Bounding box of all nodes in mesh[%d] of [%d] meshes:\n".as_ptr(),
            nomesh,
            nomeshes,
        );
    } else {
        printf(c"Bounding box of all nodes in mesh:\n".as_ptr());
    }
    printf(c"X:[%lg,%lg] ".as_ptr(), xmin, xmax);
    printf(c"Y:[%lg,%lg] ".as_ptr(), ymin, ymax);
    printf(c"Z:[%lg,%lg]\n".as_ptr(), zmin, zmax);
    if sidemax > 49.9 {
        printf(c"\nNotice: the longest bounding box side length of [%lg] is greater than 50.\n".as_ptr(), sidemax);
        printf(c"ElmerGUI includes a library of material properties, defined in SI units.  If using ElmerGUI, \n".as_ptr());
        printf(c"then the geometry is expected to have meters as length.  Geometry that exceeds 50 meters \n".as_ptr());
        printf(c"in length or width or height may not be intended.  Many Geometry generators assume \n".as_ptr());
        printf(c"millimeters as the basic unit of length.  Scaling the geometry from millimeters to meters \n".as_ptr());
        printf(c"may be the desired action.  For more help, search the Elmer users forum for posts \n".as_ptr());
        printf(c"about SI units, or for posts about Coordinate Scaling.\n".as_ptr());
        printf(c"Scaling can be accomplished in at least three ways, as follows:\n".as_ptr());
        printf(c" 1. Define the original geometry in meters, not millimeters.\n".as_ptr());
        printf(c" 2. Call ElmerGrid with -scale 0.001 0.001 0.001 as an option.\n".as_ptr());
        printf(c" 3. Add Coordinate Scaling = 0.001 to the simulation section of the sif file.\n".as_ptr());
        printf(c"If using Elmer to analyze large geometry, such as a glacier, then ignore this notice.\n\n".as_ptr());
    }
    0
}
