use crate::cost::{CostLineItem, QuantityTakeoff};
use crate::element::{BimElement, ElementKind};
use crate::model_library::{ModelCategory, ModelSpec};
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{Polyline2D, TriangleMesh};

/// A placed instance of a 3D model-library model (模型百寶箱元件). Only the catalogue id and
/// the placement are stored; the geometry comes from the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelComponent {
    pub id: ElementId,
    pub name: String,
    pub model_id: String,
    pub position: Point2D,
    pub base_z: f64,
    pub rotation_deg: f64,
    /// Uniform scale factor (1.0 = catalogue size).
    pub scale: f64,
    pub unit_price: f64,
}

impl ModelComponent {
    /// New instance of catalogue model `spec` at its default elevation and price.
    pub fn new(spec: &ModelSpec, position: Point2D, rotation_deg: f64) -> Self {
        Self {
            id: ElementId::new(),
            name: spec.name.to_string(),
            model_id: spec.id.to_string(),
            position,
            base_z: spec.elevation,
            rotation_deg,
            scale: 1.0,
            unit_price: spec.price,
        }
    }

    pub fn spec(&self) -> Option<&'static ModelSpec> {
        ModelSpec::find(&self.model_id)
    }

    pub fn category(&self) -> Option<ModelCategory> {
        self.spec().map(|s| s.category)
    }

    /// Overall (width, depth, height) after scaling, in mm.
    pub fn size(&self) -> (f64, f64, f64) {
        let (w, d, h) = self.spec().map(|s| s.geometry().size).unwrap_or((500.0, 500.0, 500.0));
        (w * self.scale, d * self.scale, h * self.scale)
    }

    fn local_to_world(&self, x: f64, y: f64) -> Point2D {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        let (x, y) = (x * self.scale, y * self.scale);
        Point2D::new(self.position.x + x * c - y * s, self.position.y + x * s + y * c)
    }

    /// World-space plan outline (closed, convex).
    pub fn outline(&self) -> Vec<Point2D> {
        match self.spec() {
            Some(spec) => spec.geometry().outline.iter().map(|&(x, y)| self.local_to_world(x, y)).collect(),
            None => [(-250.0, -250.0), (250.0, -250.0), (250.0, 250.0), (-250.0, 250.0)]
                .iter()
                .map(|&(x, y)| self.local_to_world(x, y))
                .collect(),
        }
    }
}

impl BimElement for ModelComponent {
    fn id(&self) -> ElementId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ElementKind {
        ElementKind::Model
    }

    fn display_color(&self) -> Option<[u8; 4]> {
        self.spec().map(|s| s.body_color)
    }

    fn to_2d_primitives(&self) -> Vec<Polyline2D> {
        let outline = self.outline();
        let mut prims = vec![Polyline2D::new(outline.clone(), true)];
        if self.category().is_some_and(|c| c.is_planting()) && !outline.is_empty() {
            // Tree / shrub symbol: inner crown outline and radial branches
            let centre = self.position;
            let inner: Vec<Point2D> = outline.iter().map(|p| centre + (*p - centre) * 0.55).collect();
            prims.push(Polyline2D::new(inner, true));
            let step = (outline.len() / 6).max(1);
            for p in outline.iter().step_by(step) {
                prims.push(Polyline2D::new(vec![centre, centre + (*p - centre) * 0.85], false));
            }
        } else {
            // Front marker: a short tick from the centre toward the model's front (−Y)
            let (_, d, _) = self.size();
            prims.push(Polyline2D::new(vec![self.position, self.local_to_world(0.0, -d * 0.35 / self.scale.max(1e-6))], false));
        }
        prims
    }

    fn to_3d_mesh(&self) -> TriangleMesh {
        let Some(spec) = self.spec() else {
            let (s, c) = self.rotation_deg.to_radians().sin_cos();
            let half = 250.0 * self.scale;
            return TriangleMesh::create_oriented_box(
                Point3D::new(self.position.x - half * c, self.position.y - half * s, self.base_z + half),
                Point3D::new(self.position.x + half * c, self.position.y + half * s, self.base_z + half),
                Vector3D::new(-s, c, 0.0) * (half * 2.0),
                Vector3D::new(0.0, 0.0, half * 2.0),
            );
        };
        let geometry = spec.geometry();
        let mut mesh = geometry.mesh.clone();
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        for p in &mut mesh.positions {
            let (x, y, z) = (p.x * self.scale, p.y * self.scale, p.z * self.scale);
            *p = Point3D::new(self.position.x + x * c - y * s, self.position.y + x * s + y * c, self.base_z + z);
        }
        for n in &mut mesh.normals {
            *n = Vector3D::new(n.x * c - n.y * s, n.x * s + n.y * c, n.z);
        }
        mesh
    }

    fn calculate_qto(&self) -> QuantityTakeoff {
        let mut qto = QuantityTakeoff::new();
        if self.unit_price > 0.0 {
            let category = self.category().map_or("Model 模型元件", |c| c.qto_category());
            qto.add_item(CostLineItem::new(self.id, category, format!("{} (模型庫元件)", self.name), 1.0, "組", self.unit_price));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_is_placed_rotated_and_scaled_in_world_space() {
        let spec = ModelSpec::find("fridge").expect("catalogue has a fridge");
        let mut m = ModelComponent::new(spec, Point2D::new(5000.0, 2000.0), 90.0);
        m.scale = 2.0;
        let (w, d, h) = spec.geometry().size;
        assert_eq!(m.size(), (w * 2.0, d * 2.0, h * 2.0));

        let bbox = m.to_3d_mesh().bbox().unwrap();
        assert!((bbox.min.z - 0.0).abs() < 1e-6 && (bbox.max.z - h * 2.0).abs() < 1e-6);
        // Rotated 90°: the model's width now runs along world Y
        assert!((bbox.size().y - w * 2.0).abs() < 1.0 && (bbox.size().x - d * 2.0).abs() < 1.0);
        assert!((bbox.center().x - 5000.0).abs() < 1.0 && (bbox.center().y - 2000.0).abs() < 1.0);

        let ac = ModelComponent::new(ModelSpec::find("ac_indoor").unwrap(), Point2D::ZERO, 0.0);
        assert_eq!(ac.base_z, 2200.0, "wall units mount at their default height");
        assert!(ac.to_3d_mesh().bbox().unwrap().min.z >= 2199.9);

        assert_eq!(m.calculate_qto().items.len(), 1);
        let car = ModelComponent::new(ModelSpec::find("sedan").unwrap(), Point2D::ZERO, 0.0);
        assert!(car.calculate_qto().items.is_empty(), "entourage is not costed");
        assert!(car.to_2d_primitives()[0].closed);
    }

    #[test]
    fn unknown_model_falls_back_to_a_placeholder_box() {
        let mut m = ModelComponent::new(ModelSpec::find("sedan").unwrap(), Point2D::ZERO, 0.0);
        m.model_id = "missing".into();
        assert_eq!(m.to_3d_mesh().triangle_count(), 12);
        assert_eq!(m.outline().len(), 4);
    }
}
