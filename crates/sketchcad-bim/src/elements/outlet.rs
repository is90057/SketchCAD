use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Plug / receptacle standard (插座規格)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlugStandard {
    /// 美規 NEMA: flat blades (5-15 at 110 V, 6-15 at 220 V) with a round earth pin
    #[default]
    Us,
    /// 歐規 Schuko (CEE 7/3): two round pins in a recessed socket with side earth clips
    Eu,
}

impl PlugStandard {
    pub const ALL: [PlugStandard; 2] = [PlugStandard::Us, PlugStandard::Eu];

    pub fn label(self) -> &'static str {
        match self {
            PlugStandard::Us => "美規 (US NEMA)",
            PlugStandard::Eu => "歐規 (EU Schuko)",
        }
    }
}

/// Supply voltage of an outlet circuit (電壓)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutletVoltage {
    #[default]
    V110,
    V220,
}

impl OutletVoltage {
    pub const ALL: [OutletVoltage; 2] = [OutletVoltage::V110, OutletVoltage::V220];

    pub fn label(self) -> &'static str {
        match self {
            OutletVoltage::V110 => "110V",
            OutletVoltage::V220 => "220V",
        }
    }
}

const PLATE_THICKNESS: f64 = 8.0;
const PLATE_COLOR: [u8; 4] = [246, 246, 242, 255];
const RECESS_COLOR: [u8; 4] = [205, 205, 200, 255];
const CONTACT_COLOR: [u8; 4] = [30, 30, 32, 255];
const EARTH_CLIP_COLOR: [u8; 4] = [190, 160, 90, 255];

/// Wall-mounted power outlet (電源插座). `position` lies on the wall face and `facing`
/// points from that face into the room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outlet {
    pub id: ElementId,
    pub name: String,
    pub host_wall_id: ElementId,
    pub position: Point2D,
    pub facing: Vector2D,
    /// Height of the plate centre above the floor (mm), e.g. 300 for a skirting-level outlet
    pub mount_height: f64,
    pub standard: PlugStandard,
    pub voltage: OutletVoltage,
    /// Receptacles on the plate: 1 (單插) or 2 (雙插)
    pub gangs: u8,
    pub unit_price: f64,
}

impl Outlet {
    pub fn new(
        host_wall_id: ElementId,
        position: Point2D,
        facing: Vector2D,
        standard: PlugStandard,
        voltage: OutletVoltage,
        gangs: u8,
    ) -> Self {
        let mut outlet = Self {
            id: ElementId::new(),
            name: String::new(),
            host_wall_id,
            position,
            facing,
            mount_height: 300.0,
            standard,
            voltage,
            gangs: gangs.clamp(1, 2),
            unit_price: 0.0,
        };
        outlet.refresh_spec();
        outlet
    }

    /// Re-derives the name and the default unit price after standard / voltage / gangs change.
    pub fn refresh_spec(&mut self) {
        self.gangs = self.gangs.clamp(1, 2);
        let gang = if self.gangs == 2 { "雙插" } else { "單插" };
        self.name = format!("{} {} {}插座", self.standard.label(), self.voltage.label(), gang);
        self.unit_price = Self::default_price(self.standard, self.voltage, self.gangs);
    }

    /// Supply-and-install price per outlet (TWD)
    pub fn default_price(standard: PlugStandard, voltage: OutletVoltage, gangs: u8) -> f64 {
        let base = match (standard, voltage) {
            (PlugStandard::Us, OutletVoltage::V110) => 450.0,
            (PlugStandard::Us, OutletVoltage::V220) => 650.0,
            (PlugStandard::Eu, OutletVoltage::V110) => 850.0,
            (PlugStandard::Eu, OutletVoltage::V220) => 900.0,
        };
        base + if gangs >= 2 { 250.0 } else { 0.0 }
    }

    /// Unit vectors: along the wall (x) and out of the wall (y).
    fn axes(&self) -> (Vector2D, Vector2D) {
        let out = if self.facing.length_squared() > 1e-9 { self.facing.normalize() } else { Vector2D::Y };
        (Vector2D::new(out.y, -out.x), out)
    }

    /// Cover plate size (width along the wall, height) in mm.
    fn plate_size(&self) -> (f64, f64) {
        match (self.standard, self.gangs) {
            (PlugStandard::Us, _) => (70.0, 115.0),
            (PlugStandard::Eu, 1) => (80.0, 80.0),
            (PlugStandard::Eu, _) => (151.0, 80.0),
        }
    }

    /// Centres of the receptacles on the plate, as (x along wall, z) offsets from its centre.
    fn receptacle_centres(&self) -> Vec<(f64, f64)> {
        match (self.standard, self.gangs) {
            (_, 1) => vec![(0.0, 0.0)],
            (PlugStandard::Us, _) => vec![(0.0, 25.0), (0.0, -25.0)], // duplex, stacked
            (PlugStandard::Eu, _) => vec![(-35.5, 0.0), (35.5, 0.0)], // side by side
        }
    }

    /// Plate and contacts in the local frame (x along the wall, y out of the wall face, z up
    /// from the plate centre).
    fn local_mesh(&self) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let (pw, ph) = self.plate_size();
        // The plate takes the element colour (paintable); contacts keep their own
        mesh.append(&TriangleMesh::create_box(
            Vector3D::new(pw, PLATE_THICKNESS, ph),
            Point3D::new(0.0, PLATE_THICKNESS * 0.5, 0.0),
        ));
        let face = PLATE_THICKNESS + 0.5;
        for (cx, cz) in self.receptacle_centres() {
            match (self.standard, self.voltage) {
                (PlugStandard::Us, voltage) => {
                    // Blade slots: vertical for NEMA 5-15 (110 V), horizontal for 6-15 (220 V)
                    let slot = if voltage == OutletVoltage::V110 { [3.0, 1.0, 8.0] } else { [8.0, 1.0, 3.0] };
                    for sx in [-6.5, 6.5] {
                        part(&mut mesh, slot, [cx + sx, face, cz + 3.0], CONTACT_COLOR);
                    }
                    part(&mut mesh, [5.0, 1.0, 5.0], [cx, face, cz - 8.0], CONTACT_COLOR);
                }
                (PlugStandard::Eu, _) => {
                    part(&mut mesh, [40.0, 1.0, 40.0], [cx, face, cz], RECESS_COLOR);
                    for sx in [-9.5, 9.5] {
                        part(&mut mesh, [5.0, 1.0, 5.0], [cx + sx, face + 0.6, cz], CONTACT_COLOR);
                    }
                    for sz in [-17.0, 17.0] {
                        part(&mut mesh, [8.0, 1.0, 3.0], [cx, face + 0.6, cz + sz], EARTH_CLIP_COLOR);
                    }
                }
            }
        }
        mesh
    }
}

/// Appends a coloured axis-aligned box of `size` centred at `center` (outlet local frame).
fn part(mesh: &mut TriangleMesh, size: [f64; 3], center: [f64; 3], rgba: [u8; 4]) {
    let mut b = TriangleMesh::create_box(
        Vector3D::new(size[0], size[1], size[2]),
        Point3D::new(center[0], center[1], center[2]),
    );
    b.set_color(rgba);
    mesh.append(&b);
}

impl BimElement for Outlet {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Outlet
    }

    fn dependencies(&self) -> Vec<ElementId> {
        vec![self.host_wall_id]
    }

    fn display_color(&self) -> Option<[u8; 4]> {
        Some(PLATE_COLOR)
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        // Plan symbol off the wall face: circle (US) or square (EU) on a short stem, one
        // bar per receptacle, and a second outline for 220 V circuits.
        let (along, out) = self.axes();
        let r = 90.0;
        let c = self.position + out * (60.0 + r);
        let shape = |radius: f64| -> Polyline2D {
            let pts = match self.standard {
                PlugStandard::Us => (0..24)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / 24.0;
                        c + along * (a.cos() * radius) + out * (a.sin() * radius)
                    })
                    .collect(),
                PlugStandard::Eu => vec![
                    c - along * radius - out * radius,
                    c + along * radius - out * radius,
                    c + along * radius + out * radius,
                    c - along * radius + out * radius,
                ],
            };
            Polyline2D::new(pts, true)
        };

        let mut prims = vec![Polyline2D::new(vec![self.position, c - out * r], false), shape(r)];
        if self.voltage == OutletVoltage::V220 {
            prims.push(shape(r * 0.72));
        }
        let bars: &[f64] = if self.gangs >= 2 { &[-25.0, 25.0] } else { &[0.0] };
        for &o in bars {
            let p = c + out * o;
            prims.push(Polyline2D::new(vec![p - along * (r * 0.55), p + along * (r * 0.55)], false));
        }
        prims
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let (along, out) = self.axes();
        let mut mesh = self.local_mesh();
        for p in &mut mesh.positions {
            let (lx, ly) = (p.x, p.y);
            p.x = self.position.x + lx * along.x + ly * out.x;
            p.y = self.position.y + lx * along.y + ly * out.y;
            p.z += self.mount_height;
        }
        for n in &mut mesh.normals {
            let (lx, ly) = (n.x, n.y);
            n.x = lx * along.x + ly * out.x;
            n.y = lx * along.y + ly * out.y;
        }
        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        qto.add_item(CostLineItem::new(
            self.id,
            "Electrical 電氣插座工程",
            format!("{} (含配管配線與面板安裝)", self.name),
            1.0,
            "組",
            self.unit_price,
        ));
        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        self.position.x += delta.x;
        self.position.y += delta.y;
        self.mount_height += delta.z;
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

    fn outlet(standard: PlugStandard, voltage: OutletVoltage, gangs: u8) -> Outlet {
        Outlet::new(ElementId::new(), Point2D::new(1000.0, 100.0), Vector2D::Y, standard, voltage, gangs)
    }

    #[test]
    fn us_blades_turn_horizontal_at_220v() {
        // The first contact of a receptacle is a blade slot; compare its extent along the wall vs height
        let blade_extent = |o: &Outlet| {
            let mesh = o.local_mesh();
            let slot = &mesh.positions[24..48]; // plate box = 24 vertices, then the first slot
            let span = |f: fn(&Point3D) -> f64| {
                let v: Vec<f64> = slot.iter().map(f).collect();
                v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
            };
            (span(|p| p.x), span(|p| p.z))
        };
        let (w110, h110) = blade_extent(&outlet(PlugStandard::Us, OutletVoltage::V110, 1));
        let (w220, h220) = blade_extent(&outlet(PlugStandard::Us, OutletVoltage::V220, 1));
        assert!(h110 > w110, "NEMA 5-15 blades are vertical");
        assert!(w220 > h220, "NEMA 6-15 blades are horizontal");
    }

    #[test]
    fn mesh_sits_on_the_wall_face_with_coloured_contacts() {
        for standard in PlugStandard::ALL {
            for voltage in OutletVoltage::ALL {
                for gangs in [1, 2] {
                    let o = outlet(standard, voltage, gangs);
                    let mesh = o.to_3d_mesh();
                    assert_eq!(mesh.face_colors.len(), mesh.triangle_count());
                    assert!(mesh.face_colors[..12].iter().all(Option::is_none), "plate takes the element colour");
                    assert!(mesh.face_colors[12..].iter().all(Option::is_some), "contacts are coloured");
                    let min_y = mesh.positions.iter().map(|p| p.y).fold(f64::MAX, f64::min);
                    assert!((min_y - 100.0).abs() < 1e-6, "plate back lies on the wall face");
                    assert!(o.name.contains(voltage.label()));
                }
            }
        }
    }

    #[test]
    fn spec_change_updates_name_and_price() {
        let mut o = outlet(PlugStandard::Us, OutletVoltage::V110, 2);
        let price_110 = o.unit_price;
        o.voltage = OutletVoltage::V220;
        o.refresh_spec();
        assert!(o.unit_price > price_110);
        assert!(o.name.contains("220V") && o.name.contains("雙插"));
        assert_eq!(o.to_2d_primitives().len(), 5, "stem + two outlines + two bars");
    }
}
