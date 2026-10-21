use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Accessible Ramp (無障礙坡道與車行坡道)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ramp {
    pub id: ElementId,
    pub name: String,
    pub start: Point2D,   // Bottom of ramp
    pub end: Point2D,     // Top of ramp
    pub width: f64,       // mm, e.g. 1200.0 (無障礙法規標準淨寬)
    pub total_rise: f64,  // mm, e.g. 600.0
    pub base_z: f64,      // mm, e.g. 0.0
    pub material: Material,
}

impl Ramp {
    pub fn new(
        start: Point2D,
        end: Point2D,
        width: f64,
        total_rise: f64,
        base_z: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Accessible Ramp 無障礙坡道".to_string(),
            start,
            end,
            width,
            total_rise,
            base_z,
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn slope_ratio(&self) -> f64 {
        let len = self.length();
        if self.total_rise > 0.0 {
            len / self.total_rise
        } else {
            0.0
        }
    }

    pub fn area_m2(&self) -> f64 {
        (self.length() * self.width) / 1_000_000.0
    }
}

impl BimElement for Ramp {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Ramp
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let len = self.length();
        if len <= 1.0 {
            return primitives;
        }

        let dir = (self.end - self.start) / len;
        let norm = Vector2D::new(-dir.y, dir.x);
        let half_w = self.width * 0.5;

        let p0 = self.start - norm * half_w;
        let p1 = self.start + norm * half_w;
        let p2 = self.end + norm * half_w;
        let p3 = self.end - norm * half_w;

        // 1. Boundary rectangle
        primitives.push(Polyline2D::new(vec![p0, p1, p2, p3], true));

        // 2. Start & End landing thresholds
        primitives.push(Polyline2D::new(vec![p0, p1], false));
        primitives.push(Polyline2D::new(vec![p3, p2], false));

        // 3. Slope directional arrow along centerline (向高處爬升箭頭)
        let mid_start = self.start + dir * (len * 0.2);
        let mid_end = self.start + dir * (len * 0.8);
        primitives.push(Polyline2D::new(vec![mid_start, mid_end], false));

        // Arrowhead
        let arrow_left = mid_end - dir * 250.0 + norm * 150.0;
        let arrow_right = mid_end - dir * 250.0 - norm * 150.0;
        primitives.push(Polyline2D::new(vec![arrow_left, mid_end, arrow_right], false));

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let len = self.length();
        if len <= 1.0 {
            return mesh;
        }

        let dir = (self.end - self.start) / len;
        let norm = Vector2D::new(-dir.y, dir.x);
        let half_w = self.width * 0.5;

        let z_bot = self.base_z;
        let z_top = self.base_z + self.total_rise;

        let dir_3d = Vector3D::new(dir.x, dir.y, 0.0);
        let norm_3d = Vector3D::new(norm.x, norm.y, 0.0);

        // 4 bottom vertices
        let b0 = Point3D::new(self.start.x - norm.x * half_w, self.start.y - norm.y * half_w, z_bot);
        let b1 = Point3D::new(self.start.x + norm.x * half_w, self.start.y + norm.y * half_w, z_bot);
        let b2 = Point3D::new(self.end.x + norm.x * half_w, self.end.y + norm.y * half_w, z_bot);
        let b3 = Point3D::new(self.end.x - norm.x * half_w, self.end.y - norm.y * half_w, z_bot);

        // 2 elevated top landing vertices
        let t2 = Point3D::new(self.end.x + norm.x * half_w, self.end.y + norm.y * half_w, z_top);
        let t3 = Point3D::new(self.end.x - norm.x * half_w, self.end.y - norm.y * half_w, z_top);

        // Helper to add clean quads with edge flags suppressing internal diagonals
        let add_quad = |mesh: &mut TriangleMesh, p0: Point3D, p1: Point3D, p2: Point3D, p3: Point3D, normal: Vector3D| {
            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            mesh.edge_flags.push([true, true, false]);
            mesh.edge_flags.push([false, true, true]);
        };

        // 1. Top sloped driving surface
        let sloped_normal = Vector3D::new(-dir.x * self.total_rise, -dir.y * self.total_rise, len).normalize();
        add_quad(&mut mesh, b0, b1, t2, t3, sloped_normal);

        // 2. High end vertical wall face
        let n_high = dir_3d;
        add_quad(&mut mesh, b2, b3, t3, t2, n_high);

        // 3. Left triangular flank wall
        let base = mesh.positions.len() as u32;
        let n_left = -norm_3d;
        mesh.positions.extend_from_slice(&[b0, b3, t3]);
        mesh.normals.extend_from_slice(&[n_left, n_left, n_left]);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.edge_flags.push([true, true, true]);

        // 4. Right triangular flank wall
        let base = mesh.positions.len() as u32;
        let n_right = norm_3d;
        mesh.positions.extend_from_slice(&[b1, t2, b2]);
        mesh.normals.extend_from_slice(&[n_right, n_right, n_right]);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.edge_flags.push([true, true, true]);

        // 5. Underside base slab
        let n_bot = -Vector3D::Z;
        add_quad(&mut mesh, b0, b3, b2, b1, n_bot);

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = self.area_m2();
        let vol_m3 = (self.length() * self.width * (self.total_rise * 0.5)) / 1_000_000_000.0;
        let slope_ratio = self.slope_ratio();

        // 結構混凝土澆置
        qto.add_item(CostLineItem::new(
            self.id,
            "Structure 結構工程",
            format!("{} 鋼筋混凝土實體斜坡澆置", self.name),
            vol_m3,
            "m³",
            3400.0,
        ));

        // 防滑耐磨金剛砂地坪
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 泥作與表面裝修",
            format!("{} 防滑金剛砂抿石子坡道鋪面 (坡度 1:{:.1})", self.name, slope_ratio),
            area_m2,
            "m²",
            950.0,
        ));

        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.start.x += delta.x;
        self.start.y += delta.y;
        self.end.x += delta.x;
        self.end.y += delta.y;
        self.base_z += delta.z;
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
