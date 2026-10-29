//! Annotations (註解): dimensions, multi-line text and multileaders on the floor plan.
//! Their lines go into the plan drawing like any element; their text is returned by
//! [`Annotation::texts`] for the viewport to draw at the right size and angle.

use crate::cost::QuantityTakeoff;
use crate::element::{BimElement, ElementKind};
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Direction a linear dimension measures along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimAxis {
    Horizontal,
    Vertical,
    /// Parallel to the measured points (對齊式)
    Aligned,
}

/// Annotation geometry, in plan coordinates (mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationGeometry {
    /// Linear / aligned dimension between `p1` and `p2`. The dimension line lies `offset`
    /// from `p1` along the axis normal.
    Linear { p1: Point2D, p2: Point2D, axis: DimAxis, offset: f64 },
    /// Angle from `start_deg` sweeping `sweep_deg` (CCW) around `center`, drawn at `radius`
    Angular { center: Point2D, start_deg: f64, sweep_deg: f64, radius: f64 },
    /// Radius of a circle / arc, leader drawn toward `angle_deg`
    Radius { center: Point2D, radius: f64, angle_deg: f64 },
    /// Diameter of a circle / arc, drawn through the centre at `angle_deg`
    Diameter { center: Point2D, radius: f64, angle_deg: f64 },
    /// Multi-line text (多行文字)
    Text { position: Point2D, text: String, rotation_deg: f64 },
    /// Multileader: arrow at `points[0]`, bends, text at the last point
    Leader { points: Vec<Point2D>, text: String },
}

/// Text to draw for an annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationText {
    /// Anchor of the text (see `centered`)
    pub position: Point2D,
    pub text: String,
    /// Cap height in mm
    pub height: f64,
    /// Baseline direction, degrees CCW from +X (kept readable: −90° < angle ≤ 90°)
    pub angle_deg: f64,
    /// Centred on `position` (dimensions) or starting there (text, leaders)
    pub centered: bool,
}

impl AnnotationGeometry {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear { axis: DimAxis::Aligned, .. } => "對齊式標註 (DIMALIGNED)",
            Self::Linear { .. } => "線性標註 (DIMLINEAR)",
            Self::Angular { .. } => "角度標註 (DIMANGULAR)",
            Self::Radius { .. } => "半徑標註 (DIMRADIUS)",
            Self::Diameter { .. } => "直徑標註 (DIMDIAMETER)",
            Self::Text { .. } => "多行文字 (MTEXT)",
            Self::Leader { .. } => "多重引線 (MLEADER)",
        }
    }

    /// Measuring direction of a linear dimension.
    pub fn linear_direction(p1: Point2D, p2: Point2D, axis: DimAxis) -> Vector2D {
        match axis {
            DimAxis::Horizontal => Vector2D::X,
            DimAxis::Vertical => Vector2D::Y,
            DimAxis::Aligned => (p2 - p1).try_normalize().unwrap_or(Vector2D::X),
        }
    }

    /// Measured value: mm for lengths, degrees for angles; `None` for text.
    pub fn measurement(&self) -> Option<f64> {
        match self {
            Self::Linear { p1, p2, axis, .. } => Some((*p2 - *p1).dot(Self::linear_direction(*p1, *p2, *axis)).abs()),
            Self::Angular { sweep_deg, .. } => Some(sweep_deg.abs()),
            Self::Radius { radius, .. } => Some(*radius),
            Self::Diameter { radius, .. } => Some(radius * 2.0),
            Self::Text { .. } | Self::Leader { .. } => None,
        }
    }

    /// Measured value as dimension text: `3600`, `45°`, `R1200`, `Ø2400`.
    pub fn measurement_text(&self) -> String {
        let value = self.measurement().unwrap_or(0.0);
        let number = |v: f64| {
            let s = format!("{v:.1}");
            s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
        };
        match self {
            Self::Angular { .. } => format!("{}°", number(value)),
            Self::Radius { .. } => format!("R{}", number(value)),
            Self::Diameter { .. } => format!("Ø{}", number(value)),
            _ => number(value),
        }
    }

    /// End points of the dimension line of a linear dimension.
    pub fn dimension_line(&self) -> Option<(Point2D, Point2D)> {
        let Self::Linear { p1, p2, axis, offset } = self else { return None };
        let d = Self::linear_direction(*p1, *p2, *axis);
        let n = d.perp();
        let a = *p1 + n * *offset;
        let b = a + d * (*p2 - *p1).dot(d);
        Some((a, b))
    }

    fn translate(&mut self, v: Vector2D) {
        match self {
            Self::Linear { p1, p2, .. } => {
                *p1 += v;
                *p2 += v;
            }
            Self::Angular { center, .. } | Self::Radius { center, .. } | Self::Diameter { center, .. } => *center += v,
            Self::Text { position, .. } => *position += v,
            Self::Leader { points, .. } => points.iter_mut().for_each(|p| *p += v),
        }
    }
}

/// Angle (degrees) of a direction, flipped by 180° when needed so text reads left-to-right.
fn readable_angle(v: Vector2D) -> f64 {
    let a = v.y.atan2(v.x).to_degrees();
    if a > 90.0 + 1e-9 {
        a - 180.0
    } else if a <= -90.0 + 1e-9 {
        a + 180.0
    } else {
        a
    }
}

/// Open arrowhead at `tip` pointing along `dir`, as a closed triangle.
fn arrow(tip: Point2D, dir: Vector2D, size: f64) -> Polyline2D {
    let d = dir.try_normalize().unwrap_or(Vector2D::X);
    let back = tip - d * size;
    let side = d.perp() * size * 0.3;
    Polyline2D::new(vec![tip, back + side, back - side], true)
}

/// Architectural tick (建築斜線) through `p` at 45° to the dimension line.
fn tick(p: Point2D, dir: Vector2D, size: f64) -> Polyline2D {
    let d = dir.try_normalize().unwrap_or(Vector2D::X);
    let slash = (d + d.perp()).normalize() * size * 0.5;
    Polyline2D::new(vec![p - slash, p + slash], false)
}

/// A dimension, text or leader on the floor plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: ElementId,
    pub name: String,
    pub geometry: AnnotationGeometry,
    /// Floor level it is drawn on (mm)
    #[serde(default)]
    pub elevation: f64,
    /// Text cap height in mm (250 mm reads as 2.5 mm on a 1:100 print)
    pub text_height: f64,
    /// Replaces the measured text of a dimension; `<>` stands for the measurement
    #[serde(default)]
    pub text_override: Option<String>,
    #[serde(default)]
    pub color: Option<[u8; 4]>,
}

impl Annotation {
    pub const DEFAULT_TEXT_HEIGHT: f64 = 250.0;

    pub fn new(geometry: AnnotationGeometry, elevation: f64, text_height: f64) -> Self {
        Self {
            id: ElementId::new(),
            name: geometry.label().to_string(),
            geometry,
            elevation,
            text_height,
            text_override: None,
            color: None,
        }
    }

    /// Text shown on a dimension (override with `<>` replaced by the measurement).
    pub fn dimension_text(&self) -> String {
        let measured = self.geometry.measurement_text();
        match &self.text_override {
            Some(t) if !t.trim().is_empty() => t.replace("<>", &measured),
            _ => measured,
        }
    }

    /// Texts to draw for this annotation.
    pub fn texts(&self) -> Vec<AnnotationText> {
        let h = self.text_height;
        let gap = h * 0.6;
        match &self.geometry {
            g @ AnnotationGeometry::Linear { .. } => {
                let (a, b) = g.dimension_line().unwrap_or_default();
                let d = (b - a).try_normalize().unwrap_or(Vector2D::X);
                let angle = readable_angle(d);
                let up = Vector2D::new(-angle.to_radians().sin(), angle.to_radians().cos());
                vec![AnnotationText { position: (a + b) * 0.5 + up * (gap + h * 0.5), text: self.dimension_text(), height: h, angle_deg: angle, centered: true }]
            }
            AnnotationGeometry::Angular { center, start_deg, sweep_deg, radius } => {
                let mid = (start_deg + sweep_deg * 0.5).to_radians();
                let dir = Vector2D::new(mid.cos(), mid.sin());
                vec![AnnotationText {
                    position: *center + dir * (radius + gap + h * 0.5),
                    text: self.dimension_text(),
                    height: h,
                    angle_deg: readable_angle(dir.perp()),
                    centered: true,
                }]
            }
            AnnotationGeometry::Radius { center, radius, angle_deg } | AnnotationGeometry::Diameter { center, radius, angle_deg } => {
                let dir = Vector2D::new(angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
                let along = radius * 0.5 * if matches!(self.geometry, AnnotationGeometry::Diameter { .. }) { 0.0 } else { 1.0 };
                let angle = readable_angle(dir);
                let up = Vector2D::new(-angle.to_radians().sin(), angle.to_radians().cos());
                vec![AnnotationText { position: *center + dir * along + up * (gap + h * 0.5), text: self.dimension_text(), height: h, angle_deg: angle, centered: true }]
            }
            AnnotationGeometry::Text { position, text, rotation_deg } => {
                vec![AnnotationText { position: *position, text: text.clone(), height: h, angle_deg: *rotation_deg, centered: false }]
            }
            AnnotationGeometry::Leader { points, text } => {
                let Some(&last) = points.last() else { return Vec::new() };
                let prev = points.len().checked_sub(2).map_or(last - Vector2D::X, |i| points[i]);
                let right = last.x >= prev.x;
                let landing = h * 1.2 * if right { 1.0 } else { -1.0 };
                let width = text_width(text, h);
                let x = if right { last.x + landing + gap * 0.5 } else { last.x + landing - gap * 0.5 - width };
                vec![AnnotationText { position: Point2D::new(x, last.y + h * 0.5), text: text.clone(), height: h, angle_deg: 0.0, centered: false }]
            }
        }
    }

    /// Distance from `p` to the annotation's lines or text (for picking).
    pub fn distance_to(&self, p: Point2D) -> f64 {
        let mut best = f64::INFINITY;
        for prim in self.to_2d_primitives() {
            let v = &prim.vertices;
            let n = if prim.closed { v.len() } else { v.len().saturating_sub(1) };
            for i in 0..n {
                let (a, b) = (v[i], v[(i + 1) % v.len()]);
                let ab = b - a;
                let t = if ab.length_squared() > 1e-12 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                best = best.min(p.distance(a + ab * t));
            }
        }
        for t in self.texts() {
            let lines = t.text.lines().count().max(1) as f64;
            let w = t.text.lines().map(|l| text_width(l, t.height)).fold(0.0, f64::max);
            let center = if t.centered { t.position } else { t.position + Vector2D::new(w * 0.5, -t.height * 0.75 * lines) };
            let d = (p - center).abs() - Vector2D::new(w * 0.5, t.height * 0.75 * lines);
            best = best.min(d.max(Vector2D::ZERO).length());
        }
        best
    }
}

/// Rough width of `text` at cap height `h` (CJK characters are about square).
pub fn text_width(text: &str, h: f64) -> f64 {
    text.lines()
        .map(|line| line.chars().map(|c| if c.is_ascii() { 0.62 } else { 1.05 }).sum::<f64>() * h)
        .fold(0.0, f64::max)
}

impl BimElement for Annotation {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Annotation
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let h = self.text_height;
        let mark = h * 0.8;
        let ext_gap = h * 0.4;
        let ext_over = h * 0.5;
        match &self.geometry {
            g @ AnnotationGeometry::Linear { p1, p2, .. } => {
                let (a, b) = g.dimension_line().unwrap_or_default();
                let d = (b - a).try_normalize().unwrap_or(Vector2D::X);
                let mut prims = vec![Polyline2D::new(vec![a - d * ext_over * 0.6, b + d * ext_over * 0.6], false)];
                for (point, foot) in [(*p1, a), (*p2, b)] {
                    // Extension line from just off the measured point to just past the dimension line
                    let v = foot - point;
                    if v.length() > ext_gap {
                        let u = v.normalize();
                        prims.push(Polyline2D::new(vec![point + u * ext_gap, foot + u * ext_over], false));
                    }
                    prims.push(tick(foot, d, mark));
                }
                prims
            }
            AnnotationGeometry::Angular { center, start_deg, sweep_deg, radius } => {
                let at = |deg: f64| *center + Vector2D::new(deg.to_radians().cos(), deg.to_radians().sin()) * *radius;
                let steps = ((sweep_deg.abs() / 4.0).ceil() as usize).max(4);
                let arc: Vec<Point2D> = (0..=steps).map(|i| at(start_deg + sweep_deg * i as f64 / steps as f64)).collect();
                let tangent = |deg: f64, sign: f64| Vector2D::new(-deg.to_radians().sin(), deg.to_radians().cos()) * sign;
                let s = sweep_deg.signum();
                vec![
                    Polyline2D::new(arc, false),
                    Polyline2D::new(vec![*center, at(*start_deg) + (at(*start_deg) - *center).normalize() * ext_over], false),
                    Polyline2D::new(vec![*center, at(start_deg + sweep_deg) + (at(start_deg + sweep_deg) - *center).normalize() * ext_over], false),
                    arrow(at(*start_deg), tangent(*start_deg, -s), mark),
                    arrow(at(start_deg + sweep_deg), tangent(start_deg + sweep_deg, s), mark),
                ]
            }
            AnnotationGeometry::Radius { center, radius, angle_deg } => {
                let dir = Vector2D::new(angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
                let tip = *center + dir * *radius;
                vec![Polyline2D::new(vec![*center, tip], false), arrow(tip, dir, mark)]
            }
            AnnotationGeometry::Diameter { center, radius, angle_deg } => {
                let dir = Vector2D::new(angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
                let (a, b) = (*center - dir * *radius, *center + dir * *radius);
                vec![Polyline2D::new(vec![a, b], false), arrow(b, dir, mark), arrow(a, -dir, mark)]
            }
            AnnotationGeometry::Text { .. } => Vec::new(),
            AnnotationGeometry::Leader { points, .. } => {
                if points.len() < 2 {
                    return Vec::new();
                }
                let (last, prev) = (points[points.len() - 1], points[points.len() - 2]);
                let landing = Vector2D::X * h * 1.2 * if last.x >= prev.x { 1.0 } else { -1.0 };
                let mut path = points.clone();
                path.push(last + landing);
                vec![Polyline2D::new(path, false), arrow(points[0], points[0] - points[1], mark)]
            }
        }
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

    fn p(x: f64, y: f64) -> Point2D {
        Point2D::new(x, y)
    }

    #[test]
    fn linear_dimensions_measure_along_their_axis() {
        let dim = |axis, offset| AnnotationGeometry::Linear { p1: p(0.0, 0.0), p2: p(3000.0, 4000.0), axis, offset };
        assert_eq!(dim(DimAxis::Horizontal, -500.0).measurement(), Some(3000.0));
        assert_eq!(dim(DimAxis::Vertical, 500.0).measurement(), Some(4000.0));
        assert_eq!(dim(DimAxis::Aligned, 0.0).measurement(), Some(5000.0));
        // Horizontal dimension 500 below p1: line from (0,-500) to (3000,-500)
        let (a, b) = dim(DimAxis::Horizontal, -500.0).dimension_line().unwrap();
        assert!(a.distance(p(0.0, -500.0)) < 1e-9 && b.distance(p(3000.0, -500.0)) < 1e-9);
        let ann = Annotation::new(dim(DimAxis::Horizontal, -500.0), 0.0, 250.0);
        assert_eq!(ann.texts()[0].text, "3000");
        assert!(ann.texts()[0].position.y > -500.0, "text sits above the dimension line");
        assert_eq!(ann.to_2d_primitives().len(), 5, "dimension line, two extension lines, two ticks");
        assert_eq!(ann.kind(), ElementKind::Annotation);
        assert!(ann.to_3d_mesh().positions.is_empty());
    }

    #[test]
    fn dimension_texts_read_left_to_right_and_take_overrides() {
        let mut ann = Annotation::new(
            AnnotationGeometry::Linear { p1: p(1000.0, 0.0), p2: p(0.0, 0.0), axis: DimAxis::Aligned, offset: 300.0 },
            0.0,
            250.0,
        );
        assert!(ann.texts()[0].angle_deg.abs() < 1e-9, "a right-to-left dimension is not upside down");
        ann.text_override = Some("<> (淨寬)".into());
        assert_eq!(ann.dimension_text(), "1000 (淨寬)");
        let r = AnnotationGeometry::Radius { center: p(0.0, 0.0), radius: 1200.0, angle_deg: 45.0 };
        let d = AnnotationGeometry::Diameter { center: p(0.0, 0.0), radius: 1200.0, angle_deg: 45.0 };
        let a = AnnotationGeometry::Angular { center: p(0.0, 0.0), start_deg: 0.0, sweep_deg: 37.5, radius: 800.0 };
        assert_eq!((r.measurement_text(), d.measurement_text(), a.measurement_text()), ("R1200".into(), "Ø2400".into(), "37.5°".into()));
    }

    #[test]
    fn leaders_and_text_can_be_picked_and_moved() {
        let mut leader = Annotation::new(AnnotationGeometry::Leader { points: vec![p(0.0, 0.0), p(1000.0, 1000.0)], text: "RC 牆".into() }, 0.0, 250.0);
        assert_eq!(leader.to_2d_primitives().len(), 2, "leader line with landing, and arrow");
        let text = &leader.texts()[0];
        assert!(text.position.x > 1000.0, "text right of the landing");
        assert!(leader.distance_to(text.position + Vector2D::new(100.0, -100.0)) < 1e-6, "clicking the text picks it");
        leader.translate(Vector3D::new(10.0, 0.0, 0.0));
        let AnnotationGeometry::Leader { points, .. } = &leader.geometry else { panic!() };
        assert_eq!(points[0], p(10.0, 0.0));
        let note = Annotation::new(AnnotationGeometry::Text { position: p(0.0, 0.0), text: "第一行\nline 2".into(), rotation_deg: 0.0 }, 0.0, 250.0);
        assert!(note.to_2d_primitives().is_empty() && note.distance_to(p(200.0, -200.0)) < 1e-6);
    }
}
