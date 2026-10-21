use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoofType {
    Gable, // 雙坡人字斜屋頂
    Hip,   // 四坡屋頂
    Flat,  // 現代平屋頂
}

/// Architectural Roof (建築斜屋頂與平屋頂系統)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Roof {
    pub id: ElementId,
    pub name: String,
    pub origin: Point2D,
    pub width: f64,          // mm
    pub depth: f64,          // mm
    pub thickness: f64,      // mm, e.g. 200.0
    pub base_z: f64,         // mm, e.g. 3500.0
    pub pitch_deg: f64,      // degrees, e.g. 20.0
    pub overhang: f64,       // mm, e.g. 400.0
    pub roof_type: RoofType,
    pub material: Material,
}

impl Roof {
    pub fn new(
        origin: Point2D,
        width: f64,
        depth: f64,
        base_z: f64,
        pitch_deg: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Gable Roof 雙坡斜屋頂系統".to_string(),
            origin,
            width,
            depth,
            thickness: 200.0,
            base_z,
            pitch_deg: pitch_deg.clamp(0.0, 60.0),
            overhang: 400.0,
            roof_type: RoofType::Gable,
            material,
        }
    }

    pub fn footprint_area_m2(&self) -> f64 {
        ((self.width + self.overhang * 2.0) * (self.depth + self.overhang * 2.0)) / 1_000_000.0
    }

    pub fn surface_area_m2(&self) -> f64 {
        let base_area = self.footprint_area_m2();
        if self.pitch_deg > 0.1 {
            base_area / self.pitch_deg.to_radians().cos()
        } else {
            base_area
        }
    }
}

impl BimElement for Roof {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Roof
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();

        let min_x = self.origin.x - self.overhang;
        let max_x = self.origin.x + self.width + self.overhang;
        let min_y = self.origin.y - self.overhang;
        let max_y = self.origin.y + self.depth + self.overhang;

        // 1. Roof outer boundary with overhang (屋簷外廓線)
        primitives.push(Polyline2D::new(
            vec![
                Point2D::new(min_x, min_y),
                Point2D::new(max_x, min_y),
                Point2D::new(max_x, max_y),
                Point2D::new(min_x, max_y),
            ],
            true,
        ));

        // 2. Ridge line & Slope arrows (屋脊中心線與排水坡度指示線)
        if self.roof_type == RoofType::Gable && self.pitch_deg > 0.1 {
            let mid_x = (min_x + max_x) * 0.5;
            // Ridge line
            primitives.push(Polyline2D::new(
                vec![Point2D::new(mid_x, min_y), Point2D::new(mid_x, max_y)],
                false,
            ));

            // Left slope arrows
            let arrow_y = (min_y + max_y) * 0.5;
            primitives.push(Polyline2D::new(
                vec![Point2D::new(mid_x - 300.0, arrow_y), Point2D::new(min_x + 300.0, arrow_y)],
                false,
            ));
            // Right slope arrows
            primitives.push(Polyline2D::new(
                vec![Point2D::new(mid_x + 300.0, arrow_y), Point2D::new(max_x - 300.0, arrow_y)],
                false,
            ));
        }

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();

        let min_x = self.origin.x - self.overhang;
        let max_x = self.origin.x + self.width + self.overhang;
        let min_y = self.origin.y - self.overhang;
        let max_y = self.origin.y + self.depth + self.overhang;
        let half_w = (max_x - min_x) * 0.5;

        let ridge_rise = if self.pitch_deg > 0.1 {
            half_w * self.pitch_deg.to_radians().tan()
        } else {
            0.0
        };

        let z_eaves = self.base_z;
        let z_ridge = self.base_z + ridge_rise;
        let mid_x = (min_x + max_x) * 0.5;

        if self.pitch_deg <= 0.1 || self.roof_type == RoofType::Flat {
            // Flat roof slab
            let size = Vector3D::new(max_x - min_x, max_y - min_y, self.thickness);
            let center = Point3D::new(
                (min_x + max_x) * 0.5,
                (min_y + max_y) * 0.5,
                self.base_z + self.thickness * 0.5,
            );
            return TriangleMesh::create_box(size, center);
        }

        // Helper to add clean quads with edge flags suppressing internal diagonal cuts
        let add_quad = |mesh: &mut TriangleMesh, p0: Point3D, p1: Point3D, p2: Point3D, p3: Point3D, normal: Vector3D| {
            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            mesh.edge_flags.push([true, true, false]);
            mesh.edge_flags.push([false, true, true]);
        };

        // Left sloped roof plane
        let p_left_eaves_0 = Point3D::new(min_x, min_y, z_eaves);
        let p_left_eaves_1 = Point3D::new(min_x, max_y, z_eaves);
        let p_ridge_1 = Point3D::new(mid_x, max_y, z_ridge);
        let p_ridge_0 = Point3D::new(mid_x, min_y, z_ridge);
        let n_left = Vector3D::new(-ridge_rise, 0.0, half_w).normalize();
        add_quad(&mut mesh, p_left_eaves_0, p_left_eaves_1, p_ridge_1, p_ridge_0, n_left);

        // Right sloped roof plane
        let p_right_eaves_0 = Point3D::new(max_x, min_y, z_eaves);
        let p_right_eaves_1 = Point3D::new(max_x, max_y, z_eaves);
        let n_right = Vector3D::new(ridge_rise, 0.0, half_w).normalize();
        add_quad(&mut mesh, p_ridge_0, p_ridge_1, p_right_eaves_1, p_right_eaves_0, n_right);

        // Front gable triangle (front gable wall)
        let base = mesh.positions.len() as u32;
        let n_front = -Vector3D::Y;
        mesh.positions.extend_from_slice(&[p_left_eaves_0, p_right_eaves_0, p_ridge_0]);
        mesh.normals.extend_from_slice(&[n_front, n_front, n_front]);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.edge_flags.push([true, true, true]);

        // Back gable triangle (back gable wall)
        let base = mesh.positions.len() as u32;
        let n_back = Vector3D::Y;
        mesh.positions.extend_from_slice(&[p_right_eaves_1, p_left_eaves_1, p_ridge_1]);
        mesh.normals.extend_from_slice(&[n_back, n_back, n_back]);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.edge_flags.push([true, true, true]);

        // Soffit / ceiling underside
        let n_bot = -Vector3D::Z;
        add_quad(&mut mesh, p_right_eaves_0, p_left_eaves_0, p_left_eaves_1, p_right_eaves_1, n_bot);

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = self.surface_area_m2();

        // 1. 屋面防水與隔熱工程
        qto.add_item(CostLineItem::new(
            self.id,
            "Roofing 屋面工程",
            format!("{} 防水隔熱層與瀝青油氈鋪設", self.name),
            area_m2,
            "m²",
            1200.0,
        ));

        // 2. 瓦作或立邊咬合金屬屋面板
        qto.add_item(CostLineItem::new(
            self.id,
            "Roofing 屋面工程",
            format!("{} 琉璃陶瓦/金屬立邊咬合屋面板 (坡度 {:.0}°)", self.name, self.pitch_deg),
            area_m2,
            "m²",
            2500.0,
        ));

        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.origin.x += delta.x;
        self.origin.y += delta.y;
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
