use egui::Ui;
use sketchcad_bim::{BimDocument, ElementKind, FurnitureKind, LightType, Material, SpaceType};
use sketchcad_core::ElementId;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertySortMode {
    Categorized,  // 分類檢視 (Visual Studio 預設風格)
    Alphabetical, // 字母/名稱排序
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    All,              // 綜合視圖 (構件總管樹 + VS屬性格 + 數量明細)
    HierarchyTree,    // 構件階層視圖 (Solution Explorer & Layers)
    PropertyGrid,     // Visual Studio 屬性設定視窗
    QuantitySchedule, // 即時數量與工程造價表
}

pub struct InspectorState {
    pub active_tab: InspectorTab,
    pub sort_mode: PropertySortMode,
    pub search_filter: String,
    pub hovered_prop: (String, String), // (Title, Description) for VS Help Box
    pub expanded_elements: HashSet<ElementId>,
    pub expanded_categories: HashSet<String>,
}

impl Default for InspectorState {
    fn default() -> Self {
        let mut expanded_categories = HashSet::new();
        expanded_categories.insert("structure".to_string());
        expanded_categories.insert("openings".to_string());
        expanded_categories.insert("interior".to_string());
        expanded_categories.insert("furnishing".to_string());

        Self {
            active_tab: InspectorTab::All,
            sort_mode: PropertySortMode::Categorized,
            search_filter: String::new(),
            hovered_prop: (
                "BIM 屬性說明視窗 (Visual Studio Property Inspector)".to_string(),
                "請在上方選取構件並將滑鼠懸停於各屬性欄位，此處將即時顯示該 BIM 參數的工程規範定義與雙向連動機制。".to_string(),
            ),
            expanded_elements: HashSet::new(),
            expanded_categories,
        }
    }
}

pub fn kind_display_name(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Wall => "牆體",
        ElementKind::Column => "結構柱",
        ElementKind::Beam => "結構樑",
        ElementKind::Slab => "樓板",
        ElementKind::Ceiling => "天花板",
        ElementKind::Stairs => "樓梯",
        ElementKind::Door => "門扇",
        ElementKind::Window => "窗戶",
        ElementKind::Railing => "欄杆扶手",
        ElementKind::Roof => "屋頂",
        ElementKind::Ramp => "坡道",
        ElementKind::CurtainWall => "幕牆系統",
        ElementKind::CurtainGrid => "幕牆網格",
        ElementKind::Mullion => "豎梃",
        ElementKind::Space => "空間區域",
        ElementKind::Furniture => "室內家具",
        ElementKind::LightFixture => "照明燈具",
    }
}

fn push_undo(undo_stack: &mut Vec<BimDocument>, doc: &BimDocument) {
    if undo_stack.len() >= 50 {
        undo_stack.remove(0);
    }
    undo_stack.push(doc.clone());
}

pub struct InspectorPanel;

impl InspectorPanel {
    pub fn render(
        ui: &mut Ui,
        doc: &mut BimDocument,
        undo_stack: &mut Vec<BimDocument>,
        clipboard: &mut Option<Box<dyn sketchcad_bim::BimElement>>,
        selected_id: &mut Option<ElementId>,
        state: &mut InspectorState,
    ) {
        // Tab Switcher Bar at the top of the panel (wraps when panel is shrunk)
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(state.active_tab == InspectorTab::All, "📑 綜合視圖").clicked() {
                state.active_tab = InspectorTab::All;
            }
            if ui.selectable_label(state.active_tab == InspectorTab::HierarchyTree, "🏢 構件圖層總管").clicked() {
                state.active_tab = InspectorTab::HierarchyTree;
            }
            if ui.selectable_label(state.active_tab == InspectorTab::PropertyGrid, "⚙ VS 屬性格").clicked() {
                state.active_tab = InspectorTab::PropertyGrid;
            }
            if ui.selectable_label(state.active_tab == InspectorTab::QuantitySchedule, "📊 數量造價").clicked() {
                state.active_tab = InspectorTab::QuantitySchedule;
            }
        });

        ui.separator();

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
            match state.active_tab {
                InspectorTab::All => {
                    // 1. Solution Explorer Hierarchy Tree with Photoshop-style visibility
                    render_hierarchy_tree(ui, doc, undo_stack, clipboard, selected_id, state);

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // 2. Visual Studio-style Property Grid
                    render_vs_property_grid(ui, doc, undo_stack, clipboard, selected_id, state);

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // 3. Live QTO Schedule
                    render_live_qto_schedule(ui, doc, selected_id);
                }
                InspectorTab::HierarchyTree => {
                    render_hierarchy_tree(ui, doc, undo_stack, clipboard, selected_id, state);
                }
                InspectorTab::PropertyGrid => {
                    render_vs_property_grid(ui, doc, undo_stack, clipboard, selected_id, state);
                }
                InspectorTab::QuantitySchedule => {
                    render_live_qto_schedule(ui, doc, selected_id);
                }
            }
        });
    }
}

/// 構件階層樹狀結構總管 (Solution Explorer) + Photoshop 風格圖層顯示/隱藏眼睛
fn render_hierarchy_tree(
    ui: &mut Ui,
    doc: &mut BimDocument,
    undo_stack: &mut Vec<BimDocument>,
    clipboard: &mut Option<Box<dyn sketchcad_bim::BimElement>>,
    selected_id: &mut Option<ElementId>,
    state: &mut InspectorState,
) {
    egui::CollapsingHeader::new(
        egui::RichText::new("🏢 專案構件階層總管 (Solution Explorer & Layers)")
            .strong()
            .size(14.0)
            .color(egui::Color32::from_rgb(220, 230, 245)),
    )
    .default_open(true)
    .show(ui, |ui| {
        // Top Toolbar: Quick Action Buttons
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("共 {} 個構件", doc.element_count())).small().color(egui::Color32::from_rgb(160, 175, 195)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⊘ 全部隱藏").on_hover_text("暫時隱藏畫面上所有 BIM 構件").clicked() {
                    doc.hide_all_elements();
                }
                if ui.button("👁 全部顯示").on_hover_text("顯示畫面上所有 BIM 構件").clicked() {
                    doc.show_all_elements();
                }
            });
        });

        ui.add_space(4.0);

        // Group 1: 🏗️ 結構與屋頂工程 (Structural Framework & Roof)
        render_category_group(
            ui,
            doc,
            undo_stack,
            clipboard,
            selected_id,
            state,
            "structure",
            "🏗️ 結構與屋頂工程 (Structure & Roof)",
            &[ElementKind::Wall, ElementKind::Column, ElementKind::Beam, ElementKind::Slab, ElementKind::Roof],
        );

        // Group 2: 🚪 門窗與玻璃幕牆工程 (Openings & Curtain Facade)
        render_category_group(
            ui,
            doc,
            undo_stack,
            clipboard,
            selected_id,
            state,
            "openings",
            "🚪 門窗與玻璃幕牆 (Openings & Facade)",
            &[ElementKind::Door, ElementKind::Window, ElementKind::CurtainWall, ElementKind::CurtainGrid, ElementKind::Mullion],
        );

        // Group 3: 🏠 室內裝修與無障礙動線 (Interior & Circulation)
        render_category_group(
            ui,
            doc,
            undo_stack,
            clipboard,
            selected_id,
            state,
            "interior",
            "🏠 室內裝修與動線 (Interior & Circulation)",
            &[ElementKind::Ceiling, ElementKind::Stairs, ElementKind::Railing, ElementKind::Ramp],
        );

        // Group 4: 🛋️ 室內空間與軟裝照明規劃 (Interior Space, Furniture & Lighting)
        render_category_group(
            ui,
            doc,
            undo_stack,
            clipboard,
            selected_id,
            state,
            "furnishing",
            "🛋️ 室內空間與軟裝照明 (Space, FF&E & Lighting)",
            &[ElementKind::Space, ElementKind::Furniture, ElementKind::LightFixture],
        );

    });
}

/// Render a single category group in the Solution Explorer
fn render_category_group(
    ui: &mut Ui,
    doc: &mut BimDocument,
    undo_stack: &mut Vec<BimDocument>,
    clipboard: &mut Option<Box<dyn sketchcad_bim::BimElement>>,
    selected_id: &mut Option<ElementId>,
    state: &mut InspectorState,
    cat_key: &str,
    cat_title: &str,
    kinds: &[ElementKind],
) {
    let mut elements_in_group: Vec<(ElementId, ElementKind, String)> = Vec::new();
    for &k in kinds {
        for elem in doc.get_elements_by_kind(k) {
            elements_in_group.push((elem.id(), k, elem.name().to_string()));
        }
    }

    if elements_in_group.is_empty() {
        return;
    }

    let is_cat_expanded = state.expanded_categories.contains(cat_key);
    let cat_arrow = if is_cat_expanded { "▼" } else { "▶" };

    ui.horizontal_wrapped(|ui| {
        let cat_btn = ui.selectable_label(
            false,
            egui::RichText::new(format!("{} {} ({})", cat_arrow, cat_title, elements_in_group.len()))
                .strong()
                .size(12.5)
                .color(egui::Color32::from_rgb(175, 205, 240)),
        );
        if cat_btn.clicked() {
            if is_cat_expanded {
                state.expanded_categories.remove(cat_key);
            } else {
                state.expanded_categories.insert(cat_key.to_string());
            }
        }
    });

    if is_cat_expanded {
        ui.indent(cat_key, |ui| {
            for (id, kind, name) in elements_in_group {
                render_element_tree_node(ui, doc, undo_stack, clipboard, selected_id, state, id, kind, &name);
            }
        });
    }
}

/// Render an individual element node in the Hierarchy Tree with Photoshop-style visibility eye
fn render_element_tree_node(
    ui: &mut Ui,
    doc: &mut BimDocument,
    undo_stack: &mut Vec<BimDocument>,
    _clipboard: &mut Option<Box<dyn sketchcad_bim::BimElement>>,
    selected_id: &mut Option<ElementId>,
    state: &mut InspectorState,
    id: ElementId,
    kind: ElementKind,
    name: &str,
) {
    let is_visible = doc.is_visible(id);
    let is_selected = *selected_id == Some(id);
    let is_expanded = state.expanded_elements.contains(&id);

    ui.horizontal_wrapped(|ui| {
        // 1. Photoshop-style Layer Visibility Eye Toggle
        let eye_icon = if is_visible { "👁" } else { "⊘" };
        let eye_color = if is_visible {
            egui::Color32::from_rgb(80, 220, 255)
        } else {
            egui::Color32::from_rgb(180, 80, 80)
        };
        let eye_tip = if is_visible {
            "構件圖層可見：點擊隱藏此構件 (同步從 2D 與 3D 畫面中隱藏)"
        } else {
            "構件圖層隱藏：點擊重新顯示此構件"
        };

        if ui.button(egui::RichText::new(eye_icon).color(eye_color).size(13.0))
            .on_hover_text(eye_tip)
            .clicked()
        {
            doc.toggle_visibility(id);
        }

        // 2. Tree expand/collapse toggle
        let node_arrow = if is_expanded { "▼" } else { "▶" };
        if ui.button(egui::RichText::new(node_arrow).size(11.0).color(egui::Color32::from_rgb(180, 190, 205)))
            .on_hover_text("展開/收合構件快速參數")
            .clicked()
        {
            if is_expanded {
                state.expanded_elements.remove(&id);
            } else {
                state.expanded_elements.insert(id);
                *selected_id = Some(id);
            }
        }

        // 2b. Quick Delete Button
        if ui.button(egui::RichText::new("🗑").size(11.0).color(egui::Color32::from_rgb(230, 90, 90)))
            .on_hover_text("刪除此 BIM 構件")
            .clicked()
        {
            push_undo(undo_stack, doc);
            doc.remove_element(id);
            if *selected_id == Some(id) {
                *selected_id = None;
            }
            return;
        }

        // 3. Element Name and Selection Button
        let kind_tag = format!("[{}]", kind_display_name(kind));
        let label_text = format!("{} {}", kind_tag, name);
        let rich = egui::RichText::new(label_text).size(12.0);
        let rich = if is_selected {
            rich.color(egui::Color32::from_rgb(255, 225, 80)).strong()
        } else if !is_visible {
            rich.color(egui::Color32::from_rgb(130, 140, 150))
        } else {
            rich.color(egui::Color32::from_rgb(225, 230, 240))
        };

        let item_btn = ui.selectable_label(is_selected, rich);
        if item_btn.clicked() {
            *selected_id = Some(id);
        }
        item_btn.on_hover_text("點選以在下方 Visual Studio 屬性設定格中檢視與編輯完整參數");
    });

    // In-place expanded quick-edit parameters right inside the tree node
    if is_expanded {
        ui.indent(format!("indent_elem_{:?}", id), |ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("⚡ 樹狀快速調整:").small().color(egui::Color32::from_rgb(255, 215, 0)));
                let mut needs_sync = false;

                if let Some(wall) = doc.get_wall_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("厚度:");
                        if ui.add(egui::DragValue::new(&mut wall.thickness).speed(5.0).range(100.0..=800.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("高度:");
                        if ui.add(egui::DragValue::new(&mut wall.height).speed(20.0).range(500.0..=10000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(col) = doc.get_column_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("寬度:");
                        if ui.add(egui::DragValue::new(&mut col.width).speed(10.0).range(200.0..=2000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("高度:");
                        if ui.add(egui::DragValue::new(&mut col.height).speed(20.0).range(500.0..=10000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(slab) = doc.get_slab_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("厚度:");
                        if ui.add(egui::DragValue::new(&mut slab.thickness).speed(5.0).range(50.0..=1000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("高程:");
                        if ui.add(egui::DragValue::new(&mut slab.base_z).speed(20.0).range(-5000.0..=20000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(ceil) = doc.get_ceiling_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("淨高:");
                        if ui.add(egui::DragValue::new(&mut ceil.elevation).speed(20.0).range(1800.0..=6000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(stair) = doc.get_stairs_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("梯寬:");
                        if ui.add(egui::DragValue::new(&mut stair.width).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("階數:");
                        let mut steps = stair.num_steps as i32;
                        if ui.add(egui::DragValue::new(&mut steps).speed(1.0).range(2..=50)).changed() {
                            stair.num_steps = steps.max(1) as usize;
                            needs_sync = true;
                        }
                    });
                } else if let Some(door) = doc.get_door_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("門寬:");
                        if ui.add(egui::DragValue::new(&mut door.width).speed(10.0).range(600.0..=2400.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("門高:");
                        if ui.add(egui::DragValue::new(&mut door.height).speed(10.0).range(1500.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("開向:");
                        for swing in sketchcad_bim::DoorSwing::ALL {
                            if ui.selectable_value(&mut door.swing, swing, swing.label()).changed() {
                                needs_sync = true;
                            }
                        }
                    });
                } else if let Some(win) = doc.get_window_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("窗寬:");
                        if ui.add(egui::DragValue::new(&mut win.width).speed(10.0).range(500.0..=3600.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("窗高:");
                        if ui.add(egui::DragValue::new(&mut win.height).speed(10.0).range(500.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(railing) = doc.get_railing_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("高度:");
                        if ui.add(egui::DragValue::new(&mut railing.height).speed(10.0).range(600.0..=1800.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        if ui.checkbox(&mut railing.has_glass_infill, "安全玻璃").changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(roof) = doc.get_roof_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("坡度:");
                        if ui.add(egui::DragValue::new(&mut roof.pitch_deg).speed(0.5).range(0.0..=60.0).suffix("°")).changed() {
                            needs_sync = true;
                        }
                        ui.label("厚度:");
                        if ui.add(egui::DragValue::new(&mut roof.thickness).speed(5.0).range(50.0..=600.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(ramp) = doc.get_ramp_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("寬度:");
                        if ui.add(egui::DragValue::new(&mut ramp.width).speed(10.0).range(800.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("升高:");
                        if ui.add(egui::DragValue::new(&mut ramp.total_rise).speed(10.0).range(50.0..=2000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(cw) = doc.get_curtain_wall_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("牆高:");
                        if ui.add(egui::DragValue::new(&mut cw.height).speed(20.0).range(1000.0..=15000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("跨距:");
                        if ui.add(egui::DragValue::new(&mut cw.u_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(cg) = doc.get_curtain_grid_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("水平網格:");
                        if ui.add(egui::DragValue::new(&mut cg.u_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(mul) = doc.get_mullion_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("鋁截面寬:");
                        if ui.add(egui::DragValue::new(&mut mul.profile_width).speed(2.0).range(30.0..=150.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(sp) = doc.get_space_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("淨高:");
                        if ui.add(egui::DragValue::new(&mut sp.clear_height).speed(20.0).range(2000.0..=8000.0).suffix(" mm")).changed() {
                            needs_sync = true;
                        }
                        ui.label(egui::RichText::new(format!("坪數: {:.2} 坪 ({:.1}m²)", sp.area_ping(), sp.area_m2())).color(egui::Color32::from_rgb(255, 215, 0)));
                    });
                } else if let Some(f) = doc.get_furniture_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("尺寸:");
                        if ui.add(egui::DragValue::new(&mut f.width).speed(20.0).range(300.0..=5000.0).prefix("W:").suffix("mm")).changed() {
                            needs_sync = true;
                        }
                        if ui.add(egui::DragValue::new(&mut f.depth).speed(20.0).range(300.0..=3000.0).prefix("D:").suffix("mm")).changed() {
                            needs_sync = true;
                        }
                    });
                } else if let Some(l) = doc.get_light_fixture_mut(id) {
                    ui.horizontal(|ui| {
                        ui.label("光通量:");
                        if ui.add(egui::DragValue::new(&mut l.lumens).speed(50.0).range(100.0..=10000.0).suffix(" lm")).changed() {
                            needs_sync = true;
                        }
                        ui.label("色溫:");
                        let mut cct = l.color_temp_k as i32;
                        if ui.add(egui::DragValue::new(&mut cct).speed(100.0).range(2000..=6500).suffix(" K")).changed() {
                            l.color_temp_k = cct as u32;
                            needs_sync = true;
                        }
                    });
                }

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("✥ 平移:").small());
                    if ui.small_button("X-").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(-500.0, 0.0, 0.0));
                    }
                    if ui.small_button("X+").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(500.0, 0.0, 0.0));
                    }
                    if ui.small_button("Y-").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(0.0, -500.0, 0.0));
                    }
                    if ui.small_button("Y+").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(0.0, 500.0, 0.0));
                    }
                    if ui.small_button("Z-").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(0.0, 0.0, -300.0));
                    }
                    if ui.small_button("Z+").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(id, sketchcad_core::Vector3D::new(0.0, 0.0, 300.0));
                    }
                });

                if needs_sync {
                    doc.sync();
                }
            });
        });
    }
}

/// Visual Studio 風格屬性設定視窗 (VS Property Grid)
fn render_vs_property_grid(
    ui: &mut Ui,
    doc: &mut BimDocument,
    undo_stack: &mut Vec<BimDocument>,
    clipboard: &mut Option<Box<dyn sketchcad_bim::BimElement>>,
    selected_id: &mut Option<ElementId>,
    state: &mut InspectorState,
) {
    egui::CollapsingHeader::new(
        egui::RichText::new("⚙ BIM 構件屬性設定 (Visual Studio Property Grid)")
            .strong()
            .size(14.0)
            .color(egui::Color32::from_rgb(220, 230, 245)),
    )
    .default_open(true)
    .show(ui, |ui| {
        // Collect all elements for the selector dropdown
        let all_elements = doc.all_elements();
        if selected_id.is_none() && !all_elements.is_empty() {
            *selected_id = Some(all_elements[0].id());
        }

        // Selection Dropdown Header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("目前目標:").strong());
            let current_label = if let Some(id) = *selected_id {
                if let Some(elem) = doc.get_element(id) {
                    format!("[{}] {}", kind_display_name(elem.kind()), elem.name())
                } else {
                    "無選定構件".to_string()
                }
            } else {
                "請在上方總管選取構件".to_string()
            };

            egui::ComboBox::from_id_salt("vs_prop_element_selector")
                .selected_text(current_label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for elem in &all_elements {
                        let text = format!("[{}] {}", kind_display_name(elem.kind()), elem.name());
                        if ui.selectable_label(*selected_id == Some(elem.id()), text).clicked() {
                            *selected_id = Some(elem.id());
                        }
                    }
                });
        });

        // Operations Bar for Selected Element (Delete, Move, Cut, Copy, Paste)
        if let Some(target_id) = *selected_id {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("構件操作:").strong().color(egui::Color32::from_rgb(255, 215, 0)));
                    if ui.button(egui::RichText::new("✂️ 剪下").color(egui::Color32::from_rgb(255, 180, 100)))
                        .on_hover_text("剪下目前構件至剪貼簿 [Ctrl+X]")
                        .clicked()
                    {
                        if let Some(elem) = doc.get_element(target_id) {
                            push_undo(undo_stack, doc);
                            *clipboard = Some(elem.clone_box());
                            doc.remove_element(target_id);
                            *selected_id = None;
                            return;
                        }
                    }
                    if ui.button(egui::RichText::new("📋 複製").color(egui::Color32::from_rgb(120, 200, 255)))
                        .on_hover_text("複製目前構件至剪貼簿 [Ctrl+C]")
                        .clicked()
                    {
                        if let Some(elem) = doc.get_element(target_id) {
                            *clipboard = Some(elem.clone_box());
                        }
                    }
                    if ui.add_enabled(clipboard.is_some(), egui::Button::new("📄 貼上"))
                        .on_hover_text("貼上剪貼簿中的構件 [Ctrl+V]")
                        .clicked()
                    {
                        if let Some(clip) = clipboard {
                            push_undo(undo_stack, doc);
                            let offset = sketchcad_core::Vector3D::new(500.0, 500.0, 0.0);
                            let new_id = doc.paste_element(clip.as_ref(), offset);
                            *selected_id = Some(new_id);
                        }
                    }
                    if ui.button(egui::RichText::new("🗑️ 刪除此構件 (Delete)").color(egui::Color32::from_rgb(255, 110, 110)))
                        .on_hover_text("刪除目前選定的構件 [Delete / Backspace]")
                        .clicked()
                    {
                        push_undo(undo_stack, doc);
                        doc.remove_element(target_id);
                        *selected_id = None;
                        return;
                    }
                });
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("✥ 平移 (步長 500mm):").small());
                    if ui.small_button("← X-").on_hover_text("沿 X 軸負向平移 500mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(-500.0, 0.0, 0.0));
                    }
                    if ui.small_button("X+ →").on_hover_text("沿 X 軸正向平移 500mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(500.0, 0.0, 0.0));
                    }
                    if ui.small_button("↓ Y-").on_hover_text("沿 Y 軸負向平移 500mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(0.0, -500.0, 0.0));
                    }
                    if ui.small_button("Y+ ↑").on_hover_text("沿 Y 軸正向平移 500mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(0.0, 500.0, 0.0));
                    }
                    if ui.small_button("▼ Z-").on_hover_text("沿 Z 軸負向平移 300mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(0.0, 0.0, -300.0));
                    }
                    if ui.small_button("Z+ ▲").on_hover_text("沿 Z 軸正向平移 300mm").clicked() {
                        push_undo(undo_stack, doc);
                        doc.move_element(target_id, sketchcad_core::Vector3D::new(0.0, 0.0, 300.0));
                    }
                });
            });
            ui.add_space(4.0);
        }

        // Visual Studio Toolbar: Categorized vs Alphabetical + Search Filter Box
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(state.sort_mode == PropertySortMode::Categorized, "📑 分類排序")
                .on_hover_text("依據幾何尺寸、材料、工程造價等邏輯群組分類")
                .clicked()
            {
                state.sort_mode = PropertySortMode::Categorized;
            }
            if ui.selectable_label(state.sort_mode == PropertySortMode::Alphabetical, "🔤 字母排序")
                .on_hover_text("依屬性名稱排序顯示")
                .clicked()
            {
                state.sort_mode = PropertySortMode::Alphabetical;
            }

            ui.separator();
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut state.search_filter)
                    .hint_text("篩選屬性名稱...")
                    .desired_width(100.0),
            );
            if !state.search_filter.is_empty() && ui.small_button("✕").clicked() {
                state.search_filter.clear();
            }
        });

        ui.add_space(4.0);

        // Render Property Table for the selected element
        if let Some(id) = *selected_id {
            render_selected_element_properties(ui, doc, id, state);
        } else {
            ui.label(egui::RichText::new("請先點選畫面上或樹狀總管中的構件以顯示屬性。").italics().color(egui::Color32::from_rgb(150, 160, 175)));
        }

        ui.add_space(8.0);

        // Visual Studio Iconic Property Help / Description Pane at Bottom
        render_vs_help_pane(ui, state);
    });
}

/// Renders the Visual Studio Property Rows for the specific selected element
fn render_selected_element_properties(
    ui: &mut Ui,
    doc: &mut BimDocument,
    id: ElementId,
    state: &mut InspectorState,
) {
    let mut needs_sync = false;

    // Outer Property Grid Box
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(26, 30, 36))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 52, 64)))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {

            // Section 1: 幾何尺寸 (Geometry)
            render_category_header(ui, "幾何尺寸 (Geometry)");

            if let Some(wall) = doc.get_wall_mut(id) {
                vs_property_row(ui, "厚度 (Thickness)", "定義牆體斷面結構核心厚度 (mm)。修改將即時連動 2D 雙線繪圖寬度與 3D 實體模型。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut wall.thickness).speed(5.0).range(100.0..=1200.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "高度 (Height)", "定義牆體垂直高度 (mm)。修改將即時連動三維模型高度與布林開口減除運算。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut wall.height).speed(20.0).range(500.0..=15000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "長度 (Length)", "牆體起訖點間之中心線距離 (唯讀唯感)。", state, |ui| {
                    ui.label(format!("{:.1} mm", wall.length()));
                });
                vs_property_row(ui, "開口數量 (Openings)", "嵌入於此牆體之門扇、氣密窗與開洞之總數量。", state, |ui| {
                    ui.label(format!("{} 個開口", wall.openings.len()));
                });
            } else if let Some(col) = doc.get_column_mut(id) {
                vs_property_row(ui, "寬度 (Width)", "結構柱 X 向斷面寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut col.width).speed(10.0).range(150.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "深度 (Depth)", "結構柱 Y 向斷面深度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut col.depth).speed(10.0).range(150.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "高度 (Height)", "結構柱總柱高 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut col.height).speed(20.0).range(500.0..=15000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(beam) = doc.get_beam_mut(id) {
                vs_property_row(ui, "寬度 (Width)", "結構大樑斷面寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut beam.width).speed(10.0).range(150.0..=2000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "梁深 (Depth)", "結構大樑斷面深度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut beam.depth).speed(10.0).range(200.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "長度 (Length)", "結構大樑三維軸線跨距長度 (mm)。", state, |ui| {
                    ui.label(format!("{:.1} mm", beam.length()));
                });
            } else if let Some(slab) = doc.get_slab_mut(id) {
                vs_property_row(ui, "寬度 (Width)", "樓板平面邊界 X 跨度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut slab.width).speed(50.0).range(500.0..=60000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "深度 (Depth)", "樓板平面邊界 Y 跨度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut slab.depth).speed(50.0).range(500.0..=60000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "厚度 (Thickness)", "結構樓板結構版厚 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut slab.thickness).speed(5.0).range(50.0..=1200.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(ceil) = doc.get_ceiling_mut(id) {
                vs_property_row(ui, "寬度 (Width)", "輕鋼架天花板邊界 X 跨度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ceil.width).speed(50.0).range(500.0..=60000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "深度 (Depth)", "輕鋼架天花板邊界 Y 跨度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ceil.depth).speed(50.0).range(500.0..=60000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(stair) = doc.get_stairs_mut(id) {
                vs_property_row(ui, "梯寬 (Width)", "樓梯踏面淨寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut stair.width).speed(20.0).range(600.0..=4000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "爬升總高 (Total Rise)", "樓梯自起步層至到達層之總垂直提升量 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut stair.total_rise).speed(20.0).range(500.0..=12000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "級數 (Steps Count)", "直通樓梯踏步總階數。將自動平分計算單階級高 (Riser) 與級深 (Tread)。", state, |ui| {
                    let mut steps = stair.num_steps as i32;
                    if ui.add(egui::DragValue::new(&mut steps).speed(1.0).range(2..=60)).changed() {
                        stair.num_steps = steps.max(1) as usize;
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "單階級深 (Tread)", "單一步驟之水平踏步深度 (mm)。", state, |ui| {
                    ui.label(format!("{:.1} mm", stair.tread_depth()));
                });
                vs_property_row(ui, "單階級高 (Riser)", "單一步驟之垂直踢面高度 (mm)。", state, |ui| {
                    ui.label(format!("{:.1} mm", stair.riser_height()));
                });
            } else if let Some(door) = doc.get_door_mut(id) {
                vs_property_row(ui, "門扇淨寬 (Width)", "出入推拉門框之淨通行寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut door.width).speed(10.0).range(600.0..=2400.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "門扇淨高 (Height)", "出入推拉門框之垂直淨通行高度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut door.height).speed(10.0).range(1500.0..=3200.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "開門方向 (Swing)", "左開/右開：鉸鍊位於門洞起點或終點；內開/外開：門扇朝牆的哪一側開啟。", state, |ui| {
                    egui::ComboBox::from_id_salt("door_swing")
                        .selected_text(door.swing.label())
                        .show_ui(ui, |ui| {
                            for swing in sketchcad_bim::DoorSwing::ALL {
                                if ui.selectable_value(&mut door.swing, swing, swing.label()).changed() {
                                    needs_sync = true;
                                }
                            }
                        });
                });
            } else if let Some(win) = doc.get_window_mut(id) {
                vs_property_row(ui, "窗框寬度 (Width)", "氣密防風窗之水平總開窗寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut win.width).speed(10.0).range(500.0..=4000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "窗框高度 (Height)", "氣密防風窗之垂直總開窗高度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut win.height).speed(10.0).range(500.0..=3200.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "窗台高度 (Sill Height)", "窗台底緣距離基準樓地板面之垂直距離 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut win.sill_height).speed(10.0).range(0.0..=2500.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(railing) = doc.get_railing_mut(id) {
                vs_property_row(ui, "扶手高度 (Height)", "地面或梯級面至扶手頂緣之垂直高度 (mm)，法定標準 1100mm。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut railing.height).speed(10.0).range(600.0..=2000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "扶手寬度 (Handrail Width)", "頂部主扶手握持寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut railing.handrail_width).speed(5.0).range(30.0..=150.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "立柱間距 (Post Spacing)", "相鄰結構立柱中心點之標準跨距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut railing.post_spacing).speed(20.0).range(300.0..=2500.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "立柱截面 (Post Size)", "立柱方管斷面規格邊長 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut railing.post_size).speed(2.0).range(20.0..=100.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "夾膠玻璃 (Glass Infill)", "是否在立柱間安裝 10mm 強化安全夾膠玻璃護欄隔板。", state, |ui| {
                    if ui.checkbox(&mut railing.has_glass_infill, "開啟安全玻璃隔板").changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "扶手總長 (Length)", "欄杆扶手空間起訖軸線之三維總長度 (唯讀)。", state, |ui| {
                    ui.label(format!("{:.1} mm ({:.2} m)", railing.length(), railing.length() / 1000.0));
                });
            } else if let Some(roof) = doc.get_roof_mut(id) {
                vs_property_row(ui, "屋頂跨度 (Width)", "屋頂平面覆蓋範圍 X 向跨距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.width).speed(20.0).range(1000.0..=50000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "屋頂進深 (Depth)", "屋頂平面覆蓋範圍 Y 向進深 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.depth).speed(20.0).range(1000.0..=50000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "屋面坡度 (Pitch)", "雙坡/四坡屋面傾斜角度 (degrees)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.pitch_deg).speed(0.5).range(0.0..=60.0).suffix("°")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "屋面板厚 (Thickness)", "屋面防水隔熱結構層厚度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.thickness).speed(5.0).range(50.0..=800.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "簷口出挑 (Overhang)", "外牆線向外延伸之防雨遮陽挑簷距離 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.overhang).speed(10.0).range(0.0..=2000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(ramp) = doc.get_ramp_mut(id) {
                vs_property_row(ui, "坡道淨寬 (Width)", "無障礙通行標準淨寬 (mm)，法定標準 ≥1200mm。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ramp.width).speed(10.0).range(800.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "提升總高 (Total Rise)", "坡道自起點至終點平台之垂直爬升高度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ramp.total_rise).speed(10.0).range(50.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "水平長度 (Length)", "坡道中心線之平面水平延伸距離 (唯讀)。", state, |ui| {
                    ui.label(format!("{:.1} mm", ramp.length()));
                });
                vs_property_row(ui, "無障礙坡度比 (Slope)", "爬升斜率。法定標準 1:12，防滑安全通行。", state, |ui| {
                    ui.label(format!("1:{:.1} ({:.1}%)", 1.0 / ramp.slope_ratio(), ramp.slope_ratio() * 100.0));
                });
            } else if let Some(cw) = doc.get_curtain_wall_mut(id) {
                vs_property_row(ui, "幕牆高度 (Height)", "結構單元式玻璃幕牆總立面淨高 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cw.height).speed(20.0).range(1000.0..=20000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "豎梃網格間距 (U Spacing)", "垂直鋁合金受力豎梃之標準跨距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cw.u_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "橫樑網格間距 (V Spacing)", "水平鋁合金分格橫樑之跨距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cw.v_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "玻璃厚度 (Glass Thickness)", "Low-E 雙層中空隔音隔熱玻璃厚度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cw.glass_thickness).speed(2.0).range(12.0..=60.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "立面總長 (Length)", "玻璃幕牆軸線長度 (唯讀)。", state, |ui| {
                    ui.label(format!("{:.1} mm", cw.length()));
                });
                vs_property_row(ui, "帷幕面積 (Area)", "立面總透光採光受風面積 (唯讀)。", state, |ui| {
                    ui.label(format!("{:.2} m²", cw.area_m2()));
                });
            } else if let Some(cg) = doc.get_curtain_grid_mut(id) {
                vs_property_row(ui, "網格水平間距 (U)", "幕牆分格系統水平分割間距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cg.u_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "網格垂直間距 (V)", "幕牆分格系統垂直分割間距 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cg.v_spacing).speed(20.0).range(600.0..=3000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(mul) = doc.get_mullion_mut(id) {
                vs_property_row(ui, "豎梃斷面寬度 (Width)", "擠型鋁合金結構豎梃斷面寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut mul.profile_width).speed(2.0).range(30.0..=150.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "豎梃斷面深度 (Depth)", "擠型鋁合金結構豎梃斷面風壓承受深度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut mul.profile_depth).speed(5.0).range(60.0..=300.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(sp) = doc.get_space_mut(id) {
                vs_property_row(ui, "空間類型 (Space Type)", "定義空間用途屬性 (客廳、主臥、臥室、廚房、衛浴、餐廳、書房、陽台、玄關、走廊)。", state, |ui| {
                    let current_type = sp.space_type;
                    egui::ComboBox::from_id_salt(format!("sp_type_{:?}", id))
                        .selected_text(current_type.display_name())
                        .show_ui(ui, |ui| {
                            for t in &[
                                SpaceType::LivingRoom,
                                SpaceType::MasterBedroom,
                                SpaceType::Bedroom,
                                SpaceType::Kitchen,
                                SpaceType::Bathroom,
                                SpaceType::DiningRoom,
                                SpaceType::Study,
                                SpaceType::Balcony,
                                SpaceType::Foyer,
                                SpaceType::Corridor,
                            ] {
                                if ui.selectable_label(current_type == *t, t.display_name()).clicked() {
                                    sp.space_type = *t;
                                    sp.name = t.display_name().to_string();
                                    needs_sync = true;
                                }
                            }
                        });
                });
                vs_property_row(ui, "台灣坪數 (Ping Area)", "實得使用坪數 (1 坪 = 3.30578 平方米)。室內設計報價與空間規劃核心依據。", state, |ui| {
                    ui.label(egui::RichText::new(format!("{:.2} 坪", sp.area_ping())).strong().color(egui::Color32::from_rgb(255, 215, 0)));
                });
                vs_property_row(ui, "地坪面積 (Floor Area)", "空間水平封閉多邊形覆蓋之實坪面積 (m²)。", state, |ui| {
                    ui.label(format!("{:.2} m²", sp.area_m2()));
                });
                vs_property_row(ui, "空間周長 (Perimeter)", "踢腳板工程與牆面塗刷計量長度 (m)。", state, |ui| {
                    ui.label(format!("{:.2} m", sp.perimeter_m()));
                });
                vs_property_row(ui, "天花淨高 (Clear Height)", "室內地坪完成面至天花板之垂直淨空高度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut sp.clear_height).speed(20.0).range(2000.0..=8000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "空間容積 (Volume)", "空調冷房能力與換氣量計算之體積 (m³)。", state, |ui| {
                    ui.label(format!("{:.2} m³", sp.volume_m3()));
                });
            } else if let Some(f) = doc.get_furniture_mut(id) {
                vs_property_row(ui, "家具類型 (Furniture Kind)", "家具與設備規格分類 (沙發、雙人床、餐桌椅、廚具水槽、馬桶衛浴、衣櫃、工作書桌)。", state, |ui| {
                    let current_k = f.kind;
                    egui::ComboBox::from_id_salt(format!("furn_kind_{:?}", id))
                        .selected_text(current_k.display_name())
                        .show_ui(ui, |ui| {
                            for k in &[
                                FurnitureKind::Sofa,
                                FurnitureKind::Bed,
                                FurnitureKind::DiningTable,
                                FurnitureKind::KitchenCabinet,
                                FurnitureKind::Toilet,
                                FurnitureKind::Wardrobe,
                                FurnitureKind::Desk,
                            ] {
                                if ui.selectable_label(current_k == *k, k.display_name()).clicked() {
                                    f.kind = *k;
                                    f.name = k.display_name().to_string();
                                    needs_sync = true;
                                }
                            }
                        });
                });
                vs_property_row(ui, "寬度 (Width)", "家具 X 向寬度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut f.width).speed(20.0).range(200.0..=6000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "進深 (Depth)", "家具 Y 向進深 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut f.depth).speed(20.0).range(200.0..=4000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "高度 (Height)", "家具總高度 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut f.height).speed(20.0).range(200.0..=3500.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "旋轉角度 (Rotation)", "家具平面順時鐘旋轉朝向 (°)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut f.rotation_deg).speed(5.0).range(0.0..=360.0).suffix("°")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "參考單價 (Cost)", "FF&E 軟裝採購與組裝標準造價 (TWD)。", state, |ui| {
                    ui.label(format!("NT$ {:.0}", f.kind.default_cost()));
                });
            } else if let Some(l) = doc.get_light_fixture_mut(id) {
                vs_property_row(ui, "燈具類型 (Luminaire Type)", "室內照明燈具型式 (嵌燈、吊燈、軌道燈、吸頂燈、燈帶)。", state, |ui| {
                    let current_t = l.light_type;
                    egui::ComboBox::from_id_salt(format!("light_type_{:?}", id))
                        .selected_text(current_t.display_name())
                        .show_ui(ui, |ui| {
                            for t in &[
                                LightType::Downlight,
                                LightType::Pendant,
                                LightType::TrackLight,
                                LightType::CeilingLamp,
                                LightType::LightStrip,
                            ] {
                                if ui.selectable_label(current_t == *t, t.display_name()).clicked() {
                                    l.light_type = *t;
                                    l.name = t.display_name().to_string();
                                    needs_sync = true;
                                }
                            }
                        });
                });
                vs_property_row(ui, "光通量 (Lumens)", "光源發光強度 (lm)。用於計算室內平均照度 (Lux)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut l.lumens).speed(50.0).range(50.0..=15000.0).suffix(" lm")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "相關色溫 (CCT)", "照明色彩溫度 (Kelvin)。2700K 暖黃光放鬆氛圍，4000K 自然白光專注清晰。", state, |ui| {
                    let mut cct = l.color_temp_k as i32;
                    if ui.add(egui::DragValue::new(&mut cct).speed(100.0).range(2000..=7000).suffix(" K")).changed() {
                        l.color_temp_k = cct as u32;
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "額定功率 (Wattage)", "燈具耗電功率 (Watts)。用於照明功率密度 (LPD) 綠建築節能審查。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut l.wattage).speed(1.0).range(1.0..=500.0).suffix(" W")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "配光投射角 (Beam Angle)", "光束角廣度 (°)。聚光投射焦點 vs 均勻廣角洗牆照明。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut l.beam_angle_deg).speed(2.0).range(10.0..=120.0).suffix("°")).changed() {
                        needs_sync = true;
                    }
                });
                vs_property_row(ui, "設備單價 (Price)", "電氣照明設備與配線安裝牌價 (TWD)。", state, |ui| {
                    ui.label(format!("NT$ {:.0}", l.light_type.default_specs().3));
                });
            }

            // Section 2: 空間與定位 (Spatial Constraints)
            render_category_header(ui, "空間與約束定位 (Constraints)");

            if let Some(slab) = doc.get_slab_mut(id) {
                vs_property_row(ui, "高程 base_z (Elevation)", "樓板頂面之基準世界 Z 軸高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut slab.base_z).speed(20.0).range(-10000.0..=30000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(ceil) = doc.get_ceiling_mut(id) {
                vs_property_row(ui, "室內淨高 (Clear Height)", "輕鋼架天花板自樓地板面起算之吊頂淨高 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ceil.elevation).speed(20.0).range(1800.0..=8000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(roof) = doc.get_roof_mut(id) {
                vs_property_row(ui, "基準高程 (Base Z)", "屋頂基座樓面世界高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut roof.base_z).speed(20.0).range(-5000.0..=40000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(ramp) = doc.get_ramp_mut(id) {
                vs_property_row(ui, "起步高程 (Base Z)", "坡道起點之世界高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut ramp.base_z).speed(20.0).range(-5000.0..=20000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(cw) = doc.get_curtain_wall_mut(id) {
                vs_property_row(ui, "基座高程 (Base Z)", "幕牆底部世界高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut cw.base_z).speed(20.0).range(-5000.0..=20000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(sp) = doc.get_space_mut(id) {
                vs_property_row(ui, "地坪高程 (Base Z)", "空間地坪完成面相對基準層之世界高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut sp.elevation).speed(10.0).range(-2000.0..=20000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(f) = doc.get_furniture_mut(id) {
                vs_property_row(ui, "擺放高程 (Base Z)", "家具放置之世界高程 (mm)。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut f.base_z).speed(10.0).range(-2000.0..=20000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            } else if let Some(l) = doc.get_light_fixture_mut(id) {
                vs_property_row(ui, "安裝吊頂高程 (Mounting Z)", "燈具安裝之世界高度 (mm)。通常等於天花板淨高。", state, |ui| {
                    if ui.add(egui::DragValue::new(&mut l.mounting_height).speed(20.0).range(1800.0..=10000.0).suffix(" mm")).changed() {
                        needs_sync = true;
                    }
                });
            }

            vs_property_row(ui, "基準樓層 (Base Level)", "構件所綁定之建築標高層位 (Level 1)。", state, |ui| {
                ui.label("1F (Level 1, ±0.00)");
            });

            // Section 3: 材料與物理規範 (Materials & Engineering)
            render_category_header(ui, "材料與物理規範 (Materials)");

            let mut current_mat_name = "自訂材料".to_string();
            let mut current_density = 2400.0;
            let mut current_cost_m3 = 3200.0;

            if let Some(wall) = doc.get_wall(id) {
                current_mat_name = wall.material.name.clone();
                current_density = wall.material.density_kg_m3;
                current_cost_m3 = wall.material.cost_per_m3;
            } else if let Some(col) = doc.get_column(id) {
                current_mat_name = col.material.name.clone();
                current_density = col.material.density_kg_m3;
                current_cost_m3 = col.material.cost_per_m3;
            } else if let Some(slab) = doc.get_slab(id) {
                current_mat_name = slab.material.name.clone();
                current_density = slab.material.density_kg_m3;
                current_cost_m3 = slab.material.cost_per_m3;
            } else if let Some(ceil) = doc.get_ceiling(id) {
                current_mat_name = ceil.material.name.clone();
                current_density = ceil.material.density_kg_m3;
                current_cost_m3 = ceil.material.cost_per_m3;
            } else if let Some(railing) = doc.get_railing(id) {
                current_mat_name = railing.material.name.clone();
                current_density = railing.material.density_kg_m3;
                current_cost_m3 = railing.material.cost_per_m3;
            } else if let Some(roof) = doc.get_roof(id) {
                current_mat_name = roof.material.name.clone();
                current_density = roof.material.density_kg_m3;
                current_cost_m3 = roof.material.cost_per_m3;
            } else if let Some(ramp) = doc.get_ramp(id) {
                current_mat_name = ramp.material.name.clone();
                current_density = ramp.material.density_kg_m3;
                current_cost_m3 = ramp.material.cost_per_m3;
            } else if let Some(cw) = doc.get_curtain_wall(id) {
                current_mat_name = cw.material.name.clone();
                current_density = cw.material.density_kg_m3;
                current_cost_m3 = cw.material.cost_per_m3;
            } else if let Some(mul) = doc.get_mullion(id) {
                current_mat_name = mul.material.name.clone();
                current_density = mul.material.density_kg_m3;
                current_cost_m3 = mul.material.cost_per_m3;
            } else if let Some(sp) = doc.get_space(id) {
                current_mat_name = sp.floor_material.name.clone();
                current_density = sp.floor_material.density_kg_m3;
                current_cost_m3 = sp.floor_material.cost_per_m3;
            } else if let Some(f) = doc.get_furniture(id) {
                current_mat_name = f.material.name.clone();
                current_density = f.material.density_kg_m3;
                current_cost_m3 = f.material.cost_per_m3;
            }

            let materials_presets = [
                ("鋼筋混凝土 (Concrete)", Material::concrete()),
                ("紅磚砌體 (Red Brick)", Material::brick()),
                ("結構鋼材 (Steel)", Material::structural_steel()),
                ("雙層玻璃 (Glass)", Material::glass()),
                ("實木橡木 (Timber)", Material::wood()),
                ("輕鋼架石膏板 (Gypsum)", Material::gypsum()),
                ("卡拉拉白大理石 (Carrara Marble)", Material::marble()),
                ("海島型橡木地板 (Oak Flooring)", Material::wood_flooring()),
                ("拋光石英磚 (Porcelain Tile)", Material::polished_tile()),
                ("北美胡桃木實木皮 (Walnut Veneer)", Material::walnut()),
                ("天然亞麻織物軟裝 (Linen Fabric)", Material::fabric()),
                ("無縫微水泥地坪 (Microcement)", Material::microcement()),
            ];

            let mut selected_preset_mat: Option<Material> = None;
            vs_property_row(ui, "指定材料 (Material)", "為構件選定物理材料配比與工程計價基準。", state, |ui| {
                egui::ComboBox::from_id_salt(format!("mat_select_{:?}", id))
                    .selected_text(&current_mat_name)
                    .show_ui(ui, |ui| {
                        for (title, mat) in &materials_presets {
                            if ui.selectable_label(current_mat_name == mat.name, *title).clicked() {
                                selected_preset_mat = Some(mat.clone());
                            }
                        }
                    });
            });

            if let Some(new_mat) = selected_preset_mat {
                if let Some(wall) = doc.get_wall_mut(id) {
                    wall.material = new_mat;
                } else if let Some(col) = doc.get_column_mut(id) {
                    col.material = new_mat;
                } else if let Some(slab) = doc.get_slab_mut(id) {
                    slab.material = new_mat;
                } else if let Some(ceil) = doc.get_ceiling_mut(id) {
                    ceil.material = new_mat;
                } else if let Some(beam) = doc.get_beam_mut(id) {
                    beam.material = new_mat;
                } else if let Some(stair) = doc.get_stairs_mut(id) {
                    stair.material = new_mat;
                } else if let Some(door) = doc.get_door_mut(id) {
                    door.material = new_mat;
                } else if let Some(win) = doc.get_window_mut(id) {
                    win.material = new_mat;
                } else if let Some(railing) = doc.get_railing_mut(id) {
                    railing.material = new_mat;
                } else if let Some(roof) = doc.get_roof_mut(id) {
                    roof.material = new_mat;
                } else if let Some(ramp) = doc.get_ramp_mut(id) {
                    ramp.material = new_mat;
                } else if let Some(cw) = doc.get_curtain_wall_mut(id) {
                    cw.material = new_mat;
                } else if let Some(mul) = doc.get_mullion_mut(id) {
                    mul.material = new_mat;
                } else if let Some(sp) = doc.get_space_mut(id) {
                    sp.floor_material = new_mat;
                } else if let Some(f) = doc.get_furniture_mut(id) {
                    f.material = new_mat;
                }
                needs_sync = true;
            }


            vs_property_row(ui, "材料密度 (Density)", "用於結構載重計算與地震力分析之單位體積質量 (kg/m³)。", state, |ui| {
                ui.label(format!("{:.0} kg/m³", current_density));
            });
            vs_property_row(ui, "材料單價 (Cost/m³)", "估價與工程預算連動之每立方米採購單價 (TWD)。", state, |ui| {
                ui.label(format!("TWD {:.0} / m³", current_cost_m3));
            });

            // Section 4: BIM 識別資訊 (Identity Data)
            render_category_header(ui, "BIM 識別資訊 (Identity Data)");

            vs_property_row(ui, "IFC GUID", "符合 buildingSMART IFC4 國際標準之 22 碼全域唯一識別碼。", state, |ui| {
                ui.label(egui::RichText::new(id.to_ifc_guid()).monospace().size(11.0).color(egui::Color32::from_rgb(180, 215, 255)));
            });
            vs_property_row(ui, "內部 Element ID", "SketchCAD 核心圖形資料庫原生 UUID 節點辨識符號。", state, |ui| {
                ui.label(egui::RichText::new(format!("{:?}", id)).monospace().small());
            });
            vs_property_row(ui, "圖層可見度 (Visibility)", "此構件在 2D 工程圖與 3D 空間實體模型中的即時可視狀態。", state, |ui| {
                let vis = doc.is_visible(id);
                let text = if vis { "👁 正常可見" } else { "⊘ 已隱藏" };
                let color = if vis { egui::Color32::from_rgb(80, 220, 120) } else { egui::Color32::from_rgb(220, 80, 80) };
                if ui.button(egui::RichText::new(text).color(color)).clicked() {
                    doc.toggle_visibility(id);
                }
            });
        });

    if needs_sync {
        doc.sync();
    }
}

/// Visual Studio Category Header
fn render_category_header(ui: &mut Ui, title: &str) {
    ui.add_space(3.0);
    let rect = ui.available_rect_before_wrap();
    let header_rect = egui::Rect::from_min_size(rect.min, egui::vec2(ui.available_width(), 20.0));
    ui.painter().rect_filled(header_rect, 2.0, egui::Color32::from_rgb(38, 45, 58));
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("▼ {}", title)).strong().size(11.5).color(egui::Color32::from_rgb(175, 205, 245)));
    });
    ui.add_space(2.0);
}

/// Visual Studio 2-Column Property Row Helper
fn vs_property_row<R>(
    ui: &mut Ui,
    prop_name: &str,
    prop_desc: &str,
    state: &mut InspectorState,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if !state.search_filter.is_empty() && !prop_name.to_lowercase().contains(&state.search_filter.to_lowercase()) {
        return None;
    }

    let mut result = None;
    let row_response = ui.horizontal_wrapped(|ui| {
        // Left Column: Property Name (Responsive width based on panel width)
        let label_width = (ui.available_width() * 0.42).clamp(60.0, 130.0);
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(egui::Label::new(
                    egui::RichText::new(prop_name).size(11.5).color(egui::Color32::from_rgb(215, 220, 230))
                ).wrap());
            },
        );

        // Vertical Separator
        ui.separator();

        // Right Column: Value Editor
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width().max(40.0), 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                result = Some(add_contents(ui));
            },
        );

    }).response;

    if row_response.hovered() {
        state.hovered_prop = (prop_name.to_string(), prop_desc.to_string());
    }

    result
}

/// Visual Studio Iconic Description / Help Pane at the bottom of the property grid
fn render_vs_help_pane(ui: &mut Ui, state: &InspectorState) {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(20, 24, 30))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 75)))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.add(egui::Label::new(
                egui::RichText::new(&state.hovered_prop.0)
                    .strong()
                    .size(12.5)
                    .color(egui::Color32::from_rgb(255, 225, 90)),
            ).wrap());
            ui.add_space(2.0);
            ui.add(egui::Label::new(
                egui::RichText::new(&state.hovered_prop.1)
                    .size(11.5)
                    .color(egui::Color32::from_rgb(190, 200, 215)),
            ).wrap());
        });
}

/// Live Quantity Takeoff Schedule (QTO)
fn render_live_qto_schedule(
    ui: &mut Ui,
    doc: &mut BimDocument,
    selected_id: &mut Option<ElementId>,
) {
    egui::CollapsingHeader::new(
        egui::RichText::new("📊 即時數量與工程造價明細表 (Live QTO)")
            .strong()
            .size(14.0)
            .color(egui::Color32::from_rgb(220, 230, 245)),
    )
    .default_open(true)
    .show(ui, |ui| {
        let qto = doc.generate_master_qto();
        let total_cost = qto.total_cost();

        // Summary Badge
        ui.horizontal_wrapped(|ui| {
            ui.label("預估工程總造價:");
            ui.colored_label(
                egui::Color32::from_rgb(60, 210, 100),
                format!("TWD {:>12.2}", total_cost),
            );
        });

        ui.add_space(4.0);

        // QTO Table with bidirectional scroll so narrow panel widths won't snap back
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .max_height(240.0)
            .show(ui, |ui| {
                egui::Grid::new("qto_table_grid_vs")
                    .striped(true)
                    .min_col_width(45.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("工程工項").strong());
                        ui.label(egui::RichText::new("數量").strong());
                        ui.label(egui::RichText::new("單位").strong());
                        ui.label(egui::RichText::new("單價").strong());
                        ui.label(egui::RichText::new("複價 (TWD)").strong());
                        ui.end_row();

                        for item in &qto.items {
                            let is_selected = Some(item.element_id) == *selected_id;
                            let text = egui::RichText::new(&item.description);
                            let label = if is_selected {
                                ui.selectable_label(true, text.color(egui::Color32::from_rgb(255, 215, 0)))
                            } else {
                                ui.selectable_label(false, text)
                            };
                            if label.clicked() {
                                *selected_id = Some(item.element_id);
                            }

                            ui.label(format!("{:.2}", item.quantity));
                            ui.label(&item.unit);
                            ui.label(format!("{:.0}", item.unit_price));
                            ui.label(format!("{:.0}", item.total_price));
                            ui.end_row();
                        }
                    });
            });
    });
}
