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

/// Operable configuration of a window (窗型). Opening sashes swing / project toward the
/// outside, i.e. the wall's right-hand normal side (the opposite of a door's "inward").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WindowType {
    /// 固定窗: single fixed pane
    Fixed,
    /// 橫拉窗: two horizontally sliding sashes in parallel tracks
    #[default]
    Sliding,
    /// 推開窗: side-hung sash(es); two leaves above 1000 mm width
    Casement,
    /// 推射窗: top-hung sash projecting outward
    Awning,
    /// 上下拉窗: two vertically sliding sashes
    DoubleHung,
    /// 百葉窗: tilted fixed blades for ventilation
    Louver,
}

impl WindowType {
    pub const ALL: [WindowType; 6] = [
        WindowType::Fixed,
        WindowType::Sliding,
        WindowType::Casement,
        WindowType::Awning,
        WindowType::DoubleHung,
        WindowType::Louver,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WindowType::Fixed => "固定窗 (Fixed)",
            WindowType::Sliding => "橫拉窗 (Sliding)",
            WindowType::Casement => "推開窗 (Casement)",
            WindowType::Awning => "推射窗 (Awning)",
            WindowType::DoubleHung => "上下拉窗 (Double-Hung)",
            WindowType::Louver => "百葉窗 (Louver)",
        }
    }

    /// IFC4 IfcWindowTypePartitioningEnum of a window of this type and `width`.
    pub fn ifc_partitioning(self, width: f64) -> &'static str {
        match self {
            WindowType::Sliding => "DOUBLE_PANEL_VERTICAL",
            WindowType::DoubleHung => "DOUBLE_PANEL_HORIZONTAL",
            WindowType::Casement if casement_leaves(width) == 2 => "DOUBLE_PANEL_VERTICAL",
            _ => "SINGLE_PANEL",
        }
    }
}

/// Number of casement leaves for an opening of `width` mm.
fn casement_leaves(width: f64) -> usize {
    if width > 1000.0 { 2 } else { 1 }
}

/// Outer frame profile face width and depth (mm)
const FRAME_FACE: f64 = 60.0;
const FRAME_DEPTH: f64 = 100.0;
/// Sash profile face width and depth (mm)
const SASH_FACE: f64 = 45.0;
const SASH_DEPTH: f64 = 40.0;
const GLASS_THICKNESS: f64 = 8.0;
/// Overlap of sliding sashes at the meeting stiles / rails (mm)
const SASH_OVERLAP: f64 = 50.0;
/// Depth offset of each sliding track from the frame centre (mm)
const TRACK_OFFSET: f64 = 22.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub id: ElementId,
    pub name: String,
    pub host_wall_id: ElementId,
    pub offset_along_wall: f64, // mm from wall start
    pub sill_height: f64,       // mm from floor level, e.g. 900.0
    pub width: f64,             // mm, e.g. 1500.0
    pub height: f64,            // mm, e.g. 1200.0
    #[serde(default)]
    pub window_type: WindowType,
    /// Frame and sash material (窗框); the window's paintable surface
    #[serde(default = "Material::aluminum")]
    pub frame_material: Material,
    /// Glass in the sashes (玻璃); projects before window types saved it as `material`
    #[serde(alias = "material")]
    pub glazing: Material,
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
        glazing: Material,
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
            window_type: WindowType::default(),
            frame_material: Material::aluminum(),
            glazing,
            unit_price,
            world_pos: Point2D::ZERO,
            wall_dir: Vector2D::X,
        }
    }

    pub fn set_transform(&mut self, pos: Point2D, dir: Vector2D) {
        self.world_pos = pos;
        self.wall_dir = dir;
    }

    /// Unit wall direction and its left-hand normal (the "inside" side).
    fn axes(&self) -> (Vector2D, Vector2D) {
        let dir = if self.wall_dir.length_squared() > 1e-6 {
            self.wall_dir.normalize()
        } else {
            Vector2D::X
        };
        (dir, Vector2D::new(-dir.y, dir.x))
    }

    /// Frame, sashes and glass in the window's local frame: x along the wall, y along the
    /// inside normal, z up, origin at the centre of the opening. Glass triangles carry the
    /// glazing colour; frame triangles have none so they take the element (frame / paint) colour.
    fn local_mesh(&self) -> TriangleMesh {
        let glass = Some(self.glazing.rgba());
        let (w, h) = (self.width.max(2.0 * FRAME_FACE + 10.0), self.height.max(2.0 * FRAME_FACE + 10.0));
        let (iw, ih) = (w - 2.0 * FRAME_FACE, h - 2.0 * FRAME_FACE);
        let mut mesh = TriangleMesh::new();

        // Outer frame: head, sill and two jambs
        add_box(&mut mesh, [w, FRAME_DEPTH, FRAME_FACE], [0.0, 0.0, (h - FRAME_FACE) * 0.5], None);
        add_box(&mut mesh, [w, FRAME_DEPTH, FRAME_FACE], [0.0, 0.0, -(h - FRAME_FACE) * 0.5], None);
        for side in [-1.0, 1.0] {
            add_box(&mut mesh, [FRAME_FACE, FRAME_DEPTH, ih], [side * (w - FRAME_FACE) * 0.5, 0.0, 0.0], None);
        }

        let (x0, x1, z0, z1) = (-iw * 0.5, iw * 0.5, -ih * 0.5, ih * 0.5);
        match self.window_type {
            WindowType::Fixed => {
                add_box(&mut mesh, [iw, GLASS_THICKNESS, ih], [0.0, 0.0, 0.0], glass);
            }
            WindowType::Sliding => {
                let o = SASH_OVERLAP * 0.5;
                add_sash(&mut mesh, [x0, o], [z0, z1], -TRACK_OFFSET, glass);
                add_sash(&mut mesh, [-o, x1], [z0, z1], TRACK_OFFSET, glass);
            }
            WindowType::DoubleHung => {
                let o = SASH_OVERLAP * 0.5;
                add_sash(&mut mesh, [x0, x1], [z0, o], -TRACK_OFFSET, glass);
                add_sash(&mut mesh, [x0, x1], [-o, z1], TRACK_OFFSET, glass);
            }
            WindowType::Casement => {
                if casement_leaves(w) == 2 {
                    add_box(&mut mesh, [FRAME_FACE, FRAME_DEPTH, ih], [0.0, 0.0, 0.0], None);
                    let m = FRAME_FACE * 0.5;
                    add_sash(&mut mesh, [x0, -m], [z0, z1], 0.0, glass);
                    add_sash(&mut mesh, [m, x1], [z0, z1], 0.0, glass);
                } else {
                    add_sash(&mut mesh, [x0, x1], [z0, z1], 0.0, glass);
                }
            }
            WindowType::Awning => {
                add_sash(&mut mesh, [x0, x1], [z0, z1], 0.0, glass);
                // Bottom handle on the inside face
                add_box(&mut mesh, [120.0, 25.0, 20.0], [0.0, SASH_DEPTH * 0.5 + 12.0, z0 + SASH_FACE * 0.5], None);
            }
            WindowType::Louver => {
                // Glass screen behind tilted blades
                add_box(&mut mesh, [iw, GLASS_THICKNESS, ih], [0.0, FRAME_DEPTH * 0.3, 0.0], glass);
                let blades = ((ih / 90.0).floor() as usize).max(1);
                let tilt = 30f64.to_radians();
                for i in 0..blades {
                    let z = z0 + ih * (i as f64 + 0.5) / blades as f64;
                    let mut blade = TriangleMesh::create_box(Vector3D::new(iw, 85.0, 4.0), Point3D::ZERO);
                    let (sin, cos) = tilt.sin_cos();
                    for p in &mut blade.positions {
                        let (y, pz) = (p.y, p.z);
                        p.y = y * cos - pz * sin - FRAME_DEPTH * 0.1;
                        p.z = y * sin + pz * cos + z;
                    }
                    for n in &mut blade.normals {
                        let (y, nz) = (n.y, n.z);
                        n.y = y * cos - nz * sin;
                        n.z = y * sin + nz * cos;
                    }
                    mesh.append(&blade);
                }
            }
        }
        mesh
    }
}

/// Appends an axis-aligned box of `size` centred at `center` (local window frame).
fn add_box(mesh: &mut TriangleMesh, size: [f64; 3], center: [f64; 3], color: Option<[u8; 4]>) {
    let mut b = TriangleMesh::create_box(
        Vector3D::new(size[0], size[1], size[2]),
        Point3D::new(center[0], center[1], center[2]),
    );
    if let Some(rgba) = color {
        b.set_color(rgba);
    }
    mesh.append(&b);
}

/// Appends a glazed sash filling `xs` x `zs` at depth offset `y`: four profile members and a pane.
fn add_sash(mesh: &mut TriangleMesh, xs: [f64; 2], zs: [f64; 2], y: f64, glass: Option<[u8; 4]>) {
    let (sw, sh) = (xs[1] - xs[0], zs[1] - zs[0]);
    let (cx, cz) = ((xs[0] + xs[1]) * 0.5, (zs[0] + zs[1]) * 0.5);
    let s = SASH_FACE.min(sw * 0.25).min(sh * 0.25);
    add_box(mesh, [sw, SASH_DEPTH, s], [cx, y, zs[1] - s * 0.5], None);
    add_box(mesh, [sw, SASH_DEPTH, s], [cx, y, zs[0] + s * 0.5], None);
    add_box(mesh, [s, SASH_DEPTH, sh - 2.0 * s], [xs[0] + s * 0.5, y, cz], None);
    add_box(mesh, [s, SASH_DEPTH, sh - 2.0 * s], [xs[1] - s * 0.5, y, cz], None);
    add_box(mesh, [sw - 2.0 * s, GLASS_THICKNESS, sh - 2.0 * s], [cx, y, cz], glass);
}

/// Points of a circular arc around `center` from `from` sweeping toward `to` (both at radius).
fn arc_points(center: Point2D, from: Vector2D, to: Vector2D, segments: usize) -> Vec<Point2D> {
    let a0 = from.y.atan2(from.x);
    let mut sweep = to.y.atan2(to.x) - a0;
    if sweep > std::f64::consts::PI {
        sweep -= std::f64::consts::TAU;
    } else if sweep < -std::f64::consts::PI {
        sweep += std::f64::consts::TAU;
    }
    let r = from.length();
    (0..=segments)
        .map(|i| {
            let a = a0 + sweep * i as f64 / segments as f64;
            center + Vector2D::new(a.cos(), a.sin()) * r
        })
        .collect()
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
        // CAD window plan symbol: frame outline in the wall plus a per-type sash symbol
        let (dir, norm) = self.axes();
        let half_w = self.width * 0.5;
        let half_d = 80.0; // 160mm total thickness
        let at = |along: f64, across: f64| self.world_pos + dir * along + norm * across;
        let line = |a: Point2D, b: Point2D| Polyline2D::new(vec![a, b], false);

        let mut symbol = vec![Polyline2D::new(
            vec![at(-half_w, -half_d), at(half_w, -half_d), at(half_w, half_d), at(-half_w, half_d)],
            true,
        )];
        match self.window_type {
            WindowType::Fixed => symbol.push(line(at(-half_w, 0.0), at(half_w, 0.0))),
            WindowType::Sliding => {
                let o = SASH_OVERLAP;
                symbol.push(line(at(-half_w, -TRACK_OFFSET), at(o, -TRACK_OFFSET)));
                symbol.push(line(at(-o, TRACK_OFFSET), at(half_w, TRACK_OFFSET)));
            }
            WindowType::DoubleHung => {
                symbol.push(line(at(-half_w, -TRACK_OFFSET), at(half_w, -TRACK_OFFSET)));
                symbol.push(line(at(-half_w, TRACK_OFFSET), at(half_w, TRACK_OFFSET)));
            }
            WindowType::Casement => {
                // Each leaf drawn opened 90° outward from its jamb, with its swing arc
                let leaves: Vec<(f64, f64)> = if casement_leaves(self.width) == 2 {
                    vec![(-half_w, 0.0), (half_w, 0.0)]
                } else {
                    vec![(-half_w, half_w)]
                };
                symbol.push(line(at(-half_w, 0.0), at(half_w, 0.0)));
                for (hinge, tip) in leaves {
                    let leaf = (tip - hinge).abs();
                    let open_tip = at(hinge, -half_d - leaf);
                    symbol.push(line(at(hinge, -half_d), open_tip));
                    symbol.push(Polyline2D::new(
                        arc_points(at(hinge, -half_d), open_tip - at(hinge, -half_d), at(tip, -half_d) - at(hinge, -half_d), 12),
                        false,
                    ));
                }
            }
            WindowType::Awning => {
                // Sash projected outward in its open position
                let proj = (self.height * 0.25).clamp(150.0, 400.0);
                symbol.push(line(at(-half_w, 0.0), at(half_w, 0.0)));
                symbol.push(Polyline2D::new(
                    vec![at(-half_w, -half_d), at(-half_w, -half_d - proj), at(half_w, -half_d - proj), at(half_w, -half_d)],
                    false,
                ));
            }
            WindowType::Louver => {
                symbol.push(line(at(-half_w, 0.0), at(half_w, 0.0)));
                let count = ((self.width / 120.0).floor() as usize).max(2);
                for i in 1..count {
                    let x = -half_w + self.width * i as f64 / count as f64;
                    symbol.push(line(at(x - 40.0, -half_d * 0.7), at(x + 40.0, half_d * 0.7)));
                }
            }
        }
        symbol
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let (dir, norm) = self.axes();
        let center = Point3D::new(self.world_pos.x, self.world_pos.y, self.sill_height + self.height * 0.5);
        let mut mesh = self.local_mesh();
        for p in &mut mesh.positions {
            let (lx, ly) = (p.x, p.y);
            p.x = center.x + lx * dir.x + ly * norm.x;
            p.y = center.y + lx * dir.y + ly * norm.y;
            p.z += center.z;
        }
        for n in &mut mesh.normals {
            let (lx, ly) = (n.x, n.y);
            n.x = lx * dir.x + ly * norm.x;
            n.y = lx * dir.y + ly * norm.y;
        }
        mesh
    }

    fn display_color(&self) -> Option<[u8; 4]> {
        Some(self.frame_material.rgba())
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let area_m2 = (self.width / 1000.0) * (self.height / 1000.0);
        qto.add_item(CostLineItem::new(
            self.id,
            "Doors & Windows 門窗工程",
            format!("{} · {} ({:.2} m²)", self.name, self.window_type.label(), area_m2),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn window(window_type: WindowType, width: f64) -> Window {
        let mut w = Window::new(ElementId::new(), 0.0, 900.0, width, 1400.0, Material::low_e_glass(), 0.0);
        w.window_type = window_type;
        w.frame_material = Material::black_aluminum();
        w
    }

    #[test]
    fn every_type_has_frame_and_glass_surfaces() {
        for t in WindowType::ALL {
            let mesh = window(t, 1800.0).to_3d_mesh();
            assert_eq!(mesh.face_colors.len(), mesh.triangle_count(), "{t:?}");
            let glass = (0..mesh.triangle_count()).filter(|&i| mesh.face_color(i) == Some(Material::low_e_glass().rgba())).count();
            assert!(glass > 0, "{t:?} has glass");
            assert!(glass < mesh.triangle_count(), "{t:?} has frame members");
        }
    }

    #[test]
    fn types_draw_distinct_plan_symbols() {
        let symbols: Vec<usize> = WindowType::ALL.iter().map(|&t| window(t, 1800.0).to_2d_primitives().len()).collect();
        assert_eq!(symbols[0], 2, "fixed: frame + glass line");
        assert_eq!(symbols[1], 3, "sliding: frame + two sashes");
        // Wide casements have two leaves (line + arc each) around the glass line
        assert_eq!(window(WindowType::Casement, 1800.0).to_2d_primitives().len(), 6);
        assert_eq!(window(WindowType::Casement, 800.0).to_2d_primitives().len(), 4);
    }

    #[test]
    fn frame_is_the_display_colour_and_old_projects_keep_their_glass() {
        let w = window(WindowType::Sliding, 1200.0);
        assert_eq!(w.display_color(), Some(Material::black_aluminum().rgba()));

        // A window saved before types existed stored its glass as `material`
        let mut json = serde_json::to_value(&w).unwrap();
        let obj = json.as_object_mut().unwrap();
        let glass = obj.remove("glazing").unwrap();
        obj.insert("material".into(), glass);
        obj.remove("window_type");
        obj.remove("frame_material");
        let old: Window = serde_json::from_value(json).unwrap();
        assert_eq!(old.glazing, Material::low_e_glass());
        assert_eq!(old.window_type, WindowType::Sliding);
        assert_eq!(old.frame_material, Material::aluminum());
    }
}
