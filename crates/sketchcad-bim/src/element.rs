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
