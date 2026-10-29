//! Wavefront OBJ (+ MTL colours) import and export for the 3D model library.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use sketchcad_core::{Point3D, Vector3D};
use sketchcad_geom::TriangleMesh;

#[derive(Debug, thiserror::Error)]
pub enum ObjError {
    #[error("無法讀取檔案: {0}")]
    Io(#[from] std::io::Error),
    #[error("OBJ 格式錯誤 (第 {line} 行): {message}")]
    Parse { line: usize, message: String },
    #[error("OBJ 檔案中沒有任何面 (f)")]
    Empty,
}

/// Diffuse colours (`Kd`, `d` / `Tr` opacity) of an MTL file by material name.
fn parse_mtl(text: &str) -> HashMap<String, [u8; 4]> {
    let mut materials = HashMap::new();
    let mut current: Option<(String, [f64; 4])> = None;
    let to_byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let flush = |current: &mut Option<(String, [f64; 4])>, materials: &mut HashMap<String, [u8; 4]>| {
        if let Some((name, c)) = current.take() {
            materials.insert(name, [to_byte(c[0]), to_byte(c[1]), to_byte(c[2]), to_byte(c[3])]);
        }
    };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let numbers = |parts: std::str::SplitWhitespace| parts.filter_map(|t| t.parse::<f64>().ok()).collect::<Vec<_>>();
        match parts.next() {
            Some("newmtl") => {
                flush(&mut current, &mut materials);
                current = Some((parts.collect::<Vec<_>>().join(" "), [0.8, 0.8, 0.8, 1.0]));
            }
            Some("Kd") => {
                if let (Some((_, c)), [r, g, b, ..]) = (current.as_mut(), numbers(parts).as_slice()) {
                    (c[0], c[1], c[2]) = (*r, *g, *b);
                }
            }
            Some("d") => {
                if let (Some((_, c)), [d, ..]) = (current.as_mut(), numbers(parts).as_slice()) {
                    c[3] = *d;
                }
            }
            Some("Tr") => {
                if let (Some((_, c)), [tr, ..]) = (current.as_mut(), numbers(parts).as_slice()) {
                    c[3] = 1.0 - *tr;
                }
            }
            _ => {}
        }
    }
    flush(&mut current, &mut materials);
    materials
}

/// Reads an OBJ file with the colours of its MTL library (looked up next to the file).
/// Coordinates are returned as stored in the file (no unit or axis conversion).
pub fn read_obj(path: &Path) -> Result<TriangleMesh, ObjError> {
    let text = std::fs::read_to_string(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    parse_obj(&text, |name| std::fs::read_to_string(dir.join(name)).ok())
}

/// Parses OBJ text; `load_mtl` returns the text of a referenced MTL library.
pub fn parse_obj(text: &str, load_mtl: impl Fn(&str) -> Option<String>) -> Result<TriangleMesh, ObjError> {
    let mut positions: Vec<Point3D> = Vec::new();
    let mut normals: Vec<Vector3D> = Vec::new();
    let mut materials: HashMap<String, [u8; 4]> = HashMap::new();
    let mut colour: Option<[u8; 4]> = None;
    let mut mesh = TriangleMesh::new();

    // OBJ indices are 1-based; negative ones count back from the latest element
    let resolve = |token: &str, len: usize, line: usize| -> Result<Option<usize>, ObjError> {
        if token.is_empty() {
            return Ok(None);
        }
        let i: i64 = token.parse().map_err(|_| ObjError::Parse { line, message: format!("無效的索引 '{token}'") })?;
        let idx = if i < 0 { len as i64 + i } else { i - 1 };
        if idx < 0 || idx as usize >= len {
            return Err(ObjError::Parse { line, message: format!("索引 {i} 超出範圍") });
        }
        Ok(Some(idx as usize))
    };

    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let content = raw.split('#').next().unwrap_or("");
        let mut parts = content.split_whitespace();
        let Some(keyword) = parts.next() else { continue };
        match keyword {
            "v" | "vn" => {
                let v: Vec<f64> = parts.take(3).map(|t| t.parse::<f64>()).collect::<Result<_, _>>().map_err(|_| ObjError::Parse {
                    line,
                    message: "座標不是數字".into(),
                })?;
                if v.len() < 3 {
                    return Err(ObjError::Parse { line, message: "座標需要三個分量".into() });
                }
                if keyword == "v" {
                    positions.push(Point3D::new(v[0], v[1], v[2]));
                } else {
                    normals.push(Vector3D::new(v[0], v[1], v[2]));
                }
            }
            "mtllib" => {
                for file in parts {
                    if let Some(mtl) = load_mtl(file) {
                        materials.extend(parse_mtl(&mtl));
                    }
                }
            }
            "usemtl" => {
                colour = materials.get(&parts.collect::<Vec<_>>().join(" ")).copied();
            }
            "f" => {
                let mut corners: Vec<(usize, Option<usize>)> = Vec::new();
                for token in parts {
                    let mut fields = token.split('/');
                    let v = resolve(fields.next().unwrap_or(""), positions.len(), line)?
                        .ok_or(ObjError::Parse { line, message: "面缺少頂點索引".into() })?;
                    let vn = resolve(fields.nth(1).unwrap_or(""), normals.len(), line)?;
                    corners.push((v, vn));
                }
                if corners.len() < 3 {
                    continue;
                }
                for k in 1..corners.len() - 1 {
                    let tri = [corners[0], corners[k], corners[k + 1]];
                    let p = tri.map(|(v, _)| positions[v]);
                    let face_normal = (p[1] - p[0]).cross(p[2] - p[0]);
                    if face_normal.length() < 1e-12 {
                        continue;
                    }
                    let flat = face_normal.normalize();
                    let base = mesh.positions.len() as u32;
                    for (corner, point) in tri.iter().zip(p) {
                        mesh.positions.push(point);
                        let n = corner.1.map(|i| normals[i]).filter(|n| n.length() > 1e-12).map_or(flat, |n| n.normalize());
                        mesh.normals.push(n);
                    }
                    mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
                    mesh.face_colors.push(colour);
                }
            }
            _ => {}
        }
    }
    if mesh.triangle_count() == 0 {
        return Err(ObjError::Empty);
    }
    if mesh.face_colors.iter().all(Option::is_none) {
        mesh.face_colors.clear();
    }
    Ok(mesh)
}

/// Writes a model mesh (millimetres, Z up) as OBJ + MTL in metres, Y up — the convention
/// of most modelling tools. Triangles without a face colour use `body_color`.
pub fn write_obj(path: &Path, mesh: &TriangleMesh, body_color: [u8; 4], name: &str) -> std::io::Result<()> {
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path.file_name().and_then(|n| n.to_str()).unwrap_or("model.mtl").to_string();
    let to_y_up = |v: Vector3D| Vector3D::new(v.x, v.z, -v.y);

    // Group triangles by colour so each group gets one material
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut groups: HashMap<[u8; 4], Vec<usize>> = HashMap::new();
    for t in 0..mesh.triangle_count() {
        let c = mesh.face_color(t).unwrap_or(body_color);
        if !palette.contains(&c) {
            palette.push(c);
        }
        groups.entry(c).or_default().push(t);
    }

    let mut mtl = String::from("# SketchCAD material library\n");
    for (i, c) in palette.iter().enumerate() {
        let f = |v: u8| v as f64 / 255.0;
        let _ = writeln!(mtl, "\nnewmtl mat{i}\nKd {:.4} {:.4} {:.4}\nd {:.4}", f(c[0]), f(c[1]), f(c[2]), f(c[3]));
    }

    let mut obj = format!("# SketchCAD model library export (metres, Y up)\nmtllib {mtl_name}\no {}\n", name.replace(char::is_whitespace, "_"));
    let mut vertex = 1usize;
    for (i, c) in palette.iter().enumerate() {
        let _ = writeln!(obj, "usemtl mat{i}");
        for &t in &groups[c] {
            for k in 0..3 {
                let idx = mesh.indices[t * 3 + k] as usize;
                let p = to_y_up(mesh.positions[idx]) / 1000.0;
                let n = mesh.normals.get(idx).map_or(Vector3D::Y, |n| to_y_up(*n));
                let _ = writeln!(obj, "v {:.6} {:.6} {:.6}\nvn {:.5} {:.5} {:.5}", p.x, p.y, p.z, n.x, n.y, n.z);
            }
            let _ = writeln!(obj, "f {0}//{0} {1}//{1} {2}//{2}", vertex, vertex + 1, vertex + 2);
            vertex += 3;
        }
    }
    std::fs::write(&mtl_path, mtl)?;
    std::fs::write(path, obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quads_negative_indices_and_mtl_colours() {
        let obj = "mtllib box.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvn 0 0 1\nusemtl red\nf 1//1 2//1 3//1 4//1\nusemtl none\nf -4 -2 -1\n";
        let mesh = parse_obj(obj, |name| (name == "box.mtl").then(|| "newmtl red\nKd 1 0 0\nd 0.5\n".to_string())).unwrap();
        assert_eq!(mesh.triangle_count(), 3);
        assert_eq!(mesh.face_color(0), Some([255, 0, 0, 128]));
        assert_eq!(mesh.face_color(2), None, "unknown material falls back to the body");
        assert!(mesh.normals.iter().all(|n| (n.z - 1.0).abs() < 1e-9));
        assert!(matches!(parse_obj("v 0 0 0\nf 1 2 3\n", |_| None), Err(ObjError::Parse { line: 2, .. })));
        assert!(matches!(parse_obj("v 0 0 0\n", |_| None), Err(ObjError::Empty)));
    }

    #[test]
    fn export_then_import_restores_the_mesh_in_metres_y_up() {
        let mut mesh = TriangleMesh::create_box(Vector3D::new(2000.0, 1000.0, 500.0), Point3D::new(0.0, 0.0, 250.0));
        mesh.face_colors = vec![None; mesh.triangle_count()];
        mesh.face_colors[0] = Some([10, 20, 30, 255]);
        let dir = std::env::temp_dir().join(format!("sketchcad-obj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("box.obj");
        write_obj(&path, &mesh, [200, 100, 50, 255], "Test box").unwrap();
        let back = read_obj(&path).unwrap();
        assert_eq!(back.triangle_count(), 12);
        let bbox = back.bbox().unwrap();
        assert!((bbox.size().x - 2.0).abs() < 1e-6 && (bbox.size().y - 0.5).abs() < 1e-6 && (bbox.size().z - 1.0).abs() < 1e-6);
        let colours: std::collections::HashSet<_> = back.face_colors.iter().flatten().copied().collect();
        assert!(colours.contains(&[10, 20, 30, 255]) && colours.contains(&[200, 100, 50, 255]));
        let _ = std::fs::remove_dir_all(dir);
    }
}
