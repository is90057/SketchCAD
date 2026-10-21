use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub id: ElementId,
    pub name: String,
    pub position: Point2D,
    pub width: f64,  // mm (X dimension)
    pub depth: f64,  // mm (Y dimension)
    pub height: f64, // mm (Z height)
    pub base_z: f64, // mm
    pub material: Material,
}

impl Column {
    pub fn new(position: Point2D, width: f64, depth: f64, height: f64, material: Material) -> Self {
        Self {
            id: ElementId::new(),
            name: "Structural Column 結構柱".to_string(),
            position,
            width,
            depth,
            height,
            base_z: 0.0,
            material,
        }
    }
}

impl BimElement for Column {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Column
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let hw = self.width * 0.5;
        let hd = self.depth * 0.5;
        let p = self.position;

        // Rectangle cross section
        let rect = Polyline2D::new(
            vec![
                Point2D::new(p.x - hw, p.y - hd),
                Point2D::new(p.x + hw, p.y - hd),
                Point2D::new(p.x + hw, p.y + hd),
                Point2D::new(p.x - hw, p.y + hd),
            ],
            true,
        );

        // Standard engineering column cross hatch diagonals
        let diag1 = Polyline2D::new(
            vec![
                Point2D::new(p.x - hw, p.y - hd),
                Point2D::new(p.x + hw, p.y + hd),
            ],
            false,
        );
        let diag2 = Polyline2D::new(
            vec![
                Point2D::new(p.x - hw, p.y + hd),
                Point2D::new(p.x + hw, p.y - hd),
            ],
            false,
        );

        vec![rect, diag1, diag2]
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        TriangleMesh::create_box(
            Vector3D::new(self.width, self.depth, self.height),
            Point3D::new(self.position.x, self.position.y, self.base_z + self.height * 0.5),
        )
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let vol_m3 = (self.width / 1000.0) * (self.depth / 1000.0) * (self.height / 1000.0);
        qto.add_item(CostLineItem::new(
            self.id,
            "Structure 結構工程",
            format!("{} 澆置", self.name),
            vol_m3,
            "m³",
            self.material.cost_per_m3,
        ));
        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.position.x += delta.x;
        self.position.y += delta.y;
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
