use crate::document::Storey;
use crate::element::BimElement;
use crate::{
    Beam, Ceiling, Column, CurtainGrid, CurtainWall, Door, Furniture, IfcComponent, LightFixture,
    Mullion, Railing, Ramp, Roof, Slab, Space, Stairs, Wall, Window,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sketchcad_core::ElementId;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Serializable wrapper enum for all supported BIM smart elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableElement {
    Wall(Wall),
    Door(Door),
    Window(Window),
    Column(Column),
    Beam(Beam),
    Slab(Slab),
    Ceiling(Ceiling),
    Stairs(Stairs),
    Railing(Railing),
    Roof(Roof),
    Ramp(Ramp),
    CurtainWall(CurtainWall),
    CurtainGrid(CurtainGrid),
    Mullion(Mullion),
    Space(Space),
    Furniture(Furniture),
    LightFixture(LightFixture),
    IfcComponent(IfcComponent),
}

impl SerializableElement {
    pub fn into_boxed_element(self) -> Box<dyn BimElement> {
        match self {
            SerializableElement::Wall(w) => Box::new(w),
            SerializableElement::Door(d) => Box::new(d),
            SerializableElement::Window(w) => Box::new(w),
            SerializableElement::Column(c) => Box::new(c),
            SerializableElement::Beam(b) => Box::new(b),
            SerializableElement::Slab(s) => Box::new(s),
            SerializableElement::Ceiling(c) => Box::new(c),
            SerializableElement::Stairs(s) => Box::new(s),
            SerializableElement::Railing(r) => Box::new(r),
            SerializableElement::Roof(r) => Box::new(r),
            SerializableElement::Ramp(r) => Box::new(r),
            SerializableElement::CurtainWall(cw) => Box::new(cw),
            SerializableElement::CurtainGrid(cg) => Box::new(cg),
            SerializableElement::Mullion(m) => Box::new(m),
            SerializableElement::Space(s) => Box::new(s),
            SerializableElement::Furniture(f) => Box::new(f),
            SerializableElement::LightFixture(l) => Box::new(l),
            SerializableElement::IfcComponent(c) => Box::new(c),
        }
    }
}

/// Convert an arbitrary BimElement trait object into a SerializableElement
pub fn element_to_serializable(elem: &dyn BimElement) -> Option<SerializableElement> {
    let any = elem.as_any();
    if let Some(w) = any.downcast_ref::<Wall>() {
        Some(SerializableElement::Wall(w.clone()))
    } else if let Some(d) = any.downcast_ref::<Door>() {
        Some(SerializableElement::Door(d.clone()))
    } else if let Some(w) = any.downcast_ref::<Window>() {
        Some(SerializableElement::Window(w.clone()))
    } else if let Some(c) = any.downcast_ref::<Column>() {
        Some(SerializableElement::Column(c.clone()))
    } else if let Some(b) = any.downcast_ref::<Beam>() {
        Some(SerializableElement::Beam(b.clone()))
    } else if let Some(s) = any.downcast_ref::<Slab>() {
        Some(SerializableElement::Slab(s.clone()))
    } else if let Some(c) = any.downcast_ref::<Ceiling>() {
        Some(SerializableElement::Ceiling(c.clone()))
    } else if let Some(s) = any.downcast_ref::<Stairs>() {
        Some(SerializableElement::Stairs(s.clone()))
    } else if let Some(r) = any.downcast_ref::<Railing>() {
        Some(SerializableElement::Railing(r.clone()))
    } else if let Some(r) = any.downcast_ref::<Roof>() {
        Some(SerializableElement::Roof(r.clone()))
    } else if let Some(r) = any.downcast_ref::<Ramp>() {
        Some(SerializableElement::Ramp(r.clone()))
    } else if let Some(cw) = any.downcast_ref::<CurtainWall>() {
        Some(SerializableElement::CurtainWall(cw.clone()))
    } else if let Some(cg) = any.downcast_ref::<CurtainGrid>() {
        Some(SerializableElement::CurtainGrid(cg.clone()))
    } else if let Some(m) = any.downcast_ref::<Mullion>() {
        Some(SerializableElement::Mullion(m.clone()))
    } else if let Some(s) = any.downcast_ref::<Space>() {
        Some(SerializableElement::Space(s.clone()))
    } else if let Some(f) = any.downcast_ref::<Furniture>() {
        Some(SerializableElement::Furniture(f.clone()))
    } else if let Some(l) = any.downcast_ref::<LightFixture>() {
        Some(SerializableElement::LightFixture(l.clone()))
    } else if let Some(c) = any.downcast_ref::<IfcComponent>() {
        Some(SerializableElement::IfcComponent(c.clone()))
    } else {
        None
    }
}

/// Persistent SketchCAD Project File representation (`.skc` / `.sketchcad`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchCadProject {
    pub format_version: u32,
    pub app_version: String,
    pub name: String,
    pub created_at: String,
    pub modified_at: String,
    pub elements: Vec<SerializableElement>,
    pub hidden_elements: HashSet<ElementId>,
    #[serde(default)]
    pub storeys: Vec<Storey>,
}

impl SketchCadProject {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            format_version: 1,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            name: name.into(),
            created_at: now.clone(),
            modified_at: now,
            elements: Vec::new(),
            hidden_elements: HashSet::new(),
            storeys: Vec::new(),
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let project: Self = serde_json::from_str(&content)?;
        Ok(project)
    }
}
