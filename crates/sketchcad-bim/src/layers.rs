//! Layers (圖層): named groups of elements with shared colour, line type, line weight,
//! transparency and on / off state, plus layer isolation and the LAYWALK preview.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sketchcad_core::ElementId;

use crate::element::ElementKind;
use crate::elements::draft::LineType;

/// The layer every drawing has and that cannot be deleted or renamed.
pub const LAYER_ZERO: &str = "0";

/// A drawing layer. Unset colour / line type / line weight mean "use the object's own"
/// (for BIM elements: their architectural line styles).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    #[serde(default)]
    pub color: Option<[u8; 4]>,
    #[serde(default)]
    pub linetype: Option<LineType>,
    /// Line weight in mm (e.g. 0.25)
    #[serde(default)]
    pub lineweight: Option<f32>,
    /// 0 – 90 %
    #[serde(default)]
    pub transparency: u8,
    /// On / off
    pub visible: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), color: None, linetype: None, lineweight: None, transparency: 0, visible: true }
    }

    /// Standard line weights (mm) offered by the layer manager.
    pub const LINEWEIGHTS: [f32; 9] = [0.09, 0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4];
}

/// Layer BIM elements are placed on by default (drafting and annotations use the
/// current layer instead).
pub fn default_layer_for(kind: ElementKind) -> Option<&'static str> {
    Some(match kind {
        ElementKind::Wall | ElementKind::CurtainWall | ElementKind::CurtainGrid | ElementKind::Mullion => "A-牆體",
        ElementKind::Door | ElementKind::Window => "A-門窗",
        ElementKind::Column | ElementKind::Beam | ElementKind::Slab | ElementKind::Roof => "A-結構",
        ElementKind::Stairs | ElementKind::Railing | ElementKind::Ramp => "A-樓梯坡道",
        ElementKind::Ceiling => "A-天花",
        ElementKind::Space => "A-空間",
        ElementKind::Furniture | ElementKind::Model => "A-家具設備",
        ElementKind::LightFixture | ElementKind::Outlet => "E-水電照明",
        ElementKind::Draft | ElementKind::Hatch | ElementKind::Annotation => return None,
    })
}

/// Layers created with every document.
pub fn default_layers() -> Vec<Layer> {
    let mut names = vec![LAYER_ZERO];
    for kind in [
        ElementKind::Wall,
        ElementKind::Door,
        ElementKind::Column,
        ElementKind::Stairs,
        ElementKind::Ceiling,
        ElementKind::Space,
        ElementKind::Furniture,
        ElementKind::LightFixture,
    ] {
        names.push(default_layer_for(kind).expect("BIM kind"));
    }
    names.into_iter().map(Layer::new).collect()
}

/// Layer table of a document: the layers, which element is on which layer, the current
/// layer, and the temporary isolation / LAYWALK states.
#[derive(Debug, Clone)]
pub struct LayerTable {
    pub layers: Vec<Layer>,
    /// Explicit layer of an element (others use [`default_layer_for`] / layer 0)
    pub assignments: HashMap<ElementId, String>,
    pub current: String,
    /// LAYWALK: only these layers are shown while browsing
    pub preview: Option<HashSet<String>>,
    /// LAYISO: on / off states to restore with LAYUNISO
    pub isolation_backup: Option<Vec<(String, bool)>>,
}

impl Default for LayerTable {
    fn default() -> Self {
        Self { layers: default_layers(), assignments: HashMap::new(), current: LAYER_ZERO.to_string(), preview: None, isolation_backup: None }
    }
}

impl LayerTable {
    pub fn get(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name == name)
    }

    /// Adds the layer if it does not exist yet.
    pub fn ensure(&mut self, name: &str) {
        if self.get(name).is_none() {
            self.layers.push(Layer::new(name));
        }
    }

    /// Whether objects on `name` are shown (on, and part of a LAYWALK preview if active).
    pub fn shows(&self, name: &str) -> bool {
        match &self.preview {
            Some(set) => set.contains(name),
            None => self.get(name).is_none_or(|l| l.visible),
        }
    }

    pub fn add(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("圖層名稱不可空白".into());
        }
        if self.get(name).is_some() {
            return Err(format!("圖層「{name}」已存在"));
        }
        self.layers.push(Layer::new(name));
        Ok(())
    }

    /// Renames a layer and every reference to it.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if old == LAYER_ZERO {
            return Err("圖層 0 無法更名".into());
        }
        if new.is_empty() {
            return Err("圖層名稱不可空白".into());
        }
        if old == new {
            return Ok(());
        }
        if self.get(new).is_some() {
            return Err(format!("圖層「{new}」已存在"));
        }
        let layer = self.get_mut(old).ok_or_else(|| format!("找不到圖層「{old}」"))?;
        layer.name = new.to_string();
        for assigned in self.assignments.values_mut().filter(|a| a.as_str() == old) {
            *assigned = new.to_string();
        }
        if self.current == old {
            self.current = new.to_string();
        }
        Ok(())
    }

    /// LAYISO: turns off every layer except `keep`, remembering the previous states.
    pub fn isolate(&mut self, keep: &[String]) {
        if self.isolation_backup.is_none() {
            self.isolation_backup = Some(self.layers.iter().map(|l| (l.name.clone(), l.visible)).collect());
        }
        for layer in &mut self.layers {
            layer.visible = keep.contains(&layer.name);
        }
    }

    /// LAYUNISO: restores the layers turned off by the last LAYISO. Returns false if none.
    pub fn unisolate(&mut self) -> bool {
        let Some(backup) = self.isolation_backup.take() else { return false };
        for (name, visible) in backup {
            if let Some(layer) = self.get_mut(&name) {
                layer.visible = visible;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_updates_references_and_isolation_restores() {
        let mut table = LayerTable::default();
        assert!(table.add("A-牆體").is_err(), "default layer exists");
        table.add("Notes").unwrap();
        let id = ElementId::new();
        table.assignments.insert(id, "Notes".into());
        table.current = "Notes".into();
        table.rename("Notes", "註記").unwrap();
        assert_eq!(table.assignments[&id], "註記");
        assert_eq!(table.current, "註記");
        assert!(table.rename(LAYER_ZERO, "x").is_err());

        table.get_mut("A-門窗").unwrap().visible = false;
        table.isolate(&["A-牆體".into()]);
        assert!(table.shows("A-牆體") && !table.shows("註記") && !table.shows(LAYER_ZERO));
        assert!(table.unisolate());
        assert!(table.shows("註記") && !table.shows("A-門窗"), "previous on / off states come back");
        assert!(!table.unisolate(), "nothing left to restore");

        table.preview = Some(["A-門窗".to_string()].into());
        assert!(table.shows("A-門窗") && !table.shows("A-牆體"), "LAYWALK shows only the browsed layers");
    }
}
