use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

fn default_pos() -> Point2D {
    Point2D::ZERO
}

fn default_dir() -> Vector2D {
    Vector2D::X
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub id: ElementId,
    pub name: String,
    pub host_wall_id: ElementId,
    pub offset_along_wall: f64, // mm from wall start
    pub sill_height: f64,       // mm from floor level, e.g. 900.0
    pub width: f64,             // mm, e.g. 1500.0
    pub height: f64,            // mm, e.g. 1200.0
    pub material: Material,
    pub unit_price: f64,
    #[serde(default = "default_pos")]
    pub world_pos: Point2D,
    #[serde(default = "default_dir")]
    pub wall_dir: Vector2D,
}

impl Window {
    pub fn new(
        host_wall_id: ElementId,
        offset_along_wall: f64,
        sill_height: f64,
        width: f64,
        height: f64,
        material: Material,
        unit_price: f64,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Double Glazed Window 氣密鋁門窗".to_string(),
            host_wall_id,
            offset_along_wall,
            sill_height,
            width,
            height,
            material,
            unit_price,
            world_pos: Point2D::ZERO,
            wall_dir: Vector2D::X,
        }
    }

    pub fn set_transform(&mut self, pos: Point2D, dir: Vector2D) {
        self.world_pos = pos;
        self.wall_dir = dir;
    }
}

impl BimElement for Window {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Window
    }

    fn dependencies(&self) -> Vec<ElementId> {
        vec![self.host_wall_id]
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        // Standard CAD window 2D symbol: outer frame rectangle + inner glass lines
        let dir = if self.wall_dir.length_squared() > 1e-6 {
            self.wall_dir.normalize()
        } else {
            Vector2D::X
        };
        let norm = Vector2D::new(-dir.y, dir.x);
        let half_w = self.width * 0.5;
        let half_d = 80.0; // 160mm total thickness

        // Outer window frame
        let p0 = self.world_pos - dir * half_w - norm * half_d;
        let p1 = self.world_pos + dir * half_w - norm * half_d;
        let p2 = self.world_pos + dir * half_w + norm * half_d;
        let p3 = self.world_pos - dir * half_w + norm * half_d;
        let outer = Polyline2D::new(vec![p0, p1, p2, p3], true);

        // Center glass line
        let g0 = self.world_pos - dir * half_w;
        let g1 = self.world_pos + dir * half_w;
        let glass_line = Polyline2D::new(vec![g0, g1], false);

        vec![outer, glass_line]
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let dir = if self.wall_dir.length_squared() > 1e-6 {
            self.wall_dir.normalize()
        } else {
            Vector2D::X
        };
        let norm = Vector2D::new(-dir.y, dir.x);

        let center = Point3D::new(
            self.world_pos.x,
            self.world_pos.y,
            self.sill_height + self.height * 0.5,
        );

        let mut box_mesh = TriangleMesh::create_box(
            Vector3D::new(self.width, 100.0, self.height),
            Point3D::new(0.0, 0.0, 0.0),
        );

        for p in &mut box_mesh.positions {
            let lx = p.x;
            let ly = p.y;
            p.x = center.x + lx * dir.x + ly * norm.x;
            p.y = center.y + lx * dir.y + ly * norm.y;
            p.z = center.z + p.z;
        }

        for n in &mut box_mesh.normals {
            let lnx = n.x;
            let lny = n.y;
            n.x = lnx * dir.x + lny * norm.x;
            n.y = lnx * dir.y + lny * norm.y;
        }

        box_mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = (self.width / 1000.0) * (self.height / 1000.0);
        qto.add_item(CostLineItem::new(
            self.id,
            "Doors & Windows 門窗工程",
            format!("{} ({:.2} m²)", self.name, area_m2),
            1.0,
            "樘",
            self.unit_price,
        ));
        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.world_pos.x += delta.x;
        self.world_pos.y += delta.y;
        self.sill_height += delta.z;
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
