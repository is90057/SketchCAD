use glam::{DMat4, DVec3, DVec4};
use sketchcad_bim::{
    BimDocument, BimElement, ElementKind, IfcComponent, Material, Storey,
};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector3D};
use sketchcad_geom::{extrude_polygon, is_polygon_edge, triangulate_polygon_3d, Polyline2D, TriangleMesh};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IfcParseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Malformed STEP syntax: {0}")]
    MalformedSyntax(String),
    #[error("Entity #{0} not found")]
    EntityNotFound(u64),
}

/// A value in the STEP ISO-10303-21 syntax
#[derive(Debug, Clone, PartialEq)]
pub enum StepValue {
    String(String),
    Number(f64),
    EntityRef(u64),
    Enumeration(String),
    List(Vec<StepValue>),
    Omitted, // $
    Derived, // *
}

impl StepValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            StepValue::String(s) | StepValue::Enumeration(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            StepValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_ref_id(&self) -> Option<u64> {
        match self {
            StepValue::EntityRef(id) => Some(*id),
            _ => None,
        }
    }
}

/// A parsed STEP Entity (e.g., `#12 = IFCWALL('guid', $, 'Name', ...);`)
#[derive(Debug, Clone)]
pub struct StepEntity {
    pub id: u64,
    pub type_name: String,
    pub parameters: Vec<StepValue>,
}

/// Decodes STEP ISO-10303-21 Unicode escape sequences, such as `\X2\57FA672C\X0\`
pub fn decode_ifc_unicode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if chars.peek() == Some(&'X') || chars.peek() == Some(&'x') {
                chars.next(); // consume 'X'
                let mode = chars.next().unwrap_or(' ');
                if mode == '2' || mode == '4' {
                    if chars.peek() == Some(&'\\') {
                        chars.next(); // consume '\'
                    }
                    let chunk_len = if mode == '2' { 4 } else { 8 };
                    let mut hex_buf = String::new();
                    while let Some(&hc) = chars.peek() {
                        if hc == '\\' {
                            chars.next();
                            if chars.peek() == Some(&'X') || chars.peek() == Some(&'x') {
                                chars.next();
                                if chars.peek() == Some(&'0') {
                                    chars.next();
                                    if chars.peek() == Some(&'\\') {
                                        chars.next();
                                    }
                                }
                            }
                            break;
                        } else {
                            hex_buf.push(hc);
                            chars.next();
                        }
                    }
                    // Parse hex_buf in chunks of chunk_len
                    for chunk in hex_buf.as_bytes().chunks(chunk_len) {
                        if let Ok(chunk_str) = std::str::from_utf8(chunk) {
                            if let Ok(code) = u32::from_str_radix(chunk_str, 16) {
                                if let Some(decoded_char) = std::char::from_u32(code) {
                                    result.push(decoded_char);
                                }
                            }
                        }
                    }
                    continue;
                }
            } else if chars.peek() == Some(&'\\') {
                chars.next();
                result.push('\\');
                continue;
            }
        } else if c == '\'' && chars.peek() == Some(&'\'') {
            chars.next();
            result.push('\'');
            continue;
        }
        result.push(c);
    }
    result
}

/// Low-level STEP ISO-10303-21 physical file parser
pub struct StepParser;

impl StepParser {
    pub fn parse_str(content: &str) -> Result<HashMap<u64, StepEntity>, IfcParseError> {
        let cleaned = Self::strip_comments(content);
        let mut entities = HashMap::new();

        // Extract DATA section
        let data_start = cleaned.find("DATA;").unwrap_or(0);
        let data_content = if data_start > 0 {
            &cleaned[data_start + 5..]
        } else {
            &cleaned[..]
        };
        let data_end = data_content.find("ENDSEC;").unwrap_or(data_content.len());
        let body = &data_content[..data_end];

        for stmt in body.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() || !stmt.starts_with('#') {
                continue;
            }

            if let Some(entity) = Self::parse_statement(stmt) {
                entities.insert(entity.id, entity);
            }
        }

        Ok(entities)
    }

    fn strip_comments(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_comment = false;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if !in_comment && c == '/' && chars.peek() == Some(&'*') {
                in_comment = true;
                chars.next();
            } else if in_comment && c == '*' && chars.peek() == Some(&'/') {
                in_comment = false;
                chars.next();
            } else if !in_comment {
                out.push(c);
            }
        }
        out
    }

    fn parse_statement(stmt: &str) -> Option<StepEntity> {
        let eq_idx = stmt.find('=')?;
        let id_part = stmt[1..eq_idx].trim();
        let id: u64 = id_part.parse().ok()?;

        let rest = stmt[eq_idx + 1..].trim();
        let open_paren = rest.find('(')?;
        let type_name = rest[..open_paren].trim().to_uppercase();

        let close_paren = rest.rfind(')')?;
        let args_str = &rest[open_paren + 1..close_paren];
        let parameters = Self::parse_parameter_list(args_str);

        Some(StepEntity {
            id,
            type_name,
            parameters,
        })
    }

    fn parse_parameter_list(text: &str) -> Vec<StepValue> {
        let mut values = Vec::new();
        let mut chars = text.chars().peekable();
        let mut current = String::new();
        let mut in_string = false;
        let mut paren_depth = 0;

        while let Some(&c) = chars.peek() {
            if c == '\'' && paren_depth == 0 {
                chars.next();
                if !in_string {
                    in_string = true;
                    current.clear();
                } else {
                    in_string = false;
                    values.push(StepValue::String(current.clone()));
                    current.clear();
                    // consume trailing delimiter if any
                    while let Some(&next_c) = chars.peek() {
                        if next_c == ',' || next_c.is_whitespace() {
                            chars.next();
                            if next_c == ',' {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                continue;
            } else if in_string {
                current.push(c);
                chars.next();
            } else if c == '(' {
                paren_depth += 1;
                current.push(c);
                chars.next();
            } else if c == ')' {
                paren_depth -= 1;
                current.push(c);
                chars.next();
            } else if c == ',' && paren_depth == 0 {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    values.push(Self::parse_token(trimmed));
                    current.clear();
                }
                chars.next();
            } else {
                current.push(c);
                chars.next();
            }
        }

        let trimmed = current.trim();
        if !trimmed.is_empty() {
            values.push(Self::parse_token(trimmed));
        }

        values
    }

    fn parse_token(token: &str) -> StepValue {
        if token == "$" {
            StepValue::Omitted
        } else if token == "*" {
            StepValue::Derived
        } else if token.starts_with('#') {
            if let Ok(id) = token[1..].parse::<u64>() {
                StepValue::EntityRef(id)
            } else {
                StepValue::Enumeration(token.to_string())
            }
        } else if token.starts_with('.') && token.ends_with('.') {
            StepValue::Enumeration(token[1..token.len() - 1].to_string())
        } else if token.starts_with('(') && token.ends_with(')') {
            let inner = &token[1..token.len() - 1];
            StepValue::List(Self::parse_parameter_list(inner))
        } else if let Ok(n) = token.parse::<f64>() {
            StepValue::Number(n)
        } else {
            StepValue::String(token.trim_matches('\'').to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// IFC Geometry & Placement Resolvers
// ---------------------------------------------------------------------------

struct GeometryContext<'a> {
    entities: &'a HashMap<u64, StepEntity>,
    placement_cache: HashMap<u64, DMat4>,
    unit_scale: f64, // e.g. 10.0 for cm -> mm
}

impl<'a> GeometryContext<'a> {
    fn new(entities: &'a HashMap<u64, StepEntity>, unit_scale: f64) -> Self {
        Self {
            entities,
            placement_cache: HashMap::new(),
            unit_scale,
        }
    }

    fn get_entity(&self, id: u64) -> Option<&'a StepEntity> {
        self.entities.get(&id)
    }

    fn resolve_point3d(&self, id: u64) -> Option<Point3D> {
        let ent = self.get_entity(id)?;
        if ent.type_name == "IFCCARTESIANPOINT" {
            if let Some(StepValue::List(coords)) = ent.parameters.first() {
                let x = coords.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                let y = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                let z = coords.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                return Some(Point3D::new(x, y, z));
            }
        }
        None
    }

    fn resolve_direction3d(&self, id: u64) -> Option<Vector3D> {
        let ent = self.get_entity(id)?;
        if ent.type_name == "IFCDIRECTION" {
            if let Some(StepValue::List(coords)) = ent.parameters.first() {
                let x = coords.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let z = coords.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                return Some(Vector3D::new(x, y, z));
            }
        }
        None
    }

    fn resolve_axis2_placement3d(&self, id: u64) -> DMat4 {
        let Some(ent) = self.get_entity(id) else {
            return DMat4::IDENTITY;
        };
        if ent.type_name != "IFCAXIS2PLACEMENT3D" {
            return DMat4::IDENTITY;
        }

        let loc = ent
            .parameters
            .get(0)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_point3d(ref_id))
            .unwrap_or(Point3D::ZERO);

        let axis_z = ent
            .parameters
            .get(1)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_direction3d(ref_id))
            .unwrap_or(Vector3D::new(0.0, 0.0, 1.0));

        let ref_dir_x = ent
            .parameters
            .get(2)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_direction3d(ref_id))
            .unwrap_or(Vector3D::new(1.0, 0.0, 0.0));

        let z = DVec3::new(axis_z.x, axis_z.y, axis_z.z).normalize_or_zero();
        let z = if z.length_squared() > 1e-6 {
            z
        } else {
            DVec3::Z
        };

        let x_raw = DVec3::new(ref_dir_x.x, ref_dir_x.y, ref_dir_x.z);
        let x_proj = x_raw - z * x_raw.dot(z);
        let x = if x_proj.length_squared() > 1e-6 {
            x_proj.normalize()
        } else {
            // Find perpendicular
            if z.x.abs() < 0.9 {
                DVec3::X.cross(z).normalize()
            } else {
                DVec3::Y.cross(z).normalize()
            }
        };

        let y = z.cross(x).normalize();

        DMat4::from_cols(
            DVec4::new(x.x, x.y, x.z, 0.0),
            DVec4::new(y.x, y.y, y.z, 0.0),
            DVec4::new(z.x, z.y, z.z, 0.0),
            DVec4::new(loc.x, loc.y, loc.z, 1.0),
        )
    }

    fn resolve_local_placement(&mut self, id: u64) -> DMat4 {
        if let Some(&mat) = self.placement_cache.get(&id) {
            return mat;
        }

        let Some(ent) = self.get_entity(id).cloned() else {
            return DMat4::IDENTITY;
        };

        let mat = match ent.type_name.as_str() {
            "IFCLOCALPLACEMENT" => {
                let parent_mat = if let Some(StepValue::EntityRef(parent_id)) = ent.parameters.get(0) {
                    self.resolve_local_placement(*parent_id)
                } else {
                    DMat4::IDENTITY
                };

                let rel_mat = if let Some(StepValue::EntityRef(rel_id)) = ent.parameters.get(1) {
                    self.resolve_axis2_placement3d(*rel_id)
                } else {
                    DMat4::IDENTITY
                };

                parent_mat * rel_mat
            }
            "IFCAXIS2PLACEMENT3D" => self.resolve_axis2_placement3d(id),
            _ => DMat4::IDENTITY,
        };

        self.placement_cache.insert(id, mat);
        mat
    }

    fn resolve_cartesian_transform_3d(&self, id: u64) -> DMat4 {
        let Some(ent) = self.get_entity(id) else {
            return DMat4::IDENTITY;
        };
        if ent.type_name != "IFCCARTESIANTRANSFORMATIONOPERATOR3D" {
            return DMat4::IDENTITY;
        }

        let loc = ent
            .parameters
            .get(2)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_point3d(ref_id))
            .unwrap_or(Point3D::ZERO);

        let scale = ent.parameters.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);

        let axis1 = ent
            .parameters
            .get(0)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_direction3d(ref_id))
            .unwrap_or(Vector3D::new(1.0, 0.0, 0.0));

        let axis2 = ent
            .parameters
            .get(1)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_direction3d(ref_id))
            .unwrap_or(Vector3D::new(0.0, 1.0, 0.0));

        let axis3 = ent
            .parameters
            .get(4)
            .and_then(|v| v.as_ref_id())
            .and_then(|ref_id| self.resolve_direction3d(ref_id))
            .unwrap_or(Vector3D::new(0.0, 0.0, 1.0));

        let x = DVec3::new(axis1.x, axis1.y, axis1.z).normalize_or_zero() * scale;
        let y = DVec3::new(axis2.x, axis2.y, axis2.z).normalize_or_zero() * scale;
        let z = DVec3::new(axis3.x, axis3.y, axis3.z).normalize_or_zero() * scale;

        DMat4::from_cols(
            DVec4::new(x.x, x.y, x.z, 0.0),
            DVec4::new(y.x, y.y, y.z, 0.0),
            DVec4::new(z.x, z.y, z.z, 0.0),
            DVec4::new(loc.x, loc.y, loc.z, 1.0),
        )
    }

    fn resolve_curve_points(&self, curve_id: u64) -> Vec<Point2D> {
        let Some(c_ent) = self.get_entity(curve_id) else {
            return Vec::new();
        };

        if c_ent.type_name == "IFCINDEXEDPOLYCURVE" {
            if let Some(StepValue::EntityRef(pts_id)) = c_ent.parameters.get(0) {
                if let Some(pts_ent) = self.get_entity(*pts_id) {
                    if let Some(StepValue::List(pt_lists)) = pts_ent.parameters.first() {
                        let mut pts = Vec::with_capacity(pt_lists.len());
                        for p in pt_lists {
                            if let StepValue::List(coords) = p {
                                let x = coords.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                                let y = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                                pts.push(Point2D::new(x, y));
                            }
                        }
                        return pts;
                    }
                }
            }
        } else if c_ent.type_name == "IFCPOLYLINE" {
            if let Some(StepValue::List(pt_refs)) = c_ent.parameters.first() {
                let mut pts = Vec::with_capacity(pt_refs.len());
                for r in pt_refs {
                    if let StepValue::EntityRef(pt_id) = r {
                        if let Some(p3d) = self.resolve_point3d(*pt_id) {
                            pts.push(Point2D::new(p3d.x, p3d.y));
                        }
                    }
                }
                return pts;
            }
        }
        Vec::new()
    }

    fn resolve_polygonal_faceset(&self, id: u64) -> TriangleMesh {
        let Some(ent) = self.get_entity(id) else {
            return TriangleMesh::new();
        };
        if ent.type_name != "IFCPOLYGONALFACESET" {
            return TriangleMesh::new();
        }

        // Coordinates
        let mut positions = Vec::new();
        if let Some(StepValue::EntityRef(pts_id)) = ent.parameters.get(0) {
            if let Some(pts_ent) = self.get_entity(*pts_id) {
                if let Some(StepValue::List(pt_lists)) = pts_ent.parameters.first() {
                    for p in pt_lists {
                        if let StepValue::List(coords) = p {
                            let x = coords.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                            let y = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                            let z = coords.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.unit_scale;
                            positions.push(Point3D::new(x, y, z));
                        }
                    }
                }
            }
        }

        if positions.is_empty() {
            return TriangleMesh::new();
        }

        // Faces
        let mut mesh = TriangleMesh::new();
        mesh.positions = positions;
        mesh.normals = vec![Vector3D::ZERO; mesh.positions.len()];

        if let Some(StepValue::List(faces)) = ent.parameters.get(2) {
            for f in faces {
                if let StepValue::EntityRef(face_id) = f {
                    if let Some(face_ent) = self.get_entity(*face_id) {
                        if let Some(StepValue::List(idx_list)) = face_ent.parameters.first() {
                            let indices: Vec<u32> = idx_list
                                .iter()
                                .filter_map(|v| v.as_f64().map(|n| (n as u32).saturating_sub(1)))
                                .collect();

                            let in_range = indices.iter().all(|&i| (i as usize) < mesh.positions.len());
                            if indices.len() >= 3 && in_range {
                                // Ear clipping: faces may be concave, a fan would spill outside them
                                let n = indices.len();
                                let loop_pts: Vec<Point3D> = indices.iter().map(|&i| mesh.positions[i as usize]).collect();
                                for [a, b, c] in triangulate_polygon_3d(&loop_pts) {
                                    mesh.indices.extend_from_slice(&[indices[a], indices[b], indices[c]]);
                                    mesh.edge_flags.push([
                                        is_polygon_edge(a, b, n),
                                        is_polygon_edge(b, c, n),
                                        is_polygon_edge(c, a, n),
                                    ]);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.recompute_normals(&mut mesh);
        mesh
    }

    fn resolve_extruded_area_solid(&self, id: u64) -> TriangleMesh {
        let Some(ent) = self.get_entity(id) else {
            return TriangleMesh::new();
        };
        if ent.type_name != "IFCEXTRUDEDAREASOLID" {
            return TriangleMesh::new();
        }

        let profile_id = ent.parameters.get(0).and_then(|v| v.as_ref_id());
        let pos_id = ent.parameters.get(1).and_then(|v| v.as_ref_id());
        let depth = ent.parameters.get(3).and_then(|v| v.as_f64()).unwrap_or(100.0) * self.unit_scale;

        let mut poly_pts = Vec::new();
        if let Some(p_id) = profile_id {
            if let Some(p_ent) = self.get_entity(p_id) {
                if p_ent.type_name == "IFCARBITRARYCLOSEDPROFILEDEF" {
                    if let Some(StepValue::EntityRef(curve_id)) = p_ent.parameters.get(2) {
                        poly_pts = self.resolve_curve_points(*curve_id);
                    }
                } else if p_ent.type_name == "IFCRECTANGLEPROFILEDEF" {
                    let x_dim = p_ent.parameters.get(3).and_then(|v| v.as_f64()).unwrap_or(100.0) * self.unit_scale;
                    let y_dim = p_ent.parameters.get(4).and_then(|v| v.as_f64()).unwrap_or(100.0) * self.unit_scale;
                    let hx = x_dim * 0.5;
                    let hy = y_dim * 0.5;
                    poly_pts = vec![
                        Point2D::new(-hx, -hy),
                        Point2D::new(hx, -hy),
                        Point2D::new(hx, hy),
                        Point2D::new(-hx, hy),
                    ];
                }
            }
        }

        if poly_pts.len() < 3 {
            return TriangleMesh::new();
        }

        let polyline = Polyline2D::new(poly_pts, true);
        let mut mesh = extrude_polygon(&polyline, 0.0, depth);

        // Apply local solid position if present
        if let Some(p_id) = pos_id {
            let pos_mat = self.resolve_axis2_placement3d(p_id);
            self.transform_mesh(&mut mesh, pos_mat);
        }

        mesh
    }

    fn resolve_mapped_item(&mut self, id: u64) -> TriangleMesh {
        let Some(ent) = self.get_entity(id).cloned() else {
            return TriangleMesh::new();
        };
        if ent.type_name != "IFCMAPPEDITEM" {
            return TriangleMesh::new();
        }

        let source_id = ent.parameters.get(0).and_then(|v| v.as_ref_id());
        let target_id = ent.parameters.get(1).and_then(|v| v.as_ref_id());

        let mut base_mesh = TriangleMesh::new();
        let mut mapping_origin_mat = DMat4::IDENTITY;

        if let Some(s_id) = source_id {
            if let Some(s_ent) = self.get_entity(s_id).cloned() {
                if s_ent.type_name == "IFCREPRESENTATIONMAP" {
                    if let Some(StepValue::EntityRef(origin_id)) = s_ent.parameters.get(0) {
                        mapping_origin_mat = self.resolve_axis2_placement3d(*origin_id);
                    }
                    if let Some(StepValue::EntityRef(rep_id)) = s_ent.parameters.get(1) {
                        base_mesh = self.resolve_shape_representation(*rep_id).0;
                    }
                }
            }
        }

        let target_mat = if let Some(t_id) = target_id {
            self.resolve_cartesian_transform_3d(t_id)
        } else {
            DMat4::IDENTITY
        };

        let combined = target_mat * mapping_origin_mat.inverse();
        self.transform_mesh(&mut base_mesh, combined);
        base_mesh
    }

    fn resolve_shape_representation(&mut self, rep_id: u64) -> (TriangleMesh, Vec<Polyline2D>) {
        let Some(rep_ent) = self.get_entity(rep_id).cloned() else {
            return (TriangleMesh::new(), Vec::new());
        };
        if rep_ent.type_name != "IFCSHAPEREPRESENTATION" {
            return (TriangleMesh::new(), Vec::new());
        }

        let mut composite_mesh = TriangleMesh::new();
        let mut polylines = Vec::new();

        if let Some(StepValue::List(items)) = rep_ent.parameters.get(3) {
            for it in items {
                if let StepValue::EntityRef(item_id) = it {
                    if let Some(it_ent) = self.get_entity(*item_id) {
                        match it_ent.type_name.as_str() {
                            "IFCPOLYGONALFACESET" => {
                                let m = self.resolve_polygonal_faceset(*item_id);
                                composite_mesh.append(&m);
                            }
                            "IFCEXTRUDEDAREASOLID" => {
                                let m = self.resolve_extruded_area_solid(*item_id);
                                composite_mesh.append(&m);
                            }
                            "IFCMAPPEDITEM" => {
                                let m = self.resolve_mapped_item(*item_id);
                                composite_mesh.append(&m);
                            }
                            "IFCINDEXEDPOLYCURVE" | "IFCPOLYLINE" => {
                                let pts = self.resolve_curve_points(*item_id);
                                if pts.len() >= 2 {
                                    polylines.push(Polyline2D::new(pts, true));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        (composite_mesh, polylines)
    }

    fn resolve_product_definition_shape(&mut self, shape_id: u64) -> (TriangleMesh, Vec<Polyline2D>) {
        let Some(shape_ent) = self.get_entity(shape_id).cloned() else {
            return (TriangleMesh::new(), Vec::new());
        };
        if shape_ent.type_name != "IFCPRODUCTDEFINITIONSHAPE" {
            return (TriangleMesh::new(), Vec::new());
        }

        let mut body_mesh = TriangleMesh::new();
        let mut axis_polys = Vec::new();

        if let Some(StepValue::List(rep_refs)) = shape_ent.parameters.get(2) {
            for r in rep_refs {
                if let StepValue::EntityRef(rep_id) = r {
                    if let Some(rep_ent) = self.get_entity(*rep_id) {
                        let rep_id_str = rep_ent.parameters.get(1).and_then(|v| v.as_str()).unwrap_or("");
                        let (m, p) = self.resolve_shape_representation(*rep_id);
                        if rep_id_str == "Body" || (body_mesh.vertex_count() == 0 && m.vertex_count() > 0) {
                            body_mesh.append(&m);
                        }
                        if rep_id_str == "Axis" || rep_id_str == "FootPrint" || !p.is_empty() {
                            axis_polys.extend(p);
                        }
                    }
                }
            }
        }

        (body_mesh, axis_polys)
    }

    fn transform_mesh(&self, mesh: &mut TriangleMesh, mat: DMat4) {
        for p in &mut mesh.positions {
            let v = DVec4::new(p.x, p.y, p.z, 1.0);
            let tv = mat * v;
            *p = Point3D::new(tv.x, tv.y, tv.z);
        }
        self.recompute_normals(mesh);
    }

    fn recompute_normals(&self, mesh: &mut TriangleMesh) {
        mesh.normals = vec![Vector3D::ZERO; mesh.positions.len()];
        for chunk in mesh.indices.chunks_exact(3) {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;
            if i0 < mesh.positions.len() && i1 < mesh.positions.len() && i2 < mesh.positions.len() {
                let p0 = mesh.positions[i0];
                let p1 = mesh.positions[i1];
                let p2 = mesh.positions[i2];
                let edge1 = p1 - p0;
                let edge2 = p2 - p0;
                let normal = edge1.cross(edge2);
                let len = normal.length();
                if len > 1e-8 {
                    let unit = normal / len;
                    mesh.normals[i0] = mesh.normals[i0] + unit;
                    mesh.normals[i1] = mesh.normals[i1] + unit;
                    mesh.normals[i2] = mesh.normals[i2] + unit;
                }
            }
        }
        for n in &mut mesh.normals {
            let len = n.length();
            if len > 1e-8 {
                *n = *n / len;
            } else {
                *n = Vector3D::new(0.0, 0.0, 1.0);
            }
        }
    }
}

/// High-Level IFC Importer for converting OpenBIM STEP files into a live BimDocument
pub struct IfcImporter;

impl IfcImporter {
    pub fn import_from_file<P: AsRef<Path>>(path: P) -> Result<BimDocument, IfcParseError> {
        let content = fs::read_to_string(path)?;
        Self::import_from_str(&content)
    }

    pub fn import_from_str(content: &str) -> Result<BimDocument, IfcParseError> {
        let entities = StepParser::parse_str(content)?;

        // 1. Determine Unit Scale (convert length to mm)
        // IFC standard: IFCPROJECT -> parameter 8 is IFCUNITASSIGNMENT -> list of IFCSIUNIT
        let mut unit_scale = 10.0; // Default to centimeters (common in Taiwanese architectural models)

        let mut unit_assignment_id = None;
        for ent in entities.values() {
            if ent.type_name == "IFCPROJECT" {
                if let Some(ua_val) = ent.parameters.get(8) {
                    unit_assignment_id = ua_val.as_ref_id();
                }
                break;
            }
        }
        if unit_assignment_id.is_none() {
            for ent in entities.values() {
                if ent.type_name == "IFCUNITASSIGNMENT" {
                    unit_assignment_id = Some(ent.id);
                    break;
                }
            }
        }

        if let Some(ua_id) = unit_assignment_id {
            if let Some(ua_ent) = entities.get(&ua_id) {
                if let Some(StepValue::List(units)) = ua_ent.parameters.first() {
                    for u in units {
                        if let Some(u_id) = u.as_ref_id() {
                            if let Some(u_ent) = entities.get(&u_id) {
                                if u_ent.type_name == "IFCSIUNIT" {
                                    let unit_type = u_ent.parameters.get(1).and_then(|v| v.as_str()).unwrap_or("");
                                    if unit_type == "LENGTHUNIT" {
                                        let prefix = u_ent.parameters.get(2).and_then(|v| v.as_str()).unwrap_or("");
                                        match prefix {
                                            "MILLI" => unit_scale = 1.0,
                                            "CENTI" => unit_scale = 10.0,
                                            "DECI" => unit_scale = 100.0,
                                            "METRE" | "$" | "" => {
                                                // If meters, coordinates like wall length 3.5 = 3500mm
                                                unit_scale = 1000.0;
                                            }
                                            _ => {}
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Fallback: search for first IFCSIUNIT with LENGTHUNIT
            for ent in entities.values() {
                if ent.type_name == "IFCSIUNIT" {
                    let unit_type = ent.parameters.get(1).and_then(|v| v.as_str()).unwrap_or("");
                    if unit_type == "LENGTHUNIT" {
                        let prefix = ent.parameters.get(2).and_then(|v| v.as_str()).unwrap_or("");
                        match prefix {
                            "MILLI" => { unit_scale = 1.0; break; }
                            "CENTI" => { unit_scale = 10.0; break; }
                            "DECI" => { unit_scale = 100.0; break; }
                            "METRE" | "$" | "" => { unit_scale = 1000.0; break; }
                            _ => {}
                        }
                    }
                }
            }
        }

        // 2. Find Project & Site Name
        let mut project_name = None;
        let mut site_name = None;
        for entity in entities.values() {
            if entity.type_name == "IFCPROJECT" {
                if let Some(name_val) = entity.parameters.get(2) {
                    if let Some(name) = name_val.as_str() {
                        let decoded = decode_ifc_unicode(name);
                        if !decoded.trim().is_empty() && decoded != "$" {
                            project_name = Some(decoded);
                        }
                    }
                }
            } else if entity.type_name == "IFCSITE" {
                if let Some(name_val) = entity.parameters.get(2) {
                    if let Some(name) = name_val.as_str() {
                        let decoded = decode_ifc_unicode(name);
                        if !decoded.trim().is_empty() && decoded != "$" {
                            site_name = Some(decoded);
                        }
                    }
                }
            }
        }

        let doc_name = project_name
            .or(site_name)
            .unwrap_or_else(|| "IFC 建築專案".to_string());

        let mut doc = BimDocument::new(doc_name);

        // 3. Map Spatial Structure (Building Storeys)
        let mut storey_names: HashMap<u64, String> = HashMap::new();
        for ent in entities.values() {
            if ent.type_name == "IFCBUILDINGSTOREY" {
                let name = ent
                    .parameters
                    .get(2)
                    .and_then(|v| v.as_str())
                    .map(decode_ifc_unicode)
                    .unwrap_or_else(|| format!("Storey #{}", ent.id));
                storey_names.insert(ent.id, name);
            }
        }

        let mut element_storeys: HashMap<u64, String> = HashMap::new();
        for ent in entities.values() {
            if ent.type_name == "IFCRELCONTAINEDINSPATIALSTRUCTURE" {
                let storey_name = ent
                    .parameters
                    .get(5)
                    .and_then(|v| v.as_ref_id())
                    .and_then(|sid| storey_names.get(&sid))
                    .cloned();

                if let Some(st_name) = storey_name {
                    if let Some(StepValue::List(rel_elements)) = ent.parameters.get(4) {
                        for el in rel_elements {
                            if let StepValue::EntityRef(el_id) = el {
                                element_storeys.insert(*el_id, st_name.clone());
                            }
                        }
                    }
                }
            }
        }

        let default_concrete = Material::concrete();
        let default_wood = Material::wood();
        let default_glass = Material::glass();
        let default_steel = Material::structural_steel();

        let mut geom_ctx = GeometryContext::new(&entities, unit_scale);

        // Storey elevations: prefer the storey placement (the frame contained elements are
        // placed in), falling back to the IfcBuildingStorey.Elevation attribute.
        let mut storeys: Vec<Storey> = Vec::new();
        for ent in entities.values() {
            if ent.type_name == "IFCBUILDINGSTOREY" {
                let Some(name) = storey_names.get(&ent.id) else { continue };
                let placement_z = ent
                    .parameters
                    .get(5)
                    .and_then(|v| v.as_ref_id())
                    .map(|p_id| geom_ctx.resolve_local_placement(p_id).w_axis.z);
                let attr_z = ent.parameters.get(9).and_then(|v| v.as_f64()).map(|e| e * unit_scale);
                if let Some(elevation) = placement_z.or(attr_z) {
                    storeys.push(Storey { name: name.clone(), elevation, source_elevation: Some(elevation) });
                }
            }
        }

        // 4. Parse all Building Elements
        let mut raw_components: Vec<IfcComponent> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for ent in entities.values() {
            let kind = match ent.type_name.as_str() {
                "IFCWALL" | "IFCWALLSTANDARDCASE" => ElementKind::Wall,
                "IFCCOLUMN" => ElementKind::Column,
                "IFCBEAM" => ElementKind::Beam,
                "IFCSLAB" => ElementKind::Slab,
                "IFCDOOR" => ElementKind::Door,
                "IFCWINDOW" => ElementKind::Window,
                "IFCSTAIR" | "IFCSTAIRFLIGHT" => ElementKind::Stairs,
                "IFCRAILING" => ElementKind::Railing,
                "IFCROOF" => ElementKind::Roof,
                "IFCCOVERING" => ElementKind::Ceiling,
                "IFCBUILDINGELEMENTPROXY" => ElementKind::Furniture,
                _ => continue,
            };

            let mut element_id = ent
                .parameters
                .get(0)
                .and_then(|v| v.as_str())
                .and_then(ElementId::from_ifc_guid)
                .unwrap_or_else(ElementId::new);

            if !seen_ids.insert(element_id) {
                element_id = ElementId::new();
                seen_ids.insert(element_id);
            }

            let raw_name = ent
                .parameters
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or("IFC Element");
            let name = decode_ifc_unicode(raw_name);

            let placement_id = ent.parameters.get(5).and_then(|v| v.as_ref_id());
            let shape_id = ent.parameters.get(6).and_then(|v| v.as_ref_id());

            let (mut mesh, mut polys) = if let Some(s_id) = shape_id {
                geom_ctx.resolve_product_definition_shape(s_id)
            } else {
                (TriangleMesh::new(), Vec::new())
            };

            // Compute World Transform Matrix
            let world_mat = if let Some(p_id) = placement_id {
                geom_ctx.resolve_local_placement(p_id)
            } else {
                DMat4::IDENTITY
            };

            // Transform into world CAD coordinates (mesh and points are already in mm)
            geom_ctx.transform_mesh(&mut mesh, world_mat);
            // Automatically compute sharp feature/crease edges (suppressing internal coplanar triangulation slashes)
            mesh.compute_crease_edges(25.0f64.to_radians());

            // Transform 2D polylines into world CAD space
            for poly in &mut polys {
                for v in &mut poly.vertices {
                    let tv = world_mat * DVec4::new(v.x, v.y, 0.0, 1.0);
                    *v = Point2D::new(tv.x, tv.y);
                }
            }

            // If no explicit 2D footprint was found, generate it from the 3D mesh base
            if polys.is_empty() && mesh.vertex_count() > 0 {
                if let Some(bbox) = mesh.bbox() {
                    let min_p = bbox.min;
                    let max_p = bbox.max;
                    let fp = Polyline2D::new(
                        vec![
                            Point2D::new(min_p.x, min_p.y),
                            Point2D::new(max_p.x, min_p.y),
                            Point2D::new(max_p.x, max_p.y),
                            Point2D::new(min_p.x, max_p.y),
                        ],
                        true,
                    );
                    polys.push(fp);
                }
            }

            let mat = match kind {
                ElementKind::Wall | ElementKind::Column | ElementKind::Slab => default_concrete.clone(),
                ElementKind::Beam => default_steel.clone(),
                ElementKind::Door => default_wood.clone(),
                ElementKind::Window => default_glass.clone(),
                ElementKind::Stairs => default_concrete.clone(),
                ElementKind::Railing => default_steel.clone(),
                _ => default_concrete.clone(),
            };

            let mut comp = IfcComponent::new(name, kind, mesh, polys, mat).with_id(element_id);
            if let Some(st) = element_storeys.get(&ent.id) {
                comp = comp.with_storey(st);
            }

            raw_components.push(comp);
        }

        // 5. Automatic Centering & Floor Elevation Alignment
        // In real-world survey coordinates, the site can be thousands of meters away from origin.
        // We find the bounding box of the entire building, center (X, Y) at (0, 0), and set base_z = 0.
        let mut overall_min = Point3D::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut overall_max = Point3D::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut struct_min = Point3D::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut struct_max = Point3D::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut has_struct = false;

        for comp in &raw_components {
            if let Some(b) = comp.mesh.bbox() {
                overall_min.x = overall_min.x.min(b.min.x);
                overall_min.y = overall_min.y.min(b.min.y);
                overall_min.z = overall_min.z.min(b.min.z);

                overall_max.x = overall_max.x.max(b.max.x);
                overall_max.y = overall_max.y.max(b.max.y);
                overall_max.z = overall_max.z.max(b.max.z);

                if matches!(comp.kind, ElementKind::Wall | ElementKind::Column | ElementKind::Beam | ElementKind::Door | ElementKind::Window | ElementKind::Stairs) {
                    struct_min.x = struct_min.x.min(b.min.x);
                    struct_min.y = struct_min.y.min(b.min.y);
                    struct_min.z = struct_min.z.min(b.min.z);

                    struct_max.x = struct_max.x.max(b.max.x);
                    struct_max.y = struct_max.y.max(b.max.y);
                    struct_max.z = struct_max.z.max(b.max.z);
                    has_struct = true;
                }
            }
        }

        let ref_min = if has_struct { struct_min } else { overall_min };
        let ref_max = if has_struct { struct_max } else { overall_max };

        let needs_centering = ref_min.x.is_finite()
            && (ref_min.x.abs() > 2000.0 || ref_min.y.abs() > 2000.0);

        let center_offset = if needs_centering {
            let cx = (ref_min.x + ref_max.x) * 0.5;
            let cy = (ref_min.y + ref_max.y) * 0.5;
            let cz = ref_min.z;
            Vector3D::new(-cx, -cy, -cz)
        } else {
            Vector3D::ZERO
        };

        for mut comp in raw_components {
            if needs_centering {
                comp.translate(center_offset);
            }
            doc.add_element(comp);
        }
        for storey in &mut storeys {
            storey.elevation += center_offset.z;
        }
        doc.set_storeys(storeys);

        doc.sync();
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifc::IfcExporter;
    use sketchcad_bim::{Door, Wall};

    #[test]
    fn test_ifc_roundtrip_import() {
        let mut doc = BimDocument::new("IFC Roundtrip Test Document");
        let wall = Wall::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(8000.0, 0.0),
            200.0,
            3500.0,
            Material::concrete(),
        );
        let wall_id = doc.add_element(wall);

        let door = Door::new(
            wall_id,
            2000.0,
            1000.0,
            2200.0,
            Material::wood(),
            12000.0,
        );
        let door_id = doc.add_element(door);

        let temp_path = std::env::temp_dir().join("test_roundtrip.ifc");
        IfcExporter::export_to_ifc4(&doc, &temp_path).expect("Export IFC");

        let imported_doc = IfcImporter::import_from_file(&temp_path).expect("Import IFC");
        assert_eq!(imported_doc.name, "IFC Roundtrip Test Document");
        assert!(imported_doc.get_element(wall_id).is_some());
        assert!(imported_doc.get_element(door_id).is_some());

        let _ = fs::remove_file(temp_path);
    }
}
