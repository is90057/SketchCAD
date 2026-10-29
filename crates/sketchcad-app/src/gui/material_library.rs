//! Material library window (材質庫): browse the built-in presets by category, manage the
//! user's own materials (kept in a library file shared by all projects) and pick the
//! material used by the paint bucket.

use std::path::PathBuf;

use eframe::egui;
use sketchcad_bim::{Material, MaterialCategory, MaterialLibraryFile};
use sketchcad_geom::Color;

use crate::gui::app::{material_swatch, SketchCadApp};
use crate::gui::viewport_2d::ToolMode;

/// Where the user's own materials are stored between sessions.
fn user_library_path() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        var("APPDATA").map(|d| d.join("SketchCAD").join("materials.json"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support/SketchCAD/materials.json"))
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".config")))
            .map(|d| d.join("sketchcad").join("materials.json"))
    }
}

/// Material being created or edited in the library window.
struct MaterialDraft {
    /// Name of the material being edited; `None` when creating a new one.
    original: Option<String>,
    /// Whether the material lives in the user's library (not only in the open project).
    in_user_library: bool,
    name: String,
    category: MaterialCategory,
    color: egui::Color32,
    density_kg_m3: f64,
    cost_per_m3: f64,
    thermal_conductivity: f64,
    error: Option<String>,
}

impl MaterialDraft {
    fn from_material(material: &Material, original: Option<String>, in_user_library: bool) -> Self {
        let [r, g, b, a] = material.rgba();
        Self {
            original,
            in_user_library,
            name: material.name.clone(),
            category: material.category,
            color: egui::Color32::from_rgba_unmultiplied(r, g, b, a),
            density_kg_m3: material.density_kg_m3,
            cost_per_m3: material.cost_per_m3,
            thermal_conductivity: material.thermal_conductivity,
            error: None,
        }
    }

    fn blank() -> Self {
        let mut material = Material::custom("", Color::rgb(120, 150, 130));
        material.category = MaterialCategory::Paint;
        Self::from_material(&material, None, true)
    }

    fn to_material(&self) -> Material {
        let [r, g, b, a] = self.color.to_srgba_unmultiplied();
        Material {
            name: self.name.trim().to_string(),
            density_kg_m3: self.density_kg_m3,
            cost_per_m3: self.cost_per_m3,
            color: Color::rgba(r, g, b, a),
            thermal_conductivity: self.thermal_conductivity,
            category: self.category,
        }
    }
}

pub struct MaterialLibraryState {
    pub open: bool,
    /// The user's own materials, persisted to [`user_library_path`].
    pub user_materials: Vec<Material>,
    path: Option<PathBuf>,
    search: String,
    filter: Option<MaterialCategory>,
    draft: Option<MaterialDraft>,
}

impl MaterialLibraryState {
    /// Loads the user's library from disk; a missing or unreadable file gives an empty one.
    pub fn load() -> Self {
        let path = user_library_path();
        let user_materials = path
            .as_deref()
            .and_then(|p| MaterialLibraryFile::load(p).ok())
            .unwrap_or_default();
        Self {
            open: false,
            user_materials,
            path,
            search: String::new(),
            filter: None,
            draft: None,
        }
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("找不到使用者設定資料夾，材質僅保存在目前專案".to_string());
        };
        MaterialLibraryFile::save(path, &self.user_materials).map_err(|e| format!("材質庫儲存失敗: {e}"))
    }

    fn upsert(&mut self, material: Material, replacing: Option<&str>) {
        let key = replacing.unwrap_or(&material.name).to_string();
        match self.user_materials.iter_mut().find(|m| m.name == key) {
            Some(existing) => *existing = material,
            None => self.user_materials.push(material),
        }
    }

    fn matches(&self, material: &Material) -> bool {
        let query = self.search.trim().to_lowercase();
        self.filter.is_none_or(|c| c == material.category)
            && (query.is_empty() || material.name.to_lowercase().contains(&query))
    }
}

/// Actions collected while drawing the window, applied once it is closed over.
enum LibraryAction {
    Choose(Material),
    ApplyToSelection,
    Edit(Material, bool),
    Duplicate(Material),
    SaveToUser(Material),
    Remove(String),
    SaveDraft,
    Import,
    Export,
}

impl SketchCadApp {
    /// Floating material library. Shown while the paint bucket is active or when opened
    /// from the toolbar / View menu.
    pub(crate) fn render_material_library(&mut self, ctx: &egui::Context) {
        let mut actions: Vec<LibraryAction> = Vec::new();
        let mut open = true;
        let presets = Material::presets();
        let user_names: Vec<String> = self.material_library.user_materials.iter().map(|m| m.name.clone()).collect();
        let project_only: Vec<Material> = self
            .doc
            .custom_materials()
            .iter()
            .filter(|m| !user_names.contains(&m.name))
            .cloned()
            .collect();
        let has_selection = self.selected_id.is_some_and(|id| self.doc.get_element(id).is_some());

        egui::Window::new("🎨 材質庫 (Material Library)")
            .open(&mut open)
            .default_pos([90.0, 150.0])
            .default_width(300.0)
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                // Current material
                ui.horizontal(|ui| {
                    material_swatch(ui, &self.paint_material, true);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&self.paint_material.name).strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {:.0} kg/m³ · ${:.0}/m³ · λ {:.2}",
                                self.paint_material.category.label(),
                                self.paint_material.density_kg_m3,
                                self.paint_material.cost_per_m3,
                                self.paint_material.thermal_conductivity
                            ))
                            .small()
                            .weak(),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.tool_mode == ToolMode::Paint, "🪣 油漆桶 [K]")
                        .on_hover_text("點擊 2D / 3D 視口中的構件貼附目前材質；Alt + 點擊吸取材質")
                        .clicked()
                    {
                        self.tool_mode = if self.tool_mode == ToolMode::Paint { ToolMode::Select } else { ToolMode::Paint };
                    }
                    if ui
                        .add_enabled(has_selection, egui::Button::new("套用至選取構件"))
                        .on_disabled_hover_text("請先選取一個構件")
                        .clicked()
                    {
                        actions.push(LibraryAction::ApplyToSelection);
                    }
                });
                ui.separator();

                // Search & category filter
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.material_library.search).hint_text("🔍 搜尋材質").desired_width(130.0));
                    egui::ComboBox::from_id_salt("material_library_filter")
                        .selected_text(self.material_library.filter.map_or("全部分類", |c| c.label()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.material_library.filter, None, "全部分類");
                            for category in MaterialCategory::ALL {
                                ui.selectable_value(&mut self.material_library.filter, Some(category), category.label());
                            }
                        });
                });

                egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
                    // The user's own materials
                    let mine: Vec<&Material> =
                        self.material_library.user_materials.iter().filter(|m| self.material_library.matches(m)).collect();
                    ui.label(egui::RichText::new(format!("⭐ 我的材質 ({})", mine.len())).strong());
                    if self.material_library.user_materials.is_empty() {
                        ui.label(egui::RichText::new("尚無自訂材質，請於下方新增").small().weak());
                    }
                    ui.horizontal_wrapped(|ui| {
                        for material in mine {
                            let response = material_swatch(ui, material, *material == self.paint_material);
                            if response.clicked() {
                                actions.push(LibraryAction::Choose(material.clone()));
                            }
                            response.context_menu(|ui| {
                                if ui.button("✏ 編輯材質").clicked() {
                                    actions.push(LibraryAction::Edit(material.clone(), true));
                                    ui.close_menu();
                                }
                                if ui.button("⧉ 複製為新材質").clicked() {
                                    actions.push(LibraryAction::Duplicate(material.clone()));
                                    ui.close_menu();
                                }
                                if ui.button("🗑 從材質庫移除").clicked() {
                                    actions.push(LibraryAction::Remove(material.name.clone()));
                                    ui.close_menu();
                                }
                            });
                        }
                    });

                    // Materials carried by the open project but not in the user's library
                    let in_project: Vec<&Material> = project_only.iter().filter(|m| self.material_library.matches(m)).collect();
                    if !in_project.is_empty() {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!("📁 本專案材質 ({})", in_project.len())).strong())
                            .on_hover_text("隨專案檔帶入的自訂材質；右鍵可存入我的材質庫");
                        ui.horizontal_wrapped(|ui| {
                            for material in in_project {
                                let response = material_swatch(ui, material, *material == self.paint_material);
                                if response.clicked() {
                                    actions.push(LibraryAction::Choose(material.clone()));
                                }
                                response.context_menu(|ui| {
                                    if ui.button("⭐ 存入我的材質庫").clicked() {
                                        actions.push(LibraryAction::SaveToUser(material.clone()));
                                        ui.close_menu();
                                    }
                                    if ui.button("✏ 編輯材質").clicked() {
                                        actions.push(LibraryAction::Edit(material.clone(), false));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                    }

                    // Built-in presets, grouped by category
                    for category in MaterialCategory::ALL {
                        let group: Vec<&Material> = presets
                            .iter()
                            .filter(|m| m.category == category && self.material_library.matches(m))
                            .collect();
                        if group.is_empty() {
                            continue;
                        }
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(category.label()).small().strong());
                        ui.horizontal_wrapped(|ui| {
                            for material in group {
                                let response = material_swatch(ui, material, *material == self.paint_material);
                                if response.clicked() {
                                    actions.push(LibraryAction::Choose(material.clone()));
                                }
                                response.context_menu(|ui| {
                                    if ui.button("⧉ 以此為範本新增材質").clicked() {
                                        actions.push(LibraryAction::Duplicate(material.clone()));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                    }
                });

                ui.separator();
                self.material_editor_ui(ui, &mut actions);

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("📥 匯入材質庫...").on_hover_text("從 JSON 材質庫檔案加入材質").clicked() {
                        actions.push(LibraryAction::Import);
                    }
                    if ui
                        .add_enabled(!self.material_library.user_materials.is_empty(), egui::Button::new("📤 匯出..."))
                        .on_hover_text("將我的材質匯出為 JSON，可分享給其他使用者")
                        .clicked()
                    {
                        actions.push(LibraryAction::Export);
                    }
                });
                ui.label(egui::RichText::new("右鍵點擊材質可編輯、複製或移除").small().weak());
            });

        if !open {
            self.material_library.open = false;
            if self.tool_mode == ToolMode::Paint {
                self.tool_mode = ToolMode::Select;
            }
        }
        for action in actions {
            self.apply_library_action(action);
        }
    }

    /// "New material" form, or the editor of the material being edited.
    fn material_editor_ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<LibraryAction>) {
        let Some(draft) = &mut self.material_library.draft else {
            if ui.button("＋ 新增材質").clicked() {
                self.material_library.draft = Some(MaterialDraft::blank());
            }
            return;
        };

        let title = if draft.original.is_some() { "✏ 編輯材質" } else { "＋ 新增材質" };
        ui.label(egui::RichText::new(title).strong());
        let mut cancel = false;
        egui::Grid::new("material_editor_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("名稱");
            ui.add(egui::TextEdit::singleline(&mut draft.name).hint_text("例: 灰綠色乳膠漆").desired_width(170.0));
            ui.end_row();

            ui.label("分類");
            egui::ComboBox::from_id_salt("material_editor_category")
                .selected_text(draft.category.label())
                .show_ui(ui, |ui| {
                    for category in MaterialCategory::ALL {
                        ui.selectable_value(&mut draft.category, category, category.label());
                    }
                });
            ui.end_row();

            ui.label("顏色");
            egui::color_picker::color_edit_button_srgba(ui, &mut draft.color, egui::color_picker::Alpha::OnlyBlend)
                .on_hover_text("透明度 < 100% 會以半透明顯示 (例如玻璃)");
            ui.end_row();

            ui.label("密度");
            ui.add(egui::DragValue::new(&mut draft.density_kg_m3).speed(10.0).range(1.0..=25000.0).suffix(" kg/m³"));
            ui.end_row();

            ui.label("單價");
            ui.add(egui::DragValue::new(&mut draft.cost_per_m3).speed(100.0).range(0.0..=10_000_000.0).prefix("$").suffix(" /m³"));
            ui.end_row();

            ui.label("導熱係數");
            ui.add(egui::DragValue::new(&mut draft.thermal_conductivity).speed(0.01).range(0.01..=500.0).suffix(" W/m·K"));
            ui.end_row();
        });
        if let Some(error) = &draft.error {
            ui.colored_label(egui::Color32::from_rgb(240, 120, 110), error);
        }
        ui.horizontal(|ui| {
            let save_label = if draft.original.is_some() { "💾 儲存變更" } else { "＋ 加入材質庫並選用" };
            if ui.add_enabled(!draft.name.trim().is_empty(), egui::Button::new(save_label)).clicked() {
                actions.push(LibraryAction::SaveDraft);
            }
            if ui.button("取消").clicked() {
                cancel = true;
            }
        });
        if draft.original.is_some() {
            ui.label(egui::RichText::new("變更會同步更新所有使用此材質的構件").small().weak());
        }
        if cancel {
            self.material_library.draft = None;
        }
    }

    fn apply_library_action(&mut self, action: LibraryAction) {
        match action {
            LibraryAction::Choose(material) => self.paint_material = material,
            LibraryAction::ApplyToSelection => {
                if let Some(id) = self.selected_id {
                    let name = self.doc.get_element(id).map(|e| e.name().to_string()).unwrap_or_default();
                    self.save_undo_state();
                    self.doc.paint_element(id, self.paint_material.clone());
                    self.show_notify(format!("🎨 已將「{}」貼附至 {}", self.paint_material.name, name));
                }
            }
            LibraryAction::Edit(material, in_user_library) => {
                self.material_library.draft =
                    Some(MaterialDraft::from_material(&material, Some(material.name.clone()), in_user_library));
            }
            LibraryAction::Duplicate(material) => {
                let mut draft = MaterialDraft::from_material(&material, None, true);
                draft.name = format!("{} (自訂)", material.name);
                self.material_library.draft = Some(draft);
            }
            LibraryAction::SaveToUser(material) => {
                self.material_library.upsert(material.clone(), None);
                self.persist_library(format!("已存入我的材質庫: {}", material.name));
            }
            LibraryAction::Remove(name) => {
                self.material_library.user_materials.retain(|m| m.name != name);
                self.doc.remove_custom_material(&name);
                self.persist_library(format!("已移除材質: {name}（已上色的構件保留原顏色）"));
            }
            LibraryAction::SaveDraft => self.save_material_draft(),
            LibraryAction::Import => self.import_material_library(),
            LibraryAction::Export => self.export_material_library(),
        }
    }

    fn save_material_draft(&mut self) {
        let Some(draft) = &mut self.material_library.draft else { return };
        let material = draft.to_material();
        let original = draft.original.clone();
        let renamed = original.as_deref() != Some(material.name.as_str());
        let taken = self.material_library.user_materials.iter().chain(self.doc.custom_materials()).any(|m| m.name == material.name);
        if Material::is_preset_name(&material.name) {
            draft.error = Some("名稱與內建材質相同，請換一個名稱".to_string());
            return;
        }
        if renamed && taken {
            draft.error = Some("已有同名的自訂材質".to_string());
            return;
        }
        let in_user_library = draft.in_user_library;
        self.material_library.draft = None;

        match original {
            Some(old_name) => {
                if in_user_library {
                    self.material_library.upsert(material.clone(), Some(&old_name));
                }
                self.save_undo_state();
                let updated = self.doc.update_material(&old_name, material.clone());
                if self.paint_material.name == old_name {
                    self.paint_material = material.clone();
                }
                let msg = format!("已更新材質「{}」，同步 {} 個構件", material.name, updated);
                if in_user_library {
                    self.persist_library(msg);
                } else {
                    self.show_notify(msg);
                }
            }
            None => {
                self.material_library.upsert(material.clone(), None);
                self.doc.add_custom_material(material.clone());
                self.paint_material = material.clone();
                self.persist_library(format!("已新增材質: {}", material.name));
            }
        }
    }

    /// Saves the user's library and reports `success`, or the error instead.
    fn persist_library(&mut self, success: String) {
        match self.material_library.persist() {
            Ok(()) => self.show_notify(success),
            Err(err) => self.show_notify(format!("{success}；{err}")),
        }
    }

    fn import_material_library(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("匯入材質庫")
            .add_filter("SketchCAD 材質庫 (*.json)", &["json"])
            .pick_file()
        else {
            return;
        };
        match MaterialLibraryFile::load(&path) {
            Ok(materials) => {
                let mut count = 0;
                for material in materials.into_iter().filter(|m| !Material::is_preset_name(&m.name)) {
                    self.material_library.upsert(material, None);
                    count += 1;
                }
                self.persist_library(format!("已匯入 {count} 個材質"));
            }
            Err(err) => self.show_notify(format!("無法讀取材質庫 {}: {err}", path.display())),
        }
    }

    fn export_material_library(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("匯出我的材質")
            .set_file_name("sketchcad_materials.json")
            .add_filter("SketchCAD 材質庫 (*.json)", &["json"])
            .save_file()
        else {
            return;
        };
        match MaterialLibraryFile::save(&path, &self.material_library.user_materials) {
            Ok(()) => self.show_notify(format!("已匯出 {} 個材質至 {}", self.material_library.user_materials.len(), path.display())),
            Err(err) => self.show_notify(format!("匯出失敗: {err}")),
        }
    }
}
