use anyhow::Result;
use sketchcad_bim::{Beam, BimDocument, Column, Door, Material, Wall, Window};
use sketchcad_core::{Point2D, Point3D};
use sketchcad_geom::OpeningVoid;
use sketchcad_io::{
    BcfHandler, BcfTopic, DwgAdapter, DwgVersion, DxfHandler, IfcExporter,
};
use std::fs;
use std::path::Path;

/// Generates a rich, realistic sample Architectural CAD & BIM project:
/// "Modern Two-Bedroom Villa (現代兩房度假別墅)"
pub fn generate_all_samples() -> Result<()> {
    let out_dir = Path::new("samples");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir)?;
    }

    let mut doc = BimDocument::new("Modern Architectural Villa 現代度假別墅");

    let concrete = Material::concrete();
    let brick = Material::brick();
    let wood = Material::wood();
    let glass = Material::glass();
    let steel = Material::structural_steel();

    // 0. Ground Floor Slab (12m x 8m, 200mm thickness)
    doc.insert_slab(
        Point2D::new(0.0, 0.0),
        12000.0,
        8000.0,
        200.0,
        0.0,
        concrete.clone(),
    );

    // Suspended Ceiling in Living Area (5.5m x 8m, elevation 3000mm)
    doc.insert_ceiling(
        Point2D::new(0.0, 0.0),
        5500.0,
        8000.0,
        3000.0,
        Material::gypsum(),
    );

    // Interior Straight Run Stairs (1m wide, 18 steps, total rise 3600mm)
    doc.insert_stairs(
        Point2D::new(500.0, 4500.0),
        Point2D::new(500.0, 7500.0),
        1000.0,
        3600.0,
        18,
        concrete.clone(),
    );

    // 1. Exterior Walls (12m x 8m perimeter, 200mm thickness, 3600mm height)
    // South Wall (12m long, front entrance)
    let mut wall_south = Wall::new(
        Point2D::new(0.0, 0.0),
        Point2D::new(12000.0, 0.0),
        200.0,
        3600.0,
        concrete.clone(),
    );
    // Main entrance door void
    wall_south.add_opening(OpeningVoid {
        offset_along_wall: 2000.0,
        width: 1200.0,
        base_z: 0.0,
        height: 2200.0,
    });
    // Living room panoramic window void
    wall_south.add_opening(OpeningVoid {
        offset_along_wall: 6000.0,
        width: 3000.0,
        base_z: 500.0,
        height: 2000.0,
    });
    let wall_south_id = doc.add_element(wall_south);

    // East Wall (8m long)
    let mut wall_east = Wall::new(
        Point2D::new(12000.0, 0.0),
        Point2D::new(12000.0, 8000.0),
        200.0,
        3600.0,
        concrete.clone(),
    );
    wall_east.add_opening(OpeningVoid {
        offset_along_wall: 3000.0,
        width: 1800.0,
        base_z: 900.0,
        height: 1400.0,
    });
    let wall_east_id = doc.add_element(wall_east);

    // North Wall (12m long)
    let mut wall_north = Wall::new(
        Point2D::new(12000.0, 8000.0),
        Point2D::new(0.0, 8000.0),
        200.0,
        3600.0,
        concrete.clone(),
    );
    wall_north.add_opening(OpeningVoid {
        offset_along_wall: 4000.0,
        width: 2000.0,
        base_z: 900.0,
        height: 1400.0,
    });
    let wall_north_id = doc.add_element(wall_north);

    // West Wall (8m long)
    let wall_west = Wall::new(
        Point2D::new(0.0, 8000.0),
        Point2D::new(0.0, 0.0),
        200.0,
        3600.0,
        concrete.clone(),
    );
    let _wall_west_id = doc.add_element(wall_west);

    // Interior Partition Wall (Dividing Living Space and Bedrooms, Red Brick 150mm)
    let mut wall_interior = Wall::new(
        Point2D::new(5500.0, 0.0),
        Point2D::new(5500.0, 8000.0),
        150.0,
        3600.0,
        brick,
    );
    wall_interior.add_opening(OpeningVoid {
        offset_along_wall: 2500.0,
        width: 900.0,
        base_z: 0.0,
        height: 2100.0,
    });
    let wall_interior_id = doc.add_element(wall_interior);

    // 2. Structural Columns (500x500mm at 4 exterior corners and center)
    let corners = [
        Point2D::new(0.0, 0.0),
        Point2D::new(12000.0, 0.0),
        Point2D::new(12000.0, 8000.0),
        Point2D::new(0.0, 8000.0),
        Point2D::new(5500.0, 4000.0),
    ];
    for p in corners {
        doc.add_element(Column::new(p, 500.0, 500.0, 3600.0, concrete.clone()));
    }

    // 3. Structural Steel Roof Beams (spanning east-west)
    doc.add_element(Beam::new(
        Point3D::new(0.0, 4000.0, 3600.0),
        Point3D::new(12000.0, 4000.0, 3600.0),
        350.0,
        700.0,
        steel,
    ));

    // 4. Doors
    // Main Entrance Door (Double leaf grand door)
    let mut entrance_door = Door::new(wall_south_id, 2000.0, 1200.0, 2200.0, wood.clone(), 18500.0);
    entrance_door.set_transform(Point2D::new(2000.0, 0.0), sketchcad_core::Vector2D::new(1.0, 0.0));
    let entrance_door_id = doc.add_element(entrance_door);

    // Bedroom Door
    let mut bedroom_door = Door::new(wall_interior_id, 2500.0, 900.0, 2100.0, wood, 9200.0);
    bedroom_door.set_transform(Point2D::new(5500.0, 2500.0), sketchcad_core::Vector2D::new(0.0, 1.0));
    let _bedroom_door_id = doc.add_element(bedroom_door);

    // 5. Windows
    // Living Room Panoramic Window
    let mut living_win = Window::new(wall_south_id, 6000.0, 500.0, 3000.0, 2000.0, glass.clone(), 36000.0);
    living_win.set_transform(Point2D::new(6000.0, 0.0), sketchcad_core::Vector2D::new(1.0, 0.0));
    let _living_win_id = doc.add_element(living_win);

    // East Dining Window
    let mut dining_win = Window::new(wall_east_id, 3000.0, 900.0, 1800.0, 1400.0, glass.clone(), 16800.0);
    dining_win.set_transform(Point2D::new(12000.0, 3000.0), sketchcad_core::Vector2D::new(0.0, 1.0));
    let _dining_win_id = doc.add_element(dining_win);

    // North Master Bedroom Window
    let mut bedroom_win = Window::new(wall_north_id, 4000.0, 900.0, 2000.0, 1400.0, glass, 18200.0);
    bedroom_win.set_transform(Point2D::new(8000.0, 8000.0), sketchcad_core::Vector2D::new(-1.0, 0.0));
    let _bedroom_win_id = doc.add_element(bedroom_win);

    doc.sync();

    // 6. Generate and save the 4 CAD/BIM sample files:
    // A. OpenBIM IFC4 (*.ifc)
    let ifc_path = out_dir.join("sample_villa.ifc");
    IfcExporter::export_to_ifc4(&doc, &ifc_path)?;
    println!("   ✔ 產出 IFC4 建築資訊模型: {}", ifc_path.display());

    // B. Standard DXF (*.dxf)
    let dxf_path = out_dir.join("sample_villa.dxf");
    DxfHandler::export_document_2d(&doc, &dxf_path)?;
    println!("   ✔ 產出 DXF 向量工程平面圖: {}", dxf_path.display());

    // C. Native DWG (*.dwg)
    let dwg_path = out_dir.join("sample_villa.dwg");
    let polylines = doc.generate_2d_drawing();
    let dwg_bytes = DwgAdapter::create_native_dwg(DwgVersion::Ac2018, &polylines);
    fs::write(&dwg_path, dwg_bytes)?;
    println!("   ✔ 產出原生二進制 DWG 圖檔: {}", dwg_path.display());

    // D. BIM Collaboration BCF 2.1 (*.bcfzip)
    let bcf_path = out_dir.join("sample_villa.bcfzip");
    let mut topic = BcfTopic::new(
        "別墅主入口雨遮與結構樑淨高檢討",
        "請建築師與結構技師確認南向大門 1200x2200mm 上方過樑與主樑淨高是否滿足 2.4 公尺法規需求。",
    );
    topic.components.push(entrance_door_id);
    topic.components.push(wall_south_id);
    topic.camera_pos = Point3D::new(2000.0, -6000.0, 2500.0);
    topic.camera_dir = sketchcad_core::Vector3D::new(0.0, 1.0, -0.2).normalize();
    BcfHandler::export_bcf_package(&[topic], &bcf_path)?;
    println!("   ✔ 產出 BIM 議題協同封包: {}", bcf_path.display());

    // E. Native SketchCAD Project (*.skc)
    let skc_path = out_dir.join("sample_villa.skc");
    doc.save_to_file(&skc_path)?;
    println!("   ✔ 產出 SketchCAD 原生工程專案: {}", skc_path.display());

    Ok(())
}
