use crate::cost::QuantityTakeoff;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementKind {
    Wall,
    Door,
    Window,
    Column,
    Beam,
    Slab,
    Ceiling,
    Stairs,
    Railing,
    Roof,
    Ramp,
    CurtainWall,
    CurtainGrid,
    Mullion,
    Space,
    Furniture,
    LightFixture,
    Outlet,
    /// Instance of a 3D model-library model (模型百寶箱)
    Model,
    /// 2D drafting line work: line, polyline, circle, arc, spline (繪製)
    Draft,
    /// 2D hatch fill (填充線)
    Hatch,
    /// Dimensions, text and leaders (註解)
    Annotation,
}

impl ElementKind {
    /// Default architectural display colour (sRGB RGBA; alpha < 255 = translucent), shared by
    /// the 3D viewport and colour-carrying exports such as SketchUp.
    pub fn display_rgba(self) -> [u8; 4] {
        match self {
            ElementKind::Stairs => [215, 160, 95, 255],
            ElementKind::Railing => [210, 220, 235, 180],
            ElementKind::Wall => [235, 235, 230, 255],
            ElementKind::Column => [165, 175, 185, 255],
            ElementKind::Beam => [95, 115, 135, 255],
            ElementKind::Slab => [190, 195, 200, 255],
            ElementKind::Ceiling => [230, 240, 250, 60],
            ElementKind::Door => [175, 115, 65, 255],
            ElementKind::Window => [135, 210, 245, 130],
            ElementKind::Roof => [200, 95, 70, 255],
            ElementKind::Ramp => [175, 180, 190, 255],
            ElementKind::CurtainWall => [120, 205, 240, 140],
            ElementKind::CurtainGrid => [100, 180, 220, 120],
            ElementKind::Mullion => [70, 75, 85, 255],
            ElementKind::Space => [200, 225, 240, 80], // Translucent floor finish plate
            ElementKind::Furniture => [165, 125, 85, 255], // Warm solid wood / upholstery
            ElementKind::LightFixture => [48, 52, 58, 255], // Luminaire hardware (chassis / trim)
            ElementKind::Outlet => [246, 246, 242, 255], // Wiring-device cover plate
            ElementKind::Model => [190, 185, 175, 255],
            ElementKind::Draft => [235, 235, 240, 255],
            ElementKind::Hatch => [200, 200, 210, 255],
            ElementKind::Annotation => [140, 220, 255, 255],
        }
    }
}

/// Core interface implemented by all BIM Smart Objects
pub trait BimElement: std::fmt::Debug + Send + Sync + 'static {
    fn id(&self) -> ElementId;
    fn name(&self) -> &str;
    fn kind(&self) -> ElementKind;

    /// Generates 2D planar drafting symbols (Plan view)
    fn to_2d_primitives(&self) -> Vec<Polyline2D>;

    /// Generates 3D geometric mesh for direct viewport orbit & IFC export
    fn to_3d_mesh(&self) -> TriangleMesh;

    /// Computes engineering quantities and bill of materials costs
    fn calculate_qto(&self) -> QuantityTakeoff;

    /// Translate / move the element in 3D world space
    fn translate(&mut self, delta: Vector3D);

    /// Explicit display colour (sRGB RGBA) from the source model, e.g. a SketchUp material.
    /// `None` uses the viewport's palette for the element kind.
    fn display_color(&self) -> Option<[u8; 4]> {
        None
    }

    /// Other element IDs this element depends on (e.g. Door depends on host Wall)
    fn dependencies(&self) -> Vec<ElementId> {
        Vec::new()
    }

    /// Clone into a boxed trait object
    fn clone_box(&self) -> Box<dyn BimElement>;

    /// Duplicate this element with a brand new ElementId
    fn clone_with_new_id(&self) -> Box<dyn BimElement>;

    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
