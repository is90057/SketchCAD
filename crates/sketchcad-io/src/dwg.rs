use crate::dwg_reader::{DwgHeaderMetadata, DwgParseError, DwgReader, DwgVersion};
use crate::dxf::{DxfError, DxfHandler};
use sketchcad_geom::Polyline2D;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DwgError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Native DWG parse error: {0}")]
    NativeParse(#[from] DwgParseError),
    #[error("DXF conversion bridge error: {0}")]
    DxfBridge(#[from] DxfError),
    #[error("DWG converter executable failed: {0}")]
    ExternalToolFailed(String),
}

/// Enhanced DWG Interoperability Adapter
///
/// Features:
/// 1. Pure Rust native binary parsing (direct bit-stream decoding for lines, polylines, headers).
/// 2. Version and metadata inspection (detects DWG R13 ~ 2024+ formats).
/// 3. Resilient fallback pipeline to LibreDWG / ODA when available.
pub struct DwgAdapter;

impl DwgAdapter {
    /// Read native DWG file header metadata (DWG version, codepage, preview image offset)
    pub fn read_metadata<P: AsRef<Path>>(dwg_path: P) -> Result<DwgHeaderMetadata, DwgError> {
        let meta = DwgReader::read_metadata(dwg_path)?;
        Ok(meta)
    }

    /// Read metadata directly from in-memory DWG bytes
    pub fn read_metadata_bytes(bytes: &[u8]) -> Result<DwgHeaderMetadata, DwgError> {
        let meta = DwgReader::parse_header(bytes)?;
        Ok(meta)
    }

    /// Parse entities directly from in-memory DWG bytes
    pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<Polyline2D>, DwgError> {
        let entities = DwgReader::parse_entities_from_bytes(bytes)?;
        Ok(entities)
    }

    /// Import 2D vector drawing from DWG binary file.
    /// First attempts pure Rust native binary decoding; falls back to LibreDWG bridge if needed.
    pub fn import_dwg_2d<P: AsRef<Path>>(dwg_path: P) -> Result<Vec<Polyline2D>, DwgError> {
        let dwg_path = dwg_path.as_ref();

        // 1. Try pure Rust native binary parsing first
        if let Ok(native_entities) = DwgReader::read_2d_entities(dwg_path) {
            if !native_entities.is_empty() {
                return Ok(native_entities);
            }
        }

        // 2. Fallback to external conversion bridge (LibreDWG dwg2dxf)
        let temp_dxf = std::env::temp_dir().join(format!(
            "sketchcad_import_{}.dxf",
            sketchcad_core::ElementId::new().to_ifc_guid()
        ));

        let output = Command::new("dwg2dxf")
            .arg("-y")
            .arg("-o")
            .arg(&temp_dxf)
            .arg(dwg_path)
            .output();

        match output {
            Ok(status) if status.status.success() => {
                let polylines = DxfHandler::import_dxf_2d(&temp_dxf)?;
                let _ = std::fs::remove_file(temp_dxf);
                Ok(polylines)
            }
            Ok(status) => {
                let err_msg = String::from_utf8_lossy(&status.stderr);
                Err(DwgError::ExternalToolFailed(format!(
                    "dwg2dxf failed with error: {}",
                    err_msg
                )))
            }
            Err(e) => Err(DwgError::ExternalToolFailed(format!(
                "Native DWG parser found no standard entities, and external tool dwg2dxf was not found on PATH: {}",
                e
            ))),
        }
    }

    /// Create native binary DWG file (for export and roundtrip testing)
    pub fn create_native_dwg(version: DwgVersion, polylines: &[Polyline2D]) -> Vec<u8> {
        DwgReader::create_binary_dwg(version, polylines)
    }

    /// Import DWG entities and reconstruct smart Wall elements into the BIM Document
    pub fn import_dwg_into_document<P: AsRef<Path>>(
        dwg_path: P,
        doc: &mut sketchcad_bim::BimDocument,
    ) -> Result<usize, DwgError> {
        let polylines = Self::import_dwg_2d(dwg_path)?;
        let mut count = 0;
        let default_concrete = sketchcad_bim::Material::concrete();

        for poly in polylines {
            let n = poly.vertices.len();
            if n < 2 {
                continue;
            }
            for i in 0..n - 1 {
                if poly.vertices[i].distance(poly.vertices[i + 1]) > 10.0 {
                    let mut wall = sketchcad_bim::Wall::new(
                        poly.vertices[i],
                        poly.vertices[i + 1],
                        200.0,
                        3000.0,
                        default_concrete.clone(),
                    );
                    wall.name = "DWG 匯入牆體 (Imported DWG Wall)".to_string();
                    doc.add_element(wall);
                    count += 1;
                }
            }
            if poly.closed && n > 2 && poly.vertices[n - 1].distance(poly.vertices[0]) > 10.0 {
                let mut wall = sketchcad_bim::Wall::new(
                    poly.vertices[n - 1],
                    poly.vertices[0],
                    200.0,
                    3000.0,
                    default_concrete.clone(),
                );
                wall.name = "DWG 閉合牆體".to_string();
                doc.add_element(wall);
                count += 1;
            }
        }
        doc.sync();
        Ok(count)
    }
}
