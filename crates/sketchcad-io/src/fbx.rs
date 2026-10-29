//! Autodesk FBX (binary and ASCII, 7.x) mesh import for the 3D model library: geometry,
//! model transforms and hierarchy, per-polygon material colours, file units and up axis.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use glam::{DMat3, DMat4, DQuat, DVec3, EulerRot};
use sketchcad_core::{Point3D, Vector3D};
use sketchcad_geom::TriangleMesh;

#[derive(Debug, thiserror::Error)]
pub enum FbxError {
    #[error("無法讀取檔案: {0}")]
    Io(#[from] std::io::Error),
    #[error("FBX 格式錯誤: {0}")]
    Format(String),
    #[error("FBX 檔案中沒有任何網格幾何 (Mesh)")]
    Empty,
}

fn format_err(msg: impl Into<String>) -> FbxError {
    FbxError::Format(msg.into())
}

/// Property value of an FBX node.
#[derive(Debug, Clone, PartialEq)]
enum Prop {
    Int(i64),
    Float(f64),
    Str(String),
    IntArray(Vec<i64>),
    FloatArray(Vec<f64>),
    Raw,
}

impl Prop {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Prop::Int(v) => Some(*v as f64),
            Prop::Float(v) => Some(*v),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Prop::Int(v) => Some(*v),
            Prop::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }

    fn floats(&self) -> Vec<f64> {
        match self {
            Prop::FloatArray(v) => v.clone(),
            Prop::IntArray(v) => v.iter().map(|&i| i as f64).collect(),
            other => other.as_f64().into_iter().collect(),
        }
    }

    fn ints(&self) -> Vec<i64> {
        match self {
            Prop::IntArray(v) => v.clone(),
            Prop::FloatArray(v) => v.iter().map(|&f| f as i64).collect(),
            other => other.as_i64().into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Values of a `Properties70` entry `P: "name", type, label, flags, values...`.
    fn property70(&self, name: &str) -> Option<&[Prop]> {
        self.child("Properties70")?
            .children_named("P")
            .find(|p| p.props.first().and_then(Prop::as_str) == Some(name))
            .map(|p| &p.props[4.min(p.props.len())..])
    }

    fn vec3_property(&self, name: &str) -> Option<DVec3> {
        let v = self.property70(name)?;
        Some(DVec3::new(v.first()?.as_f64()?, v.get(1)?.as_f64()?, v.get(2)?.as_f64()?))
    }
}

// ───────────────────────────── Binary FBX ─────────────────────────────

const BINARY_MAGIC: &[u8] = b"Kaydara FBX Binary  \0";

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FbxError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| format_err("檔案在資料中途結束"))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, FbxError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, FbxError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")))
    }

    fn u64(&mut self) -> Result<u64, FbxError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes")))
    }

    fn array<T>(&mut self, size: usize, convert: impl Fn(&[u8]) -> T) -> Result<Vec<T>, FbxError> {
        let (count, encoding, byte_len) = (self.u32()? as usize, self.u32()?, self.u32()? as usize);
        let raw = self.take(byte_len)?;
        let bytes = if encoding == 1 {
            let mut out = Vec::with_capacity(count * size);
            flate2::read::ZlibDecoder::new(raw).read_to_end(&mut out).map_err(|e| format_err(format!("陣列解壓縮失敗: {e}")))?;
            out
        } else {
            raw.to_vec()
        };
        if bytes.len() < count * size {
            return Err(format_err("陣列長度不符"));
        }
        Ok(bytes.chunks_exact(size).take(count).map(convert).collect())
    }

    fn property(&mut self) -> Result<Prop, FbxError> {
        let code = self.u8()?;
        Ok(match code {
            b'Y' => Prop::Int(i16::from_le_bytes(self.take(2)?.try_into().expect("2")) as i64),
            b'C' => Prop::Int(self.u8()? as i64),
            b'I' => Prop::Int(i32::from_le_bytes(self.take(4)?.try_into().expect("4")) as i64),
            b'L' => Prop::Int(i64::from_le_bytes(self.take(8)?.try_into().expect("8"))),
            b'F' => Prop::Float(f32::from_le_bytes(self.take(4)?.try_into().expect("4")) as f64),
            b'D' => Prop::Float(f64::from_le_bytes(self.take(8)?.try_into().expect("8"))),
            b'f' => Prop::FloatArray(self.array(4, |b| f32::from_le_bytes(b.try_into().expect("4")) as f64)?),
            b'd' => Prop::FloatArray(self.array(8, |b| f64::from_le_bytes(b.try_into().expect("8")))?),
            b'i' => Prop::IntArray(self.array(4, |b| i32::from_le_bytes(b.try_into().expect("4")) as i64)?),
            b'l' => Prop::IntArray(self.array(8, |b| i64::from_le_bytes(b.try_into().expect("8")))?),
            b'b' => Prop::IntArray(self.array(1, |b| b[0] as i64)?),
            b'S' => {
                let len = self.u32()? as usize;
                Prop::Str(String::from_utf8_lossy(self.take(len)?).into_owned())
            }
            b'R' => {
                let len = self.u32()? as usize;
                self.take(len)?;
                Prop::Raw
            }
            other => return Err(format_err(format!("未知的屬性型別 '{}'", other as char))),
        })
    }

    /// Reads one node record; `None` for the null record that ends a child list.
    fn node(&mut self, wide: bool) -> Result<Option<Node>, FbxError> {
        let (end, num_props, _props_len) =
            if wide { (self.u64()? as usize, self.u64()? as usize, self.u64()?) } else { (self.u32()? as usize, self.u32()? as usize, self.u32()? as u64) };
        let name_len = self.u8()? as usize;
        if end == 0 {
            return Ok(None);
        }
        if end > self.data.len() {
            return Err(format_err("節點範圍超出檔案"));
        }
        let name = String::from_utf8_lossy(self.take(name_len)?).into_owned();
        let props = (0..num_props).map(|_| self.property()).collect::<Result<Vec<_>, _>>()?;
        let mut children = Vec::new();
        while self.pos < end {
            match self.node(wide)? {
                Some(child) => children.push(child),
                None => break,
            }
        }
        self.pos = end;
        Ok(Some(Node { name, props, children }))
    }
}

fn parse_binary(data: &[u8]) -> Result<Node, FbxError> {
    let mut reader = Reader { data, pos: BINARY_MAGIC.len() + 2 };
    let version = reader.u32()?;
    let wide = version >= 7500;
    let mut root = Node::default();
    // The top-level list ends with a null record followed by a footer
    while reader.pos + if wide { 25 } else { 13 } <= data.len() {
        match reader.node(wide)? {
            Some(node) => root.children.push(node),
            None => break,
        }
    }
    Ok(root)
}

// ───────────────────────────── ASCII FBX ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Key(String),
    Str(String),
    Num(String),
    Star(String),
    Open,
    Close,
    Comma,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' => {
                tokens.push(Token::Open);
                i += 1;
            }
            '}' => {
                tokens.push(Token::Close);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '"' => {
                let start = i + 1;
                i = start;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                tokens.push(Token::Str(chars[start..i.min(chars.len())].iter().collect()));
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], ',' | '{' | '}' | '"' | ':' | ';') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if i < chars.len() && chars[i] == ':' {
                    tokens.push(Token::Key(word));
                    i += 1;
                } else if let Some(n) = word.strip_prefix('*') {
                    tokens.push(Token::Star(n.to_string()));
                } else if !word.is_empty() {
                    tokens.push(Token::Num(word));
                } else {
                    i += 1;
                }
            }
        }
    }
    tokens
}

fn number(word: &str) -> Prop {
    if word.contains(['.', 'e', 'E']) || word.eq_ignore_ascii_case("nan") || word.contains("inf") {
        Prop::Float(word.parse().unwrap_or(0.0))
    } else {
        word.parse::<i64>().map(Prop::Int).unwrap_or_else(|_| Prop::Float(word.parse().unwrap_or(0.0)))
    }
}

fn parse_ascii_nodes(tokens: &[Token], pos: &mut usize) -> Result<Vec<Node>, FbxError> {
    let mut nodes = Vec::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Close => {
                *pos += 1;
                return Ok(nodes);
            }
            Token::Key(name) => {
                *pos += 1;
                let mut node = Node { name: name.clone(), ..Default::default() };
                // Values: comma separated scalars / strings, or an array `*N { a: ... }`
                loop {
                    match tokens.get(*pos) {
                        Some(Token::Str(s)) => node.props.push(Prop::Str(s.clone())),
                        Some(Token::Num(n)) => node.props.push(number(n)),
                        Some(Token::Star(_)) => {
                            *pos += 1;
                            if tokens.get(*pos) != Some(&Token::Open) {
                                return Err(format_err("陣列缺少 {"));
                            }
                            *pos += 1;
                            if matches!(tokens.get(*pos), Some(Token::Key(_))) {
                                *pos += 1;
                            }
                            let mut values = Vec::new();
                            while let Some(t) = tokens.get(*pos) {
                                match t {
                                    Token::Num(n) => values.push(number(n)),
                                    Token::Comma => {}
                                    Token::Close => break,
                                    _ => return Err(format_err("陣列內容錯誤")),
                                }
                                *pos += 1;
                            }
                            let is_float = values.iter().any(|v| matches!(v, Prop::Float(_)));
                            node.props.push(if is_float {
                                Prop::FloatArray(values.iter().filter_map(Prop::as_f64).collect())
                            } else {
                                Prop::IntArray(values.iter().filter_map(Prop::as_i64).collect())
                            });
                        }
                        _ => break,
                    }
                    *pos += 1;
                    if tokens.get(*pos) == Some(&Token::Comma) {
                        *pos += 1;
                    } else {
                        break;
                    }
                }
                if tokens.get(*pos) == Some(&Token::Open) {
                    *pos += 1;
                    node.children = parse_ascii_nodes(tokens, pos)?;
                }
                nodes.push(node);
            }
            _ => *pos += 1,
        }
    }
    Ok(nodes)
}

fn parse_ascii(text: &str) -> Result<Node, FbxError> {
    let tokens = tokenize(text);
    let mut pos = 0;
    Ok(Node { children: parse_ascii_nodes(&tokens, &mut pos)?, ..Default::default() })
}

// ───────────────────────────── Scene ─────────────────────────────

/// Mesh read from an FBX file, in millimetres, plus the file's up axis.
#[derive(Debug, Clone)]
pub struct FbxImport {
    pub mesh: TriangleMesh,
    /// True when the file is Y-up (Maya / Blender / Unity exports), false when Z-up
    pub y_up: bool,
}

/// Reads all meshes of an FBX file (binary or ASCII) into one coloured triangle mesh.
pub fn read_fbx(path: &Path) -> Result<FbxImport, FbxError> {
    parse_fbx(&std::fs::read(path)?)
}

pub fn parse_fbx(data: &[u8]) -> Result<FbxImport, FbxError> {
    let root = if data.starts_with(BINARY_MAGIC) {
        parse_binary(data)?
    } else {
        parse_ascii(&String::from_utf8_lossy(data))?
    };
    build_scene(&root)
}

/// Local transform of a Model node (pivots and offsets are rarely used and ignored).
fn local_transform(model: &Node) -> DMat4 {
    let euler = |name: &str| {
        model.vec3_property(name).map_or(DQuat::IDENTITY, |r| {
            DQuat::from_euler(EulerRot::ZYX, r.z.to_radians(), r.y.to_radians(), r.x.to_radians())
        })
    };
    let t = model.vec3_property("Lcl Translation").unwrap_or(DVec3::ZERO);
    let s = model.vec3_property("Lcl Scaling").unwrap_or(DVec3::ONE);
    DMat4::from_translation(t)
        * DMat4::from_quat(euler("PreRotation"))
        * DMat4::from_quat(euler("Lcl Rotation"))
        * DMat4::from_quat(euler("PostRotation")).inverse()
        * DMat4::from_scale(s)
}

/// Geometric offset applied to a model's own geometry only.
fn geometric_transform(model: &Node) -> DMat4 {
    let t = model.vec3_property("GeometricTranslation").unwrap_or(DVec3::ZERO);
    let r = model.vec3_property("GeometricRotation").unwrap_or(DVec3::ZERO);
    let s = model.vec3_property("GeometricScaling").unwrap_or(DVec3::ONE);
    DMat4::from_scale_rotation_translation(s, DQuat::from_euler(EulerRot::ZYX, r.z.to_radians(), r.y.to_radians(), r.x.to_radians()), t)
}

fn material_colour(material: &Node) -> [u8; 4] {
    let colour = material
        .vec3_property("DiffuseColor")
        .or_else(|| material.vec3_property("Diffuse"))
        .or_else(|| material.vec3_property("base_color"))
        .unwrap_or(DVec3::splat(0.8));
    let opacity = material
        .property70("Opacity")
        .and_then(|v| v.first()?.as_f64())
        .or_else(|| material.property70("TransparencyFactor").and_then(|v| v.first()?.as_f64()).map(|t| 1.0 - t))
        .unwrap_or(1.0);
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(colour.x), byte(colour.y), byte(colour.z), byte(if opacity <= 0.0 { 1.0 } else { opacity })]
}

/// Per-polygon-vertex normals of a geometry, if it has a usable normal layer.
fn polygon_vertex_normals(geometry: &Node, indices: &[i64]) -> Option<Vec<DVec3>> {
    let layer = geometry.child("LayerElementNormal")?;
    let normals = layer.child("Normals")?.props.first()?.floats();
    let mapping = layer.child("MappingInformationType")?.props.first()?.as_str()?.to_string();
    let reference = layer.child("ReferenceInformationType").and_then(|n| n.props.first()?.as_str()).unwrap_or("Direct").to_string();
    let normal_index = layer.child("NormalsIndex").and_then(|n| n.props.first()).map(Prop::ints);
    let fetch = |i: usize| -> Option<DVec3> {
        let i = if reference == "IndexToDirect" { *normal_index.as_ref()?.get(i)? as usize } else { i };
        Some(DVec3::new(*normals.get(i * 3)?, *normals.get(i * 3 + 1)?, *normals.get(i * 3 + 2)?))
    };
    indices
        .iter()
        .enumerate()
        .map(|(k, &raw)| {
            let vertex = if raw < 0 { !raw } else { raw } as usize;
            match mapping.as_str() {
                "ByPolygonVertex" => fetch(k),
                "ByVertice" | "ByVertex" => fetch(vertex),
                _ => None,
            }
        })
        .collect()
}

fn build_scene(root: &Node) -> Result<FbxImport, FbxError> {
    let objects = root.child("Objects").ok_or_else(|| format_err("缺少 Objects 區段"))?;
    let id_of = |n: &Node| n.props.first().and_then(Prop::as_i64);
    let geometries: HashMap<i64, &Node> = objects
        .children_named("Geometry")
        .filter(|g| g.props.get(2).and_then(Prop::as_str).is_none_or(|t| t == "Mesh"))
        .filter_map(|g| Some((id_of(g)?, g)))
        .collect();
    let models: HashMap<i64, &Node> = objects.children_named("Model").filter_map(|m| Some((id_of(m)?, m))).collect();
    let materials: HashMap<i64, &Node> = objects.children_named("Material").filter_map(|m| Some((id_of(m)?, m))).collect();

    // Object-object connections: child -> parent, in file order
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    let mut model_geometry: Vec<(i64, i64)> = Vec::new();
    let mut model_materials: HashMap<i64, Vec<i64>> = HashMap::new();
    if let Some(connections) = root.child("Connections") {
        for c in connections.children_named("C") {
            let (Some(kind), Some(child), Some(parent)) =
                (c.props.first().and_then(Prop::as_str), c.props.get(1).and_then(Prop::as_i64), c.props.get(2).and_then(Prop::as_i64))
            else {
                continue;
            };
            if kind != "OO" {
                continue;
            }
            if geometries.contains_key(&child) && models.contains_key(&parent) {
                model_geometry.push((parent, child));
            } else if materials.contains_key(&child) && models.contains_key(&parent) {
                model_materials.entry(parent).or_default().push(child);
            } else if models.contains_key(&child) {
                parent_of.insert(child, parent);
            }
        }
    }

    let world = |model_id: i64| {
        let mut m = DMat4::IDENTITY;
        let mut current = Some(model_id);
        let mut guard = 0;
        while let Some(id) = current.filter(|_| guard < 256) {
            let Some(node) = models.get(&id) else { break };
            m = local_transform(node) * m;
            current = parent_of.get(&id).copied();
            guard += 1;
        }
        m
    };

    let mut mesh = TriangleMesh::new();
    for (model_id, geometry_id) in model_geometry {
        let geometry = geometries[&geometry_id];
        let transform = world(model_id) * geometric_transform(models[&model_id]);
        let normal_matrix = DMat3::from_mat4(transform).inverse().transpose();
        let Some(vertices) = geometry.child("Vertices").and_then(|n| n.props.first()).map(Prop::floats) else { continue };
        let Some(indices) = geometry.child("PolygonVertexIndex").and_then(|n| n.props.first()).map(Prop::ints) else { continue };
        let normals = polygon_vertex_normals(geometry, &indices);
        let palette: Vec<[u8; 4]> =
            model_materials.get(&model_id).map(|ids| ids.iter().map(|id| material_colour(materials[id])).collect()).unwrap_or_default();
        let material_layer = geometry.child("LayerElementMaterial");
        let polygon_materials = material_layer.and_then(|l| l.child("Materials")?.props.first().map(Prop::ints)).unwrap_or_default();
        let all_same = material_layer
            .and_then(|l| l.child("MappingInformationType")?.props.first()?.as_str().map(|s| s == "AllSame"))
            .unwrap_or(true);

        let point = |i: usize| -> Option<Point3D> {
            Some(transform.transform_point3(DVec3::new(*vertices.get(i * 3)?, *vertices.get(i * 3 + 1)?, *vertices.get(i * 3 + 2)?)))
        };
        let mut polygon: Vec<usize> = Vec::new(); // positions in `indices`
        let mut polygon_index = 0usize;
        for (k, &raw) in indices.iter().enumerate() {
            polygon.push(k);
            if raw >= 0 {
                continue;
            }
            let material = if all_same { polygon_materials.first() } else { polygon_materials.get(polygon_index) };
            let colour = material.and_then(|&m| palette.get(m as usize)).copied().or_else(|| palette.first().copied());
            let corners: Option<Vec<(Point3D, usize)>> = polygon
                .iter()
                .map(|&pk| {
                    let raw = indices[pk];
                    point(if raw < 0 { !raw } else { raw } as usize).map(|p| (p, pk))
                })
                .collect();
            if let Some(corners) = corners {
                for j in 1..corners.len().saturating_sub(1) {
                    let tri = [corners[0], corners[j], corners[j + 1]];
                    let face = (tri[1].0 - tri[0].0).cross(tri[2].0 - tri[0].0);
                    if face.length() < 1e-12 {
                        continue;
                    }
                    let base = mesh.positions.len() as u32;
                    for (p, pk) in tri {
                        mesh.positions.push(p);
                        let n = normals.as_ref().and_then(|ns| ns.get(pk)).map(|n| normal_matrix * *n).filter(|n| n.length() > 1e-12);
                        mesh.normals.push(n.map_or(face.normalize(), Vector3D::normalize));
                    }
                    mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
                    mesh.face_colors.push(colour);
                }
            }
            polygon.clear();
            polygon_index += 1;
        }
    }
    if mesh.triangle_count() == 0 {
        return Err(FbxError::Empty);
    }
    if mesh.face_colors.iter().all(Option::is_none) {
        mesh.face_colors.clear();
    }

    // File units (centimetres per unit; FBX default is 1 cm) and up axis
    let settings = root.child("GlobalSettings");
    let unit_cm = settings.and_then(|s| s.property70("UnitScaleFactor")?.first()?.as_f64()).filter(|v| *v > 0.0).unwrap_or(1.0);
    let up_axis = settings.and_then(|s| s.property70("UpAxis")?.first()?.as_i64()).unwrap_or(1);
    for p in &mut mesh.positions {
        *p *= unit_cm * 10.0;
    }
    Ok(FbxImport { mesh, y_up: up_axis != 2 })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASCII_CUBE: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7400
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",100
	}
}
Objects:  {
	Geometry: 1001, "Geometry::Box", "Mesh" {
		Vertices: *24 {
			a: 0,0,0,1,0,0,1,1,0,0,1,0,0,0,1,1,0,1,1,1,1,0,1,1
		}
		PolygonVertexIndex: *24 {
			a: 0,3,2,-2,4,5,6,-8,0,1,5,-5,1,2,6,-6,2,3,7,-7,3,0,4,-8
		}
		LayerElementMaterial: 0 {
			MappingInformationType: "ByPolygon"
			Materials: *6 {
				a: 0,0,0,0,1,1
			}
		}
	}
	Model: 2001, "Model::Box", "Mesh" {
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",10,0,0
			P: "Lcl Rotation", "Lcl Rotation", "", "A",0,0,90
			P: "Lcl Scaling", "Lcl Scaling", "", "A",2,1,1
		}
	}
	Material: 3001, "Material::Red", "" {
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}
	}
	Material: 3002, "Material::Glass", "" {
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",0,0,1
			P: "Opacity", "double", "Number", "",0.5
		}
	}
}
Connections:  {
	C: "OO",2001,0
	C: "OO",1001,2001
	C: "OO",3001,2001
	C: "OO",3002,2001
}
"#;

    #[test]
    fn ascii_scene_applies_units_transforms_and_materials() {
        let import = parse_fbx(ASCII_CUBE.as_bytes()).unwrap();
        assert!(import.y_up);
        let mesh = &import.mesh;
        assert_eq!(mesh.triangle_count(), 12);
        // Unit box scaled 2× in X, turned 90° about Z, moved 10 units; 1 unit = 100 cm = 1000 mm
        let bbox = mesh.bbox().unwrap();
        assert!((bbox.size().x - 1000.0).abs() < 1e-6 && (bbox.size().y - 2000.0).abs() < 1e-6, "{:?}", bbox.size());
        assert!((bbox.min.x - 9000.0).abs() < 1e-6 && (bbox.max.x - 10000.0).abs() < 1e-6);
        assert_eq!(mesh.face_color(0), Some([255, 0, 0, 255]));
        assert_eq!(mesh.face_color(11), Some([0, 0, 255, 128]));
        assert_eq!(mesh.normals.len(), mesh.positions.len());
    }

    /// Encodes a node tree as binary FBX 7.4 (u32 offsets), with zlib-compressed arrays.
    fn encode_binary(nodes: &[Node]) -> Vec<u8> {
        fn encode_node(out: &mut Vec<u8>, node: &Node) {
            let start = out.len();
            out.extend_from_slice(&[0; 12]);
            out.push(node.name.len() as u8);
            out.extend_from_slice(node.name.as_bytes());
            let props_start = out.len();
            for p in &node.props {
                match p {
                    Prop::Int(v) => {
                        out.push(b'L');
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                    Prop::Float(v) => {
                        out.push(b'D');
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                    Prop::Str(s) => {
                        out.push(b'S');
                        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                        out.extend_from_slice(s.as_bytes());
                    }
                    Prop::FloatArray(v) => {
                        out.push(b'd');
                        let raw: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
                        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                        std::io::Write::write_all(&mut z, &raw).unwrap();
                        let packed = z.finish().unwrap();
                        for n in [v.len() as u32, 1, packed.len() as u32] {
                            out.extend_from_slice(&n.to_le_bytes());
                        }
                        out.extend_from_slice(&packed);
                    }
                    Prop::IntArray(v) => {
                        out.push(b'i');
                        for n in [v.len() as u32, 0, (v.len() * 4) as u32] {
                            out.extend_from_slice(&n.to_le_bytes());
                        }
                        for i in v {
                            out.extend_from_slice(&(*i as i32).to_le_bytes());
                        }
                    }
                    Prop::Raw => {}
                }
            }
            let props_len = (out.len() - props_start) as u32;
            for child in &node.children {
                encode_node(out, child);
            }
            if !node.children.is_empty() {
                out.extend_from_slice(&[0; 13]);
            }
            let end = out.len() as u32;
            out[start..start + 4].copy_from_slice(&end.to_le_bytes());
            out[start + 4..start + 8].copy_from_slice(&(node.props.len() as u32).to_le_bytes());
            out[start + 8..start + 12].copy_from_slice(&props_len.to_le_bytes());
        }
        let mut out = BINARY_MAGIC.to_vec();
        out.extend_from_slice(&[0x1A, 0x00]);
        out.extend_from_slice(&7400u32.to_le_bytes());
        for n in nodes {
            encode_node(&mut out, n);
        }
        out.extend_from_slice(&[0; 13]);
        out.extend_from_slice(&[0; 160]);
        out
    }

    #[test]
    fn binary_file_matches_the_ascii_one() {
        let ascii = parse_ascii(ASCII_CUBE).unwrap();
        let binary = encode_binary(&ascii.children);
        let from_binary = parse_fbx(&binary).unwrap();
        let from_ascii = parse_fbx(ASCII_CUBE.as_bytes()).unwrap();
        assert_eq!(from_binary.mesh.positions, from_ascii.mesh.positions);
        assert_eq!(from_binary.mesh.face_colors, from_ascii.mesh.face_colors);
        assert!(matches!(parse_fbx(b"; FBX\nObjects: {\n}\n"), Err(FbxError::Empty)));
        assert!(parse_fbx(&binary[..120]).is_err(), "truncated files are rejected");
    }
}
