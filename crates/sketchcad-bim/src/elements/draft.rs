//! 2D drafting entities (繪製): lines, polylines, circles, arcs, splines and hatches drawn
//! on a floor plan. They are plan annotations without 3D geometry or cost.

use crate::cost::QuantityTakeoff;
use crate::element::{BimElement, ElementKind};
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Hatch fill pattern (填充圖樣).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HatchPattern {
    /// ANSI31 45° lines (general section / masonry cut)
    #[default]
    Diagonal,
    /// ANSI37 crossed 45° lines
    Cross,
    Horizontal,
    /// Square grid (tiles)
    Grid,
    /// Running-bond brick courses
    Brick,
    /// Dense lines that read as a solid fill
    Solid,
}

impl HatchPattern {
    pub const ALL: [Self; 6] = [Self::Diagonal, Self::Cross, Self::Horizontal, Self::Grid, Self::Brick, Self::Solid];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Diagonal => "斜線 (ANSI31)",
            Self::Cross => "交叉線 (ANSI37)",
            Self::Horizontal => "水平線",
            Self::Grid => "方格 (磁磚)",
            Self::Brick => "磚紋 (Brick)",
            Self::Solid => "實心填滿 (Solid)",
        }
    }
}

/// Geometry of a drafting entity, in plan coordinates (mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DraftGeometry {
    Line { start: Point2D, end: Point2D },
    /// 聚合線: connected straight segments
    Polyline { points: Vec<Point2D>, closed: bool },
    Circle { center: Point2D, radius: f64 },
    /// Arc from `start_deg` sweeping `sweep_deg` (positive = counter-clockwise)
    Arc { center: Point2D, radius: f64, start_deg: f64, sweep_deg: f64 },
    /// 多段線 / 雲形線: smooth curve through the points (centripetal Catmull-Rom)
    Spline { points: Vec<Point2D> },
    Hatch { boundary: Vec<Point2D>, pattern: HatchPattern, spacing: f64, angle_deg: f64 },
}

impl DraftGeometry {
    /// Arc through three points (start, a point on the arc, end); `None` if collinear.
    pub fn arc_through(a: Point2D, b: Point2D, c: Point2D) -> Option<Self> {
        let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
        if d.abs() < 1e-6 {
            return None;
        }
        let sq = |p: Point2D| p.length_squared();
        let center = Point2D::new(
            (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d,
            (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d,
        );
        let angle = |p: Point2D| (p.y - center.y).atan2(p.x - center.x).to_degrees();
        let (sa, sb, sc) = (angle(a), angle(b), angle(c));
        let ccw = |from: f64, to: f64| (to - from).rem_euclid(360.0);
        // Counter-clockwise when the middle point is met before the end going CCW
        let sweep = if ccw(sa, sb) <= ccw(sa, sc) { ccw(sa, sc) } else { ccw(sa, sc) - 360.0 };
        Some(Self::Arc { center, radius: center.distance(a), start_deg: sa, sweep_deg: sweep })
    }

    /// Display name (中文).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Line { .. } => "直線 (Line)",
            Self::Polyline { .. } => "聚合線 (Polyline)",
            Self::Circle { .. } => "圓 (Circle)",
            Self::Arc { .. } => "弧 (Arc)",
            Self::Spline { .. } => "多段線 / 雲形線 (Spline)",
            Self::Hatch { .. } => "填充線 (Hatch)",
        }
    }

    /// Points of the shape's outline (circles and arcs tessellated, splines sampled).
    pub fn outline(&self) -> (Vec<Point2D>, bool) {
        match self {
            Self::Line { start, end } => (vec![*start, *end], false),
            Self::Polyline { points, closed } => (points.clone(), *closed),
            Self::Circle { center, radius } => (arc_points(*center, *radius, 0.0, 360.0, 64), true),
            Self::Arc { center, radius, start_deg, sweep_deg } => {
                let segments = ((sweep_deg.abs() / 6.0).ceil() as usize).max(4);
                (arc_points(*center, *radius, *start_deg, *sweep_deg, segments), false)
            }
            Self::Spline { points } => (catmull_rom(points, 16), false),
            Self::Hatch { boundary, .. } => (boundary.clone(), true),
        }
    }

    /// Closed region a hatch can fill (closed polylines and circles).
    pub fn closed_region(&self) -> Option<Vec<Point2D>> {
        match self {
            Self::Polyline { points, closed: true } if points.len() >= 3 => Some(points.clone()),
            Self::Circle { .. } | Self::Hatch { .. } => Some(self.outline().0),
            _ => None,
        }
    }

    fn translate(&mut self, d: Vector2D) {
        match self {
            Self::Line { start, end } => {
                *start += d;
                *end += d;
            }
            Self::Polyline { points, .. } | Self::Spline { points } | Self::Hatch { boundary: points, .. } => {
                for p in points {
                    *p += d;
                }
            }
            Self::Circle { center, .. } | Self::Arc { center, .. } => *center += d,
        }
    }
}

/// `segments + 1` points along a circular arc.
fn arc_points(center: Point2D, radius: f64, start_deg: f64, sweep_deg: f64, segments: usize) -> Vec<Point2D> {
    let full = sweep_deg.abs() >= 360.0 - 1e-9;
    let count = if full { segments } else { segments + 1 };
    (0..count)
        .map(|i| {
            let a = (start_deg + sweep_deg * i as f64 / segments as f64).to_radians();
            center + Vector2D::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// Centripetal Catmull-Rom curve through `points`, `samples` points per span.
pub fn catmull_rom(points: &[Point2D], samples: usize) -> Vec<Point2D> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let n = points.len();
    let at = |i: isize| -> Point2D {
        if i < 0 {
            points[0] * 2.0 - points[1]
        } else if i as usize >= n {
            points[n - 1] * 2.0 - points[n - 2]
        } else {
            points[i as usize]
        }
    };
    let mut out = vec![points[0]];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        let knot = |a: Point2D, b: Point2D| a.distance(b).sqrt().max(1e-6);
        let (t1, t2, t3) = (knot(p0, p1), knot(p1, p2), knot(p2, p3));
        let (t0, t1, t2, t3) = (0.0, t1, t1 + t2, t1 + t2 + t3);
        for s in 1..=samples {
            let t = t1 + (t2 - t1) * s as f64 / samples as f64;
            let lerp = |a: Point2D, b: Point2D, ta: f64, tb: f64| a * ((tb - t) / (tb - ta)) + b * ((t - ta) / (tb - ta));
            let a1 = lerp(p0, p1, t0, t1);
            let a2 = lerp(p1, p2, t1, t2);
            let a3 = lerp(p2, p3, t2, t3);
            let b1 = lerp(a1, a2, t0, t2);
            let b2 = lerp(a2, a3, t1, t3);
            out.push(lerp(b1, b2, t1, t2));
        }
    }
    out
}

/// Segments of the infinite lines `{p : p·n = k·spacing}` (direction `angle_deg`) that lie
/// inside `polygon` (even-odd rule).
fn line_family(polygon: &[Point2D], angle_deg: f64, spacing: f64, shift: f64) -> Vec<(Point2D, Point2D)> {
    let a = angle_deg.to_radians();
    let (dir, normal) = (Vector2D::new(a.cos(), a.sin()), Vector2D::new(-a.sin(), a.cos()));
    let (lo, hi) = polygon.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.dot(normal)), hi.max(p.dot(normal))));
    let mut segments = Vec::new();
    if spacing <= 0.0 || polygon.len() < 3 || (hi - lo) / spacing > 20_000.0 {
        return segments;
    }
    // Strictly inside: a line along the boundary itself would just retrace the outline
    let mut k = ((lo - shift) / spacing + 1e-9).floor() + 1.0;
    while k * spacing + shift < hi - 1e-9 {
        let t = k * spacing + shift;
        let mut hits: Vec<f64> = Vec::new();
        for i in 0..polygon.len() {
            let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            let (dp, dq) = (p.dot(normal) - t, q.dot(normal) - t);
            if (dp > 0.0) != (dq > 0.0) {
                let x = p + (q - p) * (dp / (dp - dq));
                hits.push(x.dot(dir));
            }
        }
        hits.sort_by(f64::total_cmp);
        for pair in hits.chunks_exact(2) {
            segments.push((normal * t + dir * pair[0], normal * t + dir * pair[1]));
        }
        k += 1.0;
    }
    segments
}

/// Pattern lines of a hatch, clipped to its boundary.
pub fn hatch_lines(boundary: &[Point2D], pattern: HatchPattern, spacing: f64, angle_deg: f64) -> Vec<(Point2D, Point2D)> {
    match pattern {
        HatchPattern::Diagonal => line_family(boundary, 45.0 + angle_deg, spacing, 0.0),
        HatchPattern::Cross => {
            let mut lines = line_family(boundary, 45.0 + angle_deg, spacing, 0.0);
            lines.extend(line_family(boundary, 135.0 + angle_deg, spacing, 0.0));
            lines
        }
        HatchPattern::Horizontal => line_family(boundary, angle_deg, spacing, 0.0),
        HatchPattern::Grid => {
            let mut lines = line_family(boundary, angle_deg, spacing, 0.0);
            lines.extend(line_family(boundary, 90.0 + angle_deg, spacing, 0.0));
            lines
        }
        HatchPattern::Solid => line_family(boundary, 45.0 + angle_deg, (spacing * 0.08).max(8.0), 0.0),
        HatchPattern::Brick => {
            // Courses `spacing` high; head joints every 2 × spacing, offset half a brick
            let a = angle_deg.to_radians();
            let (dir, normal) = (Vector2D::new(a.cos(), a.sin()), Vector2D::new(-a.sin(), a.cos()));
            let mut lines = line_family(boundary, angle_deg, spacing, 0.0);
            let brick = spacing * 2.0;
            for (shift, row_offset) in [(0.0, 0.0), (brick * 0.5, spacing)] {
                for (p, q) in line_family(boundary, angle_deg + 90.0, brick, shift) {
                    // Keep the parts of each head joint inside alternate courses
                    let (tp, tq) = (p.dot(normal), q.dot(normal));
                    let (t0, t1) = (tp.min(tq), tp.max(tq));
                    let along = p.dot(dir);
                    let mut c = ((t0 - row_offset) / (2.0 * spacing)).floor() * 2.0 * spacing + row_offset;
                    while c < t1 {
                        let (s0, s1) = (c.max(t0), (c + spacing).min(t1));
                        if s1 > s0 {
                            lines.push((dir * along + normal * s0, dir * along + normal * s1));
                        }
                        c += 2.0 * spacing;
                    }
                }
            }
            lines
        }
    }
}

/// Line type (線型) of drafting line work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineType {
    #[default]
    Continuous,
    Dashed,
    /// Long-short centre line
    Center,
    /// Short dashes for hidden edges
    Hidden,
}

impl LineType {
    pub const ALL: [Self; 4] = [Self::Continuous, Self::Dashed, Self::Center, Self::Hidden];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Continuous => "實線 (Continuous)",
            Self::Dashed => "虛線 (Dashed)",
            Self::Center => "中心線 (Center)",
            Self::Hidden => "隱藏線 (Hidden)",
        }
    }
}

fn default_layer() -> String {
    "0".to_string()
}

/// A 2D drafting entity on the floor plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftShape {
    pub id: ElementId,
    pub name: String,
    pub geometry: DraftGeometry,
    /// Floor level the shape is drawn on (mm), so it appears on that storey's sheet
    #[serde(default)]
    pub elevation: f64,
    /// 圖層 name (CAD layer)
    #[serde(default = "default_layer")]
    pub layer: String,
    /// Pen colour; `None` = the plan's default drafting colour
    #[serde(default)]
    pub color: Option<[u8; 4]>,
    #[serde(default)]
    pub linetype: LineType,
}

impl DraftShape {
    pub fn new(geometry: DraftGeometry, elevation: f64) -> Self {
        Self {
            id: ElementId::new(),
            name: geometry.label().to_string(),
            geometry,
            elevation,
            layer: default_layer(),
            color: None,
            linetype: LineType::Continuous,
        }
    }

    /// Shortest distance from `p` to the drawn lines (0 inside a hatch).
    pub fn distance_to(&self, p: Point2D) -> f64 {
        let (points, closed) = self.geometry.outline();
        if matches!(self.geometry, DraftGeometry::Hatch { .. }) && point_in_polygon(p, &points) {
            return 0.0;
        }
        let count = if closed { points.len() } else { points.len().saturating_sub(1) };
        (0..count)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let ab = b - a;
                let t = if ab.length_squared() > 1e-12 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                p.distance(a + ab * t)
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(p: Point2D, polygon: &[Point2D]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

impl BimElement for DraftShape {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        match self.geometry {
            DraftGeometry::Hatch { .. } => ElementKind::Hatch,
            _ => ElementKind::Draft,
        }
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let (points, closed) = self.geometry.outline();
        let mut prims = vec![Polyline2D::new(points, closed)];
        if let DraftGeometry::Hatch { boundary, pattern, spacing, angle_deg } = &self.geometry {
            prims.extend(hatch_lines(boundary, *pattern, *spacing, *angle_deg).into_iter().map(|(a, b)| Polyline2D::new(vec![a, b], false)));
        }
        prims
    }

    /// Plan annotation only: no 3D body.
    fn to_3d_mesh(&self) -> TriangleMesh {
        TriangleMesh::new()
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        QuantityTakeoff::new()
    }

    fn translate(&mut self, delta: Vector3D) {
        self.geometry.translate(Vector2D::new(delta.x, delta.y));
        self.elevation += delta.z;
    }

    fn clone_box(&self) -> Box<dyn BimElement> {
        Box::new(self.clone())
    }

    fn clone_with_new_id(&self) -> Box<dyn BimElement> {
        let mut cloned = self.clone();
        cloned.id = ElementId::new();
        Box::new(cloned)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Vec<Point2D> {
        vec![Point2D::new(0.0, 0.0), Point2D::new(size, 0.0), Point2D::new(size, size), Point2D::new(0.0, size)]
    }

    #[test]
    fn three_point_arcs_pick_the_side_through_the_middle_point() {
        let (a, c) = (Point2D::new(1000.0, 0.0), Point2D::new(-1000.0, 0.0));
        let Some(DraftGeometry::Arc { center, radius, start_deg, sweep_deg }) = DraftGeometry::arc_through(a, Point2D::new(0.0, 1000.0), c) else {
            panic!("arc expected")
        };
        assert!(center.length() < 1e-9 && (radius - 1000.0).abs() < 1e-9);
        assert!((start_deg - 0.0).abs() < 1e-9 && (sweep_deg - 180.0).abs() < 1e-9, "upper half, CCW");
        let Some(DraftGeometry::Arc { sweep_deg, .. }) = DraftGeometry::arc_through(a, Point2D::new(0.0, -1000.0), c) else { panic!() };
        assert!((sweep_deg + 180.0).abs() < 1e-9, "lower half, clockwise");
        assert!(DraftGeometry::arc_through(a, Point2D::ZERO, c).is_none(), "collinear points");
        let arc = DraftGeometry::arc_through(a, Point2D::new(0.0, 1000.0), c).unwrap();
        let (pts, _) = arc.outline();
        assert!(pts.first().unwrap().distance(a) < 1e-9 && pts.last().unwrap().distance(c) < 1e-9);
    }

    #[test]
    fn splines_pass_through_every_clicked_point() {
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(1000.0, 800.0), Point2D::new(2000.0, -300.0), Point2D::new(3500.0, 200.0)];
        let curve = catmull_rom(&pts, 16);
        assert_eq!(curve.len(), 1 + 3 * 16);
        for (i, p) in pts.iter().enumerate() {
            assert!(curve[i * 16].distance(*p) < 1e-6, "point {i}");
        }
    }

    #[test]
    fn hatch_lines_stay_inside_the_boundary() {
        let boundary = square(1000.0);
        for pattern in HatchPattern::ALL {
            let lines = hatch_lines(&boundary, pattern, 100.0, 0.0);
            assert!(!lines.is_empty(), "{pattern:?}");
            for (a, b) in &lines {
                for p in [a, b] {
                    assert!(p.x > -1e-6 && p.x < 1000.0 + 1e-6 && p.y > -1e-6 && p.y < 1000.0 + 1e-6, "{pattern:?} {p:?}");
                }
            }
        }
        // 1 m square, 100 mm horizontal spacing: 9 interior lines (t = 100 … 900)
        assert_eq!(hatch_lines(&boundary, HatchPattern::Horizontal, 100.0, 0.0).len(), 9);
        // An L-shaped (concave) region splits lines into two pieces where it is notched
        let l_shape = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(1000.0, 0.0),
            Point2D::new(1000.0, 400.0),
            Point2D::new(400.0, 400.0),
            Point2D::new(400.0, 1000.0),
            Point2D::new(0.0, 1000.0),
        ];
        let vertical = hatch_lines(&l_shape, HatchPattern::Horizontal, 100.0, 90.0);
        assert!(vertical.iter().all(|(a, b)| { let x = a.x; x <= 400.0 + 1e-6 || a.y.max(b.y) <= 400.0 + 1e-6 }));
    }

    #[test]
    fn shapes_are_annotations_with_kinds_picking_and_moves() {
        let mut hatch = DraftShape::new(DraftGeometry::Hatch { boundary: square(1000.0), pattern: HatchPattern::Cross, spacing: 200.0, angle_deg: 0.0 }, 0.0);
        assert_eq!(hatch.kind(), ElementKind::Hatch);
        assert_eq!(hatch.distance_to(Point2D::new(500.0, 500.0)), 0.0);
        assert!(hatch.to_3d_mesh().positions.is_empty() && hatch.calculate_qto().items.is_empty());
        hatch.translate(Vector3D::new(100.0, 0.0, 0.0));
        assert_eq!(hatch.geometry.outline().0[0], Point2D::new(100.0, 0.0));

        let circle = DraftShape::new(DraftGeometry::Circle { center: Point2D::ZERO, radius: 500.0 }, 0.0);
        assert_eq!(circle.kind(), ElementKind::Draft);
        assert!((circle.distance_to(Point2D::new(800.0, 0.0)) - 300.0).abs() < 1.0);
        assert!(circle.geometry.closed_region().is_some());
        assert!(DraftGeometry::Polyline { points: square(10.0), closed: false }.closed_region().is_none());
    }
}
