use crate::gui::inspector::{InspectorPanel, InspectorState};
use crate::gui::viewport_2d::{ToolMode, Viewport2D};
use crate::gui::viewport_3d::Viewport3D;
use eframe::egui;
use sketchcad_bim::{
    Beam, BimDocument, Column, CurtainWall, Door, FurnitureKind, LightType, Material, Ramp, Roof,
    SpaceType, Wall, Window,
};
use sketchcad_core::{ElementId, Point2D, Point3D};
use sketchcad_geom::OpeningVoid;
use sketchcad_io::{BcfHandler, BcfTopic, DwgAdapter, DxfHandler, IfcExporter, IfcImporter};
use crate::updater::{self, UpdateState, Updater};
use std::path::Path;

/// Application developer shown in the About dialog
const DEVELOPER: &str = "James Chang";

/// Display version, e.g. "0.4" for package version 0.4.0
fn app_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    version.strip_suffix(".0").unwrap_or(version).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLayout {
    Split2D3D,
    Drafting2DOnly,
    Orbit3DOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Ifc,
    Dxf,
    Bcf,
    Skc,
}

pub struct SketchCadApp {
    pub doc: BimDocument,
    pub viewport_2d: Viewport2D,
    pub viewport_3d: Viewport3D,
    pub layout: ViewLayout,
    pub tool_mode: ToolMode,
    pub selected_id: Option<ElementId>,
    pub status_text: String,
    pub notification: Option<(String, std::time::Instant)>,
    pub show_inspector: bool,
    pub inspector_state: InspectorState,
    pub undo_stack: Vec<BimDocument>,
    pub redo_stack: Vec<BimDocument>,
    pub clipboard: Option<Box<dyn sketchcad_bim::BimElement>>,
    pub inspector_width: f32,
    pub target_inspector_width: Option<f32>,

    // File I/O and Export Modal state
    pub project_file_path: Option<std::path::PathBuf>,
    pub show_export_dialog: bool,
    pub export_path_buffer: String,
    pub export_format: ExportFormat,
    pub export_bcf_title: String,
    pub export_bcf_desc: String,
    pub show_about: bool,
    pub updater: Updater,
}

impl SketchCadApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_cjk_fonts(&cc.egui_ctx);

        let mut doc = BimDocument::new("SketchCAD Architectural Project");
        Self::populate_sample_model(&mut doc);

        // Check GitHub for a newer release in the background at startup
        updater::cleanup_previous_install();
        let updater = Updater::default();
        updater.check(&cc.egui_ctx, false);

        let mut viewport_3d = Viewport3D::new();
        viewport_3d.attach_gpu(cc.wgpu_render_state.as_ref());

        Self {
            doc,
            viewport_2d: Viewport2D::new(),
            viewport_3d,
            layout: ViewLayout::Split2D3D,
            tool_mode: ToolMode::Select,
            selected_id: None,
            status_text: "就緒 (Ready) | BIM 參數化核心已連動".to_string(),
            notification: Some(("歡迎使用 SketchCAD 次世代建築資訊模型系統！".to_string(), std::time::Instant::now())),
            show_inspector: true,
            inspector_state: InspectorState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard: None,
            inspector_width: 280.0,
            target_inspector_width: None,
            project_file_path: None,
            show_export_dialog: false,
            export_path_buffer: "exports/project_export.ifc".to_string(),
            export_format: ExportFormat::Ifc,
            export_bcf_title: "SketchCAD 設計議題審查".to_string(),
            export_bcf_desc: "由 SketchCAD 桌面端匯出之協同審查議題".to_string(),
            show_about: false,
            updater,
        }
    }

    pub fn populate_sample_model(doc: &mut BimDocument) {
        let concrete = Material::concrete();
        let wood = Material::wood();
        let glass = Material::glass();
        let steel = Material::structural_steel();

        // 1. Structural Floor Slab (8000x6000x200mm)
        doc.insert_slab(
            Point2D::new(0.0, 0.0),
            8000.0,
            6000.0,
            200.0,
            0.0,
            concrete.clone(),
        );

        // 2. Suspended Ceiling (8000x6000x30mm, elevation 2800mm)
        doc.insert_ceiling(
            Point2D::new(0.0, 0.0),
            8000.0,
            6000.0,
            2800.0,
            Material::gypsum(),
        );

        // 3. Straight Run Stairs (width 1000mm, total rise 3000mm, 15 steps)
        let stair_id = doc.insert_stairs(
            Point2D::new(1000.0, 1000.0),
            Point2D::new(4500.0, 1000.0),
            1000.0,
            3000.0,
            15,
            concrete.clone(),
        );
        if let Some(stair) = doc.get_stairs_mut(stair_id) {
            stair.base_z = 200.0;
        }

        // 4. Stainless Steel & Safety Glass Railing along the staircase
        let railing = sketchcad_bim::Railing::new(
            Point3D::new(1000.0, 1530.0, 200.0),
            Point3D::new(4500.0, 1530.0, 3200.0),
            1100.0,
            steel.clone(),
        );
        doc.add_element(railing);

        // Wall 1 with door & window openings
        let mut wall_1 = Wall::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(8000.0, 0.0),
            200.0,
            3500.0,
            concrete.clone(),
        );
        wall_1.add_opening(OpeningVoid {
            offset_along_wall: 2500.0,
            width: 1000.0,
            base_z: 0.0,
            height: 2100.0,
        });
        wall_1.add_opening(OpeningVoid {
            offset_along_wall: 5200.0,
            width: 1800.0,
            base_z: 900.0,
            height: 1400.0,
        });
        let wall_1_id = doc.add_element(wall_1);

        // Wall 2
        let wall_2 = Wall::new(
            Point2D::new(8000.0, 0.0),
            Point2D::new(8000.0, 6000.0),
            200.0,
            3500.0,
            concrete.clone(),
        );
        doc.add_element(wall_2);

        // Columns
        let col_1 = Column::new(Point2D::new(0.0, 0.0), 500.0, 500.0, 3500.0, concrete.clone());
        doc.add_element(col_1);

        let col_2 = Column::new(Point2D::new(8000.0, 0.0), 500.0, 500.0, 3500.0, concrete.clone());
        doc.add_element(col_2);

        // Beam
        let beam = Beam::new(
            Point3D::new(0.0, 0.0, 3500.0),
            Point3D::new(8000.0, 0.0, 3500.0),
            350.0,
            700.0,
            steel,
        );
        doc.add_element(beam);

        // Hosted Door & Window
        let mut door = Door::new(wall_1_id, 2500.0, 1000.0, 2100.0, wood, 9800.0);
        door.set_transform(Point2D::new(2500.0, 0.0), sketchcad_core::Vector2D::X);
        doc.add_element(door);

        let mut window = Window::new(wall_1_id, 5200.0, 900.0, 1800.0, 1400.0, glass.clone(), 16500.0);
        window.set_transform(Point2D::new(6100.0, 0.0), sketchcad_core::Vector2D::X);
        doc.add_element(window);

        // Roof (雙坡斜屋頂系統)
        let roof = Roof::new(
            Point2D::new(-300.0, -300.0),
            8600.0,
            6600.0,
            3500.0,
            20.0,
            concrete.clone(),
        );
        doc.add_element(roof);

        // Accessible Ramp (1:12 無障礙坡道)
        let ramp = Ramp::new(
            Point2D::new(8500.0, 1000.0),
            Point2D::new(8500.0, 4800.0),
            1200.0,
            0.0,
            316.0,
            concrete.clone(),
        );
        doc.add_element(ramp);

        // Curtain Wall facade (單元式玻璃幕牆系統)
        let cw = CurtainWall::new(
            Point2D::new(0.0, 6000.0),
            Point2D::new(8000.0, 6000.0),
            3500.0,
            0.0,
            1600.0,
            glass,
        );
        doc.add_element(cw);

        // Interior Spaces (室內空間區域與坪數標記)
        let living_room_boundary = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(4800.0, 0.0),
            Point2D::new(4800.0, 6000.0),
            Point2D::new(0.0, 6000.0),
        ];
        doc.insert_space(
            SpaceType::LivingRoom,
            living_room_boundary,
            200.0,
            3000.0,
            Material::wood_flooring(),
        );

        let bedroom_boundary = vec![
            Point2D::new(4800.0, 0.0),
            Point2D::new(8000.0, 0.0),
            Point2D::new(8000.0, 6000.0),
            Point2D::new(4800.0, 6000.0),
        ];
        doc.insert_space(
            SpaceType::MasterBedroom,
            bedroom_boundary,
            200.0,
            3000.0,
            Material::wood_flooring(),
        );

        // Interior Furnishings & FF&E (室內家具軟裝配置)
        // 1. Living room sofa
        doc.insert_furniture(
            FurnitureKind::Sofa,
            Point2D::new(2400.0, 4800.0),
            200.0,
            0.0,
            Material::fabric(),
        );

        // 2. Dining Table with chairs
        doc.insert_furniture(
            FurnitureKind::DiningTable,
            Point2D::new(2400.0, 2200.0),
            200.0,
            0.0,
            Material::walnut(),
        );

        // 3. Master Bed
        doc.insert_furniture(
            FurnitureKind::Bed,
            Point2D::new(6400.0, 4600.0),
            200.0,
            0.0,
            Material::fabric(),
        );

        // 4. Master Wardrobe
        doc.insert_furniture(
            FurnitureKind::Wardrobe,
            Point2D::new(7600.0, 2500.0),
            200.0,
            90.0,
            Material::walnut(),
        );

        // Ceiling Lighting (天花板照明燈具系統)
        doc.insert_light_fixture(
            LightType::CeilingLamp,
            Point2D::new(2400.0, 4800.0),
            2800.0,
        );

        doc.insert_light_fixture(
            LightType::Pendant,
            Point2D::new(2400.0, 2200.0),
            2600.0,
        );

        doc.insert_light_fixture(
            LightType::Downlight,
            Point2D::new(6400.0, 4600.0),
            2800.0,
        );

        doc.sync();
    }


    fn show_notify(&mut self, msg: impl Into<String>) {
        self.notification = Some((msg.into(), std::time::Instant::now()));
    }

    pub fn save_undo_state(&mut self) {
        if self.undo_stack.len() >= 50 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(self.doc.clone());
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.doc.clone());
            self.doc = prev;
            self.selected_id = None;
            self.show_notify("↶ 已復原 (Undo)");
        } else {
            self.show_notify("已無更早的操作可復原");
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.doc.clone());
            self.doc = next;
            self.selected_id = None;
            self.show_notify("↷ 已重做 / 取消復原 (Redo)");
        } else {
            self.show_notify("已無可取消復原的操作");
        }
    }

    pub fn cut(&mut self) {
        if let Some(id) = self.selected_id {
            if let Some(elem) = self.doc.get_element(id) {
                let name = elem.name().to_string();
                let boxed = elem.clone_box();
                self.save_undo_state();
                self.clipboard = Some(boxed);
                self.doc.remove_element(id);
                self.selected_id = None;
                self.show_notify(format!("✂️ 已剪下構件: {}", name));
            }
        }
    }

    pub fn copy(&mut self) {
        if let Some(id) = self.selected_id {
            if let Some(elem) = self.doc.get_element(id) {
                self.clipboard = Some(elem.clone_box());
                self.show_notify(format!("📋 已複製構件: {}", elem.name()));
            }
        }
    }

    pub fn paste(&mut self) {
        let clip_opt = self.clipboard.as_ref().map(|c| c.clone_box());
        if let Some(clip) = clip_opt {
            self.save_undo_state();
            let offset = sketchcad_core::Vector3D::new(500.0, 500.0, 0.0);
            let new_id = self.doc.paste_element(clip.as_ref(), offset);
            self.selected_id = Some(new_id);
            if let Some(elem) = self.doc.get_element(new_id) {
                self.show_notify(format!("📄 已貼上構件: {}", elem.name()));
            }
        } else {
            self.show_notify("剪貼簿為空，請先複製或剪下構件");
        }
    }

    pub fn new_project(&mut self) {
        self.save_undo_state();
        self.doc = BimDocument::new("新建築專案");
        self.selected_id = None;
        self.project_file_path = None;
        self.show_notify("已建立空白新專案");
    }

    pub fn prompt_open_file(&mut self) {
        let file = rfd::FileDialog::new()
            .set_title("開啟工程檔案")
            .add_filter(
                "CAD & BIM 工程檔案 (*.dxf, *.ifc, *.dwg, *.bcf, *.bcfzip, *.skc)",
                &["dxf", "ifc", "dwg", "bcf", "bcfzip", "skc"],
            )
            .add_filter("OpenBIM IFC4 模型 (*.ifc)", &["ifc"])
            .add_filter("AutoCAD DXF 圖檔 (*.dxf)", &["dxf"])
            .add_filter("AutoCAD DWG 圖檔 (*.dwg)", &["dwg"])
            .add_filter("BIM 協同議題封包 (*.bcf, *.bcfzip)", &["bcf", "bcfzip"])
            .add_filter("SketchCAD 專案檔案 (*.skc)", &["skc", "json"])
            .add_filter("所有檔案 (*.*)", &["*"])
            .pick_file();

        if let Some(path) = file {
            self.open_file(&path);
        }
    }

    pub fn prompt_save_as(&mut self) {
        let default_name = self
            .project_file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("my_project.skc");

        let file = rfd::FileDialog::new()
            .set_title("另存專案新檔")
            .set_file_name(default_name)
            .add_filter("SketchCAD 原生工程專案 (*.skc)", &["skc"])
            .save_file();

        if let Some(path) = file {
            self.save_project_as(&path);
        }
    }

    pub fn prompt_export_browse(&mut self) {
        let (filter_name, ext) = match self.export_format {
            ExportFormat::Ifc => ("OpenBIM IFC4 模型 (*.ifc)", "ifc"),
            ExportFormat::Dxf => ("AutoCAD DXF 圖檔 (*.dxf)", "dxf"),
            ExportFormat::Bcf => ("BIM 協同議題封包 (*.bcfzip)", "bcfzip"),
            ExportFormat::Skc => ("SketchCAD 原生專案 (*.skc)", "skc"),
        };
        let default_name = format!("project_export.{}", ext);
        let file = rfd::FileDialog::new()
            .set_title("選擇匯出目標路徑")
            .set_file_name(&default_name)
            .add_filter(filter_name, &[ext])
            .save_file();

        if let Some(path) = file {
            self.export_path_buffer = path.to_string_lossy().to_string();
        }
    }

    pub fn save_project(&mut self) {
        if let Some(path) = self.project_file_path.clone() {
            if let Err(e) = self.doc.save_to_file(&path) {
                self.show_notify(format!("儲存專案失敗: {}", e));
            } else {
                self.show_notify(format!("專案已成功儲存至: {}", path.display()));
            }
        } else {
            self.prompt_save_as();
        }
    }

    pub fn save_project_as<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match self.doc.save_to_file(path) {
            Ok(()) => {
                self.project_file_path = Some(path.to_path_buf());
                self.show_notify(format!("專案已成功另存新檔至: {}", path.display()));
            }
            Err(e) => {
                self.show_notify(format!("另存新檔失敗: {}", e));
            }
        }
    }

    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if !path.exists() {
            self.show_notify(format!("找不到檔案: {}", path.display()));
            return;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "ifc" => {
                match IfcImporter::import_from_file(path) {
                    Ok(imported_doc) => {
                        let count = imported_doc.element_count();
                        self.save_undo_state();
                        self.doc = imported_doc;
                        self.selected_id = None;
                        self.project_file_path = None;
                        self.viewport_3d.frame_document(&self.doc);
                        self.viewport_2d.frame_document(&self.doc);
                        self.show_notify(format!("成功開啟 IFC4 建築模型！載入 {} 個構件", count));
                    }
                    Err(e) => {
                        self.show_notify(format!("開啟 IFC 失敗: {}", e));
                    }
                }
            }
            "dxf" => {
                self.save_undo_state();
                match DxfHandler::import_dxf_into_document(path, &mut self.doc) {
                    Ok(count) => {
                        self.selected_id = None;
                        self.show_notify(format!("成功自 DXF 匯入並轉換 {} 個構件！", count));
                    }
                    Err(e) => {
                        self.show_notify(format!("開啟 DXF 失敗: {}", e));
                    }
                }
            }
            "dwg" => {
                self.save_undo_state();
                match DwgAdapter::import_dwg_into_document(path, &mut self.doc) {
                    Ok(count) => {
                        self.selected_id = None;
                        self.show_notify(format!("成功自 DWG 匯入並轉換 {} 個構件！", count));
                    }
                    Err(e) => {
                        self.show_notify(format!("開啟 DWG 失敗: {}", e));
                    }
                }
            }
            "bcf" | "bcfzip" => {
                match BcfHandler::import_bcf_package(path) {
                    Ok(topics) => {
                        let count = topics.len();
                        if let Some(first) = topics.first() {
                            self.show_notify(format!("成功載入 BCF 協同議題封包！共 {} 個議題，首項: {}", count, first.title));
                        } else {
                            self.show_notify("成功載入 BCF 協同議題封包 (無議題)");
                        }
                    }
                    Err(e) => {
                        self.show_notify(format!("開啟 BCF 失敗: {}", e));
                    }
                }
            }
            "skc" | "json" | "sketchcad" => {
                match BimDocument::load_from_file(path) {
                    Ok(loaded_doc) => {
                        let count = loaded_doc.element_count();
                        self.save_undo_state();
                        self.doc = loaded_doc;
                        self.selected_id = None;
                        self.project_file_path = Some(path.to_path_buf());
                        self.show_notify(format!("成功開啟專案: {}，恢復 {} 個構件", path.file_name().unwrap_or_default().to_string_lossy(), count));
                    }
                    Err(e) => {
                        self.show_notify(format!("開啟專案失敗: {}", e));
                    }
                }
            }
            _ => {
                self.show_notify(format!("不支援的副檔名: .{} (支援: dxf, ifc, dwg, bcf, skc)", ext));
            }
        }
    }

    pub fn do_export(&mut self) {
        let path = Path::new(&self.export_path_buffer);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match self.export_format {
            ExportFormat::Ifc => {
                match IfcExporter::export_to_ifc4(&self.doc, path) {
                    Ok(()) => self.show_notify(format!("成功匯出 IFC4 模型至: {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出 IFC 失敗: {}", e)),
                }
            }
            ExportFormat::Dxf => {
                match DxfHandler::export_document_2d(&self.doc, path) {
                    Ok(()) => self.show_notify(format!("成功匯出 DXF 2D 平面圖至: {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出 DXF 失敗: {}", e)),
                }
            }
            ExportFormat::Bcf => {
                let topic = BcfTopic::new(
                    self.export_bcf_title.clone(),
                    self.export_bcf_desc.clone(),
                );
                match BcfHandler::export_bcf_package(&[topic], path) {
                    Ok(()) => self.show_notify(format!("成功匯出 BCF 協同封包至: {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出 BCF 失敗: {}", e)),
                }
            }
            ExportFormat::Skc => {
                match self.doc.save_to_file(path) {
                    Ok(()) => self.show_notify(format!("成功匯出專案檔案至: {}", path.display())),
                    Err(e) => self.show_notify(format!("匯出專案失敗: {}", e)),
                }
            }
        }
    }

    /// 「關於」視窗：產品名稱、版本編號與開發者
    fn render_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("關於 SketchCAD")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("📐 SketchCAD").size(24.0).strong());
                    ui.label("參數化建築資訊模型 (BIM) 系統");
                    ui.add_space(10.0);
                    egui::Grid::new("about_grid").num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
                        ui.label(egui::RichText::new("版本").strong());
                        ui.label(format!("v{}", app_version()));
                        ui.end_row();
                        ui.label(egui::RichText::new("開發者").strong());
                        ui.label(DEVELOPER);
                        ui.end_row();
                        ui.label(egui::RichText::new("授權").strong());
                        ui.label(env!("CARGO_PKG_LICENSE"));
                        ui.end_row();
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let busy = self.updater.is_busy();
                        if ui
                            .add_enabled(!busy, egui::Button::new("🔄 檢查更新"))
                            .on_hover_text("到 GitHub 檢查是否有新版本")
                            .clicked()
                        {
                            self.updater.check(ctx, true);
                        }
                        if matches!(self.updater.state(), UpdateState::Checking { manual: true }) {
                            ui.spinner();
                            ui.label("檢查中…");
                        }
                        if ui.button("確定").clicked() {
                            self.show_about = false;
                        }
                    });
                    ui.add_space(4.0);
                });
            });
        if !open {
            self.show_about = false;
        }
    }

    /// Update prompts: new version found, download progress, restart, errors.
    fn render_update_window(&mut self, ctx: &egui::Context) {
        let state = self.updater.state();
        let title = match &state {
            UpdateState::UpToDate => "檢查更新",
            UpdateState::Available(_) => "發現新版本",
            UpdateState::Downloading { .. } => "正在更新",
            UpdateState::Installed { .. } => "更新完成",
            UpdateState::Failed { .. } => "更新失敗",
            UpdateState::Idle | UpdateState::Checking { .. } => return,
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                match state {
                    UpdateState::UpToDate => {
                        ui.label(format!("目前已是最新版本 (v{})。", app_version()));
                        ui.add_space(8.0);
                        if ui.button("確定").clicked() {
                            self.updater.dismiss();
                        }
                    }
                    UpdateState::Available(release) => {
                        ui.label(
                            egui::RichText::new(format!("SketchCAD v{} 已發布", release.version()))
                                .strong()
                                .size(15.0),
                        );
                        ui.label(format!("目前版本：v{}", app_version()));
                        if let Some(notes) = release.body.as_deref().filter(|b| !b.trim().is_empty()) {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("更新內容：").strong());
                            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                                ui.label(notes);
                            });
                        }
                        ui.add_space(8.0);
                        let self_update = Updater::can_self_update(&release);
                        if self_update {
                            ui.label(
                                egui::RichText::new("更新完成後需要重新啟動，請先儲存目前的專案。")
                                    .small()
                                    .color(egui::Color32::from_rgb(240, 190, 90)),
                            );
                        }
                        ui.horizontal(|ui| {
                            let label = if self_update { "⬇ 立即更新" } else { "🌐 前往下載頁面" };
                            if ui.button(label).clicked() {
                                self.updater.install(ctx, release.clone());
                            }
                            if ui.button("稍後再說").clicked() {
                                self.updater.dismiss();
                            }
                        });
                    }
                    UpdateState::Downloading { version, received, total } => {
                        ui.label(format!("正在下載 SketchCAD v{version}…"));
                        let fraction = if total > 0 { received as f32 / total as f32 } else { 0.0 };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(320.0)
                                .text(format!("{:.1} / {:.1} MB", received as f64 / 1e6, total as f64 / 1e6)),
                        );
                    }
                    UpdateState::Installed { version, relaunch } => {
                        ui.label(format!("已更新至 v{version}，重新啟動後即可使用新版本。"));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("🔁 立即重新啟動").clicked() {
                                match updater::relaunch(&relaunch) {
                                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                                    Err(e) => self.show_notify(format!("無法重新啟動：{e}，請手動開啟 SketchCAD")),
                                }
                            }
                            if ui.button("稍後").clicked() {
                                self.updater.dismiss();
                            }
                        });
                    }
                    UpdateState::Failed { message, release_url } => {
                        ui.label(message);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if let Some(url) = release_url {
                                if ui.button("🌐 手動下載").clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                    self.updater.dismiss();
                                }
                            }
                            if ui.button("關閉").clicked() {
                                self.updater.dismiss();
                            }
                        });
                    }
                    UpdateState::Idle | UpdateState::Checking { .. } => {}
                }
            });
    }

    pub fn load_sample_villa_ifc(&mut self) {
        let path = Path::new("samples/sample_villa.ifc");
        if path.exists() {
            self.open_file(path);
        } else {
            self.show_notify("未找到 samples/sample_villa.ifc");
        }
    }

    pub fn load_sample_dwg(&mut self) {
        let path = Path::new("samples/sample_villa.dwg");
        if path.exists() {
            self.open_file(path);
        } else {
            self.show_notify("未找到 samples/sample_villa.dwg");
        }
    }

    pub fn load_sample_dxf(&mut self) {
        let path = Path::new("samples/sample_villa.dxf");
        if path.exists() {
            self.open_file(path);
        } else {
            self.show_notify("未找到 samples/sample_villa.dxf");
        }
    }

    pub fn load_sample_bcf(&mut self) {
        let path = Path::new("samples/sample_villa.bcfzip");
        if path.exists() {
            self.open_file(path);
        } else {
            self.show_notify("未找到 samples/sample_villa.bcfzip");
        }
    }

    pub fn load_sample_full(&mut self) {
        self.save_undo_state();
        let mut doc = BimDocument::new("完整軟裝與帷幕現代別墅 (Full Architectural Sample)");
        Self::populate_sample_model(&mut doc);
        self.doc = doc;
        self.selected_id = None;
        self.project_file_path = None;
        self.show_notify("成功載入完整建築別墅範例模型！");
    }

    pub fn render_file_modals(&mut self, ctx: &egui::Context) {
        // Export Dialog Modal
        if self.show_export_dialog {
            let mut is_open = true;
            egui::Window::new("📤 匯出工程檔案 (Export Model)")
                .open(&mut is_open)
                .collapsible(false)
                .resizable(true)
                .default_width(540.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("請選擇欲匯出的工程格式與目標檔案:").strong());
                    ui.add_space(8.0);

                    // Format Radio Selection
                    ui.group(|ui| {
                        let prev_format = self.export_format;
                        ui.radio_value(&mut self.export_format, ExportFormat::Ifc, "🏛️ OpenBIM IFC4 建築資訊模型 (*.ifc)");
                        ui.label(egui::RichText::new("    符合 buildingSMART 標準，保留全部牆/板/柱/樑/門窗實體及 3D 幾何語義").small().color(egui::Color32::from_rgb(160, 175, 195)));
                        ui.add_space(4.0);

                        ui.radio_value(&mut self.export_format, ExportFormat::Dxf, "📐 AutoCAD 2D 工程平面圖 (*.dxf)");
                        ui.label(egui::RichText::new("    匯出精確 2D 向量多段線圖元與建築標註，相容各類 CAD 製圖軟體").small().color(egui::Color32::from_rgb(160, 175, 195)));
                        ui.add_space(4.0);

                        ui.radio_value(&mut self.export_format, ExportFormat::Bcf, "📋 BIM 協同議題報告 (*.bcfzip)");
                        ui.label(egui::RichText::new("    符合 BCF 2.1 標準，封裝設計衝突審查、視角導航與構件 GUID 關聯").small().color(egui::Color32::from_rgb(160, 175, 195)));
                        ui.add_space(4.0);

                        ui.radio_value(&mut self.export_format, ExportFormat::Skc, "💾 SketchCAD 原生工程專案 (*.skc)");
                        ui.label(egui::RichText::new("    完整保存所有參數化構件、材質紋理、圖層可見度與工程量估驗清單").small().color(egui::Color32::from_rgb(160, 175, 195)));

                        if prev_format != self.export_format {
                            let ext = match self.export_format {
                                ExportFormat::Ifc => "ifc",
                                ExportFormat::Dxf => "dxf",
                                ExportFormat::Bcf => "bcfzip",
                                ExportFormat::Skc => "skc",
                            };
                            self.export_path_buffer = format!("exports/project_export.{}", ext);
                        }
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("匯出路徑:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.export_path_buffer)
                                .desired_width(ui.available_width() - 85.0),
                        );
                        if ui.button("📂 瀏覽...").clicked() {
                            self.prompt_export_browse();
                        }
                    });

                    // Extra fields for BCF
                    if self.export_format == ExportFormat::Bcf {
                        ui.add_space(6.0);
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("BCF 議題參數設定:").small().strong());
                            ui.horizontal(|ui| {
                                ui.label("議題標題:");
                                ui.text_edit_singleline(&mut self.export_bcf_title);
                            });
                            ui.horizontal(|ui| {
                                ui.label("議題說明:");
                                ui.text_edit_singleline(&mut self.export_bcf_desc);
                            });
                        });
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("取消 (Cancel)").clicked() {
                                self.show_export_dialog = false;
                            }
                            let export_btn = egui::Button::new(
                                egui::RichText::new("📤 確定匯出 (Export)").color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(40, 150, 90));
                            if ui.add(export_btn).clicked() {
                                self.do_export();
                                self.show_export_dialog = false;
                            }
                        });
                    });
                });
            if !is_open {
                self.show_export_dialog = false;
            }
        }
    }
}

impl eframe::App for SketchCadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut do_undo = false;
        let mut do_redo = false;
        let mut do_cut = false;
        let mut do_copy = false;
        let mut do_paste = false;
        let mut do_save = false;
        let mut do_save_as = false;
        let mut do_open = false;
        let mut do_export = false;
        let mut do_new = false;

        // Keyboard Shortcuts for CAD Tools
        if !ctx.wants_keyboard_input() {
            ctx.input(|i| {
                if i.modifiers.command || i.modifiers.ctrl {
                    if i.key_pressed(egui::Key::Z) && !i.modifiers.shift {
                        do_undo = true;
                    } else if (i.key_pressed(egui::Key::Z) && i.modifiers.shift) || i.key_pressed(egui::Key::Y) {
                        do_redo = true;
                    } else if i.key_pressed(egui::Key::X) {
                        do_cut = true;
                    } else if i.key_pressed(egui::Key::C) {
                        do_copy = true;
                    } else if i.key_pressed(egui::Key::V) {
                        do_paste = true;
                    } else if i.key_pressed(egui::Key::S) && !i.modifiers.shift {
                        do_save = true;
                    } else if i.key_pressed(egui::Key::S) && i.modifiers.shift {
                        do_save_as = true;
                    } else if i.key_pressed(egui::Key::O) {
                        do_open = true;
                    } else if i.key_pressed(egui::Key::E) {
                        do_export = true;
                    } else if i.key_pressed(egui::Key::N) {
                        do_new = true;
                    }
                } else if i.key_pressed(egui::Key::Escape) || (!i.modifiers.command && !i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
                    self.tool_mode = ToolMode::Select;
                    self.viewport_2d.wall_start_pt = None;
                    self.viewport_2d.slab_start_pt = None;
                    self.viewport_2d.ceiling_start_pt = None;
                    self.viewport_2d.stair_start_pt = None;
                    self.viewport_2d.railing_start_pt = None;
                    self.viewport_2d.roof_start_pt = None;
                    self.viewport_2d.ramp_start_pt = None;
                    self.viewport_2d.curtain_wall_start_pt = None;
                    self.viewport_2d.move_base_pt = None;
                } else if i.key_pressed(egui::Key::M) {
                    self.tool_mode = ToolMode::Move;
                    self.viewport_2d.move_base_pt = None;
                } else if (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
                    && self.selected_id.is_some()
                {
                    if let Some(id) = self.selected_id {
                        self.save_undo_state();
                        self.doc.remove_element(id);
                        self.selected_id = None;
                        self.show_notify("已刪除選定構件");
                    }
                } else if i.key_pressed(egui::Key::W) && !i.modifiers.shift {
                    self.tool_mode = ToolMode::Wall;
                    self.viewport_2d.wall_start_pt = None;
                } else if i.key_pressed(egui::Key::W) && i.modifiers.shift {
                    self.tool_mode = ToolMode::Window;
                } else if i.key_pressed(egui::Key::D) {
                    self.tool_mode = ToolMode::Door;
                } else if i.key_pressed(egui::Key::C) {
                    self.tool_mode = ToolMode::Column;
                } else if i.key_pressed(egui::Key::F) {
                    self.tool_mode = ToolMode::Slab;
                    self.viewport_2d.slab_start_pt = None;
                } else if i.key_pressed(egui::Key::T) {
                    self.tool_mode = ToolMode::Ceiling;
                    self.viewport_2d.ceiling_start_pt = None;
                } else if i.key_pressed(egui::Key::R) {
                    self.tool_mode = ToolMode::Stairs;
                    self.viewport_2d.stair_start_pt = None;
                } else if i.key_pressed(egui::Key::P) {
                    self.tool_mode = ToolMode::PushPull;
                }
            });
        }

        if do_undo { self.undo(); }
        if do_redo { self.redo(); }
        if do_cut { self.cut(); }
        if do_copy { self.copy(); }
        if do_paste { self.paste(); }
        if do_save { self.save_project(); }
        if do_save_as { self.prompt_save_as(); }
        if do_open { self.prompt_open_file(); }
        if do_export { self.show_export_dialog = true; }
        if do_new { self.new_project(); }

        // 1. Top Menu Bar
        egui::TopBottomPanel::top("top_menu_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("檔案 (File)", |ui| {
                    if ui.button("📄 新增專案 (New Project) [Ctrl+N]").clicked() {
                        self.new_project();
                        ui.close_menu();
                    }
                    if ui.button("📂 開啟檔案... (Open File...) [Ctrl+O]").clicked() {
                        self.prompt_open_file();
                        ui.close_menu();
                    }
                    if ui.button("💾 儲存專案 (Save) [Ctrl+S]").clicked() {
                        self.save_project();
                        ui.close_menu();
                    }
                    if ui.button("💾 另存新檔... (Save As...) [Ctrl+Shift+S]").clicked() {
                        self.prompt_save_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("📂 載入範例 (Samples) ▶", |ui| {
                        if ui.button("🏛️ 現代度假別墅 IFC4 模型 (sample_villa.ifc)").clicked() {
                            self.load_sample_villa_ifc();
                            ui.close_menu();
                        }
                        if ui.button("📐 2D 建築平面圖 DWG (sample_villa.dwg)").clicked() {
                            self.load_sample_dwg();
                            ui.close_menu();
                        }
                        if ui.button("📏 2D 建築平面圖 DXF (sample_villa.dxf)").clicked() {
                            self.load_sample_dxf();
                            ui.close_menu();
                        }
                        if ui.button("📋 BIM 協同議題封包 (sample_villa.bcfzip)").clicked() {
                            self.load_sample_bcf();
                            ui.close_menu();
                        }
                        if ui.button("🛋️ 完整軟裝與帷幕別墅 (Villa with Furniture & Facade)").clicked() {
                            self.load_sample_full();
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if ui.button("📤 匯出檔案... (Export...) [Ctrl+E]").clicked() {
                        self.show_export_dialog = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("編輯 (Edit)", |ui| {
                    let has_undo = !self.undo_stack.is_empty();
                    let has_redo = !self.redo_stack.is_empty();
                    let has_sel = self.selected_id.is_some();
                    let has_clip = self.clipboard.is_some();

                    if ui.add_enabled(has_undo, egui::Button::new("↶ 復原 (Undo) [Ctrl+Z]")).clicked() {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_redo, egui::Button::new("↷ 取消復原 / 重做 (Redo) [Ctrl+Y / Ctrl+Shift+Z]")).clicked() {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add_enabled(has_sel, egui::Button::new("✂️ 剪下 (Cut) [Ctrl+X]")).clicked() {
                        self.cut();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("📋 複製 (Copy) [Ctrl+C]")).clicked() {
                        self.copy();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_clip, egui::Button::new("📄 貼上 (Paste) [Ctrl+V]")).clicked() {
                        self.paste();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add_enabled(has_sel, egui::Button::new("🗑️ 刪除選取構件 [Del / Backspace]")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.save_undo_state();
                            self.doc.remove_element(id);
                            self.selected_id = None;
                            self.show_notify("已刪除選定構件");
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("✥ 移動構件 [M]")).clicked() {
                        self.tool_mode = ToolMode::Move;
                        self.viewport_2d.move_base_pt = None;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("取消選取 [Esc]").clicked() {
                        self.selected_id = None;
                        self.tool_mode = ToolMode::Select;
                        ui.close_menu();
                    }
                });

                ui.menu_button("視圖 (View)", |ui| {
                    if ui.selectable_label(self.layout == ViewLayout::Split2D3D, "雙視窗 (2D 平面工程 + 3D 空間實體)").clicked() {
                        self.layout = ViewLayout::Split2D3D;
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.layout == ViewLayout::Drafting2DOnly, "僅 2D 平面工程視口").clicked() {
                        self.layout = ViewLayout::Drafting2DOnly;
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.layout == ViewLayout::Orbit3DOnly, "僅 3D 空間實體視口").clicked() {
                        self.layout = ViewLayout::Orbit3DOnly;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.show_inspector, "顯示 BIM 數據面板 (明細表/檢查器)");
                    ui.checkbox(&mut self.viewport_2d.show_dimensions, "📐 顯示 2D 尺寸標註與輔助線");
                    ui.checkbox(&mut self.viewport_3d.settings.enable_section_cut, "🔪 啟用 3D 動態剖切盒 (透視室內裝修)");
                    ui.separator();
                    if ui.button("⚙ 3D 渲染品質、解析度與剖切設定...").clicked() {
                        self.viewport_3d.settings.show_settings_panel = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("說明 (Help)", |ui| {
                    ui.label(format!("SketchCAD v{} (參數化建築資訊模型系統)", app_version()));
                    ui.label("• 2D 畫布: 滾輪/捏合縮放, 中鍵/右鍵/空白鍵+左鍵拖曳或觸控板雙指平移");
                    ui.label("• 3D 視口: 滑鼠左鍵旋轉軌道, 滾輪縮放, 右鍵/中鍵平移");
                });

                ui.menu_button("關於 (About)", |ui| {
                    if ui.button("ℹ 關於 SketchCAD...").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });

            // 2. Toolbar Ribbon with Vector Icons & Tooltips
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("↶ 復原"))
                    .on_hover_text("復原上一步操作 [Ctrl+Z]")
                    .clicked()
                {
                    self.undo();
                }
                if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("↷ 重做"))
                    .on_hover_text("取消復原 / 重做 [Ctrl+Y / Ctrl+Shift+Z]")
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();
                ui.label(egui::RichText::new("工具箱:").strong());

                let tools = [
                    (
                        ToolMode::Select,
                        "選取工具 (Select) [S / Esc]",
                        "點選 2D 或 3D 視口中的建築構件查看屬性與連動算量",
                    ),
                    (
                        ToolMode::Move,
                        "構件移動 (Move) [M]",
                        "選定構件後，在視口中點選基準點與目標位置進行互動平移",
                    ),
                    (
                        ToolMode::Wall,
                        "牆體繪製 (Wall) [W]",
                        "在 2D 平面圖中依序點擊起點與終點，即時生成帶厚度、高度與成本的 BIM 實體牆",
                    ),
                    (
                        ToolMode::Column,
                        "結構柱 (Column) [C]",
                        "放置標準混凝土/鋼構矩形柱與結構樑，自帶標準工程剖面符號",
                    ),
                    (
                        ToolMode::Door,
                        "門扇放置 (Door) [D]",
                        "於已選定之牆體上插入門扇，自動執行布林開洞與立面門片生成",
                    ),
                    (
                        ToolMode::Window,
                        "窗戶放置 (Window) [Shift+W]",
                        "於已選定之牆體上插入窗戶，自動進行實體鏤空並配置玻璃材質",
                    ),
                    (
                        ToolMode::Slab,
                        "結構樓板 (Slab) [F]",
                        "在 2D 平面依序點擊對角兩點建立結構樓板與筏基實體",
                    ),
                    (
                        ToolMode::Ceiling,
                        "天花板 (Ceiling) [T]",
                        "在 2D 平面依序點擊對角兩點建立輕鋼架天花板 (淨高 2800mm)",
                    ),
                    (
                        ToolMode::Stairs,
                        "直通樓梯 (Stairs) [R]",
                        "在 2D 平面點選起步點與終點建立標準建築直通樓梯與級高踏步",
                    ),
                    (
                        ToolMode::Railing,
                        "欄杆扶手 (Railing) [B]",
                        "在 2D 平面點選起點與終點，建立沿階梯爬升或陽台邊緣的安全防護欄杆與玻璃隔板",
                    ),
                    (
                        ToolMode::CurtainWall,
                        "幕牆系統 (Curtain Wall)",
                        "在 2D 平面依序點選起點與終點，建立帶鋁合金豎梃、橫樑與雙層玻璃的單元式幕牆系統",
                    ),
                    (
                        ToolMode::Roof,
                        "屋頂系統 (Roof)",
                        "在 2D 平面點選對角兩點建立帶坡度、出挑挑簷與屋脊線的雙坡屋頂系統",
                    ),
                    (
                        ToolMode::Ramp,
                        "無障礙坡道 (Ramp)",
                        "在 2D 平面點選起步點與到達點，建立 1:12 法定無障礙防滑坡道與爬升箭頭",
                    ),
                    (
                        ToolMode::PushPull,
                        "幾何推拉 (Push-Pull) [P]",
                        "選擇任意 3D 封閉平面，沿其法向量方向進行互動拉伸建模",
                    ),
                    (
                        ToolMode::Space,
                        "空間區域 (Space) [房間與坪數]",
                        "在 2D 平面點選對角劃定空間區域 (客廳/臥室/廚房/衛浴等)，自動計算地坪面積與台灣坪數",
                    ),
                    (
                        ToolMode::Furniture,
                        "室內家具 (Furniture) [軟裝設備]",
                        "放置床組、沙發、餐桌椅、廚具水槽、衛浴馬桶、衣櫃與書桌 (按 R 鍵旋轉)",
                    ),
                    (
                        ToolMode::LightFixture,
                        "照明燈具 (Lighting) [天花燈具]",
                        "在天花板安裝嵌燈、客廳吊燈、軌道投射燈、吸頂燈或燈帶，設定流明與色溫",
                    ),
                ];

                for (mode, title, desc) in tools {
                    if tool_icon_button(ui, mode, self.tool_mode, title, desc) {
                        self.tool_mode = mode;
                        self.viewport_2d.wall_start_pt = None;
                        self.viewport_2d.slab_start_pt = None;
                        self.viewport_2d.ceiling_start_pt = None;
                        self.viewport_2d.stair_start_pt = None;
                        self.viewport_2d.railing_start_pt = None;
                        self.viewport_2d.roof_start_pt = None;
                        self.viewport_2d.ramp_start_pt = None;
                        self.viewport_2d.curtain_wall_start_pt = None;
                        self.viewport_2d.space_start_pt = None;
                        self.viewport_2d.move_base_pt = None;
                    }
                }

                ui.separator();

                // Snap, Dimensions and 3D Section Cut toggles
                ui.checkbox(&mut self.viewport_2d.snap_enabled, "端點吸附 (Snap)")
                    .on_hover_text("開啟/關閉 2D 畫布的幾何端點吸附功能");
                ui.checkbox(&mut self.viewport_2d.show_dimensions, "📐 尺寸標註 (Dimensions)")
                    .on_hover_text("顯示或隱藏 2D 平面工程圖上的建築尺寸標註線與延伸界限輔助線");

                ui.separator();
                let cut_active = self.viewport_3d.settings.enable_section_cut;
                if ui.selectable_label(cut_active, "🔪 3D 剖切盒").on_hover_text("開啟/關閉即時 3D 剖面切割 (透視室內裝修與家具格局)").clicked() {
                    self.viewport_3d.settings.enable_section_cut = !self.viewport_3d.settings.enable_section_cut;
                }


                // Quick actions for selected element or clipboard
                if self.selected_id.is_some() || self.clipboard.is_some() {
                    ui.separator();
                    if self.selected_id.is_some() {
                        if ui.button("✂️ 剪下").on_hover_text("剪下選定構件 [Ctrl+X]").clicked() {
                            self.cut();
                        }
                        if ui.button("📋 複製").on_hover_text("複製選定構件 [Ctrl+C]").clicked() {
                            self.copy();
                        }
                    }
                    if ui.add_enabled(self.clipboard.is_some(), egui::Button::new("📄 貼上"))
                        .on_hover_text("貼上剪貼簿中的構件 [Ctrl+V]")
                        .clicked()
                    {
                        self.paste();
                    }
                    if let Some(id) = self.selected_id {
                        if ui.button(egui::RichText::new("🗑️ 刪除構件").color(egui::Color32::from_rgb(255, 120, 120)))
                            .on_hover_text("刪除目前選定的構件 [Delete / Backspace]")
                            .clicked()
                        {
                            self.save_undo_state();
                            self.doc.remove_element(id);
                            self.selected_id = None;
                            self.show_notify("已刪除選定構件");
                        }
                        if ui.button(egui::RichText::new("✥ 移動構件").color(egui::Color32::from_rgb(255, 215, 0)))
                            .on_hover_text("切換至移動工具進行視口平移 [M]")
                            .clicked()
                        {
                            self.tool_mode = ToolMode::Move;
                            self.viewport_2d.move_base_pt = None;
                        }
                    }
                }

                ui.separator();

                // Inspector Side Panel Toggle
                let toggle_label = if self.show_inspector { "收合數據面板 ⏵" } else { "展開數據面板 ◧" };
                if ui.button(toggle_label)
                    .on_hover_text("收合或展開右側「即時數量與成本明細表」與「構件參數檢查器」面板")
                    .clicked()
                {
                    self.show_inspector = !self.show_inspector;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((msg, time)) = &self.notification {
                        if time.elapsed().as_secs() < 5 {
                            ui.colored_label(egui::Color32::from_rgb(80, 200, 120), msg);
                        }
                    }
                });
            });
        });

        // 3. Bottom Status Bar (CAD Command line style)
        egui::TopBottomPanel::bottom("bottom_status_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(255, 215, 0), "CAD 指令:");
                let cmd_prompt = match self.tool_mode {
                    ToolMode::Wall => {
                        if self.viewport_2d.wall_start_pt.is_some() {
                            "_WALL [請在 2D 平面圖點擊終點完成牆體建立]"
                        } else {
                            "_WALL [請在 2D 平面圖點擊第一點起點]"
                        }
                    }
                    ToolMode::Select => "_SELECT [點選構件查看屬性與連動算量]",
                    ToolMode::Move => {
                        if self.viewport_2d.move_base_pt.is_some() {
                            "_MOVE [請點選移動目標位置完成平移]"
                        } else {
                            "_MOVE [請點選移動起點基準點 (Base Point)]"
                        }
                    }
                    ToolMode::Column => "_COLUMN [在 2D 平面圖點擊任意處放置 500x500mm 結構柱]",
                    ToolMode::Door => "_DOOR [移動滑鼠至牆體上預覽開洞與開啟弧線，點擊完成放置]",
                    ToolMode::Window => "_WINDOW [移動滑鼠至牆體上預覽氣密窗框與開洞，點擊完成放置]",
                    ToolMode::Slab => {
                        if self.viewport_2d.slab_start_pt.is_some() {
                            "_SLAB [請點擊對角第二點以完成樓板建立]"
                        } else {
                            "_SLAB [請在 2D 平面圖點擊第一角點起點]"
                        }
                    }
                    ToolMode::Ceiling => {
                        if self.viewport_2d.ceiling_start_pt.is_some() {
                            "_CEILING [請點擊對角第二點以完成天花板建立]"
                        } else {
                            "_CEILING [請在 2D 平面圖點擊第一角點起點]"
                        }
                    }
                    ToolMode::Stairs => {
                        if self.viewport_2d.stair_start_pt.is_some() {
                            "_STAIRS [請點擊終點以完成直通樓梯建立]"
                        } else {
                            "_STAIRS [請點擊樓梯起步點 (Bottom)]"
                        }
                    }
                    ToolMode::Railing => {
                        if self.viewport_2d.railing_start_pt.is_some() {
                            "_RAILING [請點擊終點完成欄杆扶手建立]"
                        } else {
                            "_RAILING [請點擊起點以開始繪製欄杆扶手]"
                        }
                    }
                    ToolMode::Roof => {
                        if self.viewport_2d.roof_start_pt.is_some() {
                            "_ROOF [請點擊對角第二點以完成雙坡屋頂建立]"
                        } else {
                            "_ROOF [請在 2D 平面點擊屋頂第一角點起點]"
                        }
                    }
                    ToolMode::Ramp => {
                        if self.viewport_2d.ramp_start_pt.is_some() {
                            "_RAMP [請點擊到達點以完成 1:12 無障礙坡道建立]"
                        } else {
                            "_RAMP [請點擊無障礙坡道起點 (Bottom)]"
                        }
                    }
                    ToolMode::CurtainWall => {
                        if self.viewport_2d.curtain_wall_start_pt.is_some() {
                            "_CURTAINWALL [請點擊終點以建立結構單元式玻璃幕牆]"
                        } else {
                            "_CURTAINWALL [請點擊起點以繪製幕牆系統]"
                        }
                    }
                    ToolMode::PushPull => "_PUSHPULL [點選牆體、結構柱或樓板進行互動推拉拉伸 (+500mm)]",
                    ToolMode::Space => {
                        if self.viewport_2d.space_start_pt.is_some() {
                            "_SPACE [請點選對角第二點以完成房間劃定與坪數計算]"
                        } else {
                            "_SPACE [請點選第一角點以劃定空間區域 (客廳/臥室/廚房/衛浴等)]"
                        }
                    }
                    ToolMode::Furniture => "_FURNITURE [在平面圖點擊以放置家具，按 R 鍵可旋轉 90° 方向]",
                    ToolMode::LightFixture => "_LIGHT [在平面圖點擊天花板位置以安裝照明燈具]",
                };

                ui.label(cmd_prompt);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("單位: 公釐 (mm) / 坪");
                    ui.separator();
                    ui.label(&self.status_text);
                });
            });
        });

        // 3.5 Bottom Viewport Sheets Tab Bar (CAD Sheets Tabs)
        egui::TopBottomPanel::bottom("bottom_sheets_tab_bar")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("圖紙視圖 (Sheets):")
                            .small()
                            .color(egui::Color32::from_rgb(140, 155, 175)),
                    );
                    ui.add_space(4.0);

                    // Sheet Tab 1: 2D Drafting
                    let is_2d = self.layout == ViewLayout::Drafting2DOnly;
                    let text_2d = egui::RichText::new("📑 2D 平面工程圖 (Level 1 Plan)")
                        .color(if is_2d {
                            egui::Color32::from_rgb(255, 255, 255)
                        } else {
                            egui::Color32::from_rgb(175, 185, 200)
                        });
                    if ui
                        .selectable_label(is_2d, text_2d)
                        .on_hover_text("切換至 2D 平面工程施工圖全螢幕視圖 (Sheet 1)")
                        .clicked()
                    {
                        self.layout = ViewLayout::Drafting2DOnly;
                    }

                    // Sheet Tab 2: 3D Orbit Perspective
                    let is_3d = self.layout == ViewLayout::Orbit3DOnly;
                    let text_3d = egui::RichText::new("🌐 3D 空間實體模型 (3D Orbit Perspective)")
                        .color(if is_3d {
                            egui::Color32::from_rgb(255, 255, 255)
                        } else {
                            egui::Color32::from_rgb(175, 185, 200)
                        });
                    if ui
                        .selectable_label(is_3d, text_3d)
                        .on_hover_text("切換至 3D 空間透視實體全螢幕視圖 (Sheet 2)")
                        .clicked()
                    {
                        self.layout = ViewLayout::Orbit3DOnly;
                    }

                    // Sheet Tab 3: Split 2D + 3D
                    let is_split = self.layout == ViewLayout::Split2D3D;
                    let text_split = egui::RichText::new("◫ 雙視窗並排 (Split View 2D + 3D)")
                        .color(if is_split {
                            egui::Color32::from_rgb(255, 255, 255)
                        } else {
                            egui::Color32::from_rgb(175, 185, 200)
                        });
                    if ui
                        .selectable_label(is_split, text_split)
                        .on_hover_text("同時並排檢視 2D 工程平面與 3D 空間模型 (Sheet 3)")
                        .clicked()
                    {
                        self.layout = ViewLayout::Split2D3D;
                    }

                    if ui.button("+").on_hover_text("新增圖紙視圖 (New Sheet View)...").clicked() {
                        self.notification = Some((
                            "已新增圖紙分頁 Sheet-4 (Level 2 Plan)".to_string(),
                            std::time::Instant::now(),
                        ));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("雙向同步核心 | 視圖比例 1:100 | WCS 世界座標系")
                                .small()
                                .color(egui::Color32::from_rgb(130, 140, 160)),
                        );
                    });
                });
            });

        // 4. Right Side Panel: Live QTO Schedule + Property Inspector (可由滑鼠自由拖曳調整寬度，支援快速預設按鈕，永不回彈)
        if self.show_inspector {
            let mut panel = egui::SidePanel::right("right_inspector_panel")
                .resizable(true)
                .min_width(160.0)
                .max_width(900.0)
                .default_width(self.inspector_width);

            if let Some(target_w) = self.target_inspector_width.take() {
                panel = panel.exact_width(target_w);
                self.inspector_width = target_w;
            }

            let panel_resp = panel.show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("BIM 數據檢視").strong());

                    // Quick width preset buttons so the user can easily set standard widths
                    if ui.small_button("窄 220").on_hover_text("設定面板為極簡緊湊寬度 (220px)").clicked() {
                        self.target_inspector_width = Some(220.0);
                    }
                    if ui.small_button("中 320").on_hover_text("設定面板為標準寬度 (320px)").clicked() {
                        self.target_inspector_width = Some(320.0);
                    }
                    if ui.small_button("寬 460").on_hover_text("設定面板為寬版數據檢視 (460px)").clicked() {
                        self.target_inspector_width = Some(460.0);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("隱藏 ⏵").on_hover_text("暫時隱藏此側邊面板以獲得最大視口空間").clicked() {
                            self.show_inspector = false;
                        }
                    });
                });
                ui.separator();
                InspectorPanel::render(
                    ui,
                    &mut self.doc,
                    &mut self.undo_stack,
                    &mut self.clipboard,
                    &mut self.selected_id,
                    &mut self.inspector_state,
                );
            });
            self.inspector_width = panel_resp.response.rect.width();
        }

        // 5. Central Viewport(s)
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.layout {
                ViewLayout::Split2D3D => {
                    ui.columns(2, |cols| {
                        // Left: 2D Engineering Viewport
                        cols[0].vertical(|ui| {
                            ui.label(egui::RichText::new("2D 平面工程繪圖視口 (XY 基準層)").strong());
                            self.viewport_2d.render(
                                ui,
                                &mut self.doc,
                                &mut self.undo_stack,
                                self.tool_mode,
                                &mut self.selected_id,
                                &mut self.status_text,
                            );
                        });

                        // Right: 3D Spatial Model Viewport
                        cols[1].vertical(|ui| {
                            ui.label(egui::RichText::new("3D 空間實體模型視口 (環繞軌道與開洞實體)").strong());
                            self.viewport_3d.render(ui, &self.doc, &mut self.selected_id);
                        });
                    });
                }
                ViewLayout::Drafting2DOnly => {
                    ui.label(egui::RichText::new("2D 平面工程繪圖視口 (全螢幕精確繪圖)").strong());
                    self.viewport_2d.render(
                        ui,
                        &mut self.doc,
                        &mut self.undo_stack,
                        self.tool_mode,
                        &mut self.selected_id,
                        &mut self.status_text,
                    );
                }
                ViewLayout::Orbit3DOnly => {
                    ui.label(egui::RichText::new("3D 空間實體模型視口 (全螢幕瀏覽與推拉)").strong());
                    self.viewport_3d.render(ui, &self.doc, &mut self.selected_id);
                }
            }
        });

        // 6. File Dialog Modals (Open, Export, Save As)
        self.render_file_modals(ctx);
        self.render_about_window(ctx);
        self.render_update_window(ctx);
    }
}

/// Configure CJK (Chinese, Japanese, Korean) font support for egui
pub(crate) fn setup_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Cross-platform font candidates for Traditional and Simplified Chinese
    let font_candidates = [
        // macOS
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        // Windows
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyhl.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\mingliu.ttc",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];

    let mut loaded = false;
    for path in font_candidates {
        if let Ok(font_bytes) = std::fs::read(path) {
            println!("   ✔ 成功載入 CJK 中文字型: {}", path);
            fonts.font_data.insert(
                "cjk_fallback".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_bytes)),
            );

            // Insert at index 0 for Proportional family (buttons, menus, labels)
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk_fallback".to_owned());

            // Add to Monospace family as well
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk_fallback".to_owned());

            loaded = true;
            break;
        }
    }

    if loaded {
        ctx.set_fonts(fonts);
    } else {
        eprintln!("   ⚠️ 未能找到系統中文字型，將使用預設字型。");
    }
}

/// Custom CAD Tool Vector Icon Button with rich tooltip
fn tool_icon_button(
    ui: &mut egui::Ui,
    mode: ToolMode,
    current_mode: ToolMode,
    title: &str,
    description: &str,
) -> bool {
    let is_active = mode == current_mode;
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::click());

    let bg_color = if is_active {
        egui::Color32::from_rgb(38, 92, 160) // Active blue
    } else if response.hovered() {
        egui::Color32::from_rgb(58, 63, 72) // Hover grey
    } else {
        egui::Color32::from_rgb(35, 38, 44) // Normal dark
    };

    let border_color = if is_active {
        egui::Color32::from_rgb(110, 175, 255)
    } else if response.hovered() {
        egui::Color32::from_rgb(85, 95, 110)
    } else {
        egui::Color32::from_rgb(52, 55, 62)
    };

    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        4.0,
        bg_color,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );

    draw_cad_icon(&painter, rect, mode, is_active);

    let response = response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(title).strong().size(13.5));
        ui.label(egui::RichText::new(description).size(12.0).color(egui::Color32::from_rgb(200, 210, 225)));
    });

    response.clicked()
}

/// Pure vector graphics renderer for CAD & BIM tool icons
fn draw_cad_icon(painter: &egui::Painter, rect: egui::Rect, mode: ToolMode, is_active: bool) {
    let stroke_color = if is_active {
        egui::Color32::from_rgb(255, 255, 255)
    } else {
        egui::Color32::from_rgb(215, 220, 230)
    };
    let accent_color = if is_active {
        egui::Color32::from_rgb(110, 200, 255)
    } else {
        egui::Color32::from_rgb(145, 180, 215)
    };

    match mode {
        ToolMode::Select => {
            // CAD Pointer Arrow Cursor
            let p0 = rect.min + egui::vec2(9.0, 7.0);
            let p1 = rect.min + egui::vec2(9.0, 23.0);
            let p2 = rect.min + egui::vec2(13.5, 18.5);
            let p3 = rect.min + egui::vec2(18.0, 24.0);
            let p4 = rect.min + egui::vec2(21.0, 22.0);
            let p5 = rect.min + egui::vec2(16.5, 16.5);
            let p6 = rect.min + egui::vec2(22.0, 16.5);
            let points = vec![p0, p1, p2, p3, p4, p5, p6];
            painter.add(egui::Shape::convex_polygon(
                points,
                if is_active { egui::Color32::from_rgb(80, 150, 240) } else { egui::Color32::from_rgb(175, 185, 200) },
                egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 25, 30)),
            ));
        }
        ToolMode::Move => {
            // CAD 4-directional Move Arrow Icon
            let center = rect.center();
            let r = 7.0;
            // Cross bars
            painter.line_segment([egui::pos2(center.x - r, center.y), egui::pos2(center.x + r, center.y)], egui::Stroke::new(1.8, stroke_color));
            painter.line_segment([egui::pos2(center.x, center.y - r), egui::pos2(center.x, center.y + r)], egui::Stroke::new(1.8, stroke_color));
            // 4 arrowheads
            let ah = 3.0;
            painter.line_segment([egui::pos2(center.x - r + ah, center.y - ah), egui::pos2(center.x - r, center.y)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x - r + ah, center.y + ah), egui::pos2(center.x - r, center.y)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x + r - ah, center.y - ah), egui::pos2(center.x + r, center.y)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x + r - ah, center.y + ah), egui::pos2(center.x + r, center.y)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x - ah, center.y - r + ah), egui::pos2(center.x, center.y - r)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x + ah, center.y - r + ah), egui::pos2(center.x, center.y - r)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x - ah, center.y + r - ah), egui::pos2(center.x, center.y + r)], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([egui::pos2(center.x + ah, center.y + r - ah), egui::pos2(center.x, center.y + r)], egui::Stroke::new(1.4, stroke_color));
            painter.circle_filled(center, 2.0, accent_color);
        }
        ToolMode::Wall => {
            // Two parallel walls with engineering diagonal hatching
            let top_y = rect.min.y + 10.0;
            let bot_y = rect.min.y + 22.0;
            let left_x = rect.min.x + 6.0;
            let right_x = rect.min.x + 26.0;

            let wall_rect = egui::Rect::from_min_max(
                egui::pos2(left_x, top_y),
                egui::pos2(right_x, bot_y),
            );
            painter.rect_filled(wall_rect, 1.0, egui::Color32::from_rgba_unmultiplied(120, 150, 180, 60));

            painter.line_segment([egui::pos2(left_x, top_y), egui::pos2(right_x, top_y)], egui::Stroke::new(2.0, stroke_color));
            painter.line_segment([egui::pos2(left_x, bot_y), egui::pos2(right_x, bot_y)], egui::Stroke::new(2.0, stroke_color));
            painter.line_segment([egui::pos2(left_x, top_y), egui::pos2(left_x, bot_y)], egui::Stroke::new(1.5, stroke_color));
            painter.line_segment([egui::pos2(right_x, top_y), egui::pos2(right_x, bot_y)], egui::Stroke::new(1.5, stroke_color));

            let hatch_stroke = egui::Stroke::new(1.0, accent_color);
            painter.line_segment([egui::pos2(left_x + 4.0, bot_y), egui::pos2(left_x + 9.0, top_y)], hatch_stroke);
            painter.line_segment([egui::pos2(left_x + 10.0, bot_y), egui::pos2(left_x + 15.0, top_y)], hatch_stroke);
            painter.line_segment([egui::pos2(left_x + 16.0, bot_y), egui::pos2(left_x + 21.0, top_y)], hatch_stroke);
        }
        ToolMode::Door => {
            // Architectural door opening & 90-degree swing arc
            let hinge = rect.min + egui::vec2(8.0, 23.0);
            let frame_right = rect.min + egui::vec2(24.0, 23.0);
            let door_open = rect.min + egui::vec2(8.0, 8.0);

            painter.line_segment([hinge, frame_right], egui::Stroke::new(1.2, egui::Color32::from_rgb(130, 130, 130)));
            painter.line_segment([hinge, door_open], egui::Stroke::new(2.2, stroke_color));

            let radius = 15.0;
            let segments = 8;
            for i in 0..segments {
                let a0 = -std::f32::consts::FRAC_PI_2 * (i as f32 / segments as f32);
                let a1 = -std::f32::consts::FRAC_PI_2 * ((i + 1) as f32 / segments as f32);
                let p0 = hinge + egui::vec2(radius * a0.cos(), radius * a0.sin());
                let p1 = hinge + egui::vec2(radius * a1.cos(), radius * a1.sin());
                painter.line_segment([p0, p1], egui::Stroke::new(1.2, accent_color));
            }
        }
        ToolMode::Window => {
            // Architectural window symbol with glass panes
            let win_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(6.0, 9.0),
                rect.min + egui::vec2(26.0, 23.0),
            );
            painter.rect_filled(win_rect, 1.0, egui::Color32::from_rgba_unmultiplied(80, 180, 240, 50));
            painter.rect_stroke(win_rect, 1.0, egui::Stroke::new(1.8, stroke_color), egui::StrokeKind::Inside);

            let mid_x = (win_rect.min.x + win_rect.max.x) * 0.5;
            let mid_y = (win_rect.min.y + win_rect.max.y) * 0.5;
            painter.line_segment([egui::pos2(mid_x, win_rect.min.y), egui::pos2(mid_x, win_rect.max.y)], egui::Stroke::new(1.2, accent_color));
            painter.line_segment([egui::pos2(win_rect.min.x, mid_y), egui::pos2(win_rect.max.x, mid_y)], egui::Stroke::new(1.2, accent_color));
        }
        ToolMode::Column => {
            // Structural column: square with diagonal cross
            let col_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(8.0, 8.0),
                rect.min + egui::vec2(24.0, 24.0),
            );
            painter.rect_filled(col_rect, 1.0, egui::Color32::from_rgba_unmultiplied(160, 160, 160, 40));
            painter.rect_stroke(col_rect, 1.0, egui::Stroke::new(1.8, stroke_color), egui::StrokeKind::Inside);

            painter.line_segment([col_rect.min, col_rect.max], egui::Stroke::new(1.4, accent_color));
            painter.line_segment(
                [egui::pos2(col_rect.min.x, col_rect.max.y), egui::pos2(col_rect.max.x, col_rect.min.y)],
                egui::Stroke::new(1.4, accent_color),
            );
        }
        ToolMode::Slab => {
            // Isometric structural floor slab
            let t0 = rect.min + egui::vec2(16.0, 9.0);
            let t1 = rect.min + egui::vec2(25.0, 13.0);
            let t2 = rect.min + egui::vec2(16.0, 17.0);
            let t3 = rect.min + egui::vec2(7.0, 13.0);
            painter.add(egui::Shape::convex_polygon(
                vec![t0, t1, t2, t3],
                egui::Color32::from_rgba_unmultiplied(80, 160, 240, 70),
                egui::Stroke::new(1.4, stroke_color),
            ));
            let f0 = t3;
            let f1 = t2;
            let f2 = t2 + egui::vec2(0.0, 5.0);
            let f3 = t3 + egui::vec2(0.0, 5.0);
            painter.add(egui::Shape::convex_polygon(
                vec![f0, f1, f2, f3],
                egui::Color32::from_rgba_unmultiplied(60, 130, 210, 120),
                egui::Stroke::new(1.2, stroke_color),
            ));
            let r0 = t2;
            let r1 = t1;
            let r2 = t1 + egui::vec2(0.0, 5.0);
            let r3 = t2 + egui::vec2(0.0, 5.0);
            painter.add(egui::Shape::convex_polygon(
                vec![r0, r1, r2, r3],
                egui::Color32::from_rgba_unmultiplied(50, 110, 180, 140),
                egui::Stroke::new(1.2, stroke_color),
            ));
        }
        ToolMode::Ceiling => {
            // Suspended ceiling tile grid
            let c_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(6.0, 8.0),
                rect.min + egui::vec2(26.0, 24.0),
            );
            painter.rect_filled(c_rect, 1.0, egui::Color32::from_rgba_unmultiplied(240, 220, 100, 40));
            painter.rect_stroke(c_rect, 1.0, egui::Stroke::new(1.6, stroke_color), egui::StrokeKind::Inside);

            let mid_x = (c_rect.min.x + c_rect.max.x) * 0.5;
            let mid_y = (c_rect.min.y + c_rect.max.y) * 0.5;
            painter.line_segment([egui::pos2(mid_x, c_rect.min.y), egui::pos2(mid_x, c_rect.max.y)], egui::Stroke::new(1.2, accent_color));
            painter.line_segment([egui::pos2(c_rect.min.x, mid_y), egui::pos2(c_rect.max.x, mid_y)], egui::Stroke::new(1.2, accent_color));
            // Hanger rods
            painter.line_segment([egui::pos2(mid_x, c_rect.min.y), egui::pos2(mid_x, rect.min.y + 4.0)], egui::Stroke::new(1.0, stroke_color));
            painter.line_segment([egui::pos2(c_rect.min.x + 3.0, c_rect.min.y), egui::pos2(c_rect.min.x + 3.0, rect.min.y + 4.0)], egui::Stroke::new(1.0, stroke_color));
            painter.line_segment([egui::pos2(c_rect.max.x - 3.0, c_rect.min.y), egui::pos2(c_rect.max.x - 3.0, rect.min.y + 4.0)], egui::Stroke::new(1.0, stroke_color));
        }
        ToolMode::Stairs => {
            // 3-step staircase profile
            let p0 = rect.min + egui::vec2(7.0, 24.0);
            let p1 = rect.min + egui::vec2(13.0, 24.0);
            let p2 = rect.min + egui::vec2(13.0, 18.0);
            let p3 = rect.min + egui::vec2(19.0, 18.0);
            let p4 = rect.min + egui::vec2(19.0, 12.0);
            let p5 = rect.min + egui::vec2(25.0, 12.0);
            let p6 = rect.min + egui::vec2(25.0, 24.0);
            painter.add(egui::Shape::convex_polygon(
                vec![p0, p1, p2, p3, p4, p5, p6],
                egui::Color32::from_rgba_unmultiplied(255, 140, 80, 50),
                egui::Stroke::new(1.6, stroke_color),
            ));
            let arrow_start = rect.min + egui::vec2(9.0, 20.0);
            let arrow_end = rect.min + egui::vec2(22.0, 8.0);
            painter.line_segment([arrow_start, arrow_end], egui::Stroke::new(1.8, accent_color));
            painter.line_segment([arrow_end, rect.min + egui::vec2(17.0, 9.0)], egui::Stroke::new(1.8, accent_color));
            painter.line_segment([arrow_end, rect.min + egui::vec2(21.0, 13.0)], egui::Stroke::new(1.8, accent_color));
        }
        ToolMode::Railing => {
            // Architectural handrail & glass balustrade vector icon
            let r0 = rect.min + egui::vec2(6.0, 18.0);
            let r1 = rect.min + egui::vec2(26.0, 10.0);
            painter.line_segment([r0, r1], egui::Stroke::new(2.4, stroke_color));

            let b0 = rect.min + egui::vec2(6.0, 24.0);
            let b1 = rect.min + egui::vec2(26.0, 16.0);
            painter.line_segment([b0, b1], egui::Stroke::new(1.2, stroke_color));

            painter.add(egui::Shape::convex_polygon(
                vec![r0, r1, b1, b0],
                egui::Color32::from_rgba_unmultiplied(100, 210, 255, 60),
                egui::Stroke::NONE,
            ));

            painter.line_segment([rect.min + egui::vec2(8.0, 17.0), rect.min + egui::vec2(8.0, 25.0)], egui::Stroke::new(1.6, accent_color));
            painter.line_segment([rect.min + egui::vec2(16.0, 14.0), rect.min + egui::vec2(16.0, 22.0)], egui::Stroke::new(1.6, accent_color));
            painter.line_segment([rect.min + egui::vec2(24.0, 11.0), rect.min + egui::vec2(24.0, 19.0)], egui::Stroke::new(1.6, accent_color));
        }
        ToolMode::CurtainWall => {
            // High-tech glazed curtain wall with aluminum mullion grid
            let c_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(6.0, 8.0),
                rect.min + egui::vec2(26.0, 24.0),
            );
            painter.rect_filled(c_rect, 1.0, egui::Color32::from_rgba_unmultiplied(100, 210, 255, 50));
            painter.rect_stroke(c_rect, 1.0, egui::Stroke::new(1.8, stroke_color), egui::StrokeKind::Inside);

            let m1_x = rect.min.x + 12.5;
            let m2_x = rect.min.x + 19.5;
            let h_y = rect.min.y + 16.0;

            painter.line_segment([egui::pos2(m1_x, c_rect.min.y), egui::pos2(m1_x, c_rect.max.y)], egui::Stroke::new(1.4, accent_color));
            painter.line_segment([egui::pos2(m2_x, c_rect.min.y), egui::pos2(m2_x, c_rect.max.y)], egui::Stroke::new(1.4, accent_color));
            painter.line_segment([egui::pos2(c_rect.min.x, h_y), egui::pos2(c_rect.max.x, h_y)], egui::Stroke::new(1.2, stroke_color));

            // Glass glare reflection tick
            painter.line_segment([rect.min + egui::vec2(8.0, 14.0), rect.min + egui::vec2(11.0, 10.0)], egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180)));
        }
        ToolMode::Roof => {
            // Architectural Gable Roof with ridge peak and eaves overhang
            let peak = rect.min + egui::vec2(16.0, 7.0);
            let eave_left = rect.min + egui::vec2(5.0, 18.0);
            let eave_right = rect.min + egui::vec2(27.0, 18.0);
            let bot_left = rect.min + egui::vec2(8.0, 24.0);
            let bot_right = rect.min + egui::vec2(24.0, 24.0);

            // Roof slope fill
            painter.add(egui::Shape::convex_polygon(
                vec![peak, eave_right, eave_left],
                egui::Color32::from_rgba_unmultiplied(255, 130, 80, 70),
                egui::Stroke::new(2.2, stroke_color),
            ));
            // Attic / Wall base
            painter.line_segment([eave_left, bot_left], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([eave_right, bot_right], egui::Stroke::new(1.4, stroke_color));
            painter.line_segment([bot_left, bot_right], egui::Stroke::new(1.4, stroke_color));

            // Ridge marker
            painter.circle_filled(peak, 2.2, accent_color);
        }
        ToolMode::Ramp => {
            // Accessible ramp wedge with climbing direction arrow
            let r0 = rect.min + egui::vec2(6.0, 23.0);
            let r1 = rect.min + egui::vec2(26.0, 12.0);
            let r2 = rect.min + egui::vec2(26.0, 23.0);

            painter.add(egui::Shape::convex_polygon(
                vec![r0, r1, r2],
                egui::Color32::from_rgba_unmultiplied(100, 200, 255, 60),
                egui::Stroke::new(1.8, stroke_color),
            ));

            // Climbing arrow
            let arrow_start = rect.min + egui::vec2(9.0, 21.0);
            let arrow_end = rect.min + egui::vec2(23.0, 14.0);
            painter.line_segment([arrow_start, arrow_end], egui::Stroke::new(2.0, accent_color));
            painter.line_segment([arrow_end, rect.min + egui::vec2(18.0, 14.0)], egui::Stroke::new(2.0, accent_color));
            painter.line_segment([arrow_end, rect.min + egui::vec2(21.0, 18.0)], egui::Stroke::new(2.0, accent_color));
        }
        ToolMode::PushPull => {
            // Isometric face with an upward extrusion arrow
            let b0 = rect.min + egui::vec2(16.0, 18.0);
            let b1 = rect.min + egui::vec2(25.0, 21.0);
            let b2 = rect.min + egui::vec2(16.0, 24.0);
            let b3 = rect.min + egui::vec2(7.0, 21.0);
            painter.add(egui::Shape::convex_polygon(
                vec![b0, b1, b2, b3],
                egui::Color32::from_rgba_unmultiplied(100, 200, 255, 60),
                egui::Stroke::new(1.4, stroke_color),
            ));

            let arrow_base = rect.min + egui::vec2(16.0, 21.0);
            let arrow_tip = rect.min + egui::vec2(16.0, 7.0);
            painter.line_segment([arrow_base, arrow_tip], egui::Stroke::new(2.2, accent_color));
            painter.line_segment([arrow_tip, rect.min + egui::vec2(12.0, 11.5)], egui::Stroke::new(2.2, accent_color));
            painter.line_segment([arrow_tip, rect.min + egui::vec2(20.0, 11.5)], egui::Stroke::new(2.2, accent_color));
        }
        ToolMode::Space => {
            // Architectural Room Space boundary with central crosshair
            let s_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(6.0, 7.0),
                rect.min + egui::vec2(26.0, 25.0),
            );
            painter.rect_filled(s_rect, 2.0, egui::Color32::from_rgba_unmultiplied(80, 160, 240, 50));
            painter.rect_stroke(s_rect, 2.0, egui::Stroke::new(1.8, stroke_color), egui::StrokeKind::Inside);
            let c = s_rect.center();
            painter.line_segment([egui::pos2(c.x - 4.0, c.y), egui::pos2(c.x + 4.0, c.y)], egui::Stroke::new(1.5, accent_color));
            painter.line_segment([egui::pos2(c.x, c.y - 4.0), egui::pos2(c.x, c.y + 4.0)], egui::Stroke::new(1.5, accent_color));
        }
        ToolMode::Furniture => {
            // Armchair / Sofa icon with backrest, cushions and armrests
            let base = egui::Rect::from_min_max(
                rect.min + egui::vec2(7.0, 13.0),
                rect.min + egui::vec2(25.0, 23.0),
            );
            painter.rect_filled(base, 2.0, egui::Color32::from_rgba_unmultiplied(220, 150, 80, 60));
            painter.rect_stroke(base, 2.0, egui::Stroke::new(1.8, stroke_color), egui::StrokeKind::Inside);

            let back = egui::Rect::from_min_max(
                rect.min + egui::vec2(7.0, 9.0),
                rect.min + egui::vec2(25.0, 14.0),
            );
            painter.rect_filled(back, 2.0, egui::Color32::from_rgba_unmultiplied(240, 170, 90, 80));
            painter.rect_stroke(back, 2.0, egui::Stroke::new(1.6, stroke_color), egui::StrokeKind::Inside);

            painter.rect_filled(egui::Rect::from_min_max(rect.min + egui::vec2(5.5, 11.0), rect.min + egui::vec2(8.5, 24.0)), 1.5, accent_color);
            painter.rect_filled(egui::Rect::from_min_max(rect.min + egui::vec2(23.5, 11.0), rect.min + egui::vec2(26.5, 24.0)), 1.5, accent_color);
        }
        ToolMode::LightFixture => {
            // Ceiling luminaire pendant with glowing light cone rays
            let top_pt = rect.min + egui::vec2(16.0, 7.0);
            let lamp_c = rect.min + egui::vec2(16.0, 13.0);
            painter.line_segment([top_pt, lamp_c], egui::Stroke::new(1.4, stroke_color));
            painter.circle_filled(lamp_c, 4.0, egui::Color32::from_rgb(255, 215, 0));
            painter.circle_stroke(lamp_c, 4.0, egui::Stroke::new(1.5, stroke_color));

            let r_left = rect.min + egui::vec2(8.0, 24.0);
            let r_mid = rect.min + egui::vec2(16.0, 25.0);
            let r_right = rect.min + egui::vec2(24.0, 24.0);
            painter.line_segment([lamp_c, r_left], egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(255, 220, 80, 180)));
            painter.line_segment([lamp_c, r_mid], egui::Stroke::new(1.4, egui::Color32::from_rgba_unmultiplied(255, 220, 80, 220)));
            painter.line_segment([lamp_c, r_right], egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(255, 220, 80, 180)));
        }

    }
}
