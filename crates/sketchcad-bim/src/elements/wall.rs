use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{extrude_polygon, BooleanEngine, OpeningVoid, Polyline2D, TriangleMesh};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub id: ElementId,
    pub name: String,
    pub start: Point2D,
    pub end: Point2D,
    pub thickness: f64, // mm, e.g. 200.0
    pub height: f64,    // mm, e.g. 3000.0
    pub base_z: f64,    // mm
    pub material: Material,
    pub openings: Vec<OpeningVoid>,
    pub hosted_openings: Vec<ElementId>,
}

impl Wall {
    pub fn new(start: Point2D, end: Point2D, thickness: f64, height: f64, material: Material) -> Self {
        Self {
            id: ElementId::new(),
            name: "Basic Wall 基礎牆體".to_string(),
            start,
            end,
            thickness,
            height,
            base_z: 0.0,
            material,
            openings: Vec::new(),
            hosted_openings: Vec::new(),
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> Vector2D {
        (self.end - self.start).normalize()
    }

    pub fn normal(&self) -> Vector2D {
        let dir = self.direction();
        Vector2D::new(-dir.y, dir.x)
    }

    /// Add an opening void (e.g. from a door or window) to cut through this wall
    pub fn add_opening(&mut self, opening: OpeningVoid) {
        self.openings.push(opening);
    }

    /// Computes the 4 corners of the 2D wall footprint polygon
    pub fn footprint_corners(&self) -> [Point2D; 4] {
        let half_t = self.thickness * 0.5;
        let n = self.normal() * half_t;
        [
            self.start + n, // p0
            self.end + n,   // p1
            self.end - n,   // p2
            self.start - n, // p3
        ]
    }

    /// Total area of all hosted door and window openings
    pub fn total_opening_area_m2(&self) -> f64 {
        self.openings
            .iter()
            .map(|op| (op.width / 1000.0) * (op.height / 1000.0))
            .sum()
    }
}

impl BimElement for Wall {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Wall
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let corners = self.footprint_corners();
        let poly = Polyline2D::new(corners.to_vec(), true);
        vec![poly]
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        if self.openings.is_empty() {
            let corners = self.footprint_corners();
            let poly = Polyline2D::new(corners.to_vec(), true);
            return extrude_polygon(&poly, self.base_z, self.height);
        }

        // Apply Boolean Subtraction to cut out doors & windows
        let solid_boxes = BooleanEngine::cut_wall_openings(
            self.length(),
            self.thickness,
            self.height,
            self.base_z,
            self.openings.clone(),
        );

        let dir = self.direction();
        let norm = self.normal();
        let mut composite = TriangleMesh::new();

        for local_box in solid_boxes {
            let box_mesh = TriangleMesh::create_box(local_box.size(), local_box.center());
            let mut transformed = box_mesh;

            // Transform each vertex from local wall space to global 3D space
            for p in &mut transformed.positions {
                let local_x = p.x;
                let local_y = p.y;
                let world_x = self.start.x + local_x * dir.x + local_y * norm.x;
                let world_y = self.start.y + local_x * dir.y + local_y * norm.y;
                *p = Point3D::new(world_x, world_y, p.z);
            }

            for n in &mut transformed.normals {
                let world_nx = n.x * dir.x + n.y * norm.x;
                let world_ny = n.x * dir.y + n.y * norm.y;
                *n = Vector3D::new(world_nx, world_ny, n.z);
            }

            composite.append(&transformed);
        }

        composite
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let length_m = self.length() / 1000.0;
        let height_m = self.height / 1000.0;
        let thickness_m = self.thickness / 1000.0;

        let gross_wall_area_m2 = length_m * height_m;
        let opening_area_m2 = self.total_opening_area_m2();
        let net_wall_area_m2 = (gross_wall_area_m2 - opening_area_m2).max(0.0);
        let net_wall_vol_m3 = net_wall_area_m2 * thickness_m;

        // Concrete / masonry volume line item (deducting opening voids)
        qto.add_item(CostLineItem::new(
            self.id,
            "Structure 結構工程",
            format!("{} 實體澆置 (扣除開口淨容積)", self.material.name),
            net_wall_vol_m3,
            "m³",
            self.material.cost_per_m3,
        ));

        // Formwork & Plastering surface item (both sides, deducting openings)
        let net_surface_area_m2 = net_wall_area_m2 * 2.0;
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 泥作與表面裝修",
            format!("{} 雙面粉刷與模板 (扣除開口淨面積)", self.name),
            net_surface_area_m2,
            "m²",
            850.0, // TWD per m2
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
        cloned.openings.clear();
        cloned.hosted_openings.clear();
        Box::new(cloned)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
