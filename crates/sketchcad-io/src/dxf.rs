use dxf::entities::{Entity, Line};
use dxf::Drawing;
use sketchcad_bim::BimDocument;
use sketchcad_geom::Polyline2D;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DxfError {
    #[error("DXF I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("DXF error: {0}")]
    Dxf(#[from] dxf::DxfError),
}

/// DXF Interoperability module for 2D drafting exchange
pub struct DxfHandler;

impl DxfHandler {
    /// Export all 2D vector drawings from the BIM Document into standard DXF format
    pub fn export_document_2d<P: AsRef<Path>>(doc: &BimDocument, path: P) -> Result<(), DxfError> {
        let mut drawing = Drawing::new();
        let polylines = doc.generate_2d_drawing();

        for poly in polylines {
            let n = poly.vertices.len();
            if n < 2 {
                continue;
            }
            for i in 0..n - 1 {
                let p1 = dxf::Point::new(poly.vertices[i].x, poly.vertices[i].y, 0.0);
                let p2 = dxf::Point::new(poly.vertices[i + 1].x, poly.vertices[i + 1].y, 0.0);
                let line = Line::new(p1, p2);
                drawing.add_entity(Entity::new(dxf::entities::EntityType::Line(line)));
            }
            if poly.closed && n > 2 {
                let p1 = dxf::Point::new(poly.vertices[n - 1].x, poly.vertices[n - 1].y, 0.0);
                let p2 = dxf::Point::new(poly.vertices[0].x, poly.vertices[0].y, 0.0);
                let line = Line::new(p1, p2);
                drawing.add_entity(Entity::new(dxf::entities::EntityType::Line(line)));
            }
        }

        drawing.save_file(path)?;
        Ok(())
    }

    /// Import 2D lines and polylines from an external DXF file
    pub fn import_dxf_2d<P: AsRef<Path>>(path: P) -> Result<Vec<Polyline2D>, DxfError> {
        let drawing = Drawing::load_file(path)?;
        let mut result = Vec::new();

        for entity in drawing.entities() {
            if let dxf::entities::EntityType::Line(ref line) = entity.specific {
                let p1 = sketchcad_core::Point2D::new(line.p1.x, line.p1.y);
                let p2 = sketchcad_core::Point2D::new(line.p2.x, line.p2.y);
                result.push(Polyline2D::new(vec![p1, p2], false));
            }
        }

        Ok(result)
    }

    /// Import 2D DXF entities and reconstruct smart Wall elements into the BIM Document
    pub fn import_dxf_into_document<P: AsRef<Path>>(path: P, doc: &mut BimDocument) -> Result<usize, DxfError> {
        let drawing = Drawing::load_file(path)?;
        let mut count = 0;
        let default_concrete = sketchcad_bim::Material::concrete();

        for entity in drawing.entities() {
            if let dxf::entities::EntityType::Line(ref line) = entity.specific {
                let p1 = sketchcad_core::Point2D::new(line.p1.x, line.p1.y);
                let p2 = sketchcad_core::Point2D::new(line.p2.x, line.p2.y);
                if p1.distance(p2) > 10.0 {
                    let mut wall = sketchcad_bim::Wall::new(p1, p2, 200.0, 3000.0, default_concrete.clone());
                    wall.name = "DXF 匯入牆體 (Imported Wall)".to_string();
                    doc.add_element(wall);
                    count += 1;
                }
            } else if let dxf::entities::EntityType::LwPolyline(ref lwpoly) = entity.specific {
                let verts: Vec<sketchcad_core::Point2D> = lwpoly
                    .vertices
                    .iter()
                    .map(|v| sketchcad_core::Point2D::new(v.x, v.y))
                    .collect();
                let n = verts.len();
                if n >= 2 {
                    for i in 0..n - 1 {
                        if verts[i].distance(verts[i + 1]) > 10.0 {
                            let mut wall = sketchcad_bim::Wall::new(
                                verts[i],
                                verts[i + 1],
                                200.0,
                                3000.0,
                                default_concrete.clone(),
                            );
                            wall.name = "DXF 多段線牆體".to_string();
                            doc.add_element(wall);
                            count += 1;
                        }
                    }
                    if lwpoly.is_closed() && n > 2 && verts[n - 1].distance(verts[0]) > 10.0 {
                        let mut wall = sketchcad_bim::Wall::new(
                            verts[n - 1],
                            verts[0],
                            200.0,
                            3000.0,
                            default_concrete.clone(),
                        );
                        wall.name = "DXF 閉合多段線牆體".to_string();
                        doc.add_element(wall);
                        count += 1;
                    }
                }
            }
        }
        doc.sync();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dxf_document_roundtrip() {
        let mut doc = BimDocument::new("DXF Roundtrip Test");
        let wall = sketchcad_bim::Wall::new(
            sketchcad_core::Point2D::new(0.0, 0.0),
            sketchcad_core::Point2D::new(5000.0, 0.0),
            200.0,
            3000.0,
            sketchcad_bim::Material::concrete(),
        );
        doc.add_element(wall);

        let temp_dxf = std::env::temp_dir().join("test_roundtrip.dxf");
        DxfHandler::export_document_2d(&doc, &temp_dxf).expect("Export DXF failed");
        assert!(temp_dxf.exists());

        let mut imported_doc = BimDocument::new("Imported DXF");
        let count = DxfHandler::import_dxf_into_document(&temp_dxf, &mut imported_doc)
            .expect("Import DXF failed");
        assert!(count > 0);
        assert!(imported_doc.element_count() > 0);

        let _ = std::fs::remove_file(temp_dxf);
    }
}
