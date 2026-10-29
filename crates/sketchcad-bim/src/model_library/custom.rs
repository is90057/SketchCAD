//! User models of the 3D model library: meshes imported from OBJ / SketchUp files, built
//! from project elements or exchanged as model packs (`*.skmodel`). They live in a
//! process-wide registry next to the built-in catalogue so placed instances resolve them
//! by id exactly like built-in models.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use sketchcad_core::Vector3D;
use sketchcad_geom::TriangleMesh;

use super::{normalize_mesh, ModelCategory, ModelGeometry, ModelSpec, CATALOG};

/// A user-supplied library model with its own mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomModel {
    pub id: String,
    pub name: String,
    pub category: ModelCategory,
    /// Colour of the triangles without a face colour (the paintable body).
    pub body_color: [u8; 4],
    pub price: f64,
    pub elevation: f64,
    /// Local mesh in millimetres, Z up, front facing −Y.
    pub mesh: TriangleMesh,
}

impl CustomModel {
    /// Wraps an imported mesh (already in millimetres, Z up). The most common face colour
    /// becomes the paintable body colour; the mesh is centred and grounded.
    pub fn from_mesh(name: impl Into<String>, category: ModelCategory, mut mesh: TriangleMesh) -> Self {
        if mesh.normals.len() != mesh.positions.len() {
            recompute_flat_normals(&mut mesh);
        }
        let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
        for c in mesh.face_colors.iter().flatten() {
            *counts.entry(*c).or_default() += 1;
        }
        let body_color = counts.into_iter().max_by_key(|(c, n)| (*n, *c)).map(|(c, _)| c);
        if let Some(body) = body_color {
            for c in &mut mesh.face_colors {
                if *c == Some(body) {
                    *c = None;
                }
            }
        }
        if mesh.face_colors.iter().all(Option::is_none) {
            mesh.face_colors.clear();
        }
        normalize_mesh(&mut mesh);
        mesh.edge_flags.clear();
        Self {
            id: format!("custom-{}", uuid::Uuid::new_v4().simple()),
            name: name.into(),
            category,
            body_color: body_color.unwrap_or([200, 200, 200, 255]),
            price: 0.0,
            elevation: 0.0,
            mesh,
        }
    }

    /// Copy of the mesh with every triangle coloured (body triangles get the body colour).
    pub fn coloured_mesh(&self) -> TriangleMesh {
        let mut mesh = self.mesh.clone();
        mesh.face_colors = (0..mesh.triangle_count()).map(|i| Some(self.mesh.face_color(i).unwrap_or(self.body_color))).collect();
        mesh
    }
}

/// Gives every vertex the normal of its triangle (meshes without usable normals).
fn recompute_flat_normals(mesh: &mut TriangleMesh) {
    mesh.normals = vec![Vector3D::Z; mesh.positions.len()];
    for t in 0..mesh.triangle_count() {
        let idx = [0, 1, 2].map(|k| mesh.indices[t * 3 + k] as usize);
        let [a, b, c] = idx.map(|i| mesh.positions[i]);
        let n = (b - a).cross(c - a);
        if n.length() > 1e-12 {
            for i in idx {
                mesh.normals[i] = n.normalize();
            }
        }
    }
}

/// Converts an imported mesh to SketchCAD's frame: scales it to millimetres and, for
/// Y-up sources (most OBJ files), turns +Y into +Z and the source front (+Z) into −Y.
pub fn convert_imported_mesh(mesh: &mut TriangleMesh, scale_to_mm: f64, y_up: bool) {
    let map = |v: Vector3D| if y_up { Vector3D::new(v.x, -v.z, v.y) } else { v };
    for p in &mut mesh.positions {
        *p = map(*p) * scale_to_mm;
    }
    for n in &mut mesh.normals {
        *n = map(*n);
    }
}

/// Registered user model and its derived geometry.
#[derive(Debug)]
pub struct CustomEntry {
    pub model: CustomModel,
    pub geometry: Arc<ModelGeometry>,
}

/// A library model, built-in or user-supplied.
#[derive(Debug, Clone)]
pub enum ModelRef {
    BuiltIn(&'static ModelSpec),
    Custom(Arc<CustomEntry>),
}

impl ModelRef {
    pub fn id(&self) -> &str {
        match self {
            Self::BuiltIn(s) => s.id,
            Self::Custom(c) => &c.model.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::BuiltIn(s) => s.name,
            Self::Custom(c) => &c.model.name,
        }
    }

    pub fn category(&self) -> ModelCategory {
        match self {
            Self::BuiltIn(s) => s.category,
            Self::Custom(c) => c.model.category,
        }
    }

    pub fn body_color(&self) -> [u8; 4] {
        match self {
            Self::BuiltIn(s) => s.body_color,
            Self::Custom(c) => c.model.body_color,
        }
    }

    pub fn price(&self) -> f64 {
        match self {
            Self::BuiltIn(s) => s.price,
            Self::Custom(c) => c.model.price,
        }
    }

    pub fn elevation(&self) -> f64 {
        match self {
            Self::BuiltIn(s) => s.elevation,
            Self::Custom(c) => c.model.elevation,
        }
    }

    pub fn geometry(&self) -> Arc<ModelGeometry> {
        match self {
            Self::BuiltIn(s) => s.geometry(),
            Self::Custom(c) => c.geometry.clone(),
        }
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }

    /// Self-contained copy for export: built-in models are baked into a mesh too.
    pub fn to_custom_model(&self) -> CustomModel {
        match self {
            Self::Custom(c) => c.model.clone(),
            Self::BuiltIn(s) => {
                let geometry = s.geometry();
                let mut mesh = geometry.mesh.clone();
                mesh.edge_flags.clear();
                CustomModel {
                    id: s.id.to_string(),
                    name: s.name.to_string(),
                    category: s.category,
                    body_color: s.body_color,
                    price: s.price,
                    elevation: s.elevation,
                    mesh,
                }
            }
        }
    }
}

fn registry() -> &'static RwLock<Vec<Arc<CustomEntry>>> {
    static REGISTRY: OnceLock<RwLock<Vec<Arc<CustomEntry>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Adds (or replaces, by id) a user model in the library.
pub fn register_custom_model(model: CustomModel) -> ModelRef {
    let entry = Arc::new(CustomEntry { geometry: Arc::new(ModelGeometry::from_mesh(model.mesh.clone())), model });
    let mut models = registry().write().expect("model registry");
    match models.iter_mut().find(|e| e.model.id == entry.model.id) {
        Some(existing) => *existing = entry.clone(),
        None => models.push(entry.clone()),
    }
    ModelRef::Custom(entry)
}

/// Removes a user model; instances already placed fall back to a placeholder box.
pub fn unregister_custom_model(id: &str) -> bool {
    let mut models = registry().write().expect("model registry");
    let before = models.len();
    models.retain(|e| e.model.id != id);
    models.len() != before
}

/// Looks a model up by id: built-in catalogue first, then the user models.
pub fn find_model(id: &str) -> Option<ModelRef> {
    if let Some(spec) = ModelSpec::find(id) {
        return Some(ModelRef::BuiltIn(spec));
    }
    registry().read().expect("model registry").iter().find(|e| e.model.id == id).cloned().map(ModelRef::Custom)
}

/// Every model of the library: the built-in catalogue followed by the user models.
pub fn all_models() -> Vec<ModelRef> {
    let mut models: Vec<ModelRef> = CATALOG.iter().map(ModelRef::BuiltIn).collect();
    models.extend(registry().read().expect("model registry").iter().cloned().map(ModelRef::Custom));
    models
}

/// Model pack file (`*.skmodel`, JSON) holding one or more library models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLibraryFile {
    pub format: String,
    pub version: u32,
    pub models: Vec<CustomModel>,
}

impl ModelLibraryFile {
    pub const FORMAT: &'static str = "sketchcad-model-library";
    pub const EXTENSION: &'static str = "skmodel";

    pub fn load(path: &Path) -> std::io::Result<Vec<CustomModel>> {
        let text = std::fs::read_to_string(path)?;
        let file: Self = serde_json::from_str(&text).map_err(std::io::Error::other)?;
        if file.format != Self::FORMAT {
            return Err(std::io::Error::other(format!("不是 SketchCAD 模型包 (format = {})", file.format)));
        }
        Ok(file.models)
    }

    pub fn save(path: &Path, models: &[CustomModel]) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = Self { format: Self::FORMAT.to_string(), version: 1, models: models.to_vec() };
        std::fs::write(path, serde_json::to_string(&file).map_err(std::io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketchcad_core::Point3D;

    fn coloured_box(y_up: bool) -> TriangleMesh {
        let mut mesh = TriangleMesh::create_box(Vector3D::new(2.0, 1.0, 0.5), Point3D::new(5.0, 5.0, 5.0));
        let mut colours = vec![Some([200, 30, 30, 255]); mesh.triangle_count()];
        colours[0] = Some([10, 10, 10, 255]);
        mesh.face_colors = colours;
        if y_up {
            // Express the same box Y-up: (x, y, z) -> (x, z, -y)
            for p in &mut mesh.positions {
                *p = Point3D::new(p.x, p.z, -p.y);
            }
        }
        mesh
    }

    #[test]
    fn imported_mesh_is_scaled_grounded_and_keeps_a_paintable_body() {
        let mut mesh = coloured_box(true);
        convert_imported_mesh(&mut mesh, 1000.0, true);
        let model = CustomModel::from_mesh("Red Box", ModelCategory::Furniture, mesh);
        assert_eq!(model.body_color, [200, 30, 30, 255], "dominant colour becomes the body");
        assert_eq!(model.mesh.face_color(0), Some([10, 10, 10, 255]));
        assert_eq!(model.mesh.face_color(1), None);

        let model_ref = register_custom_model(model.clone());
        let (w, d, h) = model_ref.geometry().size;
        assert!((w - 2000.0).abs() < 1e-6 && (d - 1000.0).abs() < 1e-6 && (h - 500.0).abs() < 1e-6, "{w} {d} {h}");
        let bbox = model_ref.geometry().mesh.bbox().unwrap();
        assert!(bbox.min.z.abs() < 1e-9 && (bbox.min.x + bbox.max.x).abs() < 1e-9);
        assert!(find_model(&model.id).is_some_and(|m| m.is_custom()));
        assert!(all_models().iter().any(|m| m.id() == model.id));
        assert!(unregister_custom_model(&model.id));
        assert!(find_model(&model.id).is_none());
    }

    #[test]
    fn model_pack_round_trips_and_builtins_export_as_meshes() {
        let dir = std::env::temp_dir().join(format!("sketchcad-models-{}", std::process::id()));
        let path = dir.join("pack.skmodel");
        let custom = CustomModel::from_mesh("Box", ModelCategory::Outdoor, coloured_box(false));
        let fridge = find_model("fridge").unwrap().to_custom_model();
        assert!(fridge.mesh.triangle_count() > 0 && fridge.id == "fridge");
        ModelLibraryFile::save(&path, &[custom.clone(), fridge.clone()]).unwrap();
        let loaded = ModelLibraryFile::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], custom);
        assert_eq!(loaded[1].mesh.triangle_count(), fridge.mesh.triangle_count());
        std::fs::write(&path, r#"{"format":"other","version":1,"models":[]}"#).unwrap();
        assert!(ModelLibraryFile::load(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
