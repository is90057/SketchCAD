use sketchcad_core::{Point2D, Point3D, Vector3D};

/// Ear-clipping triangulation of a simple polygon (convex or concave, either winding).
///
/// Returns triangles as indices into `points`, wound the same way as the input polygon.
/// Collinear and duplicate vertices are skipped instead of producing sliver triangles.
pub fn triangulate_polygon(points: &[Point2D]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }

    let mut area2 = 0.0;
    for i in 0..n {
        area2 += points[i].perp_dot(points[(i + 1) % n]);
    }
    if area2.abs() < 1e-12 {
        return Vec::new();
    }
    let ccw = area2 > 0.0;
    let mut ring: Vec<usize> = if ccw { (0..n).collect() } else { (0..n).rev().collect() };

    // Turn test relative to the polygon size so huge (survey) coordinates still work
    let eps = area2.abs() * 1e-10;
    let turn = |a: Point2D, b: Point2D, c: Point2D| (b - a).perp_dot(c - a);
    let inside = |p: Point2D, a: Point2D, b: Point2D, c: Point2D| {
        turn(a, b, p) >= -eps && turn(b, c, p) >= -eps && turn(c, a, p) >= -eps
    };

    let mut tris = Vec::with_capacity(n - 2);
    while ring.len() > 3 {
        let m = ring.len();
        let corner = |k: usize| (ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]);
        // Collinear or duplicate vertices are dropped without emitting a sliver triangle
        if let Some(k) = (0..m).find(|&k| {
            let (ia, ib, ic) = corner(k);
            turn(points[ia], points[ib], points[ic]).abs() <= eps
        }) {
            ring.remove(k);
            continue;
        }
        let mut progressed = false;
        for k in 0..m {
            let (ia, ib, ic) = corner(k);
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            if turn(a, b, c) < 0.0 {
                continue; // reflex corner
            }
            let blocked = ring.iter().any(|&j| {
                j != ia && j != ib && j != ic && {
                    let p = points[j];
                    p != a && p != b && p != c && inside(p, a, b, c)
                }
            });
            if !blocked {
                tris.push([ia, ib, ic]);
                ring.remove(k);
                progressed = true;
                break;
            }
        }
        if !progressed {
            // Self-intersecting input: clip the most convex corner to guarantee termination
            let k = (0..m)
                .max_by(|&x, &y| {
                    let tx = turn(points[ring[(x + m - 1) % m]], points[ring[x]], points[ring[(x + 1) % m]]);
                    let ty = turn(points[ring[(y + m - 1) % m]], points[ring[y]], points[ring[(y + 1) % m]]);
                    tx.total_cmp(&ty)
                })
                .unwrap_or(0);
            tris.push([ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]]);
            ring.remove(k);
        }
    }
    if ring.len() == 3 && turn(points[ring[0]], points[ring[1]], points[ring[2]]).abs() > eps {
        tris.push([ring[0], ring[1], ring[2]]);
    }

    if !ccw {
        for t in &mut tris {
            t.swap(1, 2);
        }
    }
    tris
}

/// Triangulates a planar 3D polygon (projected onto its dominant plane), preserving winding.
pub fn triangulate_polygon_3d(points: &[Point3D]) -> Vec<[usize; 3]> {
    // Newell normal is robust for concave and slightly non-planar loops
    let mut normal = Vector3D::ZERO;
    for i in 0..points.len() {
        let (p, q) = (points[i], points[(i + 1) % points.len()]);
        normal += Vector3D::new((p.y - q.y) * (p.z + q.z), (p.z - q.z) * (p.x + q.x), (p.x - q.x) * (p.y + q.y));
    }
    let abs = normal.abs();
    let projected: Vec<Point2D> = if abs.z >= abs.x && abs.z >= abs.y {
        points.iter().map(|p| Point2D::new(p.x, p.y)).collect()
    } else if abs.x >= abs.y {
        points.iter().map(|p| Point2D::new(p.y, p.z)).collect()
    } else {
        points.iter().map(|p| Point2D::new(p.z, p.x)).collect()
    };
    triangulate_polygon(&projected)
}

/// Whether polygon edge `a`-`b` is an outline edge (consecutive vertices) of an `n`-gon.
pub fn is_polygon_edge(a: usize, b: usize, n: usize) -> bool {
    (a + 1) % n == b || (b + 1) % n == a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[Point2D], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| (points[t[1]] - points[t[0]]).perp_dot(points[t[2]] - points[t[0]]) * 0.5)
            .sum()
    }

    #[test]
    fn concave_l_shape_has_no_overlap() {
        let pts = [
            Point2D::new(0.0, 0.0),
            Point2D::new(4.0, 0.0),
            Point2D::new(4.0, 1.0),
            Point2D::new(1.0, 1.0),
            Point2D::new(1.0, 4.0),
            Point2D::new(0.0, 4.0),
        ];
        let tris = triangulate_polygon(&pts);
        assert_eq!(tris.len(), 4);
        // Exact area and every triangle CCW: no fan triangles outside the polygon
        assert!((area(&pts, &tris) - 7.0).abs() < 1e-9);
        for t in &tris {
            assert!((pts[t[1]] - pts[t[0]]).perp_dot(pts[t[2]] - pts[t[0]]) > 0.0);
        }
    }

    #[test]
    fn clockwise_input_keeps_winding_and_skips_collinear() {
        let pts = [
            Point2D::new(0.0, 0.0),
            Point2D::new(0.0, 2.0),
            Point2D::new(2.0, 2.0),
            Point2D::new(2.0, 1.0), // collinear
            Point2D::new(2.0, 0.0),
        ];
        let tris = triangulate_polygon(&pts);
        assert_eq!(tris.len(), 2);
        assert!((area(&pts, &tris) + 4.0).abs() < 1e-9);
    }
}
