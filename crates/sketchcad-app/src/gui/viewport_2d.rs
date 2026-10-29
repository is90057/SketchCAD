use crate::gui::doc_watch::DocWatcher;
use crate::gui::plan_sheets::{build_plan_set, LineStyle, PlanSet};
use egui::{Color32, Pos2, Rect, Stroke, Ui};
use sketchcad_bim::{
    BimDocument, DoorSwing, ElementKind, FurnitureKind, LightType, Outlet, OutletVoltage, PlugStandard,
    SpaceType, Wall, WindowType,
};
use sketchcad_core::{ElementId, Point2D, Vector2D};
use sketchcad_render::Camera2D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Select,
    Move,
    Wall,
    Door,
    Window,
    Column,
    Slab,
    Ceiling,
    Stairs,
    Railing,
    Roof,
    Ramp,
    CurtainWall,
    PushPull,
    Space,
    Furniture,
    LightFixture,
    /// Paint bucket (油漆桶): click an element to apply the active material
    Paint,
    /// Power outlet (插座) mounted on a wall face
    Outlet,
    /// Place a model from the 3D model library (模型百寶箱)
    Model,
    /// 繪製: straight line segments (chained)
    DraftLine,
    /// 繪製: polyline of straight segments (聚合線)
    DraftPolyline,
    /// 繪製: circle by centre and radius point
    DraftCircle,
    /// 繪製: three-point arc
    DraftArc,
    /// 繪製: smooth curve through clicked points (多段線 / 雲形線)
    DraftSpline,
    /// 繪製: hatch fill of a closed region (填充線)
    DraftHatch,
    /// 修改: delete picked objects (E)
    Erase,
    /// 修改: copy with a base point, placing copies repeatedly (CO / CP)
    Copy,
    /// 修改: trim drafting to cutting edges (TR)
    Trim,
    /// 修改: extend drafting to boundary edges (EX)
    Extend,
    /// 修改: mirror across a two-point axis (MI)
    Mirror,
    /// 修改: parallel copy at a distance (O)
    Offset,
    /// 修改: rectangular / polar / path array (AR)
    Array,
    /// 修改: rotate about a base point (RO)
    Rotate,
    /// 修改: scale about a base point (SC)
    Scale,
    /// 修改: round corner between two lines / at a polyline vertex (F)
    Fillet,
    /// 修改: bevel corner (CHA)
    Chamfer,
    /// 註解: smart dimension — line → linear / aligned, circle → diameter, arc → radius (DIM)
    Dim,
    /// 註解: horizontal / vertical dimension (DIMLINEAR)
    DimLinear,
    /// 註解: dimension parallel to a slanted edge (DIMALIGNED)
    DimAligned,
    /// 註解: angle between two lines or of an arc (DIMANGULAR)
    DimAngular,
    /// 註解: radius of a circle / arc (DIMRADIUS)
    DimRadius,
    /// 註解: diameter of a circle / arc (DIMDIAMETER)
    DimDiameter,
    /// 註解: dimension a window of objects at once (QDIM)
    QDim,
    /// 註解: chain from the previous dimension (DIMCONTINUE)
    DimContinue,
    /// 註解: stack from the previous dimension's first point (DIMBASELINE)
    DimBaseline,
    /// 註解: multi-line text (MTEXT / MT)
    MText,
    /// 註解: leader with arrow and text (MLEADER)
    MLeader,
}

/// Layout of the dimensions QDIM creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QDimMode {
    Continuous,
    Baseline,
    Staggered,
}

impl QDimMode {
    pub const ALL: [Self; 3] = [Self::Continuous, Self::Baseline, Self::Staggered];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Continuous => "連續式",
            Self::Baseline => "基線式",
            Self::Staggered => "交錯式",
        }
    }
}

/// Settings of the 註解 tools.
#[derive(Debug, Clone)]
pub struct AnnotateSettings {
    pub text_height: f64,
    pub qdim_mode: QDimMode,
    /// Spacing between stacked baseline / staggered dimension lines (mm)
    pub stack_spacing: f64,
}

impl Default for AnnotateSettings {
    fn default() -> Self {
        Self { text_height: sketchcad_bim::Annotation::DEFAULT_TEXT_HEIGHT, qdim_mode: QDimMode::Continuous, stack_spacing: 600.0 }
    }
}

/// What a dimension tool has picked so far.
#[derive(Debug, Clone, Copy)]
enum DimSource {
    Segment(Point2D, Point2D),
    Circle { center: Point2D, radius: f64 },
    Arc { center: Point2D, radius: f64, start_deg: f64, sweep_deg: f64 },
    Line(Point2D, Point2D),
    Lines((Point2D, Point2D), (Point2D, Point2D)),
}

/// Text being typed for a new MTEXT / MLEADER.
struct TextEditor {
    geometry: sketchcad_bim::AnnotationGeometry,
    text: String,
    screen_pos: Pos2,
    focus: bool,
}

/// Array layout for the Array tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayMode {
    Rectangular,
    Polar,
    Path,
}

impl ArrayMode {
    pub const ALL: [Self; 3] = [Self::Rectangular, Self::Polar, Self::Path];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Rectangular => "矩形陣列",
            Self::Polar => "環形陣列",
            Self::Path => "路徑陣列",
        }
    }
}

/// Parameters of the 修改 tools, set from the ribbon.
#[derive(Debug, Clone)]
pub struct ModifySettings {
    pub offset_distance: f64,
    pub fillet_radius: f64,
    pub chamfer_d1: f64,
    pub chamfer_d2: f64,
    pub scale_factor: f64,
    pub mirror_erase_source: bool,
    pub array_mode: ArrayMode,
    pub array_rows: u32,
    pub array_cols: u32,
    pub array_row_spacing: f64,
    pub array_col_spacing: f64,
    pub array_count: u32,
    pub array_angle: f64,
}

impl Default for ModifySettings {
    fn default() -> Self {
        Self {
            offset_distance: 100.0,
            fillet_radius: 300.0,
            chamfer_d1: 200.0,
            chamfer_d2: 200.0,
            scale_factor: 2.0,
            mirror_erase_source: false,
            array_mode: ArrayMode::Rectangular,
            array_rows: 2,
            array_cols: 3,
            array_row_spacing: 1500.0,
            array_col_spacing: 1500.0,
            array_count: 6,
            array_angle: 360.0,
        }
    }
}

impl ToolMode {
    pub fn is_annotate(&self) -> bool {
        matches!(
            self,
            ToolMode::Dim
                | ToolMode::DimLinear
                | ToolMode::DimAligned
                | ToolMode::DimAngular
                | ToolMode::DimRadius
                | ToolMode::DimDiameter
                | ToolMode::QDim
                | ToolMode::DimContinue
                | ToolMode::DimBaseline
                | ToolMode::MText
                | ToolMode::MLeader
        )
    }

    pub fn is_modify(&self) -> bool {
        matches!(
            self,
            ToolMode::Erase
                | ToolMode::Copy
                | ToolMode::Trim
                | ToolMode::Extend
                | ToolMode::Mirror
                | ToolMode::Offset
                | ToolMode::Array
                | ToolMode::Rotate
                | ToolMode::Scale
                | ToolMode::Fillet
                | ToolMode::Chamfer
        )
    }

    pub fn is_drafting(&self) -> bool {
        matches!(
            self,
            ToolMode::DraftLine | ToolMode::DraftPolyline | ToolMode::DraftCircle | ToolMode::DraftArc | ToolMode::DraftSpline | ToolMode::DraftHatch
        )
    }
}

pub struct Viewport2D {
    pub camera: Camera2D,
    pub wall_start_pt: Option<Point2D>,
    pub slab_start_pt: Option<Point2D>,
    pub ceiling_start_pt: Option<Point2D>,
    pub stair_start_pt: Option<Point2D>,
    pub railing_start_pt: Option<Point2D>,
    pub roof_start_pt: Option<Point2D>,
    pub ramp_start_pt: Option<Point2D>,
    pub curtain_wall_start_pt: Option<Point2D>,
    pub space_start_pt: Option<Point2D>,
    pub furniture_cycle_kind: FurnitureKind,
    pub furniture_rotation: f64,
    pub light_cycle_type: LightType,
    /// Type given to windows placed with the Window tool
    pub window_type: WindowType,
    /// Spec given to outlets placed with the Outlet tool
    pub outlet_standard: PlugStandard,
    pub outlet_voltage: OutletVoltage,
    pub outlet_gangs: u8,
    /// Catalogue id of the model placed with the Model tool
    pub model_id: Option<String>,
    /// Points clicked so far by the active drafting tool
    pub draft_points: Vec<Point2D>,
    /// Tool the draft points belong to (switching tools drops them)
    draft_tool: ToolMode,
    /// Hatch settings for the Hatch tool
    pub hatch_pattern: sketchcad_bim::HatchPattern,
    pub hatch_spacing: f64,
    pub hatch_angle: f64,
    /// Parameters of the 修改 tools
    pub modify: ModifySettings,
    /// Object a two-step modify tool works on (offset source, first fillet line)
    modify_target: Option<(ElementId, Point2D)>,
    /// Settings of the 註解 tools
    pub annotate: AnnotateSettings,
    /// Object picked by a dimension tool
    dim_source: Option<DimSource>,
    /// Last linear dimension created (base of DIMCONTINUE / DIMBASELINE)
    last_dim: Option<ElementId>,
    /// Text window of MTEXT / MLEADER
    text_editor: Option<TextEditor>,
    pub move_base_pt: Option<Point2D>,
    pub snap_grid_size: f64, // mm, e.g. 100.0
    pub snap_enabled: bool,
    pub show_dimensions: bool, // 2D 尺寸標註與輔助線顯示/隱藏開關
    pub sheet_view: SheetView,
    plan: PlanSet,
    plan_watcher: DocWatcher,
    /// Re-frame on the next render, once the real viewport size is known
    frame_pending: bool,
    /// Hand tool: left-drag pans the drawing (✋ 平移)
    pub pan_mode: bool,
    /// Animated zoom in progress: (target zoom, screen pivot kept fixed)
    zoom_anim: Option<(f64, Pos2)>,
}

const MIN_ZOOM: f64 = 0.0005;
const MAX_ZOOM: f64 = 10.0;
/// Zoom factor per button press / key press / mouse-wheel notch
const ZOOM_STEP: f64 = 1.25;

/// Which floor-plan sheet(s) the 2D viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetView {
    /// All storey sheets tiled side by side (read-only overview)
    All,
    /// One sheet, drawn in model coordinates (editing enabled)
    Single(usize),
}

/// Click / finish events for the drafting tools in one frame.
#[derive(Clone, Copy)]
struct DraftInput {
    /// A single click (adds a point)
    clicked: bool,
    /// Double-click, right-click or Enter (ends an open polyline / spline / line chain)
    finish: bool,
}

/// Smallest closed region under `p` that a hatch can fill: closed polylines and circles,
/// room spaces and slabs.
pub fn hatch_region_at(doc: &BimDocument, p: Point2D) -> Option<Vec<Point2D>> {
    use sketchcad_bim::point_in_polygon;
    let mut regions: Vec<Vec<Point2D>> = Vec::new();
    for elem in doc.get_elements_by_kind(ElementKind::Draft) {
        if let Some(region) = doc.get_draft(elem.id()).and_then(|d| d.geometry.closed_region()) {
            regions.push(region);
        }
    }
    for elem in doc.get_elements_by_kind(ElementKind::Space) {
        if let Some(sp) = doc.get_space(elem.id()) {
            regions.push(sp.boundary.clone());
        }
    }
    for elem in doc.get_elements_by_kind(ElementKind::Slab) {
        if let Some(sl) = doc.get_slab(elem.id()) {
            let (o, w, d) = (sl.origin, sl.width, sl.depth);
            regions.push(vec![o, o + Vector2D::new(w, 0.0), o + Vector2D::new(w, d), o + Vector2D::new(0.0, d)]);
        }
    }
    let area = |poly: &Vec<Point2D>| sketchcad_geom::Polyline2D::new(poly.clone(), true).area();
    regions
        .into_iter()
        .filter(|r| r.len() >= 3 && point_in_polygon(p, r))
        .min_by(|a, b| area(a).total_cmp(&area(b)))
}

impl Viewport2D {
    /// Elevation of the level being drawn on: the open sheet, else the lowest one.
    fn draft_elevation(&self) -> f64 {
        match self.sheet_view {
            SheetView::Single(i) => self.plan.sheets.get(i).map_or(0.0, |s| s.elevation),
            SheetView::All => self.plan.sheets.first().map_or(0.0, |s| s.elevation),
        }
    }

    /// Preview and input handling of the 繪製 tools: line, polyline, circle, arc, spline
    /// and hatch.
    #[allow(clippy::too_many_arguments)]
    fn draft_tool_ui(
        &mut self,
        tool: ToolMode,
        input: DraftInput,
        cursor: Point2D,
        painter: &egui::Painter,
        to_screen: &impl Fn(Point2D) -> Pos2,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        selected_id: &mut Option<ElementId>,
        status_msg: &mut String,
    ) {
        use sketchcad_bim::DraftGeometry;
        let preview = Stroke::new(1.6, Color32::from_rgb(120, 220, 255));
        let guide = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 220, 255, 120));
        let draw_path = |pts: &[Point2D], closed: bool, stroke: Stroke| {
            let screen: Vec<Pos2> = pts.iter().map(|&p| to_screen(p)).collect();
            for w in screen.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            if closed && screen.len() > 2 {
                painter.line_segment([screen[screen.len() - 1], screen[0]], stroke);
            }
        };
        let label = |text: String| {
            painter.text(to_screen(cursor) + egui::vec2(14.0, -16.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::monospace(12.0), Color32::from_rgb(140, 225, 255));
        };
        for p in &self.draft_points {
            painter.circle_filled(to_screen(*p), 3.0, Color32::from_rgb(120, 220, 255));
        }
        let elevation = self.draft_elevation();
        let mut create = |geometry: DraftGeometry, doc: &mut BimDocument, status: &mut String| {
            push_undo(undo_stack, doc);
            let name = geometry.label();
            *selected_id = Some(doc.insert_draft(geometry, elevation));
            *status = format!("✔ 已繪製{name}");
        };
        // Clicking the first point again closes a polygon (within ~12 px)
        let near_first = |pts: &[Point2D]| pts.len() >= 3 && (to_screen(pts[0]) - to_screen(cursor)).length() < 12.0;

        match tool {
            ToolMode::DraftLine => {
                if let Some(&start) = self.draft_points.last() {
                    draw_path(&[start, cursor], false, preview);
                    let v = cursor - start;
                    label(format!("長度 {:.0} mm · 角度 {:.1}°", v.length(), v.y.atan2(v.x).to_degrees()));
                    if input.clicked && v.length() > 1.0 {
                        create(DraftGeometry::Line { start, end: cursor }, doc, status_msg);
                        self.draft_points = vec![cursor];
                    }
                } else if input.clicked {
                    self.draft_points.push(cursor);
                }
                if input.finish {
                    self.draft_points.clear();
                }
            }
            ToolMode::DraftPolyline | ToolMode::DraftSpline | ToolMode::DraftHatch => {
                let spline = tool == ToolMode::DraftSpline;
                let hatch = tool == ToolMode::DraftHatch;
                let mut pts = self.draft_points.clone();
                let closing = !spline && near_first(&pts);
                if !pts.is_empty() && !closing {
                    pts.push(cursor);
                }
                if spline {
                    draw_path(&sketchcad_bim::catmull_rom(&pts, 12), false, preview);
                    draw_path(&pts, false, guide);
                } else {
                    draw_path(&pts, closing || (hatch && pts.len() > 2), preview);
                }
                if closing {
                    painter.circle_stroke(to_screen(self.draft_points[0]), 8.0, Stroke::new(1.5, Color32::from_rgb(255, 215, 0)));
                }
                let hatch_settings = (self.hatch_pattern, self.hatch_spacing, self.hatch_angle);
                let make = |points: Vec<Point2D>, closed: bool| -> DraftGeometry {
                    if hatch {
                        DraftGeometry::Hatch { boundary: points, pattern: hatch_settings.0, spacing: hatch_settings.1, angle_deg: hatch_settings.2 }
                    } else if spline {
                        DraftGeometry::Spline { points }
                    } else {
                        DraftGeometry::Polyline { points, closed }
                    }
                };
                let min_points = if hatch { 3 } else { 2 };

                if hatch && self.draft_points.is_empty() {
                    // Pick mode: fill the closed region under the cursor
                    match hatch_region_at(doc, cursor) {
                        Some(region) => {
                            let lines = sketchcad_bim::hatch_lines(&region, hatch_settings.0, hatch_settings.1, hatch_settings.2);
                            for (a, b) in lines.iter().take(4000) {
                                painter.line_segment([to_screen(*a), to_screen(*b)], guide);
                            }
                            draw_path(&region, true, preview);
                            label("點擊填充此區域".into());
                            if input.clicked {
                                create(make(region, true), doc, status_msg);
                            }
                        }
                        None => {
                            label("點擊封閉區域內部填充，或點選頂點自訂邊界".into());
                            if input.clicked {
                                self.draft_points.push(cursor);
                            }
                        }
                    }
                    return;
                }

                if input.clicked {
                    if closing {
                        create(make(std::mem::take(&mut self.draft_points), true), doc, status_msg);
                        return;
                    }
                    if self.draft_points.last().is_none_or(|p| p.distance(cursor) > 1.0) {
                        self.draft_points.push(cursor);
                    }
                }
                if input.finish {
                    if self.draft_points.len() >= min_points {
                        create(make(std::mem::take(&mut self.draft_points), hatch), doc, status_msg);
                    } else {
                        self.draft_points.clear();
                    }
                } else if !self.draft_points.is_empty() {
                    let hint = if spline { "Enter / 右鍵 / 雙擊 完成曲線" } else if hatch { "點回起點或 Enter 完成填充邊界" } else { "點回起點封閉 · Enter / 右鍵 / 雙擊 完成" };
                    label(format!("{} 點 · {hint}", self.draft_points.len()));
                }
            }
            ToolMode::DraftCircle => {
                if let Some(&center) = self.draft_points.first() {
                    let radius = center.distance(cursor);
                    draw_path(&DraftGeometry::Circle { center, radius }.outline().0, true, preview);
                    draw_path(&[center, cursor], false, guide);
                    label(format!("半徑 {radius:.0} mm"));
                    if input.clicked && radius > 1.0 {
                        create(DraftGeometry::Circle { center, radius }, doc, status_msg);
                        self.draft_points.clear();
                    }
                } else if input.clicked {
                    self.draft_points.push(cursor);
                }
            }
            ToolMode::DraftArc => {
                match self.draft_points.as_slice() {
                    [] => {
                        label("點選弧的起點".into());
                        if input.clicked {
                            self.draft_points.push(cursor);
                        }
                    }
                    [start] => {
                        draw_path(&[*start, cursor], false, guide);
                        label("點選弧上第二點".into());
                        if input.clicked && start.distance(cursor) > 1.0 {
                            self.draft_points.push(cursor);
                        }
                    }
                    [start, mid, ..] => {
                        let (start, mid) = (*start, *mid);
                        match DraftGeometry::arc_through(start, mid, cursor) {
                            Some(arc) => {
                                draw_path(&arc.outline().0, false, preview);
                                if let DraftGeometry::Arc { radius, sweep_deg, .. } = arc {
                                    label(format!("半徑 {radius:.0} mm · 夾角 {:.1}° · 點選終點", sweep_deg.abs()));
                                }
                                if input.clicked {
                                    create(arc, doc, status_msg);
                                    self.draft_points.clear();
                                }
                            }
                            None => draw_path(&[start, mid, cursor], false, guide),
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Plan primitives of an element (empty when it does not exist).
fn element_prims(doc: &BimDocument, id: ElementId) -> Vec<sketchcad_geom::Polyline2D> {
    doc.get_element(id).map(|e| e.to_2d_primitives()).unwrap_or_default()
}

/// Centre of an element's plan drawing (reference point for path arrays).
fn element_anchor(doc: &BimDocument, id: ElementId) -> Option<Point2D> {
    let prims = element_prims(doc, id);
    let pts: Vec<Point2D> = prims.iter().flat_map(|p| p.vertices.iter().copied()).collect();
    sketchcad_core::BBox2D::from_points(&pts).map(|b| b.center())
}

/// `count` points evenly spaced along a drafting path (by length, both ends included).
fn path_points(geometry: &sketchcad_bim::DraftGeometry, count: usize) -> Vec<Point2D> {
    let (mut pts, closed) = geometry.outline();
    if closed && !pts.is_empty() {
        pts.push(pts[0]);
    }
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *cum.last().unwrap_or(&0.0);
    if pts.len() < 2 || total <= 0.0 || count == 0 {
        return Vec::new();
    }
    let steps = if closed { count } else { count.saturating_sub(1).max(1) };
    (0..count)
        .map(|k| {
            let s = total * k as f64 / steps as f64;
            let i = cum.partition_point(|&c| c <= s).clamp(1, pts.len() - 1);
            let t = if cum[i] - cum[i - 1] > 1e-12 { (s - cum[i - 1]) / (cum[i] - cum[i - 1]) } else { 0.0 };
            pts[i - 1] + (pts[i] - pts[i - 1]) * t
        })
        .collect()
}

impl Viewport2D {
    /// Preview and input handling of the 修改 tools.
    #[allow(clippy::too_many_arguments)]
    fn modify_tool_ui(
        &mut self,
        tool: ToolMode,
        input: DraftInput,
        shift: bool,
        cursor: Point2D,
        picked: Option<ElementId>,
        painter: &egui::Painter,
        to_screen: &impl Fn(Point2D) -> Pos2,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        selected_id: &mut Option<ElementId>,
        status_msg: &mut String,
    ) {
        use sketchcad_bim::{Corner, DraftGeometry, Transform2D};
        let ghost_stroke = Stroke::new(1.5, Color32::from_rgb(120, 230, 160));
        let pick_stroke = Stroke::new(2.4, Color32::from_rgb(255, 200, 90));
        let remove_stroke = Stroke::new(2.4, Color32::from_rgb(255, 90, 90));
        let draw_prims = |prims: &[sketchcad_geom::Polyline2D], map: &dyn Fn(Point2D) -> Point2D, stroke: Stroke| {
            for prim in prims {
                let pts: Vec<Pos2> = prim.vertices.iter().map(|&p| to_screen(map(p))).collect();
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], stroke);
                }
                if prim.closed && pts.len() > 2 {
                    painter.line_segment([pts[pts.len() - 1], pts[0]], stroke);
                }
            }
        };
        let draw_geometry = |g: &DraftGeometry, stroke: Stroke| {
            let (pts, closed) = g.outline();
            draw_prims(&[sketchcad_geom::Polyline2D::new(pts, closed)], &|p| p, stroke);
        };
        let label = |text: &str| {
            painter.text(to_screen(cursor) + egui::vec2(14.0, -16.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::monospace(12.0), Color32::from_rgb(150, 235, 180));
        };
        for p in &self.draft_points {
            painter.circle_filled(to_screen(*p), 3.5, Color32::from_rgb(255, 215, 0));
        }
        if input.finish {
            self.draft_points.clear();
            self.modify_target = None;
            return;
        }
        let settings = self.modify.clone();
        let target = selected_id.filter(|id| doc.get_element(*id).is_some());
        // Drafting entity under the cursor (hatches excluded for line edits)
        let picked_draft = picked.filter(|id| doc.get_draft(*id).is_some_and(|d| !matches!(d.geometry, DraftGeometry::Hatch { .. })));
        let identity = |p: Point2D| p;

        // Tools that act on the selection ask for an object first
        let needs_target = matches!(tool, ToolMode::Copy | ToolMode::Mirror | ToolMode::Rotate | ToolMode::Scale | ToolMode::Array);
        if needs_target && target.is_none() {
            if let Some(id) = picked {
                draw_prims(&element_prims(doc, id), &identity, pick_stroke);
            }
            label("請先點選要操作的物件");
            if input.clicked {
                *selected_id = picked;
            }
            return;
        }
        // Only the selection tools above use `target`; the others pick under the cursor
        let target_prims = target.map(|id| element_prims(doc, id)).unwrap_or_default();
        let target = target.unwrap_or_else(ElementId::new);

        match tool {
            ToolMode::Erase => {
                if let Some(id) = picked {
                    draw_prims(&element_prims(doc, id), &identity, remove_stroke);
                    if input.clicked {
                        push_undo(undo_stack, doc);
                        let name = doc.get_element(id).map(|e| e.name().to_string()).unwrap_or_default();
                        doc.remove_element(id);
                        if *selected_id == Some(id) {
                            *selected_id = None;
                        }
                        *status_msg = format!("✔ 已刪除 {name}");
                    }
                } else {
                    label("點選要刪除的物件");
                }
            }
            ToolMode::Copy => match self.draft_points.first().copied() {
                None => {
                    label("點選複製的基準點");
                    if input.clicked {
                        self.draft_points.push(cursor);
                    }
                }
                Some(base) => {
                    let delta = cursor - base;
                    draw_prims(&target_prims, &|p| p + delta, ghost_stroke);
                    painter.line_segment([to_screen(base), to_screen(cursor)], Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                    label(&format!("ΔX {:.0} · ΔY {:.0} mm · 連續點擊放置，Enter / 右鍵結束", delta.x, delta.y));
                    if input.clicked && delta.length() > 1e-6 {
                        if let Some(source) = doc.get_element(target).map(|e| e.clone_box()) {
                            push_undo(undo_stack, doc);
                            doc.paste_element(source.as_ref(), sketchcad_core::Vector3D::new(delta.x, delta.y, 0.0));
                            *status_msg = format!("✔ 已複製 (ΔX {:.0}, ΔY {:.0} mm)", delta.x, delta.y);
                        }
                    }
                }
            },
            ToolMode::Mirror | ToolMode::Rotate | ToolMode::Scale => {
                let xf = match (tool, self.draft_points.first().copied()) {
                    (ToolMode::Scale, _) => Some(Transform2D::scale(cursor, settings.scale_factor)),
                    (_, None) => None,
                    (ToolMode::Mirror, Some(a)) => (a.distance(cursor) > 1.0).then(|| Transform2D::mirror(a, cursor)),
                    (_, Some(base)) => {
                        let v = cursor - base;
                        let mut angle = v.y.atan2(v.x).to_degrees();
                        angle = if shift { (angle / 15.0).round() * 15.0 } else { angle.round() };
                        Some(Transform2D::rotation(base, angle))
                    }
                };
                match (tool, xf) {
                    (ToolMode::Scale, Some(xf)) => {
                        draw_prims(&target_prims, &|p| xf.point(p), ghost_stroke);
                        label(&format!("點擊基準點，以 {:.2} 倍縮放", settings.scale_factor));
                    }
                    (ToolMode::Mirror, None) => label("點選鏡射軸的第一點"),
                    (ToolMode::Mirror, Some(xf)) => {
                        let a = self.draft_points[0];
                        painter.extend(egui::Shape::dashed_line(&[to_screen(a), to_screen(cursor)], Stroke::new(1.2, Color32::from_rgb(255, 215, 0)), 8.0, 5.0));
                        draw_prims(&target_prims, &|p| xf.point(p), ghost_stroke);
                        label(if settings.mirror_erase_source { "點選鏡射軸第二點 (刪除原物件)" } else { "點選鏡射軸第二點" });
                    }
                    (_, None) => label("點選旋轉基準點"),
                    (_, Some(xf)) => {
                        draw_prims(&target_prims, &|p| xf.point(p), ghost_stroke);
                        let base = self.draft_points[0];
                        painter.line_segment([to_screen(base), to_screen(cursor)], Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                        label(&format!("旋轉 {:.0}° (Shift = 15° 間隔)", xf.x_angle_deg()));
                    }
                }
                if input.clicked {
                    match xf {
                        None => self.draft_points.push(cursor),
                        Some(xf) => {
                            push_undo(undo_stack, doc);
                            let keep_source = tool == ToolMode::Mirror && !settings.mirror_erase_source;
                            let subject = if keep_source {
                                doc.get_element(target).map(|e| e.clone_box()).map(|src| doc.paste_element(src.as_ref(), sketchcad_core::Vector3D::ZERO))
                            } else {
                                Some(target)
                            };
                            if let Some(subject) = subject {
                                match doc.transform_element(subject, &xf) {
                                    Ok(()) => {
                                        *status_msg = format!(
                                            "✔ 已{}",
                                            match tool {
                                                ToolMode::Mirror => "鏡射",
                                                ToolMode::Rotate => "旋轉",
                                                _ => "縮放",
                                            }
                                        );
                                        if keep_source {
                                            *selected_id = Some(subject);
                                        }
                                    }
                                    Err(e) => {
                                        undo_stack.pop();
                                        if keep_source {
                                            doc.remove_element(subject);
                                        }
                                        *status_msg = format!("✖ {e}");
                                    }
                                }
                            }
                            self.draft_points.clear();
                        }
                    }
                }
            }
            ToolMode::Array => {
                let mut copies: Vec<Transform2D> = Vec::new();
                let hint: String;
                match settings.array_mode {
                    ArrayMode::Rectangular => {
                        for r in 0..settings.array_rows.max(1) {
                            for c in 0..settings.array_cols.max(1) {
                                if (r, c) != (0, 0) {
                                    copies.push(Transform2D::translation(Vector2D::new(c as f64 * settings.array_col_spacing, r as f64 * settings.array_row_spacing)));
                                }
                            }
                        }
                        hint = format!("{} 列 × {} 欄 · 點擊建立", settings.array_rows, settings.array_cols);
                    }
                    ArrayMode::Polar => {
                        let n = settings.array_count.max(2);
                        let full = (settings.array_angle.abs() - 360.0).abs() < 1e-6;
                        let step = settings.array_angle / if full { n as f64 } else { (n - 1) as f64 };
                        for k in 1..n {
                            copies.push(Transform2D::rotation(cursor, step * k as f64));
                        }
                        painter.circle_filled(to_screen(cursor), 4.0, Color32::from_rgb(255, 215, 0));
                        hint = format!("點選環形陣列中心 · {n} 個 · {:.0}°", settings.array_angle);
                    }
                    ArrayMode::Path => match picked_draft.filter(|id| *id != target).zip(element_anchor(doc, target)) {
                        Some((path, anchor)) => {
                            let path_geom = doc.get_draft(path).map(|d| d.geometry.clone());
                            for p in path_geom.map(|g| path_points(&g, settings.array_count.max(1) as usize)).unwrap_or_default() {
                                copies.push(Transform2D::translation(p - anchor));
                            }
                            hint = format!("沿此路徑排列 {} 個 · 點擊建立", settings.array_count);
                        }
                        None => hint = "點選作為路徑的線條 (直線 / 聚合線 / 弧 / 圓 / 雲形線)".into(),
                    },
                }
                copies.truncate(500);
                for xf in &copies {
                    draw_prims(&target_prims, &|p| xf.point(p), ghost_stroke);
                }
                label(&hint);
                if input.clicked && !copies.is_empty() {
                    push_undo(undo_stack, doc);
                    let mut made = 0;
                    let mut failure = None;
                    for xf in &copies {
                        let Some(source) = doc.get_element(target).map(|e| e.clone_box()) else { break };
                        let copy = doc.paste_element(source.as_ref(), sketchcad_core::Vector3D::ZERO);
                        let result = if xf.x == Vector2D::X && xf.y == Vector2D::Y {
                            doc.move_element(copy, sketchcad_core::Vector3D::new(xf.t.x, xf.t.y, 0.0));
                            Ok(())
                        } else {
                            doc.transform_element(copy, xf)
                        };
                        match result {
                            Ok(()) => made += 1,
                            Err(e) => {
                                doc.remove_element(copy);
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                    *status_msg = match failure {
                        Some(e) => format!("✖ {e}"),
                        None => format!("✔ 已建立{} ({made} 個複本)", settings.array_mode.label()),
                    };
                }
            }
            ToolMode::Offset => match self.modify_target {
                None => match picked_draft {
                    Some(id) => {
                        draw_prims(&element_prims(doc, id), &identity, pick_stroke);
                        label(&format!("點選要偏移的物件 (距離 {:.0} mm)", settings.offset_distance));
                        if input.clicked {
                            self.modify_target = Some((id, cursor));
                        }
                    }
                    None => label(&format!("點選要偏移的線條 (距離 {:.0} mm)", settings.offset_distance)),
                },
                Some((id, _)) => {
                    let result = doc.get_draft(id).and_then(|d| d.geometry.offset(settings.offset_distance, cursor));
                    draw_prims(&element_prims(doc, id), &identity, pick_stroke);
                    match result {
                        Some(g) => {
                            draw_geometry(&g, ghost_stroke);
                            label("點選偏移的一側");
                            if input.clicked {
                                push_undo(undo_stack, doc);
                                *selected_id = doc.add_draft_like(id, g);
                                *status_msg = format!("✔ 已偏移 {:.0} mm", settings.offset_distance);
                                self.modify_target = None;
                            }
                        }
                        None => label("此側無法偏移 (距離大於半徑？)"),
                    }
                }
            },
            ToolMode::Trim | ToolMode::Extend => match picked_draft {
                Some(id) => {
                    let edges = doc.cutting_edges(id);
                    let geometry = doc.get_draft(id).map(|d| d.geometry.clone());
                    let pieces = geometry.as_ref().and_then(|g| if tool == ToolMode::Trim { g.trim(&edges, cursor) } else { g.extend(&edges, cursor).map(|g| vec![g]) });
                    match pieces {
                        Some(pieces) => {
                            draw_prims(&element_prims(doc, id), &identity, if tool == ToolMode::Trim { remove_stroke } else { pick_stroke });
                            for g in &pieces {
                                draw_geometry(g, Stroke::new(2.4, Color32::from_rgb(120, 230, 160)));
                            }
                            label(if tool == ToolMode::Trim { "點擊修剪紅色部分" } else { "點擊延伸到邊界" });
                            if input.clicked {
                                push_undo(undo_stack, doc);
                                let ids = doc.replace_draft(id, pieces);
                                *selected_id = ids.first().copied();
                                *status_msg = if tool == ToolMode::Trim { "✔ 已修剪".into() } else { "✔ 已延伸".into() };
                            }
                        }
                        None => label(if tool == ToolMode::Trim { "此處沒有可修剪的交點" } else { "此方向沒有可延伸到的邊界" }),
                    }
                }
                None => label(if tool == ToolMode::Trim { "點選要剪掉的線段部分" } else { "點選靠近要延伸那一端的線條" }),
            },
            ToolMode::Fillet | ToolMode::Chamfer => {
                let corner = if tool == ToolMode::Fillet { Corner::Fillet(settings.fillet_radius) } else { Corner::Chamfer(settings.chamfer_d1, settings.chamfer_d2) };
                let line_of = |id: ElementId| match doc.get_draft(id).map(|d| &d.geometry) {
                    Some(DraftGeometry::Line { start, end }) => Some((*start, *end)),
                    _ => None,
                };
                match self.modify_target {
                    None => match picked_draft.map(|id| (id, doc.get_draft(id).map(|d| d.geometry.clone()))) {
                        Some((id, Some(DraftGeometry::Polyline { points, closed }))) => {
                            match sketchcad_bim::corner_polyline(&points, closed, cursor, corner) {
                                Some(new_points) => {
                                    let g = DraftGeometry::Polyline { points: new_points, closed };
                                    draw_geometry(&g, ghost_stroke);
                                    label("點擊處理此聚合線轉角");
                                    if input.clicked {
                                        push_undo(undo_stack, doc);
                                        doc.replace_draft(id, vec![g]);
                                        *status_msg = "✔ 已處理轉角".into();
                                    }
                                }
                                None => label("轉角太短，無法套用此尺寸"),
                            }
                        }
                        Some((id, Some(DraftGeometry::Line { .. }))) => {
                            draw_prims(&element_prims(doc, id), &identity, pick_stroke);
                            label("點選第一條直線");
                            if input.clicked {
                                self.modify_target = Some((id, cursor));
                            }
                        }
                        _ => label("點選第一條直線，或聚合線的轉角"),
                    },
                    Some((first, pick1)) => {
                        draw_prims(&element_prims(doc, first), &identity, pick_stroke);
                        let second = picked_draft.filter(|id| *id != first);
                        match second.and_then(|id| Some((id, line_of(first)?, line_of(id)?))) {
                            Some((id2, l1, l2)) => match sketchcad_bim::corner_lines(l1, pick1, l2, cursor, corner) {
                                Some((a, b, extra)) => {
                                    for g in [Some(&a), Some(&b), extra.as_ref()].into_iter().flatten() {
                                        draw_geometry(g, ghost_stroke);
                                    }
                                    label("點擊完成");
                                    if input.clicked {
                                        push_undo(undo_stack, doc);
                                        doc.replace_draft(first, vec![a]);
                                        doc.replace_draft(id2, vec![b]);
                                        if let Some(g) = extra {
                                            doc.add_draft_like(first, g);
                                        }
                                        *status_msg = format!("✔ 已{}", if tool == ToolMode::Fillet { "圓角" } else { "導角" });
                                        self.modify_target = None;
                                    }
                                }
                                None => label("平行線或尺寸過大，無法處理"),
                            },
                            None => label("點選第二條直線"),
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Draws `a→b` with a dash pattern (alternating dash / gap lengths in pixels), starting
/// `phase` pixels into the pattern so consecutive segments of a curve continue it.
fn patterned_segment(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke, pattern: &[f32], phase: f32) {
    let length = (b - a).length();
    let period: f32 = pattern.iter().sum();
    if length < 1e-3 || period <= 0.0 {
        return;
    }
    let dir = (b - a) / length;
    // Locate the phase inside the pattern
    let mut p = phase.rem_euclid(period);
    let mut k = 0;
    while p >= pattern[k] {
        p -= pattern[k];
        k = (k + 1) % pattern.len();
    }
    let mut s = 0.0;
    let mut remaining = pattern[k] - p;
    while s < length {
        let e = (s + remaining).min(length);
        if k % 2 == 0 {
            painter.line_segment([a + dir * s, a + dir * e], stroke);
        }
        s = e;
        k = (k + 1) % pattern.len();
        remaining = pattern[k];
    }
}

/// Default colour of dimensions, text and leaders.
pub const ANNOTATION_COLOR: Color32 = Color32::from_rgb(140, 220, 255);

/// Draws an annotation's texts; `px_per_mm` converts the text height to screen pixels.
pub fn draw_annotation_texts(painter: &egui::Painter, a: &sketchcad_bim::Annotation, to_screen: &dyn Fn(Point2D) -> Pos2, px_per_mm: f32, color: Color32) {
    for t in a.texts() {
        // Cap height ≈ 0.72 of the font size
        let size = t.height as f32 * px_per_mm / 0.72;
        if !(3.0..=400.0).contains(&size) {
            continue;
        }
        let galley = painter.layout(t.text.clone(), egui::FontId::proportional(size), color, f32::INFINITY);
        let angle = -(t.angle_deg as f32).to_radians();
        let (s, c) = angle.sin_cos();
        let rotate = |v: egui::Vec2| egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c);
        let anchor = to_screen(t.position);
        let top_left = if t.centered { anchor - rotate(galley.size() * 0.5) } else { anchor };
        painter.add(egui::epaint::TextShape::new(top_left, galley, color).with_angle(angle));
    }
}

/// Characteristic points of the objects inside a window (QDIM): line end points, polyline
/// vertices, circle / arc centres, wall end points and column centres.
fn qdim_points(doc: &BimDocument, min: Point2D, max: Point2D) -> Vec<Point2D> {
    use sketchcad_bim::DraftGeometry;
    let inside = |p: &Point2D| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
    let mut pts = Vec::new();
    for elem in doc.all_elements() {
        let id = elem.id();
        if let Some(d) = doc.get_draft(id) {
            match &d.geometry {
                DraftGeometry::Line { start, end } => pts.extend([*start, *end]),
                DraftGeometry::Polyline { points, .. } => pts.extend(points.iter().copied()),
                DraftGeometry::Circle { center, .. } | DraftGeometry::Arc { center, .. } => pts.push(*center),
                DraftGeometry::Spline { points } => pts.extend([points[0], points[points.len() - 1]]),
                DraftGeometry::Hatch { .. } => {}
            }
        } else if let Some(w) = doc.get_wall(id) {
            pts.extend([w.start, w.end]);
        } else if let Some(c) = doc.get_column(id) {
            pts.push(c.position);
        }
    }
    pts.retain(inside);
    pts
}

/// Rays (from `x`, along ±`u1` / ±`u2`) whose angle contains `toward`: `(start_deg, sweep_deg)`.
fn angle_between_rays(u1: Vector2D, u2: Vector2D, toward: Vector2D, both_ways: bool) -> (f64, f64) {
    let deg = |v: Vector2D| v.y.atan2(v.x).to_degrees();
    let target = deg(toward);
    let signs: &[f64] = if both_ways { &[1.0, -1.0] } else { &[1.0] };
    let mut best = (deg(u1), (deg(u2) - deg(u1)).rem_euclid(360.0));
    for &s1 in signs {
        for &s2 in signs {
            for (a, b) in [(u1 * s1, u2 * s2), (u2 * s2, u1 * s1)] {
                let (start, sweep) = (deg(a), (deg(b) - deg(a)).rem_euclid(360.0));
                let inside = (target - start).rem_euclid(360.0) <= sweep;
                if inside && (both_ways && sweep < 180.0 + 1e-9 || !both_ways) {
                    best = (start, sweep);
                    return best;
                }
            }
        }
    }
    best
}

impl Viewport2D {
    /// Elevation of the level being annotated (same as drafting).
    fn annotate_elevation(&self) -> f64 {
        self.draft_elevation()
    }

    /// Linear dimension of `p1`→`p2` whose dimension line passes through `cursor`.
    fn linear_dim(p1: Point2D, p2: Point2D, axis: sketchcad_bim::DimAxis, cursor: Point2D) -> sketchcad_bim::AnnotationGeometry {
        let d = sketchcad_bim::AnnotationGeometry::linear_direction(p1, p2, axis);
        sketchcad_bim::AnnotationGeometry::Linear { p1, p2, axis, offset: (cursor - p1).dot(d.perp()) }
    }

    /// DIMLINEAR orientation (AutoCAD rule): measure horizontally when the cursor is above /
    /// below the two points, vertically when it is beside them.
    fn auto_axis(p1: Point2D, p2: Point2D, cursor: Point2D) -> sketchcad_bim::DimAxis {
        let (min, max) = (p1.min(p2), p1.max(p2));
        let out_x = (min.x - cursor.x).max(cursor.x - max.x).max(0.0);
        let out_y = (min.y - cursor.y).max(cursor.y - max.y).max(0.0);
        if out_y >= out_x { sketchcad_bim::DimAxis::Horizontal } else { sketchcad_bim::DimAxis::Vertical }
    }

    /// Previous linear dimension for DIMCONTINUE / DIMBASELINE: the selection if it is one,
    /// else the last one created.
    fn base_dim(&self, doc: &BimDocument, selected: Option<ElementId>) -> Option<(Point2D, Point2D, sketchcad_bim::DimAxis, f64)> {
        [selected, self.last_dim].into_iter().flatten().find_map(|id| match doc.get_annotation(id).map(|a| &a.geometry) {
            Some(sketchcad_bim::AnnotationGeometry::Linear { p1, p2, axis, offset }) => Some((*p1, *p2, *axis, *offset)),
            _ => None,
        })
    }

    /// Preview and input handling of the 註解 tools.
    #[allow(clippy::too_many_arguments)]
    fn annotate_tool_ui(
        &mut self,
        tool: ToolMode,
        input: DraftInput,
        cursor: Point2D,
        hover_screen: Pos2,
        picked: Option<ElementId>,
        painter: &egui::Painter,
        to_screen: &impl Fn(Point2D) -> Pos2,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        selected_id: &mut Option<ElementId>,
        status_msg: &mut String,
    ) {
        use sketchcad_bim::{Annotation, AnnotationGeometry, DimAxis, DraftGeometry};
        let h = self.annotate.text_height;
        let elevation = self.annotate_elevation();
        let ghost = Color32::from_rgb(120, 230, 160);
        let zoom = self.camera.zoom as f32;
        let preview = |g: &AnnotationGeometry| {
            let ann = Annotation::new(g.clone(), elevation, h);
            for prim in sketchcad_bim::BimElement::to_2d_primitives(&ann) {
                let pts: Vec<Pos2> = prim.vertices.iter().map(|&p| to_screen(p)).collect();
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(1.3, ghost));
                }
                if prim.closed && pts.len() > 2 {
                    painter.line_segment([pts[pts.len() - 1], pts[0]], Stroke::new(1.3, ghost));
                }
            }
            draw_annotation_texts(painter, &ann, to_screen, zoom, ghost);
        };
        let label = |text: &str| {
            painter.text(to_screen(cursor) + egui::vec2(14.0, -16.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::monospace(12.0), Color32::from_rgb(150, 235, 180));
        };
        for p in &self.draft_points {
            painter.circle_filled(to_screen(*p), 3.5, Color32::from_rgb(255, 215, 0));
        }
        if input.finish {
            self.draft_points.clear();
            self.dim_source = None;
            return;
        }
        let mut created: Vec<AnnotationGeometry> = Vec::new();
        // Object under the cursor as a dimension source
        let source_of = |id: ElementId| -> Option<DimSource> {
            if let Some(d) = doc.get_draft(id) {
                return match d.geometry {
                    DraftGeometry::Line { start, end } => Some(DimSource::Segment(start, end)),
                    DraftGeometry::Circle { center, radius } => Some(DimSource::Circle { center, radius }),
                    DraftGeometry::Arc { center, radius, start_deg, sweep_deg } => Some(DimSource::Arc { center, radius, start_deg, sweep_deg }),
                    _ => None,
                };
            }
            doc.get_wall(id).map(|w| DimSource::Segment(w.start, w.end))
        };
        let hovered = picked.and_then(source_of);

        match tool {
            ToolMode::Dim | ToolMode::DimLinear | ToolMode::DimAligned => {
                let aligned = tool == ToolMode::DimAligned;
                // DIM: lines, circles and arcs under the cursor are dimensioned directly
                if tool == ToolMode::Dim && self.draft_points.is_empty() && self.dim_source.is_none() {
                    let pending = match hovered {
                        Some(DimSource::Segment(a, b)) => {
                            let axis = if (b - a).x.abs() < 1e-6 { DimAxis::Vertical } else if (b - a).y.abs() < 1e-6 { DimAxis::Horizontal } else { DimAxis::Aligned };
                            label("點擊此線條標註長度；或點選兩點");
                            Some((DimSource::Segment(a, b), Self::linear_dim(a, b, axis, cursor + (b - a).perp().normalize_or_zero() * h * 2.0)))
                        }
                        Some(DimSource::Circle { center, radius }) => {
                            label("點擊此圓標註直徑");
                            Some((DimSource::Circle { center, radius }, AnnotationGeometry::Diameter { center, radius, angle_deg: 45.0 }))
                        }
                        Some(src @ DimSource::Arc { center, radius, .. }) => {
                            label("點擊此弧標註半徑");
                            Some((src, AnnotationGeometry::Radius { center, radius, angle_deg: 45.0 }))
                        }
                        _ => None,
                    };
                    if let Some((src, g)) = pending {
                        preview(&g);
                        if input.clicked {
                            self.dim_source = Some(src);
                        }
                        return;
                    }
                }
                if let Some(src) = self.dim_source {
                    let g = match src {
                        DimSource::Segment(a, b) => {
                            let axis = if (b - a).x.abs() < 1e-6 || (b - a).y.abs() < 1e-6 { Self::auto_axis(a, b, cursor) } else { DimAxis::Aligned };
                            Self::linear_dim(a, b, axis, cursor)
                        }
                        DimSource::Circle { center, radius } => {
                            let v = cursor - center;
                            AnnotationGeometry::Diameter { center, radius, angle_deg: v.y.atan2(v.x).to_degrees() }
                        }
                        DimSource::Arc { center, radius, .. } => {
                            let v = cursor - center;
                            AnnotationGeometry::Radius { center, radius, angle_deg: v.y.atan2(v.x).to_degrees() }
                        }
                        _ => return,
                    };
                    preview(&g);
                    label("點選標註位置");
                    if input.clicked {
                        created.push(g);
                        self.dim_source = None;
                    }
                } else {
                    match self.draft_points.as_slice() {
                        [] => {
                            label(if aligned { "點選第一個延伸線原點 (斜邊起點)" } else { "點選第一個延伸線原點" });
                            if input.clicked {
                                self.draft_points.push(cursor);
                            }
                        }
                        [p1] => {
                            painter.line_segment([to_screen(*p1), to_screen(cursor)], Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                            label("點選第二個延伸線原點");
                            if input.clicked && p1.distance(cursor) > 1.0 {
                                self.draft_points.push(cursor);
                            }
                        }
                        [p1, p2, ..] => {
                            let axis = if aligned { DimAxis::Aligned } else { Self::auto_axis(*p1, *p2, cursor) };
                            let g = Self::linear_dim(*p1, *p2, axis, cursor);
                            preview(&g);
                            label("點選標註線位置");
                            if input.clicked {
                                created.push(g);
                                self.draft_points.clear();
                            }
                        }
                    }
                }
            }
            ToolMode::DimAngular => match self.dim_source {
                None => match (hovered, self.draft_points.as_slice()) {
                    (Some(DimSource::Segment(a, b)), []) => {
                        label("點選第一條直線 (或點選弧)");
                        if input.clicked {
                            self.dim_source = Some(DimSource::Line(a, b));
                        }
                    }
                    (Some(src @ DimSource::Arc { .. }), []) => {
                        label("點選弧標註圓心角");
                        if input.clicked {
                            self.dim_source = Some(src);
                        }
                    }
                    (_, []) => {
                        label("點選第一條直線、弧，或點選角的頂點");
                        if input.clicked {
                            self.draft_points.push(cursor);
                        }
                    }
                    (_, [_v]) => {
                        label("點選角的第一個端點");
                        if input.clicked {
                            self.draft_points.push(cursor);
                        }
                    }
                    (_, [v, a]) => {
                        label("點選角的第二個端點");
                        painter.line_segment([to_screen(*v), to_screen(*a)], Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                        if input.clicked {
                            self.draft_points.push(cursor);
                        }
                    }
                    (_, [v, a, b, ..]) => {
                        let (start, sweep) = angle_between_rays(*a - *v, *b - *v, cursor - *v, false);
                        let g = AnnotationGeometry::Angular { center: *v, start_deg: start, sweep_deg: sweep, radius: v.distance(cursor) };
                        preview(&g);
                        label("點選標註弧位置");
                        if input.clicked {
                            created.push(g);
                            self.draft_points.clear();
                        }
                    }
                },
                Some(DimSource::Line(a1, b1)) => match hovered {
                    Some(DimSource::Segment(a2, b2)) if (a2, b2) != (a1, b1) => {
                        label("點選第二條直線");
                        if input.clicked {
                            self.dim_source = Some(DimSource::Lines((a1, b1), (a2, b2)));
                        }
                    }
                    _ => label("點選第二條直線"),
                },
                Some(DimSource::Lines((a1, b1), (a2, b2))) => {
                    let (u, v) = (b1 - a1, b2 - a2);
                    let denom = u.perp_dot(v);
                    if denom.abs() < 1e-9 {
                        label("兩線平行，無法標註角度");
                        if input.clicked {
                            self.dim_source = None;
                        }
                        return;
                    }
                    let x = a1 + u * ((a2 - a1).perp_dot(v) / denom);
                    let (start, sweep) = angle_between_rays(u, v, cursor - x, true);
                    let g = AnnotationGeometry::Angular { center: x, start_deg: start, sweep_deg: sweep, radius: x.distance(cursor) };
                    preview(&g);
                    label("點選標註弧位置 (決定量測哪一側的角)");
                    if input.clicked {
                        created.push(g);
                        self.dim_source = None;
                    }
                }
                Some(DimSource::Arc { center, start_deg, sweep_deg, .. }) => {
                    let (start, sweep) = if sweep_deg < 0.0 { (start_deg + sweep_deg, -sweep_deg) } else { (start_deg, sweep_deg) };
                    let g = AnnotationGeometry::Angular { center, start_deg: start, sweep_deg: sweep, radius: center.distance(cursor) };
                    preview(&g);
                    label("點選標註弧位置");
                    if input.clicked {
                        created.push(g);
                        self.dim_source = None;
                    }
                }
                _ => self.dim_source = None,
            },
            ToolMode::DimRadius | ToolMode::DimDiameter => {
                let radius_mode = tool == ToolMode::DimRadius;
                match self.dim_source {
                    Some(DimSource::Circle { center, radius }) | Some(DimSource::Arc { center, radius, .. }) => {
                        let v = cursor - center;
                        let angle_deg = v.y.atan2(v.x).to_degrees();
                        let g = if radius_mode { AnnotationGeometry::Radius { center, radius, angle_deg } } else { AnnotationGeometry::Diameter { center, radius, angle_deg } };
                        preview(&g);
                        label("點選標註方向");
                        if input.clicked {
                            created.push(g);
                            self.dim_source = None;
                        }
                    }
                    _ => match hovered {
                        Some(src @ (DimSource::Circle { .. } | DimSource::Arc { .. })) => {
                            label(if radius_mode { "點選此圓 / 弧標註半徑" } else { "點選此圓 / 弧標註直徑" });
                            if input.clicked {
                                self.dim_source = Some(src);
                            }
                        }
                        _ => label("點選圓或弧"),
                    },
                }
            }
            ToolMode::QDim => match self.draft_points.as_slice() {
                [] => {
                    label("框選物件：點選窗框第一角");
                    if input.clicked {
                        self.draft_points.push(cursor);
                    }
                }
                [a] => {
                    let r = Rect::from_two_pos(to_screen(*a), to_screen(cursor));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.2, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Middle);
                    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(80, 150, 255, 30));
                    label("點選窗框對角");
                    if input.clicked && a.distance(cursor) > 1.0 {
                        self.draft_points.push(cursor);
                    }
                }
                [a, b, ..] => {
                    let (min, max) = (a.min(*b), a.max(*b));
                    painter.rect_stroke(Rect::from_two_pos(to_screen(min), to_screen(max)), 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 200, 255, 120)), egui::StrokeKind::Middle);
                    // Horizontal chain when the cursor is above / below the window, else vertical
                    let horizontal = cursor.y > max.y || cursor.y < min.y || (cursor.x >= min.x && cursor.x <= max.x);
                    let axis = if horizontal { DimAxis::Horizontal } else { DimAxis::Vertical };
                    let key = |p: &Point2D| if horizontal { p.x } else { p.y };
                    let mut pts = qdim_points(doc, min, max);
                    pts.sort_by(|p, q| key(p).total_cmp(&key(q)));
                    pts.dedup_by(|p, q| (key(p) - key(q)).abs() < 1.0);
                    let line = if horizontal { cursor.y } else { cursor.x };
                    let spacing = self.annotate.stack_spacing * if (horizontal && cursor.y < min.y) || (!horizontal && cursor.x < min.x) { -1.0 } else { 1.0 };
                    let mut dims = Vec::new();
                    let make = |p1: Point2D, p2: Point2D, line: f64| {
                        // Offset from p1 so the dimension line lies at `line`
                        let n = AnnotationGeometry::linear_direction(p1, p2, axis).perp();
                        let at = if horizontal { Point2D::new(p1.x, line) } else { Point2D::new(line, p1.y) };
                        AnnotationGeometry::Linear { p1, p2, axis, offset: (at - p1).dot(n) }
                    };
                    match self.annotate.qdim_mode {
                        QDimMode::Continuous => dims.extend(pts.windows(2).map(|w| make(w[0], w[1], line))),
                        QDimMode::Baseline => dims.extend(pts.iter().skip(1).enumerate().map(|(i, p)| make(pts[0], *p, line + spacing * i as f64))),
                        QDimMode::Staggered => dims.extend(pts.windows(2).enumerate().map(|(i, w)| make(w[0], w[1], line + spacing * (i % 2) as f64))),
                    }
                    for g in &dims {
                        preview(g);
                    }
                    label(&format!("{} 個標註點 · {} · 點選標註線位置", pts.len(), self.annotate.qdim_mode.label()));
                    if input.clicked && !dims.is_empty() {
                        created.extend(dims);
                        self.draft_points.clear();
                    }
                }
            },
            ToolMode::DimContinue | ToolMode::DimBaseline => match self.base_dim(doc, *selected_id) {
                Some((b1, b2, axis, offset)) => {
                    let d = AnnotationGeometry::linear_direction(b1, b2, axis);
                    let n = d.perp();
                    let line = b1.dot(n) + offset; // dimension line position along n
                    let g = if tool == ToolMode::DimContinue {
                        let p2 = if axis == DimAxis::Aligned { b2 + d * (cursor - b2).dot(d) } else { cursor };
                        AnnotationGeometry::Linear { p1: b2, p2, axis, offset: line - b2.dot(n) }
                    } else {
                        let p2 = if axis == DimAxis::Aligned { b1 + d * (cursor - b1).dot(d) } else { cursor };
                        let sign = if offset < 0.0 { -1.0 } else { 1.0 };
                        AnnotationGeometry::Linear { p1: b1, p2, axis, offset: offset + sign * self.annotate.stack_spacing }
                    };
                    preview(&g);
                    label(if tool == ToolMode::DimContinue { "點選下一個延伸線原點 (連續標註，Enter 結束)" } else { "點選下一個延伸線原點 (基線標註，Enter 結束)" });
                    if input.clicked && g.measurement().unwrap_or(0.0) > 1.0 {
                        created.push(g);
                    }
                }
                None => label("請先建立或選取一個線性標註"),
            },
            ToolMode::MText => {
                label("點選文字的插入位置 (左上角)");
                if input.clicked {
                    self.text_editor = Some(TextEditor {
                        geometry: AnnotationGeometry::Text { position: cursor, text: String::new(), rotation_deg: 0.0 },
                        text: String::new(),
                        screen_pos: hover_screen,
                        focus: true,
                    });
                }
            }
            ToolMode::MLeader => match self.draft_points.first().copied() {
                None => {
                    label("點選引線箭頭位置");
                    if input.clicked {
                        self.draft_points.push(cursor);
                    }
                }
                Some(tip) => {
                    preview(&AnnotationGeometry::Leader { points: vec![tip, cursor], text: "…".into() });
                    label("點選引線轉折 (文字) 位置");
                    if input.clicked && tip.distance(cursor) > 1.0 {
                        self.text_editor = Some(TextEditor {
                            geometry: AnnotationGeometry::Leader { points: vec![tip, cursor], text: String::new() },
                            text: String::new(),
                            screen_pos: hover_screen,
                            focus: true,
                        });
                        self.draft_points.clear();
                    }
                }
            },
            _ => {}
        }

        if !created.is_empty() {
            push_undo(undo_stack, doc);
            let count = created.len();
            let mut last = None;
            for g in created {
                let linear = matches!(g, AnnotationGeometry::Linear { .. });
                let id = doc.insert_annotation(g, elevation, h);
                if linear {
                    last = Some(id);
                }
            }
            if last.is_some() {
                self.last_dim = last;
            }
            *status_msg = format!("✔ 已建立 {count} 個標註");
        }
    }

    /// Floating text box of MTEXT / MLEADER; Ctrl+Enter or 確定 places the annotation.
    fn render_text_editor(
        &mut self,
        ui: &Ui,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        selected_id: &mut Option<ElementId>,
        status_msg: &mut String,
    ) {
        let Some(editor) = &mut self.text_editor else { return };
        let mut done: Option<bool> = None;
        let title = if matches!(editor.geometry, sketchcad_bim::AnnotationGeometry::Leader { .. }) { "📝 引線文字 (MLEADER)" } else { "📝 多行文字 (MTEXT)" };
        egui::Window::new(title)
            .id(egui::Id::new("annotation_text_editor"))
            .fixed_pos(editor.screen_pos + egui::vec2(16.0, 16.0))
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                let response = ui.add(egui::TextEdit::multiline(&mut editor.text).desired_rows(4).desired_width(260.0).hint_text("輸入說明文字 (可多行)"));
                if editor.focus {
                    response.request_focus();
                    editor.focus = false;
                }
                ui.horizontal(|ui| {
                    ui.label("字高:");
                    ui.add(egui::DragValue::new(&mut self.annotate.text_height).speed(5.0).range(10.0..=5000.0).suffix(" mm"));
                });
                ui.horizontal(|ui| {
                    if ui.button("✔ 確定 (Ctrl+Enter)").clicked() {
                        done = Some(true);
                    }
                    if ui.button("取消 (Esc)").clicked() {
                        done = Some(false);
                    }
                });
                let (ctrl_enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter) && (i.modifiers.command || i.modifiers.ctrl), i.key_pressed(egui::Key::Escape)));
                if ctrl_enter {
                    done = Some(true);
                } else if esc {
                    done = Some(false);
                }
            });
        match done {
            Some(true) => {
                let Some(editor) = self.text_editor.take() else { return };
                let text = editor.text.trim_end().to_string();
                if text.trim().is_empty() {
                    return;
                }
                let geometry = match editor.geometry {
                    sketchcad_bim::AnnotationGeometry::Text { position, rotation_deg, .. } => sketchcad_bim::AnnotationGeometry::Text { position, text, rotation_deg },
                    sketchcad_bim::AnnotationGeometry::Leader { points, .. } => sketchcad_bim::AnnotationGeometry::Leader { points, text },
                    other => other,
                };
                push_undo(undo_stack, doc);
                *selected_id = Some(doc.insert_annotation(geometry, self.annotate_elevation(), self.annotate.text_height));
                *status_msg = "✔ 已建立文字註解".into();
            }
            Some(false) => self.text_editor = None,
            None => {}
        }
    }
}

fn push_undo(undo_stack: &mut Vec<BimDocument>, doc: &BimDocument) {
    if undo_stack.len() >= 50 {
        undo_stack.remove(0);
    }
    undo_stack.push(doc.clone());
}

impl Viewport2D {
    pub fn new() -> Self {
        Self {
            camera: Camera2D {
                pan_offset: Point2D::new(4000.0, 3000.0),
                zoom: 0.08, // 0.08 pixels per mm = 1000mm is 80 pixels
            },
            wall_start_pt: None,
            slab_start_pt: None,
            ceiling_start_pt: None,
            stair_start_pt: None,
            railing_start_pt: None,
            roof_start_pt: None,
            ramp_start_pt: None,
            curtain_wall_start_pt: None,
            space_start_pt: None,
            furniture_cycle_kind: FurnitureKind::Sofa,
            furniture_rotation: 0.0,
            light_cycle_type: LightType::Downlight,
            window_type: WindowType::default(),
            outlet_standard: PlugStandard::default(),
            outlet_voltage: OutletVoltage::default(),
            outlet_gangs: 2,
            model_id: None,
            draft_points: Vec::new(),
            draft_tool: ToolMode::Select,
            hatch_pattern: sketchcad_bim::HatchPattern::default(),
            hatch_spacing: 150.0,
            hatch_angle: 0.0,
            modify: ModifySettings::default(),
            modify_target: None,
            annotate: AnnotateSettings::default(),
            dim_source: None,
            last_dim: None,
            text_editor: None,
            move_base_pt: None,
            snap_grid_size: 500.0,
            snap_enabled: true,
            show_dimensions: true, // 預設顯示 2D 尺寸標註與工程輔助線
            sheet_view: SheetView::All,
            plan: PlanSet::default(),
            plan_watcher: DocWatcher::default(),
            frame_pending: true,
            pan_mode: false,
            zoom_anim: None,
        }
    }

    /// Rebuilds the plan sheets when the document changed.
    fn ensure_plan(&mut self, doc: &BimDocument) {
        if self.plan_watcher.poll(doc) {
            self.plan = build_plan_set(doc);
            if let SheetView::Single(i) = self.sheet_view {
                if i >= self.plan.sheets.len() {
                    self.sheet_view = SheetView::All;
                }
            }
        }
    }

    /// True when several sheets are tiled (tools are read-only in this mode).
    fn is_tiled(&self) -> bool {
        self.sheet_view == SheetView::All && self.plan.sheets.len() > 1
    }

    /// World-space translation of every displayed sheet: (sheet index, offset).
    fn sheet_layout(&self) -> Vec<(usize, Vector2D)> {
        if !self.is_tiled() {
            let index = match self.sheet_view {
                SheetView::Single(i) => i,
                SheetView::All => 0,
            };
            return if index < self.plan.sheets.len() { vec![(index, Vector2D::ZERO)] } else { Vec::new() };
        }
        let Some((min, max)) = self.plan.extent else { return Vec::new() };
        let n = self.plan.sheets.len();
        let cols = if n <= 3 { n } else { ((n as f64) * 1.5).sqrt().ceil() as usize };
        let size = max - min;
        let gap = size.x.max(size.y) * 0.12;
        // Rows get extra room for the sheet titles printed under each frame
        let row_gap = size.x.max(size.y) * 0.3;
        (0..n)
            .map(|i| {
                let (col, row) = ((i % cols) as f64, (i / cols) as f64);
                (i, Vector2D::new(col * (size.x + gap), -row * (size.y + row_gap)))
            })
            .collect()
    }

    /// Bounds (in viewport world space) of everything currently displayed.
    fn view_bounds(&self) -> Option<(Point2D, Point2D)> {
        let (min, max) = self.plan.extent?;
        let layout = self.sheet_layout();
        let mut lo = Point2D::splat(f64::INFINITY);
        let mut hi = Point2D::splat(f64::NEG_INFINITY);
        for (_, off) in &layout {
            lo = lo.min(min + *off);
            hi = hi.max(max + *off);
        }
        (!layout.is_empty()).then_some((lo, hi))
    }

    fn frame_bounds(&mut self, viewport: egui::Vec2) {
        if let Some((min, max)) = self.view_bounds() {
            let size = max - min;
            self.camera.pan_offset = (min + max) * 0.5;
            let zx = (viewport.x as f64 * 0.92) / size.x.max(1000.0);
            let zy = (viewport.y as f64 * 0.80) / size.y.max(1000.0);
            self.camera.zoom = zx.min(zy).clamp(MIN_ZOOM, 1.0);
            self.zoom_anim = None;
        }
    }

    /// Switches sheet view while keeping the visible drawing under the cursor in place.
    fn switch_sheet_view(&mut self, view: SheetView) {
        let offset_of = |vp: &Self, sheet: usize| {
            vp.sheet_layout().into_iter().find(|(i, _)| *i == sheet).map(|(_, o)| o).unwrap_or(Vector2D::ZERO)
        };
        match (self.sheet_view, view) {
            (SheetView::All, SheetView::Single(i)) if self.is_tiled() => {
                self.camera.pan_offset -= offset_of(self, i);
            }
            (SheetView::Single(i), SheetView::All) => {
                self.sheet_view = SheetView::All;
                if self.is_tiled() {
                    self.camera.pan_offset += offset_of(self, i);
                }
            }
            _ => {}
        }
        self.sheet_view = view;
    }

    pub fn frame_document(&mut self, doc: &BimDocument) {
        self.plan_watcher.invalidate();
        self.ensure_plan(doc);
        self.sheet_view = SheetView::All;
        self.frame_pending = true;
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        current_tool: ToolMode,
        selected_id: &mut Option<ElementId>,
        status_msg: &mut String,
    ) -> Option<ElementId> {
        let mut clicked_element = None;
        let avail = ui.available_size();
        let size = egui::Vec2::new(avail.x.max(100.0), avail.y.max(100.0));
        let (response, painter) = ui.allocate_painter(
            size,
            egui::Sense::click_and_drag(),
        );

        let rect = response.rect;
        let center = Point2D::new(rect.center().x as f64, rect.center().y as f64);

        self.ensure_plan(doc);
        if self.frame_pending {
            self.frame_pending = false;
            self.frame_bounds(rect.size());
        }

        // 1-2. Zoom & free pan (放大 / 縮小 / 自由移動)
        self.handle_navigation(ui, &response, rect, center, current_tool);

        // Background
        painter.rect_filled(rect, 0.0, Color32::from_rgb(26, 28, 32));

        // Coordinate transforms (on a camera snapshot so `self` stays free for mutation)
        let cam = self.camera.clone();
        let to_screen = |world: Point2D| -> Pos2 {
            let sx = center.x + (world.x - cam.pan_offset.x) * cam.zoom;
            let sy = center.y - (world.y - cam.pan_offset.y) * cam.zoom; // Invert Y for CAD coordinates
            Pos2::new(sx as f32, sy as f32)
        };

        let to_world = |screen: Pos2| -> Point2D {
            let wx = (screen.x as f64 - center.x) / cam.zoom + cam.pan_offset.x;
            let wy = -(screen.y as f64 - center.y) / cam.zoom + cam.pan_offset.y;
            Point2D::new(wx, wy)
        };

        let tiled = self.is_tiled();
        let layout = self.sheet_layout();
        let zoom = self.camera.zoom;
        let pan = self.camera.pan_offset;
        let sheet_to_screen = |p: Point2D, off: Vector2D| -> Pos2 {
            Pos2::new(
                (center.x + (p.x + off.x - pan.x) * zoom) as f32,
                (center.y - (p.y + off.y - pan.y) * zoom) as f32,
            )
        };

        if tiled {
            // 3. Sheet frames & titles (圖紙框與圖名)
            if let Some((min, max)) = self.plan.extent {
                for &(si, off) in &layout {
                    let sheet = &self.plan.sheets[si];
                    let frame = Rect::from_two_pos(sheet_to_screen(min, off), sheet_to_screen(max, off));
                    painter.rect_filled(frame, 2.0, Color32::from_rgb(31, 34, 40));
                    painter.rect_stroke(frame, 2.0, Stroke::new(1.0, Color32::from_rgb(72, 80, 96)), egui::StrokeKind::Outside);
                    let title = format!("{}  平面圖", sheet.name);
                    let level = format!(
                        "EL {:+.2} m · 剖切高度 {:.2} m",
                        sheet.label_elevation / 1000.0,
                        (sheet.cut_z - sheet.elevation) / 1000.0
                    );
                    let title_pos = Pos2::new(frame.left(), frame.bottom() + 6.0);
                    painter.text(title_pos, egui::Align2::LEFT_TOP, title, egui::FontId::proportional(14.0), Color32::from_rgb(235, 238, 245));
                    painter.text(
                        title_pos + egui::vec2(0.0, 18.0),
                        egui::Align2::LEFT_TOP,
                        level,
                        egui::FontId::monospace(10.5),
                        Color32::from_rgb(140, 155, 175),
                    );
                }
            }
        } else {
            // 3. Draw Engineering Grid
            self.draw_grid(&painter, rect, to_screen, to_world);

            // 4. Draw CAD Origin Axes (Red X, Green Y)
            let origin_screen = to_screen(Point2D::ZERO);
            let x_axis_end = to_screen(Point2D::new(2000.0, 0.0));
            let y_axis_end = to_screen(Point2D::new(0.0, 2000.0));
            painter.line_segment([origin_screen, x_axis_end], Stroke::new(2.0, Color32::from_rgb(230, 70, 70)));
            painter.line_segment([origin_screen, y_axis_end], Stroke::new(2.0, Color32::from_rgb(70, 210, 70)));
        }

        // 5. Floor-plan lines with architectural line weights (剖切粗線 / 投影細線 / 上方虛線)
        let selected_index = selected_id.and_then(|id| self.plan.elem_index(id));
        let sheet_clip = |off: Vector2D| match (tiled, self.plan.extent) {
            (true, Some((min, max))) => {
                Rect::from_two_pos(sheet_to_screen(min, off), sheet_to_screen(max, off)).intersect(rect)
            }
            _ => rect,
        };
        for &(si, off) in &layout {
            let clip = sheet_clip(off);
            let painter = painter.with_clip_rect(clip);
            let clip = clip.expand(4.0);
            // Dash phase carried along an element's consecutive segments (curves are many
            // short segments; restarting the pattern on each would make it look solid)
            let mut dash = (u32::MAX, 0.0f32);
            for line in &self.plan.sheets[si].lines {
                let a = sheet_to_screen(line.a, off);
                let b = sheet_to_screen(line.b, off);
                if dash.0 != line.elem {
                    dash = (line.elem, 0.0);
                }
                let phase = dash.1;
                dash.1 += (b - a).length();
                if !clip.intersects(Rect::from_two_pos(a, b)) {
                    continue;
                }
                let mut stroke = line_stroke(line.style);
                let pen = self.plan.pens.get(&line.elem);
                if let Some(pen) = pen {
                    if let Some([r, g, bl, al]) = pen.color {
                        stroke.color = Color32::from_rgba_unmultiplied(r, g, bl, al);
                    }
                    if let Some(width) = pen.width {
                        stroke.width = width;
                    }
                    stroke.color = stroke.color.gamma_multiply(pen.alpha);
                }
                match pen.and_then(|p| p.linetype) {
                    Some(sketchcad_bim::LineType::Dashed) => patterned_segment(&painter, a, b, stroke, &[9.0, 5.0], phase),
                    Some(sketchcad_bim::LineType::Hidden) => patterned_segment(&painter, a, b, stroke, &[4.0, 3.0], phase),
                    Some(sketchcad_bim::LineType::Center) => patterned_segment(&painter, a, b, stroke, &[16.0, 4.0, 3.0, 4.0], phase),
                    _ if line.style == LineStyle::Overhead => painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 7.0, 5.0)),
                    _ => {
                        painter.line_segment([a, b], stroke);
                    }
                }
            }
        }

        // 5a. Annotation text (dimension values, notes, leader text)
        for &(si, off) in &layout {
            let painter = painter.with_clip_rect(sheet_clip(off));
            for id in &self.plan.sheets[si].members {
                let Some(a) = doc.get_annotation(*id).filter(|_| doc.is_visible(*id)) else { continue };
                let pen = self.plan.elem_index(*id).and_then(|i| self.plan.pens.get(&i));
                let color = if *selected_id == Some(*id) {
                    Color32::from_rgb(255, 215, 0)
                } else {
                    let base = pen.and_then(|p| p.color).map_or(ANNOTATION_COLOR, |[r, g, b, al]| Color32::from_rgba_unmultiplied(r, g, b, al));
                    base.gamma_multiply(pen.map_or(1.0, |p| p.alpha))
                };
                draw_annotation_texts(&painter, a, &|p| sheet_to_screen(p, off), zoom as f32, color);
            }
        }

        // 5b. Highlight Selected Element
        if let Some(sel) = selected_index {
            let stroke = Stroke::new(3.0, Color32::from_rgb(255, 215, 0));
            for &(si, off) in &layout {
                let painter = painter.with_clip_rect(sheet_clip(off));
                for line in self.plan.sheets[si].lines.iter().filter(|l| l.elem == sel) {
                    painter.line_segment([sheet_to_screen(line.a, off), sheet_to_screen(line.b, off)], stroke);
                }
            }
        }

        // 5c. Architectural Dimension Lines & Witness Auxiliaries (📐 2D 尺寸標註與輔助線)
        if self.show_dimensions && !tiled {
            if let Some(&(si, _)) = layout.first() {
                let members = &self.plan.sheets[si].members;
                self.draw_dimensions(&painter, &to_screen, doc, &|id| members.contains(&id));
            }
        }

        // 5d. Space Room Tags & Area Stamps (🏷 房間區域標籤與坪數標記)
        for &(si, off) in &layout {
            for &id in &self.plan.sheets[si].members {
                let Some(sp) = doc.get_space(id) else { continue };
                if !doc.is_visible(id) {
                    continue;
                }
                let badge_w = 130.0;
                let badge_h = 42.0;
                // Only tag rooms large enough on screen to hold the badge
                let room_px = (sp.area_m2() * 1.0e6).sqrt() * zoom;
                if room_px < badge_w as f64 * 0.8 {
                    continue;
                }
                let center_screen = sheet_to_screen(sp.centroid(), off);
                let badge_rect = Rect::from_center_size(center_screen, egui::vec2(badge_w, badge_h));

                painter.rect_filled(
                    badge_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(22, 28, 40, 220),
                );
                painter.rect_stroke(
                    badge_rect,
                    4.0,
                    Stroke::new(1.2, Color32::from_rgb(80, 165, 235)),
                    egui::StrokeKind::Inside,
                );

                painter.text(
                    Pos2::new(center_screen.x, center_screen.y - 8.0),
                    egui::Align2::CENTER_CENTER,
                    &sp.name,
                    egui::FontId::proportional(12.5),
                    Color32::from_rgb(255, 255, 255),
                );

                let area_text = format!("{:.2} 坪 ({:.1} m²)", sp.area_ping(), sp.area_m2());
                painter.text(
                    Pos2::new(center_screen.x, center_screen.y + 10.0),
                    egui::Align2::CENTER_CENTER,
                    area_text,
                    egui::FontId::monospace(10.5),
                    Color32::from_rgb(255, 215, 0),
                );
            }
        }

        // 5e. Sheet selector bar (圖紙切換列) and zoom / pan controls
        self.render_sheet_bar(ui, rect);
        self.render_nav_controls(ui, rect, center);

        // 5f. Door swing picker (左開 / 右開 · 內開 / 外開) next to the selected door
        if !tiled {
            if let Some(id) = *selected_id {
                render_door_swing_popup(ui, rect, &to_screen, doc, id, undo_stack, status_msg);
            }
        }

        // Double-click a tiled sheet to open it for editing
        if tiled && response.double_clicked() {
            if let (Some(pos), Some((min, max))) = (response.interact_pointer_pos(), self.plan.extent) {
                let hit = layout.iter().find(|&&(_, off)| {
                    Rect::from_two_pos(sheet_to_screen(min, off), sheet_to_screen(max, off)).contains(pos)
                });
                if let Some(&(si, _)) = hit {
                    self.switch_sheet_view(SheetView::Single(si));
                }
            }
        }

        // Rotate Furniture shortcut with 'R' key
        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            self.furniture_rotation += std::f64::consts::FRAC_PI_2;
            if self.furniture_rotation >= std::f64::consts::TAU {
                self.furniture_rotation -= std::f64::consts::TAU;
            }
        }

        // 6. Handle Mouse Hover, Crosshair & Snap
        if let Some(hover_pos) = response.hover_pos() {
            let raw_world = to_world(hover_pos);
            let world_pt = if self.snap_enabled {
                self.snap(raw_world)
            } else {
                raw_world
            };

            *status_msg = format!(
                "X: {:.1} mm | Y: {:.1} mm (Zoom: {:.1}%)",
                world_pt.x,
                world_pt.y,
                self.camera.zoom * 1000.0
            );

            // Draw Crosshair (Full viewport crosshair)
            painter.line_segment(
                [Pos2::new(rect.left(), hover_pos.y), Pos2::new(rect.right(), hover_pos.y)],
                Stroke::new(0.8, Color32::from_rgba_unmultiplied(200, 200, 200, 80)),
            );
            painter.line_segment(
                [Pos2::new(hover_pos.x, rect.top()), Pos2::new(hover_pos.x, rect.bottom())],
                Stroke::new(0.8, Color32::from_rgba_unmultiplied(200, 200, 200, 80)),
            );

            // 7. Interactive Drawing Modes (tiled overview and ✋ pan mode: selection only)
            let active_tool = if tiled || self.pan_mode { ToolMode::Select } else { current_tool };
            if active_tool != self.draft_tool {
                self.draft_points.clear();
                self.modify_target = None;
                self.dim_source = None;
                self.draft_tool = active_tool;
            }
            if active_tool.is_annotate() && self.text_editor.is_none() {
                let input = DraftInput {
                    clicked: response.clicked() && !response.double_clicked(),
                    finish: response.double_clicked()
                        || response.secondary_clicked()
                        || (!ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter))),
                };
                let picked = self.pick_element(doc, hover_pos, world_pt, center);
                self.annotate_tool_ui(active_tool, input, world_pt, hover_pos, picked, &painter, &to_screen, doc, undo_stack, selected_id, status_msg);
            }
            if active_tool.is_modify() {
                let input = DraftInput {
                    clicked: response.clicked() && !response.double_clicked(),
                    finish: response.double_clicked()
                        || response.secondary_clicked()
                        || (!ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter))),
                };
                let shift = ui.input(|i| i.modifiers.shift);
                let picked = self.pick_element(doc, hover_pos, world_pt, center);
                self.modify_tool_ui(active_tool, input, shift, world_pt, picked, &painter, &to_screen, doc, undo_stack, selected_id, status_msg);
            }
            if active_tool.is_drafting() {
                let input = DraftInput {
                    clicked: response.clicked() && !response.double_clicked(),
                    finish: response.double_clicked()
                        || response.secondary_clicked()
                        || (!ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter))),
                };
                self.draft_tool_ui(active_tool, input, world_pt, &painter, &to_screen, doc, undo_stack, selected_id, status_msg);
            }
            match active_tool {
                ToolMode::Wall => {
                    if let Some(start) = self.wall_start_pt {
                        // Elastic rubber-band line preview
                        let p0 = to_screen(start);
                        let p1 = to_screen(world_pt);
                        painter.line_segment([p0, p1], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));

                        let dist = start.distance(world_pt);
                        let mid = Pos2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5 - 12.0);
                        painter.text(
                            mid,
                            egui::Align2::CENTER_CENTER,
                            format!("長度: {:.0} mm", dist),
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(255, 215, 0),
                        );

                        if response.clicked() {
                            if dist > 50.0 {
                                push_undo(undo_stack, doc);
                                let new_wall = Wall::new(
                                    start,
                                    world_pt,
                                    200.0,
                                    3500.0,
                                    sketchcad_bim::Material::concrete(),
                                );
                                let id = doc.add_element(new_wall);
                                *selected_id = Some(id);
                                doc.sync();
                                *status_msg = format!("✔ 已建立牆體 (長度: {:.0} mm)", dist);
                            }
                            self.wall_start_pt = None;
                        }
                    } else if response.clicked() {
                        self.wall_start_pt = Some(world_pt);
                    }
                }
                ToolMode::Column => {
                    // Preview ghost column
                    let hw = 250.0;
                    let hd = 250.0;
                    let p0 = to_screen(Point2D::new(world_pt.x - hw, world_pt.y - hd));
                    let p1 = to_screen(Point2D::new(world_pt.x + hw, world_pt.y - hd));
                    let p2 = to_screen(Point2D::new(world_pt.x + hw, world_pt.y + hd));
                    let p3 = to_screen(Point2D::new(world_pt.x - hw, world_pt.y + hd));

                    painter.line_segment([p0, p1], Stroke::new(1.8, Color32::from_rgb(100, 200, 255)));
                    painter.line_segment([p1, p2], Stroke::new(1.8, Color32::from_rgb(100, 200, 255)));
                    painter.line_segment([p2, p3], Stroke::new(1.8, Color32::from_rgb(100, 200, 255)));
                    painter.line_segment([p3, p0], Stroke::new(1.8, Color32::from_rgb(100, 200, 255)));
                    painter.line_segment([p0, p2], Stroke::new(1.2, Color32::from_rgb(100, 200, 255)));
                    painter.line_segment([p1, p3], Stroke::new(1.2, Color32::from_rgb(100, 200, 255)));

                    let label_pos = to_screen(Point2D::new(world_pt.x, world_pt.y - 350.0));
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        "點擊放置結構柱 (500x500mm)",
                        egui::FontId::monospace(12.0),
                        Color32::from_rgb(100, 200, 255),
                    );

                    if response.clicked() {
                        push_undo(undo_stack, doc);
                        let id = doc.insert_column(world_pt, 500.0, 500.0, 3500.0, sketchcad_bim::Material::concrete());
                        *selected_id = Some(id);
                        *status_msg = format!("✔ 已建立結構柱 (500x500mm) 於 ({:.0}, {:.0})", world_pt.x, world_pt.y);
                    }
                }
                ToolMode::Door => {
                    if let Some((wall_id, dist, closest_pt, dir, offset)) = find_closest_wall(doc, world_pt) {
                        if dist <= 1200.0 {
                            let norm = Vector2D::new(-dir.y, dir.x);
                            let door_width = 900.0;
                            let half_w = door_width * 0.5;
                            let hinge = closest_pt - dir * half_w;
                            let frame_end = hinge + dir * door_width;
                            let leaf_end = hinge + norm * door_width;

                            let s_hinge = to_screen(hinge);
                            let s_frame = to_screen(frame_end);
                            let s_leaf = to_screen(leaf_end);

                            painter.line_segment([s_hinge, s_frame], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));
                            painter.line_segment([s_hinge, s_leaf], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));

                            let segments = 12;
                            for i in 0..segments {
                                let a0 = (std::f64::consts::PI * 0.5) * (i as f64 / segments as f64);
                                let a1 = (std::f64::consts::PI * 0.5) * ((i + 1) as f64 / segments as f64);
                                let pt0 = hinge + dir * (door_width * a0.cos()) + norm * (door_width * a0.sin());
                                let pt1 = hinge + dir * (door_width * a1.cos()) + norm * (door_width * a1.sin());
                                painter.line_segment([to_screen(pt0), to_screen(pt1)], Stroke::new(1.5, Color32::from_rgb(255, 215, 0)));
                            }

                            let label_pos = to_screen(closest_pt + norm * 450.0);
                            painter.text(
                                label_pos,
                                egui::Align2::CENTER_CENTER,
                                "點擊於牆體放置門扇 (900x2100mm)",
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(255, 215, 0),
                            );

                            if response.clicked() {
                                push_undo(undo_stack, doc);
                                if let Some(door_id) = doc.insert_door(
                                    wall_id,
                                    offset,
                                    door_width,
                                    2100.0,
                                    sketchcad_bim::Material::wood(),
                                    9500.0,
                                ) {
                                    *selected_id = Some(door_id);
                                    *status_msg = format!("✔ 已於牆體建立門扇 (900x2100mm, 偏移: {:.0}mm)", offset);
                                }
                            }
                        } else {
                            painter.text(
                                hover_pos,
                                egui::Align2::LEFT_TOP,
                                " [請移動至牆體上以放置門扇]",
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(240, 180, 80),
                            );
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [圖面無牆體，請先繪製牆體]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(240, 100, 100),
                        );
                    }
                }
                ToolMode::Window => {
                    if let Some((wall_id, dist, closest_pt, dir, offset)) = find_closest_wall(doc, world_pt) {
                        if dist <= 1200.0 {
                            let norm = Vector2D::new(-dir.y, dir.x);
                            let win_width = 1200.0;
                            let half_w = win_width * 0.5;
                            let half_d = 80.0;

                            let p0 = closest_pt - dir * half_w - norm * half_d;
                            let p1 = closest_pt + dir * half_w - norm * half_d;
                            let p2 = closest_pt + dir * half_w + norm * half_d;
                            let p3 = closest_pt - dir * half_w + norm * half_d;

                            let s0 = to_screen(p0);
                            let s1 = to_screen(p1);
                            let s2 = to_screen(p2);
                            let s3 = to_screen(p3);

                            painter.line_segment([s0, s1], Stroke::new(2.0, Color32::from_rgb(80, 200, 255)));
                            painter.line_segment([s1, s2], Stroke::new(2.0, Color32::from_rgb(80, 200, 255)));
                            painter.line_segment([s2, s3], Stroke::new(2.0, Color32::from_rgb(80, 200, 255)));
                            painter.line_segment([s3, s0], Stroke::new(2.0, Color32::from_rgb(80, 200, 255)));

                            let g0 = to_screen(closest_pt - dir * half_w);
                            let g1 = to_screen(closest_pt + dir * half_w);
                            painter.line_segment([g0, g1], Stroke::new(2.5, Color32::from_rgb(255, 255, 255)));

                            let label_pos = to_screen(closest_pt + norm * 450.0);
                            painter.text(
                                label_pos,
                                egui::Align2::CENTER_CENTER,
                                format!("點擊於牆體放置{} (1200x1400mm)", self.window_type.label()),
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(80, 200, 255),
                            );

                            if response.clicked() {
                                push_undo(undo_stack, doc);
                                if let Some(win_id) = doc.insert_window(
                                    wall_id,
                                    offset,
                                    900.0,
                                    win_width,
                                    1400.0,
                                    sketchcad_bim::Material::glass(),
                                    16000.0,
                                ) {
                                    if let Some(win) = doc.get_window_mut(win_id) {
                                        win.window_type = self.window_type;
                                    }
                                    *selected_id = Some(win_id);
                                    *status_msg = format!("✔ 已於牆體建立{} (1200x1400mm, 偏移: {:.0}mm)", self.window_type.label(), offset);
                                }
                            }
                        } else {
                            painter.text(
                                hover_pos,
                                egui::Align2::LEFT_TOP,
                                " [請移動至牆體上以放置窗戶]",
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(240, 180, 80),
                            );
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [圖面無牆體，請先繪製牆體]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(240, 100, 100),
                        );
                    }
                }
                ToolMode::Slab => {
                    if let Some(start) = self.slab_start_pt {
                        let w = (world_pt.x - start.x).abs();
                        let d = (world_pt.y - start.y).abs();
                        let min_x = start.x.min(world_pt.x);
                        let max_x = start.x.max(world_pt.x);
                        let min_y = start.y.min(world_pt.y);
                        let max_y = start.y.max(world_pt.y);

                        let p0 = to_screen(Point2D::new(min_x, min_y));
                        let p1 = to_screen(Point2D::new(max_x, min_y));
                        let p2 = to_screen(Point2D::new(max_x, max_y));
                        let p3 = to_screen(Point2D::new(min_x, max_y));

                        let rect = Rect::from_min_max(p0, p2);
                        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(80, 160, 240, 45));
                        painter.line_segment([p0, p1], Stroke::new(2.0, Color32::from_rgb(80, 160, 240)));
                        painter.line_segment([p1, p2], Stroke::new(2.0, Color32::from_rgb(80, 160, 240)));
                        painter.line_segment([p2, p3], Stroke::new(2.0, Color32::from_rgb(80, 160, 240)));
                        painter.line_segment([p3, p0], Stroke::new(2.0, Color32::from_rgb(80, 160, 240)));
                        painter.line_segment([p0, p2], Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 160, 240, 120)));
                        painter.line_segment([p1, p3], Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 160, 240, 120)));

                        let area = (w * d) / 1_000_000.0;
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 樓板區域: {:.0} x {:.0} mm (面積: {:.2} m² / {:.1} 坪)", w, d, area, area * 0.3025),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(100, 200, 255),
                        );

                        if response.clicked() {
                            if w > 100.0 && d > 100.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_slab(
                                    Point2D::new(min_x, min_y),
                                    w,
                                    d,
                                    200.0,
                                    0.0,
                                    sketchcad_bim::Material::concrete(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立結構樓板 ({:.0}x{:.0}mm, 面積: {:.2} m²)", w, d, area);
                            }
                            self.slab_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊第一個角點以繪製樓板]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(120, 200, 255),
                        );
                        if response.clicked() {
                            self.slab_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Ceiling => {
                    if let Some(start) = self.ceiling_start_pt {
                        let w = (world_pt.x - start.x).abs();
                        let d = (world_pt.y - start.y).abs();
                        let min_x = start.x.min(world_pt.x);
                        let max_x = start.x.max(world_pt.x);
                        let min_y = start.y.min(world_pt.y);
                        let max_y = start.y.max(world_pt.y);

                        let p0 = to_screen(Point2D::new(min_x, min_y));
                        let p1 = to_screen(Point2D::new(max_x, min_y));
                        let p2 = to_screen(Point2D::new(max_x, max_y));
                        let p3 = to_screen(Point2D::new(min_x, max_y));

                        let rect = Rect::from_min_max(p0, p2);
                        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(240, 220, 100, 35));
                        painter.line_segment([p0, p1], Stroke::new(2.0, Color32::from_rgb(240, 220, 100)));
                        painter.line_segment([p1, p2], Stroke::new(2.0, Color32::from_rgb(240, 220, 100)));
                        painter.line_segment([p2, p3], Stroke::new(2.0, Color32::from_rgb(240, 220, 100)));
                        painter.line_segment([p3, p0], Stroke::new(2.0, Color32::from_rgb(240, 220, 100)));

                        let mut gx = min_x + 600.0;
                        while gx < max_x - 10.0 {
                            painter.line_segment([to_screen(Point2D::new(gx, min_y)), to_screen(Point2D::new(gx, max_y))], Stroke::new(0.8, Color32::from_rgba_unmultiplied(240, 220, 100, 100)));
                            gx += 600.0;
                        }
                        let mut gy = min_y + 600.0;
                        while gy < max_y - 10.0 {
                            painter.line_segment([to_screen(Point2D::new(min_x, gy)), to_screen(Point2D::new(max_x, gy))], Stroke::new(0.8, Color32::from_rgba_unmultiplied(240, 220, 100, 100)));
                            gy += 600.0;
                        }

                        let area = (w * d) / 1_000_000.0;
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 天花板區域: {:.0} x {:.0} mm (淨高 2800mm, 面積: {:.2} m²)", w, d, area),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(255, 230, 120),
                        );

                        if response.clicked() {
                            if w > 100.0 && d > 100.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_ceiling(
                                    Point2D::new(min_x, min_y),
                                    w,
                                    d,
                                    2800.0,
                                    sketchcad_bim::Material::gypsum(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立輕鋼架天花板 ({:.0}x{:.0}mm, 淨高 2800mm)", w, d);
                            }
                            self.ceiling_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊第一個角點以繪製天花板]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(250, 220, 100),
                        );
                        if response.clicked() {
                            self.ceiling_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Stairs => {
                    if let Some(start) = self.stair_start_pt {
                        let len = start.distance(world_pt);
                        let v = world_pt - start;
                        let dir = if len > 1e-4 { v / len } else { Vector2D::X };
                        let norm = Vector2D::new(-dir.y, dir.x);
                        let half_w = 500.0; // 1000mm width

                        let p0 = to_screen(start - norm * half_w);
                        let p1 = to_screen(start + norm * half_w);
                        let p2 = to_screen(world_pt + norm * half_w);
                        let p3 = to_screen(world_pt - norm * half_w);

                        painter.line_segment([p0, p1], Stroke::new(2.2, Color32::from_rgb(255, 140, 80)));
                        painter.line_segment([p1, p2], Stroke::new(2.2, Color32::from_rgb(255, 140, 80)));
                        painter.line_segment([p2, p3], Stroke::new(2.2, Color32::from_rgb(255, 140, 80)));
                        painter.line_segment([p3, p0], Stroke::new(2.2, Color32::from_rgb(255, 140, 80)));

                        let num_steps = ((len / 280.0).round() as usize).clamp(3, 30);
                        let tread_d = len / (num_steps as f64);
                        for i in 1..num_steps {
                            let pt = start + dir * (i as f64 * tread_d);
                            let s0 = to_screen(pt - norm * half_w);
                            let s1 = to_screen(pt + norm * half_w);
                            painter.line_segment([s0, s1], Stroke::new(1.4, Color32::from_rgb(255, 160, 100)));
                        }

                        let w_start = to_screen(start + dir * (tread_d * 0.5));
                        let w_end = to_screen(world_pt - dir * (tread_d * 0.5));
                        painter.line_segment([w_start, w_end], Stroke::new(2.0, Color32::from_rgb(255, 215, 0)));

                        let riser_h = 3000.0 / (num_steps as f64);
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 直通樓梯: 長度 {:.0}mm, 寬 1000mm, {} 階 (踏步: {:.0}mm, 級高: {:.0}mm)", len, num_steps, tread_d, riser_h),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(255, 180, 100),
                        );

                        if response.clicked() {
                            if len > 300.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_stairs(
                                    start,
                                    world_pt,
                                    1000.0,
                                    3000.0,
                                    num_steps,
                                    sketchcad_bim::Material::concrete(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立直通樓梯 ({} 階, 踏步: {:.0}mm, 總升 3000mm)", num_steps, tread_d);
                            }
                            self.stair_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊起步點 (Bottom) 以繪製樓梯]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(255, 160, 80),
                        );
                        if response.clicked() {
                            self.stair_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Railing => {
                    if let Some(start) = self.railing_start_pt {
                        let len = start.distance(world_pt);
                        let v = world_pt - start;
                        let dir = if len > 1e-4 { v / len } else { Vector2D::X };
                        let norm = Vector2D::new(-dir.y, dir.x);
                        let half_w = 30.0;

                        // Handrail path double line
                        let p0 = to_screen(start - norm * half_w);
                        let p1 = to_screen(start + norm * half_w);
                        let p2 = to_screen(world_pt + norm * half_w);
                        let p3 = to_screen(world_pt - norm * half_w);

                        painter.line_segment([p0, p3], Stroke::new(2.5, Color32::from_rgb(100, 210, 255)));
                        painter.line_segment([p1, p2], Stroke::new(2.5, Color32::from_rgb(100, 210, 255)));

                        // Baluster post points
                        let num_posts = ((len / 900.0).ceil() as usize).max(1);
                        for i in 0..=num_posts {
                            let t = (i as f64) / (num_posts as f64);
                            let pt = start + (world_pt - start) * t;
                            let scr = to_screen(pt);
                            painter.circle_filled(scr, 4.0, Color32::from_rgb(255, 220, 100));
                            painter.circle_stroke(scr, 4.0, Stroke::new(1.2, Color32::from_rgb(40, 40, 40)));
                        }

                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 欄杆扶手: 長度 {:.0}mm, 高度 1100mm, {} 根不鏽鋼立柱與夾膠玻璃", len, num_posts + 1),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(100, 220, 255),
                        );

                        if response.clicked() {
                            if len > 200.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_railing(
                                    sketchcad_core::Point3D::new(start.x, start.y, 0.0),
                                    sketchcad_core::Point3D::new(world_pt.x, world_pt.y, 0.0),
                                    1100.0,
                                    sketchcad_bim::Material::structural_steel(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立欄杆扶手 (長度: {:.0}mm, 高度: 1100mm, {} 根立柱)", len, num_posts + 1);
                            }
                            self.railing_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊起點以繪製欄杆扶手]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(100, 210, 255),
                        );
                        if response.clicked() {
                            self.railing_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Roof => {
                    if let Some(start) = self.roof_start_pt {
                        let min_x = start.x.min(world_pt.x);
                        let max_x = start.x.max(world_pt.x);
                        let min_y = start.y.min(world_pt.y);
                        let max_y = start.y.max(world_pt.y);
                        let w = max_x - min_x;
                        let d = max_y - min_y;

                        let p0 = to_screen(Point2D::new(min_x, min_y));
                        let p1 = to_screen(Point2D::new(max_x, min_y));
                        let p2 = to_screen(Point2D::new(max_x, max_y));
                        let p3 = to_screen(Point2D::new(min_x, max_y));

                        painter.line_segment([p0, p1], Stroke::new(2.2, Color32::from_rgb(255, 120, 80)));
                        painter.line_segment([p1, p2], Stroke::new(2.2, Color32::from_rgb(255, 120, 80)));
                        painter.line_segment([p2, p3], Stroke::new(2.2, Color32::from_rgb(255, 120, 80)));
                        painter.line_segment([p3, p0], Stroke::new(2.2, Color32::from_rgb(255, 120, 80)));

                        // Center ridge line
                        let mid_y = (min_y + max_y) * 0.5;
                        let r0 = to_screen(Point2D::new(min_x, mid_y));
                        let r1 = to_screen(Point2D::new(max_x, mid_y));
                        painter.line_segment([r0, r1], Stroke::new(2.5, Color32::from_rgb(255, 200, 100)));

                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 雙坡屋頂: {:.0} x {:.0} mm (坡度 20°, 屋脊高程 +3500mm)", w, d),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(255, 140, 100),
                        );

                        if response.clicked() {
                            if w > 200.0 && d > 200.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_roof(
                                    Point2D::new(min_x, min_y),
                                    w,
                                    d,
                                    3500.0,
                                    20.0,
                                    sketchcad_bim::Material::concrete(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立雙坡屋頂系統 ({:.0}x{:.0}mm, 坡度 20°)", w, d);
                            }
                            self.roof_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊第一個角點以繪製屋頂邊界]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(255, 140, 100),
                        );
                        if response.clicked() {
                            self.roof_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Ramp => {
                    if let Some(start) = self.ramp_start_pt {
                        let len = start.distance(world_pt);
                        let v = world_pt - start;
                        let dir = if len > 1e-4 { v / len } else { Vector2D::X };
                        let norm = Vector2D::new(-dir.y, dir.x);
                        let half_w = 600.0; // 1200mm accessible ramp width

                        let p0 = to_screen(start - norm * half_w);
                        let p1 = to_screen(start + norm * half_w);
                        let p2 = to_screen(world_pt + norm * half_w);
                        let p3 = to_screen(world_pt - norm * half_w);

                        painter.line_segment([p0, p1], Stroke::new(2.2, Color32::from_rgb(100, 200, 255)));
                        painter.line_segment([p1, p2], Stroke::new(2.2, Color32::from_rgb(100, 200, 255)));
                        painter.line_segment([p2, p3], Stroke::new(2.2, Color32::from_rgb(100, 200, 255)));
                        painter.line_segment([p3, p0], Stroke::new(2.2, Color32::from_rgb(100, 200, 255)));

                        // Ramp climbing arrow
                        let s_start = to_screen(start);
                        let s_end = to_screen(world_pt);
                        painter.line_segment([s_start, s_end], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));
                        let arrow_head_left = to_screen(world_pt - dir * 250.0 + norm * 150.0);
                        let arrow_head_right = to_screen(world_pt - dir * 250.0 - norm * 150.0);
                        painter.line_segment([s_end, arrow_head_left], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));
                        painter.line_segment([s_end, arrow_head_right], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));

                        let rise = (len / 12.0).clamp(100.0, 1500.0);
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 無障礙坡道: 長度 {:.0}mm, 寬 1200mm, 坡度 1:12 (高程提升 {:.0}mm)", len, rise),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(100, 210, 255),
                        );

                        if response.clicked() {
                            if len > 300.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_ramp(
                                    start,
                                    world_pt,
                                    1200.0,
                                    0.0,
                                    rise,
                                    sketchcad_bim::Material::concrete(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立無障礙坡道 (長度: {:.0}mm, 坡度 1:12, 提升 {:.0}mm)", len, rise);
                            }
                            self.ramp_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊起點以繪製無障礙坡道 (Bottom)]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(100, 210, 255),
                        );
                        if response.clicked() {
                            self.ramp_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::CurtainWall => {
                    if let Some(start) = self.curtain_wall_start_pt {
                        let len = start.distance(world_pt);
                        let v = world_pt - start;
                        let dir = if len > 1e-4 { v / len } else { Vector2D::X };
                        let norm = Vector2D::new(-dir.y, dir.x);
                        let half_w = 40.0;

                        let p0 = to_screen(start - norm * half_w);
                        let p1 = to_screen(start + norm * half_w);
                        let p2 = to_screen(world_pt + norm * half_w);
                        let p3 = to_screen(world_pt - norm * half_w);

                        painter.line_segment([p0, p3], Stroke::new(2.4, Color32::from_rgb(120, 220, 255)));
                        painter.line_segment([p1, p2], Stroke::new(2.4, Color32::from_rgb(120, 220, 255)));

                        // Vertical mullion division preview ticks
                        let u_spacing = 1500.0;
                        let num_bays = (len / u_spacing).round().max(1.0) as usize;
                        let step = len / (num_bays as f64);
                        for i in 0..=num_bays {
                            let pt = start + dir * (i as f64 * step);
                            let s0 = to_screen(pt - norm * (half_w * 1.5));
                            let s1 = to_screen(pt + norm * (half_w * 1.5));
                            painter.line_segment([s0, s1], Stroke::new(2.0, Color32::from_rgb(255, 215, 0)));
                        }

                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 結構單元式玻璃幕牆: 長度 {:.0}mm, 高度 3500mm, {} 跨單元 (豎梃間距: {:.0}mm)", len, num_bays, step),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(120, 220, 255),
                        );

                        if response.clicked() {
                            if len > 300.0 {
                                push_undo(undo_stack, doc);
                                let id = doc.insert_curtain_wall(
                                    start,
                                    world_pt,
                                    3500.0,
                                    0.0,
                                    step,
                                    sketchcad_bim::Material::glass(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立結構單元式玻璃幕牆 (長度: {:.0}mm, {} 跨, 豎梃間距 {:.0}mm)", len, num_bays, step);
                            }
                            self.curtain_wall_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點擊起點以繪製玻璃幕牆系統]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(120, 220, 255),
                        );
                        if response.clicked() {
                            self.curtain_wall_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::PushPull => {

                    let mut target = None;
                    let mut min_d = 700.0;

                    for wall in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Wall) {
                        if let Some(w) = doc.get_wall(wall.id()) {
                            let v = w.end - w.start;
                            let len = v.length();
                            if len > 1e-4 {
                                let dir = v / len;
                                let proj = (world_pt - w.start).dot(dir).clamp(0.0, len);
                                let d = world_pt.distance(w.start + dir * proj);
                                if d < min_d {
                                    min_d = d;
                                    target = Some((w.id, "牆體", w.height));
                                }
                            }
                        }
                    }
                    for col in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Column) {
                        if let Some(c) = doc.get_column(col.id()) {
                            let d = world_pt.distance(c.position);
                            if d < min_d {
                                min_d = d;
                                target = Some((c.id, "結構柱", c.height));
                            }
                        }
                    }
                    for slab in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Slab) {
                        if let Some(s) = doc.get_slab(slab.id()) {
                            let center = s.origin + Point2D::new(s.width * 0.5, s.depth * 0.5);
                            let d = world_pt.distance(center);
                            if d < min_d {
                                min_d = d;
                                target = Some((s.id, "結構樓板", s.thickness));
                            }
                        }
                    }

                    if let Some((id, name, height)) = target {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_BOTTOM,
                            format!(" 點擊推拉 {} 高度: {:.0} -> {:.0} mm (+500mm)", name, height, height + 500.0),
                            egui::FontId::monospace(12.5),
                            Color32::from_rgb(255, 160, 50),
                        );

                        if response.clicked() {
                            push_undo(undo_stack, doc);
                            doc.push_pull_element(id, 500.0);
                            *selected_id = Some(id);
                            *status_msg = format!("✔ 已推拉 {} (高度拉伸為 {:.0} mm)", name, height + 500.0);
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [點選牆體、結構柱或樓板進行推拉拉伸]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(200, 200, 200),
                        );
                    }
                }
                ToolMode::Space => {
                    if let Some(start) = self.space_start_pt {
                        let p0 = to_screen(Point2D::new(start.x.min(world_pt.x), start.y.min(world_pt.y)));
                        let p1 = to_screen(Point2D::new(start.x.max(world_pt.x), start.y.max(world_pt.y)));
                        let box_rect = Rect::from_two_pos(p0, p1);
                        painter.rect_filled(box_rect, 0.0, Color32::from_rgba_unmultiplied(80, 160, 240, 45));
                        painter.rect_stroke(box_rect, 0.0, Stroke::new(2.0, Color32::from_rgb(80, 180, 255)), egui::StrokeKind::Inside);

                        let w = (world_pt.x - start.x).abs();
                        let d = (world_pt.y - start.y).abs();
                        let area_m2 = (w * d) / 1_000_000.0;
                        let area_ping = area_m2 * 0.3025;

                        painter.text(
                            box_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("空間區域: {:.1}x{:.1}m\n{:.2} 坪 ({:.1} m²)", w / 1000.0, d / 1000.0, area_ping, area_m2),
                            egui::FontId::monospace(13.0),
                            Color32::from_rgb(255, 230, 80),
                        );

                        if response.clicked() {
                            if w >= 200.0 && d >= 200.0 {
                                push_undo(undo_stack, doc);
                                let min_x = start.x.min(world_pt.x);
                                let max_x = start.x.max(world_pt.x);
                                let min_y = start.y.min(world_pt.y);
                                let max_y = start.y.max(world_pt.y);
                                let boundary = vec![
                                    Point2D::new(min_x, min_y),
                                    Point2D::new(max_x, min_y),
                                    Point2D::new(max_x, max_y),
                                    Point2D::new(min_x, max_y),
                                ];
                                let id = doc.insert_space(
                                    SpaceType::LivingRoom,
                                    boundary,
                                    0.0,
                                    3000.0,
                                    sketchcad_bim::Material::wood_flooring(),
                                );
                                *selected_id = Some(id);
                                *status_msg = format!("✔ 已建立空間區域 (坪數: {:.2} 坪 / {:.1} m²)", area_ping, area_m2);
                            }
                            self.space_start_pt = None;
                        }
                    } else {
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點選第一角點以劃定空間區域 (客廳/臥室/廚房/衛浴等)]",
                            egui::FontId::monospace(12.0),
                            Color32::from_rgb(100, 200, 255),
                        );
                        if response.clicked() {
                            self.space_start_pt = Some(world_pt);
                        }
                    }
                }
                ToolMode::Furniture => {
                    let kind = self.furniture_cycle_kind;
                    let (fw, fd, _fh) = kind.default_dimensions();
                    let default_mat = kind.default_material();

                    let rot = self.furniture_rotation;
                    let cos = rot.cos();
                    let sin = rot.sin();
                    let hw = fw * 0.5;
                    let hd = fd * 0.5;

                    let corners = [
                        world_pt + Vector2D::new(-hw * cos - -hd * sin, -hw * sin + -hd * cos),
                        world_pt + Vector2D::new(hw * cos - -hd * sin, hw * sin + -hd * cos),
                        world_pt + Vector2D::new(hw * cos - hd * sin, hw * sin + hd * cos),
                        world_pt + Vector2D::new(-hw * cos - hd * sin, -hw * sin + hd * cos),
                    ];
                    let scr_pts: Vec<Pos2> = corners.iter().map(|&p| to_screen(p)).collect();
                    for i in 0..4 {
                        painter.line_segment([scr_pts[i], scr_pts[(i + 1) % 4]], Stroke::new(1.8, Color32::from_rgb(255, 180, 80)));
                    }
                    painter.line_segment([scr_pts[0], scr_pts[2]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 180, 80, 100)));
                    painter.line_segment([scr_pts[1], scr_pts[3]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 180, 80, 100)));

                    let label_pos = to_screen(world_pt + Vector2D::new(0.0, -hd - 300.0));
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        format!("點擊放置 {} ({:.0}x{:.0}mm) [按 R 鍵旋轉方向]", kind.display_name(), fw, fd),
                        egui::FontId::monospace(12.0),
                        Color32::from_rgb(255, 200, 100),
                    );

                    if response.clicked() {
                        push_undo(undo_stack, doc);
                        let id = doc.insert_furniture(
                            kind,
                            world_pt,
                            0.0,
                            rot.to_degrees(),
                            default_mat,
                        );
                        *selected_id = Some(id);
                        *status_msg = format!("✔ 已放置室內家具: {} ({:.0}x{:.0}mm)", kind.display_name(), fw, fd);
                    }
                }
                ToolMode::DraftLine
                | ToolMode::DraftPolyline
                | ToolMode::DraftCircle
                | ToolMode::DraftArc
                | ToolMode::DraftSpline
                | ToolMode::DraftHatch => {}
                ToolMode::Erase
                | ToolMode::Copy
                | ToolMode::Trim
                | ToolMode::Extend
                | ToolMode::Mirror
                | ToolMode::Offset
                | ToolMode::Array
                | ToolMode::Rotate
                | ToolMode::Scale
                | ToolMode::Fillet
                | ToolMode::Chamfer => {}
                ToolMode::Dim
                | ToolMode::DimLinear
                | ToolMode::DimAligned
                | ToolMode::DimAngular
                | ToolMode::DimRadius
                | ToolMode::DimDiameter
                | ToolMode::QDim
                | ToolMode::DimContinue
                | ToolMode::DimBaseline
                | ToolMode::MText
                | ToolMode::MLeader => {}
                ToolMode::Model => {
                    match self.model_id.as_deref().and_then(sketchcad_bim::find_model) {
                        Some(model) => {
                            let rot_deg = self.furniture_rotation.to_degrees();
                            let preview = sketchcad_bim::ModelComponent::new(&model, world_pt, rot_deg);
                            let stroke = Stroke::new(1.8, Color32::from_rgb(120, 220, 160));
                            for prim in sketchcad_bim::BimElement::to_2d_primitives(&preview) {
                                let pts: Vec<Pos2> = prim.vertices.iter().map(|&p| to_screen(p)).collect();
                                for pair in pts.windows(2) {
                                    painter.line_segment([pair[0], pair[1]], stroke);
                                }
                                if prim.closed && pts.len() > 2 {
                                    painter.line_segment([pts[pts.len() - 1], pts[0]], stroke);
                                }
                            }
                            let (w, d, h) = preview.size();
                            painter.text(
                                to_screen(world_pt + Vector2D::new(0.0, -d * 0.5 - 300.0)),
                                egui::Align2::CENTER_CENTER,
                                format!("點擊放置 {} ({:.0}×{:.0}×{:.0}mm) [R 旋轉 90°]", model.name(), w, d, h),
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(150, 235, 180),
                            );
                            if response.clicked() {
                                push_undo(undo_stack, doc);
                                if let Some(id) = doc.insert_model(model.id(), world_pt, rot_deg) {
                                    *selected_id = Some(id);
                                    *status_msg = format!("✔ 已放置模型: {}", model.name());
                                }
                            }
                        }
                        None => {
                            painter.text(
                                to_screen(world_pt) + egui::vec2(0.0, -24.0),
                                egui::Align2::CENTER_CENTER,
                                "請先從「📦 3D 模型百寶箱」選擇要放置的模型",
                                egui::FontId::proportional(13.0),
                                Color32::from_rgb(255, 200, 100),
                            );
                        }
                    }
                }
                ToolMode::LightFixture => {
                    let l_type = self.light_cycle_type;
                    let (lumens, kelvin, watts, _) = l_type.default_specs();
                    let name = l_type.display_name();

                    let scr = to_screen(world_pt);
                    painter.circle_stroke(scr, 16.0, Stroke::new(2.0, Color32::from_rgb(255, 220, 90)));
                    painter.line_segment([Pos2::new(scr.x - 20.0, scr.y), Pos2::new(scr.x + 20.0, scr.y)], Stroke::new(1.5, Color32::from_rgb(255, 220, 90)));
                    painter.line_segment([Pos2::new(scr.x, scr.y - 20.0), Pos2::new(scr.x, scr.y + 20.0)], Stroke::new(1.5, Color32::from_rgb(255, 220, 90)));

                    let label_pos = to_screen(world_pt + Vector2D::new(0.0, -350.0));
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        format!("點擊安裝天花燈具: {} ({}K, {:.0} lm, {:.0}W)", name, kelvin, lumens, watts),
                        egui::FontId::monospace(12.0),
                        Color32::from_rgb(255, 230, 120),
                    );

                    if response.clicked() {
                        push_undo(undo_stack, doc);
                        let id = doc.insert_light_fixture(
                            l_type,
                            world_pt,
                            2800.0, // Standard ceiling height
                        );
                        *selected_id = Some(id);
                        *status_msg = format!("✔ 已安裝天花照明燈具: {} (天花高度 2800mm)", name);
                    }
                }
                ToolMode::Move => {
                    if let Some(target_id) = *selected_id {
                        if let Some(base_pt) = self.move_base_pt {
                            let p0 = to_screen(base_pt);
                            let p1 = to_screen(world_pt);
                            painter.line_segment([p0, p1], Stroke::new(2.5, Color32::from_rgb(255, 215, 0)));
                            painter.circle_filled(p0, 4.0, Color32::from_rgb(255, 215, 0));
                            painter.circle_filled(p1, 5.0, Color32::from_rgb(255, 255, 100));

                            let dx = world_pt.x - base_pt.x;
                            let dy = world_pt.y - base_pt.y;
                            let dist = base_pt.distance(world_pt);
                            let mid = Pos2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5 - 14.0);
                            painter.text(
                                mid,
                                egui::Align2::CENTER_CENTER,
                                format!("ΔX: {:.0} mm, ΔY: {:.0} mm (平移距離: {:.0} mm)", dx, dy, dist),
                                egui::FontId::monospace(13.0),
                                Color32::from_rgb(255, 220, 80),
                            );

                            if response.clicked() {
                                push_undo(undo_stack, doc);
                                doc.move_element(target_id, sketchcad_core::Vector3D::new(dx, dy, 0.0));
                                *status_msg = format!("✔ 已完成平移 (ΔX: {:.0} mm, ΔY: {:.0} mm)", dx, dy);
                                self.move_base_pt = None;
                            }
                        } else {
                            painter.text(
                                hover_pos,
                                egui::Align2::LEFT_TOP,
                                " [請在視口中點擊移動起點 (基準點)]",
                                egui::FontId::monospace(12.5),
                                Color32::from_rgb(255, 215, 0),
                            );
                            if response.clicked() {
                                self.move_base_pt = Some(world_pt);
                            }
                        }
                    } else {
                        let closest = self.pick_element(doc, hover_pos, world_pt, center);
                        painter.text(
                            hover_pos,
                            egui::Align2::LEFT_TOP,
                            " [請點選欲移動的 BIM 構件]",
                            egui::FontId::monospace(12.5),
                            Color32::from_rgb(255, 200, 100),
                        );
                        if response.clicked() {
                            *selected_id = closest;
                            if let Some(id) = closest {
                                self.move_base_pt = Some(world_pt);
                                if let Some(elem) = doc.get_element(id) {
                                    *status_msg = format!("已選定 {}，請點選目標位置完成平移", elem.name());
                                }
                            }
                        }
                    }
                }
                ToolMode::Select => {
                    let closest_id = self.pick_element(doc, hover_pos, world_pt, center);
                    if response.clicked() {
                        *selected_id = closest_id;
                        if let Some(id) = closest_id {
                            if let Some(elem) = doc.get_element(id) {
                                *status_msg = format!("選定構件: {}", elem.name());
                            }
                        }
                    }
                }
                ToolMode::Outlet => {
                    let wall_hit = find_closest_wall(doc, world_pt)
                        .filter(|(_, dist, ..)| *dist <= 800.0)
                        .and_then(|(wall_id, _, on_axis, dir, _)| {
                            let wall = doc.get_wall(wall_id)?;
                            // Mount on the wall face on the cursor's side
                            let norm = Vector2D::new(-dir.y, dir.x);
                            let side = if (world_pt - on_axis).dot(norm) >= 0.0 { 1.0 } else { -1.0 };
                            Some((wall_id, on_axis + norm * (side * wall.thickness * 0.5), norm * side))
                        });
                    match wall_hit {
                        Some((wall_id, face_pt, facing)) => {
                            let preview = Outlet::new(wall_id, face_pt, facing, self.outlet_standard, self.outlet_voltage, self.outlet_gangs);
                            for prim in sketchcad_bim::BimElement::to_2d_primitives(&preview) {
                                let pts: Vec<Pos2> = prim.vertices.iter().map(|&p| to_screen(p)).collect();
                                let stroke = Stroke::new(1.8, Color32::from_rgb(255, 170, 60));
                                if prim.closed {
                                    painter.add(egui::Shape::closed_line(pts, stroke));
                                } else {
                                    painter.add(egui::Shape::line(pts, stroke));
                                }
                            }
                            painter.text(
                                to_screen(face_pt + facing * 450.0),
                                egui::Align2::CENTER_CENTER,
                                format!("點擊安裝 {} (離地 300mm)", preview.name),
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(255, 190, 90),
                            );
                            if response.clicked() {
                                push_undo(undo_stack, doc);
                                if let Some(id) = doc.insert_outlet(wall_id, face_pt, facing, self.outlet_standard, self.outlet_voltage, self.outlet_gangs) {
                                    *selected_id = Some(id);
                                    *status_msg = format!("✔ 已安裝 {}", preview.name);
                                }
                            }
                        }
                        None => {
                            painter.text(
                                hover_pos,
                                egui::Align2::LEFT_TOP,
                                " [請移動至牆體旁以安裝插座]",
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(240, 180, 80),
                            );
                        }
                    }
                }
                ToolMode::Paint => {
                    // The app applies the material to the returned element
                    if response.clicked() {
                        clicked_element = self.pick_element(doc, hover_pos, world_pt, center);
                        if clicked_element.is_some() {
                            *selected_id = clicked_element;
                        }
                    }
                }
            }
        }
        self.render_text_editor(ui, doc, undo_stack, selected_id, status_msg);
        clicked_element
    }

    /// Zooms to `new_zoom` keeping the world point under `pivot` (screen) fixed.
    fn zoom_about(&mut self, new_zoom: f64, pivot: Pos2, center: Point2D) {
        let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let (dx, dy) = (pivot.x as f64 - center.x, pivot.y as f64 - center.y);
        let world_x = dx / self.camera.zoom + self.camera.pan_offset.x;
        let world_y = -dy / self.camera.zoom + self.camera.pan_offset.y;
        self.camera.zoom = new_zoom;
        self.camera.pan_offset = Point2D::new(world_x - dx / new_zoom, world_y + dy / new_zoom);
    }

    /// Starts (or extends) a smooth zoom by `factor` around `pivot`.
    fn zoom_smooth(&mut self, factor: f64, pivot: Pos2) {
        let current_target = self.zoom_anim.map_or(self.camera.zoom, |(z, _)| z);
        self.zoom_anim = Some(((current_target * factor).clamp(MIN_ZOOM, MAX_ZOOM), pivot));
    }

    fn pan_by_screen(&mut self, delta: egui::Vec2) {
        self.camera.pan_offset.x -= delta.x as f64 / self.camera.zoom;
        self.camera.pan_offset.y += delta.y as f64 / self.camera.zoom;
    }

    /// Mouse, trackpad and keyboard navigation:
    /// - wheel: zoom at cursor · trackpad two-finger scroll: pan · pinch / Ctrl(⌘)+scroll: zoom
    /// - drag with middle / right button, Space + left, ✋ pan mode, Select tool or the tiled
    ///   overview: pan
    /// - keys `+` / `-` zoom, arrows pan, `0` / Home fit the drawing
    fn handle_navigation(
        &mut self,
        ui: &Ui,
        response: &egui::Response,
        rect: Rect,
        center: Point2D,
        current_tool: ToolMode,
    ) {
        let space_down = ui.input(|i| i.key_down(egui::Key::Space));
        let left_pans = self.pan_mode || space_down || self.is_tiled() || matches!(current_tool, ToolMode::Select | ToolMode::Paint);
        let panning = response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (left_pans && response.dragged_by(egui::PointerButton::Primary));
        if panning {
            self.pan_by_screen(response.drag_delta());
            self.zoom_anim = None;
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if response.hovered() && (self.pan_mode || space_down) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let pivot = response.hover_pos().unwrap_or(rect.center());
        if response.hovered() {
            let (wheel_notches, trackpad_pan, pinch) = ui.input(|i| {
                let mut notches = 0.0f32;
                let mut pan = egui::Vec2::ZERO;
                for event in &i.events {
                    if let egui::Event::MouseWheel { unit, delta, modifiers } = event {
                        if modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
                            continue; // reported through zoom_delta()
                        }
                        match unit {
                            egui::MouseWheelUnit::Point => pan += *delta,
                            egui::MouseWheelUnit::Line => notches += delta.y,
                            egui::MouseWheelUnit::Page => notches += delta.y * 3.0,
                        }
                    }
                }
                (notches, pan, i.zoom_delta())
            });
            if wheel_notches != 0.0 {
                self.zoom_smooth(ZOOM_STEP.powf(wheel_notches as f64), pivot);
            }
            if trackpad_pan != egui::Vec2::ZERO {
                self.pan_by_screen(trackpad_pan);
            }
            if (pinch - 1.0).abs() > f32::EPSILON {
                self.zoom_anim = None;
                self.zoom_about(self.camera.zoom * pinch as f64, pivot, center);
            }

            if !ui.ctx().wants_keyboard_input() {
                let (zoom_in, zoom_out, fit, arrows) = ui.input(|i| {
                    let mut arrows = egui::Vec2::ZERO;
                    for (key, dir) in [
                        (egui::Key::ArrowLeft, egui::vec2(1.0, 0.0)),
                        (egui::Key::ArrowRight, egui::vec2(-1.0, 0.0)),
                        (egui::Key::ArrowUp, egui::vec2(0.0, 1.0)),
                        (egui::Key::ArrowDown, egui::vec2(0.0, -1.0)),
                    ] {
                        if i.key_down(key) {
                            arrows += dir;
                        }
                    }
                    (
                        i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                        i.key_pressed(egui::Key::Minus),
                        i.key_pressed(egui::Key::Num0) || i.key_pressed(egui::Key::Home),
                        arrows * (i.stable_dt.min(0.1) * 900.0),
                    )
                });
                if zoom_in {
                    self.zoom_smooth(ZOOM_STEP, pivot);
                }
                if zoom_out {
                    self.zoom_smooth(1.0 / ZOOM_STEP, pivot);
                }
                if fit {
                    self.frame_bounds(rect.size());
                }
                if arrows != egui::Vec2::ZERO {
                    self.pan_by_screen(arrows);
                    ui.ctx().request_repaint();
                }
            }
        }

        // Advance the smooth zoom animation
        if let Some((target, anim_pivot)) = self.zoom_anim {
            let dt = ui.input(|i| i.stable_dt.min(0.1)) as f64;
            let t = 1.0 - (-dt * 16.0).exp();
            let zoom = self.camera.zoom * (target / self.camera.zoom).powf(t);
            if (zoom / target - 1.0).abs() < 0.002 {
                self.zoom_about(target, anim_pivot, center);
                self.zoom_anim = None;
            } else {
                self.zoom_about(zoom, anim_pivot, center);
                ui.ctx().request_repaint();
            }
        }
    }

    /// Bottom-right overlay: zoom in / out, fit drawing, hand (pan) tool and the current scale.
    fn render_nav_controls(&mut self, ui: &mut Ui, rect: Rect, center: Point2D) {
        let size = egui::vec2(236.0, 30.0);
        let bar_rect = Rect::from_min_size(rect.right_bottom() - size - egui::vec2(10.0, 10.0), size);
        let mut action: Option<u8> = None;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(22, 26, 34, 230))
                .stroke(Stroke::new(1.0, Color32::from_rgb(64, 74, 92)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let button = |ui: &mut Ui, text: &str, tip: &str| {
                            ui.add(egui::Button::new(egui::RichText::new(text).size(14.0)).min_size(egui::vec2(26.0, 22.0)))
                                .on_hover_text(tip)
                                .clicked()
                        };
                        if button(ui, "＋", "放大 (+ / 滾輪向上)") {
                            action = Some(0);
                        }
                        if button(ui, "－", "縮小 (- / 滾輪向下)") {
                            action = Some(1);
                        }
                        if button(ui, "全圖", "顯示整張圖面 (0 / Home)") {
                            action = Some(2);
                        }
                        let hand = ui
                            .add(
                                egui::Button::new(egui::RichText::new("✋").size(14.0))
                                    .selected(self.pan_mode)
                                    .min_size(egui::vec2(26.0, 22.0)),
                            )
                            .on_hover_text("平移模式：左鍵拖曳移動圖面\n(也可用中鍵/右鍵拖曳、空白鍵+左鍵、觸控板雙指、方向鍵)");
                        if hand.clicked() {
                            action = Some(3);
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", self.camera.zoom * 1000.0))
                                .monospace()
                                .small()
                                .color(Color32::from_rgb(170, 185, 205)),
                        )
                        .on_hover_text("縮放比例 (100% = 1 m 顯示為 100 px)");
                    });
                });
        });
        let pivot = Pos2::new(center.x as f32, center.y as f32);
        match action {
            Some(0) => self.zoom_smooth(ZOOM_STEP, pivot),
            Some(1) => self.zoom_smooth(1.0 / ZOOM_STEP, pivot),
            Some(2) => self.frame_bounds(rect.size()),
            Some(3) => self.pan_mode = !self.pan_mode,
            _ => {}
        }
        if action.is_some() {
            ui.ctx().request_repaint();
        }
    }

    /// Element under the cursor: nearest drawn plan line within a few pixels, otherwise the
    /// parametric hit test (rooms, slab interiors) when editing a single sheet.
    fn pick_element(&self, doc: &BimDocument, screen: Pos2, world_pt: Point2D, center: Point2D) -> Option<ElementId> {
        const PICK_RADIUS_PX: f32 = 8.0;
        let zoom = self.camera.zoom;
        let pan = self.camera.pan_offset;
        let to_screen = |p: Point2D, off: Vector2D| {
            Pos2::new((center.x + (p.x + off.x - pan.x) * zoom) as f32, (center.y - (p.y + off.y - pan.y) * zoom) as f32)
        };
        let mut best: Option<(f32, u32)> = None;
        for (si, off) in self.sheet_layout() {
            for line in &self.plan.sheets[si].lines {
                let d = distance_to_segment(screen, to_screen(line.a, off), to_screen(line.b, off));
                if d <= PICK_RADIUS_PX && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, line.elem));
                }
            }
        }
        match best {
            Some((_, elem)) => self.plan.elem_ids.get(elem as usize).copied(),
            None if !self.is_tiled() => pick_closest_element(doc, world_pt),
            None => None,
        }
    }

    /// Overlay bar listing the floor-plan sheets: tiled overview or a single sheet.
    fn render_sheet_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let sheet_count = self.plan.sheets.len();
        if sheet_count <= 1 {
            return;
        }
        let bar_rect = Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(rect.width() - 16.0, 26.0));
        let mut requested = None;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let all = self.sheet_view == SheetView::All;
                    if ui
                        .selectable_label(all, egui::RichText::new(format!("▦ 全部圖紙 ({})", sheet_count)).small().strong())
                        .on_hover_text("並排顯示所有樓層平面圖（雙擊圖紙進入單張編輯）")
                        .clicked()
                    {
                        requested = Some(SheetView::All);
                    }
                    ui.separator();
                    for (i, sheet) in self.plan.sheets.iter().enumerate() {
                        let on = self.sheet_view == SheetView::Single(i);
                        if ui
                            .selectable_label(on, egui::RichText::new(&sheet.name).small())
                            .on_hover_text(format!("EL {:+.2} m 平面圖", sheet.label_elevation / 1000.0))
                            .clicked()
                        {
                            requested = Some(SheetView::Single(i));
                        }
                    }
                });
            });
        });
        if let Some(view) = requested {
            if view != self.sheet_view {
                // Picking a sheet from the bar re-frames it; double-click keeps the view in place
                self.open_sheet(view, rect.size());
            }
        }
    }

    /// Shows the given sheet view and frames it in a viewport of `viewport` points.
    pub fn open_sheet(&mut self, view: SheetView, viewport: egui::Vec2) {
        self.switch_sheet_view(view);
        self.frame_bounds(viewport);
    }

    fn snap(&self, pt: Point2D) -> Point2D {
        let snap = self.snap_grid_size;
        Point2D::new(
            (pt.x / snap).round() * snap,
            (pt.y / snap).round() * snap,
        )
    }

    fn draw_grid<F1, F2>(&self, painter: &egui::Painter, rect: Rect, to_screen: F1, to_world: F2)
    where
        F1: Fn(Point2D) -> Pos2,
        F2: Fn(Pos2) -> Point2D,
    {
        let top_left = to_world(rect.left_top());
        let bottom_right = to_world(rect.right_bottom());

        let min_x = top_left.x.min(bottom_right.x);
        let max_x = top_left.x.max(bottom_right.x);
        let min_y = top_left.y.min(bottom_right.y);
        let max_y = top_left.y.max(bottom_right.y);

        let grid_step = 1000.0; // 1 meter grid
        let start_x = (min_x / grid_step).floor() * grid_step;
        let start_y = (min_y / grid_step).floor() * grid_step;

        let mut x = start_x;
        while x <= max_x {
            let p0 = to_screen(Point2D::new(x, min_y));
            let p1 = to_screen(Point2D::new(x, max_y));
            painter.line_segment([p0, p1], Stroke::new(0.5, Color32::from_rgba_unmultiplied(60, 65, 75, 100)));
            x += grid_step;
        }

        let mut y = start_y;
        while y <= max_y {
            let p0 = to_screen(Point2D::new(min_x, y));
            let p1 = to_screen(Point2D::new(max_x, y));
            painter.line_segment([p0, p1], Stroke::new(0.5, Color32::from_rgba_unmultiplied(60, 65, 75, 100)));
            y += grid_step;
        }
    }

    /// Render 2D architectural CAD dimension lines, extension witness lines, 45° ticks, and distance text
    pub fn draw_dimensions(
        &self,
        painter: &egui::Painter,
        to_screen: &impl Fn(Point2D) -> Pos2,
        doc: &BimDocument,
        on_sheet: &dyn Fn(ElementId) -> bool,
    ) {
        // 1. Dimensions for Walls
        for elem in doc.get_elements_by_kind(ElementKind::Wall) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(wall) = doc.get_wall(elem.id()) {
                let len = wall.length();
                if len >= 100.0 {
                    draw_cad_dim_segment(
                        painter,
                        to_screen,
                        wall.start,
                        wall.end,
                        400.0,
                        None,
                    );
                }
            }
        }

        // 2. Dimensions for Slabs
        for elem in doc.get_elements_by_kind(ElementKind::Slab) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(slab) = doc.get_slab(elem.id()) {
                let p_origin = slab.origin;
                let p_x = p_origin + Point2D::new(slab.width, 0.0);
                let p_y = p_origin + Point2D::new(0.0, slab.depth);

                draw_cad_dim_segment(painter, to_screen, p_origin, p_x, -450.0, None);
                draw_cad_dim_segment(painter, to_screen, p_origin, p_y, 450.0, None);
            }
        }

        // 3. Dimensions for Columns
        for elem in doc.get_elements_by_kind(ElementKind::Column) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(col) = doc.get_column(elem.id()) {
                let scr = to_screen(col.position);
                let text = format!("□ {:.0}x{:.0}", col.width, col.depth);
                let font_id = egui::FontId::monospace(10.5);
                let galley = painter.layout_no_wrap(text, font_id, Color32::from_rgb(180, 220, 255));
                let rect = egui::Rect::from_center_size(
                    Pos2::new(scr.x, scr.y - 22.0),
                    galley.size() + egui::vec2(6.0, 3.0),
                );
                painter.rect_filled(rect, 2.0, Color32::from_rgba_unmultiplied(20, 24, 32, 210));
                painter.rect_stroke(rect, 2.0, Stroke::new(0.6, Color32::from_rgba_unmultiplied(100, 180, 255, 120)), egui::StrokeKind::Inside);
                painter.galley(rect.center() - galley.size() * 0.5, galley, Color32::from_rgb(180, 220, 255));
            }
        }

        // 4. Dimensions for Roofs
        for elem in doc.get_elements_by_kind(ElementKind::Roof) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(roof) = doc.get_roof(elem.id()) {
                let p_origin = roof.origin;
                let p_x = p_origin + Point2D::new(roof.width, 0.0);
                let p_y = p_origin + Point2D::new(0.0, roof.depth);

                draw_cad_dim_segment(painter, to_screen, p_origin, p_x, -500.0, Some(&format!("跨距 {:.0} (坡度 {:.0}°)", roof.width, roof.pitch_deg)));
                draw_cad_dim_segment(painter, to_screen, p_origin, p_y, 500.0, Some(&format!("進深 {:.0}", roof.depth)));
            }
        }

        // 5. Dimensions for Ramps
        for elem in doc.get_elements_by_kind(ElementKind::Ramp) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(ramp) = doc.get_ramp(elem.id()) {
                let len = ramp.length();
                if len >= 100.0 {
                    let slope_inv = if ramp.slope_ratio() > 1e-4 { (1.0 / ramp.slope_ratio()).round() } else { 12.0 };
                    draw_cad_dim_segment(
                        painter,
                        to_screen,
                        ramp.start,
                        ramp.end,
                        700.0,
                        Some(&format!("{:.0} (坡度 1:{:.0}, 升 {:.0})", len, slope_inv, ramp.total_rise)),
                    );

                }
            }
        }

        // 6. Dimensions for Curtain Walls
        for elem in doc.get_elements_by_kind(ElementKind::CurtainWall) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(cw) = doc.get_curtain_wall(elem.id()) {
                let len = cw.length();
                if len >= 100.0 {
                    let bays = (len / cw.u_spacing).round().max(1.0) as usize;
                    draw_cad_dim_segment(
                        painter,
                        to_screen,
                        cw.start,
                        cw.end,
                        -450.0,
                        Some(&format!("{:.0} ({} 跨 @ {:.0})", len, bays, len / (bays as f64))),
                    );
                }
            }
        }

        // 7. Dimensions for Stairs
        for elem in doc.get_elements_by_kind(ElementKind::Stairs) {
            if !on_sheet(elem.id()) {
                continue;
            }
            if let Some(stair) = doc.get_stairs(elem.id()) {
                let len = stair.length();
                if len >= 100.0 {
                    draw_cad_dim_segment(
                        painter,
                        to_screen,
                        stair.start,
                        stair.end,
                        650.0,
                        Some(&format!("{:.0} ({} 階 @ {:.0})", len, stair.num_steps, stair.tread_depth())),
                    );
                }
            }
        }
    }
}

/// Draw architectural CAD dimension line with witness extension lines, 45-degree ticks, and text pill
pub fn draw_cad_dim_segment<F: Fn(Point2D) -> Pos2>(
    painter: &egui::Painter,
    to_screen: &F,
    p1: Point2D,
    p2: Point2D,
    offset_dist: f64,
    override_text: Option<&str>,
) {
    let v = p2 - p1;
    let len = v.length();
    if len < 40.0 {
        return;
    }
    let dir = v / len;
    let norm = Vector2D::new(-dir.y, dir.x);

    let p1_dim = p1 + norm * offset_dist;
    let p2_dim = p2 + norm * offset_dist;

    let s_p1_dim = to_screen(p1_dim);
    let s_p2_dim = to_screen(p2_dim);

    // Witness (extension) lines extending from element boundary slightly past the dimension line
    let sign = if offset_dist >= 0.0 { 1.0 } else { -1.0 };
    let ext_start_1 = to_screen(p1 + norm * (offset_dist * 0.15));
    let ext_end_1 = to_screen(p1 + norm * (offset_dist + 160.0 * sign));
    let ext_start_2 = to_screen(p2 + norm * (offset_dist * 0.15));
    let ext_end_2 = to_screen(p2 + norm * (offset_dist + 160.0 * sign));

    let witness_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 190, 240, 150));
    painter.line_segment([ext_start_1, ext_end_1], witness_stroke);
    painter.line_segment([ext_start_2, ext_end_2], witness_stroke);

    // Dimension line
    let dim_stroke = Stroke::new(1.3, Color32::from_rgba_unmultiplied(100, 210, 255, 230));
    painter.line_segment([s_p1_dim, s_p2_dim], dim_stroke);

    // Architectural 45-degree slash ticks at dimension endpoints
    let tick_vec = egui::vec2(4.5, -4.5);
    let tick_stroke = Stroke::new(2.0, Color32::from_rgb(255, 220, 90));
    painter.line_segment([s_p1_dim - tick_vec, s_p1_dim + tick_vec], tick_stroke);
    painter.line_segment([s_p2_dim - tick_vec, s_p2_dim + tick_vec], tick_stroke);

    // Dimension Text Badge
    let mid_pos = Pos2::new((s_p1_dim.x + s_p2_dim.x) * 0.5, (s_p1_dim.y + s_p2_dim.y) * 0.5);
    let text = match override_text {
        Some(t) => t.to_string(),
        None => format!("{:.0}", len),
    };

    let font_id = egui::FontId::monospace(11.0);
    let text_galley = painter.layout_no_wrap(text, font_id, Color32::from_rgb(255, 235, 140));
    let text_rect = egui::Rect::from_center_size(mid_pos, text_galley.size() + egui::vec2(8.0, 4.0));
    painter.rect_filled(text_rect, 3.0, Color32::from_rgba_unmultiplied(20, 24, 32, 225));
    painter.rect_stroke(text_rect, 3.0, Stroke::new(0.8, Color32::from_rgba_unmultiplied(100, 210, 255, 120)), egui::StrokeKind::Inside);
    painter.galley(mid_pos - text_galley.size() * 0.5, text_galley, Color32::from_rgb(255, 235, 140));
}


/// Helper: Finds the closest Wall element to a given world point
pub fn find_closest_wall(
    doc: &BimDocument,
    pt: Point2D,
) -> Option<(ElementId, f64, Point2D, Vector2D, f64)> {
    let mut best: Option<(ElementId, f64, Point2D, Vector2D, f64)> = None;

    for elem in doc.get_elements_by_kind(ElementKind::Wall) {
        if let Some(wall) = elem.as_any().downcast_ref::<Wall>() {
            let len = wall.length();
            if len <= 1.0 {
                continue;
            }
            let dir = wall.direction();
            let v = pt - wall.start;
            let proj = v.x * dir.x + v.y * dir.y;
            let offset = proj.clamp(0.0, len);
            let closest_pt = wall.start + dir * offset;
            let dist = pt.distance(closest_pt);

            if let Some((_, best_dist, _, _, _)) = best {
                if dist < best_dist {
                    best = Some((wall.id, dist, closest_pt, dir, offset));
                }
            } else {
                best = Some((wall.id, dist, closest_pt, dir, offset));
            }
        }
    }

    best
}

/// Helper: Finds the closest BIM element (Wall, Column, Door, Window, Slab, Ceiling, Stairs, Railing)
pub fn pick_closest_element(doc: &BimDocument, world_pt: Point2D) -> Option<ElementId> {
    let mut closest_id = None;
    let mut min_dist = 700.0;

    for wall in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Wall) {
        if let Some(w) = doc.get_wall(wall.id()) {
            let v = w.end - w.start;
            let len = v.length();
            if len > 1e-4 {
                let dir = v / len;
                let proj = (world_pt - w.start).dot(dir).clamp(0.0, len);
                let d = world_pt.distance(w.start + dir * proj);
                if d < min_dist {
                    min_dist = d;
                    closest_id = Some(w.id);
                }
            }
        }
    }

    for col in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Column) {
        if let Some(c) = doc.get_column(col.id()) {
            let d = world_pt.distance(c.position);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(c.id);
            }
        }
    }

    for door in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Door) {
        if let Some(d_elem) = doc.get_door(door.id()) {
            let d = world_pt.distance(d_elem.world_pos);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(d_elem.id);
            }
        }
    }

    for win in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Window) {
        if let Some(w_elem) = doc.get_window(win.id()) {
            let d = world_pt.distance(w_elem.world_pos);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(w_elem.id);
            }
        }
    }

    for slab in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Slab) {
        if let Some(s) = doc.get_slab(slab.id()) {
            let center = s.origin + Point2D::new(s.width * 0.5, s.depth * 0.5);
            let d = world_pt.distance(center);
            if d < min_dist || (world_pt.x >= s.origin.x && world_pt.x <= s.origin.x + s.width && world_pt.y >= s.origin.y && world_pt.y <= s.origin.y + s.depth) {
                min_dist = d.min(min_dist);
                closest_id = Some(s.id);
            }
        }
    }

    for ceil in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Ceiling) {
        if let Some(c) = doc.get_ceiling(ceil.id()) {
            let center = c.origin + Point2D::new(c.width * 0.5, c.depth * 0.5);
            let d = world_pt.distance(center);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(c.id);
            }
        }
    }

    for stair in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Stairs) {
        if let Some(st) = doc.get_stairs(stair.id()) {
            let mid = (st.start + st.end) * 0.5;
            let d = world_pt.distance(mid);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(st.id);
            }
        }
    }

    for railing in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Railing) {
        if let Some(r) = doc.get_railing(railing.id()) {
            let start_2d = Point2D::new(r.start.x, r.start.y);
            let end_2d = Point2D::new(r.end.x, r.end.y);
            let v = end_2d - start_2d;
            let len = v.length();
            if len > 1e-4 {
                let dir = v / len;
                let proj = (world_pt - start_2d).dot(dir).clamp(0.0, len);
                let d = world_pt.distance(start_2d + dir * proj);
                if d < min_dist {
                    min_dist = d;
                    closest_id = Some(r.id);
                }
            }
        }
    }

    for roof in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Roof) {
        if let Some(r) = doc.get_roof(roof.id()) {
            let center = r.origin + Point2D::new(r.width * 0.5, r.depth * 0.5);
            let d = world_pt.distance(center);
            if d < min_dist || (world_pt.x >= r.origin.x && world_pt.x <= r.origin.x + r.width && world_pt.y >= r.origin.y && world_pt.y <= r.origin.y + r.depth) {
                min_dist = d.min(min_dist);
                closest_id = Some(r.id);
            }
        }
    }

    for ramp in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Ramp) {
        if let Some(rm) = doc.get_ramp(ramp.id()) {
            let mid = (rm.start + rm.end) * 0.5;
            let d = world_pt.distance(mid);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(rm.id);
            }
        }
    }


    for cw in doc.get_elements_by_kind(sketchcad_bim::ElementKind::CurtainWall) {
        if let Some(c) = doc.get_curtain_wall(cw.id()) {
            let v = c.end - c.start;
            let len = v.length();
            if len > 1e-4 {
                let dir = v / len;
                let proj = (world_pt - c.start).dot(dir).clamp(0.0, len);
                let d = world_pt.distance(c.start + dir * proj);
                if d < min_dist {
                    min_dist = d;
                    closest_id = Some(c.id);
                }
            }
        }
    }

    for space in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Space) {
        if let Some(sp) = doc.get_space(space.id()) {
            let d = world_pt.distance(sp.centroid());
            if d < min_dist {
                min_dist = d;
                closest_id = Some(sp.id);
            }
        }
    }

    for furn in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Furniture) {
        if let Some(f) = doc.get_furniture(furn.id()) {
            let d = world_pt.distance(f.position);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(f.id);
            }
        }
    }

    for outlet in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Outlet) {
        if let Some(o) = doc.get_outlet(outlet.id()) {
            let d = world_pt.distance(o.position);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(o.id);
            }
        }
    }

    for model in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Model) {
        if let Some(m) = doc.get_model(model.id()) {
            let d = world_pt.distance(m.position);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(m.id);
            }
        }
    }

    for light in doc.get_elements_by_kind(sketchcad_bim::ElementKind::LightFixture) {
        if let Some(l) = doc.get_light_fixture(light.id()) {
            let d = world_pt.distance(l.position);
            if d < min_dist {
                min_dist = d;
                closest_id = Some(l.id);
            }
        }
    }

    for elem in doc.get_elements_by_kind(sketchcad_bim::ElementKind::Annotation) {
        if let Some(a) = doc.get_annotation(elem.id()) {
            let dist = a.distance_to(world_pt).max(1.0);
            if dist < min_dist {
                min_dist = dist;
                closest_id = Some(a.id);
            }
        }
    }

    // Drafting last: slabs claim any click inside them, line work right under the cursor wins
    for kind in [sketchcad_bim::ElementKind::Draft, sketchcad_bim::ElementKind::Hatch] {
        for elem in doc.get_elements_by_kind(kind) {
            if let Some(d) = doc.get_draft(elem.id()) {
                // Inside a hatch counts as "near" but loses to line work right under the cursor
                let dist = match d.distance_to(world_pt) {
                    0.0 => 250.0,
                    dist => dist,
                };
                if dist < min_dist {
                    min_dist = dist;
                    closest_id = Some(d.id);
                }
            }
        }
    }
    closest_id
}

/// Floating picker shown above a selected door to flip its hinge side and swing side.
fn render_door_swing_popup(
    ui: &Ui,
    rect: Rect,
    to_screen: &impl Fn(Point2D) -> Pos2,
    doc: &mut BimDocument,
    id: ElementId,
    undo_stack: &mut Vec<BimDocument>,
    status_msg: &mut String,
) {
    let Some(door) = doc.get_door(id) else { return };
    let dir = if door.wall_dir.length_squared() > 1e-6 { door.wall_dir.normalize() } else { Vector2D::X };
    let norm = Vector2D::new(-dir.y, dir.x);
    let side = if door.swing.is_inward() { norm } else { -norm };
    let threshold_mid = door.world_pos + dir * (door.width * 0.5);
    let anchor = to_screen(threshold_mid);
    if !rect.contains(anchor) {
        return;
    }
    let current = door.swing;
    // Place the picker on the wall side opposite the swing so it never covers the arc
    let swing_screen = to_screen(threshold_mid + side * door.width) - anchor;
    let opens_upward = swing_screen.y < 0.0;
    let (pivot, y) = if opens_upward {
        (egui::Align2::CENTER_TOP, anchor.y + 22.0)
    } else {
        (egui::Align2::CENTER_BOTTOM, anchor.y - 22.0)
    };
    let pos = Pos2::new(
        anchor.x.clamp(rect.left() + 150.0, rect.right() - 150.0),
        y.clamp(rect.top() + 70.0, rect.bottom() - 50.0),
    );

    let mut chosen = None;
    egui::Area::new(egui::Id::new("door_swing_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .pivot(pivot)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("開門方向").small().strong());
                    ui.separator();
                    let mut pick = |ui: &mut Ui, text: &str, tip: &str, swing: DoorSwing| {
                        if ui.selectable_label(current == swing, text).on_hover_text(tip).clicked() && swing != current {
                            chosen = Some(swing);
                        }
                    };
                    pick(ui, "左開", "鉸鍊在左側", DoorSwing::from_parts(true, current.is_inward()));
                    pick(ui, "右開", "鉸鍊在右側", DoorSwing::from_parts(false, current.is_inward()));
                    ui.separator();
                    pick(ui, "內開", "門扇往室內側開啟", DoorSwing::from_parts(current.is_left(), true));
                    pick(ui, "外開", "門扇往室外側開啟", DoorSwing::from_parts(current.is_left(), false));
                });
            });
        });

    if let Some(swing) = chosen {
        push_undo(undo_stack, doc);
        if let Some(door) = doc.get_door_mut(id) {
            door.swing = swing;
            *status_msg = format!("✔ 已設定開門方向：{}", swing.label());
        }
    }
}

/// Screen line weight / color per drafting line category (dark CAD background).
fn line_stroke(style: LineStyle) -> Stroke {
    match style {
        LineStyle::Cut => Stroke::new(2.2, Color32::from_rgb(240, 242, 246)),
        LineStyle::Symbol => Stroke::new(1.3, Color32::from_rgb(200, 206, 216)),
        LineStyle::Projection => Stroke::new(1.0, Color32::from_rgb(128, 138, 154)),
        LineStyle::Furniture => Stroke::new(1.0, Color32::from_rgb(118, 152, 182)),
        LineStyle::Fixture => Stroke::new(0.9, Color32::from_rgba_unmultiplied(225, 190, 110, 170)),
        LineStyle::Overhead => Stroke::new(0.9, Color32::from_rgba_unmultiplied(150, 152, 165, 150)),
        LineStyle::Boundary => Stroke::new(0.8, Color32::from_rgba_unmultiplied(80, 150, 220, 120)),
        LineStyle::Draft => Stroke::new(1.4, Color32::from_rgb(245, 245, 250)),
        LineStyle::Hatch => Stroke::new(0.7, Color32::from_rgba_unmultiplied(170, 185, 205, 190)),
        LineStyle::Annotation => Stroke::new(1.0, ANNOTATION_COLOR),
    }
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    let t = if len_sq > 1e-6 { ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_under(vp: &Viewport2D, screen: Pos2, center: Point2D) -> Point2D {
        Point2D::new(
            (screen.x as f64 - center.x) / vp.camera.zoom + vp.camera.pan_offset.x,
            -(screen.y as f64 - center.y) / vp.camera.zoom + vp.camera.pan_offset.y,
        )
    }

    #[test]
    fn zoom_keeps_point_under_cursor_fixed() {
        let mut vp = Viewport2D::new();
        let center = Point2D::new(400.0, 300.0);
        let cursor = Pos2::new(620.0, 140.0);
        let before = world_under(&vp, cursor, center);
        for factor in [ZOOM_STEP, ZOOM_STEP, 1.0 / ZOOM_STEP, 7.0] {
            vp.zoom_about(vp.camera.zoom * factor, cursor, center);
            assert!(world_under(&vp, cursor, center).distance(before) < 1e-6);
        }
    }

    #[test]
    fn zoom_is_clamped_and_smooth_target_accumulates() {
        let mut vp = Viewport2D::new();
        let start = vp.camera.zoom;
        vp.zoom_smooth(ZOOM_STEP, Pos2::ZERO);
        vp.zoom_smooth(ZOOM_STEP, Pos2::ZERO);
        let (target, _) = vp.zoom_anim.unwrap();
        assert!((target - start * ZOOM_STEP * ZOOM_STEP).abs() < 1e-12);

        vp.zoom_about(1e9, Pos2::ZERO, Point2D::ZERO);
        assert_eq!(vp.camera.zoom, MAX_ZOOM);
        vp.zoom_about(1e-12, Pos2::ZERO, Point2D::ZERO);
        assert_eq!(vp.camera.zoom, MIN_ZOOM);
    }
}

#[cfg(test)]
mod draft_tests {
    use super::*;
    use sketchcad_bim::{DraftGeometry, Material};

    #[test]
    fn dimension_helpers_follow_autocad_conventions() {
        use sketchcad_bim::DimAxis;
        let (p1, p2) = (Point2D::new(0.0, 0.0), Point2D::new(3000.0, 1000.0));
        assert_eq!(Viewport2D::auto_axis(p1, p2, Point2D::new(1500.0, 2500.0)), DimAxis::Horizontal, "dragged above: horizontal");
        assert_eq!(Viewport2D::auto_axis(p1, p2, Point2D::new(4500.0, 500.0)), DimAxis::Vertical, "dragged beside: vertical");

        // Two lines crossing at the origin; the cursor in the upper-right quadrant measures 90°
        let (start, sweep) = angle_between_rays(Vector2D::X, Vector2D::Y, Vector2D::new(1.0, 1.0), true);
        assert!((sweep - 90.0).abs() < 1e-9 && start.abs() < 1e-9);
        // Lower-left quadrant: still 90°, between −X and −Y
        let (start, sweep) = angle_between_rays(Vector2D::X, Vector2D::Y, Vector2D::new(-1.0, -1.0), true);
        assert!((sweep - 90.0).abs() < 1e-9 && (start.rem_euclid(360.0) - 180.0).abs() < 1e-9);
        // Vertex mode: the reflex side when the cursor is outside the 90° angle
        let (_, sweep) = angle_between_rays(Vector2D::X, Vector2D::Y, Vector2D::new(-1.0, -1.0), false);
        assert!((sweep - 270.0).abs() < 1e-9);

        let mut doc = BimDocument::new("QDIM");
        doc.insert_draft(sketchcad_bim::DraftGeometry::Line { start: Point2D::new(0.0, 0.0), end: Point2D::new(1000.0, 0.0) }, 0.0);
        doc.insert_column(Point2D::new(2500.0, 0.0), 400.0, 400.0, 3000.0, Material::concrete());
        doc.insert_column(Point2D::new(9000.0, 0.0), 400.0, 400.0, 3000.0, Material::concrete());
        let pts = qdim_points(&doc, Point2D::new(-100.0, -100.0), Point2D::new(3000.0, 100.0));
        assert_eq!(pts.len(), 3, "line ends and the column inside the window: {pts:?}");
    }

    #[test]
    fn hatch_picks_the_innermost_closed_region() {
        let mut doc = BimDocument::new("Hatch");
        doc.insert_slab(Point2D::new(0.0, 0.0), 10_000.0, 8_000.0, 200.0, -200.0, Material::concrete());
        doc.insert_draft(DraftGeometry::Circle { center: Point2D::new(3000.0, 3000.0), radius: 1000.0 }, 0.0);
        // An open polyline is not a region
        doc.insert_draft(DraftGeometry::Polyline { points: vec![Point2D::new(6000.0, 1000.0), Point2D::new(9000.0, 1000.0), Point2D::new(9000.0, 4000.0)], closed: false }, 0.0);

        let in_circle = hatch_region_at(&doc, Point2D::new(3200.0, 3000.0)).unwrap();
        assert_eq!(in_circle.len(), 64, "the circle, not the slab around it");
        let in_slab = hatch_region_at(&doc, Point2D::new(8000.0, 2000.0)).unwrap();
        assert_eq!(in_slab.len(), 4, "falls back to the slab outline");
        assert!(hatch_region_at(&doc, Point2D::new(20_000.0, 0.0)).is_none());

        let id = doc.insert_draft(DraftGeometry::Hatch { boundary: in_circle, pattern: sketchcad_bim::HatchPattern::Brick, spacing: 100.0, angle_deg: 0.0 }, 0.0);
        assert_eq!(pick_closest_element(&doc, Point2D::new(3000.0, 3000.0)), Some(id), "clicking inside a hatch selects it");
    }
}
