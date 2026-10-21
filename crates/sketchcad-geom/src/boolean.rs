use crate::half_edge::TriangleMesh;
use serde::{Deserialize, Serialize};
use sketchcad_core::{BBox3D, Point3D};

/// Represents an opening void (e.g. door or window cavity) to be subtracted from a host element
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OpeningVoid {
    pub offset_along_wall: f64, // mm from start point
    pub width: f64,             // mm
    pub base_z: f64,            // mm (0 for doors, sill height for windows)
    pub height: f64,            // mm
}

/// CSG / Boolean Subtraction Engine
pub struct BooleanEngine;

impl BooleanEngine {
    /// Exact CSG Boolean Subtraction of a void box from a host box (A - B).
    /// Decomposes the difference into at most 6 non-overlapping disjoint bounding boxes.
    pub fn subtract_box(host: &BBox3D, void: &BBox3D) -> Vec<BBox3D> {
        // If there's no intersection, return the original host box
        let inter_min = host.min.max(void.min);
        let inter_max = host.max.min(void.max);

        if inter_min.x >= inter_max.x || inter_min.y >= inter_max.y || inter_min.z >= inter_max.z {
            return vec![*host];
        }

        let mut pieces = Vec::new();

        // 1. Bottom piece (below intersection in Z)
        if host.min.z < inter_min.z {
            pieces.push(BBox3D::new(
                host.min,
                Point3D::new(host.max.x, host.max.y, inter_min.z),
            ));
        }

        // 2. Top piece (above intersection in Z)
        if inter_max.z < host.max.z {
            pieces.push(BBox3D::new(
                Point3D::new(host.min.x, host.min.y, inter_max.z),
                host.max,
            ));
        }

        // In the middle Z slab [inter_min.z, inter_max.z]:
        // 3. Left piece (-X)
        if host.min.x < inter_min.x {
            pieces.push(BBox3D::new(
                Point3D::new(host.min.x, host.min.y, inter_min.z),
                Point3D::new(inter_min.x, host.max.y, inter_max.z),
            ));
        }

        // 4. Right piece (+X)
        if inter_max.x < host.max.x {
            pieces.push(BBox3D::new(
                Point3D::new(inter_max.x, host.min.y, inter_min.z),
                Point3D::new(host.max.x, host.max.y, inter_max.z),
            ));
        }

        // In the remaining central core [inter_min.x, inter_max.x] x [inter_min.z, inter_max.z]:
        // 5. Front piece (-Y)
        if host.min.y < inter_min.y {
            pieces.push(BBox3D::new(
                Point3D::new(inter_min.x, host.min.y, inter_min.z),
                Point3D::new(inter_max.x, inter_min.y, inter_max.z),
            ));
        }

        // 6. Back piece (+Y)
        if inter_max.y < host.max.y {
            pieces.push(BBox3D::new(
                Point3D::new(inter_min.x, inter_max.y, inter_min.z),
                Point3D::new(inter_max.x, host.max.y, inter_max.z),
            ));
        }

        pieces
    }

    /// Converts a list of disjoint bounding boxes into a combined triangle mesh
    pub fn boxes_to_mesh(boxes: &[BBox3D]) -> TriangleMesh {
        let mut mesh = TriangleMesh::new();
        for b in boxes {
            let size = b.size();
            let center = b.center();
            mesh.append(&TriangleMesh::create_box(size, center));
        }
        mesh
    }

    /// Decomposes a parametric wall with multiple hosted door/window openings
    /// into watertight disjoint solid pieces with exact void cutouts.
    pub fn cut_wall_openings(
        wall_length: f64,
        wall_thickness: f64,
        wall_height: f64,
        base_z: f64,
        mut openings: Vec<OpeningVoid>,
    ) -> Vec<BBox3D> {
        openings.sort_by(|a, b| {
            a.offset_along_wall
                .partial_cmp(&b.offset_along_wall)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let half_t = wall_thickness * 0.5;
        let mut solid_boxes = Vec::new();
        let mut current_offset = 0.0;

        for op in openings {
            let op_start = op.offset_along_wall.clamp(0.0, wall_length);
            let op_end = (op.offset_along_wall + op.width).clamp(0.0, wall_length);

            // 1. Solid wall segment before opening
            if op_start > current_offset {
                let _seg_len = op_start - current_offset;
                solid_boxes.push(BBox3D::new(
                    Point3D::new(current_offset, -half_t, base_z),
                    Point3D::new(op_start, half_t, base_z + wall_height),
                ));
            }

            // 2. Bottom sill solid (below window if sill height > 0)
            if op.base_z > 0.0 {
                solid_boxes.push(BBox3D::new(
                    Point3D::new(op_start, -half_t, base_z),
                    Point3D::new(op_end, half_t, base_z + op.base_z),
                ));
            }

            // 3. Top lintel solid (above door / window up to wall height)
            let op_top = op.base_z + op.height;
            if op_top < wall_height {
                solid_boxes.push(BBox3D::new(
                    Point3D::new(op_start, -half_t, base_z + op_top),
                    Point3D::new(op_end, half_t, base_z + wall_height),
                ));
            }

            current_offset = op_end;
        }

        // 4. Remaining solid segment after last opening
        if current_offset < wall_length {
            solid_boxes.push(BBox3D::new(
                Point3D::new(current_offset, -half_t, base_z),
                Point3D::new(wall_length, half_t, base_z + wall_height),
            ));
        }

        solid_boxes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csg_box_subtraction() {
        // Host cube 1000x1000x1000 centered at origin (-500 to +500)
        let host = BBox3D::new(
            Point3D::new(-500.0, -500.0, -500.0),
            Point3D::new(500.0, 500.0, 500.0),
        );
        // Void cutting through the upper half
        let void = BBox3D::new(
            Point3D::new(-200.0, -200.0, 0.0),
            Point3D::new(200.0, 200.0, 600.0),
        );

        let diff = BooleanEngine::subtract_box(&host, &void);
        assert!(!diff.is_empty());

        // Total volume of pieces must equal host volume minus intersection volume
        let host_vol = host.size().x * host.size().y * host.size().z;
        let inter_vol = 400.0 * 400.0 * 500.0; // 400 x 400 x 500
        let expected_vol = host_vol - inter_vol;

        let total_piece_vol: f64 = diff
            .iter()
            .map(|b| b.size().x * b.size().y * b.size().z)
            .sum();

        assert!((total_piece_vol - expected_vol).abs() < 1e-6);
    }

    #[test]
    fn test_wall_opening_cutouts() {
        // 6000mm wall with a 900mm wide door at offset 2000mm
        let openings = vec![OpeningVoid {
            offset_along_wall: 2000.0,
            width: 900.0,
            base_z: 0.0,
            height: 2100.0,
        }];

        let boxes = BooleanEngine::cut_wall_openings(6000.0, 200.0, 3000.0, 0.0, openings);
        // Expect 3 boxes: Left segment (2000mm), Lintel above door (900mm wide x 900mm tall), Right segment (3100mm)
        assert_eq!(boxes.len(), 3);
    }
}
