use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// Generic IFC building element supporting full 3D geometry meshes and 2D floorplan footprints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfcComponent {
    pub id: ElementId,
    pub name: String,
    pub kind: ElementKind,
    pub mesh: TriangleMesh,
    pub primitives_2d: Vec<Polyline2D>,
    pub material: Material,
    pub storey_name: Option<String>,
    pub base_z: f64,
    pub volume_m3: f64,
    pub area_m2: f64,
    /// Colour from the source model (SketchUp material); `None` = palette colour for `kind`
    #[serde(default)]
    pub display_color: Option<[u8; 4]>,
}

impl IfcComponent {
    pub fn new(
        name: impl Into<String>,
        kind: ElementKind,
        mesh: TriangleMesh,
        primitives_2d: Vec<Polyline2D>,
        material: Material,
    ) -> Self {
        let bbox = mesh.bbox();
        let base_z = bbox.as_ref().map(|b| b.min.z).unwrap_or(0.0);
        let volume_m3 = bbox
            .as_ref()
            .map(|b| {
                let s = b.size();
                (s.x / 1000.0) * (s.y / 1000.0) * (s.z / 1000.0)
            })
            .unwrap_or(0.0);
        let area_m2 = primitives_2d
            .iter()
            .map(|p| p.area() / 1_000_000.0)
            .sum::<f64>();

        Self {
            id: ElementId::new(),
            name: name.into(),
            kind,
            mesh,
            primitives_2d,
            material,
            storey_name: None,
            base_z,
            volume_m3,
            area_m2,
            display_color: None,
        }
    }

    pub fn with_id(mut self, id: ElementId) -> Self {
        self.id = id;
        self
    }

    pub fn with_display_color(mut self, rgba: [u8; 4]) -> Self {
        self.display_color = Some(rgba);
        self
    }

    pub fn with_storey(mut self, storey: impl Into<String>) -> Self {
        self.storey_name = Some(storey.into());
        self
    }
}

impl BimElement for IfcComponent {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        self.kind
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        self.primitives_2d.clone()
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        self.mesh.clone()
    }

    fn display_color(&self) -> Option<[u8; 4]> {
        self.display_color
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        let unit_cost = self.material.cost_per_m3;
        let item = CostLineItem::new(
            self.id,
            format!("{:?}", self.kind),
            format!("{} ({})", self.name, self.material.name),
            self.volume_m3,
            "m³",
            unit_cost,
        );
        qto.add_item(item);
        qto
    }

    fn translate(&mut self, delta: Vector3D) {
        for p in &mut self.mesh.positions {
            *p = Point3D::new(p.x + delta.x, p.y + delta.y, p.z + delta.z);
        }
        for poly in &mut self.primitives_2d {
            for v in &mut poly.vertices {
                *v = Point2D::new(v.x + delta.x, v.y + delta.y);
            }
        }
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
