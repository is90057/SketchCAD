//! Layer tools (圖層): the layer properties manager (LA), isolation (LAYISO / LAYUNISO),
//! layer walk (LAYWALK), layer delete (LAYDEL) and copy to layer (COPYTOLAYER).

use std::collections::{HashMap, HashSet};

use eframe::egui;
use sketchcad_bim::{Layer, LineType, LAYER_ZERO};
use sketchcad_core::ElementId;

use crate::gui::app::SketchCadApp;

/// Layer commands of the ribbon and command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCommand {
    Manager,
    Isolate,
    Unisolate,
    Walk,
    Delete,
    CopyToLayer,
}

/// COPYTOLAYER dialog.
struct CopyToLayer {
    /// Copy the selection (true) or every object on `source_layer`
    selection: bool,
    source_layer: String,
    target: String,
}

#[derive(Default)]
pub struct LayerUiState {
    pub manager_open: bool,
    new_name: String,
    /// Layer being renamed and the edited name
    renaming: Option<(String, String)>,
    walk_open: bool,
    walk_selection: HashSet<String>,
    walk_restore: bool,
    /// LAYDEL dialog: layer to delete
    laydel: Option<String>,
    copy_to_layer: Option<CopyToLayer>,
    error: Option<String>,
}

fn rgba_color(rgba: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Small colour chip of a layer (white outline = "by object").
pub fn layer_swatch(ui: &mut egui::Ui, layer: &Layer) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    match layer.color {
        Some(c) => ui.painter().rect_filled(rect, 2.0, rgba_color(c)),
        None => ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(200)), egui::StrokeKind::Inside),
    };
}

impl SketchCadApp {
    /// Element count per layer.
    fn layer_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for e in self.doc.all_elements() {
            *counts.entry(self.doc.layer_of(e.id())).or_insert(0) += 1;
        }
        counts
    }

    pub(crate) fn layer_command(&mut self, command: LayerCommand) {
        let ui = &mut self.layer_ui;
        match command {
            LayerCommand::Manager => ui.manager_open = true,
            LayerCommand::Isolate => match self.selected_id {
                Some(id) => {
                    self.save_undo_state();
                    let kept = self.doc.isolate_layers_of(&[id]);
                    self.show_notify(format!("已隔離圖層: {} (LAYUNISO 取消隔離)", kept.join(", ")));
                }
                None => self.show_notify("LAYISO：請先選取物件，將只顯示該物件所在的圖層"),
            },
            LayerCommand::Unisolate => {
                if self.doc.unisolate_layers() {
                    self.show_notify("已取消圖層隔離，恢復先前的圖層狀態");
                } else {
                    self.show_notify("目前沒有被隔離的圖層");
                }
            }
            LayerCommand::Walk => {
                ui.walk_open = true;
                ui.walk_restore = true;
                ui.walk_selection = [self.doc.current_layer().to_string()].into();
            }
            LayerCommand::Delete => {
                let layer = self.selected_id.map(|id| self.doc.layer_of(id)).filter(|l| l != LAYER_ZERO);
                let ui = &mut self.layer_ui;
                ui.laydel = Some(layer.unwrap_or_else(|| {
                    self.doc.layers().iter().find(|l| l.name != LAYER_ZERO).map(|l| l.name.clone()).unwrap_or_default()
                }));
            }
            LayerCommand::CopyToLayer => {
                let source_layer = self.selected_id.map(|id| self.doc.layer_of(id)).unwrap_or_else(|| self.doc.current_layer().to_string());
                self.layer_ui.copy_to_layer = Some(CopyToLayer {
                    selection: self.selected_id.is_some(),
                    source_layer,
                    target: self.doc.current_layer().to_string(),
                });
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn layer_ui_copy_dialog_open(&self) -> bool {
        self.layer_ui.copy_to_layer.is_some()
    }

    /// All layer windows; call once per frame.
    pub(crate) fn render_layer_windows(&mut self, ctx: &egui::Context) {
        if self.layer_ui.manager_open {
            self.render_layer_manager(ctx);
        }
        if self.layer_ui.walk_open {
            self.render_layer_walk(ctx);
        }
        if self.layer_ui.laydel.is_some() {
            self.render_laydel(ctx);
        }
        if self.layer_ui.copy_to_layer.is_some() {
            self.render_copy_to_layer(ctx);
        }
    }

    /// LA: 圖層性質管理員.
    fn render_layer_manager(&mut self, ctx: &egui::Context) {
        let counts = self.layer_counts();
        let original: Vec<Layer> = self.doc.layers().to_vec();
        let mut layers = original.clone();
        let current = self.doc.current_layer().to_string();
        let mut open = true;
        let mut set_current: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut rename_commit: Option<(String, String)> = None;
        let mut add = false;
        let state = &mut self.layer_ui;

        egui::Window::new("📁 圖層性質管理員 (LA)")
            .open(&mut open)
            .default_size([760.0, 360.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut state.new_name).hint_text("新圖層名稱").desired_width(160.0));
                    if ui.button("＋ 新增圖層").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        add = true;
                    }
                    ui.separator();
                    ui.label(egui::RichText::new(format!("目前圖層: {current}")).strong());
                });
                if let Some(err) = &state.error {
                    ui.colored_label(egui::Color32::from_rgb(255, 130, 110), err);
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("layer_grid").striped(true).num_columns(9).spacing([10.0, 4.0]).show(ui, |ui| {
                        for title in ["目前", "名稱", "開/關", "顏色", "線型", "線粗", "透明度", "物件", ""] {
                            ui.label(egui::RichText::new(title).small().strong());
                        }
                        ui.end_row();
                        for layer in &mut layers {
                            let is_current = layer.name == current;
                            if ui.radio(is_current, "").on_hover_text("設為目前圖層 (新繪製的線條與註解放在此圖層)").clicked() {
                                set_current = Some(layer.name.clone());
                            }
                            // Name (double-click to rename)
                            match &mut state.renaming {
                                Some((old, buf)) if *old == layer.name => {
                                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(140.0));
                                    r.request_focus();
                                    if r.lost_focus() {
                                        rename_commit = Some((old.clone(), buf.clone()));
                                    }
                                }
                                _ => {
                                    let r = ui.add(egui::Label::new(&layer.name).sense(egui::Sense::click())).on_hover_text("雙擊更名");
                                    if r.double_clicked() && layer.name != LAYER_ZERO {
                                        state.renaming = Some((layer.name.clone(), layer.name.clone()));
                                    }
                                }
                            }
                            let eye = if layer.visible { "👁" } else { "⊘" };
                            ui.checkbox(&mut layer.visible, eye);
                            ui.horizontal(|ui| {
                                let mut by_object = layer.color.is_none();
                                if ui.checkbox(&mut by_object, "").on_hover_text("勾選 = 不指定 (物件保持原本的顏色)").changed() {
                                    layer.color = if by_object { None } else { Some([255, 255, 255, 255]) };
                                }
                                if let Some(rgba) = &mut layer.color {
                                    let mut c = rgba_color(*rgba);
                                    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                                        *rgba = c.to_srgba_unmultiplied();
                                    }
                                } else {
                                    ui.label(egui::RichText::new("依物件").small().weak());
                                }
                            });
                            egui::ComboBox::from_id_salt(("layer_lt", &layer.name))
                                .width(110.0)
                                .selected_text(layer.linetype.map_or("依物件", |l| l.label()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut layer.linetype, None, "依物件");
                                    for lt in LineType::ALL {
                                        ui.selectable_value(&mut layer.linetype, Some(lt), lt.label());
                                    }
                                });
                            egui::ComboBox::from_id_salt(("layer_lw", &layer.name))
                                .width(70.0)
                                .selected_text(layer.lineweight.map_or("預設".to_string(), |w| format!("{w:.2} mm")))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut layer.lineweight, None, "預設");
                                    for w in Layer::LINEWEIGHTS {
                                        ui.selectable_value(&mut layer.lineweight, Some(w), format!("{w:.2} mm"));
                                    }
                                });
                            ui.add(egui::DragValue::new(&mut layer.transparency).range(0..=90).suffix(" %"));
                            ui.label(counts.get(&layer.name).copied().unwrap_or(0).to_string());
                            let deletable = layer.name != LAYER_ZERO && !is_current;
                            if ui.add_enabled(deletable, egui::Button::new("🗑").small()).on_hover_text("刪除空白圖層 (有物件請用 LAYDEL)").clicked() {
                                delete = Some(layer.name.clone());
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.label(egui::RichText::new("顏色 / 線型 / 線粗未指定時，物件使用本身的建築線型與顏色；繪圖線條與註解可在性質面板選「依圖層」。").small().weak());
            });

        // Apply edits
        for (before, after) in original.iter().zip(&layers) {
            if before != after {
                if before.visible != after.visible {
                    self.save_undo_state();
                }
                if let Some(layer) = self.doc.layer_mut(&before.name) {
                    *layer = Layer { name: before.name.clone(), ..after.clone() };
                }
            }
        }
        let acted = add || set_current.is_some() || rename_commit.is_some() || delete.is_some();
        let result = if add {
            let name = self.layer_ui.new_name.trim().to_string();
            self.save_undo_state();
            self.doc.add_layer(&name).map(|_| {
                self.layer_ui.new_name.clear();
            })
        } else if let Some(name) = set_current {
            self.doc.set_current_layer(&name);
            Ok(())
        } else if let Some((old, new)) = rename_commit {
            self.layer_ui.renaming = None;
            self.save_undo_state();
            self.doc.rename_layer(&old, &new)
        } else if let Some(name) = delete {
            self.save_undo_state();
            self.doc.delete_layer(&name)
        } else {
            Ok(())
        };
        match result {
            // An error stays visible until the next action
            Ok(()) if acted => self.layer_ui.error = None,
            Ok(()) => {}
            Err(e) => {
                // Every failing action above recorded an undo step first
                self.undo_stack.pop();
                self.layer_ui.error = Some(e);
            }
        }
        if !open {
            self.layer_ui.manager_open = false;
            self.layer_ui.renaming = None;
            self.layer_ui.error = None;
        }
    }

    /// LAYWALK: click layer names to preview only those layers.
    fn render_layer_walk(&mut self, ctx: &egui::Context) {
        let counts = self.layer_counts();
        let mut open = true;
        let mut close = false;
        let names: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).collect();
        let state = &mut self.layer_ui;
        egui::Window::new("🚶 瀏覽圖層 (LAYWALK)")
            .open(&mut open)
            .default_size([260.0, 360.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("點擊圖層名稱即時預覽；Ctrl / ⌘ 點擊可多選").small().weak());
                ui.horizontal(|ui| {
                    if ui.small_button("全選").clicked() {
                        state.walk_selection = names.iter().cloned().collect();
                    }
                    if ui.small_button("清除").clicked() {
                        state.walk_selection.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for name in &names {
                        let selected = state.walk_selection.contains(name);
                        let text = format!("{name}  ({})", counts.get(name).copied().unwrap_or(0));
                        if ui.selectable_label(selected, text).clicked() {
                            let multi = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                            if !multi {
                                state.walk_selection.clear();
                                state.walk_selection.insert(name.clone());
                            } else if !state.walk_selection.remove(name) {
                                state.walk_selection.insert(name.clone());
                            }
                        }
                    }
                });
                ui.separator();
                ui.checkbox(&mut state.walk_restore, "結束時還原圖層狀態");
                if ui.button("結束瀏覽").clicked() {
                    close = true;
                }
            });
        let selection = self.layer_ui.walk_selection.clone();
        if !open || close {
            self.layer_ui.walk_open = false;
            self.doc.set_layer_preview(None);
            if !self.layer_ui.walk_restore {
                // Keep what was being previewed: turn the other layers off
                self.save_undo_state();
                for name in names {
                    if let Some(layer) = self.doc.layer_mut(&name) {
                        layer.visible = selection.contains(&name);
                    }
                }
            }
        } else {
            self.doc.set_layer_preview(Some(selection));
        }
    }

    /// LAYDEL: delete a layer together with its objects.
    fn render_laydel(&mut self, ctx: &egui::Context) {
        let counts = self.layer_counts();
        let names: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).filter(|n| n != LAYER_ZERO).collect();
        let Some(mut layer) = self.layer_ui.laydel.clone() else { return };
        let mut action: Option<bool> = None;
        egui::Window::new("🗑 刪除圖層 (LAYDEL)").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("要刪除的圖層:");
                egui::ComboBox::from_id_salt("laydel_layer").selected_text(&layer).show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(&mut layer, name.clone(), format!("{name} ({})", counts.get(name).copied().unwrap_or(0)));
                    }
                });
            });
            let n = counts.get(&layer).copied().unwrap_or(0);
            ui.colored_label(
                egui::Color32::from_rgb(255, 140, 110),
                format!("⚠ 將刪除圖層「{layer}」並一併刪除其中 {n} 個物件。"),
            );
            ui.label(egui::RichText::new("(SketchCAD 仍保留一步 Ctrl+Z 以防誤刪)").small().weak());
            ui.horizontal(|ui| {
                if ui.add_enabled(!layer.is_empty(), egui::Button::new(egui::RichText::new("刪除圖層與物件").color(egui::Color32::from_rgb(255, 120, 120)))).clicked() {
                    action = Some(true);
                }
                if ui.button("取消").clicked() {
                    action = Some(false);
                }
            });
        });
        match action {
            Some(true) => {
                self.layer_ui.laydel = None;
                self.save_undo_state();
                match self.doc.delete_layer_with_objects(&layer) {
                    Ok(n) => {
                        if self.selected_id.is_some_and(|id| self.doc.get_element(id).is_none()) {
                            self.selected_id = None;
                        }
                        self.show_notify(format!("已刪除圖層「{layer}」及 {n} 個物件"));
                    }
                    Err(e) => {
                        self.undo_stack.pop();
                        self.show_notify(e);
                    }
                }
            }
            Some(false) => self.layer_ui.laydel = None,
            None => self.layer_ui.laydel = Some(layer),
        }
    }

    /// COPYTOLAYER: copy objects onto another layer.
    fn render_copy_to_layer(&mut self, ctx: &egui::Context) {
        let counts = self.layer_counts();
        let names: Vec<String> = self.doc.layers().iter().map(|l| l.name.clone()).collect();
        let has_selection = self.selected_id.is_some();
        let Some(state) = &mut self.layer_ui.copy_to_layer else { return };
        let mut action: Option<bool> = None;
        egui::Window::new("⧉ 複製到圖層 (COPYTOLAYER)").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("要複製的物件:");
            ui.add_enabled_ui(has_selection, |ui| ui.radio_value(&mut state.selection, true, "目前選取的物件"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.selection, false, "整個圖層的物件:");
                egui::ComboBox::from_id_salt("ctl_source").selected_text(&state.source_layer).show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(&mut state.source_layer, name.clone(), format!("{name} ({})", counts.get(name).copied().unwrap_or(0)));
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("目標圖層:");
                egui::ComboBox::from_id_salt("ctl_target").selected_text(&state.target).show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(&mut state.target, name.clone(), name);
                    }
                });
            });
            ui.label(egui::RichText::new("複本放在原位置，可再用搬移 (M) 移開").small().weak());
            ui.horizontal(|ui| {
                if ui.button("⧉ 複製").clicked() {
                    action = Some(true);
                }
                if ui.button("取消").clicked() {
                    action = Some(false);
                }
            });
        });
        let Some(go) = action else { return };
        let Some(state) = self.layer_ui.copy_to_layer.take() else { return };
        if !go {
            return;
        }
        let ids: Vec<ElementId> = if state.selection {
            self.selected_id.into_iter().collect()
        } else {
            self.doc.objects_on_layer(&state.source_layer)
        };
        if ids.is_empty() {
            self.show_notify("沒有可複製的物件");
            return;
        }
        self.save_undo_state();
        let copies = self.doc.copy_to_layer(&ids, &state.target);
        self.selected_id = copies.first().copied();
        self.show_notify(format!("已複製 {} 個物件到圖層「{}」", copies.len(), state.target));
    }
}
