//! SketchUp (`.skp`) import and export through the bundled OpenSKP library.
//!
//! * Import: every placed component / group (per material) becomes an [`IfcComponent`] carrying
//!   its SketchUp colour. The element kind is guessed from the instance, definition and tag names.
//! * Export: every visible element becomes a SketchUp group on a tag named after its kind; coplanar
//!   triangles are merged back into real SketchUp faces (with holes), so walls and slabs stay clean.

use sketchcad_bim::{BimDocument, ElementKind, IfcComponent, Material};
use sketchcad_core::{Point3D, Vector3D};
use sketchcad_geom::TriangleMesh;
use sketchcad_skp::{SkpFace, SkpGroup, SkpMaterial, SkpMesh, SkpModel};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

const MM_PER_INCH: f64 = 25.4;
/// Triangles closer than this to coplanar are merged into one SketchUp face
const COPLANAR_COS: f64 = 0.999_99;
const COPLANAR_DIST_MM: f64 = 0.5;

#[derive(Debug, Error)]
pub enum SkpError {
    #[error("SketchUp 檔案讀寫失敗：{0}")]
    OpenSkp(String),
    #[error("SketchUp 檔案中沒有可匯入的幾何")]
    Empty,
}

pub struct SkpImporter;

impl SkpImporter {
    pub fn import_from_file<P: AsRef<Path>>(path: P) -> Result<BimDocument, SkpError> {
        let path = path.as_ref();
        let meshes = sketchcad_skp::read_skp(path).map_err(SkpError::OpenSkp)?;
        let name = path.file_stem().map_or("SketchUp 模型".to_string(), |s| s.to_string_lossy().into_owned());
        let mut doc = BimDocument::new(name);

        // A face with different front / back materials is baked twice (front + reversed back).
        // Keep one copy per placed instance, preferring explicitly painted materials.
        let mut by_path: HashMap<&str, Vec<&SkpMesh>> = HashMap::new();
        for mesh in &meshes {
            by_path.entry(mesh.path.as_str()).or_default().push(mesh);
        }
        let mut paths: Vec<&str> = by_path.keys().copied().collect();
        paths.sort_unstable();

        let mut count = 0;
        for path in paths {
            let mut group = by_path.remove(path).unwrap_or_default();
            group.sort_by_key(|m| m.uses_layer_color);
            let mut seen_triangles = HashSet::new();
            for mesh in group {
                let triangle_mesh = to_triangle_mesh(mesh, &mut seen_triangles);
                if triangle_mesh.indices.is_empty() {
                    continue;
                }
                let kind = guess_kind(mesh);
                let element = IfcComponent::new(display_name(mesh), kind, triangle_mesh, Vec::new(), material_for(kind))
                    .with_display_color(srgb_bytes(mesh.color));
                let id = doc.add_element(element);
                if mesh.layer_hidden {
                    doc.set_visible(id, false);
                }
                count += 1;
            }
        }
        if count == 0 {
            return Err(SkpError::Empty);
        }
        doc.sync();
        Ok(doc)
    }
}

/// Converts one baked OpenSKP mesh (metres, Y up) to millimetres with Z up, dropping triangles
/// already imported for the same instance (the reversed back side of a two-material face).
fn to_triangle_mesh(mesh: &SkpMesh, seen: &mut HashSet<[(i64, i64, i64); 3]>) -> TriangleMesh {
    let to_world = |p: [f32; 3]| Point3D::new(p[0] as f64 * 1000.0, -(p[2] as f64) * 1000.0, p[1] as f64 * 1000.0);
    let positions: Vec<Point3D> = mesh.positions.iter().map(|p| to_world(*p)).collect();
    let quantize = |p: Point3D| ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64, (p.z * 100.0).round() as i64);

    let mut out = TriangleMesh::new();
    for tri in mesh.indices.chunks_exact(3) {
        let idx = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        if idx.iter().any(|&i| i >= positions.len()) {
            continue;
        }
        let v = idx.map(|i| positions[i]);
        let mut key = v.map(quantize);
        key.sort_unstable();
        if !seen.insert(key) {
            continue;
        }
        let normal = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let base = out.positions.len() as u32;
        out.positions.extend_from_slice(&v);
        out.normals.extend_from_slice(&[normal; 3]);
        out.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    out.compute_crease_edges(25.0f64.to_radians());
    out
}

fn display_name(mesh: &SkpMesh) -> String {
    let name = [&mesh.name, &mesh.definition_name]
        .into_iter()
        .find(|n| !n.is_empty() && n.as_str() != "ROOT")
        .cloned();
    name.unwrap_or_else(|| {
        if mesh.layer.is_empty() || mesh.layer == "Layer0" {
            "SketchUp 幾何".to_string()
        } else {
            format!("SketchUp 幾何 ({})", mesh.layer)
        }
    })
}

/// BIM kind from instance / definition / tag names (English and Chinese keywords).
fn guess_kind(mesh: &SkpMesh) -> ElementKind {
    let text = format!("{} {} {}", mesh.name, mesh.definition_name, mesh.layer).to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
    if has(&["curtain", "幕牆"]) {
        ElementKind::CurtainWall
    } else if has(&["window", "窗"]) {
        ElementKind::Window
    } else if has(&["door", "門"]) {
        ElementKind::Door
    } else if has(&["stair", "step", "樓梯", "階梯"]) {
        ElementKind::Stairs
    } else if has(&["rail", "欄杆", "扶手"]) {
        ElementKind::Railing
    } else if has(&["roof", "屋頂", "屋面"]) {
        ElementKind::Roof
    } else if has(&["ceiling", "天花"]) {
        ElementKind::Ceiling
    } else if has(&["column", "pillar", "柱"]) {
        ElementKind::Column
    } else if has(&["beam", "樑", "梁"]) {
        ElementKind::Beam
    } else if has(&["slab", "floor", "樓板", "地板"]) {
        ElementKind::Slab
    } else if has(&["wall", "牆"]) {
        ElementKind::Wall
    } else {
        ElementKind::Furniture
    }
}

fn material_for(kind: ElementKind) -> Material {
    match kind {
        ElementKind::Door | ElementKind::Furniture | ElementKind::Stairs => Material::wood(),
        ElementKind::Window | ElementKind::CurtainWall => Material::glass(),
        ElementKind::Beam | ElementKind::Railing => Material::structural_steel(),
        ElementKind::Ceiling => Material::gypsum(),
        _ => Material::concrete(),
    }
}

/// OpenSKP reports glTF base colours (linear 0..1 of the original sRGB bytes): back to bytes.
fn srgb_bytes(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

// ------------------------------------------------------------------------------- export

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkpExportReport {
    pub groups: usize,
    pub faces: usize,
    /// Faces SketchUp's format could not represent and that were left out
    pub skipped_faces: usize,
}

pub struct SkpExporter;

impl SkpExporter {
    /// Writes all visible elements to a new `.skp` file (SketchUp 2013+ format).
    pub fn export_to_skp<P: AsRef<Path>>(doc: &BimDocument, path: P) -> Result<SkpExportReport, SkpError> {
        let mut model = SkpModel::default();
        let mut material_index: HashMap<[u8; 4], usize> = HashMap::new();
        let mut layer_index: HashMap<String, usize> = HashMap::new();

        let mut elements = doc.all_elements();
        elements.sort_by_key(|e| (format!("{:?}", e.kind()), e.name().to_string()));
        for elem in elements {
            if !doc.is_visible(elem.id()) {
                continue;
            }
            let faces = mesh_to_skp_faces(&elem.to_3d_mesh());
            if faces.is_empty() {
                continue;
            }
            let rgba = elem.display_color().unwrap_or_else(|| elem.kind().display_rgba());
            let material = *material_index.entry(rgba).or_insert_with(|| {
                model.materials.push(SkpMaterial {
                    name: format!("{:?}_{:02X}{:02X}{:02X}", elem.kind(), rgba[0], rgba[1], rgba[2]),
                    rgba,
                });
                model.materials.len() - 1
            });
            let layer_name = format!("{:?}", elem.kind());
            let layer = *layer_index.entry(layer_name.clone()).or_insert_with(|| {
                model.layers.push(layer_name);
                model.layers.len() - 1
            });
            model.groups.push(SkpGroup { name: elem.name().to_string(), layer: Some(layer), material: Some(material), faces });
        }
        if model.groups.is_empty() {
            return Err(SkpError::Empty);
        }

        let faces = model.groups.iter().map(|g| g.faces.len()).sum();
        let report = sketchcad_skp::write_skp(path.as_ref(), &model).map_err(SkpError::OpenSkp)?;
        Ok(SkpExportReport { groups: model.groups.len(), faces, skipped_faces: report.skipped_faces })
    }
}

/// Merges coplanar, edge-connected triangles into planar faces (outer loop + holes) in inches.
/// Regions whose outline cannot be traced unambiguously are written as soft-edged triangles.
pub fn mesh_to_skp_faces(mesh: &TriangleMesh) -> Vec<SkpFace> {
    type Key = (i64, i64, i64);
    let quantize = |p: Point3D| -> Key { ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64, (p.z * 100.0).round() as i64) };
    let to_inches = |p: Point3D| [p.x / MM_PER_INCH, p.y / MM_PER_INCH, p.z / MM_PER_INCH];

    // 1. Valid triangles with their plane
    struct Tri {
        v: [Point3D; 3],
        k: [Key; 3],
        n: Vector3D,
    }
    let mut tris = Vec::new();
    for t in mesh.indices.chunks_exact(3) {
        let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
        if idx.iter().any(|&i| i >= mesh.positions.len()) {
            continue;
        }
        let v = idx.map(|i| mesh.positions[i]);
        let k = v.map(quantize);
        if k[0] == k[1] || k[1] == k[2] || k[0] == k[2] {
            continue;
        }
        let cross = (v[1] - v[0]).cross(v[2] - v[0]);
        if cross.length() < 1e-6 {
            continue;
        }
        tris.push(Tri { v, k, n: cross.normalize() });
    }

    // 2. Union coplanar neighbours across shared edges
    let mut edge_owner: HashMap<(Key, Key), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for e in 0..3 {
            let (a, b) = (t.k[e], t.k[(e + 1) % 3]);
            edge_owner.entry(if a <= b { (a, b) } else { (b, a) }).or_default().push(i);
        }
    }
    let mut parent: Vec<usize> = (0..tris.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for owners in edge_owner.values() {
        if let [a, b] = owners[..] {
            let (ta, tb) = (&tris[a], &tris[b]);
            let same_plane = ta.n.dot(tb.n) > COPLANAR_COS
                && tb.v.iter().all(|p| (*p - ta.v[0]).dot(ta.n).abs() < COPLANAR_DIST_MM);
            if same_plane {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra] = rb;
            }
        }
    }
    let mut regions: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..tris.len() {
        let r = root(&mut parent, i);
        regions.entry(r).or_default().push(i);
    }
    let mut regions: Vec<Vec<usize>> = regions.into_values().collect();
    regions.sort_by_key(|r| r[0]);

    // 3. Trace each region's boundary loops from its directed, unshared edges
    let mut faces = Vec::new();
    for region in regions {
        let mut use_count: HashMap<(Key, Key), u32> = HashMap::new();
        for &i in &region {
            let t = &tris[i];
            for e in 0..3 {
                let (a, b) = (t.k[e], t.k[(e + 1) % 3]);
                *use_count.entry(if a <= b { (a, b) } else { (b, a) }).or_default() += 1;
            }
        }
        let mut next: HashMap<Key, Key> = HashMap::new();
        let mut point: HashMap<Key, Point3D> = HashMap::new();
        let mut ambiguous = false;
        for &i in &region {
            let t = &tris[i];
            for e in 0..3 {
                let (a, b) = (t.k[e], t.k[(e + 1) % 3]);
                if use_count[&if a <= b { (a, b) } else { (b, a) }] == 1 {
                    ambiguous |= next.insert(a, b).is_some();
                    point.insert(a, t.v[e]);
                }
            }
        }

        let normal = tris[region[0]].n;
        let mut loops: Vec<Vec<Point3D>> = Vec::new();
        if !ambiguous && !next.is_empty() {
            let mut remaining = next.clone();
            while let Some(&start) = remaining.keys().min() {
                let mut lp = Vec::new();
                let mut cur = start;
                loop {
                    let Some(n) = remaining.remove(&cur) else {
                        ambiguous = true;
                        break;
                    };
                    lp.push(point[&cur]);
                    cur = n;
                    if cur == start {
                        break;
                    }
                }
                if ambiguous {
                    break;
                }
                let lp = drop_collinear(lp, normal);
                if lp.len() >= 3 {
                    loops.push(lp);
                }
            }
        }

        if ambiguous || loops.is_empty() {
            for &i in &region {
                faces.push(SkpFace { outer: tris[i].v.map(to_inches).to_vec(), holes: Vec::new(), soft_edges: true });
            }
            continue;
        }
        // Outer loop = largest area; the remaining loops are holes
        loops.sort_by(|a, b| loop_area(b, normal).total_cmp(&loop_area(a, normal)));
        let outer = loops.remove(0);
        faces.push(SkpFace {
            outer: outer.into_iter().map(to_inches).collect(),
            holes: loops.into_iter().map(|l| l.into_iter().map(to_inches).collect()).collect(),
            soft_edges: false,
        });
    }
    faces
}

fn loop_area(points: &[Point3D], normal: Vector3D) -> f64 {
    let mut sum = Vector3D::ZERO;
    for i in 0..points.len() {
        sum += points[i].cross(points[(i + 1) % points.len()]);
    }
    (sum.dot(normal) * 0.5).abs()
}

/// Removes vertices lying on the straight line between their neighbours.
fn drop_collinear(points: Vec<Point3D>, normal: Vector3D) -> Vec<Point3D> {
    let n = points.len();
    if n < 4 {
        return points;
    }
    let keep: Vec<Point3D> = (0..n)
        .filter(|&i| {
            let (prev, cur, next) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let (a, b) = (cur - prev, next - cur);
            let turn = a.cross(b).dot(normal).abs();
            turn > 1e-6 * a.length().max(1e-9) * b.length().max(1e-9) || a.dot(b) < 0.0
        })
        .map(|i| points[i])
        .collect();
    if keep.len() >= 3 { keep } else { points }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketchcad_bim::{Door, Wall};
    use sketchcad_core::Point2D;
    use sketchcad_geom::OpeningVoid;

    #[test]
    fn box_merges_into_six_quads() {
        let mesh = TriangleMesh::create_box(Vector3D::new(1000.0, 2000.0, 3000.0), Point3D::ZERO);
        let faces = mesh_to_skp_faces(&mesh);
        assert_eq!(faces.len(), 6);
        assert!(faces.iter().all(|f| f.outer.len() == 4 && f.holes.is_empty() && !f.soft_edges));
    }

    #[test]
    fn wall_with_opening_roundtrips_through_skp() {
        let mut doc = BimDocument::new("roundtrip");
        let mut wall = Wall::new(Point2D::ZERO, Point2D::new(5000.0, 0.0), 200.0, 3000.0, Material::concrete());
        wall.add_opening(OpeningVoid { offset_along_wall: 1500.0, width: 1000.0, base_z: 0.0, height: 2100.0 });
        let wall_id = doc.add_element(wall);
        doc.add_element(Door::new(wall_id, 1500.0, 1000.0, 2100.0, Material::wood(), 0.0));

        let path = std::env::temp_dir().join(format!("sketchcad_roundtrip_{}.skp", std::process::id()));
        let report = SkpExporter::export_to_skp(&doc, &path).unwrap();
        assert_eq!(report.groups, 2);
        assert_eq!(report.skipped_faces, 0);

        let imported = SkpImporter::import_from_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let kinds: HashSet<ElementKind> = imported.all_elements().iter().map(|e| e.kind()).collect();
        assert!(kinds.contains(&ElementKind::Wall) && kinds.contains(&ElementKind::Door), "{kinds:?}");

        // Geometry comes back in millimetres, Z up, at the same place
        let wall_box = imported
            .all_elements()
            .into_iter()
            .find(|e| e.kind() == ElementKind::Wall)
            .and_then(|e| e.to_3d_mesh().bbox())
            .unwrap();
        assert!((wall_box.max.x - 5000.0).abs() < 1.0, "{wall_box:?}");
        assert!((wall_box.max.z - 3000.0).abs() < 1.0, "{wall_box:?}");
    }
}
