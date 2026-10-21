use chrono::Utc;
use sketchcad_bim::{BimDocument, ElementKind};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// IFC (Industry Foundation Classes) OpenBIM STEP ISO-10303-21 Generator
pub struct IfcExporter;

impl IfcExporter {
    /// Export the entire BIM Document into an IFC4 STEP physical file (.ifc)
    pub fn export_to_ifc4<P: AsRef<Path>>(doc: &BimDocument, path: P) -> io::Result<()> {
        let mut file = File::create(path)?;
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

        writeln!(file, "ISO-10303-21;")?;
        writeln!(file, "HEADER;")?;
        writeln!(
            file,
            "FILE_DESCRIPTION(('ViewDefinition [CoordinationView_V2.0]'), '2;1');"
        )?;
        writeln!(
            file,
            "FILE_NAME('{}', '{}', ('SketchCAD User'), ('SketchCAD Engine'), 'SketchCAD {}', 'SketchCAD OpenBIM Kernel', '');",
            doc.name, timestamp, env!("CARGO_PKG_VERSION")
        )?;
        writeln!(file, "FILE_SCHEMA(('IFC4'));")?;
        writeln!(file, "ENDSEC;")?;
        writeln!(file, "DATA;")?;

        let mut step_id = 1;

        // Base Context & Units
        let project_id = step_id;
        step_id += 1;
        let site_id = step_id;
        step_id += 1;
        let building_id = step_id;
        step_id += 1;
        let storey_id = step_id;
        step_id += 1;

        // Write Project Structure
        writeln!(
            file,
            "#{}=IFCPROJECT('{}', $, '{}', 'SketchCAD BIM Model', $, $, $, $, $);",
            project_id,
            sketchcad_core::ElementId::new().to_ifc_guid(),
            doc.name
        )?;
        writeln!(
            file,
            "#{}=IFCSITE('{}', $, 'Default Site', $, $, $, $, $, .ELEMENT., $, $, $, $, $);",
            site_id,
            sketchcad_core::ElementId::new().to_ifc_guid()
        )?;
        writeln!(
            file,
            "#{}=IFCBUILDING('{}', $, 'Building 1', $, $, $, $, $, .ELEMENT., $, $, $);",
            building_id,
            sketchcad_core::ElementId::new().to_ifc_guid()
        )?;
        writeln!(
            file,
            "#{}=IFCBUILDINGSTOREY('{}', $, 'Level 1 1FL', $, $, $, $, $, .ELEMENT., 0.0);",
            storey_id,
            sketchcad_core::ElementId::new().to_ifc_guid()
        )?;

        // Export Smart Objects
        // 1. Walls
        for wall in doc.get_elements_by_kind(ElementKind::Wall) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = wall.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCWALL('{}', $, '{}', 'Parametric BIM Wall', $, $, $, $, .SOLIDWALL.);",
                entity_id, ifc_guid, wall.name()
            )?;
        }

        // 2. Doors
        for door in doc.get_elements_by_kind(ElementKind::Door) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = door.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCDOOR('{}', $, '{}', 'Parametric BIM Door', $, $, $, $, 2100.0, 900.0, .DOOR., .SINGLE_SWING_LEFT.);",
                entity_id, ifc_guid, door.name()
            )?;
        }

        // 3. Windows
        for window in doc.get_elements_by_kind(ElementKind::Window) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = window.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCWINDOW('{}', $, '{}', 'Parametric BIM Window', $, $, $, $, 1200.0, 1500.0, .WINDOW., .WINDOW_CASEMENT.);",
                entity_id, ifc_guid, window.name()
            )?;
        }

        // 4. Columns
        for column in doc.get_elements_by_kind(ElementKind::Column) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = column.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCCOLUMN('{}', $, '{}', 'Structural Column', $, $, $, $, .COLUMN.);",
                entity_id, ifc_guid, column.name()
            )?;
        }

        // 5. Beams
        for beam in doc.get_elements_by_kind(ElementKind::Beam) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = beam.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCBEAM('{}', $, '{}', 'Structural Beam', $, $, $, $, .BEAM.);",
                entity_id, ifc_guid, beam.name()
            )?;
        }

        // 6. Slabs
        for slab in doc.get_elements_by_kind(ElementKind::Slab) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = slab.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCSLAB('{}', $, '{}', 'Structural Floor Slab', $, $, $, $, .FLOOR.);",
                entity_id, ifc_guid, slab.name()
            )?;
        }

        // 7. Ceilings
        for ceil in doc.get_elements_by_kind(ElementKind::Ceiling) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = ceil.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCCOVERING('{}', $, '{}', 'Suspended Ceiling', $, $, $, $, .CEILING.);",
                entity_id, ifc_guid, ceil.name()
            )?;
        }

        // 8. Stairs
        for stair in doc.get_elements_by_kind(ElementKind::Stairs) {
            let entity_id = step_id;
            step_id += 1;
            let ifc_guid = stair.id().to_ifc_guid();
            writeln!(
                file,
                "#{}=IFCSTAIR('{}', $, '{}', 'Straight Run Stair', $, $, $, $, .STRAIGHT_RUN_STAIR.);",
                entity_id, ifc_guid, stair.name()
            )?;
        }

        writeln!(file, "ENDSEC;")?;
        writeln!(file, "END-ISO-10303-21;")?;
        Ok(())
    }
}
