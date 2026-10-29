//! 3D model library browser (3D 模型百寶箱): pick a ready-made model by category and place
//! it in the 2D plan with the Model tool.

use std::collections::HashMap;

use eframe::egui;
use sketchcad_bim::{ModelCategory, ModelSpec, CATALOG};
use sketchcad_core::Vector3D;

use crate::gui::app::SketchCadApp;
use crate::gui::viewport_2d::ToolMode;

/// Shaded thumbnail triangles in a unit square (y down), painted far to near.
type Thumbnail = Vec<([egui::Pos2; 3], egui::Color32)>;

#[derive(Default)]
pub struct ModelBrowserState {
    pub open: bool,
    category: Option<ModelCategory>,
    search: String,
    thumbnails: HashMap<&'static str, Thumbnail>,
}

/// Renders a model into a flat-shaded three-quarter view from the front left.
fn render_thumbnail(spec: &ModelSpec) -> Thumbnail {
    let geometry = spec.geometry();
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
        let [r, g, bl, _] = mesh.face_color(t).unwrap_or(spec.body_color);
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
            let unit = pts.map(|(x, y)| {
                egui::pos2(((x - min.0 + offset.0) / extent) as f32, ((y - min.1 + offset.1) / extent) as f32)
            });
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

impl SketchCadApp {
    /// Floating 3D model library. Shown while the Model tool is active or when opened from
    /// the toolbar / View menu.
    pub(crate) fn render_model_browser(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut picked: Option<&'static str> = None;
        let selected = self.viewport_2d.model_id;

        egui::Window::new("📦 3D 模型百寶箱 (Model Library)")
            .open(&mut open)
            .default_pos([380.0, 140.0])
            .default_size([520.0, 520.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                let state = &mut self.model_browser;
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(state.category.is_none(), format!("全部 ({})", CATALOG.len())).clicked() {
                        state.category = None;
                    }
                    for category in ModelCategory::ALL {
                        let count = ModelSpec::in_category(category).count();
                        if ui.selectable_label(state.category == Some(category), format!("{} ({count})", category.label())).clicked() {
                            state.category = Some(category);
                        }
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("🔍 搜尋模型 (例: 冰箱、tree、車)").desired_width(f32::INFINITY));
                ui.separator();

                let query = state.search.trim().to_lowercase();
                let models: Vec<&'static ModelSpec> = CATALOG
                    .iter()
                    .filter(|m| state.category.is_none_or(|c| c == m.category))
                    .filter(|m| query.is_empty() || m.name.to_lowercase().contains(&query) || m.id.contains(&query))
                    .collect();

                egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    if models.is_empty() {
                        ui.label(egui::RichText::new("找不到符合的模型").weak());
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                        for spec in models {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(112.0, 118.0), egui::Sense::click());
                            if !ui.is_rect_visible(rect) {
                                continue;
                            }
                            let is_selected = selected == Some(spec.id);
                            let painter = ui.painter_at(rect);
                            let bg = if is_selected {
                                egui::Color32::from_rgb(40, 70, 105)
                            } else if response.hovered() {
                                egui::Color32::from_rgb(48, 54, 64)
                            } else {
                                egui::Color32::from_rgb(34, 38, 45)
                            };
                            painter.rect_filled(rect, 6.0, bg);
                            let thumb = state.thumbnails.entry(spec.id).or_insert_with(|| render_thumbnail(spec));
                            paint_thumbnail(&painter, egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 6.0), egui::vec2(88.0, 82.0)), thumb);
                            painter.text(
                                egui::pos2(rect.center().x, rect.max.y - 18.0),
                                egui::Align2::CENTER_CENTER,
                                short_name(spec.name),
                                egui::FontId::proportional(12.0),
                                egui::Color32::from_rgb(225, 230, 238),
                            );
                            let (w, d, h) = spec.geometry().size;
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
                            let price = if spec.price > 0.0 { format!("參考單價 NT$ {:.0}", spec.price) } else { "配景 (不計價)".to_string() };
                            let response = response.on_hover_text(format!(
                                "{}\n{} · {:.0}×{:.0}×{:.0} mm\n{}{}",
                                spec.name,
                                spec.category.label(),
                                w,
                                d,
                                h,
                                price,
                                if spec.elevation > 0.0 { format!("\n預設離地 {:.0} mm", spec.elevation) } else { String::new() }
                            ));
                            if response.clicked() {
                                picked = Some(spec.id);
                            }
                        }
                    });
                });

                ui.separator();
                match selected.and_then(ModelSpec::find) {
                    Some(spec) => ui.label(
                        egui::RichText::new(format!("已選: {} — 在 2D 平面點擊放置，按 R 旋轉 90°；放置後可於屬性面板調整縮放與高程", short_name(spec.name)))
                            .small()
                            .color(egui::Color32::from_rgb(150, 235, 180)),
                    ),
                    None => ui.label(egui::RichText::new("點選模型卡片後，在 2D 平面點擊即可放置").small().weak()),
                };
            });

        if let Some(id) = picked {
            self.viewport_2d.model_id = Some(id);
            self.tool_mode = ToolMode::Model;
        }
        if !open {
            self.model_browser.open = false;
            if self.tool_mode == ToolMode::Model {
                self.tool_mode = ToolMode::Select;
            }
        }
    }
}
