use crate::half_edge::TriangleMesh;
use crate::primitives::Polyline2D;
use crate::triangulate::{is_polygon_edge, triangulate_polygon};
use sketchcad_core::{Point3D, Vector3D};

/// Push-Pull Extrusion engine: takes a 2D closed planar polygon and extrudes it
/// along a 3D direction vector (height) to create a watertight 3D solid mesh.
pub fn extrude_polygon(polygon: &Polyline2D, z_base: f64, height: f64) -> TriangleMesh {
    let mut mesh = TriangleMesh::new();

    // Drop repeated points (including a closing point equal to the first one)
    let mut verts = polygon.vertices.clone();
    verts.dedup_by(|a, b| a.distance(*b) < 1e-6);
    while verts.len() > 1 && verts[0].distance(verts[verts.len() - 1]) < 1e-6 {
        verts.pop();
    }
    let n = verts.len();
    if n < 3 {
        return mesh;
    }
    if Polyline2D::new(verts.clone(), true).signed_area() < 0.0 {
        verts.reverse();
    }

    // 1. Bottom face vertices (z = z_base)
    // 2. Top face vertices (z = z_base + height)
    let mut bottom_indices = Vec::with_capacity(n);
    let mut top_indices = Vec::with_capacity(n);

    for p in &verts {
        let b_idx = mesh.positions.len() as u32;
        mesh.positions.push(Point3D::new(p.x, p.y, z_base));
        mesh.normals.push(Vector3D::new(0.0, 0.0, -1.0));
        bottom_indices.push(b_idx);

        let t_idx = mesh.positions.len() as u32;
        mesh.positions.push(Point3D::new(p.x, p.y, z_base + height));
        mesh.normals.push(Vector3D::new(0.0, 0.0, 1.0));
        top_indices.push(t_idx);
    }

    // Cap faces: ear-clipped so concave outlines (L / U shaped slabs) stay inside the profile
    let caps = triangulate_polygon(&verts);

    // Bottom face (facing down, clockwise)
    for &[a, b, c] in &caps {
        mesh.indices.extend_from_slice(&[bottom_indices[a], bottom_indices[c], bottom_indices[b]]);
        mesh.edge_flags.push([is_polygon_edge(a, c, n), is_polygon_edge(c, b, n), is_polygon_edge(b, a, n)]);
    }

    // Top face (facing up, counter-clockwise)
    for &[a, b, c] in &caps {
        mesh.indices.extend_from_slice(&[top_indices[a], top_indices[b], top_indices[c]]);
        mesh.edge_flags.push([is_polygon_edge(a, b, n), is_polygon_edge(b, c, n), is_polygon_edge(c, a, n)]);
    }

    // Side faces (quads along each segment)
    for i in 0..n {
        let next = (i + 1) % n;
        let p_bottom_0 = Point3D::new(verts[i].x, verts[i].y, z_base);
        let p_bottom_1 = Point3D::new(verts[next].x, verts[next].y, z_base);
        let p_top_0 = Point3D::new(verts[i].x, verts[i].y, z_base + height);
        let p_top_1 = Point3D::new(verts[next].x, verts[next].y, z_base + height);

        let seg_dir = p_bottom_1 - p_bottom_0;
        let up = Vector3D::new(0.0, 0.0, 1.0);
        let side_normal = seg_dir.cross(up).normalize_or_zero();

        let base = mesh.positions.len() as u32;
        mesh.positions.extend_from_slice(&[p_bottom_0, p_bottom_1, p_top_1, p_top_0]);
        mesh.normals.extend_from_slice(&[side_normal, side_normal, side_normal, side_normal]);

        mesh.indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
        mesh.edge_flags.push([true, true, false]);
        mesh.edge_flags.push([false, true, true]);
    }

    mesh
}
