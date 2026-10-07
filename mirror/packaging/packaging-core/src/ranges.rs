//! Pure-Rust core: version-range engine for specifiers and ranges.
//!
//! Stage-1 mirror of `packaging/_ranges.py` (boundary/bound algebra,
//! intersection, filtering, per-operator range builders) plus the
//! specifier-expressible encoding helpers from `packaging/ranges.py`.
//! Operates on [`ParsedVersion`](super::version::ParsedVersion); the binding
//! wraps these in the Python-visible classes.
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

use std::cmp::Ordering;
use std::collections::HashSet;

use super::version::{
    self, LocalSeg, NumString, ParsedVersion,
};
use super::version::Parser;

/// Where a boundary marker sits in the version ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    AfterLocals,
    AfterPosts,
}

/// A point on the version line between two real PEP 440 versions:
/// `V < V+local < AFTER_LOCALS(V) < V.post0 < AFTER_POSTS(V)`.
#[derive(Debug, Clone, Eq)]
pub struct BoundaryVersion {
    pub version: ParsedVersion,
    pub kind: BoundaryKind,
}

impl PartialEq for BoundaryVersion {
    fn eq(&self, other: &Self) -> bool {
        // Mirror of `BoundaryVersion.__eq__`: order-key equality, so
        // `AFTER_POSTS(1.0) == AFTER_POSTS(1.0.post1)`.
        cmp_boundaries(self, other) == Ordering::Equal
    }
}

/// A bound endpoint: a real version, a boundary, or an infinity.
#[derive(Debug, Clone, Eq)]
pub enum BoundPoint {
    NegInf,
    Ver(ParsedVersion),
    Bnd(BoundaryVersion),
}

impl PartialEq for BoundPoint {
    fn eq(&self, other: &Self) -> bool {
        eq_point(self, other)
    }
}

impl BoundPoint {
    pub fn is_none(&self) -> bool {
        matches!(self, BoundPoint::NegInf)
    }
}

/// Lower bound of a version range (`None` version = unbounded below).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerBound {
    pub point: BoundPoint,
    pub inclusive: bool,
}

/// Upper bound of a version range (`None` version = unbounded above).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpperBound {
    pub point: BoundPoint,
    pub inclusive: bool,
}

/// A single contiguous interval as a (lower, upper) bound pair.
pub type Interval = (LowerBound, UpperBound);

pub fn neg_inf() -> LowerBound {
    LowerBound { point: BoundPoint::NegInf, inclusive: false }
}

pub fn pos_inf() -> UpperBound {
    UpperBound { point: BoundPoint::NegInf, inclusive: false }
}

pub fn full_range() -> Vec<Interval> {
    vec![(neg_inf(), pos_inf())]
}

/// The smallest possible PEP 440 version.
pub fn min_version() -> ParsedVersion {
    version::parse("0.dev0", None).expect("MIN_VERSION parses")
}

/// The smallest non-pre-release version.
pub fn min_release() -> ParsedVersion {
    version::parse("0", None).expect("MIN_RELEASE parses")
}

// ---------------------------------------------------------------------------
// Boundary ordering.
// ---------------------------------------------------------------------------

/// Post number that sorts above any real post number or local label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostNum {
    N(NumString),
    Inf,
}

impl PostNum {
    fn cmp(&self, other: &PostNum) -> Ordering {
        match (self, other) {
            (PostNum::N(a), PostNum::N(b)) => version::cmp_num(a, b),
            (PostNum::N(_), PostNum::Inf) => Ordering::Less,
            (PostNum::Inf, PostNum::N(_)) => Ordering::Greater,
            (PostNum::Inf, PostNum::Inf) => Ordering::Equal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalTail {
    Missing,
    Segs(Vec<LocalSeg>),
    Inf,
}

/// Suffix rank tuple shared by the version/boundary comparison keys:
/// `(pre_rank, pre_n, post_rank, post_n, dev_rank, dev_n)`.
type SuffixKey = (i64, NumString, i64, NumString, i64, NumString);
/// Comparison key of a real version: `(epoch, trimmed release, suffix, tail)`.
type VersionKey<'a> = (&'a NumString, Vec<NumString>, SuffixKey, LocalTail);
/// Owned order key for boundary-vs-boundary comparison.
type BoundaryKey = (NumString, Vec<NumString>, (i64, NumString, i64, PostNum, i64, NumString));

/// Comparison key of a real version: `(epoch, trimmed release, suffix, tail)`.
fn version_key_parts(v: &ParsedVersion) -> VersionKey<'_> {
    let trimmed = version::trim_release(&v.release);
    let (pre_rank, pre_n) = version::pre_rank(&v.pre, &v.post, &v.dev);
    let (post_rank, post_n) = match &v.post {
        None => (0, "0".to_string()),
        Some(n) => (1, n.clone()),
    };
    let (dev_rank, dev_n) = match &v.dev {
        None => (1, "0".to_string()),
        Some(n) => (0, n.clone()),
    };
    let tail = match &v.local {
        None => LocalTail::Missing,
        Some(segs) => LocalTail::Segs(segs.clone()),
    };
    (
        &v.epoch,
        trimmed,
        (pre_rank, pre_n, post_rank, post_n, dev_rank, dev_n),
        tail,
    )
}

fn cmp_release_vec(a: &[NumString], b: &[NumString]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        match version::cmp_num(x, y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

/// Total order over versions and boundaries, mirroring `_order_key` tuple
/// comparison (a boundary key extends its base version's key with an
/// infinite local tail, and `AFTER_POSTS` additionally lifts the post
/// number to infinity).
pub fn cmp_point(a: &BoundPoint, b: &BoundPoint) -> Ordering {
    match (a, b) {
        (BoundPoint::NegInf, BoundPoint::NegInf) => Ordering::Equal,
        // NegInf only appears as a LowerBound version (-inf); it sorts
        // below everything. (UpperBound +inf is also NegInf here.)
        (BoundPoint::NegInf, _) => Ordering::Less,
        (_, BoundPoint::NegInf) => Ordering::Greater,
        (BoundPoint::Ver(x), BoundPoint::Ver(y)) => version::cmp(x, y),
        (BoundPoint::Ver(x), BoundPoint::Bnd(by)) => cmp_mixed(x, by, false),
        (BoundPoint::Bnd(bx), BoundPoint::Ver(y)) => cmp_mixed(y, bx, true),
        (BoundPoint::Bnd(bx), BoundPoint::Bnd(by)) => cmp_boundaries(bx, by),
    }
}

fn suffix_of(v: &ParsedVersion, after_posts: bool) -> (i64, NumString, i64, PostNum, i64, NumString) {
    let (pre_rank, pre_n) = version::pre_rank(&v.pre, &v.post, &v.dev);
    let (post_rank, post_n) = match &v.post {
        None => (0, PostNum::N("0".to_string())),
        Some(n) => (1, PostNum::N(n.clone())),
    };
    let (dev_rank, dev_n) = match &v.dev {
        None => (1, "0".to_string()),
        Some(n) => (0, n.clone()),
    };
    if after_posts {
        (pre_rank, pre_n, 1, PostNum::Inf, 1, "0".to_string())
    } else {
        (pre_rank, pre_n, post_rank, post_n, dev_rank, dev_n)
    }
}

/// Compare a real version against a boundary's order key.
/// `flip` inverts the result (for boundary-on-the-left).
fn cmp_mixed(v: &ParsedVersion, b: &BoundaryVersion, flip: bool) -> Ordering {
    let ord = cmp_version_to_boundary_key(v, b);
    if flip { ord.reverse() } else { ord }
}

fn cmp_version_to_boundary_key(v: &ParsedVersion, b: &BoundaryVersion) -> Ordering {
    let (e1, r1, s1, t1) = version_key_parts(v);
    let bv = &b.version;
    let (e2, r2) = (&bv.epoch, version::trim_release(&bv.release));
    match version::cmp_num(e1, e2) {
        Ordering::Equal => {}
        other => return other,
    }
    match cmp_release_vec(&r1, &r2) {
        Ordering::Equal => {}
        other => return other,
    }
    let (pr1, pn1, por1, pon1, dr1, dn1) = s1;
    let (pr2, pn2, por2, pon2, dr2, dn2) = suffix_of(bv, b.kind == BoundaryKind::AfterPosts);
    // Suffix element-wise: pre_rank, pre_n, post_rank, post_n, dev_rank, dev_n.
    // The boundary side carries an infinite local tail.
    let ord = pr1
        .cmp(&pr2)
        .then_with(|| version::cmp_num(&pn1, &pn2))
        .then_with(|| por1.cmp(&por2))
        .then_with(|| {
            let a = PostNum::N(pon1.clone());
            a.cmp(&pon2)
        })
        .then_with(|| dr1.cmp(&dr2))
        .then_with(|| version::cmp_num(&dn1, &dn2));
    match ord {
        Ordering::Equal => {}
        other => return other,
    }
    match &t1 {
        LocalTail::Missing | LocalTail::Segs(_) => Ordering::Less,
        LocalTail::Inf => Ordering::Equal,
    }
}

pub fn cmp_boundaries(a: &BoundaryVersion, b: &BoundaryVersion) -> Ordering {
    let (ea, ra, sa) = boundary_key(a);
    let (eb, rb, sb) = boundary_key(b);
    match version::cmp_num(&ea, &eb) {
        Ordering::Equal => {}
        other => return other,
    }
    match cmp_release_vec(&ra, &rb) {
        Ordering::Equal => {}
        other => return other,
    }
    let (pra, pna, poa, pona, dra, dna) = sa;
    let (prb, pnb, pob, ponb, drb, dnb) = sb;
    pra.cmp(&prb)
        .then_with(|| version::cmp_num(&pna, &pnb))
        .then_with(|| poa.cmp(&pob))
        .then_with(|| pona.cmp(&ponb))
        .then_with(|| dra.cmp(&drb))
        .then_with(|| version::cmp_num(&dna, &dnb))
}

/// Owned order key for boundary-vs-boundary comparison.
fn boundary_key(b: &BoundaryVersion) -> BoundaryKey {
    let v = &b.version;
    let (pre_rank, pre_n) = version::pre_rank(&v.pre, &v.post, &v.dev);
    let suffix = if b.kind == BoundaryKind::AfterPosts {
        (pre_rank, pre_n, 1, PostNum::Inf, 1, "0".to_string())
    } else {
        let (post_rank, post_n) = match &v.post {
            None => (0, PostNum::N("0".to_string())),
            Some(n) => (1, PostNum::N(n.clone())),
        };
        let (dev_rank, dev_n) = match &v.dev {
            None => (1, "0".to_string()),
            Some(n) => (0, n.clone()),
        };
        (pre_rank, pre_n, post_rank, post_n, dev_rank, dev_n)
    };
    (
        v.epoch.clone(),
        version::trim_release(&v.release),
        suffix,
    )
}

/// Equality matching `BoundaryVersion.__eq__` (order-key equality).
pub fn eq_point(a: &BoundPoint, b: &BoundPoint) -> bool {
    match (a, b) {
        (BoundPoint::NegInf, BoundPoint::NegInf) => true,
        (BoundPoint::Ver(x), BoundPoint::Ver(y)) => version::cmp(x, y) == Ordering::Equal,
        (BoundPoint::Bnd(x), BoundPoint::Bnd(y)) => cmp_boundaries(x, y) == Ordering::Equal,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// LowerBound / UpperBound ordering (mirror total_ordering lt/eq).
// ---------------------------------------------------------------------------

impl LowerBound {
    /// Mirror of `LowerBound.__lt__`.
    pub fn lt(&self, other: &LowerBound) -> bool {
        if self.point.is_none() {
            return !other.point.is_none();
        }
        if other.point.is_none() {
            return false;
        }
        if !eq_point(&self.point, &other.point) {
            return cmp_point(&self.point, &other.point) == Ordering::Less;
        }
        self.inclusive && !other.inclusive
    }

    pub fn eq_bounds(&self, other: &LowerBound) -> bool {
        eq_point(&self.point, &other.point) && self.inclusive == other.inclusive
    }

    pub fn cmp_bounds(&self, other: &LowerBound) -> Ordering {
        if self.lt(other) {
            Ordering::Less
        } else if other.lt(self) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl UpperBound {
    /// Mirror of `UpperBound.__lt__`.
    pub fn lt(&self, other: &UpperBound) -> bool {
        if self.point.is_none() {
            return false;
        }
        if other.point.is_none() {
            return true;
        }
        if !eq_point(&self.point, &other.point) {
            return cmp_point(&self.point, &other.point) == Ordering::Less;
        }
        !self.inclusive && other.inclusive
    }

    pub fn eq_bounds(&self, other: &UpperBound) -> bool {
        eq_point(&self.point, &other.point) && self.inclusive == other.inclusive
    }

    pub fn cmp_bounds(&self, other: &UpperBound) -> Ordering {
        if self.lt(other) {
            Ordering::Less
        } else if other.lt(self) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

// ---------------------------------------------------------------------------
// Membership predicates.
// ---------------------------------------------------------------------------

/// Mirror of `_make_above_after_posts(version)(parsed)`.
pub fn above_after_posts(version: &ParsedVersion, parsed: &ParsedVersion) -> bool {
    // `version.__ge__(parsed)` ⟺ `cmp(version, parsed) != Less`.
    if version::cmp(version, parsed) != Ordering::Less {
        return false;
    }
    if version::cmp_num(&parsed.epoch, &version.epoch) != Ordering::Equal {
        return true;
    }
    let trimmed = version::trim_release_leave_one(&version.release);
    let tl = trimmed.len();
    if parsed.release.len() < tl {
        return true;
    }
    if parsed.release[..tl] != trimmed[..] {
        return true;
    }
    for extra in parsed.release.iter().skip(tl) {
        if extra != "0" {
            return true;
        }
    }
    if parsed.pre != version.pre {
        return true;
    }
    false
}

/// Mirror of `_make_above_after_locals(version)(parsed)`.
pub fn above_after_locals(version: &ParsedVersion, parsed: &ParsedVersion) -> bool {
    // `version.__ge__(parsed)` ⟺ `cmp(version, parsed) != Less`.
    if version::cmp(version, parsed) != Ordering::Less {
        return false;
    }
    if version::cmp_num(&parsed.epoch, &version.epoch) != Ordering::Equal {
        return true;
    }
    let trimmed = version::trim_release_leave_one(&version.release);
    let tl = trimmed.len();
    if parsed.release.len() < tl {
        return true;
    }
    if parsed.release[..tl] != trimmed[..] {
        return true;
    }
    for extra in parsed.release.iter().skip(tl) {
        if extra != "0" {
            return true;
        }
    }
    if parsed.pre != version.pre {
        return true;
    }
    if parsed.post != version.post {
        return true;
    }
    parsed.dev != version.dev
}

/// Mirror of `_make_below_after_locals(version)(parsed)`.
pub fn below_after_locals(version: &ParsedVersion, parsed: &ParsedVersion) -> bool {
    if version::cmp(version, parsed) != Ordering::Less {
        return true;
    }
    if version::cmp_num(&parsed.epoch, &version.epoch) != Ordering::Equal {
        return false;
    }
    let trimmed = version::trim_release_leave_one(&version.release);
    let tl = trimmed.len();
    if parsed.release.len() < tl {
        return false;
    }
    if parsed.release[..tl] != trimmed[..] {
        return false;
    }
    for extra in parsed.release.iter().skip(tl) {
        if extra != "0" {
            return false;
        }
    }
    if parsed.pre != version.pre {
        return false;
    }
    if parsed.post != version.post {
        return false;
    }
    parsed.dev == version.dev
}

/// Mirror of `BoundaryVersion.__ge__` for an AFTER_POSTS upper bound
/// ("is `parsed` at or below the boundary?").
/// `bv >= parsed` ⟺ `!(bv < parsed)` ⟺ `V >= parsed || parsed in V's post
/// family` (same epoch, release matching the trimmed release with zero
/// extras, same pre, and same dev or any post segment).
pub fn below_after_posts(version: &ParsedVersion, parsed: &ParsedVersion) -> bool {
    if version::cmp(version, parsed) != Ordering::Less {
        return true;
    }
    if parsed.epoch != version.epoch {
        return false;
    }
    let trimmed = version::trim_release_leave_one(&version.release);
    let tl = trimmed.len();
    if parsed.release.len() < tl {
        return false;
    }
    if parsed.release[..tl] != trimmed[..] {
        return false;
    }
    for extra in parsed.release.iter().skip(tl) {
        if extra != "0" {
            return false;
        }
    }
    if parsed.pre != version.pre {
        return false;
    }
    parsed.dev == version.dev || parsed.post.is_some()
}

/// "Is `parsed` at or above this lower bound?" (mirror of `_above`).
pub fn above_bound(lower: &LowerBound, parsed: &ParsedVersion) -> Option<bool> {
    match &lower.point {
        BoundPoint::NegInf => None,
        BoundPoint::Bnd(b) if b.kind == BoundaryKind::AfterPosts => {
            Some(above_after_posts(&b.version, parsed))
        }
        BoundPoint::Bnd(b) => Some(above_after_locals(&b.version, parsed)),
        BoundPoint::Ver(v) => Some(if lower.inclusive {
            version::cmp(v, parsed) != Ordering::Greater
        } else {
            version::cmp(v, parsed) == Ordering::Less
        }),
    }
}

/// "Is `parsed` at or below this upper bound?" (mirror of `_below`).
pub fn below_bound(upper: &UpperBound, parsed: &ParsedVersion) -> Option<bool> {
    match &upper.point {
        BoundPoint::NegInf => None,
        BoundPoint::Bnd(b)
            if b.kind == BoundaryKind::AfterLocals =>
        {
            Some(below_after_locals(&b.version, parsed))
        }
        BoundPoint::Bnd(b) if b.kind == BoundaryKind::AfterPosts => {
            Some(below_after_posts(&b.version, parsed))
        }
        // `BoundaryKind` has only the two variants above.
        BoundPoint::Bnd(_) => unreachable!("unknown boundary kind"),
        BoundPoint::Ver(v) => Some(if upper.inclusive {
            version::cmp(v, parsed) != Ordering::Less
        } else {
            version::cmp(v, parsed) == Ordering::Greater
        }),
    }
}

// ---------------------------------------------------------------------------
// Version surgery (mirror of `__replace__`-based helpers; derived values
// are always valid, so no validation is needed).
// ---------------------------------------------------------------------------

/// `Version.from_parts(epoch, release, dev=0)` shape.
pub fn from_parts_dev0(epoch: &NumString, release: Vec<NumString>) -> ParsedVersion {
    ParsedVersion {
        epoch: epoch.clone(),
        release,
        pre: None,
        post: None,
        dev: Some("0".to_string()),
        local: None,
    }
}

pub fn next_prefix_dev0(version: &ParsedVersion) -> ParsedVersion {
    let mut release = version.release.clone();
    if let Some(last) = release.last_mut() {
        *last = version::add_one_num(last);
    }
    from_parts_dev0(&version.epoch, release)
}

pub fn base_dev0(version: &ParsedVersion) -> ParsedVersion {
    from_parts_dev0(&version.epoch, version.release.clone())
}

/// Mirror of `least_version_above(boundary)`.
pub fn least_version_above(b: &BoundaryVersion) -> Option<ParsedVersion> {
    let base = &b.version;
    if b.kind == BoundaryKind::AfterLocals {
        if let Some(dev) = &base.dev {
            let mut v = base.clone();
            v.dev = Some(version::add_one_num(dev));
            v.local = None;
            return Some(v);
        }
        let next_post = match &base.post {
            Some(p) => version::add_one_num(p),
            None => "0".to_string(),
        };
        let mut v = base.clone();
        v.post = Some(next_post);
        v.dev = Some("0".to_string());
        v.local = None;
        return Some(v);
    }
    if let Some((kind, number)) = &base.pre {
        let mut v = base.clone();
        v.pre = Some((kind.clone(), version::add_one_num(number)));
        v.post = None;
        v.dev = Some("0".to_string());
        v.local = None;
        return Some(v);
    }
    None
}

/// Mirror of `range_is_empty(lower, upper)`.
pub fn range_is_empty(lower: &LowerBound, upper: &UpperBound) -> bool {
    if upper.point.is_none() {
        return false;
    }
    if lower.point.is_none() {
        if !upper.inclusive {
            if let BoundPoint::Ver(v) = &upper.point {
                if version::cmp(v, &min_version()) != Ordering::Greater {
                    return true;
                }
            }
        }
        return false;
    }
    if let BoundPoint::Bnd(b) = &lower.point {
        if let Some(successor) = least_version_above(b) {
            let succ_pt = BoundPoint::Ver(successor);
            if eq_point(&upper.point, &succ_pt) {
                return !upper.inclusive;
            }
            return cmp_point(&upper.point, &succ_pt) == Ordering::Less;
        }
    }
    if eq_point(&lower.point, &upper.point) {
        return !(lower.inclusive && upper.inclusive);
    }
    cmp_point(&lower.point, &upper.point) == Ordering::Greater
}

/// Mirror of `intersect_ranges`.
pub fn intersect_ranges(left: &[Interval], right: &[Interval]) -> Vec<Interval> {
    let mut result = Vec::new();
    let (mut li, mut ri) = (0, 0);
    while li < left.len() && ri < right.len() {
        let (ll, lu) = &left[li];
        let (rl, ru) = &right[ri];
        let lower = if ll.cmp_bounds(rl) != Ordering::Less { ll.clone() } else { rl.clone() };
        let upper = if lu.cmp_bounds(ru) != Ordering::Greater { lu.clone() } else { ru.clone() };
        if !range_is_empty(&lower, &upper) {
            result.push((lower, upper));
        }
        if lu.cmp_bounds(ru) == Ordering::Less {
            li += 1;
        } else {
            ri += 1;
        }
    }
    result
}

/// Mirror of `_union_ranges`.
pub fn union_ranges(left: &[Interval], right: &[Interval]) -> Vec<Interval> {
    if left.is_empty() {
        return right.to_vec();
    }
    if right.is_empty() {
        return left.to_vec();
    }
    let mut input: Vec<Interval> = Vec::with_capacity(left.len() + right.len());
    let (mut li, mut ri) = (0, 0);
    while li < left.len() && ri < right.len() {
        if left[li].0.cmp_bounds(&right[ri].0) != Ordering::Greater {
            input.push(left[li].clone());
            li += 1;
        } else {
            input.push(right[ri].clone());
            ri += 1;
        }
    }
    input.extend_from_slice(&left[li..]);
    input.extend_from_slice(&right[ri..]);
    let mut merged: Vec<Interval> = vec![input[0].clone()];
    for (lower, upper) in input.into_iter().skip(1) {
        let (prev_lower, prev_upper) = merged.last().unwrap().clone();
        let overlaps = if prev_upper.point.is_none()
            || lower.point.is_none()
            || cmp_point(&prev_upper.point, &lower.point) == Ordering::Greater
        {
            true
        } else if eq_point(&prev_upper.point, &lower.point) {
            prev_upper.inclusive || lower.inclusive
        } else {
            let gap_lower = LowerBound { point: prev_upper.point.clone(), inclusive: !prev_upper.inclusive };
            let gap_upper = UpperBound { point: lower.point.clone(), inclusive: !lower.inclusive };
            range_is_empty(&gap_lower, &gap_upper)
        };
        if overlaps {
            let new_upper = if prev_upper.cmp_bounds(&upper) != Ordering::Less {
                prev_upper
            } else {
                upper
            };
            *merged.last_mut().unwrap() = (prev_lower, new_upper);
        } else {
            merged.push((lower, upper));
        }
    }
    merged
}

/// Mirror of `_complement_ranges`.
pub fn complement_ranges(ranges: &[Interval]) -> Vec<Interval> {
    if ranges.is_empty() {
        return full_range();
    }
    let mut result = Vec::new();
    let mut prev_upper: Option<UpperBound> = None;
    for (lower, upper) in ranges {
        match &prev_upper {
            None => {
                if !lower.point.is_none() {
                    result.push((
                        neg_inf(),
                        UpperBound { point: lower.point.clone(), inclusive: !lower.inclusive },
                    ));
                }
            }
            Some(prev) => {
                result.push((
                    LowerBound { point: prev.point.clone(), inclusive: !prev.inclusive },
                    UpperBound { point: lower.point.clone(), inclusive: !lower.inclusive },
                ));
            }
        }
        prev_upper = Some(upper.clone());
    }
    if let Some(prev) = prev_upper {
        if !prev.point.is_none() {
            result.push((
                LowerBound { point: prev.point.clone(), inclusive: !prev.inclusive },
                pos_inf(),
            ));
        }
    }
    result
}

/// Mirror of `_canonical_floor`.
pub fn canonical_floor(bounds: Vec<Interval>) -> Vec<Interval> {
    if bounds.is_empty() {
        return bounds;
    }
    let (lower, upper) = &bounds[0];
    if range_is_empty(&neg_inf(), upper) {
        return bounds[1..].to_vec();
    }
    if lower.inclusive {
        if let BoundPoint::Ver(v) = &lower.point {
            if version::cmp(v, &min_version()) != Ordering::Greater {
                let mut out = vec![(neg_inf(), upper.clone())];
                out.extend_from_slice(&bounds[1..]);
                return out;
            }
        }
    }
    bounds
}

/// Mirror of `_predecessor_boundary`.
pub fn predecessor_boundary(version: &ParsedVersion) -> Option<BoundaryVersion> {
    let dev = version.dev.as_ref()?;
    let candidate = if version.pre.is_some() && dev == "0" && version.post.is_none() {
        let (kind, number) = version.pre.clone().unwrap();
        if version::cmp_num(&number, "1") == Ordering::Less {
            return None;
        }
        let mut base = version.clone();
        base.pre = Some((kind, version::sub_one_num(&number)));
        base.dev = None;
        BoundaryVersion { version: base, kind: BoundaryKind::AfterPosts }
    } else if version::cmp_num(dev, "1") != Ordering::Less {
        let mut base = version.clone();
        base.dev = Some(version::sub_one_num(dev));
        BoundaryVersion { version: base, kind: BoundaryKind::AfterLocals }
    } else if dev == "0" && version.post.is_some() {
        let post = version.post.clone().unwrap();
        let mut base = version.clone();
        if post == "0" {
            base.post = None;
        } else {
            base.post = Some(version::sub_one_num(&post));
        }
        base.dev = None;
        BoundaryVersion { version: base, kind: BoundaryKind::AfterLocals }
    } else {
        return None;
    };
    if least_version_above(&candidate).as_ref() == Some(version) {
        Some(candidate)
    } else {
        None
    }
}

/// Mirror of `_canonicalize`.
pub fn canonicalize(bounds: Vec<Interval>) -> Vec<Interval> {
    bounds
        .into_iter()
        .map(|(lower, upper)| {
            let mut new_lower = lower;
            let mut new_upper = upper;
            if let BoundPoint::Ver(v) = &new_lower.point {
                if new_lower.inclusive {
                    if let Some(b) = predecessor_boundary(v) {
                        new_lower = LowerBound { point: BoundPoint::Bnd(b), inclusive: false };
                    }
                }
            }
            if let BoundPoint::Ver(v) = &new_upper.point {
                if !new_upper.inclusive {
                    if let Some(b) = predecessor_boundary(v) {
                        new_upper = UpperBound { point: BoundPoint::Bnd(b), inclusive: true };
                    }
                }
            }
            (new_lower, new_upper)
        })
        .collect()
}

/// Mirror of `matches_bounds_only`.
pub fn matches_bounds_only(ranges: &[Interval], version: &ParsedVersion) -> bool {
    for (lower, upper) in ranges {
        if let Some(false) = above_bound(lower, version) { return false }
        if below_bound(upper, version) != Some(false) { return true }
    }
    false
}

/// Mirror of `resolve_prereleases`.
pub fn resolve_prereleases(configured: Option<bool>, autodetected: Option<bool>) -> Option<bool> {
    if let Some(c) = configured {
        return Some(c);
    }
    if autodetected == Some(true) {
        return Some(true);
    }
    None
}

// ---------------------------------------------------------------------------
// Per-operator range builders.
// ---------------------------------------------------------------------------

fn nearest_release_above_prerelease(version: &ParsedVersion) -> ParsedVersion {
    if version.pre.is_some() {
        let mut v = version.clone();
        v.pre = None;
        v.post = None;
        v.dev = None;
        v.local = None;
        return v;
    }
    let mut v = version.clone();
    v.dev = None;
    v.local = None;
    v
}

fn lowest_release_at_or_above(point: &BoundPoint) -> ParsedVersion {
    match point {
        BoundPoint::NegInf => min_release(),
        BoundPoint::Bnd(b) => {
            let inner = &b.version;
            if inner.pre.is_some() || inner.dev.is_some() {
                return nearest_release_above_prerelease(inner);
            }
            let next_post = match &inner.post {
                Some(p) => version::add_one_num(p),
                None => "0".to_string(),
            };
            let mut v = inner.clone();
            v.post = Some(next_post);
            v.local = None;
            v
        }
        BoundPoint::Ver(v) => {
            if v.pre.is_some() || v.dev.is_some() {
                nearest_release_above_prerelease(v)
            } else {
                v.clone()
            }
        }
    }
}

/// Mirror of `ranges_are_prerelease_only`.
pub fn ranges_are_prerelease_only(ranges: &[Interval]) -> bool {
    for (lower, upper) in ranges {
        let nearest = lowest_release_at_or_above(&lower.point);
        let nearest_pt = BoundPoint::Ver(nearest);
        match &upper.point {
            BoundPoint::NegInf => return false,
            _ => {
                if cmp_point(&nearest_pt, &upper.point) == Ordering::Less {
                    return false;
                }
                if eq_point(&nearest_pt, &upper.point) && upper.inclusive {
                    return false;
                }
            }
        }
    }
    true
}

/// Mirror of `wildcard_ranges`.
pub fn wildcard_ranges(op: &str, base: &ParsedVersion) -> Vec<Interval> {
    let lower_v = base_dev0(base);
    let upper_v = next_prefix_dev0(base);
    if op == "==" {
        vec![(
            LowerBound { point: BoundPoint::Ver(lower_v), inclusive: true },
            UpperBound { point: BoundPoint::Ver(upper_v), inclusive: false },
        )]
    } else {
        vec![
            (neg_inf(), UpperBound { point: BoundPoint::Ver(lower_v), inclusive: false }),
            (
                LowerBound { point: BoundPoint::Ver(upper_v), inclusive: true },
                pos_inf(),
            ),
        ]
    }
}

/// Mirror of `standard_ranges`. `version` is the parsed spec version;
/// `has_local` mirrors `"+" in version_str`.
pub fn standard_ranges(op: &str, version: &ParsedVersion, has_local: bool) -> Vec<Interval> {
    match op {
        ">=" => vec![(LowerBound { point: BoundPoint::Ver(version.clone()), inclusive: true }, pos_inf())],
        "<=" => vec![(
            neg_inf(),
            UpperBound {
                point: BoundPoint::Bnd(BoundaryVersion { version: version.clone(), kind: BoundaryKind::AfterLocals }),
                inclusive: true,
            },
        )],
        ">" => {
            if let Some(dev) = &version.dev {
                let mut v = version.clone();
                v.dev = Some(version::add_one_num(dev));
                v.local = None;
                return vec![(LowerBound { point: BoundPoint::Ver(v), inclusive: true }, pos_inf())];
            }
            if let Some(post) = &version.post {
                let mut v = version.clone();
                v.post = Some(version::add_one_num(post));
                v.dev = Some("0".to_string());
                v.local = None;
                return vec![(LowerBound { point: BoundPoint::Ver(v), inclusive: true }, pos_inf())];
            }
            vec![(
                LowerBound {
                    point: BoundPoint::Bnd(BoundaryVersion { version: version.clone(), kind: BoundaryKind::AfterPosts }),
                    inclusive: false,
                },
                pos_inf(),
            )]
        }
        "<" => {
            let bound = if is_prerelease(version) {
                version.clone()
            } else {
                let mut v = version.clone();
                v.dev = Some("0".to_string());
                v.local = None;
                v
            };
            if version::cmp(&bound, &min_version()) != Ordering::Greater {
                return Vec::new();
            }
            vec![(neg_inf(), UpperBound { point: BoundPoint::Ver(bound), inclusive: false })]
        }
        "==" | "!=" => {
            let after_locals = BoundaryVersion { version: version.clone(), kind: BoundaryKind::AfterLocals };
            if op == "==" {
                if has_local {
                    vec![(
                        LowerBound { point: BoundPoint::Ver(version.clone()), inclusive: true },
                        UpperBound { point: BoundPoint::Ver(version.clone()), inclusive: true },
                    )]
                } else {
                    vec![(
                        LowerBound { point: BoundPoint::Ver(version.clone()), inclusive: true },
                        UpperBound { point: BoundPoint::Bnd(after_locals), inclusive: true },
                    )]
                }
            } else if has_local {
                vec![
                    (neg_inf(), UpperBound { point: BoundPoint::Ver(version.clone()), inclusive: false }),
                    (LowerBound { point: BoundPoint::Ver(version.clone()), inclusive: false }, pos_inf()),
                ]
            } else {
                vec![
                    (neg_inf(), UpperBound { point: BoundPoint::Ver(version.clone()), inclusive: false }),
                    (
                        LowerBound { point: BoundPoint::Bnd(after_locals), inclusive: false },
                        pos_inf(),
                    ),
                ]
            }
        }
        "~=" => {
            let mut prefix_release = version.release.clone();
            prefix_release.pop();
            let prefix = ParsedVersion {
                epoch: version.epoch.clone(),
                release: prefix_release,
                pre: version.pre.clone(),
                post: version.post.clone(),
                dev: version.dev.clone(),
                local: None,
            };
            vec![(
                LowerBound { point: BoundPoint::Ver(version.clone()), inclusive: true },
                UpperBound { point: BoundPoint::Ver(next_prefix_dev0(&prefix)), inclusive: false },
            )]
        }
        _ => Vec::new(),
    }
}

/// Mirror of `bounds_for_spec`.
pub fn bounds_for_spec(op: &str, version_str: &str, version: &ParsedVersion) -> Vec<Interval> {
    if version_str.ends_with(".*") {
        return wildcard_ranges(op, version);
    }
    standard_ranges(op, version, version_str.contains('+'))
}

/// Mirror of `intersect_specifier_bounds`.
pub fn intersect_specifier_bounds(per_spec: Vec<Vec<Interval>>) -> Vec<Interval> {
    let mut result: Option<Vec<Interval>> = None;
    for sub in per_spec {
        match result {
            None => result = Some(sub),
            Some(cur) => {
                let next = intersect_ranges(&cur, &sub);
                if next.is_empty() {
                    return Vec::new();
                }
                result = Some(next);
            }
        }
    }
    result.unwrap_or_default()
}

pub fn is_prerelease(v: &ParsedVersion) -> bool {
    v.dev.is_some() || v.pre.is_some()
}

// ---------------------------------------------------------------------------
// Specifier-string parsing (mirror of `Specifier._regex` + op split).
// ---------------------------------------------------------------------------

/// A validated `(operator, version)` specifier body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecBody {
    pub op: String,
    pub version: String,
}

/// Validate a specifier string, mirroring `_regex.fullmatch` with
/// `VERBOSE | IGNORECASE`, then split operator/version exactly like
/// `Specifier.__init__` does post-match.
pub fn parse_spec(s: &str) -> Option<SpecBody> {
    let chars: Vec<char> = s.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && version::is_py_space(chars[start]) {
        start += 1;
    }
    while end > start && version::is_py_space(chars[end - 1]) {
        end -= 1;
    }
    if start == end {
        return None;
    }
    let text: String = chars[start..end].iter().collect();
    let c: Vec<char> = chars[start..end].to_vec();
    // Operator dispatch on the stripped text (mirrors the startswith chain,
    // tried only after a successful fullmatch — here fused with validation).
    if text.starts_with("===") {
        let rest: String = c[3..].iter().collect();
        let rest = rest.trim_start_matches(|ch: char| version::is_py_space(ch));
        if rest.is_empty() {
            return None;
        }
        if rest.chars().all(|ch| !version::is_py_space(ch) && ch != ';' && ch != ')') {
            return Some(SpecBody { op: "===".to_string(), version: rest.to_string() });
        }
        return None;
    }
    for op in ["~=", "==", "!=", "<=", ">="] {
        if text.starts_with(op) {
            let rest: String = c[op.len()..].iter().collect();
            let body = skip_spaces(&rest);
            return parse_op_body(op, body).map(|version| SpecBody { op: op.to_string(), version }).ok();
        }
    }
    let first = c[0];
    if first == '<' || first == '>' {
        let rest: String = c[1..].iter().collect();
        let body = skip_spaces(&rest);
        let op = first.to_string();
        return parse_op_body(&op, body).map(|version| SpecBody { op, version }).ok();
    }
    None
}

fn skip_spaces(s: &str) -> &str {
    let mut i = 0;
    for ch in s.chars() {
        if version::is_py_space(ch) {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    &s[i..]
}

/// Validate the version body for one operator class.
fn parse_op_body(op: &str, body: &str) -> Result<String, ()> {
    let chars: Vec<char> = body.chars().collect();
    let mut p = Parser::new(&chars, None);
    // Optional `v` prefix (case-insensitive).
    if matches!(p.peek(), Some('v') | Some('V')) {
        p.pos += 1;
    }
    // Optional epoch.
    {
        let save = p.pos;
        if p.eat_digits().is_some() {
            if p.peek() == Some('!') {
                p.pos += 1;
            } else {
                p.pos = save;
            }
        }
    }
    // Release segment (required).
    let first = p.eat_digits().ok_or(())?;
    let _ = first;
    let mut groups = 1;
    if op == "~=" {
        // Compatible release needs at least two release segments.
        let mut extra = 0;
        while p.peek() == Some('.') {
            if p.chars.get(p.pos + 1).copied().is_some_and(version::is_digit) {
                p.pos += 1;
                p.eat_digits();
                extra += 1;
            } else {
                break;
            }
        }
        if extra == 0 {
            return Err(());
        }
        groups += extra;
    } else {
        while p.peek() == Some('.') {
            if p.chars.get(p.pos + 1).copied().is_some_and(version::is_digit) {
                p.pos += 1;
                p.eat_digits();
                groups += 1;
            } else {
                break;
            }
        }
    }
    let _ = groups;
    match op {
        "==" | "!=" => {
            // Wildcard or the pre/post/dev/local chain.
            if p.peek() == Some('.') && p.chars.get(p.pos + 1).copied() == Some('*') {
                p.pos += 2;
                if p.pos != p.chars.len() {
                    return Err(());
                }
                return Ok(body.to_string());
            }
            let _ = p.eat_pre().map_err(|_| ())?;
            let _ = p.eat_post().map_err(|_| ())?;
            let _ = p.eat_dev().map_err(|_| ())?;
            let _ = p.eat_local().map_err(|_| ())?;
            if p.pos != p.chars.len() {
                return Err(());
            }
            Ok(body.to_string())
        }
        "~=" | "<" | ">" | "<=" | ">=" => {
            let _ = p.eat_pre().map_err(|_| ())?;
            let _ = p.eat_post().map_err(|_| ())?;
            let _ = p.eat_dev().map_err(|_| ())?;
            if p.pos != p.chars.len() {
                return Err(());
            }
            Ok(body.to_string())
        }
        _ => Err(()),
    }
}

// ---------------------------------------------------------------------------
// `to_specifier_set` encoding (mirror of `ranges.py` helpers).
// ---------------------------------------------------------------------------

/// Cap on materialized `!=` chains.
pub const MAX_EXCLUSION_RUN: usize = 128;

/// Mirror of `_is_dev0_version`.
pub fn is_dev0_version(v: &ParsedVersion) -> bool {
    v.dev.as_deref() == Some("0") && v.pre.is_none() && v.post.is_none() && v.local.is_none()
}

/// Mirror of `_clean_lower`.
pub fn clean_lower(version: &ParsedVersion) -> Option<Vec<String>> {
    if version.dev.as_deref() != Some("0") || version.pre.is_some() || version.local.is_some() {
        return None;
    }
    if let Some(post) = &version.post {
        if version::cmp_num(post, "1") == Ordering::Less {
            return None;
        }
        let mut base = version.clone();
        base.post = Some(version::sub_one_num(post));
        base.dev = None;
        return Some(vec![format!(">{}", version::display(&base))]);
    }
    let family = version::trim_release_leave_one(&version.release);
    let last = family.last()?;
    if version::cmp_num(last, "1") == Ordering::Less {
        return None;
    }
    let mut below_release: Vec<NumString> = family[..family.len() - 1].to_vec();
    below_release.push(version::sub_one_num(last));
    let below = ParsedVersion {
        epoch: version.epoch.clone(),
        release: below_release.clone(),
        pre: None,
        post: None,
        dev: None,
        local: None,
    };
    if version.epoch == "0" && below_release.iter().all(|n| n == "0") {
        return Some(vec![format!("!={}.*", version::display(&below))]);
    }
    Some(vec![
        format!(">={}", version::display(&below)),
        format!("!={}.*", version::display(&below)),
    ])
}

/// Mirror of `_dev_family_anchor`.
pub fn dev_family_anchor(family: &ParsedVersion) -> Option<Vec<String>> {
    if version::cmp(family, &min_version()) != Ordering::Greater {
        return Some(Vec::new());
    }
    if let Some(clean) = clean_lower(family) {
        return Some(clean);
    }
    if family.pre.is_none() && family.post.as_deref() == Some("0") {
        let mut base = family.clone();
        base.post = None;
        base.dev = None;
        let s = version::display(&base);
        return Some(vec![format!(">={s}"), format!("!={s}")]);
    }
    None
}

/// Mirror of `_encode_lower`.
pub fn encode_lower(lower: &LowerBound, keep_dev0: bool) -> Option<Vec<String>> {
    let lv = match &lower.point {
        BoundPoint::NegInf => return Some(Vec::new()),
        BoundPoint::Ver(v) => {
            if !lower.inclusive {
                return None;
            }
            if !keep_dev0 {
                if let Some(clean) = clean_lower(v) {
                    return Some(clean);
                }
            }
            return Some(vec![format!(">={}", version::display(v))]);
        }
        BoundPoint::Bnd(b) => b,
    };
    if lv.kind == BoundaryKind::AfterPosts {
        return Some(vec![format!(">{}", version::display(&lv.version))]);
    }
    let inner = &lv.version;
    if version::cmp(inner, &min_version()) != Ordering::Greater {
        return Some(vec![format!("!={}", version::display(inner))]);
    }
    if !keep_dev0 {
        if let Some(dev) = &inner.dev {
            let mut family = inner.clone();
            family.dev = Some("0".to_string());
            if let Some(anchor) = dev_family_anchor(&family) {
                if version::cmp_num(&version::add_one_num(dev), &MAX_EXCLUSION_RUN.to_string()) == Ordering::Greater {
                    return None;
                }
                let mut out = anchor;
                let mut d = "0".to_string();
                loop {
                    let mut v = family.clone();
                    v.dev = Some(d.clone());
                    out.push(format!("!={}", version::display(&v)));
                    if d == *dev {
                        break;
                    }
                    d = version::add_one_num(&d);
                }
                return Some(out);
            }
        } else if let Some(successor) = least_version_above(lv) {
            if let Some(clean) = clean_lower(&successor) {
                return Some(clean);
            }
        }
    }
    Some(vec![
        format!(">={}", version::display(inner)),
        format!("!={}", version::display(inner)),
    ])
}

/// Mirror of `_encode_upper`.
pub fn encode_upper(upper: &UpperBound, keep_dev0: bool) -> Option<Vec<String>> {
    match &upper.point {
        BoundPoint::NegInf => Some(Vec::new()),
        BoundPoint::Ver(v) => {
            if !upper.inclusive {
                if v.dev.as_deref() == Some("0") && v.pre.is_none() && v.local.is_none() {
                    if !keep_dev0 {
                        let mut base = v.clone();
                        base.dev = None;
                        return Some(vec![format!("<{}", version::display(&base))]);
                    }
                    return Some(vec![format!("<{}", version::display(v))]);
                }
                return Some(vec![
                    format!("<={}", version::display(v)),
                    format!("!={}", version::display(v)),
                ]);
            }
            None
        }
        BoundPoint::Bnd(b) => {
            if b.kind == BoundaryKind::AfterLocals {
                let inner = &b.version;
                if !keep_dev0 && inner.pre.is_none() && inner.post.is_some() && inner.dev.is_none() {
                    let post = inner.post.clone().unwrap();
                    let mut next = inner.clone();
                    next.post = Some(version::add_one_num(&post));
                    return Some(vec![format!("<{}", version::display(&next))]);
                }
                return Some(vec![format!("<={}", version::display(inner))]);
            }
            match least_version_above(b) {
                Some(successor) => Some(vec![format!("<{}", version::display(&successor))]),
                None => None,
            }
        }
    }
}

/// Mirror of `_detect_equal_wildcard`.
pub fn detect_equal_wildcard(lower: &LowerBound, upper: &UpperBound) -> Option<ParsedVersion> {
    let (lv, uv) = match (&lower.point, &upper.point) {
        (BoundPoint::Ver(l), BoundPoint::Ver(u)) => (l, u),
        _ => return None,
    };
    if !lower.inclusive || upper.inclusive {
        return None;
    }
    if !(is_dev0_version(lv) && is_dev0_version(uv)) {
        return None;
    }
    if lv.epoch != uv.epoch {
        return None;
    }
    let mut lr = version::trim_release_leave_one(&lv.release);
    let mut ur = version::trim_release_leave_one(&uv.release);
    let padded = lr.len().max(ur.len());
    // `_detect_equal_wildcard` pads with zeros; trimmed tails have no
    // zeros, so padding only extends the shorter side.
    lr.extend(std::iter::repeat_n("0".to_string(), padded - lr.len()));
    ur.extend(std::iter::repeat_n("0".to_string(), padded - ur.len()));
    if padded == 0 || lr[..padded - 1] != ur[..padded - 1] {
        return None;
    }
    if version::cmp_num(&ur[padded - 1], &version::add_one_num(&lr[padded - 1])) != Ordering::Equal {
        return None;
    }
    let mut wildcard = lv.clone();
    wildcard.release = lr;
    wildcard.dev = None;
    Some(wildcard)
}

/// Mirror of `_epoch_floor_lower`.
pub fn epoch_floor_lower(
    lower: &LowerBound,
    upper: &UpperBound,
) -> Option<(ParsedVersion, NumString, bool)> {
    let (version, excluded_devs) = match &lower.point {
        BoundPoint::Bnd(b) if b.kind == BoundaryKind::AfterLocals => {
            let v = &b.version;
            let dev = v.dev.as_ref()?;
            (v.clone(), version::add_one_num(dev))
        }
        BoundPoint::Ver(v) if lower.inclusive => {
            if v.dev.as_deref() != Some("0") {
                return None;
            }
            (v.clone(), "0".to_string())
        }
        _ => return None,
    };
    if version.epoch == "0" {
        return None;
    }
    if version.pre.is_some() || version.post.is_some() || version.local.is_some() {
        return None;
    }
    if version::trim_release_leave_one(&version.release).iter().any(|n| n != "0") {
        return None;
    }
    let next_family = ParsedVersion {
        epoch: version.epoch.clone(),
        release: vec!["1".to_string()],
        pre: None,
        post: None,
        dev: Some("0".to_string()),
        local: None,
    };
    let cap = UpperBound { point: BoundPoint::Ver(next_family), inclusive: false };
    if upper.cmp_bounds(&cap) == Ordering::Greater {
        return None;
    }
    let family = ParsedVersion {
        epoch: version.epoch.clone(),
        release: vec!["0".to_string()],
        pre: None,
        post: None,
        dev: None,
        local: None,
    };
    let upper_at_cap = upper.eq_bounds(&cap);
    Some((family, excluded_devs, upper_at_cap))
}

/// Mirror of `_encode_interval`.
pub fn encode_interval(lower: &LowerBound, upper: &UpperBound, keep_dev0: bool) -> Option<Vec<String>> {
    // `[V+local, V+local]` singleton.
    if let (BoundPoint::Ver(lv), BoundPoint::Ver(uv)) = (&lower.point, &upper.point) {
        if lower.inclusive && upper.inclusive && version::cmp(lv, uv) == Ordering::Equal && lv.local.is_some() {
            return Some(vec![format!("=={}", version::display(lv))]);
        }
    }
    // `[V, AFTER_LOCALS(V)]` singleton.
    if let BoundPoint::Ver(lv) = &lower.point {
        if lower.inclusive && upper.inclusive {
            if let BoundPoint::Bnd(b) = &upper.point {
                if b.kind == BoundaryKind::AfterLocals && version::cmp(&b.version, lv) == Ordering::Equal {
                    return Some(vec![format!("=={}", version::display(lv))]);
                }
            }
        }
    }
    if let Some(wildcard) = detect_equal_wildcard(lower, upper) {
        return Some(vec![format!("=={}.*", version::display(&wildcard))]);
    }
    if !keep_dev0 {
        if let Some((family, excluded_devs, upper_at_cap)) = epoch_floor_lower(lower, upper) {
            if version::cmp_num(&excluded_devs, &MAX_EXCLUSION_RUN.to_string()) == Ordering::Greater {
                return None;
            }
            let mut parts = vec![format!("=={}.*", version::display(&family))];
            // `range(excluded_devs)`: devs `0..excluded_devs` (empty when 0).
            let mut d = "0".to_string();
            while d != excluded_devs {
                let mut v = family.clone();
                v.dev = Some(d.clone());
                parts.push(format!("!={}", version::display(&v)));
                d = version::add_one_num(&d);
            }
            if !upper_at_cap {
                parts.extend(encode_upper(upper, keep_dev0)?);
            }
            return Some(parts);
        }
    }
    let mut parts = encode_lower(lower, keep_dev0)?;
    parts.extend(encode_upper(upper, keep_dev0)?);
    Some(parts)
}

/// Mirror of `_detect_not_equal`.
pub fn detect_not_equal(left_upper: &UpperBound, right_lower: &LowerBound) -> Option<Vec<ParsedVersion>> {
    let first = match &left_upper.point {
        BoundPoint::Bnd(b) => least_version_above(b)?,
        BoundPoint::NegInf => return None,
        BoundPoint::Ver(v) => {
            if left_upper.inclusive {
                return None;
            }
            v.clone()
        }
    };
    match &right_lower.point {
        BoundPoint::Ver(rv) => {
            if !right_lower.inclusive || version::cmp(rv, &first) != Ordering::Equal || first.local.is_none() {
                // Single-point gap `[V+local, V+local]` only.
                if !right_lower.inclusive
                    && version::cmp(rv, &first) == Ordering::Equal
                    && first.local.is_some()
                {
                    return Some(vec![first]);
                }
                return None;
            }
            None
        }
        BoundPoint::NegInf => None,
        BoundPoint::Bnd(rb) => {
            if rb.kind != BoundaryKind::AfterLocals {
                return None;
            }
            let last = &rb.version;
            if version::cmp(&first, last) == Ordering::Equal {
                return Some(vec![first]);
            }
            let second = least_version_above(&BoundaryVersion { version: first.clone(), kind: BoundaryKind::AfterLocals })?;
            if second.dev.is_none() || last.dev.is_none() {
                return None;
            }
            let (sd, ld) = (second.dev.clone().unwrap(), last.dev.clone().unwrap());
            if version::cmp_num(&ld, &sd) == Ordering::Less {
                return None;
            }
            let mut probe = last.clone();
            probe.dev = Some(sd.clone());
            if version::cmp(&probe, &second) != Ordering::Equal {
                return None;
            }
            // Chain length check against the shared cap: `first` plus the
            // dev run is `last.dev - second.dev + 2` points; past the cap
            // there is no exclusion form. The loop caps iterations at the
            // same bound, so astronomic dev numbers terminate too.
            let mut run = vec![first];
            let mut d = sd;
            loop {
                let mut v = second.clone();
                v.dev = Some(d.clone());
                run.push(v);
                if d == ld {
                    break;
                }
                d = version::add_one_num(&d);
                if run.len() > MAX_EXCLUSION_RUN {
                    return None;
                }
            }
            if run.len() > MAX_EXCLUSION_RUN {
                return None;
            }
            Some(run)
        }
    }
}

/// Mirror of `_decompose_dev0_gap`.
pub fn decompose_dev0_gap(
    lower_trim: &[NumString],
    upper_trim: &[NumString],
    epoch: &NumString,
    budget: usize,
) -> Option<Vec<ParsedVersion>> {
    let mut diff = 0;
    while diff < lower_trim.len() && diff < upper_trim.len() && lower_trim[diff] == upper_trim[diff] {
        diff += 1;
    }
    if lower_trim.len() > diff + 1 {
        return None;
    }
    let common = &lower_trim[..diff];
    let lower_val: usize = if lower_trim.len() > diff {
        lower_trim[diff].parse().ok()?
    } else {
        0
    };
    let upper_val: usize = upper_trim.get(diff)?.parse::<usize>().ok()?;
    let span = upper_val.saturating_sub(lower_val);
    if span > budget {
        return None;
    }
    let mut fragments: Vec<ParsedVersion> = (lower_val..upper_val)
        .map(|segment| {
            let mut release: Vec<NumString> = common.to_vec();
            release.push(segment.to_string());
            ParsedVersion {
                epoch: epoch.clone(),
                release,
                pre: None,
                post: None,
                dev: None,
                local: None,
            }
        })
        .collect();
    if upper_trim.len() == diff + 1 {
        return Some(fragments);
    }
    let mut next_lower = common.to_vec();
    next_lower.push(upper_val.to_string());
    let tail = decompose_dev0_gap(
        &next_lower,
        upper_trim,
        epoch,
        budget.saturating_sub(span.max(1)),
    )?;
    fragments.extend(tail);
    Some(fragments)
}

/// Mirror of `_encode_gap`.
pub fn encode_gap(left_upper: &UpperBound, right_lower: &LowerBound) -> Option<Vec<String>> {
    if let Some(points) = detect_not_equal(left_upper, right_lower) {
        return Some(points.iter().map(|p| format!("!={}", version::display(p))).collect());
    }
    let left_v = match &left_upper.point {
        BoundPoint::Ver(v) => v,
        _ => return None,
    };
    if left_upper.inclusive || !is_dev0_version(left_v) {
        return None;
    }
    let (upper_dev0, run_length, budget) = match &right_lower.point {
        BoundPoint::Ver(rv) if right_lower.inclusive => (rv.clone(), 0usize, MAX_EXCLUSION_RUN),
        BoundPoint::Bnd(rb)
            if rb.kind == BoundaryKind::AfterLocals && !right_lower.inclusive =>
        {
            let upper = &rb.version;
            let dev_num: usize = upper.dev.as_ref()?.parse().ok()?;
            if dev_num + 1 > MAX_EXCLUSION_RUN {
                return None;
            }
            let mut base = upper.clone();
            base.dev = Some("0".to_string());
            (base, dev_num + 1, MAX_EXCLUSION_RUN - (dev_num + 1))
        }
        _ => return None,
    };
    if !is_dev0_version(&upper_dev0) {
        return None;
    }
    if left_v.epoch != upper_dev0.epoch || version::cmp(left_v, &upper_dev0) != Ordering::Less {
        return None;
    }
    let prefixes = decompose_dev0_gap(
        &version::trim_release_leave_one(&left_v.release),
        &version::trim_release_leave_one(&upper_dev0.release),
        &left_v.epoch,
        budget,
    )?;
    let mut exclusions: Vec<String> = prefixes.iter().map(|p| format!("!={}.*", version::display(p))).collect();
    for d in 0..run_length {
        let mut v = upper_dev0.clone();
        v.dev = Some(d.to_string());
        exclusions.push(format!("!={}", version::display(&v)));
    }
    Some(exclusions)
}

/// Mirror of `_encode_gaps`.
pub fn encode_gaps(bounds: &[Interval]) -> Option<Vec<String>> {
    let mut exclusions = Vec::new();
    for index in 1..bounds.len() {
        exclusions.extend(encode_gap(&bounds[index - 1].1, &bounds[index].0)?);
    }
    Some(exclusions)
}

/// Mirror of `_tighten_no_prereleases`.
pub fn tighten_no_prereleases(bounds: Vec<Interval>) -> Vec<Interval> {
    let (lower, upper) = match bounds.last() {
        Some(last) => last.clone(),
        None => return bounds,
    };
    if let BoundPoint::Ver(v) = &upper.point {
        if !upper.inclusive && !is_prerelease(v) && v.local.is_none() {
            let mut snapped = v.clone();
            snapped.dev = Some("0".to_string());
            let mut out = bounds[..bounds.len() - 1].to_vec();
            out.push((lower, UpperBound { point: BoundPoint::Ver(snapped), inclusive: false }));
            return out;
        }
    }
    bounds
}

// ---------------------------------------------------------------------------
// Repr helpers (mirror of `_format_*`).
// ---------------------------------------------------------------------------

/// Mirror of `_bound_version_str`.
pub fn bound_version_str(point: &BoundPoint) -> String {
    match point {
        BoundPoint::NegInf => "?".to_string(),
        BoundPoint::Ver(v) => version::display(v),
        BoundPoint::Bnd(b) => {
            let kind = match b.kind {
                BoundaryKind::AfterLocals => "AFTER_LOCALS",
                BoundaryKind::AfterPosts => "AFTER_POSTS",
            };
            format!("{}[{kind}]", version::display(&b.version))
        }
    }
}

/// Mirror of `_format_intervals`.
pub fn format_intervals(bounds: &[Interval]) -> String {
    bounds
        .iter()
        .map(|(lower, upper)| {
            let l = match &lower.point {
                BoundPoint::NegInf => "(-inf".to_string(),
                p => format!("{}{}", if lower.inclusive { "[" } else { "(" }, bound_version_str(p)),
            };
            let u = match &upper.point {
                BoundPoint::NegInf => "+inf)".to_string(),
                p => format!("{}{}", bound_version_str(p), if upper.inclusive { "]" } else { ")" }),
            };
            format!("{l}, {u}")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

// ---------------------------------------------------------------------------
// VersionRange state + set algebra over it.
// ---------------------------------------------------------------------------

/// Mirror of the `VersionRange` slot state (bounds, literals, flags, region,
/// configured policy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeState {
    pub bounds: Vec<Interval>,
    pub admit: HashSet<String>,
    pub reject: HashSet<String>,
    pub admit_arbitrary: bool,
    pub pre_region: Vec<Interval>,
    pub configured: Option<bool>,
}

impl RangeState {
    pub fn has_literals(&self) -> bool {
        !self.admit.is_empty() || !self.reject.is_empty()
    }

    pub fn arbitrary_active(&self) -> bool {
        self.admit_arbitrary && self.bounds == full_range()
    }

    pub fn is_plain(&self) -> bool {
        !self.has_literals() && !self.admit_arbitrary && self.configured != Some(false)
    }

    /// Mirror of `VersionRange._build` (with `admits` resolved by the caller
    /// through `struct_admits`).
    pub fn build(
        bounds: Vec<Interval>,
        admit: HashSet<String>,
        reject: HashSet<String>,
        admit_arbitrary: bool,
        pre_region: Vec<Interval>,
        configured: Option<bool>,
        struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool,
    ) -> Self {
        let bounds = canonicalize(bounds);
        let mut admit = if !admit.is_empty() && !reject.is_empty() {
            admit.difference(&reject).cloned().collect()
        } else {
            admit
        };
        if !admit.is_empty() {
            admit.retain(|literal| !struct_admits(&bounds, admit_arbitrary, literal));
        }
        let reject = if !reject.is_empty() {
            reject
                .into_iter()
                .filter(|literal| struct_admits(&bounds, admit_arbitrary, literal))
                .collect()
        } else {
            reject
        };
        let pre_region = if configured.is_some() || pre_region.is_empty() {
            Vec::new()
        } else {
            intersect_ranges(&canonicalize(pre_region), &bounds)
        };
        RangeState { bounds, admit, reject, admit_arbitrary, pre_region, configured }
    }

    pub fn check_policy_compat(&self, other: &RangeState) -> Result<(), (Option<bool>, Option<bool>)> {
        if self.configured != other.configured {
            return Err((self.configured, other.configured));
        }
        Ok(())
    }

    pub fn merged_region(&self, other: &RangeState) -> Vec<Interval> {
        if other.pre_region.is_empty() {
            return self.pre_region.clone();
        }
        if self.pre_region.is_empty() {
            return other.pre_region.clone();
        }
        union_ranges(&self.pre_region, &other.pre_region)
    }

    #[allow(clippy::too_many_arguments)]
    fn combine_literals(
        &self,
        other: &RangeState,
        new_bounds: Vec<Interval>,
        op: SetOp,
        admit_arbitrary: bool,
        pre_region: Vec<Interval>,
        configured: Option<bool>,
        struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool,
    ) -> Self {
        let mut admits = HashSet::new();
        let mut rejects = HashSet::new();
        let literals: HashSet<&String> = self
            .admit
            .iter()
            .chain(self.reject.iter())
            .chain(other.admit.iter())
            .chain(other.reject.iter())
            .collect();
        for literal in literals {
            let self_in = self.matches_literal(literal);
            let other_in = other.matches_literal(literal);
            let want = match op {
                SetOp::Intersection => self_in && other_in,
                SetOp::Union => self_in || other_in,
                SetOp::Difference => self_in && !other_in,
            };
            if want {
                admits.insert(literal.clone());
            } else {
                rejects.insert(literal.clone());
            }
        }
        Self::build(new_bounds, admits, rejects, admit_arbitrary, pre_region, configured, struct_admits)
    }

    fn matches_literal(&self, literal: &str) -> bool {
        if self.reject.contains(literal) {
            return false;
        }
        if self.admit.contains(literal) {
            return true;
        }
        match version::parse(literal, None) {
            Err(_) => self.arbitrary_active(),
            Ok(parsed) => matches_bounds_only(&self.bounds, &parsed),
        }
    }

    pub fn intersection(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> Self {
        let new_bounds = intersect_ranges(&self.bounds, &other.bounds);
        let new_region = self.merged_region(other);
        let combined_arb = self.admit_arbitrary && other.admit_arbitrary && !new_bounds.is_empty();
        let configured = self.configured;
        if !self.has_literals() && !other.has_literals() {
            return Self::build(new_bounds, HashSet::new(), HashSet::new(), combined_arb, new_region, configured, struct_admits);
        }
        self.combine_literals(other, new_bounds, SetOp::Intersection, combined_arb, new_region, configured, struct_admits)
    }

    pub fn union(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> Self {
        let new_bounds = union_ranges(&self.bounds, &other.bounds);
        let new_region = self.merged_region(other);
        let combined_arb = if !new_bounds.is_empty() {
            (self.admit_arbitrary && !self.bounds.is_empty())
                || (other.admit_arbitrary && !other.bounds.is_empty())
        } else {
            self.admit_arbitrary || other.admit_arbitrary
        };
        let configured = self.configured;
        if !self.has_literals() && !other.has_literals() {
            return Self::build(new_bounds, HashSet::new(), HashSet::new(), combined_arb, new_region, configured, struct_admits);
        }
        self.combine_literals(other, new_bounds, SetOp::Union, combined_arb, new_region, configured, struct_admits)
    }

    pub fn complement(&self, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> Self {
        Self::build(
            complement_ranges(&self.bounds),
            self.reject.clone(),
            self.admit.clone(),
            self.admit_arbitrary,
            Vec::new(),
            self.configured,
            struct_admits,
        )
    }

    pub fn difference(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> Self {
        if other.bounds.is_empty() && other.admit.is_empty() {
            return self.clone();
        }
        let new_bounds = intersect_ranges(&self.bounds, &complement_ranges(&other.bounds));
        let new_region = if self.configured.is_none() { self.pre_region.clone() } else { Vec::new() };
        let combined_arb = self.admit_arbitrary && new_bounds == self.bounds;
        if !self.has_literals() && !other.has_literals() {
            return Self::build(new_bounds, HashSet::new(), HashSet::new(), combined_arb, new_region, self.configured, struct_admits);
        }
        self.combine_literals(other, new_bounds, SetOp::Difference, combined_arb, new_region, self.configured, struct_admits)
    }

    pub fn is_subset(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> bool {
        if self.arbitrary_active() && !other.arbitrary_active() {
            return false;
        }
        if self.is_plain() && other.is_plain() {
            return intersect_ranges(&self.bounds, &complement_ranges(&other.bounds)).is_empty();
        }
        self.difference(other, struct_admits).is_empty_state()
    }

    pub fn is_superset(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> bool {
        other.is_subset(self, struct_admits)
    }

    pub fn is_disjoint(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> bool {
        if self.is_plain() && other.is_plain() {
            return intersect_ranges(&self.bounds, &other.bounds).is_empty();
        }
        self.intersection(other, struct_admits).is_empty_state()
    }

    pub fn same_releases(&self, other: &RangeState, struct_admits: &dyn Fn(&[Interval], bool, &str) -> bool) -> bool {
        self.difference(other, struct_admits).is_empty_state()
            && other.difference(self, struct_admits).is_empty_state()
    }

    /// Mirror of `is_empty` (property).
    pub fn is_empty_state(&self) -> bool {
        if self.arbitrary_active() {
            return false;
        }
        let excludes = self.configured == Some(false);
        for literal in &self.admit {
            if excludes {
                if let Ok(parsed) = version::parse(literal, None) {
                    if is_prerelease(&parsed) {
                        continue;
                    }
                }
            }
            return false;
        }
        if self.bounds.is_empty() {
            return true;
        }
        excludes && ranges_are_prerelease_only(&self.bounds)
    }

    /// Mirror of `__repr__` body/tail (without the class-name wrapper).
    /// Returns `(body, tail, region)`: `tail` holds the `arbitrary`/`pre`
    /// flags, `region` the formatted opt-in region when present (the
    /// binding applies Python `repr()` to `body` and `region`).
    pub fn repr_body(&self) -> (String, String, Option<String>) {
        let mut parts = Vec::new();
        if !self.bounds.is_empty() {
            parts.push(format_intervals(&self.bounds));
        }
        if !self.admit.is_empty() {
            let mut lits: Vec<&String> = self.admit.iter().collect();
            lits.sort();
            parts.push(format!("{{{}}}", lits.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
        let body = if parts.is_empty() { "(empty)".to_string() } else { parts.join(" | ") };
        let mut body = body;
        if !self.reject.is_empty() {
            let mut lits: Vec<&String> = self.reject.iter().collect();
            lits.sort();
            body = format!("{body} \\ {{{}}}", lits.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }
        let mut tail = String::new();
        if self.admit_arbitrary {
            tail.push_str(" arbitrary");
        }
        if let Some(c) = self.configured {
            tail.push_str(if c { " pre=True" } else { " pre=False" });
        }
        let region = if self.pre_region.is_empty() {
            None
        } else {
            Some(format_intervals(&self.pre_region))
        };
        (body, tail, region)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOp {
    Intersection,
    Union,
    Difference,
}

/// Specifier-set relation operand check error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationError {
    NotASet,
    HasArbitrary,
}

/// Mirror of `_check_relation_operand` (type/=== validation only; the
/// prerelease-mismatch rule lives in `is_subset`... actually re-read:
/// "If either set uses === specifiers, or the two sets were given different
/// prereleases arguments (unset on one side counts as different)" — hmm, the
/// code only checks `isinstance` + `===`! The "different prereleases" rule is
/// enforced by `_check_policy_compat` inside to_range().is_subset (configured
/// mismatch → ValueError). Mirror faithfully: only type + === here.
pub fn check_relation_operand(has_arbitrary_self: bool, has_arbitrary_other: bool) -> Result<(), RelationError> {
    if has_arbitrary_self || has_arbitrary_other {
        return Err(RelationError::HasArbitrary);
    }
    Ok(())
}

/// Mirror of `_fast_match`: match without building a range.
/// Returns `None` when the range path must be used.
pub fn fast_match(op: &str, ver_str: &str, spec_version: &ParsedVersion, parsed: &ParsedVersion) -> Option<bool> {
    if ver_str.ends_with(".*") || parsed.local.is_some() {
        return None;
    }
    match op {
        ">=" => Some(version::cmp(parsed, spec_version) != Ordering::Less),
        "<=" => Some(version::cmp(parsed, spec_version) != Ordering::Greater),
        "==" => Some(version::cmp(parsed, spec_version) == Ordering::Equal),
        "!=" => Some(version::cmp(parsed, spec_version) != Ordering::Equal),
        "<" | ">" => {
            if parsed.epoch != spec_version.epoch
                || version::trim_release_leave_one(&parsed.release)
                    != version::trim_release_leave_one(&spec_version.release)
            {
                return Some(if op == "<" {
                    version::cmp(parsed, spec_version) == Ordering::Less
                } else {
                    version::cmp(parsed, spec_version) == Ordering::Greater
                });
            }
            None
        }
        _ => None,
    }
}

/// `SpecifierSet.contains` fast path over simple specifiers (mirror of the
/// per-spec `_fast_match` loop): `None` when the range path is needed,
/// `Some(false)` on the first failing spec, `Some(true)` when all answer.
pub fn set_fast_match(specs: &[(String, String, ParsedVersion)], parsed: &ParsedVersion) -> Option<bool> {
    for (op, ver_str, spec_version) in specs {
        match fast_match(op, ver_str, spec_version, parsed) {
            None => return None,
            Some(false) => return Some(false),
            Some(true) => {}
        }
    }
    Some(true)
}
