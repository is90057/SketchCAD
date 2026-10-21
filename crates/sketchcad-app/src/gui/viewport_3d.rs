use crate::gui::doc_watch::DocWatcher;
use crate::gui::plan_sheets::{main_cluster, CLUSTER_MARGIN};
use crate::gui::gpu_scene::{EdgeVertex, FaceVertex, GpuScene, SceneGeometry, SceneUniforms, TranslucentRange};
use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Stroke, Ui};
use sketchcad_bim::{BimDocument, ElementKind};
use sketchcad_core::{ElementId, Point3D, Vector3D};
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

pub struct Viewport3D {
    pub camera: OrbitCamera3D,
    pub push_pull_distance: f64,
    pub settings: Viewport3DSettings,
    gpu: Option<GpuScene>,
    cache: Option<SceneCache>,
    doc_watcher: DocWatcher,
    /// Animated camera turn requested from the ViewCube: target (yaw, pitch)
    view_anim: Option<(f64, f64)>,
}

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
        }
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
        let max_dim = size.x.max(size.y).max(size.z).max(5000.0);
        self.camera.target = center;
        self.camera.distance = (max_dim * 1.5).clamp(2000.0, 150_000.0);
        self.camera.yaw_rad = HOME_YAW;
        self.camera.pitch_rad = HOME_PITCH;
    }

    pub fn render(&mut self, ui: &mut Ui, doc: &BimDocument, selected_id: &mut Option<ElementId>) {
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

        // 1. Interactive Orbit, Pan & Zoom
        if response.dragged_by(egui::PointerButton::Primary) {
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
                egui::vec2(315.0, 480.0),
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
                            ui.horizontal(|ui| {
                                ui.label("切面軸向:");
                                ui.radio_value(&mut self.settings.section_axis, 0, "X");
                                ui.radio_value(&mut self.settings.section_axis, 1, "Y");
                                ui.radio_value(&mut self.settings.section_axis, 2, "Z (平剖)");
                            });
                            ui.horizontal(|ui| {
                                ui.label("切面位置:");
                                let (min, max) = match self.settings.section_axis {
                                    0 => (-2000.0, 16000.0),
                                    1 => (-2000.0, 16000.0),
                                    _ => (0.0, 6000.0),
                                };
                                ui.add(egui::Slider::new(&mut self.settings.section_offset, min..=max).step_by(50.0).suffix(" mm"));
                            });
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
    /// Eye position plus the right-handed camera basis (forward, right, up) used by `look_at_rh`.
    fn camera_basis(&self) -> (Point3D, Vector3D, Vector3D, Vector3D) {
        let eye = self.camera.eye_position();
        let forward = (self.camera.target - eye).normalize();
        let right = forward.cross(Vector3D::Z).normalize();
        let up = right.cross(forward);
        (eye, forward, right, up)
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
            let material = material_for(kind, &self.settings);

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

                let (fill, edge) = shade_triangle(&self.settings, kind, material, face_normal, centroid, light_dir);
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

        for elem in doc.all_elements() {
            if !doc.is_visible(elem.id()) {
                continue;
            }
            let kind = elem.kind();
            let mut mesh = elem.to_3d_mesh();
            mesh.compute_crease_edges(CREASE_ANGLE_DEG.to_radians());
            let material = material_for(kind, &self.settings);

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

                let (fill_color, edge_color) = shade_triangle(&self.settings, kind, material, face_normal, centroid, light_dir);
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

/// Architectural material palette: (base sRGB, translucent, alpha).
fn material_for(kind: ElementKind, settings: &Viewport3DSettings) -> ([u8; 3], bool, u8) {
    match kind {
        ElementKind::Stairs => ([215, 160, 95], false, 255),
        ElementKind::Railing => ([210, 220, 235], true, 180),
        ElementKind::Wall => ([235, 235, 230], false, 255),
        ElementKind::Column => ([165, 175, 185], false, 255),
        ElementKind::Beam => ([95, 115, 135], false, 255),
        ElementKind::Slab => ([190, 195, 200], false, 255),
        ElementKind::Ceiling => {
            if settings.transparent_ceiling {
                ([230, 240, 250], true, 60)
            } else {
                ([245, 245, 240], false, 255)
            }
        }
        ElementKind::Door => ([175, 115, 65], false, 255),
        ElementKind::Window => ([135, 210, 245], true, 130),
        ElementKind::Roof => ([200, 95, 70], false, 255),
        ElementKind::Ramp => ([175, 180, 190], false, 255),
        ElementKind::CurtainWall => ([120, 205, 240], true, 140),
        ElementKind::CurtainGrid => ([100, 180, 220], true, 120),
        ElementKind::Mullion => ([70, 75, 85], false, 255),
        ElementKind::Space => ([200, 225, 240], true, 80), // Translucent floor finish plate
        ElementKind::Furniture => ([165, 125, 85], false, 255), // Warm solid wood / upholstery
        ElementKind::LightFixture => ([48, 52, 58], false, 255), // Luminaire hardware (chassis / trim)
    }
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

