use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Architectural & Structural Stairs (直通/轉角樓梯)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stairs {
    pub id: ElementId,
    pub name: String,
    pub start: Point2D,      // Bottom of flight (start of run)
    pub end: Point2D,        // Top of flight (end of run)
    pub width: f64,          // Flight width in mm (e.g. 1000 - 1200mm)
    pub total_rise: f64,     // Total elevation gain in mm (e.g. 3000mm)
    pub base_z: f64,         // Elevation of bottom landing in mm (e.g. 0.0)
    pub num_steps: usize,    // Number of risers/treads (e.g. 16 - 18)
    pub material: Material,
}

impl Stairs {
    pub fn new(
        start: Point2D,
        end: Point2D,
        width: f64,
        total_rise: f64,
        num_steps: usize,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Straight Run Stairs 直通樓梯".to_string(),
            start,
            end,
            width,
            total_rise,
            base_z: 0.0,
            num_steps: num_steps.max(1),
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> Vector2D {
        let v = self.end - self.start;
        let len = v.length();
        if len > 1e-4 {
            v / len
        } else {
            Vector2D::X
        }
    }

    pub fn normal(&self) -> Vector2D {
        let dir = self.direction();
        Vector2D::new(-dir.y, dir.x)
    }

    pub fn tread_depth(&self) -> f64 {
        self.length() / (self.num_steps as f64)
    }

    pub fn riser_height(&self) -> f64 {
        self.total_rise / (self.num_steps as f64)
    }
}

impl BimElement for Stairs {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Stairs
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let dir = self.direction();
        let norm = self.normal();
        let half_w = self.width * 0.5;
        let tread_d = self.tread_depth();

        // 1. Boundary of the entire stair flight
        let p0 = self.start - norm * half_w;
        let p1 = self.start + norm * half_w;
        let p2 = self.end + norm * half_w;
        let p3 = self.end - norm * half_w;
        primitives.push(Polyline2D::new(vec![p0, p1, p2, p3], true));

        // 2. Step lines (treads)
        for i in 1..self.num_steps {
            let pt_on_axis = self.start + dir * (i as f64 * tread_d);
            let s_left = pt_on_axis - norm * half_w;
            let s_right = pt_on_axis + norm * half_w;
            primitives.push(Polyline2D::new(vec![s_left, s_right], false));
        }

        // 3. Central Walkline arrow (UP indication)
        let walk_start = self.start + dir * (tread_d * 0.5);
        let walk_end = self.end - dir * (tread_d * 0.5);
        primitives.push(Polyline2D::new(vec![walk_start, walk_end], false));

        // Arrow head pointing to walk_end
        let arrow_len = 250.0;
        let arrow_left = walk_end - dir * arrow_len + norm * (arrow_len * 0.5);
        let arrow_right = walk_end - dir * arrow_len - norm * (arrow_len * 0.5);
        primitives.push(Polyline2D::new(vec![arrow_left, walk_end, arrow_right], false));

        // 4. Starting circle indicator (small diamond/circle at bottom step)
        let circle_r = 100.0;
        let c0 = walk_start + dir * circle_r;
        let c1 = walk_start + norm * circle_r;
        let c2 = walk_start - dir * circle_r;
        let c3 = walk_start - norm * circle_r;
        primitives.push(Polyline2D::new(vec![c0, c1, c2, c3], true));

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let dir = self.direction();
        let norm = self.normal();
        let half_w = self.width * 0.5;
        let tread_d = self.tread_depth();
        let riser_h = self.riser_height();

        let dir_3d = Vector3D::new(dir.x, dir.y, 0.0);
        let norm_3d = Vector3D::new(norm.x, norm.y, 0.0);
        let up_3d = Vector3D::Z;

        // Helper to add an oriented quad with clean boundary edge stroke flags (suppressing diagonal triangulation)
        // Vertices p0, p1, p2, p3 MUST be in counter-clockwise order around the outward-pointing normal!
        let add_quad = |mesh: &mut TriangleMesh,
                        p0: Point3D,
                        p1: Point3D,
                        p2: Point3D,
                        p3: Point3D,
                        normal: Vector3D| {
            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);

            // Triangle 1: p0 -> p1 -> p2 (edge 0->1: true, edge 1->2: true, edge 2->0: false [internal diagonal])
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            mesh.edge_flags.push([true, true, false]);

            // Triangle 2: p0 -> p2 -> p3 (edge 0->2: false [internal diagonal], edge 2->3: true, edge 3->0: true)
            mesh.indices.extend_from_slice(&[base, base + 2, base + 3]);
            mesh.edge_flags.push([false, true, true]);
        };

        // 1. Generate clean Step Treads and Risers for each step (層次分明、帶踏步凸緣的獨立階梯)
        let nosing = 25.0; // 25mm nosing overhang over the riser
        let waist_drop = 120.0; // Concrete waist slab depth below steps

        for i in 0..self.num_steps {
            let p_front = self.start + dir * (i as f64 * tread_d);
            let p_back = self.start + dir * ((i + 1) as f64 * tread_d);
            let z_prev = self.base_z + i as f64 * riser_h;
            let z_top = self.base_z + (i + 1) as f64 * riser_h;

            // Riser (Vertical face facing -dir_3d towards bottom approaching stairs)
            // CCW order: bottom-right -> bottom-left -> top-left -> top-right
            let r0 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_prev);
            let r1 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_prev);
            let r2 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_top);
            let r3 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_top);
            add_quad(&mut mesh, r0, r1, r2, r3, -dir_3d);

            // Tread (Horizontal face with nosing overhang, facing +Z upward)
            // CCW order: front-left -> back-left -> back-right -> front-right
            let p_nosing_front = p_front - dir * nosing;
            let t0 = Point3D::new(p_nosing_front.x - norm.x * half_w, p_nosing_front.y - norm.y * half_w, z_top);
            let t1 = Point3D::new(p_back.x - norm.x * half_w, p_back.y - norm.y * half_w, z_top);
            let t2 = Point3D::new(p_back.x + norm.x * half_w, p_back.y + norm.y * half_w, z_top);
            let t3 = Point3D::new(p_nosing_front.x + norm.x * half_w, p_nosing_front.y + norm.y * half_w, z_top);
            add_quad(&mut mesh, t0, t1, t2, t3, up_3d);

            // Step Left Flank (facing -norm_3d to the left)
            // CCW order: bottom-front -> top-front -> top-back -> bottom-back
            let fl0 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_prev);
            let fl1 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_top);
            let fl2 = Point3D::new(p_back.x - norm.x * half_w, p_back.y - norm.y * half_w, z_top);
            let fl3 = Point3D::new(p_back.x - norm.x * half_w, p_back.y - norm.y * half_w, z_prev);
            add_quad(&mut mesh, fl0, fl1, fl2, fl3, -norm_3d);

            // Step Right Flank (facing +norm_3d to the right)
            // CCW order: bottom-front -> bottom-back -> top-back -> top-front
            let fr0 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_prev);
            let fr1 = Point3D::new(p_back.x + norm.x * half_w, p_back.y + norm.y * half_w, z_prev);
            let fr2 = Point3D::new(p_back.x + norm.x * half_w, p_back.y + norm.y * half_w, z_top);
            let fr3 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_top);
            add_quad(&mut mesh, fr0, fr1, fr2, fr3, norm_3d);

            // Stepped Side Support Wall below step to waist slab
            let z_soffit_front = (self.base_z + i as f64 * riser_h - waist_drop).max(self.base_z - waist_drop);
            let z_soffit_back = self.base_z + (i + 1) as f64 * riser_h - waist_drop;

            // Left side skirt
            let lsk0 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_soffit_front);
            let lsk1 = Point3D::new(p_front.x - norm.x * half_w, p_front.y - norm.y * half_w, z_prev);
            let lsk2 = Point3D::new(p_back.x - norm.x * half_w, p_back.y - norm.y * half_w, z_prev);
            let lsk3 = Point3D::new(p_back.x - norm.x * half_w, p_back.y - norm.y * half_w, z_soffit_back);
            add_quad(&mut mesh, lsk0, lsk1, lsk2, lsk3, -norm_3d);

            // Right side skirt
            let rsk0 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_soffit_front);
            let rsk1 = Point3D::new(p_back.x + norm.x * half_w, p_back.y + norm.y * half_w, z_soffit_back);
            let rsk2 = Point3D::new(p_back.x + norm.x * half_w, p_back.y + norm.y * half_w, z_prev);
            let rsk3 = Point3D::new(p_front.x + norm.x * half_w, p_front.y + norm.y * half_w, z_prev);
            add_quad(&mut mesh, rsk0, rsk1, rsk2, rsk3, norm_3d);
        }

        // 2. Monolithic Underside Soffit (結構梯板斜向底面，位於階梯下方 120mm，絕不遮蔽踏步)
        let total_top_z = self.base_z + self.num_steps as f64 * riser_h;
        let p_end = self.end;
        let s0 = Point3D::new(self.start.x + norm.x * half_w, self.start.y + norm.y * half_w, self.base_z - waist_drop);
        let s1 = Point3D::new(p_end.x + norm.x * half_w, p_end.y + norm.y * half_w, total_top_z - waist_drop);
        let s2 = Point3D::new(p_end.x - norm.x * half_w, p_end.y - norm.y * half_w, total_top_z - waist_drop);
        let s3 = Point3D::new(self.start.x - norm.x * half_w, self.start.y - norm.y * half_w, self.base_z - waist_drop);
        let soffit_norm = (dir_3d * riser_h - up_3d * tread_d).cross(norm_3d).normalize();
        add_quad(&mut mesh, s0, s1, s2, s3, -soffit_norm);

        // 3. Top Landing Back Vertical Face
        let bk0 = Point3D::new(p_end.x - norm.x * half_w, p_end.y - norm.y * half_w, total_top_z - waist_drop);
        let bk1 = Point3D::new(p_end.x + norm.x * half_w, p_end.y + norm.y * half_w, total_top_z - waist_drop);
        let bk2 = Point3D::new(p_end.x + norm.x * half_w, p_end.y + norm.y * half_w, total_top_z);
        let bk3 = Point3D::new(p_end.x - norm.x * half_w, p_end.y - norm.y * half_w, total_top_z);
        add_quad(&mut mesh, bk0, bk1, bk2, bk3, dir_3d);

        // 4. Bottom Landing Front Vertical Face
        let ft0 = Point3D::new(self.start.x + norm.x * half_w, self.start.y + norm.y * half_w, self.base_z - waist_drop);
        let ft1 = Point3D::new(self.start.x - norm.x * half_w, self.start.y - norm.y * half_w, self.base_z - waist_drop);
        let ft2 = Point3D::new(self.start.x - norm.x * half_w, self.start.y - norm.y * half_w, self.base_z);
        let ft3 = Point3D::new(self.start.x + norm.x * half_w, self.start.y + norm.y * half_w, self.base_z);
        add_quad(&mut mesh, ft0, ft1, ft2, ft3, -dir_3d);

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let tread_d = self.tread_depth() / 1000.0;
        let riser_h = self.riser_height() / 1000.0;
        let width_m = self.width / 1000.0;

        // Sum of volumes: width * tread_depth * (1 + 2 + ... + N) * riser_height
        let total_vol_m3: f64 = (1..=self.num_steps)
            .map(|i| width_m * tread_d * (i as f64 * riser_h))
            .sum();

        // 結構工程: 樓梯級高結構澆置
        qto.add_item(CostLineItem::new(
            self.id,
            "Structure 結構工程",
            format!("{} 鋼筋混凝土澆置 ({} 階, 總升高 {:.0}mm)", self.name, self.num_steps, self.total_rise),
            total_vol_m3,
            "m³",
            4200.0,
        ));

        // 泥作裝修: 踏面金剛砂防滑條與收邊
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 泥作與表面裝修",
            format!("{} 踏步防滑條與金剛砂泥作粉光", self.name),
            self.num_steps as f64,
            "階",
            650.0,
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
