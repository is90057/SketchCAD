use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Beam {
    pub id: ElementId,
    pub name: String,
    pub start: Point3D,
    pub end: Point3D,
    pub width: f64,  // mm
    pub depth: f64,  // mm
    pub material: Material,
}

impl Beam {
    pub fn new(start: Point3D, end: Point3D, width: f64, depth: f64, material: Material) -> Self {
        Self {
            id: ElementId::new(),
            name: "Structural Beam 結構大樑".to_string(),
            start,
            end,
            width,
            depth,
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }
}

impl BimElement for Beam {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Beam
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        // Planar dashed line symbol for overhead beam
        let p_start = Point2D::new(self.start.x, self.start.y);
        let p_end = Point2D::new(self.end.x, self.end.y);
        vec![Polyline2D::new(vec![p_start, p_end], false)]
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let len = self.length();
        if len <= 1.0 {
            return TriangleMesh::new();
        }
        let dir = (self.end - self.start) / len;
        let norm = if dir.x.abs() > 1e-4 || dir.y.abs() > 1e-4 {
            let n2 = Vector2D::new(-dir.y, dir.x).normalize();
            Vector3D::new(n2.x, n2.y, 0.0)
        } else {
            Vector3D::Y
        };
        let height_vec = Vector3D::new(0.0, 0.0, self.depth);
        TriangleMesh::create_oriented_box(
            self.start,
            self.end,
            norm * self.width,
            height_vec,
        )
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let vol_m3 = (self.width / 1000.0) * (self.depth / 1000.0) * (self.length() / 1000.0);
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
        self.start += delta;
        self.end += delta;
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
