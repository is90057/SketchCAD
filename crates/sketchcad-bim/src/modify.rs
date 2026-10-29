//! Modify operations (修改): 2D affine transforms for rotate / scale / mirror, and the
//! drafting edits trim, extend, offset, fillet and chamfer.

use sketchcad_core::{Point2D, Vector2D};

use crate::elements::draft::{catmull_rom, DraftGeometry, HatchPattern};

const EPS: f64 = 1e-7;

/// 2D affine transform `p ↦ M·p + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    /// Columns of the linear part: images of +X and +Y
    pub x: Vector2D,
    pub y: Vector2D,
    pub t: Vector2D,
}

impl Transform2D {
    pub const IDENTITY: Self = Self { x: Vector2D::X, y: Vector2D::Y, t: Vector2D::ZERO };

    pub fn translation(d: Vector2D) -> Self {
        Self { t: d, ..Self::IDENTITY }
    }

    fn linear_about(center: Point2D, x: Vector2D, y: Vector2D) -> Self {
        // p' = L(p - c) + c  ⇒  t = c - L(c)
        let lc = x * center.x + y * center.y;
        Self { x, y, t: center - lc }
    }

    pub fn rotation(center: Point2D, degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::linear_about(center, Vector2D::new(c, s), Vector2D::new(-s, c))
    }

    pub fn scale(center: Point2D, factor: f64) -> Self {
        Self::linear_about(center, Vector2D::X * factor, Vector2D::Y * factor)
    }

    /// Reflection across the line through `a` and `b`.
    pub fn mirror(a: Point2D, b: Point2D) -> Self {
        let d = (b - a).normalize_or_zero();
        let (c2, s2) = (d.x * d.x - d.y * d.y, 2.0 * d.x * d.y);
        Self::linear_about(a, Vector2D::new(c2, s2), Vector2D::new(s2, -c2))
    }

    pub fn point(&self, p: Point2D) -> Point2D {
        self.x * p.x + self.y * p.y + self.t
    }

    pub fn vector(&self, v: Vector2D) -> Vector2D {
        self.x * v.x + self.y * v.y
    }

    pub fn det(&self) -> f64 {
        self.x.x * self.y.y - self.y.x * self.x.y
    }

    pub fn is_mirror(&self) -> bool {
        self.det() < 0.0
    }

    /// Uniform scale factor of the linear part.
    pub fn scale_factor(&self) -> f64 {
        self.det().abs().sqrt()
    }

    /// Direction (degrees) the +X axis is mapped to.
    pub fn x_angle_deg(&self) -> f64 {
        self.x.y.atan2(self.x.x).to_degrees()
    }

    /// New orientation of something facing `deg`.
    pub fn angle_deg(&self, deg: f64) -> f64 {
        let (s, c) = deg.to_radians().sin_cos();
        let v = self.vector(Vector2D::new(c, s));
        v.y.atan2(v.x).to_degrees()
    }

    /// True when axis-aligned rectangles stay axis aligned (rotations by multiples of 90°,
    /// mirrors across horizontal / vertical lines).
    pub fn keeps_axes(&self) -> bool {
        (self.x.x.abs() < 1e-9 || self.x.y.abs() < 1e-9) && (self.y.x.abs() < 1e-9 || self.y.y.abs() < 1e-9)
    }
}

impl DraftGeometry {
    /// The shape under `xf` (arcs stay arcs; a mirror reverses their sweep).
    pub fn transformed(&self, xf: &Transform2D) -> DraftGeometry {
        let map = |pts: &[Point2D]| pts.iter().map(|p| xf.point(*p)).collect::<Vec<_>>();
        let s = xf.scale_factor();
        match self {
            Self::Line { start, end } => Self::Line { start: xf.point(*start), end: xf.point(*end) },
            Self::Polyline { points, closed } => Self::Polyline { points: map(points), closed: *closed },
            Self::Spline { points } => Self::Spline { points: map(points) },
            Self::Circle { center, radius } => Self::Circle { center: xf.point(*center), radius: radius * s },
            Self::Arc { center, radius, start_deg, sweep_deg } => Self::Arc {
                center: xf.point(*center),
                radius: radius * s,
                start_deg: xf.angle_deg(*start_deg),
                sweep_deg: if xf.is_mirror() { -sweep_deg } else { *sweep_deg },
            },
            Self::Hatch { boundary, pattern, spacing, angle_deg } => {
                // Map the direction of the pattern lines (pattern base angle + user angle)
                let base = match pattern {
                    HatchPattern::Diagonal | HatchPattern::Cross | HatchPattern::Solid => 45.0,
                    _ => 0.0,
                };
                Self::Hatch {
                    boundary: map(boundary),
                    pattern: *pattern,
                    spacing: spacing * s,
                    angle_deg: (xf.angle_deg(base + angle_deg) - base + 180.0).rem_euclid(360.0) - 180.0,
                }
            }
        }
    }

    /// Straight segments of the drawn outline (cutting edges for trim / extend).
    pub fn segments(&self) -> Vec<(Point2D, Point2D)> {
        if matches!(self, Self::Hatch { .. }) {
            return Vec::new();
        }
        let (pts, closed) = self.outline();
        let n = if closed { pts.len() } else { pts.len().saturating_sub(1) };
        (0..n).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect()
    }
}

// ───────────────────────────── intersections ─────────────────────────────

/// Parameters (along `a→b`, `c→d`) where two segments cross; `None` if they do not.
fn segment_hit(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> Option<(f64, f64)> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((-EPS..=1.0 + EPS).contains(&u)).then_some((t, u))
}

/// Angles (degrees) where a circle meets a segment.
fn circle_hits(center: Point2D, radius: f64, a: Point2D, b: Point2D) -> Vec<f64> {
    let d = b - a;
    let f = a - center;
    let (qa, qb, qc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - radius * radius);
    let disc = qb * qb - 4.0 * qa * qc;
    if qa < 1e-12 || disc < 0.0 {
        return Vec::new();
    }
    let root = disc.sqrt();
    [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)]
        .into_iter()
        .filter(|u| (-EPS..=1.0 + EPS).contains(u))
        .map(|u| {
            let p = a + d * u - center;
            p.y.atan2(p.x).to_degrees()
        })
        .collect()
}

/// Cumulative arc length of a polyline's vertices.
fn cumulative(points: &[Point2D]) -> Vec<f64> {
    let mut acc = vec![0.0];
    for w in points.windows(2) {
        acc.push(acc.last().unwrap() + w[0].distance(w[1]));
    }
    acc
}

/// Sub-polyline between arc lengths `s0 < s1`.
fn sub_polyline(points: &[Point2D], cum: &[f64], s0: f64, s1: f64) -> Vec<Point2D> {
    let at = |s: f64| -> Point2D {
        let i = cum.partition_point(|&c| c <= s).clamp(1, points.len() - 1);
        let (c0, c1) = (cum[i - 1], cum[i]);
        let t = if c1 - c0 > 1e-12 { (s - c0) / (c1 - c0) } else { 0.0 };
        points[i - 1] + (points[i] - points[i - 1]) * t
    };
    let mut out = vec![at(s0)];
    out.extend(points.iter().zip(cum).filter(|(_, &c)| c > s0 + EPS && c < s1 - EPS).map(|(p, _)| *p));
    out.push(at(s1));
    out
}

/// Closest arc-length parameter on a polyline to `p`.
fn project_on_polyline(points: &[Point2D], cum: &[f64], p: Point2D) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for i in 0..points.len().saturating_sub(1) {
        let (a, b) = (points[i], points[i + 1]);
        let ab = b - a;
        let t = if ab.length_squared() > 1e-12 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let d = p.distance(a + ab * t);
        if d < best.0 {
            best = (d, cum[i] + (cum[i + 1] - cum[i]) * t);
        }
    }
    best.1
}

/// Interval of `cuts` (sorted, within (lo, hi)) around `x`, or `None` without cuts.
fn enclosing(cuts: &[f64], lo: f64, hi: f64, x: f64) -> Option<(f64, f64)> {
    if cuts.is_empty() {
        return None;
    }
    let a = cuts.iter().copied().filter(|&c| c < x).fold(lo, f64::max);
    let b = cuts.iter().copied().filter(|&c| c > x).fold(hi, f64::min);
    Some((a, b))
}

// ───────────────────────────── trim / extend ─────────────────────────────

impl DraftGeometry {
    /// 修剪: removes the piece of the shape around `pick` bounded by the cutting edges.
    /// Returns the remaining pieces (possibly none), or `None` when no edge crosses it.
    pub fn trim(&self, cutters: &[(Point2D, Point2D)], pick: Point2D) -> Option<Vec<DraftGeometry>> {
        match self {
            Self::Line { start, end } => {
                let len = start.distance(*end);
                let pts = [*start, *end];
                let cum = [0.0, len];
                let cuts = line_cuts(*start, *end, cutters, len);
                let (a, b) = enclosing(&cuts, 0.0, len, project_on_polyline(&pts, &cum, pick))?;
                let mut out = Vec::new();
                for (s0, s1) in [(0.0, a), (b, len)] {
                    if s1 - s0 > 1e-3 {
                        let seg = sub_polyline(&pts, &cum, s0, s1);
                        out.push(Self::Line { start: seg[0], end: seg[seg.len() - 1] });
                    }
                }
                Some(out)
            }
            Self::Polyline { points, closed: false } | Self::Spline { points } => {
                let pts = if matches!(self, Self::Spline { .. }) { catmull_rom(points, 16) } else { points.clone() };
                let cum = cumulative(&pts);
                let total = *cum.last()?;
                let mut cuts = Vec::new();
                for i in 0..pts.len().saturating_sub(1) {
                    for c in line_cuts(pts[i], pts[i + 1], cutters, pts[i].distance(pts[i + 1])) {
                        cuts.push(cum[i] + c);
                    }
                }
                cuts.retain(|&c| c > 1e-3 && c < total - 1e-3);
                let (a, b) = enclosing(&cuts, 0.0, total, project_on_polyline(&pts, &cum, pick))?;
                Some(
                    [(0.0, a), (b, total)]
                        .into_iter()
                        .filter(|(s0, s1)| s1 - s0 > 1e-3)
                        .map(|(s0, s1)| Self::Polyline { points: sub_polyline(&pts, &cum, s0, s1), closed: false })
                        .collect(),
                )
            }
            Self::Polyline { points, closed: true } => {
                // Open the ring at the pick, then the removed piece is bounded by the first
                // and last cut along it
                let mut ring = points.clone();
                ring.push(points[0]);
                let cum = cumulative(&ring);
                let total = *cum.last()?;
                let mut cuts = Vec::new();
                for i in 0..ring.len() - 1 {
                    for c in line_cuts(ring[i], ring[i + 1], cutters, ring[i].distance(ring[i + 1])) {
                        cuts.push(cum[i] + c);
                    }
                }
                let x = project_on_polyline(&ring, &cum, pick);
                ring_trim(&cuts, total, x).map(|(keep0, keep1)| {
                    // Keep [keep0, keep1] going forward around the ring (may wrap)
                    let doubled: Vec<Point2D> = ring.iter().chain(ring[1..].iter()).copied().collect();
                    let cum2 = cumulative(&doubled);
                    let (s0, s1) = (keep0, if keep1 <= keep0 { keep1 + total } else { keep1 });
                    vec![Self::Polyline { points: sub_polyline(&doubled, &cum2, s0, s1), closed: false }]
                })
            }
            Self::Circle { center, radius } => {
                let cuts: Vec<f64> = cutters.iter().flat_map(|(a, b)| circle_hits(*center, *radius, *a, *b)).map(|d| d.rem_euclid(360.0)).collect();
                let x = (pick.y - center.y).atan2(pick.x - center.x).to_degrees().rem_euclid(360.0);
                ring_trim(&cuts, 360.0, x).map(|(k0, k1)| {
                    vec![Self::Arc { center: *center, radius: *radius, start_deg: k0, sweep_deg: (k1 - k0).rem_euclid(360.0) }]
                })
            }
            Self::Arc { center, radius, start_deg, sweep_deg } => {
                let along = |deg: f64| ((deg - start_deg) * sweep_deg.signum()).rem_euclid(360.0);
                let span = sweep_deg.abs();
                let cuts: Vec<f64> = cutters
                    .iter()
                    .flat_map(|(a, b)| circle_hits(*center, *radius, *a, *b))
                    .map(along)
                    .filter(|&s| s > 1e-6 && s < span - 1e-6)
                    .collect();
                let x = along((pick.y - center.y).atan2(pick.x - center.x).to_degrees()).min(span);
                let (a, b) = enclosing(&cuts, 0.0, span, x)?;
                let dir = sweep_deg.signum();
                Some(
                    [(0.0, a), (b, span)]
                        .into_iter()
                        .filter(|(s0, s1)| s1 - s0 > 1e-6)
                        .map(|(s0, s1)| Self::Arc { center: *center, radius: *radius, start_deg: start_deg + dir * s0, sweep_deg: dir * (s1 - s0) })
                        .collect(),
                )
            }
            Self::Hatch { .. } => None,
        }
    }

    /// 延伸: lengthens the end nearest `pick` to the closest boundary edge in its direction.
    pub fn extend(&self, boundaries: &[(Point2D, Point2D)], pick: Point2D) -> Option<DraftGeometry> {
        let ray = |from: Point2D, dir: Vector2D| -> Option<Point2D> {
            boundaries
                .iter()
                .filter_map(|(c, d)| segment_hit(from, from + dir, *c, *d).map(|(t, _)| t))
                .filter(|&t| t > 1e-6)
                .min_by(f64::total_cmp)
                .map(|t| from + dir * t)
        };
        match self {
            Self::Line { start, end } => {
                if pick.distance(*end) <= pick.distance(*start) {
                    ray(*end, *end - *start).map(|p| Self::Line { start: *start, end: p })
                } else {
                    ray(*start, *start - *end).map(|p| Self::Line { start: p, end: *end })
                }
            }
            Self::Polyline { points, closed: false } if points.len() >= 2 => {
                let mut pts = points.clone();
                let n = pts.len();
                if pick.distance(pts[n - 1]) <= pick.distance(pts[0]) {
                    pts[n - 1] = ray(pts[n - 1], pts[n - 1] - pts[n - 2])?;
                } else {
                    pts[0] = ray(pts[0], pts[0] - pts[1])?;
                }
                Some(Self::Polyline { points: pts, closed: false })
            }
            Self::Arc { center, radius, start_deg, sweep_deg } => {
                let end_deg = start_deg + sweep_deg;
                let point_at = |deg: f64| *center + Vector2D::new(deg.to_radians().cos(), deg.to_radians().sin()) * *radius;
                let at_end = pick.distance(point_at(end_deg)) <= pick.distance(point_at(*start_deg));
                let dir = sweep_deg.signum();
                // Angular distance from the chosen end, moving away from the arc
                let (from, outward) = if at_end { (end_deg, dir) } else { (*start_deg, -dir) };
                let gap = 360.0 - sweep_deg.abs();
                let grow = boundaries
                    .iter()
                    .flat_map(|(a, b)| circle_hits(*center, *radius, *a, *b))
                    .map(|deg| ((deg - from) * outward).rem_euclid(360.0))
                    .filter(|&g| g > 1e-6 && g < gap - 1e-6)
                    .min_by(f64::total_cmp)?;
                Some(if at_end {
                    Self::Arc { center: *center, radius: *radius, start_deg: *start_deg, sweep_deg: sweep_deg + dir * grow }
                } else {
                    Self::Arc { center: *center, radius: *radius, start_deg: start_deg - dir * grow, sweep_deg: sweep_deg + dir * grow }
                })
            }
            _ => None,
        }
    }

    // ───────────────────────────── offset ─────────────────────────────

    /// 偏移: parallel copy `distance` away, on the side of `side`.
    pub fn offset(&self, distance: f64, side: Point2D) -> Option<DraftGeometry> {
        match self {
            Self::Line { start, end } => {
                let n = (*end - *start).perp().normalize_or_zero();
                let sign = if (side - *start).dot(n) >= 0.0 { 1.0 } else { -1.0 };
                let d = n * sign * distance;
                Some(Self::Line { start: *start + d, end: *end + d })
            }
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                let r = if side.distance(*center) < *radius { radius - distance } else { radius + distance };
                (r > 1e-6).then(|| match self {
                    Self::Arc { start_deg, sweep_deg, .. } => Self::Arc { center: *center, radius: r, start_deg: *start_deg, sweep_deg: *sweep_deg },
                    _ => Self::Circle { center: *center, radius: r },
                })
            }
            Self::Polyline { points, closed } => {
                let sign = polyline_side(points, *closed, side);
                offset_polyline(points, *closed, distance * sign).map(|p| Self::Polyline { points: p, closed: *closed })
            }
            Self::Spline { points } => {
                // Offset the points through which the curve passes along the curve normal
                let curve = catmull_rom(points, 16);
                let sign = polyline_side(&curve, false, side);
                let n = points.len();
                let shifted = (0..n)
                    .map(|i| {
                        let tangent = points[(i + 1).min(n - 1)] - points[i.saturating_sub(1)];
                        points[i] + tangent.perp().normalize_or_zero() * distance * sign
                    })
                    .collect();
                Some(Self::Spline { points: shifted })
            }
            Self::Hatch { .. } => None,
        }
    }
}

fn line_cuts(a: Point2D, b: Point2D, cutters: &[(Point2D, Point2D)], len: f64) -> Vec<f64> {
    cutters
        .iter()
        .filter_map(|(c, d)| segment_hit(a, b, *c, *d))
        .map(|(t, _)| t * len)
        .filter(|&s| s > 1e-3 && s < len - 1e-3)
        .collect()
}

/// For a closed parameter ring `[0, total)`: the kept piece `(from, to)` after removing the
/// stretch around `x` between its neighbouring cuts. Needs at least two cuts.
fn ring_trim(cuts: &[f64], total: f64, x: f64) -> Option<(f64, f64)> {
    let mut cuts: Vec<f64> = cuts.iter().map(|c| c.rem_euclid(total)).collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if cuts.len() < 2 {
        return None;
    }
    let after = cuts.iter().copied().find(|&c| c > x).unwrap_or(cuts[0]);
    let before = cuts.iter().copied().rev().find(|&c| c < x).unwrap_or(cuts[cuts.len() - 1]);
    Some((after, before))
}

/// +1 when `side` lies left of the polyline (counter-clockwise normal), −1 when right.
fn polyline_side(points: &[Point2D], closed: bool, side: Point2D) -> f64 {
    let n = if closed { points.len() } else { points.len().saturating_sub(1) };
    let mut best = (f64::INFINITY, 1.0);
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        let ab = b - a;
        let t = if ab.length_squared() > 1e-12 { ((side - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let d = side.distance(a + ab * t);
        if d < best.0 {
            best = (d, if ab.perp_dot(side - a) >= 0.0 { 1.0 } else { -1.0 });
        }
    }
    best.1
}

/// Mitred offset of a polyline; positive `d` = left side.
fn offset_polyline(points: &[Point2D], closed: bool, d: f64) -> Option<Vec<Point2D>> {
    let n = points.len();
    if n < 2 {
        return None;
    }
    let seg_count = if closed { n } else { n - 1 };
    let lines: Vec<(Point2D, Vector2D)> = (0..seg_count)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let dir = (b - a).normalize_or_zero();
            (a + dir.perp() * d, dir)
        })
        .collect();
    let meet = |(p, u): (Point2D, Vector2D), (q, v): (Point2D, Vector2D)| -> Point2D {
        let denom = u.perp_dot(v);
        if denom.abs() < 1e-9 {
            q
        } else {
            p + u * ((q - p).perp_dot(v) / denom)
        }
    };
    let out = (0..n)
        .map(|i| {
            if closed {
                meet(lines[(i + seg_count - 1) % seg_count], lines[i % seg_count])
            } else if i == 0 {
                lines[0].0
            } else if i == n - 1 {
                points[n - 1] + lines[seg_count - 1].1.perp() * d
            } else {
                meet(lines[i - 1], lines[i])
            }
        })
        .collect();
    Some(out)
}

// ───────────────────────────── fillet / chamfer ─────────────────────────────

/// Corner treatment for fillet / chamfer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Corner {
    /// 圓角 with this radius (0 = sharp corner: trim / extend both lines to meet)
    Fillet(f64),
    /// 導角 cutting back this far along the first and second line
    Chamfer(f64, f64),
}

/// Joins two lines at their (extended) intersection. Each line keeps the part on the side
/// it was picked; returns the two new lines and the arc / chamfer segment between them.
pub fn corner_lines(
    l1: (Point2D, Point2D),
    pick1: Point2D,
    l2: (Point2D, Point2D),
    pick2: Point2D,
    corner: Corner,
) -> Option<(DraftGeometry, DraftGeometry, Option<DraftGeometry>)> {
    let (u, v) = (l1.1 - l1.0, l2.1 - l2.0);
    let denom = u.perp_dot(v);
    if denom.abs() < 1e-9 {
        return None; // parallel
    }
    let x = l1.0 + u * ((l2.0 - l1.0).perp_dot(v) / denom);
    // Far end of each line on the picked side of the corner
    let keep = |(a, b): (Point2D, Point2D), pick: Point2D| -> Option<(Point2D, Vector2D)> {
        let dir = (b - a).normalize_or_zero();
        let side = (pick - x).dot(dir).signum();
        let far = if (a - x).dot(dir) * side > (b - x).dot(dir) * side { a } else { b };
        let len = (far - x).dot(dir) * side;
        (len > 1e-6).then_some((far, dir * side))
    };
    let (e1, u1) = keep(l1, pick1)?;
    let (e2, u2) = keep(l2, pick2)?;
    let (len1, len2) = (e1.distance(x), e2.distance(x));
    match corner {
        Corner::Fillet(r) if r <= 1e-9 => Some((DraftGeometry::Line { start: x, end: e1 }, DraftGeometry::Line { start: x, end: e2 }, None)),
        Corner::Fillet(r) => {
            let theta = u1.angle_to(u2).abs();
            if theta < 1e-6 || (std::f64::consts::PI - theta) < 1e-6 {
                return None;
            }
            let tangent = r / (theta * 0.5).tan();
            if tangent >= len1 || tangent >= len2 {
                return None; // radius too large for these lines
            }
            let (t1, t2) = (x + u1 * tangent, x + u2 * tangent);
            let center = x + (u1 + u2).normalize() * (r / (theta * 0.5).sin());
            let mid = center + (x - center).normalize() * r;
            let arc = DraftGeometry::arc_through(t1, mid, t2)?;
            Some((DraftGeometry::Line { start: t1, end: e1 }, DraftGeometry::Line { start: t2, end: e2 }, Some(arc)))
        }
        Corner::Chamfer(d1, d2) => {
            if d1 >= len1 || d2 >= len2 {
                return None;
            }
            let (p1, p2) = (x + u1 * d1, x + u2 * d2);
            let bevel = (p1.distance(p2) > 1e-6).then_some(DraftGeometry::Line { start: p1, end: p2 });
            Some((DraftGeometry::Line { start: p1, end: e1 }, DraftGeometry::Line { start: p2, end: e2 }, bevel))
        }
    }
}

/// Rounds or bevels the polyline vertex nearest `pick` (arcs become short straight segments).
pub fn corner_polyline(points: &[Point2D], closed: bool, pick: Point2D, corner: Corner) -> Option<Vec<Point2D>> {
    let n = points.len();
    let candidates: Vec<usize> = if closed { (0..n).collect() } else { (1..n.saturating_sub(1)).collect() };
    let i = candidates.into_iter().min_by(|&a, &b| points[a].distance(pick).total_cmp(&points[b].distance(pick)))?;
    let (prev, x, next) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
    let (u1, u2) = ((prev - x).normalize_or_zero(), (next - x).normalize_or_zero());
    let (len1, len2) = (prev.distance(x), next.distance(x));
    let replacement: Vec<Point2D> = match corner {
        Corner::Fillet(r) if r <= 1e-9 => return Some(points.to_vec()),
        Corner::Fillet(r) => {
            let theta = u1.angle_to(u2).abs();
            if theta < 1e-6 || (std::f64::consts::PI - theta) < 1e-6 {
                return None;
            }
            let tangent = r / (theta * 0.5).tan();
            if tangent >= len1 || tangent >= len2 {
                return None;
            }
            let (t1, t2) = (x + u1 * tangent, x + u2 * tangent);
            let center = x + (u1 + u2).normalize() * (r / (theta * 0.5).sin());
            let (a1, a2) = ((t1 - center).to_angle(), (t2 - center).to_angle());
            let mut sweep = a2 - a1;
            while sweep > std::f64::consts::PI {
                sweep -= std::f64::consts::TAU;
            }
            while sweep < -std::f64::consts::PI {
                sweep += std::f64::consts::TAU;
            }
            let steps = ((sweep.abs().to_degrees() / 10.0).ceil() as usize).max(2);
            (0..=steps)
                .map(|k| {
                    let a = a1 + sweep * k as f64 / steps as f64;
                    center + Vector2D::new(a.cos(), a.sin()) * r
                })
                .collect()
        }
        Corner::Chamfer(d1, d2) => {
            if d1 >= len1 || d2 >= len2 {
                return None;
            }
            vec![x + u1 * d1, x + u2 * d2]
        }
    };
    let mut out = points[..i].to_vec();
    out.extend(replacement);
    out.extend_from_slice(&points[i + 1..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2D {
        Point2D::new(x, y)
    }

    fn close(a: Point2D, b: Point2D) -> bool {
        a.distance(b) < 1e-6
    }

    fn is_line(g: &DraftGeometry, start: Point2D, end: Point2D) -> bool {
        matches!(g, DraftGeometry::Line { start: s, end: e } if close(*s, start) && close(*e, end))
    }

    #[test]
    fn transforms_rotate_scale_and_mirror_about_a_point() {
        let r = Transform2D::rotation(p(100.0, 0.0), 90.0);
        assert!(close(r.point(p(200.0, 0.0)), p(100.0, 100.0)));
        assert!((r.angle_deg(0.0) - 90.0).abs() < 1e-9 && !r.is_mirror());
        let s = Transform2D::scale(p(10.0, 10.0), 2.0);
        assert!(close(s.point(p(20.0, 10.0)), p(30.0, 10.0)) && (s.scale_factor() - 2.0).abs() < 1e-9);
        let m = Transform2D::mirror(p(0.0, 0.0), p(0.0, 10.0));
        assert!(close(m.point(p(5.0, 3.0)), p(-5.0, 3.0)) && m.is_mirror());
        assert!(m.keeps_axes() && !Transform2D::rotation(p(0.0, 0.0), 30.0).keeps_axes());

        // A mirrored arc keeps its end points (mirrored) and reverses direction
        let arc = DraftGeometry::arc_through(p(1000.0, 0.0), p(0.0, 1000.0), p(-1000.0, 0.0)).unwrap();
        let mirrored = arc.transformed(&Transform2D::mirror(p(0.0, 0.0), p(1.0, 0.0)));
        let (a, b) = (arc.outline().0, mirrored.outline().0);
        assert!(close(b[0], p(1000.0, 0.0)) && close(*b.last().unwrap(), p(-1000.0, 0.0)));
        assert!(close(b[b.len() / 2], p(a[a.len() / 2].x, -a[a.len() / 2].y)));
    }

    #[test]
    fn trimming_a_line_between_two_cutters_keeps_both_ends() {
        let line = DraftGeometry::Line { start: p(0.0, 0.0), end: p(1000.0, 0.0) };
        let cutters = [(p(300.0, -100.0), p(300.0, 100.0)), (p(700.0, -100.0), p(700.0, 100.0))];
        let pieces = line.trim(&cutters, p(500.0, 0.0)).unwrap();
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], DraftGeometry::Line { start: p(0.0, 0.0), end: p(300.0, 0.0) });
        assert_eq!(pieces[1], DraftGeometry::Line { start: p(700.0, 0.0), end: p(1000.0, 0.0) });
        // Trimming an end piece shortens the line
        assert_eq!(line.trim(&cutters, p(900.0, 0.0)).unwrap(), vec![DraftGeometry::Line { start: p(0.0, 0.0), end: p(700.0, 0.0) }]);
        assert!(line.trim(&[], p(500.0, 0.0)).is_none(), "nothing crosses it");
    }

    #[test]
    fn trimming_circles_and_closed_polylines_opens_them() {
        let circle = DraftGeometry::Circle { center: p(0.0, 0.0), radius: 100.0 };
        let cutter = [(p(-200.0, 0.0), p(200.0, 0.0))];
        let Some(pieces) = circle.trim(&cutter, p(0.0, 100.0)) else { panic!() };
        let DraftGeometry::Arc { sweep_deg, .. } = pieces[0] else { panic!("{pieces:?}") };
        assert!((sweep_deg - 180.0).abs() < 1e-6);
        let (pts, _) = pieces[0].outline();
        assert!(pts.iter().all(|q| q.y <= 1e-6), "the upper half was removed");

        let square = DraftGeometry::Polyline { points: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)], closed: true };
        let pieces = square.trim(&[(p(50.0, -10.0), p(50.0, 110.0))], p(0.0, 50.0)).unwrap();
        let DraftGeometry::Polyline { points, closed: false } = &pieces[0] else { panic!() };
        assert!(points.iter().all(|q| q.x >= 50.0 - 1e-6), "left half removed: {points:?}");
        assert_eq!(points.len(), 4);
    }

    #[test]
    fn extend_reaches_the_nearest_boundary() {
        let line = DraftGeometry::Line { start: p(0.0, 0.0), end: p(100.0, 0.0) };
        let walls = [(p(500.0, -50.0), p(500.0, 50.0)), (p(300.0, -50.0), p(300.0, 50.0))];
        assert_eq!(line.extend(&walls, p(90.0, 0.0)), Some(DraftGeometry::Line { start: p(0.0, 0.0), end: p(300.0, 0.0) }));
        assert!(line.extend(&walls, p(10.0, 0.0)).is_none(), "nothing behind the start");
        let arc = DraftGeometry::Arc { center: p(0.0, 0.0), radius: 100.0, start_deg: 0.0, sweep_deg: 45.0 };
        let Some(DraftGeometry::Arc { sweep_deg, .. }) = arc.extend(&[(p(-200.0, 0.0), p(0.0, 0.0)), (p(0.0, 0.0), p(0.0, 200.0))], p(70.0, 70.0)) else { panic!() };
        assert!((sweep_deg - 90.0).abs() < 1e-6, "{sweep_deg}");
    }

    #[test]
    fn offsets_follow_the_picked_side() {
        let line = DraftGeometry::Line { start: p(0.0, 0.0), end: p(100.0, 0.0) };
        assert_eq!(line.offset(20.0, p(50.0, -5.0)), Some(DraftGeometry::Line { start: p(0.0, -20.0), end: p(100.0, -20.0) }));
        let circle = DraftGeometry::Circle { center: p(0.0, 0.0), radius: 100.0 };
        assert_eq!(circle.offset(30.0, p(10.0, 0.0)), Some(DraftGeometry::Circle { center: p(0.0, 0.0), radius: 70.0 }));
        assert!(circle.offset(150.0, p(10.0, 0.0)).is_none(), "cannot shrink below zero");
        let square = DraftGeometry::Polyline { points: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)], closed: true };
        let Some(DraftGeometry::Polyline { points, .. }) = square.offset(10.0, p(200.0, 50.0)) else { panic!() };
        assert!(close(points[0], p(-10.0, -10.0)) && close(points[2], p(110.0, 110.0)), "grown outward: {points:?}");
    }

    #[test]
    fn fillet_and_chamfer_join_two_lines() {
        let l1 = (p(0.0, 0.0), p(1000.0, 0.0));
        let l2 = (p(1200.0, 200.0), p(1200.0, 1000.0));
        // Radius 0: both lines meet at (1200, 0)
        let (a, b, extra) = corner_lines(l1, p(100.0, 0.0), l2, p(1200.0, 900.0), Corner::Fillet(0.0)).unwrap();
        assert!(is_line(&a, p(1200.0, 0.0), p(0.0, 0.0)) && is_line(&b, p(1200.0, 0.0), p(1200.0, 1000.0)));
        assert!(extra.is_none());
        let (a, b, arc) = corner_lines(l1, p(100.0, 0.0), l2, p(1200.0, 900.0), Corner::Fillet(200.0)).unwrap();
        assert!(is_line(&a, p(1000.0, 0.0), p(0.0, 0.0)) && is_line(&b, p(1200.0, 200.0), p(1200.0, 1000.0)));
        let Some(DraftGeometry::Arc { center, radius, .. }) = arc else { panic!() };
        assert!(close(center, p(1000.0, 200.0)) && (radius - 200.0).abs() < 1e-6);
        let (_, _, bevel) = corner_lines(l1, p(100.0, 0.0), l2, p(1200.0, 900.0), Corner::Chamfer(100.0, 300.0)).unwrap();
        assert!(bevel.is_some_and(|g| is_line(&g, p(1100.0, 0.0), p(1200.0, 300.0))));
        assert!(corner_lines(l1, p(100.0, 0.0), (p(0.0, 50.0), p(10.0, 50.0)), p(5.0, 50.0), Corner::Fillet(10.0)).is_none(), "parallel");

        let ell = vec![p(0.0, 0.0), p(1000.0, 0.0), p(1000.0, 1000.0)];
        let rounded = corner_polyline(&ell, false, p(990.0, 10.0), Corner::Fillet(100.0)).unwrap();
        assert!(rounded.len() > 4 && close(rounded[1], p(900.0, 0.0)) && close(rounded[rounded.len() - 2], p(1000.0, 100.0)));
        let bevelled = corner_polyline(&ell, false, p(990.0, 10.0), Corner::Chamfer(100.0, 100.0)).unwrap();
        let expected = [p(0.0, 0.0), p(900.0, 0.0), p(1000.0, 100.0), p(1000.0, 1000.0)];
        assert!(bevelled.len() == 4 && bevelled.iter().zip(expected).all(|(a, b)| close(*a, b)));
    }
}
