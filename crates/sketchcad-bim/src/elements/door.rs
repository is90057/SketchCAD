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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoorSwing {
    InwardLeft,
    InwardRight,
    OutwardLeft,
    OutwardRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Door {
    pub id: ElementId,
    pub name: String,
    pub host_wall_id: ElementId,
    pub offset_along_wall: f64, // mm from wall start
    pub width: f64,             // mm, e.g. 900.0
    pub height: f64,            // mm, e.g. 2100.0
    pub thickness: f64,         // mm, e.g. 40.0
    pub swing: DoorSwing,
    pub material: Material,
    pub unit_price: f64,        // direct hardware + installation cost
    #[serde(default = "default_pos")]
    pub world_pos: Point2D,
    #[serde(default = "default_dir")]
    pub wall_dir: Vector2D,
}

impl Door {
    pub fn new(
        host_wall_id: ElementId,
        offset_along_wall: f64,
        width: f64,
        height: f64,
        material: Material,
        unit_price: f64,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Single Flush Door 單扇平開門".to_string(),
            host_wall_id,
            offset_along_wall,
            width,
            height,
            thickness: 45.0,
            swing: DoorSwing::InwardLeft,
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

impl BimElement for Door {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Door
    }

    fn dependencies(&self) -> Vec<ElementId> {
        vec![self.host_wall_id]
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let dir = if self.wall_dir.length_squared() > 1e-6 {
            self.wall_dir.normalize()
        } else {
            Vector2D::X
        };
        let norm = Vector2D::new(-dir.y, dir.x);

        // 1. Threshold line
        let hinge = self.world_pos;
        let frame_end = hinge + dir * self.width;
        primitives.push(Polyline2D::new(vec![hinge, frame_end], false));

        // 2. Leaf line (open at 90 degrees)
        let leaf_end = hinge + norm * self.width;
        primitives.push(Polyline2D::new(vec![hinge, leaf_end], false));

        // 3. Swing Arc (approximated with 16 segments)
        let segments = 16;
        let mut arc_verts = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let theta = (std::f64::consts::PI * 0.5) * (i as f64 / segments as f64);
            let pt = hinge + dir * (self.width * theta.cos()) + norm * (self.width * theta.sin());
            arc_verts.push(pt);
        }
        primitives.push(Polyline2D::new(arc_verts, false));

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let dir = if self.wall_dir.length_squared() > 1e-6 {
            self.wall_dir.normalize()
        } else {
            Vector2D::X
        };
        let norm = Vector2D::new(-dir.y, dir.x);

        let center = Point3D::new(
            self.world_pos.x + dir.x * (self.width * 0.5),
            self.world_pos.y + dir.y * (self.width * 0.5),
            self.height * 0.5,
        );

        let mut box_mesh = TriangleMesh::create_box(
            Vector3D::new(self.width, self.thickness, self.height),
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
        qto.add_item(CostLineItem::new(
            self.id,
            "Doors & Windows 門窗工程",
            format!("{} ({}x{}mm)", self.name, self.width as u32, self.height as u32),
            1.0,
            "組",
            self.unit_price,
        ));
        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.world_pos.x += delta.x;
        self.world_pos.y += delta.y;
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
