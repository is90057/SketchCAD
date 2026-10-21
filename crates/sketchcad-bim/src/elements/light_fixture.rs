use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightType {
    Downlight,   // LED 嵌燈 / 筒燈
    Pendant,     // 藝術吊燈 (餐廳 / 客廳主燈)
    TrackLight,  // 投射軌道燈 (洗牆 / 畫廊重點照明)
    CeilingLamp, // 現代吸頂燈
    LightStrip,  // 天花板間接照明燈帶
}

impl LightType {
    pub fn display_name(&self) -> &'static str {
        match self {
            LightType::Downlight => "LED 嵌入式筒燈 (Downlight)",
            LightType::Pendant => "現代造型吊燈 (Pendant Light)",
            LightType::TrackLight => "重點投射軌道燈 (Track Spotlight)",
            LightType::CeilingLamp => "大氣吸頂燈 (Ceiling Light)",
            LightType::LightStrip => "間接照明隱藏燈帶 (LED Strip)",
        }
    }

    pub fn default_specs(&self) -> (f64, u32, f64, f64) {
        // (lumens, color_temp_k, wattage, cost_twd)
        match self {
            LightType::Downlight => (850.0, 3000, 9.0, 650.0),
            LightType::Pendant => (3200.0, 3500, 36.0, 6800.0),
            LightType::TrackLight => (1400.0, 4000, 15.0, 1450.0),
            LightType::CeilingLamp => (4200.0, 4000, 48.0, 4500.0),
            LightType::LightStrip => (2400.0, 3000, 24.0, 1800.0),
        }
    }
}

/// Architectural & Interior Lighting Fixture Smart Element (照明燈具與電氣符號)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightFixture {
    pub id: ElementId,
    pub name: String,
    pub light_type: LightType,
    pub position: Point2D,
    pub mounting_height: f64, // mm, e.g. 2800.0 (Ceiling level)
    pub lumens: f64,          // Luminous flux (lm)
    pub color_temp_k: u32,    // Color temperature Kelvin (e.g. 3000K, 4000K)
    pub wattage: f64,         // Power (Watts)
    pub beam_angle_deg: f64,  // Beam angle (e.g. 36.0 deg)
}

impl LightFixture {
    pub fn new(
        light_type: LightType,
        position: Point2D,
        mounting_height: f64,
    ) -> Self {
        let (lumens, color_temp_k, wattage, _) = light_type.default_specs();
        let name = light_type.display_name().to_string();
        Self {
            id: ElementId::new(),
            name,
            light_type,
            position,
            mounting_height: mounting_height.max(1000.0),
            lumens,
            color_temp_k,
            wattage,
            beam_angle_deg: 38.0,
        }
    }
}

impl BimElement for LightFixture {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::LightFixture
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let mut prims = Vec::new();
        let cx = self.position.x;
        let cy = self.position.y;
        let r = 90.0; // 90mm radius in plan

        // Reflected Ceiling Plan Electrical CAD symbol (12-segment circle)
        let segments = 12;
        let mut circle_pts = Vec::new();
        for i in 0..segments {
            let theta = (i as f64) * 2.0 * std::f64::consts::PI / (segments as f64);
            circle_pts.push(Point2D::new(cx + r * theta.cos(), cy + r * theta.sin()));
        }
        prims.push(Polyline2D {
            vertices: circle_pts,
            closed: true,
        });

        // Crosshairs inside circle
        prims.push(Polyline2D {
            vertices: vec![
                Point2D::new(cx - r * 1.3, cy),
                Point2D::new(cx + r * 1.3, cy),
            ],
            closed: false,
        });
        prims.push(Polyline2D {
            vertices: vec![
                Point2D::new(cx, cy - r * 1.3),
                Point2D::new(cx, cy + r * 1.3),
            ],
            closed: false,
        });

        if self.light_type == LightType::Pendant {
            // Extra diagonal rays for decorative pendant lamp
            let dr = r * 0.7;
            prims.push(Polyline2D {
                vertices: vec![Point2D::new(cx - dr, cy - dr), Point2D::new(cx + dr, cy + dr)],
                closed: false,
            });
            prims.push(Polyline2D {
                vertices: vec![Point2D::new(cx - dr, cy + dr), Point2D::new(cx + dr, cy - dr)],
                closed: false,
            });
        }

        prims
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let cx = self.position.x;
        let cy = self.position.y;
        let z_ceil = self.mounting_height;

        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut edge_flags = Vec::new();

        // 1. Kind-Specific Architectural Luminaire Hardware (精緻建築燈具本體)
        match self.light_type {
            LightType::Downlight => {
                // Outer ceiling flange trim ring (崁孔修飾環: diameter 150mm)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil, z_ceil - 15.0, 75.0, 75.0, 16);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, z_ceil), 75.0, Vector3D::Z, 16, true);
                // Deep anti-glare baffle cup (內凹消眩杯: diameter 100mm)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil - 15.0, z_ceil - 50.0, 50.0, 38.0, 16);
                // Emissive frosted COB LED lens (中央發光柔光透鏡)
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, z_ceil - 50.0), 38.0, -Vector3D::Z, 16, false);
            }
            LightType::Pendant => {
                // Ceiling canopy disc (吸頂圓盤)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil, z_ceil - 20.0, 60.0, 60.0, 12);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, z_ceil), 60.0, Vector3D::Z, 12, true);
                // Suspension cord (細黑金屬吊線: diameter 8mm)
                let shade_top_z = z_ceil - 650.0;
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil - 20.0, shade_top_z, 4.0, 4.0, 6);
                // Designer flared conical lampshade (現代極簡大器燈罩)
                let shade_bot_z = shade_top_z - 220.0;
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, shade_top_z, shade_bot_z, 40.0, 180.0, 16);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, shade_top_z), 40.0, Vector3D::Z, 12, true);
                // Glowing bulb inside shade (發光燈泡)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, shade_top_z - 80.0, shade_top_z - 170.0, 38.0, 42.0, 12);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, shade_top_z - 170.0), 42.0, -Vector3D::Z, 12, false);
            }
            LightType::TrackLight => {
                // Track rail (鋁合金導電天花軌道: length 1200mm, width 40mm, depth 25mm)
                let tr_w = 40.0;
                let tr_len = 1200.0;
                let tr_h = 25.0;
                let x0 = cx - tr_len * 0.5;
                let x1 = cx + tr_len * 0.5;
                let y0 = cy - tr_w * 0.5;
                let y1 = cy + tr_w * 0.5;
                let z0 = z_ceil - tr_h;
                let z1 = z_ceil;
                let corners = [
                    Point3D::new(x0, y0, z0), Point3D::new(x1, y0, z0),
                    Point3D::new(x1, y1, z0), Point3D::new(x0, y1, z0),
                    Point3D::new(x0, y0, z1), Point3D::new(x1, y0, z1),
                    Point3D::new(x1, y1, z1), Point3D::new(x0, y1, z1),
                ];
                let faces = [
                    (3, 2, 1, 0, -Vector3D::Z), (4, 5, 6, 7, Vector3D::Z),
                    (0, 1, 5, 4, -Vector3D::Y), (2, 3, 7, 6, Vector3D::Y),
                    (3, 0, 4, 7, -Vector3D::X), (1, 2, 6, 5, Vector3D::X),
                ];
                for &(i0, i1, i2, i3, norm) in &faces {
                    let b = positions.len() as u32;
                    positions.extend_from_slice(&[corners[i0], corners[i1], corners[i2], corners[i3]]);
                    normals.extend_from_slice(&[norm, norm, norm, norm]);
                    indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
                    edge_flags.extend_from_slice(&[[true, true, false], [false, true, true]]);
                }
                // 2 Directional spotlight cans (2 支聚焦射燈筒)
                for &spot_x in &[cx - 300.0, cx + 300.0] {
                    add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(spot_x, cy, z0 - 45.0), 38.0, Vector3D::Z, 12, true);
                    let body_bot_z = z0 - 180.0;
                    add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, spot_x, cy, z0 - 45.0, body_bot_z, 36.0, 36.0, 12);
                    // Luminous front lens
                    add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(spot_x, cy, body_bot_z), 34.0, -Vector3D::Z, 12, false);
                }
            }
            LightType::CeilingLamp => {
                // Slim flush-mount base ring (超薄金屬底盤)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil, z_ceil - 20.0, 270.0, 270.0, 24);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, z_ceil), 270.0, Vector3D::Z, 24, true);
                // Curved frosted acrylic diffuser dish (大面積柔光吸頂罩)
                add_fixture_cylinder(&mut positions, &mut normals, &mut indices, &mut edge_flags, cx, cy, z_ceil - 20.0, z_ceil - 65.0, 260.0, 240.0, 24);
                add_fixture_disc(&mut positions, &mut normals, &mut indices, &mut edge_flags, Point3D::new(cx, cy, z_ceil - 65.0), 240.0, -Vector3D::Z, 24, false);
            }
            LightType::LightStrip => {
                // Continuous aluminum housing channel
                let strip_len = 2400.0;
                let strip_w = 40.0;
                let strip_h = 25.0;
                let x0 = cx - strip_len * 0.5;
                let x1 = cx + strip_len * 0.5;
                let y0 = cy - strip_w * 0.5;
                let y1 = cy + strip_w * 0.5;
                let z0 = z_ceil - strip_h;
                let z1 = z_ceil;
                let corners = [
                    Point3D::new(x0, y0, z0), Point3D::new(x1, y0, z0),
                    Point3D::new(x1, y1, z0), Point3D::new(x0, y1, z0),
                    Point3D::new(x0, y0, z1), Point3D::new(x1, y0, z1),
                    Point3D::new(x1, y1, z1), Point3D::new(x0, y1, z1),
                ];
                let faces = [
                    (3, 2, 1, 0, -Vector3D::Z), (4, 5, 6, 7, Vector3D::Z),
                    (0, 1, 5, 4, -Vector3D::Y), (2, 3, 7, 6, Vector3D::Y),
                    (3, 0, 4, 7, -Vector3D::X), (1, 2, 6, 5, Vector3D::X),
                ];
                for &(i0, i1, i2, i3, norm) in &faces {
                    let b = positions.len() as u32;
                    positions.extend_from_slice(&[corners[i0], corners[i1], corners[i2], corners[i3]]);
                    normals.extend_from_slice(&[norm, norm, norm, norm]);
                    indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
                    edge_flags.extend_from_slice(&[[true, true, false], [false, true, true]]);
                }
            }
        }

        // 2. Realistic Floor Illumination Pool (地面柔和圓形照度光斑與擴散光圈)
        // Strictly edge_flags = [false, false, false] - Absolutely NO ugly multi-cone wireframes!
        let pool_segments = 24;
        let pool_r = (z_ceil * (self.beam_angle_deg.to_radians() * 0.5).tan()).clamp(600.0, 2400.0);
        let floor_z = 2.0; // 2mm above floor surface to avoid z-fighting

        // Inner intense core pool (中心核心亮區)
        let core_r = pool_r * 0.45;
        let core_center_idx = positions.len() as u32;
        positions.push(Point3D::new(cx, cy, floor_z));
        normals.push(Vector3D::Z);

        let core_start = positions.len() as u32;
        for i in 0..pool_segments {
            let theta = (i as f64) * 2.0 * std::f64::consts::PI / (pool_segments as f64);
            positions.push(Point3D::new(cx + core_r * theta.cos(), cy + core_r * theta.sin(), floor_z));
            normals.push(Vector3D::Z);
        }
        for i in 0..pool_segments {
            let next = (i + 1) % pool_segments;
            let r0 = core_start + (i as u32);
            let r1 = core_start + (next as u32);
            indices.extend_from_slice(&[core_center_idx, r0, r1]);
            edge_flags.push([false, false, false]); // NO WIRES!
        }

        // Outer soft falloff ring (外圍漫射衰減圈)
        let outer_start = positions.len() as u32;
        for i in 0..pool_segments {
            let theta = (i as f64) * 2.0 * std::f64::consts::PI / (pool_segments as f64);
            positions.push(Point3D::new(cx + pool_r * theta.cos(), cy + pool_r * theta.sin(), floor_z));
            normals.push(Vector3D::Z);
        }
        for i in 0..pool_segments {
            let next = (i + 1) % pool_segments;
            let c0 = core_start + (i as u32);
            let c1 = core_start + (next as u32);
            let o0 = outer_start + (i as u32);
            let o1 = outer_start + (next as u32);

            indices.extend_from_slice(&[c0, o0, o1]);
            edge_flags.push([false, false, false]); // NO WIRES!

            indices.extend_from_slice(&[c0, o1, c1]);
            edge_flags.push([false, false, false]); // NO WIRES!
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
        let (_, _, _, unit_cost) = self.light_type.default_specs();

        qto.add_item(CostLineItem::new(
            self.id,
            "Electrical 電氣與照明工程",
            format!("{} (含配線、迴路開關與燈具安裝工資)", self.name),
            1.0,
            "組",
            unit_cost + 450.0,
        ));

        qto
    }


    fn translate(&mut self, delta: Vector3D) {
        self.position.x += delta.x;
        self.position.y += delta.y;
        self.mounting_height += delta.z;
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

fn add_fixture_disc(
    positions: &mut Vec<Point3D>,
    normals: &mut Vec<Vector3D>,
    indices: &mut Vec<u32>,
    edge_flags: &mut Vec<[bool; 3]>,
    center: Point3D,
    radius: f64,
    normal: Vector3D,
    segments: usize,
    show_edges: bool,
) {
    let center_idx = positions.len() as u32;
    positions.push(center);
    normals.push(normal);

    let ring_start = positions.len() as u32;
    for i in 0..segments {
        let theta = (i as f64) * 2.0 * std::f64::consts::PI / (segments as f64);
        positions.push(Point3D::new(center.x + radius * theta.cos(), center.y + radius * theta.sin(), center.z));
        normals.push(normal);
    }

    for i in 0..segments {
        let next = (i + 1) % segments;
        let r0 = ring_start + (i as u32);
        let r1 = ring_start + (next as u32);
        if normal.z >= 0.0 {
            indices.extend_from_slice(&[center_idx, r0, r1]);
        } else {
            indices.extend_from_slice(&[center_idx, r1, r0]);
        }
        edge_flags.push([false, show_edges, show_edges]);
    }
}

fn add_fixture_cylinder(
    positions: &mut Vec<Point3D>,
    normals: &mut Vec<Vector3D>,
    indices: &mut Vec<u32>,
    edge_flags: &mut Vec<[bool; 3]>,
    cx: f64,
    cy: f64,
    z_top: f64,
    z_bot: f64,
    r_top: f64,
    r_bot: f64,
    segments: usize,
) {
    let top_start = positions.len() as u32;
    for i in 0..segments {
        let theta = (i as f64) * 2.0 * std::f64::consts::PI / (segments as f64);
        let norm = Vector3D::new(theta.cos(), theta.sin(), 0.0);
        positions.push(Point3D::new(cx + r_top * theta.cos(), cy + r_top * theta.sin(), z_top));
        normals.push(norm);
    }

    let bot_start = positions.len() as u32;
    for i in 0..segments {
        let theta = (i as f64) * 2.0 * std::f64::consts::PI / (segments as f64);
        let norm = Vector3D::new(theta.cos(), theta.sin(), 0.0);
        positions.push(Point3D::new(cx + r_bot * theta.cos(), cy + r_bot * theta.sin(), z_bot));
        normals.push(norm);
    }

    for i in 0..segments {
        let next = (i + 1) % segments;
        let t0 = top_start + (i as u32);
        let t1 = top_start + (next as u32);
        let b0 = bot_start + (i as u32);
        let b1 = bot_start + (next as u32);

        indices.extend_from_slice(&[t0, b0, b1]);
        edge_flags.push([true, true, false]);

        indices.extend_from_slice(&[t0, b1, t1]);
        edge_flags.push([false, true, true]);
    }
}
