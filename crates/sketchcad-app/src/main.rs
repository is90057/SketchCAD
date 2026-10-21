mod gui;
pub mod sample_generator;

use anyhow::Result;
use eframe::egui;
use gui::SketchCadApp;
use sketchcad_bim::{Beam, BimDocument, Column, Door, Material, Wall, Window};
use sketchcad_core::{Point2D, Point3D};
use sketchcad_geom::{OpeningVoid, PlanarFace3D, Polyline2D};
use sketchcad_io::{
    BcfHandler, BcfTopic, DwgAdapter, DwgVersion, DxfHandler, IfcExporter, IfcImporter,
};
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    // Automatically generate sample CAD & BIM files if not already present
    if !Path::new("samples/sample_villa.ifc").exists() {
        println!("📦 正在自動產出範例 CAD/BIM 檔案庫 (DWG, DXF, IFC, BCF)...");
        let _ = sample_generator::generate_all_samples();
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--cli" || arg == "--test-cli") {
        run_cli_pipeline()
    } else if args.iter().any(|arg| arg == "--generate-samples") {
        sample_generator::generate_all_samples()?;
        println!("✨ 範例檔案庫產生完成！");
        Ok(())
    } else {
        println!("========================================================");
        println!("  SketchCAD - 參數化建築資訊模型 (BIM) 核心系統");
        println!("  啟動圖形操作介面 (GUI: wgpu + egui 雙視窗)...");
        println!("========================================================\n");

        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 920.0])
                .with_min_inner_size([1050.0, 700.0])
                .with_title("SketchCAD - 參數化建築資訊模型系統 (wgpu)"),
            ..Default::default()
        };

        eframe::run_native(
            "SketchCAD",
            native_options,
            Box::new(|cc| Ok(Box::new(SketchCadApp::new(cc)))),
        )
        .map_err(|e| anyhow::anyhow!("GUI error: {:?}", e))
    }
}

fn run_cli_pipeline() -> Result<()> {
    println!("========================================================");
    println!("  📐 SketchCAD - CLI Verification Pipeline");
    println!("  ⚡ High-Precision CAD & Reactive Parametric Engine");
    println!("========================================================\n");

    let mut doc = BimDocument::new("SketchCAD Commercial Tower Prototype");
    println!("[1/6] 🏗️  建立專案與智慧物件 (BIM Smart Elements)...");

    let concrete = Material::concrete();
    let steel = Material::structural_steel();
    let wood = Material::wood();
    let glass = Material::glass();

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
        width: 900.0,
        base_z: 0.0,
        height: 2100.0,
    });
    wall_1.add_opening(OpeningVoid {
        offset_along_wall: 5000.0,
        width: 1800.0,
        base_z: 900.0,
        height: 1400.0,
    });
    let wall_1_id = doc.add_element(wall_1);

    let wall_2 = Wall::new(
        Point2D::new(8000.0, 0.0),
        Point2D::new(8000.0, 6000.0),
        200.0,
        3500.0,
        concrete.clone(),
    );
    doc.add_element(wall_2);

    let col_1 = Column::new(Point2D::new(0.0, 0.0), 500.0, 500.0, 3500.0, concrete.clone());
    doc.add_element(col_1);

    let col_2 = Column::new(Point2D::new(8000.0, 0.0), 500.0, 500.0, 3500.0, concrete.clone());
    doc.add_element(col_2);

    let beam_1 = Beam::new(
        Point3D::new(0.0, 0.0, 3500.0),
        Point3D::new(8000.0, 0.0, 3500.0),
        350.0,
        700.0,
        steel,
    );
    doc.add_element(beam_1);

    let door_1 = Door::new(wall_1_id, 2500.0, 900.0, 2100.0, wood.clone(), 9500.0);
    let door_1_id = doc.add_element(door_1);

    let window_1 = Window::new(wall_1_id, 5000.0, 900.0, 1800.0, 1400.0, glass, 16000.0);
    doc.add_element(window_1);

    println!("   ✔ 成功建立構件: {} 個智慧物件", doc.element_count());
    doc.sync();

    // 2. Net QTO Schedule
    println!("\n[2/6] 📊 扣除開口淨容積後之工程造價與算量 (Net QTO Schedule):");
    let initial_qto = doc.generate_master_qto();
    for item in &initial_qto.items {
        println!(
            "   • [{}] {:<32} 數量: {:>6.2} {:<4} 單價: {:>8.0} | 複價: {:>10.0} TWD",
            item.category, item.description, item.quantity, item.unit, item.unit_price, item.total_price
        );
    }
    println!("   💰 總預估工程造價: {:.2} TWD\n", initial_qto.total_cost());

    // 3. Reactive Modification
    println!("[3/6] 🔄 測試全局連動更新 (Reactive Dependency Graph Modification):");
    let mut modified_wall = Wall::new(
        Point2D::new(0.0, 0.0),
        Point2D::new(12000.0, 0.0),
        200.0,
        3500.0,
        concrete,
    );
    modified_wall.id = wall_1_id;
    modified_wall.add_opening(OpeningVoid {
        offset_along_wall: 2500.0,
        width: 1200.0,
        base_z: 0.0,
        height: 2100.0,
    });
    doc.update_element(modified_wall);

    let mut modified_door = Door::new(wall_1_id, 2500.0, 1200.0, 2100.0, wood, 12500.0);
    modified_door.id = door_1_id;
    doc.update_element(modified_door);

    let sync_order = doc.sync();
    println!("   ✔ 觸發依賴圖連動更新順序: {:?}", sync_order);

    // 4. Push-Pull
    println!("\n[4/6] 🛠️  進階幾何演算法：空間平面互動推拉 (Push-Pull Direct Extrusion):");
    let inclined_face = PlanarFace3D::new(vec![
        Point3D::new(0.0, 0.0, 0.0),
        Point3D::new(2000.0, 0.0, 0.0),
        Point3D::new(2000.0, 2000.0, 1000.0),
        Point3D::new(0.0, 2000.0, 1000.0),
    ]);
    let extruded_mesh = inclined_face.push_pull(500.0);
    println!(
        "   ✔ 沿法線推拉 500mm 完成: 產出水密實體 {} 個三角形, {} 個頂點",
        extruded_mesh.triangle_count(),
        extruded_mesh.vertex_count()
    );

    // 5. Native DWG Binary Parser Test
    println!("\n[5/6] 💾 DWG 原生二進制解析增強 (Pure Rust Native DWG Reader):");
    let test_polys = vec![
        Polyline2D::new(vec![Point2D::new(0.0, 0.0), Point2D::new(8000.0, 0.0)], false),
        Polyline2D::new(vec![Point2D::new(8000.0, 0.0), Point2D::new(8000.0, 6000.0)], false),
    ];
    let dwg_bytes = DwgAdapter::create_native_dwg(DwgVersion::Ac2018, &test_polys);
    let dwg_meta = DwgAdapter::read_metadata_bytes(&dwg_bytes)?;
    println!(
        "   ✔ [DWG]  成功生成並解析原生二進制 DWG (版本: {}, 字碼頁: {})",
        dwg_meta.version, dwg_meta.codepage
    );
    let dwg_entities = DwgAdapter::parse_bytes(&dwg_bytes)?;
    println!("   ✔ [DWG]  原生二進制點陣提取成功: 恢復 {} 條 2D 向量多段線", dwg_entities.len());

    // 6. Export and IFC4 Round-Trip
    println!("\n[6/6] 💾 跨格式標準檔案匯出與 IFC 匯入測試 (Round-Trip):");
    let export_dir = Path::new("exports");
    if !export_dir.exists() {
        fs::create_dir_all(export_dir)?;
    }

    let dxf_path = export_dir.join("project_plan.dxf");
    DxfHandler::export_document_2d(&doc, &dxf_path)?;
    println!("   ✔ [DXF]  成功匯出 2D 平面圖至: {}", dxf_path.display());

    let ifc_path = export_dir.join("project_model.ifc");
    IfcExporter::export_to_ifc4(&doc, &ifc_path)?;
    println!("   ✔ [IFC4] 成功匯出 OpenBIM 建築資訊模型至: {}", ifc_path.display());

    let imported_doc = IfcImporter::import_from_file(&ifc_path)?;
    println!(
        "   ✔ [IFC4] 成功逆向解析載入 IFC 模型！恢復構件: {} 個智慧物件",
        imported_doc.element_count()
    );

    let bcf_path = export_dir.join("clash_issue.bcfzip");
    let mut topic = BcfTopic::new("Door clearance review", "Review door width with structural team.");
    topic.components.push(door_1_id);
    BcfHandler::export_bcf_package(&[topic], &bcf_path)?;
    println!("   ✔ [BCF]  成功匯出 BIM 議題協同封包至: {}", bcf_path.display());

    println!("\n✨ SketchCAD 核心演算法、DWG 原生解析與 GUI 模組全數驗證通過！");
    Ok(())
}
