//! 3D model library (3D 模型百寶箱): a catalogue of ready-made parametric models —
//! architectural features, furniture, appliances, plants, vehicles, street furniture and
//! scale figures — that can be placed into a project as [`crate::ModelComponent`]s.
//!
//! Models are generated procedurally from a small solid-modelling kit ([`Kit`]) in a local
//! frame: millimetres, Z up, the footprint centred on the origin, the front facing −Y.

mod catalog;
mod custom;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sketchcad_core::{Point3D, Vector3D};
use sketchcad_geom::TriangleMesh;

pub use catalog::CATALOG;
pub use custom::{
    all_models, convert_imported_mesh, find_model, register_custom_model, unregister_custom_model, CustomModel, ModelLibraryFile,
    ModelRef,
};

/// Shelf of the model library (模型分類).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelCategory {
    Architecture,
    Furniture,
    Appliance,
    Kitchen,
    PottedPlant,
    Landscape,
    Vehicle,
    Outdoor,
    People,
}

impl ModelCategory {
    pub const ALL: [Self; 9] = [
        Self::Architecture,
        Self::Furniture,
        Self::Appliance,
        Self::Kitchen,
        Self::PottedPlant,
        Self::Landscape,
        Self::Vehicle,
        Self::Outdoor,
        Self::People,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Architecture => "🏢 建築構件",
            Self::Furniture => "🗄 家具",
            Self::Appliance => "🔌 電器用品",
            Self::Kitchen => "🍽 廚房用品",
            Self::PottedPlant => "🌱 盆栽",
            Self::Landscape => "🌳 植物景觀",
            Self::Vehicle => "🚗 車輛",
            Self::Outdoor => "🌄 戶外設施",
            Self::People => "🚶 人物配景",
        }
    }

    /// Cost-estimate heading for models of this category.
    pub fn qto_category(&self) -> &'static str {
        match self {
            Self::Architecture => "Model 建築構件 (模型庫)",
            Self::Furniture => "FF&E 家具與軟裝設備工程",
            Self::Appliance => "Appliance 家電設備",
            Self::Kitchen => "Kitchen 廚具與廚房用品",
            Self::PottedPlant | Self::Landscape => "Landscape 植栽景觀工程",
            Self::Vehicle | Self::Outdoor | Self::People => "Entourage 配景與戶外設施",
        }
    }

    /// Plants are drawn with a tree / shrub plan symbol.
    pub fn is_planting(&self) -> bool {
        matches!(self, Self::PottedPlant | Self::Landscape)
    }
}

/// One entry of the catalogue.
#[derive(Debug)]
pub struct ModelSpec {
    /// Stable identifier stored in project files.
    pub id: &'static str,
    pub name: &'static str,
    pub category: ModelCategory,
    /// Colour of the model's main body — the surfaces the paint bucket recolours.
    pub body_color: [u8; 4],
    /// Default unit price (TWD per piece).
    pub price: f64,
    /// Default mounting height of the model's base above the floor (mm).
    pub elevation: f64,
    build: fn(&mut Kit),
}

/// Built geometry of a catalogue model, shared by every instance.
#[derive(Debug)]
pub struct ModelGeometry {
    /// Local mesh; triangles without a face colour take the body / paint colour.
    pub mesh: TriangleMesh,
    /// Convex plan outline (local XY), counter-clockwise.
    pub outline: Vec<(f64, f64)>,
    /// Overall (width, depth, height) in mm.
    pub size: (f64, f64, f64),
}

impl ModelSpec {
    pub fn find(id: &str) -> Option<&'static ModelSpec> {
        CATALOG.iter().find(|m| m.id == id)
    }

    pub fn in_category(category: ModelCategory) -> impl Iterator<Item = &'static ModelSpec> {
        CATALOG.iter().filter(move |m| m.category == category)
    }

    /// Geometry of this model, built on first use and cached.
    pub fn geometry(&self) -> Arc<ModelGeometry> {
        static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<ModelGeometry>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(geometry) = cache.lock().expect("model cache").get(self.id) {
            return geometry.clone();
        }
        let geometry = Arc::new(self.build_geometry());
        cache.lock().expect("model cache").insert(self.id, geometry.clone());
        geometry
    }

    fn build_geometry(&self) -> ModelGeometry {
        let mut kit = Kit::default();
        (self.build)(&mut kit);
        ModelGeometry::from_mesh(kit.mesh)
    }
}

impl ModelGeometry {
    /// Normalises a model mesh — footprint centred on the origin, resting on z = 0 — and
    /// derives its feature edges, plan outline and size.
    pub fn from_mesh(mut mesh: TriangleMesh) -> Self {
        normalize_mesh(&mut mesh);
        if mesh.face_colors.iter().all(Option::is_none) {
            mesh.face_colors.clear();
        }
        mesh.compute_crease_edges(35f64.to_radians());

        let size = mesh.bbox().map(|b| {
            let s = b.size();
            (s.x, s.y, s.z)
        });
        let points: Vec<(f64, f64)> = mesh.positions.iter().map(|p| (p.x, p.y)).collect();
        ModelGeometry {
            outline: convex_hull(points),
            size: size.unwrap_or((0.0, 0.0, 0.0)),
            mesh,
        }
    }
}

/// Moves a mesh so its footprint is centred on the origin and it rests on z = 0.
pub(crate) fn normalize_mesh(mesh: &mut TriangleMesh) {
    if let Some(bbox) = mesh.bbox() {
        let shift = Vector3D::new(-(bbox.min.x + bbox.max.x) * 0.5, -(bbox.min.y + bbox.max.y) * 0.5, -bbox.min.z);
        for p in &mut mesh.positions {
            *p += shift;
        }
    }
}

/// Andrew's monotone chain; returns the hull counter-clockwise without repeating the start.
fn convex_hull(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &(f64, f64)>> = if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Colour of a part: `None` = the model body (paintable), `Some` = a fixed colour.
pub type Paint = Option<[u8; 4]>;

/// Body colour marker for [`Kit`] parts.
pub const BODY: Paint = None;

/// Minimal solid-modelling kit used to author catalogue models.
#[derive(Default)]
pub struct Kit {
    mesh: TriangleMesh,
}

fn p3(v: [f64; 3]) -> Point3D {
    Point3D::new(v[0], v[1], v[2])
}

fn v3(v: [f64; 3]) -> Vector3D {
    Vector3D::new(v[0], v[1], v[2])
}

/// Two unit vectors perpendicular to `axis` (and to each other).
fn basis(axis: Vector3D) -> (Vector3D, Vector3D) {
    let helper = if axis.z.abs() < 0.9 { Vector3D::new(0.0, 0.0, 1.0) } else { Vector3D::new(1.0, 0.0, 0.0) };
    let u = helper.cross(axis).normalize();
    let v = axis.cross(u).normalize();
    (u, v)
}

impl Kit {
    /// Adds one triangle, wound so that it faces along the given vertex normals.
    fn tri(&mut self, p: [Point3D; 3], n: [Vector3D; 3], paint: Paint) {
        let face = (p[1] - p[0]).cross(p[2] - p[0]);
        let (p, n) = if face.dot(n[0] + n[1] + n[2]) < 0.0 { ([p[0], p[2], p[1]], [n[0], n[2], n[1]]) } else { (p, n) };
        let base = self.mesh.positions.len() as u32;
        self.mesh.positions.extend_from_slice(&p);
        self.mesh.normals.extend_from_slice(&n);
        self.mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        self.mesh.edge_flags.push([true; 3]);
        self.mesh.face_colors.push(paint);
    }

    fn quad(&mut self, p: [Point3D; 4], n: Vector3D, paint: Paint) {
        self.tri([p[0], p[1], p[2]], [n; 3], paint);
        self.tri([p[0], p[2], p[3]], [n; 3], paint);
    }

    /// Axis-aligned box between two corners.
    pub fn cuboid(&mut self, min: [f64; 3], max: [f64; 3], paint: Paint) {
        let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        let h = [(max[0] - min[0]).abs() * 0.5, (max[1] - min[1]).abs() * 0.5, (max[2] - min[2]).abs() * 0.5];
        self.oriented_box(c, [h[0], 0.0, 0.0], [0.0, h[1], 0.0], [0.0, 0.0, h[2]], paint);
    }

    /// Box centred at `c` spanned by three half-extent vectors (any orientation).
    pub fn oriented_box(&mut self, c: [f64; 3], ex: [f64; 3], ey: [f64; 3], ez: [f64; 3], paint: Paint) {
        let (c, ex, ey, ez) = (p3(c), v3(ex), v3(ey), v3(ez));
        for (axis, a, b) in [(ex, ey, ez), (ey, ez, ex), (ez, ex, ey)] {
            if axis.length() < 1e-9 || a.length() < 1e-9 || b.length() < 1e-9 {
                continue;
            }
            for sign in [1.0, -1.0] {
                let f = c + axis * sign;
                let n = (axis * sign).normalize();
                self.quad([f - a - b, f + a - b, f + a + b, f - a + b], n, paint);
            }
        }
    }

    /// Box of size `size` centred at `c`, rotated by `yaw` (about Z) after tilting by `tilt`
    /// (about the box's own X axis). Angles in degrees.
    pub fn tilted_box(&mut self, c: [f64; 3], size: [f64; 3], yaw: f64, tilt: f64, paint: Paint) {
        let (sy, cy) = yaw.to_radians().sin_cos();
        let (st, ct) = tilt.to_radians().sin_cos();
        let x = [cy, sy, 0.0];
        let y = [-sy * ct, cy * ct, st];
        let z = [sy * st, -cy * st, ct];
        let scale = |v: [f64; 3], s: f64| [v[0] * s * 0.5, v[1] * s * 0.5, v[2] * s * 0.5];
        self.oriented_box(c, scale(x, size[0]), scale(y, size[1]), scale(z, size[2]), paint);
    }

    /// Truncated cone from `a` (radius `ra`) to `b` (radius `rb`), capped at both ends.
    pub fn frustum(&mut self, a: [f64; 3], b: [f64; 3], ra: f64, rb: f64, segments: usize, paint: Paint) {
        let (a, b) = (p3(a), p3(b));
        let axis_vec = b - a;
        let len = axis_vec.length();
        if len < 1e-9 {
            return;
        }
        let axis = axis_vec / len;
        let (u, v) = basis(axis);
        let segments = segments.max(3);
        let slope = (ra - rb) / len;
        let ring = |i: usize| {
            let t = i as f64 / segments as f64 * std::f64::consts::TAU;
            u * t.cos() + v * t.sin()
        };
        for i in 0..segments {
            let (r0, r1) = (ring(i), ring(i + 1));
            let (n0, n1) = ((r0 + axis * slope).normalize(), (r1 + axis * slope).normalize());
            let (a0, a1, b0, b1) = (a + r0 * ra, a + r1 * ra, b + r0 * rb, b + r1 * rb);
            if ra > 1e-9 {
                self.tri([a0, a1, b1], [n0, n1, n1], paint);
                self.tri([a, a1, a0], [-axis; 3], paint);
            }
            if rb > 1e-9 {
                self.tri([a0, b1, b0], [n0, n1, n0], paint);
                self.tri([b, b0, b1], [axis; 3], paint);
            }
        }
    }

    /// Cylinder of radius `r` from `a` to `b`.
    pub fn cylinder(&mut self, a: [f64; 3], b: [f64; 3], r: f64, segments: usize, paint: Paint) {
        self.frustum(a, b, r, r, segments, paint);
    }

    /// Ellipsoid with semi-axes `r` centred at `c`.
    pub fn ellipsoid(&mut self, c: [f64; 3], r: [f64; 3], paint: Paint) {
        self.leaf(c, r, 0.0, 0.0, paint);
    }

    /// Ellipsoid rotated like [`Self::tilted_box`]: `yaw` about Z after `tilt` about its own
    /// X axis (degrees). Flattened ones make leaves, petals and fronds.
    pub fn leaf(&mut self, c: [f64; 3], r: [f64; 3], yaw: f64, tilt: f64, paint: Paint) {
        let (sy, cy) = yaw.to_radians().sin_cos();
        let (st, ct) = tilt.to_radians().sin_cos();
        let ax = Vector3D::new(cy, sy, 0.0);
        let ay = Vector3D::new(-sy * ct, cy * ct, st);
        let az = Vector3D::new(sy * st, -cy * st, ct);
        let (rings, segs) = (8, 14);
        let point = |i: usize, j: usize| {
            let (s_t, c_t) = (i as f64 / rings as f64 * std::f64::consts::PI).sin_cos();
            let (s_p, c_p) = (j as f64 / segs as f64 * std::f64::consts::TAU).sin_cos();
            let u = [s_t * c_p, s_t * s_p, c_t];
            let pos = p3(c) + ax * (r[0] * u[0]) + ay * (r[1] * u[1]) + az * (r[2] * u[2]);
            let normal = (ax * (u[0] / r[0]) + ay * (u[1] / r[1]) + az * (u[2] / r[2])).normalize();
            (pos, normal)
        };
        for i in 0..rings {
            for j in 0..segs {
                let (p00, n00) = point(i, j);
                let (p10, n10) = point(i + 1, j);
                let (p11, n11) = point(i + 1, j + 1);
                let (p01, n01) = point(i, j + 1);
                if i != rings - 1 {
                    self.tri([p00, p10, p11], [n00, n10, n11], paint);
                }
                if i != 0 {
                    self.tri([p00, p11, p01], [n00, n11, n01], paint);
                }
            }
        }
    }

    pub fn sphere(&mut self, c: [f64; 3], r: f64, paint: Paint) {
        self.ellipsoid(c, [r, r, r], paint);
    }

    /// Torus around an axis parallel to Y (a wheel rolling along X).
    pub fn torus_y(&mut self, c: [f64; 3], major: f64, minor: f64, paint: Paint) {
        let (nu, nv) = (18, 8);
        let point = |i: usize, j: usize| {
            let (su, cu) = (i as f64 / nu as f64 * std::f64::consts::TAU).sin_cos();
            let (sv, cv) = (j as f64 / nv as f64 * std::f64::consts::TAU).sin_cos();
            let radial = Vector3D::new(cu, 0.0, su);
            let normal = radial * cv + Vector3D::new(0.0, sv, 0.0);
            (p3(c) + radial * major + normal * minor, normal)
        };
        for i in 0..nu {
            for j in 0..nv {
                let (p00, n00) = point(i, j);
                let (p10, n10) = point(i + 1, j);
                let (p11, n11) = point(i + 1, j + 1);
                let (p01, n01) = point(i, j + 1);
                self.tri([p00, p10, p11], [n00, n10, n11], paint);
                self.tri([p00, p11, p01], [n00, n11, n01], paint);
            }
        }
    }

    /// Convex profile in the XZ plane, extruded along Y from `y0` to `y1`.
    pub fn extrude_y(&mut self, profile: &[(f64, f64)], y0: f64, y1: f64, paint: Paint) {
        if profile.len() < 3 {
            return;
        }
        let n = profile.len() as f64;
        let (cx, cz) = profile.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0 / n, acc.1 + p.1 / n));
        let at = |p: (f64, f64), y: f64| Point3D::new(p.0, y, p.1);
        for i in 0..profile.len() {
            let (p, q) = (profile[i], profile[(i + 1) % profile.len()]);
            // Side face, normal pointing away from the profile centroid
            let mut normal = Vector3D::new(q.1 - p.1, 0.0, -(q.0 - p.0)).normalize();
            if normal.dot(Vector3D::new((p.0 + q.0) * 0.5 - cx, 0.0, (p.1 + q.1) * 0.5 - cz)) < 0.0 {
                normal = -normal;
            }
            self.quad([at(p, y0), at(q, y0), at(q, y1), at(p, y1)], normal, paint);
            // End caps as fans
            let centre = (cx, cz);
            self.tri([at(centre, y0), at(p, y0), at(q, y0)], [Vector3D::new(0.0, -1.0, 0.0); 3], paint);
            self.tri([at(centre, y1), at(p, y1), at(q, y1)], [Vector3D::new(0.0, 1.0, 0.0); 3], paint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_model_builds_a_grounded_centred_mesh() {
        let mut ids: Vec<_> = CATALOG.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len(), "model ids must be unique");
        for category in ModelCategory::ALL {
            assert!(ModelSpec::in_category(category).count() >= 3, "{category:?} needs models");
        }
        for spec in CATALOG {
            let geometry = spec.geometry();
            let mesh = &geometry.mesh;
            assert!(mesh.triangle_count() > 0, "{}", spec.id);
            assert_eq!(mesh.normals.len(), mesh.positions.len(), "{}", spec.id);
            assert!(mesh.face_colors.is_empty() || mesh.face_colors.len() == mesh.triangle_count(), "{}", spec.id);
            assert!(mesh.face_colors.iter().any(Option::is_none) || mesh.face_colors.is_empty(), "{} has no paintable body", spec.id);
            let bbox = mesh.bbox().unwrap();
            assert!(bbox.min.z.abs() < 1e-6, "{} rests on the floor", spec.id);
            assert!((bbox.min.x + bbox.max.x).abs() < 1e-6 && (bbox.min.y + bbox.max.y).abs() < 1e-6, "{} is centred", spec.id);
            let (w, d, h) = geometry.size;
            assert!(w > 50.0 && d > 30.0 && h > 50.0 && w < 20000.0 && d < 20000.0 && h < 20000.0, "{}: {w}x{d}x{h}", spec.id);
            assert!(geometry.outline.len() >= 3, "{}", spec.id);
            for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
                assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && (n.length() - 1.0).abs() < 1e-6, "{}", spec.id);
            }
        }
    }

    #[test]
    fn primitives_face_outwards() {
        let mut kit = Kit::default();
        kit.cuboid([0.0, 0.0, 0.0], [100.0, 100.0, 100.0], BODY);
        kit.cylinder([300.0, 0.0, 0.0], [300.0, 0.0, 100.0], 50.0, 12, BODY);
        kit.sphere([600.0, 0.0, 50.0], 50.0, BODY);
        let centres = [Point3D::new(50.0, 50.0, 50.0), Point3D::new(300.0, 0.0, 50.0), Point3D::new(600.0, 0.0, 50.0)];
        let mesh = &kit.mesh;
        for t in 0..mesh.triangle_count() {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.positions[mesh.indices[t * 3 + k] as usize]);
            let centroid = Point3D::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0, (a.z + b.z + c.z) / 3.0);
            let solid = centres.iter().min_by(|p, q| centroid.distance(**p).total_cmp(&centroid.distance(**q))).unwrap();
            assert!((b - a).cross(c - a).dot(centroid - *solid) > 0.0, "triangle {t} faces inwards");
        }
    }
}
