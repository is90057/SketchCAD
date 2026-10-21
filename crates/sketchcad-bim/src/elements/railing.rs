use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Architectural Railing & Balustrade (金屬/玻璃安全欄杆扶手)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Railing {
    pub id: ElementId,
    pub name: String,
    pub start: Point3D,               // Starting 3D coordinate (base point)
    pub end: Point3D,                 // Ending 3D coordinate (base point)
    pub height: f64,                  // Total railing height in mm (default 1100.0mm)
    pub handrail_width: f64,          // Top handrail width in mm (default 50.0mm)
    pub post_spacing: f64,            // Baluster post interval in mm (default 900.0mm)
    pub post_size: f64,               // Baluster post square width in mm (default 40.0mm)
    pub has_glass_infill: bool,       // Laminated safety glass infill panels
    pub material: Material,
}

impl Railing {
    pub fn new(
        start: Point3D,
        end: Point3D,
        height: f64,
        material: Material,
    ) -> Self {
        Self {
            id: ElementId::new(),
            name: "Glass & Steel Railing 玻璃金屬欄杆扶手".to_string(),
            start,
            end,
            height: height.max(600.0),
            handrail_width: 50.0,
            post_spacing: 900.0,
            post_size: 40.0,
            has_glass_infill: true,
            material,
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn planar_length(&self) -> f64 {
        let p0 = Point2D::new(self.start.x, self.start.y);
        let p1 = Point2D::new(self.end.x, self.end.y);
        p0.distance(p1)
    }

    pub fn direction_2d(&self) -> Vector2D {
        let v = Vector2D::new(self.end.x - self.start.x, self.end.y - self.start.y);
        let len = v.length();
        if len > 1e-4 {
            v / len
        } else {
            Vector2D::X
        }
    }

    pub fn normal_2d(&self) -> Vector2D {
        let d = self.direction_2d();
        Vector2D::new(-d.y, d.x)
    }
}

impl BimElement for Railing {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Railing
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut primitives = Vec::new();
        let p_start = Point2D::new(self.start.x, self.start.y);
        let p_end = Point2D::new(self.end.x, self.end.y);
        let norm = self.normal_2d();
        let half_w = self.handrail_width * 0.5;

        // 1. Double lines representing top handrail path in plan view
        let left_0 = p_start - norm * half_w;
        let left_1 = p_end - norm * half_w;
        let right_0 = p_start + norm * half_w;
        let right_1 = p_end + norm * half_w;
        primitives.push(Polyline2D::new(vec![left_0, left_1], false));
        primitives.push(Polyline2D::new(vec![right_0, right_1], false));

        // 2. Baluster post footprints
        let len = self.planar_length();
        let num_intervals = ((len / self.post_spacing).ceil() as usize).max(1);
        let post_half = self.post_size * 0.5;

        for i in 0..=num_intervals {
            let t = (i as f64) / (num_intervals as f64);
            let pt = p_start + (p_end - p_start) * t;
            let c0 = pt + Point2D::new(-post_half, -post_half);
            let c1 = pt + Point2D::new(post_half, -post_half);
            let c2 = pt + Point2D::new(post_half, post_half);
            let c3 = pt + Point2D::new(-post_half, post_half);
            primitives.push(Polyline2D::new(vec![c0, c1, c2, c3], true));
        }

        primitives
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let total_dist = self.length();
        if total_dist < 10.0 {
            return mesh;
        }

        let dir_3d = (self.end - self.start) / total_dist;
        let norm_2d = self.normal_2d();
        let norm_3d = Vector3D::new(norm_2d.x, norm_2d.y, 0.0);
        let along_3d = Vector3D::new(-norm_2d.y, norm_2d.x, 0.0);

        let num_posts = ((total_dist / self.post_spacing).ceil() as usize).max(1);

        // 1. Vertical Posts (Balusters)
        for i in 0..=num_posts {
            let t = (i as f64) / (num_posts as f64);
            let base_pt = self.start + (self.end - self.start) * t;
            let post_h = self.height;

            let post_mesh = TriangleMesh::create_oriented_box(
                base_pt,
                base_pt + Vector3D::new(0.0, 0.0, post_h),
                norm_3d * self.post_size,
                along_3d * self.post_size,
            );
            mesh.append(&post_mesh);
        }

        // 2. Top Handrail Profile (sloping with the railing from start to end)
        let top_start = self.start + Vector3D::new(0.0, 0.0, self.height - 25.0);
        let top_end = self.end + Vector3D::new(0.0, 0.0, self.height - 25.0);

        let rail_mesh = TriangleMesh::create_oriented_box(
            top_start,
            top_end,
            norm_3d * self.handrail_width,
            Vector3D::new(0.0, 0.0, 50.0),
        );
        mesh.append(&rail_mesh);

        // 3. Bottom Guard Rail (100mm above ground/tread)
        let bot_start = self.start + Vector3D::new(0.0, 0.0, 100.0);
        let bot_end = self.end + Vector3D::new(0.0, 0.0, 100.0);

        let bot_rail_mesh = TriangleMesh::create_oriented_box(
            bot_start,
            bot_end,
            norm_3d * (self.handrail_width * 0.6),
            Vector3D::new(0.0, 0.0, 30.0),
        );
        mesh.append(&bot_rail_mesh);

        // 4. Infill Safety Glass Panels (between posts)
        if self.has_glass_infill && num_posts >= 1 {
            let glass_h = self.height - 200.0;
            if glass_h > 100.0 {
                for i in 0..num_posts {
                    let t0 = (i as f64) / (num_posts as f64);
                    let t1 = ((i + 1) as f64) / (num_posts as f64);
                    let p0 = self.start + (self.end - self.start) * t0;
                    let p1 = self.start + (self.end - self.start) * t1;

                    let panel_len = p0.distance(p1) - self.post_size * 1.2;
                    if panel_len > 50.0 {
                        let offset_dir = dir_3d * (self.post_size * 0.6);
                        let g_start = p0 + offset_dir + Vector3D::new(0.0, 0.0, 100.0 + glass_h * 0.5);
                        let g_end = p1 - offset_dir + Vector3D::new(0.0, 0.0, 100.0 + glass_h * 0.5);
                        let glass_mesh = TriangleMesh::create_oriented_box(
                            g_start,
                            g_end,
                            norm_3d * 12.0,
                            Vector3D::new(0.0, 0.0, glass_h),
                        );
                        mesh.append(&glass_mesh);
                    }
                }
            }
        }

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let len_m = self.length() / 1000.0;
        let num_posts = ((self.length() / self.post_spacing).ceil() as usize).max(1) + 1;

        // 欄杆扶手工程
        qto.add_item(CostLineItem::new(
            self.id,
            "Interior & Finishes 金屬與裝修工程",
            format!("{} (高度 {:.0}mm, 立柱 {} 支, 長度 {:.2}m)", self.name, self.height, num_posts, len_m),
            len_m,
            "m",
            3800.0,
        ));

        if self.has_glass_infill {
            let glass_area = len_m * (self.height - 200.0) / 1000.0;
            qto.add_item(CostLineItem::new(
                self.id,
                "Doors & Windows 玻璃工程",
                format!("{} 10mm 強化安全夾膠玻璃隔板", self.name),
                glass_area,
                "m²",
                2600.0,
            ));
        }

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
