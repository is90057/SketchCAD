use chrono::Utc;
use serde::{Deserialize, Serialize};
use sketchcad_core::{ElementId, Point3D, Vector3D};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use uuid::Uuid;
use zip::read::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BcfTopic {
    pub guid: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub status: String,
    pub priority: String,
    pub components: Vec<ElementId>,
    pub camera_pos: Point3D,
    pub camera_dir: Vector3D,
}

impl BcfTopic {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            guid: Uuid::new_v4().to_string(),
            title: title.into(),
            description: description.into(),
            author: "SketchCAD User".to_string(),
            status: "Active".to_string(),
            priority: "Normal".to_string(),
            components: Vec::new(),
            camera_pos: Point3D::new(5000.0, -5000.0, 3000.0),
            camera_dir: Vector3D::new(-0.7, 0.7, -0.3).normalize(),
        }
    }
}

/// BCF (BIM Collaboration Format) Exporter & Importer (Zip archive)
pub struct BcfHandler;

impl BcfHandler {
    /// Save a collection of topics into a standard `.bcfzip` or `.bcf` archive
    pub fn export_bcf_package<P: AsRef<Path>>(topics: &[BcfTopic], path: P) -> io::Result<()> {
        let file = File::create(path)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // 1. bcf.version
        zip.start_file("bcf.version", options)?;
        writeln!(
            zip,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Version VersionId="2.1" detailedVersion="2.1" />"#
        )?;

        let now = Utc::now().to_rfc3339();

        // 2. Each topic as a folder: {guid}/markup.bcf & {guid}/viewpoint.bcfv
        for topic in topics {
            let topic_dir = format!("{}/", topic.guid);

            // markup.bcf
            zip.start_file(format!("{}markup.bcf", topic_dir), options)?;
            let markup_xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<Markup xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Topic Guid="{guid}" TopicType="Issue" TopicStatus="{status}">
    <Title>{title}</Title>
    <Priority>{priority}</Priority>
    <CreationDate>{date}</CreationDate>
    <CreationAuthor>{author}</CreationAuthor>
    <Description>{desc}</Description>
  </Topic>
  <Viewpoints Guid="{guid}">
    <Viewpoint>viewpoint.bcfv</Viewpoint>
  </Viewpoints>
</Markup>"#,
                guid = topic.guid,
                status = topic.status,
                title = topic.title,
                priority = topic.priority,
                date = now,
                author = topic.author,
                desc = topic.description,
            );
            zip.write_all(markup_xml.as_bytes())?;

            // viewpoint.bcfv
            zip.start_file(format!("{}viewpoint.bcfv", topic_dir), options)?;
            let mut components_xml = String::new();
            for comp_id in &topic.components {
                components_xml.push_str(&format!(
                    r#"      <Component IfcGuid="{}" />"#,
                    comp_id.to_ifc_guid()
                ));
                components_xml.push('\n');
            }

            let viewpoint_xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<VisualizationInfo Guid="{guid}">
  <Components>
{components}
  </Components>
  <PerspectiveCamera>
    <CameraViewPoint>
      <X>{px}</X><Y>{py}</Y><Z>{pz}</Z>
    </CameraViewPoint>
    <CameraDirection>
      <X>{dx}</X><Y>{dy}</Y><Z>{dz}</Z>
    </CameraDirection>
    <CameraUpVector>
      <X>0.0</X><Y>0.0</Y><Z>1.0</Z>
    </CameraUpVector>
    <FieldOfView>60.0</FieldOfView>
  </PerspectiveCamera>
</VisualizationInfo>"#,
                guid = topic.guid,
                components = components_xml,
                px = topic.camera_pos.x,
                py = topic.camera_pos.y,
                pz = topic.camera_pos.z,
                dx = topic.camera_dir.x,
                dy = topic.camera_dir.y,
                dz = topic.camera_dir.z,
            );
            zip.write_all(viewpoint_xml.as_bytes())?;
        }

        zip.finish()?;
        Ok(())
    }

    /// Import and parse topics from a `.bcfzip` or `.bcf` archive
    pub fn import_bcf_package<P: AsRef<Path>>(path: P) -> io::Result<Vec<BcfTopic>> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;
        let mut topics = Vec::new();

        // Pass 1: read markups
        for i in 0..archive.len() {
            let mut zip_file = archive.by_index(i)?;
            let name = zip_file.name().to_string();
            if name.ends_with("markup.bcf") {
                let mut xml = String::new();
                zip_file.read_to_string(&mut xml)?;

                let guid = xml
                    .find("Topic Guid=\"")
                    .and_then(|idx| {
                        let rem = &xml[idx + 12..];
                        rem.find('"').map(|end| rem[..end].to_string())
                    })
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                let title = extract_tag_content(&xml, "Title").unwrap_or_else(|| "BCF 協同議題".to_string());
                let description = extract_tag_content(&xml, "Description").unwrap_or_default();
                let author = extract_tag_content(&xml, "CreationAuthor").unwrap_or_else(|| "BIM 使用者".to_string());
                let status = extract_tag_content(&xml, "TopicStatus")
                    .or_else(|| {
                        xml.find("TopicStatus=\"").and_then(|idx| {
                            let rem = &xml[idx + 13..];
                            rem.find('"').map(|end| rem[..end].to_string())
                        })
                    })
                    .unwrap_or_else(|| "Active".to_string());
                let priority = extract_tag_content(&xml, "Priority").unwrap_or_else(|| "Normal".to_string());

                let mut topic = BcfTopic::new(title, description);
                topic.guid = guid;
                topic.author = author;
                topic.status = status;
                topic.priority = priority;

                topics.push(topic);
            }
        }

        Ok(topics)
    }
}

fn extract_tag_content(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml[start..].find(&close_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcf_export_and_import() {
        let temp_bcf = std::env::temp_dir().join("test_roundtrip.bcfzip");
        let topic = BcfTopic::new("測試議題", "檢討樑柱接頭淨高");
        let topics = vec![topic];

        BcfHandler::export_bcf_package(&topics, &temp_bcf).expect("Export BCF failed");
        assert!(temp_bcf.exists());

        let loaded = BcfHandler::import_bcf_package(&temp_bcf).expect("Import BCF failed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "測試議題");
        assert_eq!(loaded[0].description, "檢討樑柱接頭淨高");

        let _ = std::fs::remove_file(temp_bcf);
    }
}
