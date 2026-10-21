use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpaceType {
    LivingRoom,    // 客廳
    MasterBedroom, // 主臥室
    Bedroom,       // 次臥室 / 客房
    Kitchen,       // 廚房
    Bathroom,      // 衛浴間
    DiningRoom,    // 餐廳
    Study,         // 書房
    Balcony,       // 景觀陽台
    Foyer,         // 玄關
    Corridor,      // 走道
}

impl SpaceType {
    pub fn display_name(&self) -> &'static str {
        match self {
            SpaceType::LivingRoom => "客廳 (Living Room)",
            SpaceType::MasterBedroom => "主臥室 (Master Bedroom)",
            SpaceType::Bedroom => "次臥室 (Bedroom)",
            SpaceType::Kitchen => "廚房 (Kitchen)",
            SpaceType::Bathroom => "衛浴間 (Bathroom)",
            SpaceType::DiningRoom => "餐廳 (Dining Room)",
            SpaceType::Study => "書房 (Study)",
            SpaceType::Balcony => "陽台 (Balcony)",
            SpaceType::Foyer => "玄關 (Foyer)",
            SpaceType::Corridor => "走廊 (Corridor)",
        }
    }

    pub fn default_color_rgb(&self) -> [u8; 3] {
        match self {
            SpaceType::LivingRoom => [235, 215, 175],
            SpaceType::MasterBedroom => [215, 195, 225],
            SpaceType::Bedroom => [195, 215, 235],
            SpaceType::Kitchen => [235, 195, 175],
            SpaceType::Bathroom => [180, 225, 225],
            SpaceType::DiningRoom => [235, 225, 185],
            SpaceType::Study => [205, 225, 195],
            SpaceType::Balcony => [195, 235, 195],
            SpaceType::Foyer => [225, 210, 195],
            SpaceType::Corridor => [220, 220, 220],
        }
    }
}

/// Architectural & Interior Design Room Space / Zone (房間與空間區域)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: ElementId,
    pub name: String,
    pub space_type: SpaceType,
    pub boundary: Vec<Point2D>,
    pub elevation: f64,    // mm, Base elevation Z
    pub clear_height: f64, // mm, Clear ceiling height (default: 2800.0)
    pub floor_material: Material,
}

impl Space {
    pub fn new(
        space_type: SpaceType,
        boundary: Vec<Point2D>,
        elevation: f64,
        clear_height: f64,
        floor_material: Material,
    ) -> Self {
        let name = space_type.display_name().to_string();
        Self {
            id: ElementId::new(),
            name,
            space_type,
            boundary,
            elevation,
            clear_height: clear_height.max(1500.0),
            floor_material,
        }
    }

    /// Construct a rectangular space from opposite corners
    pub fn from_rect(
        space_type: SpaceType,
        p_min: Point2D,
        p_max: Point2D,
        elevation: f64,
        clear_height: f64,
        floor_material: Material,
    ) -> Self {
        let min_x = p_min.x.min(p_max.x);
        let max_x = p_min.x.max(p_max.x);
        let min_y = p_min.y.min(p_max.y);
        let max_y = p_min.y.max(p_max.y);

        let boundary = vec![
            Point2D::new(min_x, min_y),
            Point2D::new(max_x, min_y),
            Point2D::new(max_x, max_y),
            Point2D::new(min_x, max_y),
        ];

        Self::new(space_type, boundary, elevation, clear_height, floor_material)
    }

    /// Computes the precise floor area in square meters (m²) using shoelace formula
    pub fn area_m2(&self) -> f64 {
        let n = self.boundary.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            sum += self.boundary[i].x * self.boundary[j].y - self.boundary[j].x * self.boundary[i].y;
        }
        (sum.abs() * 0.5) / 1_000_000.0
    }

    /// Computes Taiwanese architectural area in Ping (坪, 1 m² = 0.3025 坪)
    pub fn area_ping(&self) -> f64 {
        self.area_m2() * 0.3025
    }

    /// Computes the perimeter of the room in meters (m)
    pub fn perimeter_m(&self) -> f64 {
        let n = self.boundary.len();
        if n < 2 {
            return 0.0;
        }
        let mut peri = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            peri += self.boundary[i].distance(self.boundary[j]);
        }
        peri / 1000.0
    }

    /// Computes the interior room air volume in cubic meters (m³)
    pub fn volume_m3(&self) -> f64 {
        self.area_m2() * (self.clear_height / 1000.0)
    }

    /// Computes the visual centroid of the space boundary for tag badge placement
    pub fn centroid(&self) -> Point2D {
        if self.boundary.is_empty() {
            return Point2D::ZERO;
        }
        let n = self.boundary.len();
        if n < 3 {
            return self.boundary[0];
        }
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut signed_area = 0.0;

        for i in 0..n {
            let j = (i + 1) % n;
            let a = self.boundary[i].x * self.boundary[j].y - self.boundary[j].x * self.boundary[i].y;
            signed_area += a;
            cx += (self.boundary[i].x + self.boundary[j].x) * a;
            cy += (self.boundary[i].y + self.boundary[j].y) * a;
        }

        signed_area *= 0.5;
        if signed_area.abs() < 1e-4 {
            let mut sum = Point2D::ZERO;
            for p in &self.boundary {
                sum = sum + *p;
            }
            return sum / (n as f64);
        }

        Point2D::new(cx / (6.0 * signed_area), cy / (6.0 * signed_area))
    }
}

impl BimElement for Space {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Space
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        if self.boundary.len() < 3 {
            return Vec::new();
        }
        let mut prims = Vec::new();

        // 1. Outer room boundary polyline
        prims.push(Polyline2D {
            vertices: self.boundary.clone(),
            closed: true,
        });

        // 2. Center room tag marker (cross mark at centroid)
        let c = self.centroid();
        let tag_size = 150.0; // mm
        prims.push(Polyline2D {
            vertices: vec![
                Point2D::new(c.x - tag_size, c.y),
                Point2D::new(c.x + tag_size, c.y),
            ],
            closed: false,
        });
        prims.push(Polyline2D {
            vertices: vec![
                Point2D::new(c.x, c.y - tag_size),
                Point2D::new(c.x, c.y + tag_size),
            ],
            closed: false,
        });

        prims
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        if self.boundary.len() < 3 {
            return TriangleMesh::new();
        }
        let finish_thickness = 18.0; // 18mm floor finish tile plate
        let base_z = self.elevation;
        let top_z = base_z + finish_thickness;

        let n = self.boundary.len();
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut edge_flags = Vec::new();

        // Bottom vertices: 0..n
        for p in &self.boundary {
            positions.push(Point3D::new(p.x, p.y, base_z));
            normals.push(-Vector3D::Z);
        }
        // Top vertices: n..2n
        for p in &self.boundary {
            positions.push(Point3D::new(p.x, p.y, top_z));
            normals.push(Vector3D::Z);
        }

        // Side faces (perimeter skirt)
        for i in 0..n {
            let next = (i + 1) % n;
            let b0 = i as u32;
            let b1 = next as u32;
            let t0 = (n + i) as u32;
            let t1 = (n + next) as u32;

            indices.extend_from_slice(&[b0, b1, t1]);
            edge_flags.push([true, true, false]);

            indices.extend_from_slice(&[b0, t1, t0]);
            edge_flags.push([false, true, true]);
        }

        // Top face fan triangulation
        for i in 1..n - 1 {
            indices.extend_from_slice(&[n as u32, (n + i) as u32, (n + i + 1) as u32]);
            edge_flags.push([i == 1, true, i == n - 2]);
        }

        // Bottom face fan triangulation
        for i in 1..n - 1 {
            indices.extend_from_slice(&[0, (i + 1) as u32, i as u32]);
            edge_flags.push([i == n - 2, true, i == 1]);
        }

        TriangleMesh {
            positions,
            normals,
            indices,
            edge_flags,
        }
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area = self.area_m2();
        let peri = self.perimeter_m();
        let wall_paint_area = (peri * (self.clear_height / 1000.0) - 4.0).max(0.0); // minus doors

        // Floor finish item
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 室內地坪工程",
            format!("{} 地坪鋪設工程 ({})", self.name, self.floor_material.name),
            area,
            "m²",
            self.floor_material.cost_per_m3 * 0.02 + 1200.0,
        ));

        // Skirting board
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 室內地坪工程",
            format!("{} 防潮踢腳板收邊工程 (H=80mm)", self.name),
            peri,
            "m",
            350.0,
        ));

        // Interior wall paint
        qto.add_item(CostLineItem::new(
            self.id,
            "Finishes 牆面裝修工程",
            format!("{} 水性環保乳膠漆粉刷 (含批土三度面漆)", self.name),
            wall_paint_area,
            "m²",
            450.0,
        ));

        qto
    }


    fn translate(&mut self, delta: Vector3D) {
        for p in &mut self.boundary {
            p.x += delta.x;
            p.y += delta.y;
        }
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
