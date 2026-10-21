//! Floor-plan sheets (樓層平面圖): one drawing sheet per building storey.
//!
//! Native BIM elements contribute their parametric drafting symbols (door swings, stair arrows...)
//! to the sheet of the storey they stand on. Imported IFC geometry is drawn the way an
//! architectural plan is defined: a horizontal section at the storey cut height (thick cut lines)
//! plus the visible feature edges between the floor and the cut plane (thin projection lines).

use sketchcad_bim::{BimDocument, ElementKind, IfcComponent};
use sketchcad_core::{ElementId, Point2D, Point3D};
use sketchcad_geom::TriangleMesh;
use std::collections::HashSet;

/// Gap (mm) below which element footprints are considered part of the same building cluster.
pub const CLUSTER_MARGIN: f64 = 3000.0;
/// Plan cut plane height above the storey floor (mm).
const CUT_HEIGHT: f64 = 1200.0;
/// Geometry this far below the floor level is still shown in projection (mm).
const VIEW_DEPTH_TOLERANCE: f64 = 50.0;
/// Elements whose base is within this distance below a storey elevation belong to that storey.
const LEVEL_SNAP: f64 = 100.0;
const CREASE_ANGLE_DEG: f64 = 25.0;

/// Drafting line category, mapped to a line weight / color by the 2D viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// Cut by the plan plane (walls, columns): heaviest weight
    Cut,
    /// Parametric symbols: doors, windows, stairs, railings
    Symbol,
    /// Visible below the cut plane (slab edges, sills, imported geometry)
    Projection,
    Furniture,
    /// Ceiling-mounted fixtures
    Fixture,
    /// Above the cut plane (roofs, ceilings, beams): dashed
    Overhead,
    /// Room / space boundaries
    Boundary,
}

impl LineStyle {
    fn draw_rank(self) -> u8 {
        match self {
            LineStyle::Boundary => 0,
            LineStyle::Overhead => 1,
            LineStyle::Projection => 2,
            LineStyle::Fixture => 3,
            LineStyle::Furniture => 4,
            LineStyle::Symbol => 5,
            LineStyle::Cut => 6,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlanLine {
    pub a: Point2D,
    pub b: Point2D,
    pub style: LineStyle,
    /// Index into [`PlanSet::elem_ids`]
    pub elem: u32,
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub elevation: f64,
    /// Elevation shown on the sheet title (source-model datum)
    pub label_elevation: f64,
    pub cut_z: f64,
    pub lines: Vec<PlanLine>,
    /// Elements whose home storey is this sheet (used for tags and dimensions)
    pub members: HashSet<ElementId>,
}

#[derive(Debug, Default)]
pub struct PlanSet {
    pub sheets: Vec<Sheet>,
    pub elem_ids: Vec<ElementId>,
    /// Common drawing extent of all sheets (outliers trimmed), used for framing and tiling
    pub extent: Option<(Point2D, Point2D)>,
}

impl PlanSet {
    pub fn elem_index(&self, id: ElementId) -> Option<u32> {
        self.elem_ids.iter().position(|e| *e == id).map(|i| i as u32)
    }
}

/// Builds all floor-plan sheets of the document.
pub fn build_plan_set(doc: &BimDocument) -> PlanSet {
    struct Item {
        id: ElementId,
        kind: ElementKind,
        mesh: TriangleMesh,
        min_z: f64,
        max_z: f64,
        storey: Option<String>,
        imported: bool,
    }

    let mut items: Vec<Item> = Vec::new();
    for elem in doc.all_elements() {
        if !doc.is_visible(elem.id()) {
            continue;
        }
        let mesh = elem.to_3d_mesh();
        let (min_z, max_z) = mesh.bbox().map_or((0.0, 0.0), |b| (b.min.z, b.max.z));
        let ifc = elem.as_any().downcast_ref::<IfcComponent>();
        items.push(Item {
            id: elem.id(),
            kind: elem.kind(),
            imported: ifc.is_some_and(|c| !c.mesh.positions.is_empty()),
            storey: ifc.and_then(|c| c.storey_name.clone()),
            mesh,
            min_z,
            max_z,
        });
    }
    // Stable element order so the sheets do not reshuffle between rebuilds
    items.sort_by_key(|it| it.id.to_string());

    // 1. Sheet levels: document storeys, or a single level at the lowest floor
    let mut levels: Vec<(String, f64, f64)> = doc
        .storeys()
        .iter()
        .map(|s| (s.name.clone(), s.elevation, s.source_elevation.unwrap_or(s.elevation)))
        .collect();
    if levels.is_empty() {
        let floor = items
            .iter()
            .filter(|it| matches!(it.kind, ElementKind::Wall | ElementKind::Column | ElementKind::Slab))
            .map(|it| it.min_z)
            .fold(f64::INFINITY, f64::min);
        let floor = if floor.is_finite() { floor.min(0.0) } else { 0.0 };
        levels.push(("1F".to_string(), floor, floor));
    }
    let mut sheets: Vec<Sheet> = levels
        .iter()
        .enumerate()
        .map(|(i, (name, elevation, label_elevation))| {
            let cut_offset = match levels.get(i + 1) {
                Some((_, next, _)) => CUT_HEIGHT.min((next - elevation) * 0.5).max(1.0),
                None => CUT_HEIGHT,
            };
            Sheet {
                name: name.clone(),
                elevation: *elevation,
                label_elevation: *label_elevation,
                cut_z: elevation + cut_offset,
                lines: Vec::new(),
                members: HashSet::new(),
            }
        })
        .collect();

    // 2. Geometry per element
    let mut elem_ids = Vec::with_capacity(items.len());
    let mut elem_kinds = Vec::with_capacity(items.len());
    let mut projected_keys: Vec<HashSet<(i64, i64, i64, i64)>> = vec![HashSet::new(); sheets.len()];
    for item in &mut items {
        let elem = elem_ids.len() as u32;
        elem_ids.push(item.id);
        elem_kinds.push(item.kind);

        let home = item
            .storey
            .as_ref()
            .and_then(|name| sheets.iter().position(|s| &s.name == name))
            .unwrap_or_else(|| {
                sheets
                    .iter()
                    .rposition(|s| s.elevation <= item.min_z + LEVEL_SNAP)
                    .unwrap_or(0)
            });
        sheets[home].members.insert(item.id);

        if item.imported {
            if item.mesh.edge_flags.len() != item.mesh.triangle_count() {
                item.mesh.compute_crease_edges(CREASE_ANGLE_DEG.to_radians());
            }
            for (sheet, keys) in sheets.iter_mut().zip(&mut projected_keys) {
                section_mesh(&item.mesh, item.min_z, item.max_z, sheet, keys, elem);
            }
        } else if let Some(elem_ref) = doc.get_element(item.id) {
            let (style, take) = native_style(item.kind);
            for poly in elem_ref.to_2d_primitives().into_iter().take(take.unwrap_or(usize::MAX)) {
                let v = &poly.vertices;
                let count = if poly.closed && v.len() > 2 { v.len() } else { v.len().saturating_sub(1) };
                for i in 0..count {
                    sheets[home].lines.push(PlanLine { a: v[i], b: v[(i + 1) % v.len()], style, elem });
                }
            }
        }
    }

    // Draw order: light context lines first, cut lines on top
    for sheet in &mut sheets {
        sheet.lines.sort_by_key(|l| l.style.draw_rank());
    }

    // 3. Drop sheets with nothing on them (keep at least one), then compute the common extent
    if sheets.iter().any(|s| !s.lines.is_empty()) {
        sheets.retain(|s| !s.lines.is_empty());
    } else {
        sheets.truncate(1);
    }
    let extent = robust_extent(&sheets, &elem_kinds);

    PlanSet { sheets, elem_ids, extent }
}

/// Line style and the number of leading primitives used for native elements on a floor plan.
/// Reflected-ceiling details (ceiling grids, slab level crosses, roof slope arrows) are omitted.
fn native_style(kind: ElementKind) -> (LineStyle, Option<usize>) {
    match kind {
        ElementKind::Wall | ElementKind::Column | ElementKind::CurtainWall | ElementKind::Mullion => (LineStyle::Cut, None),
        ElementKind::Door
        | ElementKind::Window
        | ElementKind::Stairs
        | ElementKind::Railing
        | ElementKind::Ramp
        | ElementKind::CurtainGrid => (LineStyle::Symbol, None),
        ElementKind::Furniture => (LineStyle::Furniture, None),
        ElementKind::LightFixture => (LineStyle::Fixture, None),
        ElementKind::Slab => (LineStyle::Projection, Some(1)),
        ElementKind::Space => (LineStyle::Boundary, Some(1)),
        ElementKind::Ceiling => (LineStyle::Overhead, Some(1)),
        ElementKind::Roof => (LineStyle::Overhead, Some(2)),
        ElementKind::Beam => (LineStyle::Overhead, None),
    }
}

/// Adds the plan section (cut lines) and visible projection edges of one mesh to a sheet.
fn section_mesh(
    mesh: &TriangleMesh,
    min_z: f64,
    max_z: f64,
    sheet: &mut Sheet,
    projected_keys: &mut HashSet<(i64, i64, i64, i64)>,
    elem: u32,
) {
    // Offset avoids vertices lying exactly on the cut plane (degenerate double hits)
    let z_cut = sheet.cut_z + 0.37;
    let z_low = sheet.elevation - VIEW_DEPTH_TOLERANCE;
    let is_cut = min_z < z_cut && max_z > z_cut;
    let is_projected = max_z >= z_low && min_z <= z_cut;
    if !is_cut && !is_projected {
        return;
    }
    let flat = |p: Point3D| Point2D::new(p.x, p.y);

    for t in 0..mesh.triangle_count() {
        let idx = [mesh.indices[t * 3] as usize, mesh.indices[t * 3 + 1] as usize, mesh.indices[t * 3 + 2] as usize];
        if idx.iter().any(|&i| i >= mesh.positions.len()) {
            continue;
        }
        let v = idx.map(|i| mesh.positions[i]);

        if is_cut {
            let d = v.map(|p| p.z - z_cut);
            let mut hits = [Point2D::ZERO; 2];
            let mut n = 0;
            for e in 0..3 {
                let (i, j) = (e, (e + 1) % 3);
                if (d[i] < 0.0) != (d[j] < 0.0) && n < 2 {
                    let s = d[i] / (d[i] - d[j]);
                    hits[n] = flat(v[i] + (v[j] - v[i]) * s);
                    n += 1;
                }
            }
            if n == 2 && hits[0].distance(hits[1]) > 0.5 {
                sheet.lines.push(PlanLine { a: hits[0], b: hits[1], style: LineStyle::Cut, elem });
            }
        }

        if is_projected {
            let flags = mesh.edge_flags.get(t).copied().unwrap_or([true; 3]);
            for e in 0..3 {
                if !flags[e] {
                    continue;
                }
                let (p, q) = (v[e], v[(e + 1) % 3]);
                if p.z < z_low || q.z < z_low || p.z > z_cut || q.z > z_cut {
                    continue;
                }
                let (a, b) = (flat(p), flat(q));
                if a.distance(b) < 1.0 {
                    continue; // vertical edge seen end-on
                }
                let ka = (a.x.round() as i64, a.y.round() as i64);
                let kb = (b.x.round() as i64, b.y.round() as i64);
                let key = if ka <= kb { (ka.0, ka.1, kb.0, kb.1) } else { (kb.0, kb.1, ka.0, ka.1) };
                if projected_keys.insert(key) {
                    sheet.lines.push(PlanLine { a, b, style: LineStyle::Projection, elem });
                }
            }
        }
    }
}

/// Drawing extent of the building itself: site / landscape elements are ignored when structural
/// elements exist, and stray far-away elements are dropped by grouping element boxes into clusters
/// of touching boxes (with a margin) and keeping the cluster holding the most lines.
fn robust_extent(sheets: &[Sheet], kinds: &[ElementKind]) -> Option<(Point2D, Point2D)> {
    let structural = |elem: u32| {
        matches!(
            kinds[elem as usize],
            ElementKind::Wall
                | ElementKind::Column
                | ElementKind::Door
                | ElementKind::Window
                | ElementKind::Stairs
                | ElementKind::CurtainWall
        )
    };
    let lines = || sheets.iter().flat_map(|s| &s.lines);
    let use_all = !lines().any(|l| structural(l.elem));

    let mut boxes: std::collections::HashMap<u32, (Point2D, Point2D, usize)> = std::collections::HashMap::new();
    for line in lines().filter(|l| use_all || structural(l.elem)) {
        let entry = boxes.entry(line.elem).or_insert((line.a, line.a, 0));
        for p in [line.a, line.b] {
            entry.0 = entry.0.min(p);
            entry.1 = entry.1.max(p);
        }
        entry.2 += 1;
    }
    let boxes: Vec<(Point2D, Point2D, usize)> = boxes.into_values().collect();
    let members = main_cluster(&boxes, CLUSTER_MARGIN);
    let (min, max) = members.iter().fold(
        (Point2D::splat(f64::INFINITY), Point2D::splat(f64::NEG_INFINITY)),
        |(lo, hi), &i| (lo.min(boxes[i].0), hi.max(boxes[i].1)),
    );
    if members.is_empty() {
        return None;
    }
    let pad = ((max.x - min.x).max(max.y - min.y) * 0.04).max(300.0);
    Some((min - Point2D::splat(pad), max + Point2D::splat(pad)))
}

/// Indices of the largest group of touching boxes `(min, max, weight)`: boxes closer than
/// `margin` are grouped (union-find) and the group with the highest total weight wins.
/// Used to ignore stray elements placed far away from the building.
pub fn main_cluster(boxes: &[(Point2D, Point2D, usize)], margin: f64) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..boxes.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..boxes.len() {
        for j in (i + 1)..boxes.len() {
            let (a, b) = (&boxes[i], &boxes[j]);
            let touching = a.0.x - margin <= b.1.x
                && b.0.x - margin <= a.1.x
                && a.0.y - margin <= b.1.y
                && b.0.y - margin <= a.1.y;
            if touching {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut weight = vec![0usize; boxes.len()];
    for (i, b) in boxes.iter().enumerate() {
        let r = root(&mut parent, i);
        weight[r] += b.2;
    }
    let Some(best) = (0..boxes.len()).max_by_key(|&r| weight[r]) else { return Vec::new() };
    (0..boxes.len()).filter(|&i| root(&mut parent, i) == best).collect()
}
