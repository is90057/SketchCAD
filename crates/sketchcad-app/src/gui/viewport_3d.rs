use crate::gui::doc_watch::DocWatcher;
use crate::gui::plan_sheets::{main_cluster, CLUSTER_MARGIN};
use crate::gui::gpu_scene::{EdgeVertex, FaceVertex, GpuScene, SceneGeometry, SceneUniforms, TranslucentRange};
use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Stroke, Ui};
use sketchcad_bim::{BimDocument, ElementKind};
use sketchcad_core::{ElementId, Point2D, Point3D, Vector2D, Vector3D};
use sketchcad_render::OrbitCamera3D;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionScale {
    Standard, // 1.0x 標準清晰度 (Standard HD)
    High,     // 1.5x 高清細緻 (1.5x Supersampling)
    Ultra,    // 2.0x 極致雙倍取樣 (Retina 2.0x SSAA)
}

impl ResolutionScale {
    pub fn factor(&self) -> f32 {
        match self {
            ResolutionScale::Standard => 1.0,
            ResolutionScale::High => 1.5,
            ResolutionScale::Ultra => 2.0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ResolutionScale::Standard => "1.0x 標準清晰 (Standard HD)",
            ResolutionScale::High => "1.5x 高清細緻 (1.5x SSAA)",
            ResolutionScale::Ultra => "2.0x 極致雙倍取樣 (Retina 2.0x)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShadingMode {
    Realistic,  // 擬真實體材質與日光著色 (Realistic Shaded with Material Colors)
    HiddenLine, // 建築消隱線框技術工程藍圖 (Technical Architectural Hidden-Line)
    XRay,       // X-Ray 半透明空間透視模式 (Semi-Transparent X-Ray)
}

#[derive(Debug, Clone)]
pub struct Viewport3DSettings {
    pub resolution_scale: ResolutionScale,
    pub shading_mode: ShadingMode,
    pub edge_stroke_width: f32,
    pub transparent_ceiling: bool,
    pub show_silhouette_edges: bool,
    pub show_ground_grid: bool,
    pub show_settings_panel: bool,
    pub enable_section_cut: bool,
    pub section_axis: u8, // 0 = X, 1 = Y, 2 = Z
    pub section_offset: f32,
    pub section_invert: bool,
}

impl Default for Viewport3DSettings {
    fn default() -> Self {
        Self {
            resolution_scale: ResolutionScale::Ultra,
            shading_mode: ShadingMode::Realistic,
            edge_stroke_width: 1.0,
            transparent_ceiling: true, // Prevents ceiling from blocking interior stairs
            show_silhouette_edges: true,
            show_ground_grid: false, // Default false: clean architectural rendering without ground grid slicing through rooms
            show_settings_panel: false,
            enable_section_cut: false,
            section_axis: 2, // Z-axis plan cut
            section_offset: 2600.0, // 2600mm cut height
            section_invert: false,
        }
    }
}

/// Near clip plane distance (mm) for the reverse-Z perspective projection.
const NEAR_PLANE_MM: f64 = 10.0;
/// Upper bound on offscreen render pixels (before 4x MSAA) to keep supersampling affordable.
const MAX_RENDER_PIXELS: f32 = 3840.0 * 2400.0;
/// Dihedral angle above which a shared triangle edge is drawn as a feature line.
const CREASE_ANGLE_DEG: f64 = 25.0;

/// A world-space triangle kept on the CPU for click picking.
struct PickTri {
    v: [Point3D; 3],
    elem: u32,
}

/// Scene geometry derived from the document; rebuilt only when its fingerprint changes.
struct SceneCache {
    fingerprint: u64,
    elem_ids: Vec<ElementId>,
    pick_tris: Vec<PickTri>,
    /// Per element: translucent vertex range and the centroid used for depth sorting.
    translucent: Vec<(TranslucentRange, Point3D)>,
    /// `translucent` ranges ordered back-to-front for `sorted_for_eye`.
    translucent_order: Vec<TranslucentRange>,
    sorted_for_eye: Option<Point3D>,
    triangle_count: usize,
}

/// Model-library edits made in the 3D view, applied by the app (the view only reads the
/// document while drawing).
#[derive(Debug, Clone, PartialEq)]
pub enum ModelAction {
    Place { model_id: String, position: Point2D, base_z: f64, rotation_deg: f64 },
    /// A drag-move starts (the app records an undo step)
    BeginMove(ElementId),
    MoveTo { id: ElementId, position: Point2D, base_z: f64 },
    Rotate(ElementId, f64),
    Duplicate(ElementId),
    Delete(ElementId),
}

/// A model being dragged in the 3D view.
struct ModelDrag {
    id: ElementId,
    /// Height of the horizontal plane the grab point slides on
    plane_z: f64,
    /// Model position minus the grab point (keeps the model under the cursor)
    offset: Vector2D,
    start_base_z: f64,
    vertical_px: f32,
}

/// Where a model would be placed under the cursor.
struct Placement {
    position: Point2D,
    base_z: f64,
    rotation_deg: f64,
    on_wall: bool,
}

pub struct Viewport3D {
    pub camera: OrbitCamera3D,
    /// Library model being placed with the Model tool (set by the app every frame)
    pub placing_model: Option<String>,
    pub placing_rotation_deg: f64,
    model_actions: Vec<ModelAction>,
    model_drag: Option<ModelDrag>,
    pub push_pull_distance: f64,
    pub settings: Viewport3DSettings,
    gpu: Option<GpuScene>,
    cache: Option<SceneCache>,
    doc_watcher: DocWatcher,
    /// Animated camera turn requested from the ViewCube: target (yaw, pitch)
    view_anim: Option<(f64, f64)>,
    /// Bounding box of the visible model, used for the section-cut slider range
    scene_bounds: Option<(Point3D, Point3D)>,
}

/// Plan-style cut height above a floor level for the Z section (mm)
const SECTION_CUT_ABOVE_FLOOR: f64 = 1200.0;

/// Default "home" orientation (isometric-like view from the south-east)
const HOME_YAW: f64 = 0.75;
const HOME_PITCH: f64 = 0.45;
const MAX_PITCH: f64 = 1.45;

impl Viewport3D {
    pub fn new() -> Self {
        Self {
            camera: OrbitCamera3D {
                target: Point3D::new(4000.0, 3000.0, 1500.0),
                distance: 14000.0,
                yaw_rad: 0.75,
                pitch_rad: 0.55,
                fov_y_rad: 50.0f64.to_radians(),
            },
            push_pull_distance: 500.0,
            settings: Viewport3DSettings::default(),
            gpu: None,
            cache: None,
            doc_watcher: DocWatcher::default(),
            view_anim: None,
            scene_bounds: None,
            placing_model: None,
            placing_rotation_deg: 0.0,
            model_actions: Vec::new(),
            model_drag: None,
        }
    }

    /// Model-library edits requested in the 3D view since the last call.
    pub fn take_model_actions(&mut self) -> Vec<ModelAction> {
        std::mem::take(&mut self.model_actions)
    }

    /// Enables the depth-buffered GPU renderer. Without it the viewport falls back to the
    /// (slower, approximate) painter's-algorithm software renderer.
    pub fn attach_gpu(&mut self, render_state: Option<&egui_wgpu::RenderState>) {
        self.gpu = render_state.map(GpuScene::new);
        self.cache = None;
    }

    pub fn frame_document(&mut self, doc: &BimDocument) {
        // Frame the building: structural elements when present, ignoring stray far-away ones
        let mut boxes = Vec::new();
        let mut all = Vec::new();
        for el in doc.all_elements() {
            if let Some(b) = el.to_3d_mesh().bbox() {
                all.push(b);
                if matches!(el.kind(), ElementKind::Wall | ElementKind::Column | ElementKind::Beam | ElementKind::Door | ElementKind::Window | ElementKind::Stairs | ElementKind::Roof) {
                    boxes.push(b);
                }
            }
        }
        if boxes.is_empty() {
            boxes = all;
        }
        let footprints: Vec<_> = boxes.iter().map(|b| (b.min.truncate(), b.max.truncate(), 1)).collect();
        let members = main_cluster(&footprints, CLUSTER_MARGIN);
        if members.is_empty() {
            return;
        }
        let (f_min, f_max) = members.iter().fold(
            (Point3D::splat(f64::INFINITY), Point3D::splat(f64::NEG_INFINITY)),
            |(lo, hi), &i| (lo.min(boxes[i].min), hi.max(boxes[i].max)),
        );
        let center = (f_min + f_max) * 0.5;
        let size = f_max - f_min;
        let max_dim = size.x.max(size.y).max(size.z).max(1500.0);
        self.camera.target = center;
        self.camera.distance = (max_dim * 1.5).clamp(2000.0, 150_000.0);
        self.camera.yaw_rad = HOME_YAW;
        self.camera.pitch_rad = HOME_PITCH;
    }

    /// Draws the 3D view. Returns the element hit by a click in the scene this frame.
    pub fn render(&mut self, ui: &mut Ui, doc: &BimDocument, selected_id: &mut Option<ElementId>) -> Option<ElementId> {
        let avail = ui.available_size();
        let size = egui::Vec2::new(avail.x.max(100.0), avail.y.max(100.0));
        let (response, painter) = ui.allocate_painter(
            size,
            egui::Sense::click_and_drag(),
        );

        let rect = response.rect;

        // ViewCube geometry & interactive bounds (Top-Left of 3D Viewport)
        let cube_center = Pos2::new(rect.min.x + 58.0, rect.min.y + 88.0);
        let cube_rect = Rect::from_center_size(cube_center, egui::vec2(84.0, 84.0));
        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
        let mouse_over_cube = mouse_pos.map_or(false, |p| cube_rect.contains(p));

        // 0. Drag-moving the selected library model (Shift = up / down)
        self.handle_model_drag(ui, &response, rect, doc, *selected_id, mouse_over_cube);

        // 1. Interactive Orbit, Pan & Zoom
        if self.model_drag.is_none() && response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            let rot_speed = if mouse_over_cube { 0.012 } else { 0.008 };
            self.camera.yaw_rad -= delta.x as f64 * rot_speed;
            self.camera.pitch_rad = (self.camera.pitch_rad + delta.y as f64 * rot_speed)
                .clamp(-MAX_PITCH, MAX_PITCH);
            self.view_anim = None;
        } else if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            // Grab-style pan in the screen plane: the point under the cursor follows the mouse
            let delta = response.drag_delta();
            let (_, _, right, up) = self.camera_basis();
            let world_per_px = 2.0 * self.camera.distance * (self.camera.fov_y_rad * 0.5).tan() / rect.height().max(1.0) as f64;
            self.camera.target += (right * (-delta.x as f64) + up * (delta.y as f64)) * world_per_px;
        }
        self.advance_view_animation(ui.ctx());

        // Smooth (animated) wheel zoom plus trackpad pinch zoom
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let mut factor = (-(scroll as f64) * 0.0018).exp();
            if (pinch - 1.0).abs() > f32::EPSILON {
                factor /= pinch as f64;
            }
            if (factor - 1.0).abs() > 1e-6 {
                self.camera.distance = (self.camera.distance * factor).clamp(500.0, 100_000.0);
            }
        }

        let clicked_scene = response.clicked() && !mouse_over_cube;
        let total_drawn_polys = if self.gpu.is_some() {
            self.render_scene_gpu(ui, &painter, rect, doc, selected_id, mouse_pos, clicked_scene)
        } else {
            self.render_scene_software(&painter, rect, doc, selected_id, mouse_pos, clicked_scene)
        };

        // 7.5 Model library: placement preview, and the toolbar of a selected model
        if self.placing_model.is_some() {
            self.render_model_placement(&painter, rect, mouse_pos.filter(|p| rect.contains(*p) && !mouse_over_cube), clicked_scene);
        } else if let Some(id) = *selected_id {
            self.render_model_toolbar(ui, rect, doc, id);
        }

        // 7.6 Render Interactive 3D ViewCube (導覽立方體)
        self.render_viewcube(ui.ctx(), &painter, cube_center, mouse_pos, response.clicked() && mouse_over_cube);

        // 8. Viewport Top Status HUD & Settings Button
        let hud_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 12.0, rect.min.y + 12.0),
            egui::vec2(rect.width() - 24.0, 32.0),
        );

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(hud_rect), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "3D 空間實體模型 | 渲染多邊形: {} | 視距: {:.0}mm | 推拉: {:.0}mm | 解析度: {}",
                        total_drawn_polys, self.camera.distance, self.push_pull_distance, self.settings.resolution_scale.label()
                    ))
                    .small()
                    .strong()
                    .color(Color32::from_rgb(30, 35, 45)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_text = if self.settings.show_settings_panel {
                        "⚙ 關閉 3D 設定"
                    } else {
                        "⚙ 3D 解析度與品質設定"
                    };
                    if ui.button(egui::RichText::new(btn_text).small().strong()).clicked() {
                        self.settings.show_settings_panel = !self.settings.show_settings_panel;
                    }
                });
            });
        });

        // 9. Interactive 3D Settings Overlay Panel
        if self.settings.show_settings_panel {
            let settings_rect = Rect::from_min_size(
                Pos2::new(rect.max.x - 330.0, rect.min.y + 48.0),
                egui::vec2(315.0, 560.0),
            );

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(settings_rect), |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(24, 28, 36, 245))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(70, 85, 110)))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚙ 3D 渲染與剖切設定").strong().size(13.0).color(Color32::from_rgb(225, 235, 250)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    self.settings.show_settings_panel = false;
                                }
                            });
                        });
                        ui.separator();

                        ui.add_space(3.0);
                        ui.label(egui::RichText::new("1. 渲染解析度倍率 (Super-Sampling):").small().strong().color(Color32::from_rgb(255, 215, 0)));
                        ui.radio_value(&mut self.settings.resolution_scale, ResolutionScale::Standard, "1.0x 標準清晰 (Standard)");
                        ui.radio_value(&mut self.settings.resolution_scale, ResolutionScale::High, "1.5x 高清抗鋸齒 (1.5x SSAA)");
                        ui.radio_value(&mut self.settings.resolution_scale, ResolutionScale::Ultra, "2.0x 極致細緻 (Retina 2.0x SSAA)");

                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("2. 空間著色風格 (Shading Mode):").small().strong().color(Color32::from_rgb(255, 215, 0)));
                        ui.radio_value(&mut self.settings.shading_mode, ShadingMode::Realistic, "🎨 擬真實體材質著色 (Material)");
                        ui.radio_value(&mut self.settings.shading_mode, ShadingMode::HiddenLine, "📐 建築消隱線框 (Technical)");
                        ui.radio_value(&mut self.settings.shading_mode, ShadingMode::XRay, "🩻 X-Ray 空間透視 (X-Ray)");

                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("3. 透視保護與工程邊線:").small().strong().color(Color32::from_rgb(255, 215, 0)));
                        ui.checkbox(&mut self.settings.transparent_ceiling, "天花板玻璃化 (透視內部樓梯)");
                        ui.checkbox(&mut self.settings.show_silhouette_edges, "顯示幾何邊線輪廓");
                        ui.checkbox(&mut self.settings.show_ground_grid, "顯示地面 1000mm 參考網格");

                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("4. 🔪 3D 動態剖切盒 (Section Cut):").small().strong().color(Color32::from_rgb(255, 215, 0)));
                        ui.checkbox(&mut self.settings.enable_section_cut, "啟用剖切面 (透視室內家具與格局)");
                        if self.settings.enable_section_cut {
                            let previous_axis = self.settings.section_axis;
                            ui.horizontal(|ui| {
                                ui.label("切面軸向:");
                                ui.radio_value(&mut self.settings.section_axis, 0, "X");
                                ui.radio_value(&mut self.settings.section_axis, 1, "Y");
                                ui.radio_value(&mut self.settings.section_axis, 2, "Z (平剖)");
                            });
                            if self.settings.section_axis != previous_axis {
                                self.settings.section_offset = self.default_section_offset(doc) as f32;
                            }

                            // Slider spans the model extent along the chosen axis
                            let (min, max) = self.section_range();
                            ui.horizontal(|ui| {
                                ui.label("切面位置:");
                                ui.add(
                                    egui::Slider::new(&mut self.settings.section_offset, min..=max)
                                        .step_by(10.0)
                                        .suffix(" mm"),
                                );
                            });
                            let span = (max - min).max(1.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "模型範圍 {:.0} ~ {:.0} mm · 目前位於 {:.0}%",
                                    min,
                                    max,
                                    (self.settings.section_offset - min) / span * 100.0
                                ))
                                .small()
                                .color(Color32::from_rgb(160, 175, 195)),
                            );

                            // Z cut relative to the building storeys (樓層標高 + 1.2 m)
                            if self.settings.section_axis == 2 && !doc.storeys().is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label("依樓層:");
                                    let current = doc.storeys().iter().find(|st| {
                                        ((st.elevation + SECTION_CUT_ABOVE_FLOOR) as f32 - self.settings.section_offset).abs() < 1.0
                                    });
                                    egui::ComboBox::from_id_salt("section_storey")
                                        .selected_text(current.map_or("自訂".to_string(), |st| st.name.clone()))
                                        .show_ui(ui, |ui| {
                                            for st in doc.storeys() {
                                                let label = format!(
                                                    "{}  (EL {:+.2} m + 1.2 m)",
                                                    st.name,
                                                    st.source_elevation.unwrap_or(st.elevation) / 1000.0
                                                );
                                                if ui.selectable_label(current == Some(st), label).clicked() {
                                                    self.settings.section_offset = (st.elevation + SECTION_CUT_ABOVE_FLOOR) as f32;
                                                }
                                            }
                                        });
                                });
                            }
                            ui.checkbox(&mut self.settings.section_invert, "反轉剖切方向");
                        }

                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.label("輪廓線寬:");
                            ui.add(egui::Slider::new(&mut self.settings.edge_stroke_width, 0.5..=2.5).step_by(0.5).suffix(" px"));
                        });
                    });
            });
        }

        // A scene click re-picks `selected_id` (None on empty space)
        if clicked_scene { *selected_id } else { None }
    }

    /// Smoothly turns the camera toward the orientation chosen on the ViewCube.
    fn advance_view_animation(&mut self, ctx: &egui::Context) {
        let Some((target_yaw, target_pitch)) = self.view_anim else { return };
        let dt = ctx.input(|i| i.stable_dt.min(0.1)) as f64;
        let t = 1.0 - (-dt * 11.0).exp();
        // Shortest way around for yaw
        let d_yaw = (target_yaw - self.camera.yaw_rad + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let d_pitch = target_pitch - self.camera.pitch_rad;
        if d_yaw.abs() < 1e-3 && d_pitch.abs() < 1e-3 {
            self.camera.yaw_rad = target_yaw;
            self.camera.pitch_rad = target_pitch;
            self.view_anim = None;
        } else {
            self.camera.yaw_rad += d_yaw * t;
            self.camera.pitch_rad += d_pitch * t;
            ctx.request_repaint();
        }
    }

    /// Starts an animated turn so the camera looks at the target from direction `dir`
    /// (pointing from the target toward the eye).
    fn look_from(&mut self, dir: Vector3D) {
        let dir = dir.normalize();
        let pitch = dir.z.clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH);
        let yaw = if dir.x.abs() < 1e-6 && dir.y.abs() < 1e-6 {
            // Straight top / bottom view: keep the current heading, squared to the axes
            (self.camera.yaw_rad / std::f64::consts::FRAC_PI_2).round() * std::f64::consts::FRAC_PI_2
        } else {
            dir.x.atan2(-dir.y)
        };
        self.view_anim = Some((yaw, pitch));
    }

    /// Interactive 3D ViewCube (導覽立方體) in the top-left corner.
    ///
    /// Each visible face is split into a 3x3 grid like Autodesk's ViewCube: the centre cell
    /// turns to that orthographic view (前/後/左/右/上/下), edge cells to the 45° view between two
    /// faces and corner cells to an isometric view. Dragging over the cube orbits the scene and
    /// the ⌂ button returns to the home view.
    fn render_viewcube(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        center: Pos2,
        mouse_pos: Option<Pos2>,
        clicked: bool,
    ) {
        const SCALE: f32 = 21.0; // screen half-size of the unit cube
        const BAND: f32 = 0.26; // width of edge / corner cells in face coordinates
        let (_, forward, right, up) = self.camera_basis();
        let project = |v: Vector3D| center + egui::vec2(v.dot(right) as f32 * SCALE, -(v.dot(up) as f32) * SCALE);

        // Compass ring
        painter.circle_filled(center, 38.0, Color32::from_rgba_unmultiplied(22, 26, 34, 185));
        painter.circle_stroke(center, 38.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 120, 160, 100)));

        struct Face {
            name: &'static str,
            normal: Vector3D,
            u: Vector3D,
            v: Vector3D,
        }
        let faces = [
            Face { name: "上", normal: Vector3D::Z, u: Vector3D::X, v: Vector3D::Y },
            Face { name: "下", normal: -Vector3D::Z, u: Vector3D::X, v: -Vector3D::Y },
            Face { name: "前", normal: -Vector3D::Y, u: Vector3D::X, v: Vector3D::Z },
            Face { name: "後", normal: Vector3D::Y, u: -Vector3D::X, v: Vector3D::Z },
            Face { name: "左", normal: -Vector3D::X, u: -Vector3D::Y, v: Vector3D::Z },
            Face { name: "右", normal: Vector3D::X, u: Vector3D::Y, v: Vector3D::Z },
        ];

        // Visible faces, far to near
        let mut visible: Vec<(&Face, f64)> = faces
            .iter()
            .map(|f| (f, f.normal.dot(-forward)))
            .filter(|(_, facing)| *facing > 0.01)
            .collect();
        visible.sort_by(|a, b| a.1.total_cmp(&b.1));

        // Face-local (u, v) in [0, 1]² of a screen point, for a projected face parallelogram
        let face_uv = |face: &Face, p: Pos2| -> Option<(f32, f32)> {
            let p0 = project(face.normal - face.u - face.v);
            let e1 = project(face.normal + face.u - face.v) - p0;
            let e2 = project(face.normal - face.u + face.v) - p0;
            let det = e1.x * e2.y - e1.y * e2.x;
            if det.abs() < 1.0 {
                return None;
            }
            let d = p - p0;
            let u = (d.x * e2.y - d.y * e2.x) / det;
            let v = (e1.x * d.y - e1.y * d.x) / det;
            ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some((u, v))
        };
        let cell_of = |t: f32| if t < BAND { -1i8 } else if t > 1.0 - BAND { 1 } else { 0 };
        let cell_range = |c: i8| match c {
            -1 => (0.0, BAND),
            0 => (BAND, 1.0 - BAND),
            _ => (1.0 - BAND, 1.0),
        };

        // Hovered cell on the nearest face under the cursor
        let hovered = mouse_pos.and_then(|m| {
            visible.iter().rev().find_map(|(face, _)| {
                face_uv(face, m).map(|(u, v)| (face.name, cell_of(u), cell_of(v)))
            })
        });

        let light = Vector3D::new(0.6, -0.5, 0.75).normalize();
        for (face, facing) in &visible {
            let corners = [
                face.normal - face.u - face.v,
                face.normal + face.u - face.v,
                face.normal + face.u + face.v,
                face.normal - face.u + face.v,
            ]
            .map(project);
            let shade = (0.78 + 0.22 * face.normal.dot(light)) as f32;
            let base = Color32::from_rgb((236.0 * shade) as u8, (240.0 * shade) as u8, (247.0 * shade) as u8);
            painter.add(egui::Shape::convex_polygon(corners.to_vec(), base, Stroke::new(1.2, Color32::from_rgb(78, 92, 112))));

            // Faint 3x3 grid hinting the clickable edge / corner zones
            let p0 = corners[0];
            let (e1, e2) = (corners[1] - p0, corners[3] - p0);
            let grid = Stroke::new(0.6, Color32::from_rgba_unmultiplied(78, 92, 112, 70));
            for t in [BAND, 1.0 - BAND] {
                painter.line_segment([p0 + e1 * t, p0 + e1 * t + e2], grid);
                painter.line_segment([p0 + e2 * t, p0 + e2 * t + e1], grid);
            }

            if let Some((name, cu, cv)) = hovered {
                if name == face.name {
                    let (u0, u1) = cell_range(cu);
                    let (v0, v1) = cell_range(cv);
                    let cell = vec![p0 + e1 * u0 + e2 * v0, p0 + e1 * u1 + e2 * v0, p0 + e1 * u1 + e2 * v1, p0 + e1 * u0 + e2 * v1];
                    painter.add(egui::Shape::convex_polygon(
                        cell,
                        Color32::from_rgba_unmultiplied(70, 160, 255, 190),
                        Stroke::NONE,
                    ));
                }
            }

            // Upright label, fading out as the face turns edge-on
            if *facing > 0.3 {
                let face_center = project(face.normal);
                let alpha = (((*facing - 0.3) / 0.4).clamp(0.0, 1.0) * 255.0) as u8;
                painter.text(
                    face_center,
                    egui::Align2::CENTER_CENTER,
                    face.name,
                    egui::FontId::proportional(10.0 + 3.0 * *facing as f32),
                    Color32::from_rgba_unmultiplied(30, 40, 58, alpha),
                );
            }
        }

        // North (+Y) marker on the ring, drawn over the cube so it is never hidden
        let north = egui::vec2(Vector3D::Y.dot(right) as f32, -(Vector3D::Y.dot(up) as f32));
        if north.length() > 0.05 {
            painter.text(
                center + north.normalized() * 33.0,
                egui::Align2::CENTER_CENTER,
                "N",
                egui::FontId::monospace(10.0),
                Color32::from_rgb(255, 80, 80),
            );
        }

        // Home button (⌂): back to the default view
        let home_center = center + egui::vec2(28.0, -28.0);
        let home_hovered = mouse_pos.is_some_and(|m| m.distance(home_center) < 9.0);
        painter.circle_filled(
            home_center,
            9.0,
            if home_hovered { Color32::from_rgb(70, 160, 255) } else { Color32::from_rgb(40, 48, 62) },
        );
        painter.text(home_center, egui::Align2::CENTER_CENTER, "⌂", egui::FontId::proportional(12.0), Color32::WHITE);

        if hovered.is_some() || home_hovered {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if clicked {
            if home_hovered {
                self.view_anim = Some((HOME_YAW, HOME_PITCH));
            } else if let Some((name, cu, cv)) = hovered {
                if let Some(face) = faces.iter().find(|f| f.name == name) {
                    self.look_from(face.normal + face.u * cu as f64 + face.v * cv as f64);
                }
            }
        }
    }
}

impl Viewport3D {
    /// World-space ray through a screen point of the viewport.
    fn screen_ray(&self, rect: Rect, pos: Pos2) -> (Point3D, Vector3D) {
        let (eye, forward, right, up) = self.camera_basis();
        let aspect = rect.width() as f64 / rect.height().max(1.0) as f64;
        let tan_half = (self.camera.fov_y_rad * 0.5).tan();
        let ndc_x = ((pos.x - rect.min.x) / rect.width() * 2.0 - 1.0) as f64;
        let ndc_y = (1.0 - (pos.y - rect.min.y) / rect.height() * 2.0) as f64;
        (eye, (forward + right * (ndc_x * tan_half * aspect) + up * (ndc_y * tan_half)).normalize())
    }

    /// Screen position of a world point (`None` behind the camera).
    fn project(&self, rect: Rect, p: Point3D) -> Option<Pos2> {
        let (eye, forward, right, up) = self.camera_basis();
        let aspect = rect.width() as f64 / rect.height().max(1.0) as f64;
        let tan_half = (self.camera.fov_y_rad * 0.5).tan();
        let v = p - eye;
        let z = v.dot(forward);
        if z < NEAR_PLANE_MM {
            return None;
        }
        let x = v.dot(right) / (z * tan_half * aspect);
        let y = v.dot(up) / (z * tan_half);
        Some(Pos2::new(rect.center().x + (x as f32) * rect.width() * 0.5, rect.center().y - (y as f32) * rect.height() * 0.5))
    }

    /// Nearest visible surface under a ray: hit point and its normal (facing the camera).
    /// Falls back to the ground plane z = 0 when nothing else is hit.
    fn surface_hit(&self, eye: Point3D, dir: Vector3D) -> Option<(Point3D, Vector3D, bool)> {
        let mut best: Option<(f64, Vector3D)> = None;
        if let Some(cache) = &self.cache {
            for tri in &cache.pick_tris {
                if let Some(t) = ray_triangle(eye, dir, &tri.v) {
                    if best.is_none_or(|(bt, _)| t < bt) && !section_removed(&self.settings, eye + dir * t) {
                        let n = (tri.v[1] - tri.v[0]).cross(tri.v[2] - tri.v[0]);
                        best = Some((t, n));
                    }
                }
            }
        }
        if let Some((t, n)) = best {
            let n = if n.length() > 1e-12 { n.normalize() } else { Vector3D::Z };
            let n = if n.dot(dir) > 0.0 { -n } else { n };
            return Some((eye + dir * t, n, true));
        }
        if dir.z < -1e-6 {
            let t = -eye.z / dir.z;
            return Some((eye + dir * t, Vector3D::Z, false));
        }
        None
    }

    /// Placement of `model` for a surface hit: against a wall the model backs onto it and
    /// faces the room at its default height; on a floor / table it sits on the surface.
    fn placement_for(&self, model: &sketchcad_bim::ModelRef, hit: (Point3D, Vector3D, bool)) -> Placement {
        let (p, n, on_element) = hit;
        if on_element && n.z.abs() < 0.5 {
            let n2 = Vector2D::new(n.x, n.y).normalize_or_zero();
            let depth = model.geometry().size.1;
            return Placement {
                position: Point2D::new(p.x, p.y) + n2 * (depth * 0.5 + 5.0),
                base_z: model.elevation(),
                rotation_deg: n2.x.atan2(-n2.y).to_degrees(),
                on_wall: true,
            };
        }
        let base_z = if p.z < 200.0 { model.elevation().max(p.z) } else { p.z };
        Placement { position: Point2D::new(p.x, p.y), base_z, rotation_deg: self.placing_rotation_deg, on_wall: false }
    }

    /// Ghost box of the model under the cursor; a click places it.
    fn render_model_placement(&mut self, painter: &egui::Painter, rect: Rect, mouse: Option<Pos2>, clicked: bool) {
        let Some(model) = self.placing_model.as_deref().and_then(sketchcad_bim::find_model) else {
            if let Some(m) = mouse {
                painter.text(m + egui::vec2(0.0, -24.0), egui::Align2::CENTER_CENTER, "請先從「📦 3D 模型百寶箱」選擇模型", egui::FontId::proportional(13.0), Color32::from_rgb(255, 200, 100));
            }
            return;
        };
        let Some(mouse) = mouse else { return };
        let (eye, dir) = self.screen_ray(rect, mouse);
        let Some(hit) = self.surface_hit(eye, dir) else { return };
        let place = self.placement_for(&model, hit);

        let mut preview = sketchcad_bim::ModelComponent::new(&model, place.position, place.rotation_deg);
        preview.base_z = place.base_z;
        let (_, _, h) = preview.size();
        let outline = preview.outline();
        let colour = if place.on_wall { Color32::from_rgb(255, 200, 90) } else { Color32::from_rgb(110, 235, 160) };
        let stroke = Stroke::new(1.8, colour);
        let ring = |z: f64| outline.iter().map(|q| self.project(rect, Point3D::new(q.x, q.y, z))).collect::<Vec<_>>();
        let (bottom, top) = (ring(place.base_z), ring(place.base_z + h));
        if let Some(fill) = bottom.iter().copied().collect::<Option<Vec<Pos2>>>() {
            painter.add(egui::Shape::convex_polygon(fill, colour.gamma_multiply(0.18), Stroke::NONE));
        }
        let step = (outline.len() / 8).max(1);
        for i in 0..outline.len() {
            let j = (i + 1) % outline.len();
            for r in [&bottom, &top] {
                if let (Some(a), Some(b)) = (r[i], r[j]) {
                    painter.line_segment([a, b], stroke);
                }
            }
            if i % step == 0 {
                if let (Some(a), Some(b)) = (bottom[i], top[i]) {
                    painter.line_segment([a, b], Stroke::new(1.0, colour.gamma_multiply(0.7)));
                }
            }
        }
        let label = if place.on_wall { "靠牆放置 (自動朝外)" } else { "點擊放置 · R 旋轉 90°" };
        painter.text(
            mouse + egui::vec2(0.0, -26.0),
            egui::Align2::CENTER_CENTER,
            format!("{} — {} · 高程 {:.0}mm", model.name(), label, place.base_z),
            egui::FontId::proportional(12.5),
            colour,
        );
        if clicked {
            self.model_actions.push(ModelAction::Place {
                model_id: model.id().to_string(),
                position: place.position,
                base_z: place.base_z,
                rotation_deg: place.rotation_deg,
            });
        }
    }

    /// Starts, continues and ends a drag-move of the selected library model.
    fn handle_model_drag(&mut self, ui: &Ui, response: &egui::Response, rect: Rect, doc: &BimDocument, selected: Option<ElementId>, over_cube: bool) {
        if response.drag_started_by(egui::PointerButton::Primary) && !over_cube && self.placing_model.is_none() {
            let press = ui.input(|i| i.pointer.press_origin());
            if let (Some(id), Some(press)) = (selected, press) {
                if let Some(model) = doc.get_model(id) {
                    let (eye, dir) = self.screen_ray(rect, press);
                    let mesh = sketchcad_bim::BimElement::to_3d_mesh(model);
                    let hit = (0..mesh.triangle_count())
                        .filter_map(|t| {
                            let v = [0, 1, 2].map(|k| mesh.positions[mesh.indices[t * 3 + k] as usize]);
                            ray_triangle(eye, dir, &v)
                        })
                        .min_by(f64::total_cmp);
                    if let Some(t) = hit {
                        let grab = eye + dir * t;
                        self.model_drag = Some(ModelDrag {
                            id,
                            plane_z: grab.z,
                            offset: model.position - Point2D::new(grab.x, grab.y),
                            start_base_z: model.base_z,
                            vertical_px: 0.0,
                        });
                        self.model_actions.push(ModelAction::BeginMove(id));
                    }
                }
            }
        }
        let Some(drag) = &mut self.model_drag else { return };
        if !response.dragged_by(egui::PointerButton::Primary) {
            self.model_drag = None;
            return;
        }
        let Some(model) = doc.get_model(drag.id) else {
            self.model_drag = None;
            return;
        };
        let (shift, pointer) = ui.input(|i| (i.modifiers.shift, i.pointer.hover_pos()));
        if shift {
            // Raise / lower: screen pixels to world units at the model's distance
            drag.vertical_px -= response.drag_delta().y;
            let eye = self.camera.eye_position();
            let dist = (Point3D::new(model.position.x, model.position.y, model.base_z) - eye).length();
            let world_per_px = 2.0 * dist * (self.camera.fov_y_rad * 0.5).tan() / rect.height().max(1.0) as f64;
            let base_z = (drag.start_base_z + drag.vertical_px as f64 * world_per_px).round();
            self.model_actions.push(ModelAction::MoveTo { id: drag.id, position: model.position, base_z });
        } else if let Some(pos) = pointer {
            let (plane_z, offset, id) = (drag.plane_z, drag.offset, drag.id);
            let (eye, dir) = self.screen_ray(rect, pos);
            if dir.z.abs() > 1e-6 {
                let t = (plane_z - eye.z) / dir.z;
                if t > 0.0 {
                    let p = eye + dir * t;
                    let position = (Point2D::new(p.x, p.y) + offset).round();
                    self.model_actions.push(ModelAction::MoveTo { id, position, base_z: model.base_z });
                }
            }
        }
    }

    /// Floating toolbar above a selected library model: rotate, duplicate, delete.
    fn render_model_toolbar(&mut self, ui: &Ui, rect: Rect, doc: &BimDocument, id: ElementId) {
        let Some(model) = doc.get_model(id) else { return };
        let Some(bbox) = sketchcad_bim::BimElement::to_3d_mesh(model).bbox() else { return };
        let top = Point3D::new((bbox.min.x + bbox.max.x) * 0.5, (bbox.min.y + bbox.max.y) * 0.5, bbox.max.z);
        let Some(anchor) = self.project(rect, top) else { return };
        if !rect.shrink(20.0).contains(anchor) {
            return;
        }
        let dragging = self.model_drag.is_some();
        egui::Area::new(egui::Id::new("model_toolbar_3d"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor + egui::vec2(-118.0, -64.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("⟲ 90°").on_hover_text("逆時針旋轉 90°").clicked() {
                            self.model_actions.push(ModelAction::Rotate(id, 90.0));
                        }
                        if ui.small_button("⟳ 90°").on_hover_text("順時針旋轉 90°").clicked() {
                            self.model_actions.push(ModelAction::Rotate(id, -90.0));
                        }
                        if ui.small_button("⧉ 複製").on_hover_text("複製一個並放在旁邊").clicked() {
                            self.model_actions.push(ModelAction::Duplicate(id));
                        }
                        if ui.small_button("🗑 刪除").on_hover_text("刪除此模型 [Del]").clicked() {
                            self.model_actions.push(ModelAction::Delete(id));
                        }
                    });
                    let hint = if dragging { format!("高程 {:.0} mm", model.base_z) } else { "拖曳移動 · Shift+拖曳升降".to_string() };
                    ui.label(egui::RichText::new(hint).small().weak());
                });
            });
    }

    /// Eye position plus the right-handed camera basis (forward, right, up) used by `look_at_rh`.
    fn camera_basis(&self) -> (Point3D, Vector3D, Vector3D, Vector3D) {
        let eye = self.camera.eye_position();
        let forward = (self.camera.target - eye).normalize();
        let right = forward.cross(Vector3D::Z).normalize();
        let up = right.cross(forward);
        (eye, forward, right, up)
    }

    /// Section slider range along the current axis: the visible model extent plus a margin.
    fn section_range(&self) -> (f32, f32) {
        let axis = self.settings.section_axis.min(2) as usize;
        match self.scene_bounds {
            Some((min, max)) => {
                let (lo, hi) = (min[axis], max[axis]);
                let margin = ((hi - lo) * 0.02).max(100.0);
                // Whole 100 mm bounds so slider steps land on round values
                (((lo - margin) / 100.0).floor() as f32 * 100.0, ((hi + margin) / 100.0).ceil() as f32 * 100.0)
            }
            None => (0.0, 6000.0),
        }
    }

    /// Sensible cut position for the current axis: the middle of the model for X / Y, and
    /// 1.2 m above the ground-floor storey (or the model base) for a Z plan cut.
    fn default_section_offset(&self, doc: &BimDocument) -> f64 {
        let axis = self.settings.section_axis.min(2) as usize;
        let Some((min, max)) = self.scene_bounds else { return 2600.0 };
        if axis < 2 {
            return (min[axis] + max[axis]) * 0.5;
        }
        // Ground floor: the storey whose authored elevation is closest to ±0
        let floor = doc
            .storeys()
            .iter()
            .min_by(|a, b| {
                let ground = |st: &sketchcad_bim::Storey| st.source_elevation.unwrap_or(st.elevation).abs();
                ground(a).total_cmp(&ground(b))
            })
            .map_or(min.z, |st| st.elevation);
        (floor + SECTION_CUT_ABOVE_FLOOR).min((min.z + max.z) * 0.5).max(min.z)
    }

    /// Records the model bounds and moves the cut back into range if the model changed.
    fn update_scene_bounds(&mut self, bounds: Option<(Point3D, Point3D)>, doc: &BimDocument) {
        self.scene_bounds = bounds;
        let (min, max) = self.section_range();
        if !(min..=max).contains(&self.settings.section_offset) {
            self.settings.section_offset = self.default_section_offset(doc) as f32;
        }
    }

    /// Hash of the display settings that are baked into the cached scene geometry.
    fn style_fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.settings.shading_mode.hash(&mut h);
        self.settings.transparent_ceiling.hash(&mut h);
        self.settings.show_ground_grid.hash(&mut h);
        h.finish()
    }

    /// Builds GPU vertex data and CPU picking data for all visible elements.
    fn build_scene(&self, doc: &BimDocument, fingerprint: u64) -> (SceneGeometry, SceneCache) {
        let mut geom = SceneGeometry::default();
        let mut cache = SceneCache {
            fingerprint,
            elem_ids: Vec::new(),
            pick_tris: Vec::new(),
            translucent: Vec::new(),
            translucent_order: Vec::new(),
            sorted_for_eye: None,
            triangle_count: 0,
        };
        let light_dir = Vector3D::new(0.6, -0.5, 0.75).normalize();
        let to_f32 = |p: Point3D| [p.x as f32, p.y as f32, p.z as f32];

        for elem in doc.all_elements() {
            if !doc.is_visible(elem.id()) {
                continue;
            }
            let kind = elem.kind();
            let elem_index = cache.elem_ids.len() as u32;
            cache.elem_ids.push(elem.id());

            let mut mesh = elem.to_3d_mesh();
            mesh.compute_crease_edges(CREASE_ANGLE_DEG.to_radians());
            let material = element_material(doc, elem, &self.settings);

            // Each feature edge is shared by two triangles; draw it only once.
            let quantize = |p: Point3D| ((p.x * 10.0).round() as i64, (p.y * 10.0).round() as i64, (p.z * 10.0).round() as i64);
            let mut seen_edges = HashSet::new();
            let translucent_start = geom.translucent.len();
            let mut translucent_sum = Vector3D::ZERO;

            for i in 0..mesh.triangle_count() {
                let idx = [
                    mesh.indices[i * 3] as usize,
                    mesh.indices[i * 3 + 1] as usize,
                    mesh.indices[i * 3 + 2] as usize,
                ];
                if idx.iter().any(|&k| k >= mesh.positions.len()) {
                    continue;
                }
                let v = idx.map(|k| mesh.positions[k]);
                let cross = (v[1] - v[0]).cross(v[2] - v[0]);
                let cross_len = cross.length();
                if cross_len < 1e-6 {
                    continue; // Degenerate triangle
                }
                let face_normal = cross / cross_len;
                let centroid = (v[0] + v[1] + v[2]) * (1.0 / 3.0);

                let (fill, edge) = shade_triangle(&self.settings, kind, triangle_material(&mesh, i, material), face_normal, centroid, light_dir);
                let fill_rgba = fill.to_srgba_unmultiplied();
                let edge_rgba = edge.to_srgba_unmultiplied();

                let target = if fill_rgba[3] < 255 {
                    translucent_sum += centroid;
                    &mut geom.translucent
                } else {
                    &mut geom.opaque
                };
                for p in v {
                    target.push(FaceVertex { pos: to_f32(p), color: fill_rgba, elem: elem_index });
                }
                cache.pick_tris.push(PickTri { v, elem: elem_index });
                cache.triangle_count += 1;

                if edge_rgba[3] == 0 {
                    continue;
                }
                let flags = mesh.edge_flags.get(i).copied().unwrap_or([true, true, true]);
                for e in 0..3 {
                    if !flags[e] {
                        continue;
                    }
                    let (a, b) = (v[e], v[(e + 1) % 3]);
                    let (ka, kb) = (quantize(a), quantize(b));
                    if seen_edges.insert(if ka <= kb { (ka, kb) } else { (kb, ka) }) {
                        EdgeVertex::push_segment(&mut geom.edges, to_f32(a), to_f32(b), edge_rgba, elem_index);
                    }
                }
            }

            let vertex_count = geom.translucent.len() - translucent_start;
            if vertex_count > 0 {
                let range = TranslucentRange {
                    first_vertex: translucent_start as u32,
                    vertex_count: vertex_count as u32,
                };
                cache.translucent.push((range, translucent_sum * (3.0 / vertex_count as f64)));
            }
        }

        if self.settings.show_ground_grid {
            let grid_size = 50000.0f32;
            let step = 5000.0f32;
            let color = Color32::from_rgba_unmultiplied(170, 175, 180, 70).to_srgba_unmultiplied();
            let mut c = -grid_size;
            while c <= grid_size {
                EdgeVertex::push_segment(&mut geom.edges, [c, -grid_size, 0.0], [c, grid_size, 0.0], color, u32::MAX);
                EdgeVertex::push_segment(&mut geom.edges, [-grid_size, c, 0.0], [grid_size, c, 0.0], color, u32::MAX);
                c += step;
            }
        }

        (geom, cache)
    }

    /// Depth-buffered GPU rendering path. Returns the number of scene triangles.
    #[allow(clippy::too_many_arguments)]
    fn render_scene_gpu(
        &mut self,
        ui: &Ui,
        painter: &egui::Painter,
        rect: Rect,
        doc: &BimDocument,
        selected_id: &mut Option<ElementId>,
        mouse_pos: Option<Pos2>,
        clicked: bool,
    ) -> usize {
        // 1. Rebuild cached geometry only when the model or style settings changed
        let doc_changed = self.doc_watcher.poll(doc);
        let fingerprint = self.style_fingerprint();
        if doc_changed || self.cache.as_ref().map_or(true, |c| c.fingerprint != fingerprint) {
            let (geom, cache) = self.build_scene(doc, fingerprint);
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.upload_geometry(&geom);
            }
            let bounds = cache.pick_tris.iter().flat_map(|t| t.v).fold(None, |acc: Option<(Point3D, Point3D)>, p| {
                Some(acc.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))))
            });
            self.update_scene_bounds(bounds, doc);
            self.cache = Some(cache);
        }

        let (eye, forward, right, up) = self.camera_basis();
        let (Some(gpu), Some(cache)) = (self.gpu.as_mut(), self.cache.as_mut()) else {
            return 0;
        };

        // 2. Back-to-front order of translucent elements (only when the eye moved)
        if cache.sorted_for_eye != Some(eye) {
            let mut order: Vec<(f64, TranslucentRange)> =
                cache.translucent.iter().map(|(range, c)| ((*c - eye).length_squared(), *range)).collect();
            order.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
            cache.translucent_order = order.into_iter().map(|(_, range)| range).collect();
            cache.sorted_for_eye = Some(eye);
        }

        // 3. Offscreen render size: physical pixels x supersampling, within the GPU budget
        let ppp = ui.ctx().pixels_per_point();
        let mut scale = ppp * self.settings.resolution_scale.factor();
        let pixels = rect.width() * rect.height() * scale * scale;
        if pixels > MAX_RENDER_PIXELS {
            scale *= (MAX_RENDER_PIXELS / pixels).sqrt();
        }
        let max_dim = gpu.max_texture_dimension() as f32;
        scale = scale.min(max_dim / rect.width()).min(max_dim / rect.height());
        let size_px = [
            ((rect.width() * scale).round() as u32).max(1),
            ((rect.height() * scale).round() as u32).max(1),
        ];
        let px_per_point = size_px[0] as f32 / rect.width();

        // 4. Camera uniforms (reverse-Z infinite perspective for precise depth at any distance)
        let aspect = rect.width() as f64 / rect.height().max(1.0) as f64;
        let view = self.camera.view_matrix();
        let proj = sketchcad_core::DMat4::perspective_infinite_reverse_rh(self.camera.fov_y_rad, aspect, NEAR_PLANE_MM);
        let tan_half = (self.camera.fov_y_rad * 0.5).tan();
        let selected_index = selected_id.and_then(|id| cache.elem_ids.iter().position(|e| *e == id));
        let edge_px = if self.settings.show_silhouette_edges {
            self.settings.edge_stroke_width * px_per_point
        } else {
            0.0
        };
        let uniforms = SceneUniforms {
            view_proj: (proj * view).as_mat4().to_cols_array_2d(),
            view: view.as_mat4().to_cols_array_2d(),
            proj: proj.as_mat4().to_cols_array_2d(),
            cam_right: [right.x as f32, right.y as f32, right.z as f32, (tan_half * aspect) as f32],
            cam_up: [up.x as f32, up.y as f32, up.z as f32, tan_half as f32],
            cam_fwd: [forward.x as f32, forward.y as f32, forward.z as f32, NEAR_PLANE_MM as f32],
            viewport: [size_px[0] as f32, size_px[1] as f32, edge_px, 2.5 * px_per_point],
            section: [
                self.settings.section_axis as f32,
                self.settings.section_offset,
                self.settings.section_invert as u8 as f32,
                self.settings.enable_section_cut as u8 as f32,
            ],
            selection: [selected_index.unwrap_or(0) as u32, selected_index.is_some() as u32, 0, 0],
        };

        let texture_id = gpu.render(size_px, &uniforms, &cache.translucent_order, true);
        painter.image(
            texture_id,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );

        // 5. Click picking: nearest ray-triangle hit that is not removed by the section cut
        if clicked {
            if let Some(m) = mouse_pos {
                let ndc_x = ((m.x - rect.min.x) / rect.width() * 2.0 - 1.0) as f64;
                let ndc_y = (1.0 - (m.y - rect.min.y) / rect.height() * 2.0) as f64;
                let dir = (forward + right * (ndc_x * tan_half * aspect) + up * (ndc_y * tan_half)).normalize();
                let mut best: Option<(f64, u32)> = None;
                for tri in &cache.pick_tris {
                    if let Some(t) = ray_triangle(eye, dir, &tri.v) {
                        if best.map_or(true, |(bt, _)| t < bt) && !section_removed(&self.settings, eye + dir * t) {
                            best = Some((t, tri.elem));
                        }
                    }
                }
                *selected_id = best.map(|(_, e)| cache.elem_ids[e as usize]);
            }
        }

        cache.triangle_count
    }

    /// Fallback renderer (no wgpu): painter's-algorithm triangle sort drawn with the egui painter.
    #[allow(clippy::too_many_arguments)]
    fn render_scene_software(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        doc: &BimDocument,
        selected_id: &mut Option<ElementId>,
        mouse_pos: Option<Pos2>,
        clicked: bool,
    ) -> usize {
        let width = rect.width() as f64;
        let height = rect.height() as f64;
        let aspect = width / height.max(1.0);

        let horizon_y = rect.center().y + (self.camera.pitch_rad * 150.0) as f32;
        let sky_rect = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, horizon_y.clamp(rect.min.y, rect.max.y)));
        let ground_rect = Rect::from_min_max(Pos2::new(rect.min.x, horizon_y.clamp(rect.min.y, rect.max.y)), rect.max);
        painter.rect_filled(sky_rect, 0.0, Color32::from_rgb(185, 215, 240));
        painter.rect_filled(ground_rect, 0.0, Color32::from_rgb(220, 222, 215));

        let vp_mat = self.camera.projection_matrix(aspect) * self.camera.view_matrix();
        let (eye_pos, cam_forward, _, _) = self.camera_basis();

        let project_point = |p: Point3D| -> Option<Pos2> {
            let p4 = vp_mat * p.extend(1.0);
            if p4.w <= 10.0 {
                return None; // Point is behind or too close to camera lens
            }
            let screen_x = rect.min.x + ((p4.x / p4.w + 1.0) * 0.5 * width) as f32;
            let screen_y = rect.min.y + ((1.0 - p4.y / p4.w) * 0.5 * height) as f32;
            Some(Pos2::new(screen_x, screen_y))
        };

        if self.settings.show_ground_grid {
            let grid_size = 50000.0;
            let step = 5000.0;
            let stroke = Stroke::new(0.6, Color32::from_rgba_unmultiplied(170, 175, 180, 70));
            let mut c = -grid_size;
            while c <= grid_size {
                for (a, b) in [
                    (Point3D::new(c, -grid_size, 0.0), Point3D::new(c, grid_size, 0.0)),
                    (Point3D::new(-grid_size, c, 0.0), Point3D::new(grid_size, c, 0.0)),
                ] {
                    if let (Some(p0), Some(p1)) = (project_point(a), project_point(b)) {
                        painter.line_segment([p0, p1], stroke);
                    }
                }
                c += step;
            }
        }

        struct ScenePoly {
            points: [Pos2; 3],
            depth: f64,
            fill_color: Color32,
            edge_color: Color32,
            edge_flags: [bool; 3],
            elem_id: ElementId,
        }

        let mut scene_polys: Vec<ScenePoly> = Vec::new();
        let light_dir = Vector3D::new(0.6, -0.5, 0.75).normalize();
        let edge_width = if self.settings.show_silhouette_edges {
            self.settings.edge_stroke_width * self.settings.resolution_scale.factor()
        } else {
            0.0
        };

        let mut bounds: Option<(Point3D, Point3D)> = None;
        for elem in doc.all_elements() {
            if !doc.is_visible(elem.id()) {
                continue;
            }
            let kind = elem.kind();
            let mut mesh = elem.to_3d_mesh();
            mesh.compute_crease_edges(CREASE_ANGLE_DEG.to_radians());
            let material = element_material(doc, elem, &self.settings);
            if let Some(b) = mesh.bbox() {
                bounds = Some(bounds.map_or((b.min, b.max), |(lo, hi)| (lo.min(b.min), hi.max(b.max))));
            }

            for i in 0..mesh.triangle_count() {
                let v0 = mesh.positions[mesh.indices[i * 3] as usize];
                let v1 = mesh.positions[mesh.indices[i * 3 + 1] as usize];
                let v2 = mesh.positions[mesh.indices[i * 3 + 2] as usize];
                let centroid = (v0 + v1 + v2) * (1.0 / 3.0);
                if section_removed(&self.settings, centroid) {
                    continue;
                }

                let cross = (v1 - v0).cross(v2 - v0);
                let cross_len = cross.length();
                if cross_len < 1e-6 {
                    continue; // Degenerate triangle
                }
                let face_normal = cross / cross_len;
                let view_dir = centroid - eye_pos;

                let (fill_color, edge_color) = shade_triangle(&self.settings, kind, triangle_material(&mesh, i, material), face_normal, centroid, light_dir);
                // Back-face culling for opaque surfaces
                if fill_color.a() == 255 && view_dir.dot(face_normal) >= 0.0 {
                    continue;
                }

                let (Some(s0), Some(s1), Some(s2)) = (project_point(v0), project_point(v1), project_point(v2)) else {
                    continue;
                };

                scene_polys.push(ScenePoly {
                    points: [s0, s1, s2],
                    depth: view_dir.dot(cam_forward),
                    fill_color,
                    edge_color,
                    edge_flags: mesh.edge_flags.get(i).copied().unwrap_or([true, true, true]),
                    elem_id: elem.id(),
                });
            }
        }

        self.update_scene_bounds(bounds, doc);

        // Painter's Algorithm: Sort from furthest (highest depth) to closest (lowest depth)
        scene_polys.sort_unstable_by(|a, b| b.depth.total_cmp(&a.depth));

        for poly in &scene_polys {
            let is_sel = selected_id.map_or(false, |id| id == poly.elem_id);
            painter.add(egui::Shape::convex_polygon(poly.points.to_vec(), poly.fill_color, Stroke::NONE));

            if edge_width > 0.0 || is_sel {
                let stroke = if is_sel {
                    Stroke::new(2.5, Color32::from_rgb(255, 215, 0))
                } else {
                    Stroke::new(edge_width, poly.edge_color)
                };
                for e in 0..3 {
                    if poly.edge_flags[e] {
                        painter.line_segment([poly.points[e], poly.points[(e + 1) % 3]], stroke);
                    }
                }
            }
        }

        if clicked {
            if let Some(m_pos) = mouse_pos {
                *selected_id = scene_polys
                    .iter()
                    .rev()
                    .find(|poly| point_in_triangle(m_pos, poly.points[0], poly.points[1], poly.points[2]))
                    .map(|poly| poly.elem_id);
            }
        }

        scene_polys.len()
    }
}

/// Material of an element: its paint-bucket finish or source-model colour (imported SketchUp
/// geometry) when it has one, otherwise the palette colour of its kind.
fn element_material(doc: &BimDocument, elem: &dyn sketchcad_bim::BimElement, settings: &Viewport3DSettings) -> ([u8; 3], bool, u8) {
    match doc.display_rgba(elem.id()) {
        Some([r, g, b, a]) => ([r, g, b], a < 255, a),
        None => material_for(elem.kind(), settings),
    }
}

/// Material of triangle `i`: its own colour override (e.g. window glass), else the element's.
fn triangle_material(mesh: &sketchcad_geom::TriangleMesh, i: usize, element: ([u8; 3], bool, u8)) -> ([u8; 3], bool, u8) {
    match mesh.face_color(i) {
        Some([r, g, b, a]) => ([r, g, b], a < 255, a),
        None => element,
    }
}

/// Architectural material palette: (base sRGB, translucent, alpha).
fn material_for(kind: ElementKind, settings: &Viewport3DSettings) -> ([u8; 3], bool, u8) {
    if kind == ElementKind::Ceiling && !settings.transparent_ceiling {
        return ([245, 245, 240], false, 255);
    }
    let [r, g, b, a] = kind.display_rgba();
    ([r, g, b], a < 255, a)
}

/// Fill and edge colors of one triangle for the active shading mode (world-fixed sun light).
fn shade_triangle(
    settings: &Viewport3DSettings,
    kind: ElementKind,
    (mat_base_rgb, is_translucent, default_alpha): ([u8; 3], bool, u8),
    face_normal: Vector3D,
    centroid: Point3D,
    light_dir: Vector3D,
) -> (Color32, Color32) {
    let is_light_pool = kind == ElementKind::LightFixture && centroid.z < 60.0;
    match settings.shading_mode {
        ShadingMode::Realistic => {
            if kind == ElementKind::LightFixture {
                if is_light_pool {
                    // Floor illumination pool: soft, warm, glowing, zero wireframe
                    (Color32::from_rgba_unmultiplied(255, 235, 160, 65), Color32::TRANSPARENT)
                } else if face_normal.z < -0.6 {
                    // Luminous emitter lens/bulb facing downwards
                    (Color32::from_rgb(255, 255, 235), Color32::from_rgba_unmultiplied(255, 240, 180, 120))
                } else {
                    // Architectural luminaire housing (matte dark graphite/aluminum)
                    let diffuse = (face_normal.dot(light_dir)).max(0.0) as f32;
                    let light_factor = (0.42 + diffuse * 0.58).clamp(0.2, 1.15);
                    let r = (48.0 * light_factor).clamp(0.0, 255.0) as u8;
                    let g = (52.0 * light_factor).clamp(0.0, 255.0) as u8;
                    let b = (58.0 * light_factor).clamp(0.0, 255.0) as u8;
                    (Color32::from_rgb(r, g, b), Color32::from_rgb(25, 28, 32))
                }
            } else {
                let diffuse = (face_normal.dot(light_dir)).max(0.0) as f32;
                let light_factor = (0.38 + diffuse * 0.62).clamp(0.2, 1.15);
                let lit = mat_base_rgb.map(|c| (c as f32 * light_factor).clamp(0.0, 255.0) as u8);
                let fill = Color32::from_rgba_unmultiplied(lit[0], lit[1], lit[2], default_alpha);
                let edge = if kind == ElementKind::Stairs {
                    Color32::from_rgb(50, 35, 20) // Crisp nosing line for steps
                } else if is_translucent {
                    Color32::from_rgba_unmultiplied(80, 140, 180, 160)
                } else {
                    Color32::from_rgb(50, 55, 65)
                };
                (fill, edge)
            }
        }
        ShadingMode::HiddenLine => {
            let fill = if is_translucent || is_light_pool {
                Color32::from_rgba_unmultiplied(245, 250, 255, 50)
            } else {
                Color32::WHITE
            };
            let edge = if is_light_pool { Color32::TRANSPARENT } else { Color32::from_rgb(20, 25, 30) };
            (fill, edge)
        }
        ShadingMode::XRay => {
            let fill = Color32::from_rgba_unmultiplied(mat_base_rgb[0], mat_base_rgb[1], mat_base_rgb[2], 75);
            let edge = if is_light_pool {
                Color32::TRANSPARENT
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 140)
            };
            (fill, edge)
        }
    }
}

/// True when `p` lies on the side of the section plane that is cut away.
fn section_removed(settings: &Viewport3DSettings, p: Point3D) -> bool {
    if !settings.enable_section_cut {
        return false;
    }
    let coord = match settings.section_axis {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    } as f32;
    if settings.section_invert {
        coord < settings.section_offset
    } else {
        coord > settings.section_offset
    }
}

/// Möller–Trumbore ray / triangle intersection (two-sided); returns the ray parameter of the hit.
fn ray_triangle(origin: Point3D, dir: Vector3D, v: &[Point3D; 3]) -> Option<f64> {
    let e1 = v[1] - v[0];
    let e2 = v[2] - v[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    let s = origin - v[0];
    let bu = s.dot(p) * inv_det;
    if !(0.0..=1.0).contains(&bu) {
        return None;
    }
    let q = s.cross(e1);
    let bv = dir.dot(q) * inv_det;
    if bv < 0.0 || bu + bv > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv_det;
    (t > NEAR_PLANE_MM).then_some(t)
}

/// Helper: 2D Point-in-Triangle test using cross-product signs
fn point_in_triangle(p: Pos2, p0: Pos2, p1: Pos2, p2: Pos2) -> bool {
    let sign = |p1: Pos2, p2: Pos2, p3: Pos2| -> f32 {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let d1 = sign(p, p0, p1);
    let d2 = sign(p, p1, p2);
    let d3 = sign(p, p2, p0);
    let has_neg = (d1 < -0.01) || (d2 < -0.01) || (d3 < -0.01);
    let has_pos = (d1 > 0.01) || (d2 > 0.01) || (d3 > 0.01);
    !(has_neg && has_pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketchcad_bim::Storey;

    #[test]
    fn screen_rays_and_projection_agree() {
        let vp = Viewport3D::new();
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 500.0));
        for pos in [Pos2::new(410.0, 270.0), Pos2::new(50.0, 60.0), Pos2::new(700.0, 480.0)] {
            let (eye, dir) = vp.screen_ray(rect, pos);
            let back = vp.project(rect, eye + dir * 7000.0).unwrap();
            assert!((back - pos).length() < 0.01, "{pos:?} -> {back:?}");
        }
    }

    #[test]
    fn models_back_onto_walls_and_sit_on_counters() {
        let vp = Viewport3D::new();
        let ac = sketchcad_bim::find_model("ac_indoor").unwrap();
        let depth = ac.geometry().size.1;
        // Wall face at x = 1000 facing +X
        let wall = vp.placement_for(&ac, (Point3D::new(1000.0, 500.0, 1200.0), Vector3D::X, true));
        assert!(wall.on_wall);
        assert!((wall.position.x - (1000.0 + depth * 0.5 + 5.0)).abs() < 1e-6 && (wall.position.y - 500.0).abs() < 1e-6);
        assert_eq!(wall.base_z, 2200.0, "wall units keep their mounting height");
        // The model's front (local −Y) turns to face out of the wall (+X)
        let (s, c) = wall.rotation_deg.to_radians().sin_cos();
        assert!((Vector2D::new(s, -c) - Vector2D::X).length() < 1e-9);

        let microwave = sketchcad_bim::find_model("microwave").unwrap();
        let floor = vp.placement_for(&microwave, (Point3D::new(0.0, 0.0, 0.0), Vector3D::Z, false));
        assert_eq!(floor.base_z, 850.0, "clicking the floor uses the default counter height");
        let counter = vp.placement_for(&microwave, (Point3D::new(0.0, 0.0, 900.0), Vector3D::Z, true));
        assert_eq!((counter.base_z, counter.on_wall), (900.0, false), "clicking a counter top sits on it");
    }

    fn viewport_with_bounds(min: Point3D, max: Point3D, doc: &BimDocument) -> Viewport3D {
        let mut vp = Viewport3D::new();
        vp.update_scene_bounds(Some((min, max)), doc);
        vp
    }

    #[test]
    fn section_range_follows_model_extent() {
        let doc = BimDocument::new("t");
        let mut vp = viewport_with_bounds(Point3D::new(-6000.0, -4000.0, 0.0), Point3D::new(6000.0, 4000.0, 18000.0), &doc);
        vp.settings.section_axis = 2;
        let (lo, hi) = vp.section_range();
        assert!(lo < 0.0 && hi > 18000.0, "Z range must cover the whole 18 m building: {lo}..{hi}");
        vp.settings.section_axis = 0;
        let (lo, hi) = vp.section_range();
        assert!(lo < -6000.0 && hi > 6000.0);
        assert_eq!(vp.default_section_offset(&doc), 0.0); // middle of X
    }

    #[test]
    fn z_default_is_ground_floor_plus_cut_height_and_out_of_range_cut_is_reset() {
        let mut doc = BimDocument::new("t");
        // Geometry shifted so the basement sits at 0 (as the IFC importer does)
        let storey = |name: &str, z: f64| Storey { name: name.into(), elevation: z + 1500.0, source_elevation: Some(z) };
        doc.set_storeys(vec![storey("BASS", -1500.0), storey("GL", 0.0), storey("2FL", 3850.0)]);

        let mut vp = Viewport3D::new();
        vp.settings.section_offset = 50_000.0; // far outside the model
        vp.update_scene_bounds(Some((Point3D::ZERO, Point3D::new(10_000.0, 10_000.0, 18_000.0))), &doc);
        // GL (authored ±0) is at z = 1500 in model space -> cut at 2700
        assert_eq!(vp.settings.section_offset, 2700.0);
    }
}

