use serde::{Deserialize, Serialize};
use sketchcad_core::{BBox3D, Point3D, Vector3D};

/// A triangle mesh suitable for GPU rendering and export (e.g. IFC / OBJ / DXF 3DFACE).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriangleMesh {
    pub positions: Vec<Point3D>,
    pub normals: Vec<Vector3D>,
    pub indices: Vec<u32>,
    #[serde(default)]
    pub edge_flags: Vec<[bool; 3]>,
}

impl TriangleMesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn bbox(&self) -> Option<BBox3D> {
        BBox3D::from_points(&self.positions)
    }

    /// Appends another triangle mesh into this one, re-indexing appropriately.
    pub fn append(&mut self, other: &TriangleMesh) {
        let base_index = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        for &idx in &other.indices {
            self.indices.push(base_index + idx);
        }
        self.edge_flags.extend_from_slice(&other.edge_flags);
    }

    /// Computes feature crease edges by inspecting dihedral angles between adjacent triangles.
    /// Internal triangulation edges between coplanar faces (angle < threshold) are marked false.
    /// Boundary edges and sharp crease edges (angle >= threshold) are marked true.
    pub fn compute_crease_edges(&mut self, crease_angle_rad: f64) {
        let tri_count = self.indices.len() / 3;
        if tri_count == 0 {
            self.edge_flags.clear();
            return;
        }

        // 1. Calculate face normal for each triangle
        let mut face_normals = Vec::with_capacity(tri_count);
        for t in 0..tri_count {
            let i0 = self.indices[t * 3] as usize;
            let i1 = self.indices[t * 3 + 1] as usize;
            let i2 = self.indices[t * 3 + 2] as usize;

            if i0 < self.positions.len() && i1 < self.positions.len() && i2 < self.positions.len() {
                let p0 = self.positions[i0];
                let p1 = self.positions[i1];
                let p2 = self.positions[i2];
                let n = (p1 - p0).cross(p2 - p0);
                let len = n.length();
                // Slivers from polygon triangulation (collinear vertices) have no meaningful
                // normal; judge by the angle (|cross| / longest edge²), not the absolute area.
                let longest_sq = (p1 - p0)
                    .length_squared()
                    .max((p2 - p1).length_squared())
                    .max((p0 - p2).length_squared());
                face_normals.push(if len > 1e-6 && len > longest_sq * 1e-5 { n / len } else { Vector3D::ZERO });
            } else {
                face_normals.push(Vector3D::ZERO);
            }
        }

        // 2. Build spatial edge map using quantized coordinates (0.1mm tolerance)
        #[inline]
        fn quantize(p: Point3D) -> (i64, i64, i64) {
            (
                (p.x * 10.0).round() as i64,
                (p.y * 10.0).round() as i64,
                (p.z * 10.0).round() as i64,
            )
        }

        type EdgeKey = ((i64, i64, i64), (i64, i64, i64));
        let mut edge_map: std::collections::HashMap<EdgeKey, Vec<(usize, usize)>> =
            std::collections::HashMap::with_capacity(tri_count * 3);

        for t in 0..tri_count {
            let i0 = self.indices[t * 3] as usize;
            let i1 = self.indices[t * 3 + 1] as usize;
            let i2 = self.indices[t * 3 + 2] as usize;
            if i0 >= self.positions.len() || i1 >= self.positions.len() || i2 >= self.positions.len() {
                continue;
            }
            if face_normals[t] == Vector3D::ZERO {
                // Degenerate triangle: its edges lie along neighbouring edges, never draw them
                // and do not let it turn a neighbour's interior edge into a false "crease".
                continue;
            }

            let pts = [self.positions[i0], self.positions[i1], self.positions[i2]];
            let keys = [
                quantize(pts[0]),
                quantize(pts[1]),
                quantize(pts[2]),
            ];

            let edges = [
                (keys[0], keys[1]),
                (keys[1], keys[2]),
                (keys[2], keys[0]),
            ];

            for (e_idx, &(k0, k1)) in edges.iter().enumerate() {
                if k0 == k1 {
                    continue; // Degenerate edge
                }
                let canonical_key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
                edge_map.entry(canonical_key).or_default().push((t, e_idx));
            }
        }

        // 3. Mark edges: true for silhouette/crease edges, false for coplanar internal edges
        let cos_thresh = crease_angle_rad.cos();
        let mut flags = vec![[false; 3]; tri_count];

        for (_key, entries) in edge_map {
            if entries.len() == 1 {
                // Open boundary edge: always visible
                let (t, e) = entries[0];
                flags[t][e] = true;
            } else if entries.len() == 2 {
                let (t0, e0) = entries[0];
                let (t1, e1) = entries[1];
                let n0 = face_normals[t0];
                let n1 = face_normals[t1];
                let dot = n0.dot(n1);

                // If dot < cos_thresh, dihedral angle >= crease_angle -> feature edge!
                if dot < cos_thresh {
                    flags[t0][e0] = true;
                    flags[t1][e1] = true;
                }
            } else {
                // Non-manifold edge (3+ faces meeting): mark visible
                for &(t, e) in &entries {
                    flags[t][e] = true;
                }
            }
        }

        self.edge_flags = flags;
    }

    /// Creates a 3D rectangular box mesh (e.g. for simple walls, beams, columns).
    pub fn create_box(size: Vector3D, center: Point3D) -> Self {
        let hx = size.x * 0.5;
        let hy = size.y * 0.5;
        let hz = size.z * 0.5;

        // 8 corners
        let corners = [
            Point3D::new(-hx, -hy, -hz) + center, // 0
            Point3D::new(hx, -hy, -hz) + center,  // 1
            Point3D::new(hx, hy, -hz) + center,   // 2
            Point3D::new(-hx, hy, -hz) + center,  // 3
            Point3D::new(-hx, -hy, hz) + center,  // 4
            Point3D::new(hx, -hy, hz) + center,   // 5
            Point3D::new(hx, hy, hz) + center,    // 6
            Point3D::new(-hx, hy, hz) + center,   // 7
        ];

        // 6 faces * 2 triangles = 12 triangles (all CCW with outward-pointing normals)
        let face_indices: &[[u32; 4]] = &[
            [0, 3, 2, 1], // Bottom (-Z)
            [4, 5, 6, 7], // Top (+Z)
            [0, 1, 5, 4], // Front (-Y)
            [2, 3, 7, 6], // Back (+Y)
            [3, 0, 4, 7], // Left (-X)
            [1, 2, 6, 5], // Right (+X)
        ];

        let mut mesh = TriangleMesh::new();

        for quad in face_indices {
            let p0 = corners[quad[0] as usize];
            let p1 = corners[quad[1] as usize];
            let p2 = corners[quad[2] as usize];
            let p3 = corners[quad[3] as usize];

            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let normal = edge1.cross(edge2).normalize();

            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);

            // Two triangles for the quad (diagonal internal edge is suppressed)
            mesh.indices.extend_from_slice(&[
                base, base + 1, base + 2,
                base, base + 2, base + 3,
            ]);
            mesh.edge_flags.push([true, true, false]);
            mesh.edge_flags.push([false, true, true]);
        }

        mesh
    }

    /// Creates an oriented 3D solid box along a 3D centerline from `start` to `end`,
    /// with `width_vec` defining the cross-width vector and `height_vec` defining the thickness/elevation vector.
    pub fn create_oriented_box(
        start: Point3D,
        end: Point3D,
        width_vec: Vector3D,
        height_vec: Vector3D,
    ) -> Self {
        let hw = width_vec * 0.5;
        let hh = height_vec * 0.5;

        // 8 corners
        let corners = [
            start - hw - hh, // 0
            start + hw - hh, // 1
            end + hw - hh,   // 2
            end - hw - hh,   // 3
            start - hw + hh, // 4
            start + hw + hh, // 5
            end + hw + hh,   // 6
            end - hw + hh,   // 7
        ];

        let face_indices: &[[u32; 4]] = &[
            [0, 3, 2, 1], // Bottom (-hh)
            [4, 5, 6, 7], // Top (+hh)
            [0, 1, 5, 4], // Start cap (-dir)
            [2, 3, 7, 6], // End cap (+dir)
            [3, 0, 4, 7], // Left (-hw)
            [1, 2, 6, 5], // Right (+hw)
        ];

        let mut mesh = TriangleMesh::new();

        for quad in face_indices {
            let p0 = corners[quad[0] as usize];
            let p1 = corners[quad[1] as usize];
            let p2 = corners[quad[2] as usize];
            let p3 = corners[quad[3] as usize];

            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let normal = edge1.cross(edge2).normalize();

            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[normal, normal, normal, normal]);

            mesh.indices.extend_from_slice(&[
                base, base + 1, base + 2,
                base, base + 2, base + 3,
            ]);
            mesh.edge_flags.push([true, true, false]);
            mesh.edge_flags.push([false, true, true]);
        }

        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_crease_edges_suppresses_coplanar_diagonals() {
        // Flat quad split into two coplanar triangles with shared diagonal (0, 2)
        // 3---2
        // | / |
        // 0---1
        let mut mesh = TriangleMesh::new();
        mesh.positions = vec![
            Point3D::new(0.0, 0.0, 0.0), // 0
            Point3D::new(10.0, 0.0, 0.0), // 1
            Point3D::new(10.0, 10.0, 0.0), // 2
            Point3D::new(0.0, 10.0, 0.0), // 3
        ];
        // Triangle 0: (0, 1, 2)
        // Triangle 1: (0, 2, 3)
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        mesh.compute_crease_edges(25.0f64.to_radians());

        assert_eq!(mesh.edge_flags.len(), 2);
        // Triangle 0 edges: (0,1)=true, (1,2)=true, (2,0)=false (diagonal)
        assert_eq!(mesh.edge_flags[0], [true, true, false]);
        // Triangle 1 edges: (0,2)=false (diagonal), (2,3)=true, (3,0)=true
        assert_eq!(mesh.edge_flags[1], [false, true, true]);
    }

    #[test]
    fn test_compute_crease_edges_preserves_90_degree_box_corners() {
        let mut box_mesh = TriangleMesh::create_box(Vector3D::new(100.0, 100.0, 100.0), Point3D::ZERO);
        box_mesh.compute_crease_edges(25.0f64.to_radians());

        // In a box of 12 triangles (6 quads * 2), each quad has 1 internal diagonal (false) and 4 outer edges (true)
        // Total edges = 12 triangles * 3 = 36 edges
        // 12 shared diagonals (2 per quad * 6 = 12 half-edges) should be false!
        // 24 boundary crease edges should be true!
        let mut false_count = 0;
        let mut true_count = 0;
        for flags in &box_mesh.edge_flags {
            for &f in flags {
                if f { true_count += 1; } else { false_count += 1; }
            }
        }
        assert_eq!(false_count, 12, "All 6 internal quad diagonals must be false");
        assert_eq!(true_count, 24, "All 12 sharp 90-degree box edges must be true");
    }
}
