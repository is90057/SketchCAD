use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Structural Floor Slab or Foundation Raft (結構樓板 / 筏式基礎板)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slab {
    pub id: ElementId,
    pub name: String,
    pub origin: Point2D, // Lower-left / minimum corner
    pub width: f64,      // mm (X dimension)
    pub depth: f64,      // mm (Y dimension)
    pub thickness: f64,  // mm (Z dimension, e.g. 150 - 250mm)
    pub base_z: f64,     // mm (Elevation of bottom face, e.g. -200mm or 3000mm)
    pub material: Material,
}

impl Slab {
    pub fn new(
        origin: Point2D,
        width: f64,
        depth: f64,
        thickness: f64,
        base_z: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Structural Floor Slab 結構樓板".to_string(),
            origin,
            width,
            depth,
            thickness,
            base_z,
            material,
        }
    }

    pub fn area_m2(&self) -> f64 {
        (self.width.abs() / 1000.0) * (self.depth.abs() / 1000.0)
    }

    pub fn volume_m3(&self) -> f64 {
        self.area_m2() * (self.thickness.abs() / 1000.0)
    }

    pub fn corners(&self) -> [Point2D; 4] {
        let x0 = self.origin.x;
        let y0 = self.origin.y;
        let x1 = x0 + self.width;
        let y1 = y0 + self.depth;
        [
            Point2D::new(x0, y0),
            Point2D::new(x1, y0),
            Point2D::new(x1, y1),
            Point2D::new(x0, y1),
        ]
    }
}

impl BimElement for Slab {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Slab
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let corners = self.corners();
        // 1. Boundary polygon
        let boundary = Polyline2D::new(corners.to_vec(), true);

        // 2. Architectural Floor Level Indicator (Cross line or subtle boundary offset)
        let diag1 = Polyline2D::new(vec![corners[0], corners[2]], false);
        let diag2 = Polyline2D::new(vec![corners[1], corners[3]], false);

        vec![boundary, diag1, diag2]
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let size = Vector3D::new(self.width.abs(), self.depth.abs(), self.thickness.abs());
        let center = Point3D::new(
            self.origin.x + self.width * 0.5,
            self.origin.y + self.depth * 0.5,
            self.base_z + self.thickness * 0.5,
        );
        TriangleMesh::create_box(size, center)
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let vol_m3 = self.volume_m3();
        let area_m2 = self.area_m2();

        // 結構體澆置
        qto.add_item(CostLineItem::new(
            self.id,
            "Structure 結構工程",
            format!("{} 鋼筋混凝土澆置 (厚 {:.0}mm)", self.name, self.thickness),
            vol_m3,
            "m³",
            self.material.cost_per_m3,
        ));

        // 表面粉光耐磨處理
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 泥作與表面裝修",
            format!("{} 表面整體粉光與防塵耐磨地坪", self.name),
            area_m2,
            "m²",
            480.0,
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
