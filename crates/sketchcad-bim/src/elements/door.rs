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

impl DoorSwing {
    pub const ALL: [DoorSwing; 4] = [
        DoorSwing::InwardLeft,
        DoorSwing::InwardRight,
        DoorSwing::OutwardLeft,
        DoorSwing::OutwardRight,
    ];

    /// Hinge at the start of the opening (left, seen along the wall direction)
    pub fn is_left(self) -> bool {
        matches!(self, DoorSwing::InwardLeft | DoorSwing::OutwardLeft)
    }

    /// Leaf opens toward the wall's left-hand normal side
    pub fn is_inward(self) -> bool {
        matches!(self, DoorSwing::InwardLeft | DoorSwing::InwardRight)
    }

    pub fn from_parts(left: bool, inward: bool) -> Self {
        match (left, inward) {
            (true, true) => DoorSwing::InwardLeft,
            (false, true) => DoorSwing::InwardRight,
            (true, false) => DoorSwing::OutwardLeft,
            (false, false) => DoorSwing::OutwardRight,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DoorSwing::InwardLeft => "左開・內開",
            DoorSwing::InwardRight => "右開・內開",
            DoorSwing::OutwardLeft => "左開・外開",
            DoorSwing::OutwardRight => "右開・外開",
        }
    }
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

        // Hinge at the opening start (left) or end (right); leaf swings to the normal side
        // (inward) or the opposite side (outward)
        let start = self.world_pos;
        let end = start + dir * self.width;
        let (hinge, along) = if self.swing.is_left() { (start, dir) } else { (end, -dir) };
        let side = if self.swing.is_inward() { norm } else { -norm };

        // 1. Threshold line
        primitives.push(Polyline2D::new(vec![start, end], false));

        // 2. Leaf line (open at 90 degrees)
        let leaf_end = hinge + side * self.width;
        primitives.push(Polyline2D::new(vec![hinge, leaf_end], false));

        // 3. Swing Arc (approximated with 16 segments)
        let segments = 16;
        let mut arc_verts = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let theta = (std::f64::consts::PI * 0.5) * (i as f64 / segments as f64);
            let pt = hinge + along * (self.width * theta.cos()) + side * (self.width * theta.sin());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_end(swing: DoorSwing) -> Point2D {
        let mut door = Door::new(ElementId::new(), 0.0, 900.0, 2100.0, Material::wood(), 0.0);
        door.set_transform(Point2D::new(1000.0, 0.0), Vector2D::X);
        door.swing = swing;
        door.to_2d_primitives()[1].vertices[1]
    }

    #[test]
    fn swing_moves_hinge_and_leaf_side() {
        // Wall along +X, opening from x=1000 to x=1900, inward = +Y
        assert_eq!(leaf_end(DoorSwing::InwardLeft), Point2D::new(1000.0, 900.0));
        assert_eq!(leaf_end(DoorSwing::InwardRight), Point2D::new(1900.0, 900.0));
        assert_eq!(leaf_end(DoorSwing::OutwardLeft), Point2D::new(1000.0, -900.0));
        assert_eq!(leaf_end(DoorSwing::OutwardRight), Point2D::new(1900.0, -900.0));
        for swing in DoorSwing::ALL {
            assert_eq!(DoorSwing::from_parts(swing.is_left(), swing.is_inward()), swing);
        }
    }
}
