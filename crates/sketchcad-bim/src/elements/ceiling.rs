use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Suspended or Plasterboard Ceiling (天花板 / 輕鋼架石膏板天花)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ceiling {
    pub id: ElementId,
    pub name: String,
    pub origin: Point2D,   // Minimum corner
    pub width: f64,        // mm (X dimension)
    pub depth: f64,        // mm (Y dimension)
    pub thickness: f64,    // mm (e.g. 25 - 40mm)
    pub elevation: f64,    // mm (Height above floor, e.g. 2700 - 3000mm)
    pub grid_spacing: f64, // mm (e.g. 600.0mm for standard 2x2ft grid)
    pub material: Material,
}

impl Ceiling {
    pub fn new(
        origin: Point2D,
        width: f64,
        depth: f64,
        elevation: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Suspended Ceiling 輕鋼架天花板".to_string(),
            origin,
            width,
            depth,
            thickness: 30.0,
            elevation,
            grid_spacing: 600.0,
            material,
        }
    }

    pub fn area_m2(&self) -> f64 {
        (self.width.abs() / 1000.0) * (self.depth.abs() / 1000.0)
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

impl BimElement for Ceiling {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Ceiling
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let corners = self.corners();

        // 1. Ceiling Boundary
        primitives.push(Polyline2D::new(corners.to_vec(), true));

        // 2. 600x600mm Ceiling Grid Lines (Reflected Ceiling Plan convention)
        let min_x = corners[0].x.min(corners[1].x);
        let max_x = corners[0].x.max(corners[1].x);
        let min_y = corners[0].y.min(corners[2].y);
        let max_y = corners[0].y.max(corners[2].y);

        let step = self.grid_spacing.max(300.0);

        // Vertical grid lines
        let mut x = min_x + step;
        while x < max_x - 10.0 {
            primitives.push(Polyline2D::new(
                vec![Point2D::new(x, min_y), Point2D::new(x, max_y)],
                false,
            ));
            x += step;
        }

        // Horizontal grid lines
        let mut y = min_y + step;
        while y < max_y - 10.0 {
            primitives.push(Polyline2D::new(
                vec![Point2D::new(min_x, y), Point2D::new(max_x, y)],
                false,
            ));
            y += step;
        }

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let size = Vector3D::new(self.width.abs(), self.depth.abs(), self.thickness.abs());
        let center = Point3D::new(
            self.origin.x + self.width * 0.5,
            self.origin.y + self.depth * 0.5,
            self.elevation + self.thickness * 0.5,
        );
        TriangleMesh::create_box(size, center)
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = self.area_m2();

        // 天花板工程
        qto.add_item(CostLineItem::new(
            self.id,
            "Ceiling 天花板工程",
            format!("{} 連工帶料安裝 (淨高 {:.0}mm)", self.name, self.elevation),
            area_m2,
            "m²",
            1250.0,
        ));

        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.origin.x += delta.x;
        self.origin.y += delta.y;
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
