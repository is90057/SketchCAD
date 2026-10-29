//! 3D model library browser (3D 模型百寶箱): pick a ready-made model by category and place
//! it in the 2D plan or the 3D view; import models (OBJ, SketchUp, model packs), turn
//! project elements into models and export models for other users or programs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use sketchcad_bim::{
    all_models, convert_imported_mesh, find_model, register_custom_model, unregister_custom_model, CustomModel, ModelCategory,
    ModelGeometry, ModelLibraryFile, ModelRef,
};
use sketchcad_core::Vector3D;
use sketchcad_geom::TriangleMesh;

use crate::gui::app::SketchCadApp;
use crate::gui::material_library::app_data_dir;
use crate::gui::viewport_2d::ToolMode;

/// Shaded thumbnail triangles in a unit square (y down), painted far to near.
type Thumbnail = Vec<([egui::Pos2; 3], egui::Color32)>;

/// Length unit of an imported file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportUnit {
    Millimetre,
    Centimetre,
    Metre,
    Inch,
}

impl ImportUnit {
    const ALL: [Self; 4] = [Self::Millimetre, Self::Centimetre, Self::Metre, Self::Inch];

    fn label(&self) -> &'static str {
        match self {
            Self::Millimetre => "公釐 (mm)",
            Self::Centimetre => "公分 (cm)",
            Self::Metre => "公尺 (m)",
            Self::Inch => "英吋 (in)",
        }
    }

    fn to_mm(self) -> f64 {
        match self {
            Self::Millimetre => 1.0,
            Self::Centimetre => 10.0,
            Self::Metre => 1000.0,
            Self::Inch => 25.4,
        }
    }

    /// Guess from the largest dimension: furniture-to-building sized objects are a few
    /// metres, so small numbers mean metres and medium ones centimetres.
    fn guess(max_dim: f64) -> Self {
        if max_dim < 50.0 {
            Self::Metre
        } else if max_dim < 600.0 {
            Self::Centimetre
        } else {
            Self::Millimetre
        }
    }
}

/// Mesh waiting for the user to confirm units, axes and name before joining the library.
struct PendingImport {
    source: String,
    raw: TriangleMesh,
    unit: ImportUnit,
    y_up: bool,
    name: String,
    category: ModelCategory,
    price: f64,
    elevation: f64,
    preview_key: Option<(ImportUnit, bool)>,
    preview: Option<(Thumbnail, (f64, f64, f64))>,
}

impl PendingImport {
    fn new(source: String, name: String, raw: TriangleMesh, unit: ImportUnit, y_up: bool) -> Self {
        Self {
            source,
            raw,
            unit,
            y_up,
            name,
            category: ModelCategory::Furniture,
            price: 0.0,
            elevation: 0.0,
            preview_key: None,
            preview: None,
        }
    }

    fn converted(&self) -> TriangleMesh {
        let mut mesh = self.raw.clone();
        convert_imported_mesh(&mut mesh, self.unit.to_mm(), self.y_up);
        mesh
    }

    fn to_model(&self) -> CustomModel {
        let mut model = CustomModel::from_mesh(self.name.trim(), self.category, self.converted());
        model.price = self.price;
        model.elevation = self.elevation;
        model
    }
}

/// Name / category / price editor of a user model.
struct ModelEdit {
    id: String,
    name: String,
    category: ModelCategory,
    price: f64,
    elevation: f64,
}

#[derive(Default)]
pub struct ModelBrowserState {
    pub open: bool,
    category: Option<ModelCategory>,
    /// Show only the user's own (imported) models
    mine_only: bool,
    search: String,
    thumbnails: HashMap<String, Thumbnail>,
    /// Ids of the models kept in the user's library file (shared by all projects)
    user_models: Vec<String>,
    path: Option<PathBuf>,
    pending: Option<PendingImport>,
    editing: Option<ModelEdit>,
    /// `.blend` file being converted by Blender in the background
    converting: Option<(String, std::sync::mpsc::Receiver<Result<TriangleMesh, String>>)>,
}

impl ModelBrowserState {
    /// Loads and registers the user's own models.
    pub fn load() -> Self {
        let path = app_data_dir().map(|d| d.join(format!("models.{}", ModelLibraryFile::EXTENSION)));
        let models = path.as_deref().and_then(|p| ModelLibraryFile::load(p).ok()).unwrap_or_default();
        let user_models = models.into_iter().map(|m| register_custom_model(m).id().to_string()).collect();
        Self { path, user_models, ..Default::default() }
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("找不到使用者設定資料夾，模型僅保存在目前專案".into());
        };
        let models: Vec<CustomModel> = self
            .user_models
            .iter()
            .filter_map(|id| match find_model(id) {
                Some(ModelRef::Custom(entry)) => Some(entry.model.clone()),
                _ => None,
            })
            .collect();
        ModelLibraryFile::save(path, &models).map_err(|e| format!("模型庫儲存失敗: {e}"))
    }
}

/// Renders a model into a flat-shaded three-quarter view from the front left.
fn render_thumbnail(geometry: &ModelGeometry, body_color: [u8; 4]) -> Thumbnail {
    let mesh = &geometry.mesh;
    let to_camera = Vector3D::new(-0.55, -1.0, 0.75).normalize();
    let forward = -to_camera;
    let right = forward.cross(Vector3D::Z).normalize();
    let up = right.cross(forward).normalize();
    let light = Vector3D::new(-0.3, -0.7, 1.0).normalize();

    let mut tris: Vec<(f64, [(f64, f64); 3], egui::Color32)> = Vec::with_capacity(mesh.triangle_count());
    for t in 0..mesh.triangle_count() {
        let [a, b, c] = [0, 1, 2].map(|k| mesh.positions[mesh.indices[t * 3 + k] as usize]);
        let normal = (b - a).cross(c - a);
        if normal.length() < 1e-9 {
            continue;
        }
        let normal = normal.normalize();
        if normal.dot(to_camera) <= 0.0 {
            continue; // back face
        }
        let [r, g, bl, _] = mesh.face_color(t).unwrap_or(body_color);
        let shade = 0.4 + 0.6 * normal.dot(light).max(0.0);
        let tint = |v: u8| (v as f64 * shade).round().clamp(0.0, 255.0) as u8;
        let colour = egui::Color32::from_rgb(tint(r), tint(g), tint(bl));
        let project = |p: Vector3D| (p.dot(right), -p.dot(up));
        let depth = (a + b + c).dot(to_camera) / 3.0;
        tris.push((depth, [project(a), project(b), project(c)], colour));
    }
    tris.sort_by(|x, y| x.0.total_cmp(&y.0));

    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for (_, pts, _) in &tris {
        for &(x, y) in pts {
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
        }
    }
    let extent = (max.0 - min.0).max(max.1 - min.1).max(1e-6);
    let offset = ((extent - (max.0 - min.0)) * 0.5, (extent - (max.1 - min.1)) * 0.5);
    tris.into_iter()
        .map(|(_, pts, colour)| {
            let unit = pts.map(|(x, y)| egui::pos2(((x - min.0 + offset.0) / extent) as f32, ((y - min.1 + offset.1) / extent) as f32));
            (unit, colour)
        })
        .collect()
}

fn paint_thumbnail(painter: &egui::Painter, rect: egui::Rect, thumbnail: &Thumbnail) {
    let mut mesh = egui::Mesh::default();
    let map = |p: egui::Pos2| rect.min + egui::vec2(p.x * rect.width(), p.y * rect.height());
    for (pts, colour) in thumbnail {
        let base = mesh.vertices.len() as u32;
        for p in pts {
            mesh.colored_vertex(map(*p), *colour);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Chinese part of a catalogue name ("雙門冰箱 (Refrigerator)" → "雙門冰箱").
fn short_name(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

fn file_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("模型").to_string()
}

/// Actions collected while drawing the window, applied afterwards.
enum BrowserAction {
    Pick(String),
    Import,
    FromSelection,
    ExportPack(String),
    ExportObj(String),
    ExportMine,
    SaveToMine(String),
    Edit(String),
    SaveEdit,
    Remove(String),
    ConfirmImport,
    CancelImport,
    CancelEdit,
}

impl SketchCadApp {
    /// Floating 3D model library. Shown while the Model tool is active or when opened from
    /// the toolbar / View menu.
    pub(crate) fn render_model_browser(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut actions: Vec<BrowserAction> = Vec::new();
        let selected = self.viewport_2d.model_id.clone();
        let has_selection = self.selected_id.is_some_and(|id| self.doc.get_element(id).is_some());
        let models = all_models();
        self.poll_blend_conversion(ctx);

        egui::Window::new("📦 3D 模型百寶箱 (Model Library)")
            .open(&mut open)
            .default_pos([380.0, 140.0])
            .default_size([560.0, 560.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                let state = &mut self.model_browser;
                if let Some((source, _)) = &state.converting {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.label(format!("正在以 Blender 轉換 {source} …"));
                        ui.label(egui::RichText::new("大型場景可能需要數十秒").small().weak());
                    });
                    ui.add_space(20.0);
                    return;
                }
                if let Some(pending) = &mut state.pending {
                    import_panel_ui(ui, pending, &mut actions);
                    return;
                }

                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(state.category.is_none() && !state.mine_only, format!("全部 ({})", models.len())).clicked() {
                        (state.category, state.mine_only) = (None, false);
                    }
                    let mine = models.iter().filter(|m| m.is_custom()).count();
                    if ui.selectable_label(state.mine_only, format!("⭐ 我的模型 ({mine})")).clicked() {
                        (state.category, state.mine_only) = (None, true);
                    }
                    for category in ModelCategory::ALL {
                        let count = models.iter().filter(|m| m.category() == category).count();
                        if ui.selectable_label(state.category == Some(category), format!("{} ({count})", category.label())).clicked() {
                            (state.category, state.mine_only) = (Some(category), false);
                        }
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("🔍 搜尋模型 (例: 冰箱、tree、車)").desired_width(f32::INFINITY));

                if let Some(edit) = &mut state.editing {
                    ui.separator();
                    model_edit_ui(ui, edit, &mut actions);
                }
                ui.separator();

                let query = state.search.trim().to_lowercase();
                let visible: Vec<&ModelRef> = models
                    .iter()
                    .filter(|m| state.category.is_none_or(|c| c == m.category()))
                    .filter(|m| !state.mine_only || m.is_custom())
                    .filter(|m| query.is_empty() || m.name().to_lowercase().contains(&query) || m.id().contains(&query))
                    .collect();

                egui::ScrollArea::vertical().auto_shrink([false, false]).max_height((ui.available_height() - 64.0).max(120.0)).show(ui, |ui| {
                    if visible.is_empty() {
                        let hint = if state.mine_only { "尚無自訂模型 — 請用下方「📥 匯入模型」或「➕ 由選取構件建立」" } else { "找不到符合的模型" };
                        ui.label(egui::RichText::new(hint).weak());
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                        for model in visible {
                            let in_mine = state.user_models.iter().any(|id| id == model.id());
                            model_card(ui, model, selected.as_deref() == Some(model.id()), in_mine, &mut state.thumbnails, &mut actions);
                        }
                    });
                });

                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("📥 匯入模型...")
                        .on_hover_text("匯入 OBJ、FBX、Blender (.blend)、SketchUp (.skp) 或 SketchCAD 模型包 (.skmodel)")
                        .clicked()
                    {
                        actions.push(BrowserAction::Import);
                    }
                    if ui
                        .add_enabled(has_selection, egui::Button::new("➕ 由選取構件建立"))
                        .on_hover_text("把目前選取的構件 (含顏色) 存成百寶箱模型")
                        .on_disabled_hover_text("請先在 2D / 3D 視圖選取一個構件")
                        .clicked()
                    {
                        actions.push(BrowserAction::FromSelection);
                    }
                    if ui
                        .add_enabled(!state.user_models.is_empty(), egui::Button::new("📤 匯出我的模型..."))
                        .on_hover_text("將「我的模型」全部匯出為一個模型包 (.skmodel)，可分享給其他使用者")
                        .clicked()
                    {
                        actions.push(BrowserAction::ExportMine);
                    }
                });
                let hint = match selected.as_deref().and_then(find_model) {
                    Some(model) => format!("已選: {} — 在 2D 平面或 3D 視圖點擊放置，R 旋轉 90°；右鍵卡片可匯出", short_name(model.name())),
                    None => "點選模型卡片後，在 2D 平面或 3D 視圖點擊放置；右鍵卡片可匯出 / 編輯".to_string(),
                };
                ui.label(egui::RichText::new(hint).small().color(egui::Color32::from_rgb(150, 235, 180)));
            });

        if !open {
            self.model_browser.open = false;
            self.model_browser.pending = None;
            if self.tool_mode == ToolMode::Model {
                self.tool_mode = ToolMode::Select;
            }
        }
        for action in actions {
            self.apply_browser_action(action);
        }
    }

    fn apply_browser_action(&mut self, action: BrowserAction) {
        match action {
            BrowserAction::Pick(id) => {
                self.viewport_2d.model_id = Some(id);
                self.tool_mode = ToolMode::Model;
            }
            BrowserAction::Import => self.import_model_file(),
            BrowserAction::FromSelection => {
                let Some(id) = self.selected_id else { return };
                let Some(mesh) = self.doc.element_mesh_with_colors(id) else { return };
                let name = self.doc.get_element(id).map(|e| e.name().to_string()).unwrap_or_default();
                self.model_browser.pending = Some(PendingImport::new(format!("構件「{name}」"), name, mesh, ImportUnit::Millimetre, false));
            }
            BrowserAction::ExportPack(id) => {
                let Some(model) = find_model(&id) else { return };
                let Some(path) = save_dialog("匯出模型包", &format!("{}.skmodel", short_name(model.name())), ModelLibraryFile::EXTENSION) else {
                    return;
                };
                match ModelLibraryFile::save(&path, &[model.to_custom_model()]) {
                    Ok(()) => self.show_notify(format!("已匯出模型包: {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出失敗: {e}")),
                }
            }
            BrowserAction::ExportObj(id) => {
                let Some(model) = find_model(&id) else { return };
                let Some(path) = save_dialog("匯出 OBJ 模型", &format!("{}.obj", short_name(model.name())), "obj") else {
                    return;
                };
                match sketchcad_io::write_obj(&path, &model.geometry().mesh, model.body_color(), model.name()) {
                    Ok(()) => self.show_notify(format!("已匯出 OBJ + MTL (公尺, Y 軸朝上): {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出失敗: {e}")),
                }
            }
            BrowserAction::ExportMine => {
                let Some(path) = save_dialog("匯出我的模型", "my_models.skmodel", ModelLibraryFile::EXTENSION) else { return };
                let models: Vec<CustomModel> =
                    self.model_browser.user_models.iter().filter_map(|id| find_model(id)).map(|m| m.to_custom_model()).collect();
                match ModelLibraryFile::save(&path, &models) {
                    Ok(()) => self.show_notify(format!("已匯出 {} 個模型至 {}", models.len(), path.display())),
                    Err(e) => self.show_notify(format!("匯出失敗: {e}")),
                }
            }
            BrowserAction::SaveToMine(id) => {
                if !self.model_browser.user_models.contains(&id) {
                    self.model_browser.user_models.push(id);
                }
                self.persist_models("已存入我的百寶箱".into());
            }
            BrowserAction::Edit(id) => {
                if let Some(ModelRef::Custom(entry)) = find_model(&id) {
                    let m = &entry.model;
                    self.model_browser.editing =
                        Some(ModelEdit { id, name: m.name.clone(), category: m.category, price: m.price, elevation: m.elevation });
                }
            }
            BrowserAction::SaveEdit => {
                let Some(edit) = self.model_browser.editing.take() else { return };
                let Some(ModelRef::Custom(entry)) = find_model(&edit.id) else { return };
                let mut model = entry.model.clone();
                (model.name, model.category, model.price, model.elevation) = (edit.name.trim().to_string(), edit.category, edit.price, edit.elevation);
                register_custom_model(model);
                if self.model_browser.user_models.contains(&edit.id) {
                    self.persist_models("已更新模型資訊".into());
                }
            }
            BrowserAction::Remove(id) => {
                self.model_browser.user_models.retain(|m| *m != id);
                let in_use = self.doc.custom_models_in_use().iter().any(|m| m.id == id);
                if !in_use {
                    unregister_custom_model(&id);
                    self.model_browser.thumbnails.remove(&id);
                    if self.viewport_2d.model_id.as_deref() == Some(id.as_str()) {
                        self.viewport_2d.model_id = None;
                    }
                }
                let msg = if in_use { "已從我的百寶箱移除 (本專案仍在使用，保留於本專案模型)" } else { "已從百寶箱移除模型" };
                self.persist_models(msg.into());
            }
            BrowserAction::CancelImport => self.model_browser.pending = None,
            BrowserAction::CancelEdit => self.model_browser.editing = None,
            BrowserAction::ConfirmImport => {
                let Some(pending) = self.model_browser.pending.take() else { return };
                let model = pending.to_model();
                let name = model.name.clone();
                let id = register_custom_model(model).id().to_string();
                self.model_browser.user_models.push(id.clone());
                self.model_browser.mine_only = true;
                self.model_browser.category = None;
                self.viewport_2d.model_id = Some(id);
                self.tool_mode = ToolMode::Model;
                self.persist_models(format!("已加入百寶箱: {name} — 點擊 2D / 3D 視圖即可放置"));
            }
        }
    }

    /// Picks up the result of a background Blender conversion.
    fn poll_blend_conversion(&mut self, ctx: &egui::Context) {
        let Some((source, rx)) = &self.model_browser.converting else { return };
        match rx.try_recv() {
            Ok(result) => {
                let source = source.clone();
                self.model_browser.converting = None;
                match result {
                    // Blender's OBJ export is in metres with Y up
                    Ok(mesh) => {
                        let name = source.trim_end_matches(".blend").to_string();
                        self.model_browser.pending = Some(PendingImport::new(source, name, mesh, ImportUnit::Metre, true));
                    }
                    Err(e) => self.show_notify(format!("無法匯入 Blender 檔案: {e}")),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.model_browser.converting = None;
                self.show_notify("Blender 轉換意外中止");
            }
        }
    }

    fn persist_models(&mut self, success: String) {
        match self.model_browser.persist() {
            Ok(()) => self.show_notify(success),
            Err(err) => self.show_notify(format!("{success}；{err}")),
        }
    }

    fn import_model_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("匯入模型至百寶箱")
            .add_filter("3D 模型 (*.obj, *.fbx, *.blend, *.skp, *.skmodel)", &["obj", "fbx", "blend", "skp", "skmodel"])
            .add_filter("Wavefront OBJ (*.obj)", &["obj"])
            .add_filter("Autodesk FBX (*.fbx)", &["fbx"])
            .add_filter("Blender (*.blend)", &["blend"])
            .add_filter("SketchUp 模型 (*.skp)", &["skp"])
            .add_filter("SketchCAD 模型包 (*.skmodel)", &["skmodel"])
            .pick_file()
        else {
            return;
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let source = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        match ext.as_str() {
            "skmodel" => match ModelLibraryFile::load(&path) {
                Ok(models) => {
                    let count = models.len();
                    for model in models {
                        let id = register_custom_model(model).id().to_string();
                        self.model_browser.thumbnails.remove(&id);
                        if !self.model_browser.user_models.contains(&id) {
                            self.model_browser.user_models.push(id);
                        }
                    }
                    self.model_browser.mine_only = true;
                    self.persist_models(format!("已從 {source} 匯入 {count} 個模型"));
                }
                Err(e) => self.show_notify(format!("無法讀取模型包: {e}")),
            },
            "obj" => match sketchcad_io::read_obj(&path) {
                Ok(mesh) => {
                    let max_dim = mesh.bbox().map_or(0.0, |b| b.size().max_element());
                    self.model_browser.pending = Some(PendingImport::new(source, file_stem(&path), mesh, ImportUnit::guess(max_dim), true));
                }
                Err(e) => self.show_notify(format!("無法匯入 OBJ: {e}")),
            },
            "skp" => match sketchcad_io::SkpImporter::import_from_file(&path) {
                Ok(doc) => {
                    let mut mesh = TriangleMesh::new();
                    for elem in doc.all_elements() {
                        if let Some(part) = doc.element_mesh_with_colors(elem.id()) {
                            mesh.append(&part);
                        }
                    }
                    if mesh.triangle_count() == 0 {
                        self.show_notify("SketchUp 檔案中沒有可匯入的幾何");
                    } else {
                        self.model_browser.pending = Some(PendingImport::new(source, file_stem(&path), mesh, ImportUnit::Millimetre, false));
                    }
                }
                Err(e) => self.show_notify(format!("無法匯入 SketchUp 模型: {e}")),
            },
            "fbx" => match sketchcad_io::read_fbx(&path) {
                // FBX carries its own units; the reader returns millimetres
                Ok(import) => {
                    self.model_browser.pending =
                        Some(PendingImport::new(source, file_stem(&path), import.mesh, ImportUnit::Millimetre, import.y_up));
                }
                Err(e) => self.show_notify(format!("無法匯入 FBX: {e}")),
            },
            "blend" => {
                if sketchcad_io::find_blender().is_none() {
                    self.show_notify(sketchcad_io::BlendError::BlenderNotFound.to_string());
                    return;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                let ctx = self.model_browser_ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(sketchcad_io::read_blend(&path).map_err(|e| e.to_string()));
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                });
                self.model_browser.converting = Some((source, rx));
            }
            _ => self.show_notify("不支援的模型格式，請選擇 .obj、.fbx、.blend、.skp 或 .skmodel"),
        }
    }
}

fn save_dialog(title: &str, file_name: &str, ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).set_file_name(file_name).add_filter(ext.to_uppercase(), &[ext]).save_file()
}

/// One clickable model card with thumbnail, name, size and a context menu.
fn model_card(
    ui: &mut egui::Ui,
    model: &ModelRef,
    is_selected: bool,
    in_mine: bool,
    thumbnails: &mut HashMap<String, Thumbnail>,
    actions: &mut Vec<BrowserAction>,
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(112.0, 118.0), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let bg = if is_selected {
        egui::Color32::from_rgb(40, 70, 105)
    } else if response.hovered() {
        egui::Color32::from_rgb(48, 54, 64)
    } else {
        egui::Color32::from_rgb(34, 38, 45)
    };
    painter.rect_filled(rect, 6.0, bg);
    let geometry = model.geometry();
    let thumb = thumbnails.entry(model.id().to_string()).or_insert_with(|| render_thumbnail(&geometry, model.body_color()));
    paint_thumbnail(&painter, egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 6.0), egui::vec2(88.0, 82.0)), thumb);
    if model.is_custom() {
        let badge = if in_mine { "⭐" } else { "📁" };
        painter.text(rect.min + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, badge, egui::FontId::proportional(12.0), egui::Color32::GOLD);
    }
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 18.0),
        egui::Align2::CENTER_CENTER,
        short_name(model.name()),
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(225, 230, 238),
    );
    let (w, d, h) = geometry.size;
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 5.0),
        egui::Align2::CENTER_CENTER,
        format!("{:.1}×{:.1}×{:.1}m", w / 1000.0, d / 1000.0, h / 1000.0),
        egui::FontId::proportional(9.5),
        egui::Color32::from_rgb(150, 160, 175),
    );
    if is_selected {
        painter.rect_stroke(rect, 6.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(110, 190, 255)), egui::StrokeKind::Inside);
    }
    let price = if model.price() > 0.0 { format!("參考單價 NT$ {:.0}", model.price()) } else { "配景 (不計價)".to_string() };
    let source = match (model.is_custom(), in_mine) {
        (false, _) => "內建模型",
        (true, true) => "⭐ 我的模型",
        (true, false) => "📁 本專案模型 (右鍵可存入我的百寶箱)",
    };
    let response = response.on_hover_text(format!(
        "{}\n{} · {:.0}×{:.0}×{:.0} mm · {} 個三角面\n{}{}\n{}",
        model.name(),
        model.category().label(),
        w,
        d,
        h,
        geometry.mesh.triangle_count(),
        price,
        if model.elevation() > 0.0 { format!("\n預設離地 {:.0} mm", model.elevation()) } else { String::new() },
        source
    ));
    if response.clicked() {
        actions.push(BrowserAction::Pick(model.id().to_string()));
    }
    response.context_menu(|ui| {
        if ui.button("📤 匯出模型包 (.skmodel)...").clicked() {
            actions.push(BrowserAction::ExportPack(model.id().to_string()));
            ui.close_menu();
        }
        if ui.button("📤 匯出 OBJ (+MTL)...").clicked() {
            actions.push(BrowserAction::ExportObj(model.id().to_string()));
            ui.close_menu();
        }
        if model.is_custom() {
            ui.separator();
            if !in_mine && ui.button("⭐ 存入我的百寶箱").clicked() {
                actions.push(BrowserAction::SaveToMine(model.id().to_string()));
                ui.close_menu();
            }
            if ui.button("✏ 編輯名稱與分類").clicked() {
                actions.push(BrowserAction::Edit(model.id().to_string()));
                ui.close_menu();
            }
            if in_mine && ui.button("🗑 從百寶箱移除").clicked() {
                actions.push(BrowserAction::Remove(model.id().to_string()));
                ui.close_menu();
            }
        }
    });
}

fn category_combo(ui: &mut egui::Ui, id: &str, category: &mut ModelCategory) {
    egui::ComboBox::from_id_salt(id).selected_text(category.label()).show_ui(ui, |ui| {
        for c in ModelCategory::ALL {
            ui.selectable_value(category, c, c.label());
        }
    });
}

fn model_edit_ui(ui: &mut egui::Ui, edit: &mut ModelEdit, actions: &mut Vec<BrowserAction>) {
    ui.label(egui::RichText::new("✏ 編輯模型資訊").strong());
    egui::Grid::new("model_edit_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("名稱");
        ui.text_edit_singleline(&mut edit.name);
        ui.end_row();
        ui.label("分類");
        category_combo(ui, "model_edit_category", &mut edit.category);
        ui.end_row();
        ui.label("單價");
        ui.add(egui::DragValue::new(&mut edit.price).speed(100.0).range(0.0..=10_000_000.0).prefix("NT$ "));
        ui.end_row();
        ui.label("預設離地");
        ui.add(egui::DragValue::new(&mut edit.elevation).speed(10.0).range(0.0..=20_000.0).suffix(" mm"));
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.add_enabled(!edit.name.trim().is_empty(), egui::Button::new("💾 儲存")).clicked() {
            actions.push(BrowserAction::SaveEdit);
        }
        if ui.button("取消").clicked() {
            actions.push(BrowserAction::CancelEdit);
        }
    });
}

/// Settings and preview of a model being imported.
fn import_panel_ui(ui: &mut egui::Ui, pending: &mut PendingImport, actions: &mut Vec<BrowserAction>) {
    ui.label(egui::RichText::new(format!("📥 匯入模型 — {}", pending.source)).strong());
    ui.label(egui::RichText::new(format!("{} 個三角面", pending.raw.triangle_count())).small().weak());
    ui.separator();

    let key = (pending.unit, pending.y_up);
    if pending.preview_key != Some(key) {
        let model = CustomModel::from_mesh("", pending.category, pending.converted());
        let geometry = ModelGeometry::from_mesh(model.mesh);
        pending.preview = Some((render_thumbnail(&geometry, model.body_color), geometry.size));
        pending.preview_key = Some(key);
    }

    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(170.0, 170.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 6.0, egui::Color32::from_rgb(34, 38, 45));
        if let Some((thumb, _)) = &pending.preview {
            paint_thumbnail(ui.painter(), rect.shrink(10.0), thumb);
        }
        ui.vertical(|ui| {
            egui::Grid::new("import_grid").num_columns(2).spacing([8.0, 5.0]).show(ui, |ui| {
                ui.label("名稱");
                ui.text_edit_singleline(&mut pending.name);
                ui.end_row();
                ui.label("分類");
                category_combo(ui, "import_category", &mut pending.category);
                ui.end_row();
                ui.label("檔案單位");
                egui::ComboBox::from_id_salt("import_unit").selected_text(pending.unit.label()).show_ui(ui, |ui| {
                    for unit in ImportUnit::ALL {
                        ui.selectable_value(&mut pending.unit, unit, unit.label());
                    }
                });
                ui.end_row();
                ui.label("座標軸");
                ui.checkbox(&mut pending.y_up, "Y 軸朝上 (OBJ / Blender / 3ds Max 匯出常見)");
                ui.end_row();
                ui.label("單價");
                ui.add(egui::DragValue::new(&mut pending.price).speed(100.0).range(0.0..=10_000_000.0).prefix("NT$ "));
                ui.end_row();
                ui.label("預設離地");
                ui.add(egui::DragValue::new(&mut pending.elevation).speed(10.0).range(0.0..=20_000.0).suffix(" mm"));
                ui.end_row();
            });
            if let Some((_, (w, d, h))) = pending.preview {
                let warn = w.max(d).max(h) > 30_000.0 || w.max(d).max(h) < 50.0;
                let text = format!("匯入後尺寸: {w:.0} × {d:.0} × {h:.0} mm");
                let colour = if warn { egui::Color32::from_rgb(255, 170, 90) } else { egui::Color32::from_rgb(150, 235, 180) };
                ui.label(egui::RichText::new(text).color(colour));
                if warn {
                    ui.label(egui::RichText::new("尺寸看起來不合理，請確認檔案單位").small().color(colour));
                }
            }
        });
    });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.add_enabled(!pending.name.trim().is_empty(), egui::Button::new("✔ 加入我的百寶箱")).clicked() {
            actions.push(BrowserAction::ConfirmImport);
        }
        if ui.button("取消").clicked() {
            actions.push(BrowserAction::CancelImport);
        }
    });
}
