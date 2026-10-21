use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FurnitureKind {
    Bed,            // 雙人床 / 加大主臥床
    Sofa,           // 客廳舒適沙發組
    DiningTable,    // 實木餐桌椅組 (6人)
    KitchenCabinet, // 系統廚具與水槽料理台
    Toilet,         // 衛浴單體馬桶與水箱
    Wardrobe,       // 臥室大容量收納衣櫃
    Desk,           // 書房辦公書桌椅組
}

impl FurnitureKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            FurnitureKind::Bed => "雙人床組 (Double Bed)",
            FurnitureKind::Sofa => "客廳沙發 (Living Sofa)",
            FurnitureKind::DiningTable => "餐桌椅組 (Dining Table)",
            FurnitureKind::KitchenCabinet => "系統廚具水槽 (Kitchen Counter)",
            FurnitureKind::Toilet => "衛浴馬桶 (Toilet Fixture)",
            FurnitureKind::Wardrobe => "收納大衣櫃 (Wardrobe Cabinet)",
            FurnitureKind::Desk => "書房書桌椅 (Study Desk)",
        }
    }

    pub fn default_dimensions(&self) -> (f64, f64, f64) {
        // (width, depth, height) in mm
        match self {
            FurnitureKind::Bed => (1800.0, 2000.0, 950.0),
            FurnitureKind::Sofa => (2200.0, 900.0, 850.0),
            FurnitureKind::DiningTable => (1600.0, 900.0, 750.0),
            FurnitureKind::KitchenCabinet => (2400.0, 650.0, 850.0),
            FurnitureKind::Toilet => (500.0, 700.0, 780.0),
            FurnitureKind::Wardrobe => (1800.0, 600.0, 2400.0),
            FurnitureKind::Desk => (1400.0, 700.0, 750.0),
        }
    }

    pub fn default_cost(&self) -> f64 {
        match self {
            FurnitureKind::Bed => 32000.0,
            FurnitureKind::Sofa => 45000.0,
            FurnitureKind::DiningTable => 28000.0,
            FurnitureKind::KitchenCabinet => 85000.0,
            FurnitureKind::Toilet => 18000.0,
            FurnitureKind::Wardrobe => 48000.0,
            FurnitureKind::Desk => 16000.0,
        }
    }
}

/// Parametric Interior Furniture & Fixture Smart Object (室內家具與櫥櫃設備)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Furniture {
    pub id: ElementId,
    pub name: String,
    pub kind: FurnitureKind,
    pub position: Point2D,
    pub base_z: f64,
    pub rotation_deg: f64, // degrees (0.0 .. 360.0)
    pub width: f64,        // mm
    pub depth: f64,        // mm
    pub height: f64,       // mm
    pub material: Material,
}

impl Furniture {
    pub fn new(
        kind: FurnitureKind,
        position: Point2D,
        base_z: f64,
        rotation_deg: f64,
        material: Material,
    ) -> Self {
        let (w, d, h) = kind.default_dimensions();
        let name = kind.display_name().to_string();
        Self {
            id: ElementId::new(),
            name,
            kind,
            position,
            base_z,
            rotation_deg,
            width: w,
            depth: d,
            height: h,
            material,
        }
    }

    /// Rotate and translate local offset (dx, dy) to world space
    fn local_to_world(&self, dx: f64, dy: f64) -> Point2D {
        let rad = self.rotation_deg.to_radians();
        let cos = rad.cos();
        let sin = rad.sin();
        Point2D::new(
            self.position.x + dx * cos - dy * sin,
            self.position.y + dx * sin + dy * cos,
        )
    }

    /// Four outer footprint corners in world space
    pub fn footprint_corners(&self) -> [Point2D; 4] {
        let hw = self.width * 0.5;
        let hd = self.depth * 0.5;
        [
            self.local_to_world(-hw, -hd),
            self.local_to_world(hw, -hd),
            self.local_to_world(hw, hd),
            self.local_to_world(-hw, hd),
        ]
    }
}

/// Helper to add an oriented, properly wound box sub-element in world coordinates
pub fn add_oriented_box(
    mesh: &mut TriangleMesh,
    center_local: Point3D,
    size: Vector3D,
    pos: Point2D,
    base_z: f64,
    rotation_deg: f64,
) {
    let rad = rotation_deg.to_radians();
    let cos = rad.cos();
    let sin = rad.sin();

    let hx = size.x * 0.5;
    let hy = size.y * 0.5;
    let hz = size.z * 0.5;

    // 8 local corners
    let local_corners = [
        Point3D::new(center_local.x - hx, center_local.y - hy, center_local.z - hz), // 0: ---
        Point3D::new(center_local.x + hx, center_local.y - hy, center_local.z - hz), // 1: +--
        Point3D::new(center_local.x + hx, center_local.y + hy, center_local.z - hz), // 2: ++-
        Point3D::new(center_local.x - hx, center_local.y + hy, center_local.z - hz), // 3: -+-
        Point3D::new(center_local.x - hx, center_local.y - hy, center_local.z + hz), // 4: --+
        Point3D::new(center_local.x + hx, center_local.y - hy, center_local.z + hz), // 5: +-+
        Point3D::new(center_local.x + hx, center_local.y + hy, center_local.z + hz), // 6: +++
        Point3D::new(center_local.x - hx, center_local.y + hy, center_local.z + hz), // 7: -++
    ];

    // Transform to world space
    let world_corners: Vec<Point3D> = local_corners
        .iter()
        .map(|lp| {
            let wx = pos.x + lp.x * cos - lp.y * sin;
            let wy = pos.y + lp.x * sin + lp.y * cos;
            let wz = base_z + lp.z;
            Point3D::new(wx, wy, wz)
        })
        .collect();

    // 6 outward normals in world space:
    let n_bottom = Vector3D::new(0.0, 0.0, -1.0);
    let n_top = Vector3D::new(0.0, 0.0, 1.0);
    let n_front = Vector3D::new(sin, -cos, 0.0);  // -Y local
    let n_back = Vector3D::new(-sin, cos, 0.0);   // +Y local
    let n_left = Vector3D::new(-cos, -sin, 0.0);  // -X local
    let n_right = Vector3D::new(cos, sin, 0.0);   // +X local

    let faces = [
        // (c0, c1, c2, c3, normal) with CCW winding when viewed from outside
        (3, 2, 1, 0, n_bottom), // Bottom (-Z)
        (4, 5, 6, 7, n_top),    // Top (+Z)
        (0, 1, 5, 4, n_front),  // Front (-Y)
        (2, 3, 7, 6, n_back),   // Back (+Y)
        (3, 0, 4, 7, n_left),   // Left (-X)
        (1, 2, 6, 5, n_right),  // Right (+X)
    ];

    for &(i0, i1, i2, i3, norm) in &faces {
        let base = mesh.positions.len() as u32;
        mesh.positions.extend_from_slice(&[
            world_corners[i0],
            world_corners[i1],
            world_corners[i2],
            world_corners[i3],
        ]);
        mesh.normals.extend_from_slice(&[norm, norm, norm, norm]);

        // Triangle 1: i0 -> i1 -> i2
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.edge_flags.push([true, true, false]);

        // Triangle 2: i0 -> i2 -> i3
        mesh.indices.extend_from_slice(&[base, base + 2, base + 3]);
        mesh.edge_flags.push([false, true, true]);
    }
}

impl BimElement for Furniture {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Furniture
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut prims = Vec::new();
        let corners = self.footprint_corners();

        // 1. Outer bounding box
        prims.push(Polyline2D {
            vertices: corners.to_vec(),
            closed: true,
        });

        let hw = self.width * 0.5;
        let hd = self.depth * 0.5;

        // 2. Kind-specific architectural CAD symbols
        match self.kind {
            FurnitureKind::Bed => {
                // Headboard (back line)
                let hb_d = hd - 100.0;
                let hb_p1 = self.local_to_world(-hw, hb_d);
                let hb_p2 = self.local_to_world(hw, hb_d);
                prims.push(Polyline2D {
                    vertices: vec![hb_p1, hb_p2],
                    closed: false,
                });

                // Dual Pillows (枕頭)
                let pw = hw * 0.42;
                let pd = 350.0;
                let py = hb_d - 400.0;
                // Left pillow
                let l_p1 = self.local_to_world(-hw * 0.55 - pw * 0.5, py);
                let l_p2 = self.local_to_world(-hw * 0.55 + pw * 0.5, py);
                let l_p3 = self.local_to_world(-hw * 0.55 + pw * 0.5, py + pd);
                let l_p4 = self.local_to_world(-hw * 0.55 - pw * 0.5, py + pd);
                prims.push(Polyline2D {
                    vertices: vec![l_p1, l_p2, l_p3, l_p4],
                    closed: true,
                });
                // Right pillow
                let r_p1 = self.local_to_world(hw * 0.55 - pw * 0.5, py);
                let r_p2 = self.local_to_world(hw * 0.55 + pw * 0.5, py);
                let r_p3 = self.local_to_world(hw * 0.55 + pw * 0.5, py + pd);
                let r_p4 = self.local_to_world(hw * 0.55 - pw * 0.5, py + pd);
                prims.push(Polyline2D {
                    vertices: vec![r_p1, r_p2, r_p3, r_p4],
                    closed: true,
                });

                // Blanket fold line
                let bf1 = self.local_to_world(-hw + 80.0, -hd + 700.0);
                let bf2 = self.local_to_world(hw - 80.0, -hd + 700.0);
                prims.push(Polyline2D {
                    vertices: vec![bf1, bf2],
                    closed: false,
                });
            }
            FurnitureKind::Sofa => {
                // Backrest & Armrests
                let arm_w = 160.0;
                let back_d = 200.0;

                // Left armrest
                let la1 = self.local_to_world(-hw, -hd);
                let la2 = self.local_to_world(-hw + arm_w, -hd);
                let la3 = self.local_to_world(-hw + arm_w, hd);
                let la4 = self.local_to_world(-hw, hd);
                prims.push(Polyline2D {
                    vertices: vec![la1, la2, la3, la4],
                    closed: true,
                });

                // Right armrest
                let ra1 = self.local_to_world(hw - arm_w, -hd);
                let ra2 = self.local_to_world(hw, -hd);
                let ra3 = self.local_to_world(hw, hd);
                let ra4 = self.local_to_world(hw - arm_w, hd);
                prims.push(Polyline2D {
                    vertices: vec![ra1, ra2, ra3, ra4],
                    closed: true,
                });

                // Backrest
                let b1 = self.local_to_world(-hw + arm_w, hd - back_d);
                let b2 = self.local_to_world(hw - arm_w, hd - back_d);
                prims.push(Polyline2D {
                    vertices: vec![b1, b2],
                    closed: false,
                });

                // Seat cushion divisions (3 cushions)
                let seat_w = (self.width - arm_w * 2.0) / 3.0;
                for i in 1..=2 {
                    let cx = -hw + arm_w + (i as f64) * seat_w;
                    let c1 = self.local_to_world(cx, -hd);
                    let c2 = self.local_to_world(cx, hd - back_d);
                    prims.push(Polyline2D {
                        vertices: vec![c1, c2],
                        closed: false,
                    });
                }
            }
            FurnitureKind::DiningTable => {
                // 6 Dining chairs around table
                let cw = 420.0; // chair width
                let cd = 420.0; // chair depth

                // 3 Top chairs
                for i in 0..3 {
                    let cx = -hw + (i as f64 + 0.5) * (self.width / 3.0);
                    let ch1 = self.local_to_world(cx - cw * 0.5, hd + 20.0);
                    let ch2 = self.local_to_world(cx + cw * 0.5, hd + 20.0);
                    let ch3 = self.local_to_world(cx + cw * 0.5, hd + 20.0 + cd);
                    let ch4 = self.local_to_world(cx - cw * 0.5, hd + 20.0 + cd);
                    prims.push(Polyline2D {
                        vertices: vec![ch1, ch2, ch3, ch4],
                        closed: true,
                    });
                }
                // 3 Bottom chairs
                for i in 0..3 {
                    let cx = -hw + (i as f64 + 0.5) * (self.width / 3.0);
                    let ch1 = self.local_to_world(cx - cw * 0.5, -hd - 20.0 - cd);
                    let ch2 = self.local_to_world(cx + cw * 0.5, -hd - 20.0 - cd);
                    let ch3 = self.local_to_world(cx + cw * 0.5, -hd - 20.0);
                    let ch4 = self.local_to_world(cx - cw * 0.5, -hd - 20.0);
                    prims.push(Polyline2D {
                        vertices: vec![ch1, ch2, ch3, ch4],
                        closed: true,
                    });
                }
            }
            FurnitureKind::KitchenCabinet => {
                // Stainless sink basin on left
                let sw = 600.0;
                let sd = 420.0;
                let sx = -hw + 450.0;
                let s1 = self.local_to_world(sx - sw * 0.5, -sd * 0.5);
                let s2 = self.local_to_world(sx + sw * 0.5, -sd * 0.5);
                let s3 = self.local_to_world(sx + sw * 0.5, sd * 0.5);
                let s4 = self.local_to_world(sx - sw * 0.5, sd * 0.5);
                prims.push(Polyline2D {
                    vertices: vec![s1, s2, s3, s4],
                    closed: true,
                });

                // Cooktop dual burners on right
                let bx = hw - 500.0;
                let bw = 650.0;
                let bd = 440.0;
                let b1 = self.local_to_world(bx - bw * 0.5, -bd * 0.5);
                let b2 = self.local_to_world(bx + bw * 0.5, -bd * 0.5);
                let b3 = self.local_to_world(bx + bw * 0.5, bd * 0.5);
                let b4 = self.local_to_world(bx - bw * 0.5, bd * 0.5);
                prims.push(Polyline2D {
                    vertices: vec![b1, b2, b3, b4],
                    closed: true,
                });
            }
            FurnitureKind::Toilet => {
                // Toilet tank on back
                let tank_d = 200.0;
                let t1 = self.local_to_world(-hw, hd - tank_d);
                let t2 = self.local_to_world(hw, hd - tank_d);
                prims.push(Polyline2D {
                    vertices: vec![t1, t2],
                    closed: false,
                });

                // Bowl inner curve
                let b1 = self.local_to_world(-hw * 0.75, hd - tank_d);
                let b2 = self.local_to_world(-hw * 0.7, -hd + 100.0);
                let b3 = self.local_to_world(0.0, -hd + 50.0);
                let b4 = self.local_to_world(hw * 0.7, -hd + 100.0);
                let b5 = self.local_to_world(hw * 0.75, hd - tank_d);
                prims.push(Polyline2D {
                    vertices: vec![b1, b2, b3, b4, b5],
                    closed: false,
                });
            }
            FurnitureKind::Wardrobe => {
                // Sliding doors divider line & handles
                let mid = self.local_to_world(0.0, -hd);
                let mid_top = self.local_to_world(0.0, hd);
                prims.push(Polyline2D {
                    vertices: vec![mid, mid_top],
                    closed: false,
                });
                // Cross diagonal shelf lines
                let d1 = self.local_to_world(-hw + 80.0, -hd + 60.0);
                let d2 = self.local_to_world(-80.0, hd - 60.0);
                prims.push(Polyline2D {
                    vertices: vec![d1, d2],
                    closed: false,
                });
            }
            FurnitureKind::Desk => {
                // Drawer unit on right
                let dw = 400.0;
                let dr1 = self.local_to_world(hw - dw, -hd);
                let dr2 = self.local_to_world(hw - dw, hd);
                prims.push(Polyline2D {
                    vertices: vec![dr1, dr2],
                    closed: false,
                });
                // Chair polygon
                let cw = 450.0;
                let cd = 420.0;
                let ch1 = self.local_to_world(-cw * 0.5, -hd - 20.0 - cd);
                let ch2 = self.local_to_world(cw * 0.5, -hd - 20.0 - cd);
                let ch3 = self.local_to_world(cw * 0.5, -hd - 20.0);
                let ch4 = self.local_to_world(-cw * 0.5, -hd - 20.0);
                prims.push(Polyline2D {
                    vertices: vec![ch1, ch2, ch3, ch4],
                    closed: true,
                });
            }
        }

        prims
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let w = self.width;
        let d = self.depth;
        let h = self.height;
        let hw = w * 0.5;
        let hd = d * 0.5;

        let mut add_box = |center: Point3D, size: Vector3D| {
            add_oriented_box(&mut mesh, center, size, self.position, self.base_z, self.rotation_deg);
        };

        match self.kind {
            FurnitureKind::Bed => {
                // 1. Bed base platform (床框底台)
                add_box(
                    Point3D::new(0.0, -d * 0.04, 110.0),
                    Vector3D::new(w * 0.96, d * 0.90, 220.0),
                );
                // 4 Bed corner legs (4 支金屬/實木床腳)
                let leg_w = 60.0;
                let leg_h = 100.0;
                for &lx in &[-hw + 80.0, hw - 80.0] {
                    for &ly in &[-hd + 80.0, hd - 200.0] {
                        add_box(
                            Point3D::new(lx, ly, leg_h * 0.5),
                            Vector3D::new(leg_w, leg_w, leg_h),
                        );
                    }
                }
                // 2. Thick mattress (厚實獨立筒雙人床墊)
                add_box(
                    Point3D::new(0.0, -d * 0.04, 340.0),
                    Vector3D::new(w * 0.92, d * 0.88, 240.0),
                );
                // 3. Tall Headboard (典雅床頭大板)
                add_box(
                    Point3D::new(0.0, hd - 45.0, h * 0.5),
                    Vector3D::new(w, 90.0, h),
                );
                // 4. Headboard upholstery panel (床頭繃布軟包裝飾飾板)
                add_box(
                    Point3D::new(0.0, hd - 5.0, h * 0.55),
                    Vector3D::new(w * 0.90, 20.0, h * 0.70),
                );
                // 5. Dual Pillows (左、右高級舒適枕頭)
                let pw = w * 0.38;
                let pd = 420.0;
                let ph = 120.0;
                let py = hd - 380.0;
                let pz = 480.0;
                add_box(
                    Point3D::new(-hw * 0.50, py, pz),
                    Vector3D::new(pw, pd, ph),
                );
                add_box(
                    Point3D::new(hw * 0.50, py, pz),
                    Vector3D::new(pw, pd, ph),
                );
                // 6. Duvet / Folded Quilt (羽絨被/被褥與反摺層)
                let quilt_d = d * 0.58;
                let quilt_y = -hd + quilt_d * 0.5 + 40.0;
                add_box(
                    Point3D::new(0.0, quilt_y, 480.0),
                    Vector3D::new(w * 0.94, quilt_d, 60.0),
                );
                // Foldover edge rim (被緣翻摺收邊條)
                add_box(
                    Point3D::new(0.0, quilt_y + quilt_d * 0.5, 490.0),
                    Vector3D::new(w * 0.94, 100.0, 80.0),
                );
            }
            FurnitureKind::Wardrobe => {
                // 1. Recessed base plinth (防潮踢腳板)
                add_box(
                    Point3D::new(0.0, -15.0, 40.0),
                    Vector3D::new(w * 0.96, d - 30.0, 80.0),
                );
                // 2. Main cabinet carcass (衣櫃櫃體框)
                let body_h = h - 80.0;
                add_box(
                    Point3D::new(0.0, 0.0, 80.0 + body_h * 0.5),
                    Vector3D::new(w, d, body_h),
                );
                // 3. Multi-door panels (2 扇大器平開/推拉門片，微凸並帶有門縫)
                let door_w = (w - 20.0) * 0.5;
                let door_h = body_h - 20.0;
                let door_z = 80.0 + body_h * 0.5;
                let door_y = -hd - 10.0;
                // Left door
                add_box(
                    Point3D::new(-hw * 0.5, door_y, door_z),
                    Vector3D::new(door_w - 6.0, 20.0, door_h),
                );
                // Right door
                add_box(
                    Point3D::new(hw * 0.5, door_y, door_z),
                    Vector3D::new(door_w - 6.0, 20.0, door_h),
                );
                // 4. Vertical Sleek Metal Pulls (現代極簡長條把手)
                let handle_h = 450.0;
                let handle_z = 1100.0;
                let handle_y = door_y - 15.0;
                add_box(
                    Point3D::new(-45.0, handle_y, handle_z),
                    Vector3D::new(16.0, 15.0, handle_h),
                );
                add_box(
                    Point3D::new(45.0, handle_y, handle_z),
                    Vector3D::new(16.0, 15.0, handle_h),
                );
                // 5. Crown Top Fascia (天花收頂封板)
                add_box(
                    Point3D::new(0.0, -5.0, h - 20.0),
                    Vector3D::new(w + 20.0, d + 15.0, 40.0),
                );
            }
            FurnitureKind::DiningTable => {
                // 1. Tabletop (實木厚桌面)
                let top_th = 45.0;
                add_box(
                    Point3D::new(0.0, 0.0, h - top_th * 0.5),
                    Vector3D::new(w, d, top_th),
                );
                // 2. Apron frame under top (桌底支撐邊框)
                add_box(
                    Point3D::new(0.0, 0.0, h - top_th - 30.0),
                    Vector3D::new(w * 0.88, d * 0.82, 60.0),
                );
                // 3. 4 Heavy Table Legs (4 支穩固桌腳)
                let leg_size = 70.0;
                let leg_h = h - top_th;
                let lx = hw - 90.0;
                let ly = hd - 90.0;
                for &x in &[-lx, lx] {
                    for &y in &[-ly, ly] {
                        add_box(
                            Point3D::new(x, y, leg_h * 0.5),
                            Vector3D::new(leg_size, leg_size, leg_h),
                        );
                    }
                }
                // 4. Dining Chairs (環繞餐桌的 6 張北歐風實木餐椅: 3 張上方、3 張下方)
                let chair_w = 420.0;
                let chair_d = 420.0;
                let chair_seat_h = 450.0;
                let seat_th = 40.0;
                let back_h = 380.0;

                // 3 Chairs on North side (+Y)
                for i in 0..3 {
                    let cx = -hw + (i as f64 + 0.5) * (w / 3.0);
                    let cy = hd + 220.0;
                    // Seat cushion
                    add_box(
                        Point3D::new(cx, cy, chair_seat_h - seat_th * 0.5),
                        Vector3D::new(chair_w, chair_d, seat_th),
                    );
                    // 4 Chair legs
                    for &chx in &[-chair_w * 0.4, chair_w * 0.4] {
                        for &chy in &[-chair_d * 0.4, chair_d * 0.4] {
                            add_box(
                                Point3D::new(cx + chx, cy + chy, (chair_seat_h - seat_th) * 0.5),
                                Vector3D::new(32.0, 32.0, chair_seat_h - seat_th),
                            );
                        }
                    }
                    // Chair backrest (at outer edge)
                    add_box(
                        Point3D::new(cx, cy + chair_d * 0.45, chair_seat_h + back_h * 0.5),
                        Vector3D::new(chair_w, 28.0, back_h),
                    );
                }

                // 3 Chairs on South side (-Y)
                for i in 0..3 {
                    let cx = -hw + (i as f64 + 0.5) * (w / 3.0);
                    let cy = -hd - 220.0;
                    // Seat cushion
                    add_box(
                        Point3D::new(cx, cy, chair_seat_h - seat_th * 0.5),
                        Vector3D::new(chair_w, chair_d, seat_th),
                    );
                    // 4 Chair legs
                    for &chx in &[-chair_w * 0.4, chair_w * 0.4] {
                        for &chy in &[-chair_d * 0.4, chair_d * 0.4] {
                            add_box(
                                Point3D::new(cx + chx, cy + chy, (chair_seat_h - seat_th) * 0.5),
                                Vector3D::new(32.0, 32.0, chair_seat_h - seat_th),
                            );
                        }
                    }
                    // Chair backrest
                    add_box(
                        Point3D::new(cx, cy - chair_d * 0.45, chair_seat_h + back_h * 0.5),
                        Vector3D::new(chair_w, 28.0, back_h),
                    );
                }
            }
            FurnitureKind::KitchenCabinet => {
                // 1. Recessed moisture-proof kickplate (防潮黑色踢腳板)
                add_box(
                    Point3D::new(0.0, 25.0, 40.0),
                    Vector3D::new(w, d - 50.0, 80.0),
                );
                // 2. Base carcass body (廚具下櫃主體)
                let cab_h = h - 120.0;
                add_box(
                    Point3D::new(0.0, 0.0, 80.0 + cab_h * 0.5),
                    Vector3D::new(w, d, cab_h),
                );
                // 3. Cabinet doors & drawer fronts (4 組門片/抽屜)
                let num_doors = 4;
                let dw = w / (num_doors as f64);
                for i in 0..num_doors {
                    let cx = -hw + (i as f64 + 0.5) * dw;
                    add_box(
                        Point3D::new(cx, -hd - 10.0, 80.0 + cab_h * 0.5),
                        Vector3D::new(dw - 6.0, 20.0, cab_h - 10.0),
                    );
                    // Sleek horizontal handle
                    add_box(
                        Point3D::new(cx, -hd - 22.0, 80.0 + cab_h - 60.0),
                        Vector3D::new(dw * 0.5, 12.0, 16.0),
                    );
                }
                // 4. Countertop (厚實石英石料理檯面 with front drip nose)
                let top_th = 40.0;
                add_box(
                    Point3D::new(0.0, -10.0, h - top_th * 0.5),
                    Vector3D::new(w + 20.0, d + 30.0, top_th),
                );
                // 5. Stainless Sink Basin (不鏽鋼水槽與下沉槽體)
                let sink_x = -hw + 480.0;
                add_box(
                    Point3D::new(sink_x, 0.0, h + 3.0),
                    Vector3D::new(600.0, 430.0, 6.0),
                );
                // 6. Gooseneck Faucet (弧形冷熱水龍頭)
                add_box(
                    Point3D::new(sink_x, 170.0, h + 100.0),
                    Vector3D::new(28.0, 28.0, 200.0),
                );
                add_box(
                    Point3D::new(sink_x, 80.0, h + 200.0),
                    Vector3D::new(24.0, 160.0, 24.0),
                );
                // 7. Built-in Cooktop (黑色微晶玻璃雙口瓦斯爐)
                let cook_x = hw - 520.0;
                add_box(
                    Point3D::new(cook_x, 0.0, h + 4.0),
                    Vector3D::new(680.0, 440.0, 8.0),
                );
                // 2 Burner grates (爐架)
                add_box(
                    Point3D::new(cook_x - 160.0, 0.0, h + 18.0),
                    Vector3D::new(200.0, 200.0, 20.0),
                );
                add_box(
                    Point3D::new(cook_x + 160.0, 0.0, h + 18.0),
                    Vector3D::new(200.0, 200.0, 20.0),
                );
            }
            FurnitureKind::Toilet => {
                // 1. Water tank (後方高立式陶瓷儲水箱)
                let tank_w = 420.0;
                let tank_d = 190.0;
                let tank_h = 380.0;
                let tank_y = hd - tank_d * 0.5;
                add_box(
                    Point3D::new(0.0, tank_y, 400.0 + tank_h * 0.5),
                    Vector3D::new(tank_w, tank_d, tank_h),
                );
                // 2. Tank lid (水箱蓋)
                add_box(
                    Point3D::new(0.0, tank_y, 400.0 + tank_h + 15.0),
                    Vector3D::new(tank_w + 20.0, tank_d + 20.0, 30.0),
                );
                // 3. Dual flush button (頂部鍍鉻雙段沖水按鈕)
                add_box(
                    Point3D::new(0.0, tank_y, 400.0 + tank_h + 35.0),
                    Vector3D::new(65.0, 45.0, 10.0),
                );
                // 4. Toilet Pedestal Base (馬桶下半部陶瓷基座)
                let base_y = -hd + 300.0;
                add_box(
                    Point3D::new(0.0, base_y, 110.0),
                    Vector3D::new(320.0, 460.0, 220.0),
                );
                // 5. Toilet Bowl Body (馬桶盆體)
                add_box(
                    Point3D::new(0.0, base_y - 20.0, 280.0),
                    Vector3D::new(380.0, 520.0, 160.0),
                );
                // 6. Toilet Seat Ring & Lid (座圈與緩降馬桶蓋)
                add_box(
                    Point3D::new(0.0, base_y - 30.0, 390.0),
                    Vector3D::new(390.0, 530.0, 35.0),
                );
            }
            FurnitureKind::Sofa => {
                // 1. 4 Modern sofa feet (沙發 4 支五金矮腳)
                let foot_h = 60.0;
                let foot_w = 55.0;
                for &fx in &[-hw + 80.0, hw - 80.0] {
                    for &fy in &[-hd + 80.0, hd - 80.0] {
                        add_box(
                            Point3D::new(fx, fy, foot_h * 0.5),
                            Vector3D::new(foot_w, foot_w, foot_h),
                        );
                    }
                }
                // 2. Sofa base chassis (底座實木托盤)
                let base_th = 120.0;
                add_box(
                    Point3D::new(0.0, 0.0, foot_h + base_th * 0.5),
                    Vector3D::new(w, d, base_th),
                );
                // 3. Left & Right Armrests (左右兩側舒適厚實扶手)
                let arm_w = 180.0;
                let arm_h = 420.0;
                let arm_z = foot_h + arm_h * 0.5;
                add_box(
                    Point3D::new(-hw + arm_w * 0.5, 0.0, arm_z),
                    Vector3D::new(arm_w, d, arm_h),
                );
                add_box(
                    Point3D::new(hw - arm_w * 0.5, 0.0, arm_z),
                    Vector3D::new(arm_w, d, arm_h),
                );
                // 4. Backrest Main Wall (沙發靠背主體)
                let back_d = 200.0;
                let back_h = h - foot_h;
                let back_w = w - arm_w * 2.0;
                let back_y = hd - back_d * 0.5;
                add_box(
                    Point3D::new(0.0, back_y, foot_h + back_h * 0.5),
                    Vector3D::new(back_w, back_d, back_h),
                );
                // 5. 3 Plush Seat Cushions (3 塊厚實獨立高回彈座墊)
                let seat_count = 3;
                let cushion_w = (back_w - 20.0) / (seat_count as f64);
                let cushion_d = d - back_d - 40.0;
                let cushion_h = 160.0;
                let cushion_y = -hd + cushion_d * 0.5 + 20.0;
                let cushion_z = foot_h + base_th + cushion_h * 0.5 - 20.0;
                for i in 0..seat_count {
                    let cx = -back_w * 0.5 + (i as f64 + 0.5) * cushion_w;
                    add_box(
                        Point3D::new(cx, cushion_y, cushion_z),
                        Vector3D::new(cushion_w - 10.0, cushion_d, cushion_h),
                    );
                    // 6. Matching Backrest Pillow Cushions (靠枕)
                    add_box(
                        Point3D::new(cx, back_y - back_d * 0.5 - 50.0, cushion_z + 220.0),
                        Vector3D::new(cushion_w - 15.0, 120.0, 300.0),
                    );
                }
            }
            FurnitureKind::Desk => {
                // 1. Desktop Work Surface (寬敞書桌板)
                let top_th = 40.0;
                add_box(
                    Point3D::new(0.0, 0.0, h - top_th * 0.5),
                    Vector3D::new(w, d, top_th),
                );
                // 2. Left O-Frame Metal Leg (左側現代極簡鋼製口字形桌腳)
                let leg_th = 45.0;
                let leg_h = h - top_th;
                let lx = -hw + 40.0;
                // Vertical front column
                add_box(
                    Point3D::new(lx, -hd + 60.0, leg_h * 0.5),
                    Vector3D::new(leg_th, leg_th, leg_h),
                );
                // Vertical back column
                add_box(
                    Point3D::new(lx, hd - 60.0, leg_h * 0.5),
                    Vector3D::new(leg_th, leg_th, leg_h),
                );
                // Bottom runner
                add_box(
                    Point3D::new(lx, 0.0, 25.0),
                    Vector3D::new(leg_th, d * 0.85, 45.0),
                );
                // 3. Modesty Privacy Baffle (後擋板)
                add_box(
                    Point3D::new(0.0, hd - 50.0, h - 220.0),
                    Vector3D::new(w * 0.85, 20.0, 280.0),
                );
                // 4. Right 3-Drawer Pedestal (右側收納三抽木櫃)
                let ped_w = 420.0;
                let ped_d = d * 0.88;
                let ped_h = h - top_th - 60.0;
                let px = hw - ped_w * 0.5 - 20.0;
                add_box(
                    Point3D::new(px, 0.0, ped_h * 0.5),
                    Vector3D::new(ped_w, ped_d, ped_h),
                );
                // 3 Drawers with handles
                let drawer_h = (ped_h - 20.0) / 3.0;
                for i in 0..3 {
                    let dz = 10.0 + (i as f64 + 0.5) * drawer_h;
                    // Drawer front panel
                    add_box(
                        Point3D::new(px, -ped_d * 0.5 - 10.0, dz),
                        Vector3D::new(ped_w - 10.0, 16.0, drawer_h - 8.0),
                    );
                    // Sleek handle
                    add_box(
                        Point3D::new(px, -ped_d * 0.5 - 22.0, dz),
                        Vector3D::new(120.0, 12.0, 16.0),
                    );
                }
                // 5. Office Task Chair (人體工學電腦辦公椅: 5 爪底盤 + 氣壓棒 + 坐墊 + 弧形透氣網背)
                let chair_x = -hw * 0.15;
                let chair_y = -hd - 280.0;
                // 5-Star caster base
                add_box(
                    Point3D::new(chair_x, chair_y, 35.0),
                    Vector3D::new(480.0, 480.0, 60.0),
                );
                // Central gas lift piston
                add_box(
                    Point3D::new(chair_x, chair_y, 230.0),
                    Vector3D::new(60.0, 60.0, 330.0),
                );
                // Ergonomic seat cushion
                add_box(
                    Point3D::new(chair_x, chair_y, 440.0),
                    Vector3D::new(450.0, 440.0, 70.0),
                );
                // Mesh backrest
                add_box(
                    Point3D::new(chair_x, chair_y - 200.0, 700.0),
                    Vector3D::new(430.0, 30.0, 460.0),
                );
            }
        }

        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let unit_cost = self.kind.default_cost();

        qto.add_item(CostLineItem::new(
            self.id,
            "FF&E 家具與軟裝設備工程",
            format!("{} (採購與室內組裝定位)", self.name),
            1.0,
            "組",
            unit_cost,
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
