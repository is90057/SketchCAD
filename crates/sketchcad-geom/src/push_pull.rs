use crate::half_edge::TriangleMesh;
use sketchcad_core::{Point3D, Vector3D};

/// A planar polygonal face in 3D space ready for interactive Push-Pull direct modeling
#[derive(Debug, Clone, PartialEq)]
pub struct PlanarFace3D {
    pub vertices: Vec<Point3D>,
}

impl PlanarFace3D {
    pub fn new(vertices: Vec<Point3D>) -> Self {
        Self { vertices }
    }

    /// Computes the unit normal of the planar face using Newell's method for arbitrary 3D polygons
    pub fn normal(&self) -> Vector3D {
        let n = self.vertices.len();
        if n < 3 {
            return Vector3D::Z;
        }

        let mut normal = Vector3D::ZERO;
        for i in 0..n {
            let current = self.vertices[i];
            let next = self.vertices[(i + 1) % n];

            normal.x += (current.y - next.y) * (current.z + next.z);
            normal.y += (current.z - next.z) * (current.x + next.x);
            normal.z += (current.x - next.x) * (current.y + next.y);
        }

        if normal.length_squared() > 1e-12 {
            normal.normalize()
        } else {
            Vector3D::Z
        }
    }

    /// Computes center point of the face
    pub fn centroid(&self) -> Point3D {
        if self.vertices.is_empty() {
            return Point3D::ZERO;
        }
        let sum: Point3D = self.vertices.iter().fold(Point3D::ZERO, |acc, &p| acc + p);
        sum / (self.vertices.len() as f64)
    }

    /// Push-Pull Operation (Planar direct modeling extrusion):
    /// Extrudes the face along its normal by `distance`.
    /// Positive distance pulls the face outwards, generating connected side faces and a new displaced cap face.
    pub fn push_pull(&self, distance: f64) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        let n = self.vertices.len();
        if n < 3 || distance.abs() < 1e-9 {
            return mesh;
        }

        let normal = self.normal();
        let displacement = normal * distance;

        // Base face vertices (original) and displaced cap face vertices
        let mut base_indices = Vec::with_capacity(n);
        let mut cap_indices = Vec::with_capacity(n);

        for &p in &self.vertices {
            let b_idx = mesh.positions.len() as u32;
            mesh.positions.push(p);
            mesh.normals.push(-normal);
            base_indices.push(b_idx);

            let c_idx = mesh.positions.len() as u32;
            mesh.positions.push(p + displacement);
            mesh.normals.push(normal);
            cap_indices.push(c_idx);
        }

        // Triangulate Base Face (reversed winding so normal points outwards)
        for i in 1..n - 1 {
            mesh.indices.push(base_indices[0]);
            mesh.indices.push(base_indices[i + 1]);
            mesh.indices.push(base_indices[i]);
        }

        // Triangulate Displaced Cap Face (standard counter-clockwise winding)
        for i in 1..n - 1 {
            mesh.indices.push(cap_indices[0]);
            mesh.indices.push(cap_indices[i]);
            mesh.indices.push(cap_indices[i + 1]);
        }

        // Generate Side Quad Faces
        for i in 0..n {
            let next = (i + 1) % n;
            let p0 = self.vertices[i];
            let p1 = self.vertices[next];
            let p2 = p1 + displacement;
            let p3 = p0 + displacement;

            let edge_u = p1 - p0;
            let edge_v = p3 - p0;
            let side_normal = edge_u.cross(edge_v).normalize();

            let base = mesh.positions.len() as u32;
            mesh.positions.extend_from_slice(&[p0, p1, p2, p3]);
            mesh.normals.extend_from_slice(&[side_normal, side_normal, side_normal, side_normal]);

            mesh.indices.extend_from_slice(&[
                base, base + 1, base + 2,
                base, base + 2, base + 3,
            ]);
        }

        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pull_cube_generation() {
        // Create a 1000x1000 square face on XY plane
        let face = PlanarFace3D::new(vec![
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(1000.0, 0.0, 0.0),
            Point3D::new(1000.0, 1000.0, 0.0),
            Point3D::new(0.0, 1000.0, 0.0),
        ]);

        assert_eq!(face.normal(), Vector3D::Z);

        // Pull face upwards by 1000mm to create a 3D Cube
        let cube_mesh = face.push_pull(1000.0);
        assert_eq!(cube_mesh.triangle_count(), 12); // 6 faces * 2 triangles = 12 triangles

        let bbox = cube_mesh.bbox().unwrap();
        assert_eq!(bbox.size(), Vector3D::new(1000.0, 1000.0, 1000.0));
    }
}
