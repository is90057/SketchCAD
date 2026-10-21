use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Unitized Curtain Wall System (單元式/框架式玻璃幕牆系統)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurtainWall {
    pub id: ElementId,
    pub name: String,
    pub start: Point2D,
    pub end: Point2D,
    pub height: f64,             // mm, e.g. 3600.0
    pub base_z: f64,             // mm, e.g. 0.0
    pub glass_thickness: f64,    // mm, e.g. 28.0 (雙層中空Low-E安全玻璃)
    pub u_spacing: f64,          // mm, e.g. 1500.0 (豎梃分格間距)
    pub v_spacing: f64,          // mm, e.g. 1800.0 (橫梃水平分格間距)
    pub material: Material,
}

impl CurtainWall {
    pub fn new(
        start: Point2D,
        end: Point2D,
        height: f64,
        base_z: f64,
        u_spacing: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Glazed Curtain Wall 結構單元式玻璃幕牆".to_string(),
            start,
            end,
            height,
            base_z,
            glass_thickness: 28.0,
            u_spacing: u_spacing.clamp(600.0, 3000.0),
            v_spacing: 1800.0,
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn area_m2(&self) -> f64 {
        (self.length() * self.height) / 1_000_000.0
    }
}

impl BimElement for CurtainWall {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::CurtainWall
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let len = self.length();
        if len <= 1.0 {
            return primitives;
        }

        let dir = (self.end - self.start) / len;
        let norm = Vector2D::new(-dir.y, dir.x);
        let half_t = self.glass_thickness * 0.5 + 20.0; // visual representation width

        // Double glass boundary line
        let p0 = self.start - norm * half_t;
        let p1 = self.start + norm * half_t;
        let p2 = self.end + norm * half_t;
        let p3 = self.end - norm * half_t;
        primitives.push(Polyline2D::new(vec![p0, p1, p2, p3], true));

        // Centerline glass plane
        primitives.push(Polyline2D::new(vec![self.start, self.end], false));

        // Mullion tick marks at each grid division
        let num_bays = ((len / self.u_spacing).round() as usize).max(1);
        let bay_len = len / (num_bays as f64);
        for i in 0..=num_bays {
            let pt = self.start + dir * (i as f64 * bay_len);
            let m0 = pt - norm * (half_t + 25.0);
            let m1 = pt + norm * (half_t + 25.0);
            primitives.push(Polyline2D::new(vec![m0, m1], false));
        }

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
        let half_t = self.glass_thickness * 0.5;

        let z0 = self.base_z;
        let z1 = self.base_z + self.height;

        let p0 = Point3D::new(self.start.x - norm.x * half_t, self.start.y - norm.y * half_t, z0);
        let p1 = Point3D::new(self.start.x + norm.x * half_t, self.start.y + norm.y * half_t, z0);
        let p2 = Point3D::new(self.end.x + norm.x * half_t, self.end.y + norm.y * half_t, z0);
        let p3 = Point3D::new(self.end.x - norm.x * half_t, self.end.y - norm.y * half_t, z0);

        let p4 = Point3D::new(self.start.x - norm.x * half_t, self.start.y - norm.y * half_t, z1);
        let p5 = Point3D::new(self.start.x + norm.x * half_t, self.start.y + norm.y * half_t, z1);
        let p6 = Point3D::new(self.end.x + norm.x * half_t, self.end.y + norm.y * half_t, z1);
        let p7 = Point3D::new(self.end.x - norm.x * half_t, self.end.y - norm.y * half_t, z1);

        // Helper to add clean quads with edge flags suppressing internal diagonals
        let add_quad = |mesh: &mut TriangleMesh, p0: Point3D, p1: Point3D, p2: Point3D, p3: Point3D, normal: Vector3D| {
            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            mesh.edge_flags.push([true, true, false]);
            mesh.edge_flags.push([false, true, true]);
        };

        let norm_3d = Vector3D::new(norm.x, norm.y, 0.0);
        let dir_3d = Vector3D::new(dir.x, dir.y, 0.0);

        // Front glass face
        add_quad(&mut mesh, p1, p2, p6, p5, norm_3d);
        // Back glass face
        add_quad(&mut mesh, p3, p0, p4, p7, -norm_3d);
        // Left end
        add_quad(&mut mesh, p0, p1, p5, p4, -dir_3d);
        // Right end
        add_quad(&mut mesh, p2, p3, p7, p6, dir_3d);
        // Top
        add_quad(&mut mesh, p4, p5, p6, p7, Vector3D::Z);
        // Bottom
        add_quad(&mut mesh, p3, p2, p1, p0, -Vector3D::Z);

        // Vertical Mullion bars protruding on outer glass face
        let num_bays = ((len / self.u_spacing).round() as usize).max(1);
        let bay_len = len / (num_bays as f64);
        for i in 0..=num_bays {
            let center_2d = self.start + dir * (i as f64 * bay_len);
            let m_center = Point3D::new(center_2d.x, center_2d.y, (z0 + z1) * 0.5);
            let m_box = TriangleMesh::create_box(
                Vector3D::new(60.0, 120.0, self.height),
                m_center,
            );
            mesh.append(&m_box);
        }

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = self.area_m2();

        // 雙層中空 Low-E 氣密玻璃單元
        qto.add_item(CostLineItem::new(
            self.id,
            "Curtain Wall 帷幕工程",
            format!("{} 雙層中空Low-E鋼化夾膠玻璃單元 (分格 {:.0}mm)", self.name, self.u_spacing),
            area_m2,
            "m²",
            6800.0,
        ));

        // 陽極氧化斷熱鋁合金框架與結構膠
        qto.add_item(CostLineItem::new(
            self.id,
            "Curtain Wall 帷幕工程",
            format!("{} 斷熱鋁合金立柱與橫樑骨架、結構密封膠", self.name),
            area_m2,
            "m²",
            4200.0,
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

/// Curtain Grid Layout (幕牆模矩網格劃分)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurtainGrid {
    pub id: ElementId,
    pub name: String,
    pub curtain_wall_id: Option<ElementId>,
    pub origin: Point2D,
    pub width: f64,          // mm (total span)
    pub height: f64,         // mm (total height)
    pub base_z: f64,         // mm
    pub u_spacing: f64,      // mm (horizontal module spacing)
    pub v_spacing: f64,      // mm (vertical module spacing)
}

impl CurtainGrid {
    pub fn new(
        curtain_wall_id: Option<ElementId>,
        origin: Point2D,
        width: f64,
        height: f64,
        base_z: f64,
        u_spacing: f64,
        v_spacing: f64,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Parametric Curtain Grid 幕牆模矩網格".to_string(),
            curtain_wall_id,
            origin,
            width,
            height,
            base_z,
            u_spacing: u_spacing.clamp(500.0, 3000.0),
            v_spacing: v_spacing.clamp(500.0, 3000.0),
        }
    }
}

impl BimElement for CurtainGrid {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::CurtainGrid
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let p0 = self.origin;
        let p1 = Point2D::new(self.origin.x + self.width, self.origin.y);
        let p2 = Point2D::new(self.origin.x + self.width, self.origin.y + 100.0);
        let p3 = Point2D::new(self.origin.x, self.origin.y + 100.0);
        primitives.push(Polyline2D::new(vec![p0, p1, p2, p3], true));

        // Division ticks
        let num_u = ((self.width / self.u_spacing).round() as usize).max(1);
        let u_step = self.width / (num_u as f64);
        for i in 0..=num_u {
            let x = self.origin.x + (i as f64 * u_step);
            primitives.push(Polyline2D::new(
                vec![Point2D::new(x, self.origin.y - 40.0), Point2D::new(x, self.origin.y + 140.0)],
                false,
            ));
        }

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        // Grid wireframe guide box
        let size = Vector3D::new(self.width, 40.0, self.height);
        let center = Point3D::new(
            self.origin.x + self.width * 0.5,
            self.origin.y,
            self.base_z + self.height * 0.5,
        );
        TriangleMesh::create_box(size, center)
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let num_u = ((self.width / self.u_spacing).round() as usize).max(1);
        let num_v = ((self.height / self.v_spacing).round() as usize).max(1);
        let total_nodes = (num_u + 1) * (num_v + 1);

        qto.add_item(CostLineItem::new(
            self.id,
            "Curtain Wall 帷幕工程",
            format!("{} 模矩放樣與精確節點測量 (共 {} 個網格節點)", self.name, total_nodes),
            total_nodes as f64,
            "節點",
            450.0,
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

/// Structural Mullion & Transom (帷幕豎梃與橫樑立柱)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mullion {
    pub id: ElementId,
    pub name: String,
    pub start: Point3D,
    pub end: Point3D,
    pub profile_width: f64,  // mm, e.g. 60.0
    pub profile_depth: f64,  // mm, e.g. 150.0
    pub material: Material,
}

impl Mullion {
    pub fn new(
        start: Point3D,
        end: Point3D,
        profile_width: f64,
        profile_depth: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Aluminum Mullion 結構鋁合金豎梃立柱".to_string(),
            start,
            end,
            profile_width,
            profile_depth,
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }
}

impl BimElement for Mullion {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Mullion
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let hw = self.profile_width * 0.5;
        let hd = self.profile_depth * 0.5;

        let mid = (self.start + self.end) * 0.5;
        let p0 = Point2D::new(mid.x - hw, mid.y - hd);
        let p1 = Point2D::new(mid.x + hw, mid.y - hd);
        let p2 = Point2D::new(mid.x + hw, mid.y + hd);
        let p3 = Point2D::new(mid.x - hw, mid.y + hd);

        primitives.push(Polyline2D::new(vec![p0, p1, p2, p3], true));
        primitives.push(Polyline2D::new(vec![p0, p2], false));
        primitives.push(Polyline2D::new(vec![p1, p3], false));

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let len = self.length();
        let center = (self.start + self.end) * 0.5;
        let v = self.end - self.start;
        let is_vertical = v.z.abs() > v.x.abs().max(v.y.abs());

        let size = if is_vertical {
            Vector3D::new(self.profile_width, self.profile_depth, len)
        } else {
            Vector3D::new(len, self.profile_depth, self.profile_width)
        };

        TriangleMesh::create_box(size, center)
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let len_m = self.length() / 1000.0;

        qto.add_item(CostLineItem::new(
            self.id,
            "Curtain Wall 帷幕工程",
            format!("{} 擠型鋁合金 ({}x{}mm)", self.name, self.profile_width as u32, self.profile_depth as u32),
            len_m,
            "m",
            1650.0,
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
