use crate::cost::QuantityTakeoff;
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use crate::{
    Beam, Ceiling, Column, CurtainGrid, CurtainWall, Door, Furniture, Annotation, DraftShape, LightFixture, ModelComponent, Mullion, Outlet,
    Railing, Ramp, Roof, Slab, Space, SpaceType, Stairs, Wall, Window,
};

use sketchcad_core::{ChangeEvent, DependencyGraph, ElementId, Point2D};
use sketchcad_geom::{Polyline2D, TriangleMesh};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::project::{element_to_serializable, PaintedElement, SketchCadProject};

/// Placement of a freely positioned object: plan position of its footprint centre, base
/// height, rotation (the front faces local −Y) and overall (width, depth, height) in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementPlacement {
    pub position: sketchcad_core::Point2D,
    pub base_z: f64,
    pub rotation_deg: f64,
    pub size: (f64, f64, f64),
}

/// A building level (樓層) used to generate one floor-plan sheet per storey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Storey {
    pub name: String,
    /// Finished floor elevation in mm (same Z frame as the element geometry)
    pub elevation: f64,
    /// Elevation as authored in the source model (e.g. IFC), when the import shifted the geometry
    #[serde(default)]
    pub source_elevation: Option<f64>,
}

/// Globally unique revision source, so revisions never repeat across documents (undo / redo swaps).
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// Master BIM Document containing all smart elements, the reactive dependency graph,
/// and coordinating global synchronization between 2D drawings, 3D meshes, and QTO schedules.
#[derive(Debug)]
pub struct BimDocument {
    pub name: String,
    elements: HashMap<ElementId, Box<dyn BimElement>>,
    dependency_graph: DependencyGraph,
    events: Vec<ChangeEvent>,
    hidden_elements: HashSet<ElementId>,
    storeys: Vec<Storey>,
    revision: u64,
    /// Elements handed out through `get_*_mut`; their content may change without a revision bump.
    edited: HashSet<ElementId>,
    /// User-added materials, offered after the built-in presets (材質庫)
    custom_materials: Vec<Material>,
    /// Paint-bucket finish applied to an element; overrides its display colour
    painted: HashMap<ElementId, Material>,
    /// Layers (圖層) and which element is on which layer
    layer_table: crate::layers::LayerTable,
}

impl Clone for BimDocument {
    fn clone(&self) -> Self {
        let mut elements = HashMap::new();
        for (id, elem) in &self.elements {
            elements.insert(*id, elem.clone_box());
        }
        Self {
            name: self.name.clone(),
            elements,
            dependency_graph: self.dependency_graph.clone(),
            events: self.events.clone(),
            hidden_elements: self.hidden_elements.clone(),
            storeys: self.storeys.clone(),
            revision: self.revision,
            edited: self.edited.clone(),
            custom_materials: self.custom_materials.clone(),
            painted: self.painted.clone(),
            layer_table: self.layer_table.clone(),
        }
    }
}

impl BimDocument {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            elements: HashMap::new(),
            dependency_graph: DependencyGraph::new(),
            events: Vec::new(),
            hidden_elements: HashSet::new(),
            storeys: Vec::new(),
            revision: next_revision(),
            edited: HashSet::new(),
            custom_materials: Vec::new(),
            painted: HashMap::new(),
            layer_table: crate::layers::LayerTable::default(),
        }
    }

    /// Marks the document as structurally modified. Every `&mut self` API except the
    /// `get_*_mut` accessors calls this, so views can cache derived geometry.
    fn touch(&mut self) {
        self.revision = next_revision();
    }

    /// Changes on every structural modification (add / remove / move / visibility / sync ...).
    /// Edits made through `get_*_mut` do not bump it; see [`Self::edited_elements`].
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Elements that were borrowed mutably and may have been edited in place.
    /// Views should re-check these (e.g. by hashing) in addition to watching [`Self::revision`].
    pub fn edited_elements(&self) -> impl Iterator<Item = ElementId> + '_ {
        self.edited.iter().copied()
    }

    /// Building storeys sorted by elevation (empty for single-level models).
    pub fn storeys(&self) -> &[Storey] {
        &self.storeys
    }

    pub fn set_storeys(&mut self, mut storeys: Vec<Storey>) {
        self.touch();
        storeys.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        self.storeys = storeys;
    }

    /// Add a new BIM smart element to the document.
    pub fn add_element<E: BimElement + 'static>(&mut self, element: E) -> ElementId {
        self.touch();
        let id = element.id();
        let deps = element.dependencies();

        let kind = element.kind();
        self.elements.insert(id, Box::new(element));
        self.dependency_graph.add_element(id);
        self.assign_new_element_layer(id, kind);

        for dep in deps {
            let _ = self.dependency_graph.add_dependency(dep, id);
        }

        self.events.push(ChangeEvent::ElementCreated(id));
        id
    }

    /// Add a boxed BIM smart element to the document.
    pub fn add_boxed_element(&mut self, element: Box<dyn BimElement>) -> ElementId {
        self.touch();
        let id = element.id();
        let deps = element.dependencies();

        let kind = element.kind();
        self.elements.insert(id, element);
        self.dependency_graph.add_element(id);
        self.assign_new_element_layer(id, kind);

        for dep in deps {
            let _ = self.dependency_graph.add_dependency(dep, id);
        }

        self.events.push(ChangeEvent::ElementCreated(id));
        self.sync();
        id
    }

    /// Paste an element with a brand new ID and spatial offset into the document
    pub fn paste_element(&mut self, source: &dyn BimElement, offset: sketchcad_core::Vector3D) -> ElementId {
        self.touch();
        let mut new_elem = source.clone_with_new_id();
        new_elem.translate(offset);
        let new_id = new_elem.id();
        if let Some(paint) = self.painted.get(&source.id()).cloned() {
            self.painted.insert(new_id, paint);
        }
        if let Some(layer) = self.layer_table.assignments.get(&source.id()).cloned() {
            self.layer_table.assignments.insert(new_id, layer);
        }
        let deps = new_elem.dependencies();

        self.elements.insert(new_id, new_elem);
        self.dependency_graph.add_element(new_id);

        for dep in deps {
            let _ = self.dependency_graph.add_dependency(dep, new_id);
        }

        self.events.push(ChangeEvent::ElementCreated(new_id));
        self.sync();
        new_id
    }

    /// Remove an element from the BIM document and cascade removal in the dependency graph
    pub fn remove_element(&mut self, id: ElementId) -> bool {
        self.touch();
        if self.elements.remove(&id).is_some() {
            self.hidden_elements.remove(&id);
            self.painted.remove(&id);
            self.layer_table.assignments.remove(&id);
            self.dependency_graph.remove_element(id);
            self.events.push(ChangeEvent::ElementDeleted(id));
            self.sync();
            true
        } else {
            false
        }
    }

    /// Move / translate an element in 3D world space
    pub fn move_element(&mut self, id: ElementId, delta: sketchcad_core::Vector3D) -> bool {
        self.touch();
        // Doors and windows hosted by a moved wall travel with it
        let hosted: Vec<ElementId> = self
            .elements
            .values()
            .filter(|e| {
                let any = e.as_any();
                any.downcast_ref::<Door>().is_some_and(|d| d.host_wall_id == id)
                    || any.downcast_ref::<Window>().is_some_and(|w| w.host_wall_id == id)
            })
            .map(|e| e.id())
            .collect();
        for h in hosted {
            if let Some(opening) = self.elements.get_mut(&h) {
                opening.translate(delta);
                self.edited.insert(h);
            }
        }
        if let Some(elem) = self.elements.get_mut(&id) {
            elem.translate(delta);
            self.events.push(ChangeEvent::ElementModified(id));
            self.sync();
            true
        } else {
            false
        }
    }

    /// Add a Door and automatically register its opening void into the host Wall
    pub fn add_door(&mut self, door: crate::elements::door::Door) -> ElementId {
        self.touch();
        self.add_element(door)
    }

    /// Add a Window and automatically register its opening void into the host Wall
    pub fn add_window(&mut self, window: crate::elements::window::Window) -> ElementId {
        self.touch();
        self.add_element(window)
    }

    /// Retrieve an element by ID
    pub fn get_element(&self, id: ElementId) -> Option<&dyn BimElement> {
        self.elements.get(&id).map(|b| b.as_ref())
    }

    /// Retrieve an element mutably by ID
    pub fn get_element_mut(&mut self, id: ElementId) -> Option<&mut dyn BimElement> {
        self.edited.insert(id);
        self.elements.get_mut(&id).map(|b| b.as_mut())
    }

    pub fn get_wall(&self, id: ElementId) -> Option<&Wall> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Wall>())
    }

    pub fn get_wall_mut(&mut self, id: ElementId) -> Option<&mut Wall> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Wall>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_column(&self, id: ElementId) -> Option<&Column> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Column>())
    }

    pub fn get_column_mut(&mut self, id: ElementId) -> Option<&mut Column> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Column>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_door(&self, id: ElementId) -> Option<&Door> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Door>())
    }

    pub fn get_door_mut(&mut self, id: ElementId) -> Option<&mut Door> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Door>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_window(&self, id: ElementId) -> Option<&Window> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Window>())
    }

    pub fn get_window_mut(&mut self, id: ElementId) -> Option<&mut Window> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Window>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_slab(&self, id: ElementId) -> Option<&Slab> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Slab>())
    }

    pub fn get_slab_mut(&mut self, id: ElementId) -> Option<&mut Slab> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Slab>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_ceiling(&self, id: ElementId) -> Option<&Ceiling> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Ceiling>())
    }

    pub fn get_ceiling_mut(&mut self, id: ElementId) -> Option<&mut Ceiling> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Ceiling>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_stairs(&self, id: ElementId) -> Option<&Stairs> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Stairs>())
    }

    pub fn get_stairs_mut(&mut self, id: ElementId) -> Option<&mut Stairs> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Stairs>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_beam(&self, id: ElementId) -> Option<&Beam> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Beam>())
    }

    pub fn get_beam_mut(&mut self, id: ElementId) -> Option<&mut Beam> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Beam>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_railing(&self, id: ElementId) -> Option<&Railing> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Railing>())
    }

    pub fn get_railing_mut(&mut self, id: ElementId) -> Option<&mut Railing> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Railing>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_roof(&self, id: ElementId) -> Option<&Roof> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Roof>())
    }

    pub fn get_roof_mut(&mut self, id: ElementId) -> Option<&mut Roof> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Roof>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_ramp(&self, id: ElementId) -> Option<&Ramp> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Ramp>())
    }

    pub fn get_ramp_mut(&mut self, id: ElementId) -> Option<&mut Ramp> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Ramp>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_curtain_wall(&self, id: ElementId) -> Option<&CurtainWall> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<CurtainWall>())
    }

    pub fn get_curtain_wall_mut(&mut self, id: ElementId) -> Option<&mut CurtainWall> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<CurtainWall>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_curtain_grid(&self, id: ElementId) -> Option<&CurtainGrid> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<CurtainGrid>())
    }

    pub fn get_curtain_grid_mut(&mut self, id: ElementId) -> Option<&mut CurtainGrid> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<CurtainGrid>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_mullion(&self, id: ElementId) -> Option<&Mullion> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Mullion>())
    }

    pub fn get_mullion_mut(&mut self, id: ElementId) -> Option<&mut Mullion> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Mullion>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_space(&self, id: ElementId) -> Option<&Space> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Space>())
    }

    pub fn get_space_mut(&mut self, id: ElementId) -> Option<&mut Space> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Space>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_furniture(&self, id: ElementId) -> Option<&Furniture> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Furniture>())
    }

    pub fn get_furniture_mut(&mut self, id: ElementId) -> Option<&mut Furniture> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Furniture>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_light_fixture(&self, id: ElementId) -> Option<&LightFixture> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<LightFixture>())
    }

    pub fn get_light_fixture_mut(&mut self, id: ElementId) -> Option<&mut LightFixture> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<LightFixture>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_outlet(&self, id: ElementId) -> Option<&Outlet> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Outlet>())
    }

    pub fn get_outlet_mut(&mut self, id: ElementId) -> Option<&mut Outlet> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Outlet>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    pub fn get_annotation(&self, id: ElementId) -> Option<&Annotation> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<Annotation>())
    }

    pub fn get_annotation_mut(&mut self, id: ElementId) -> Option<&mut Annotation> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<Annotation>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    /// Adds a dimension, text or leader (註解) on the floor at `elevation`.
    pub fn insert_annotation(&mut self, geometry: crate::AnnotationGeometry, elevation: f64, text_height: f64) -> ElementId {
        let id = self.add_element(Annotation::new(geometry, elevation, text_height));
        self.sync();
        id
    }

    pub fn get_draft(&self, id: ElementId) -> Option<&DraftShape> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<DraftShape>())
    }

    pub fn get_draft_mut(&mut self, id: ElementId) -> Option<&mut DraftShape> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<DraftShape>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    /// Rotates / scales / mirrors an element in plan (修改). Supports drafting, walls (their
    /// doors and windows follow), columns, slabs and ceilings (90° steps only), spaces,
    /// furniture, library models and light fixtures. Returns why when it cannot.
    pub fn transform_element(&mut self, id: ElementId, xf: &crate::Transform2D) -> Result<(), String> {
        let s = xf.scale_factor();
        let quarter_turns = |xf: &crate::Transform2D| xf.x.x.abs() < 1e-9;
        // Doors and windows hosted by a wall move with it
        let hosted: Vec<ElementId> = self
            .elements
            .values()
            .filter(|e| {
                let any = e.as_any();
                any.downcast_ref::<Door>().is_some_and(|d| d.host_wall_id == id)
                    || any.downcast_ref::<Window>().is_some_and(|w| w.host_wall_id == id)
            })
            .map(|e| e.id())
            .collect();
        if let Some(d) = self.get_draft_mut(id) {
            d.geometry = d.geometry.transformed(xf);
        } else if let Some(w) = self.get_wall_mut(id) {
            (w.start, w.end) = (xf.point(w.start), xf.point(w.end));
            w.thickness *= s;
            for o in &mut w.openings {
                (o.offset_along_wall, o.width) = (o.offset_along_wall * s, o.width * s);
            }
            for hosted in hosted {
                if let Some(door) = self.get_door_mut(hosted) {
                    (door.world_pos, door.wall_dir) = (xf.point(door.world_pos), xf.vector(door.wall_dir).normalize_or_zero());
                    (door.offset_along_wall, door.width) = (door.offset_along_wall * s, door.width * s);
                } else if let Some(win) = self.get_window_mut(hosted) {
                    (win.world_pos, win.wall_dir) = (xf.point(win.world_pos), xf.vector(win.wall_dir).normalize_or_zero());
                    (win.offset_along_wall, win.width) = (win.offset_along_wall * s, win.width * s);
                }
            }
        } else if let Some(c) = self.get_column_mut(id) {
            if !xf.keeps_axes() {
                return Err("結構柱只能旋轉 90° 的倍數".into());
            }
            c.position = xf.point(c.position);
            (c.width, c.depth) = if quarter_turns(xf) { (c.depth * s, c.width * s) } else { (c.width * s, c.depth * s) };
        } else if self.get_slab(id).is_some() || self.get_ceiling(id).is_some() {
            if !xf.keeps_axes() {
                return Err("樓板與天花板只能旋轉 90° 的倍數".into());
            }
            let rect = |origin: Point2D, w: f64, d: f64| {
                let a = xf.point(origin);
                let b = xf.point(origin + sketchcad_core::Vector2D::new(w, d));
                (a.min(b), (b - a).abs())
            };
            if let Some(sl) = self.get_slab_mut(id) {
                let (o, size) = rect(sl.origin, sl.width, sl.depth);
                (sl.origin, sl.width, sl.depth) = (o, size.x, size.y);
            } else if let Some(ce) = self.get_ceiling_mut(id) {
                let (o, size) = rect(ce.origin, ce.width, ce.depth);
                (ce.origin, ce.width, ce.depth) = (o, size.x, size.y);
            }
        } else if let Some(sp) = self.get_space_mut(id) {
            sp.boundary = sp.boundary.iter().map(|p| xf.point(*p)).collect();
            if xf.is_mirror() {
                sp.boundary.reverse();
            }
        } else if let Some(f) = self.get_furniture_mut(id) {
            f.position = xf.point(f.position);
            f.rotation_deg = (xf.angle_deg(f.rotation_deg)).rem_euclid(360.0);
            (f.width, f.depth, f.height) = (f.width * s, f.depth * s, f.height * s);
        } else if let Some(m) = self.get_model_mut(id) {
            m.position = xf.point(m.position);
            m.rotation_deg = (xf.angle_deg(m.rotation_deg)).rem_euclid(360.0);
            m.scale *= s;
        } else if let Some(l) = self.get_light_fixture_mut(id) {
            l.position = xf.point(l.position);
        } else {
            let name = self.get_element(id).map(|e| e.name().to_string()).unwrap_or_default();
            return Err(format!("「{name}」不支援旋轉 / 縮放 / 鏡射"));
        }
        self.touch();
        self.events.push(ChangeEvent::ElementModified(id));
        self.sync();
        Ok(())
    }

    /// Line work that trims / extends drafting: other drafting entities (not hatches),
    /// wall and column outlines.
    pub fn cutting_edges(&self, exclude: ElementId) -> Vec<(Point2D, Point2D)> {
        let mut edges = Vec::new();
        for elem in self.elements.values() {
            if elem.id() == exclude || !self.is_visible(elem.id()) {
                continue;
            }
            if let Some(d) = elem.as_any().downcast_ref::<DraftShape>() {
                edges.extend(d.geometry.segments());
            } else if matches!(elem.kind(), ElementKind::Wall | ElementKind::Column) {
                if let Some(outline) = elem.to_2d_primitives().into_iter().next() {
                    let v = &outline.vertices;
                    let n = if outline.closed { v.len() } else { v.len().saturating_sub(1) };
                    edges.extend((0..n).map(|i| (v[i], v[(i + 1) % v.len()])));
                }
            }
        }
        edges
    }

    /// Replaces a drafting entity by `pieces` (trim): the first piece keeps the element,
    /// further pieces become copies with the same layer, colour and line type; no pieces
    /// deletes it. Returns the ids of the resulting elements.
    pub fn replace_draft(&mut self, id: ElementId, pieces: Vec<crate::DraftGeometry>) -> Vec<ElementId> {
        let Some(template) = self.get_draft(id).cloned() else { return Vec::new() };
        let mut pieces = pieces.into_iter();
        let Some(first) = pieces.next() else {
            self.remove_element(id);
            return Vec::new();
        };
        let mut ids = vec![id];
        let layer = self.layer_of(id);
        if let Some(d) = self.get_draft_mut(id) {
            d.name = first.label().to_string();
            d.geometry = first;
        }
        for geometry in pieces {
            let mut copy = template.clone();
            copy.id = ElementId::new();
            copy.name = geometry.label().to_string();
            copy.geometry = geometry;
            let new_id = self.add_element(copy);
            self.layer_table.assignments.insert(new_id, layer.clone());
            ids.push(new_id);
        }
        self.touch();
        self.sync();
        ids
    }

    /// Adds a drafting entity that copies `template`'s layer, colour, line type and level.
    pub fn add_draft_like(&mut self, template: ElementId, geometry: crate::DraftGeometry) -> Option<ElementId> {
        let mut copy = self.get_draft(template)?.clone();
        copy.id = ElementId::new();
        copy.name = geometry.label().to_string();
        copy.geometry = geometry;
        let layer = self.layer_of(template);
        let id = self.add_element(copy);
        self.layer_table.assignments.insert(id, layer);
        self.sync();
        Some(id)
    }

    /// Adds a 2D drafting entity (line, arc, hatch …) on the floor at `elevation`.
    pub fn insert_draft(&mut self, geometry: crate::DraftGeometry, elevation: f64) -> ElementId {
        let id = self.add_element(DraftShape::new(geometry, elevation));
        self.sync();
        id
    }

    pub fn get_model(&self, id: ElementId) -> Option<&ModelComponent> {
        self.elements.get(&id).and_then(|e| e.as_any().downcast_ref::<ModelComponent>())
    }

    pub fn get_model_mut(&mut self, id: ElementId) -> Option<&mut ModelComponent> {
        let elem = self.elements.get_mut(&id).and_then(|e| e.as_any_mut().downcast_mut::<ModelComponent>());
        if elem.is_some() {
            self.edited.insert(id);
        }
        elem
    }

    /// Position, height, rotation and size of a freely placed object (library models and
    /// furniture), the elements that can be moved and snapped to surfaces in 3D.
    pub fn placement(&self, id: ElementId) -> Option<ElementPlacement> {
        if let Some(m) = self.get_model(id) {
            Some(ElementPlacement { position: m.position, base_z: m.base_z, rotation_deg: m.rotation_deg, size: m.size() })
        } else {
            self.get_furniture(id).map(|f| ElementPlacement {
                position: f.position,
                base_z: f.base_z,
                rotation_deg: f.rotation_deg,
                size: (f.width, f.depth, f.height),
            })
        }
    }

    /// Moves / turns a placed object (see [`Self::placement`]). Returns false for other kinds.
    pub fn set_placement(&mut self, id: ElementId, position: sketchcad_core::Point2D, base_z: f64, rotation_deg: f64) -> bool {
        let rotation_deg = rotation_deg.rem_euclid(360.0);
        if let Some(m) = self.get_model_mut(id) {
            (m.position, m.base_z, m.rotation_deg) = (position, base_z, rotation_deg);
        } else if let Some(f) = self.get_furniture_mut(id) {
            (f.position, f.base_z, f.rotation_deg) = (position, base_z, rotation_deg);
        } else {
            return false;
        }
        self.touch();
        true
    }

    /// Place a model from the 3D model library (模型百寶箱). Returns `None` for an unknown id.
    pub fn insert_model(&mut self, model_id: &str, position: sketchcad_core::Point2D, rotation_deg: f64) -> Option<ElementId> {
        let model = crate::model_library::find_model(model_id)?;
        let id = self.add_element(ModelComponent::new(&model, position, rotation_deg));
        self.sync();
        Some(id)
    }

    /// Mount a power outlet on a wall face; `facing` points from the face into the room.
    pub fn insert_outlet(
        &mut self,
        host_wall_id: ElementId,
        position: sketchcad_core::Point2D,
        facing: sketchcad_core::Vector2D,
        standard: crate::PlugStandard,
        voltage: crate::OutletVoltage,
        gangs: u8,
    ) -> Option<ElementId> {
        self.get_wall(host_wall_id)?;
        let id = self.add_element(Outlet::new(host_wall_id, position, facing, standard, voltage, gangs));
        self.sync();
        Some(id)
    }

    /// Insert an architectural Room Space / Zone
    pub fn insert_space(
        &mut self,
        space_type: SpaceType,
        boundary: Vec<sketchcad_core::Point2D>,
        elevation: f64,
        clear_height: f64,
        floor_material: crate::Material,
    ) -> ElementId {
        self.touch();
        let space = Space::new(space_type, boundary, elevation, clear_height, floor_material);
        let id = self.add_element(space);
        self.sync();
        id
    }

    /// Insert an Interior Furniture / Fixture smart element
    pub fn insert_furniture(
        &mut self,
        kind: crate::FurnitureKind,
        position: sketchcad_core::Point2D,
        base_z: f64,
        rotation_deg: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let furniture = Furniture::new(kind, position, base_z, rotation_deg, material);
        let id = self.add_element(furniture);
        self.sync();
        id
    }

    /// Insert an Architectural Lighting Fixture
    pub fn insert_light_fixture(
        &mut self,
        light_type: crate::LightType,
        position: sketchcad_core::Point2D,
        mounting_height: f64,
    ) -> ElementId {
        self.touch();
        let fixture = LightFixture::new(light_type, position, mounting_height);
        let id = self.add_element(fixture);
        self.sync();
        id
    }

    /// Insert a Roof with footprint origin, width, depth, base_z, and pitch
    pub fn insert_roof(
        &mut self,
        origin: sketchcad_core::Point2D,
        width: f64,
        depth: f64,
        base_z: f64,
        pitch_deg: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let roof = Roof::new(origin, width, depth, base_z, pitch_deg, material);
        let id = self.add_element(roof);
        self.sync();
        id
    }

    /// Insert an accessible standard Ramp
    pub fn insert_ramp(
        &mut self,
        start_center: sketchcad_core::Point2D,
        end_center: sketchcad_core::Point2D,
        width: f64,
        base_z: f64,
        total_rise: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let ramp = Ramp::new(start_center, end_center, width, base_z, total_rise, material);
        let id = self.add_element(ramp);
        self.sync();
        id
    }

    /// Insert a Curtain Wall facade
    pub fn insert_curtain_wall(
        &mut self,
        start: sketchcad_core::Point2D,
        end: sketchcad_core::Point2D,
        height: f64,
        base_z: f64,
        u_spacing: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let cw = CurtainWall::new(start, end, height, base_z, u_spacing, material);
        let id = self.add_element(cw);
        self.sync();
        id
    }


    /// Insert a Railing between start and end 3D coordinates
    pub fn insert_railing(
        &mut self,
        start: sketchcad_core::Point3D,
        end: sketchcad_core::Point3D,
        height: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let railing = Railing::new(start, end, height, material);
        let id = self.add_element(railing);
        self.sync();
        id
    }

    /// Insert a Beam between start and end 3D coordinates
    pub fn insert_beam(
        &mut self,
        start: sketchcad_core::Point3D,
        end: sketchcad_core::Point3D,
        width: f64,
        depth: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let beam = Beam::new(start, end, width, depth, material);
        let id = self.add_element(beam);
        self.sync();
        id
    }

    /// Insert a Door into a host wall with automatic 3D opening subtraction and positioning
    pub fn insert_door(
        &mut self,
        wall_id: ElementId,
        offset_along_wall: f64,
        width: f64,
        height: f64,
        material: crate::Material,
        unit_price: f64,
    ) -> Option<ElementId> {
        self.touch();
        let (pos, dir) = {
            let wall = self.get_wall(wall_id)?;
            let dir = wall.direction();
            let start_offset = (offset_along_wall - width * 0.5).max(0.0);
            let pos = wall.start + dir * start_offset;
            (pos, dir)
        };

        let start_offset = (offset_along_wall - width * 0.5).max(0.0);
        if let Some(wall) = self.get_wall_mut(wall_id) {
            wall.add_opening(sketchcad_geom::OpeningVoid {
                offset_along_wall: start_offset,
                width,
                base_z: 0.0,
                height,
            });
        }

        let mut door = Door::new(wall_id, start_offset, width, height, material, unit_price);
        door.set_transform(pos, dir);
        let id = self.add_element(door);
        self.sync();
        Some(id)
    }

    /// Insert a Window into a host wall with automatic 3D opening subtraction and positioning
    pub fn insert_window(
        &mut self,
        wall_id: ElementId,
        offset_along_wall: f64,
        sill_height: f64,
        width: f64,
        height: f64,
        material: crate::Material,
        unit_price: f64,
    ) -> Option<ElementId> {
        self.touch();
        let (pos, dir) = {
            let wall = self.get_wall(wall_id)?;
            let dir = wall.direction();
            let pos = wall.start + dir * offset_along_wall;
            (pos, dir)
        };

        let start_offset = (offset_along_wall - width * 0.5).max(0.0);
        if let Some(wall) = self.get_wall_mut(wall_id) {
            wall.add_opening(sketchcad_geom::OpeningVoid {
                offset_along_wall: start_offset,
                width,
                base_z: sill_height,
                height,
            });
        }

        let mut win = Window::new(wall_id, start_offset, sill_height, width, height, material, unit_price);
        win.set_transform(pos, dir);
        let id = self.add_element(win);
        self.sync();
        Some(id)
    }

    /// Insert a Column at a world coordinate
    pub fn insert_column(
        &mut self,
        position: sketchcad_core::Point2D,
        width: f64,
        depth: f64,
        height: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let col = Column::new(position, width, depth, height, material);
        let id = self.add_element(col);
        self.sync();
        id
    }

    /// Insert a Floor Slab at a specified rectangular footprint
    pub fn insert_slab(
        &mut self,
        origin: sketchcad_core::Point2D,
        width: f64,
        depth: f64,
        thickness: f64,
        base_z: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let slab = Slab::new(origin, width, depth, thickness, base_z, material);
        let id = self.add_element(slab);
        self.sync();
        id
    }

    /// Insert a Suspended Ceiling at a specified rectangular boundary and elevation
    pub fn insert_ceiling(
        &mut self,
        origin: sketchcad_core::Point2D,
        width: f64,
        depth: f64,
        elevation: f64,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let ceiling = Ceiling::new(origin, width, depth, elevation, material);
        let id = self.add_element(ceiling);
        self.sync();
        id
    }

    /// Insert a Staircase flight between start and end coordinates
    pub fn insert_stairs(
        &mut self,
        start: sketchcad_core::Point2D,
        end: sketchcad_core::Point2D,
        width: f64,
        total_rise: f64,
        num_steps: usize,
        material: crate::Material,
    ) -> ElementId {
        self.touch();
        let stairs = Stairs::new(start, end, width, total_rise, num_steps, material);
        let id = self.add_element(stairs);
        self.sync();
        id
    }

    /// Push-pull extrusion on an existing element (Wall, Column, or Slab)
    pub fn push_pull_element(&mut self, id: ElementId, delta_height: f64) -> bool {
        self.touch();
        if let Some(wall) = self.get_wall_mut(id) {
            wall.height = (wall.height + delta_height).max(500.0);
            self.dependency_graph.mark_dirty(id);
            self.sync();
            return true;
        }
        if let Some(col) = self.get_column_mut(id) {
            col.height = (col.height + delta_height).max(500.0);
            self.dependency_graph.mark_dirty(id);
            self.sync();
            return true;
        }
        if let Some(slab) = self.get_slab_mut(id) {
            slab.thickness = (slab.thickness + delta_height).max(50.0);
            self.dependency_graph.mark_dirty(id);
            self.sync();
            return true;
        }
        false
    }

    /// Update/replace an existing element. Automatically marks it and its dependents dirty!
    pub fn update_element<E: BimElement + 'static>(&mut self, element: E) {
        self.touch();
        let id = element.id();
        self.elements.insert(id, Box::new(element));
        self.dependency_graph.mark_dirty(id);
        self.events.push(ChangeEvent::ElementModified(id));
    }

    /// Perform a reactive sync cycle: re-evaluates all dirty elements in topological order.
    pub fn sync(&mut self) -> Vec<ElementId> {
        self.touch();
        let order = self.dependency_graph.get_evaluation_order();
        for &id in &order {
            // Element re-evaluation logic (e.g. recalculate derived geometry or hosting cuts)
            self.dependency_graph.clear_dirty(id);
        }
        self.events.push(ChangeEvent::GlobalSyncRequired);
        order
    }

    /// Check if an element is visible: not hidden itself and its layer is on
    pub fn is_visible(&self, id: ElementId) -> bool {
        !self.hidden_elements.contains(&id) && self.layer_table.shows(&self.layer_of(id))
    }

    // ───────────────────────────── layers (圖層) ─────────────────────────────

    /// New drafting / annotations go on the current layer; BIM elements on their category
    /// layer (re-created if it was deleted or renamed).
    fn assign_new_element_layer(&mut self, id: ElementId, kind: ElementKind) {
        if self.layer_table.assignments.contains_key(&id) {
            return;
        }
        match crate::layers::default_layer_for(kind) {
            Some(default) => self.layer_table.ensure(default),
            None => {
                let current = self.layer_table.current.clone();
                self.layer_table.assignments.insert(id, current);
            }
        }
    }

    pub fn layer_table(&self) -> &crate::layers::LayerTable {
        &self.layer_table
    }

    pub fn layers(&self) -> &[crate::Layer] {
        &self.layer_table.layers
    }

    /// Mutable layer properties (colour, line type, line weight, transparency, on / off).
    pub fn layer_mut(&mut self, name: &str) -> Option<&mut crate::Layer> {
        self.touch();
        self.layer_table.get_mut(name)
    }

    /// Layer an element is on.
    pub fn layer_of(&self, id: ElementId) -> String {
        if let Some(layer) = self.layer_table.assignments.get(&id) {
            return layer.clone();
        }
        self.elements
            .get(&id)
            .and_then(|e| crate::layers::default_layer_for(e.kind()))
            .unwrap_or(crate::layers::LAYER_ZERO)
            .to_string()
    }

    /// Moves an element to `layer` (created if missing).
    pub fn set_layer_of(&mut self, id: ElementId, layer: &str) {
        self.touch();
        self.layer_table.ensure(layer);
        self.layer_table.assignments.insert(id, layer.to_string());
    }

    pub fn current_layer(&self) -> &str {
        &self.layer_table.current
    }

    pub fn set_current_layer(&mut self, layer: &str) {
        self.touch();
        self.layer_table.ensure(layer);
        self.layer_table.current = layer.to_string();
    }

    pub fn add_layer(&mut self, name: &str) -> Result<(), String> {
        self.touch();
        self.layer_table.add(name)
    }

    /// Pins elements that sit on `layer` only by default, so renaming / deleting the
    /// layer does not silently move them.
    fn pin_default_layer(&mut self, layer: &str) {
        let ids: Vec<ElementId> = self.elements.keys().copied().filter(|id| !self.layer_table.assignments.contains_key(id) && self.layer_of(*id) == layer).collect();
        for id in ids {
            self.layer_table.assignments.insert(id, layer.to_string());
        }
    }

    pub fn rename_layer(&mut self, old: &str, new: &str) -> Result<(), String> {
        self.touch();
        self.pin_default_layer(old);
        self.layer_table.rename(old, new)
    }

    /// Elements on a layer.
    pub fn objects_on_layer(&self, layer: &str) -> Vec<ElementId> {
        self.elements.keys().copied().filter(|id| self.layer_of(*id) == layer).collect()
    }

    /// Deletes an empty layer (not layer 0 or the current layer).
    pub fn delete_layer(&mut self, name: &str) -> Result<(), String> {
        if name == crate::layers::LAYER_ZERO {
            return Err("圖層 0 無法刪除".into());
        }
        if name == self.layer_table.current {
            return Err("無法刪除目前圖層，請先切換目前圖層".into());
        }
        let count = self.objects_on_layer(name).len();
        if count > 0 {
            return Err(format!("圖層「{name}」上還有 {count} 個物件 (可用 LAYDEL 連同物件刪除)"));
        }
        self.touch();
        self.layer_table.layers.retain(|l| l.name != name);
        Ok(())
    }

    /// LAYDEL: deletes a layer together with every object on it. Returns how many objects
    /// were removed.
    pub fn delete_layer_with_objects(&mut self, name: &str) -> Result<usize, String> {
        if name == crate::layers::LAYER_ZERO {
            return Err("圖層 0 無法刪除".into());
        }
        let ids = self.objects_on_layer(name);
        for id in &ids {
            self.remove_element(*id);
        }
        if self.layer_table.current == name {
            self.layer_table.current = crate::layers::LAYER_ZERO.to_string();
        }
        self.layer_table.layers.retain(|l| l.name != name);
        self.touch();
        Ok(ids.len())
    }

    /// LAYISO: shows only the layers of `ids`.
    pub fn isolate_layers_of(&mut self, ids: &[ElementId]) -> Vec<String> {
        let mut keep: Vec<String> = ids.iter().map(|id| self.layer_of(*id)).collect();
        keep.sort();
        keep.dedup();
        self.touch();
        self.layer_table.isolate(&keep);
        keep
    }

    /// LAYUNISO: restores the layers hidden by LAYISO.
    pub fn unisolate_layers(&mut self) -> bool {
        self.touch();
        self.layer_table.unisolate()
    }

    /// LAYWALK: temporarily shows only `layers` (`None` ends the preview).
    pub fn set_layer_preview(&mut self, layers: Option<HashSet<String>>) {
        if self.layer_table.preview != layers {
            self.touch();
            self.layer_table.preview = layers;
        }
    }

    /// COPYTOLAYER: copies the objects in place onto `layer`; returns the copies.
    pub fn copy_to_layer(&mut self, ids: &[ElementId], layer: &str) -> Vec<ElementId> {
        let mut copies = Vec::new();
        for id in ids {
            let Some(source) = self.get_element(*id).map(|e| e.clone_box()) else { continue };
            let copy = self.paste_element(source.as_ref(), sketchcad_core::Vector3D::ZERO);
            self.set_layer_of(copy, layer);
            copies.push(copy);
        }
        copies
    }

    /// Set element visibility (Photoshop-style layer show/hide)
    pub fn set_visible(&mut self, id: ElementId, visible: bool) {
        self.touch();
        if visible {
            self.hidden_elements.remove(&id);
        } else {
            self.hidden_elements.insert(id);
        }
        self.events.push(ChangeEvent::ElementModified(id));
    }

    /// Toggle element visibility
    pub fn toggle_visibility(&mut self, id: ElementId) -> bool {
        self.touch();
        let new_vis = !self.is_visible(id);
        self.set_visible(id, new_vis);
        new_vis
    }

    /// Unhide all elements
    pub fn show_all_elements(&mut self) {
        self.touch();
        self.hidden_elements.clear();
        self.events.push(ChangeEvent::GlobalSyncRequired);
    }

    /// Hide all elements
    pub fn hide_all_elements(&mut self) {
        self.touch();
        for id in self.elements.keys() {
            self.hidden_elements.insert(*id);
        }
        self.events.push(ChangeEvent::GlobalSyncRequired);
    }

    /// Retrieve all elements in the document
    pub fn all_elements(&self) -> Vec<&dyn BimElement> {
        self.elements.values().map(|b| b.as_ref()).collect()
    }

    /// Gathers all 2D CAD vector polylines for the plan view (respecting visibility)
    pub fn generate_2d_drawing(&self) -> Vec<Polyline2D> {
        let mut drawing = Vec::new();
        for element in self.elements.values() {
            if self.is_visible(element.id()) {
                drawing.extend(element.to_2d_primitives());
            }
        }
        drawing
    }

    /// Gathers and merges all 3D geometry into a composite scene mesh (respecting visibility)
    pub fn generate_3d_model(&self) -> TriangleMesh {
        let mut composite = TriangleMesh::new();
        for element in self.elements.values() {
            if self.is_visible(element.id()) {
                composite.append(&element.to_3d_mesh());
            }
        }
        composite
    }

    /// Aggregates Quantity Takeoff (QTO) across all smart objects into a master bill of quantities
    pub fn generate_master_qto(&self) -> QuantityTakeoff {
        let mut master_qto = QuantityTakeoff::new();
        for element in self.elements.values() {
            let element_qto = element.calculate_qto();
            master_qto.append(&element_qto);
        }
        master_qto
    }

    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Physical material of an element, for kinds that carry one.
    pub fn element_material(&self, id: ElementId) -> Option<&Material> {
        if let Some(e) = self.get_wall(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_column(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_slab(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_ceiling(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_beam(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_stairs(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_door(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_window(id) {
            Some(&e.frame_material)
        } else if let Some(e) = self.get_railing(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_roof(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_ramp(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_curtain_wall(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_mullion(id) {
            Some(&e.material)
        } else if let Some(e) = self.get_space(id) {
            Some(&e.floor_material)
        } else if let Some(e) = self.get_furniture(id) {
            Some(&e.material)
        } else {
            None
        }
    }

    /// Replaces an element's physical material (density / cost / QTO). Returns false for
    /// kinds without one. A painted element is repainted so its colour follows the material.
    pub fn set_element_material(&mut self, id: ElementId, material: Material) -> bool {
        self.touch();
        if self.painted.contains_key(&id) {
            self.painted.insert(id, material.clone());
        }
        let Some(elem) = self.elements.get_mut(&id) else {
            return false;
        };
        let any = elem.as_any_mut();
        macro_rules! slot {
            ($($ty:ty => $field:ident),+ $(,)?) => {
                $(if any.is::<$ty>() {
                    &mut any.downcast_mut::<$ty>().expect("type checked").$field
                } else)+ {
                    return false;
                }
            };
        }
        let slot = slot!(
            Wall => material, Column => material, Slab => material, Ceiling => material,
            Beam => material, Stairs => material, Door => material, Window => frame_material,
            Railing => material, Roof => material, Ramp => material, CurtainWall => material,
            Mullion => material, Space => floor_material, Furniture => material,
        );
        *slot = material;
        self.dependency_graph.mark_dirty(id);
        self.events.push(ChangeEvent::ElementModified(id));
        true
    }

    /// Paint bucket (油漆桶): applies `material` to the element's surfaces. The colour shows
    /// in the 3D view and colour exports; the element's physical material is replaced too
    /// when it has one. Returns false if the element does not exist.
    pub fn paint_element(&mut self, id: ElementId, material: Material) -> bool {
        if !self.elements.contains_key(&id) {
            return false;
        }
        self.painted.insert(id, material.clone());
        self.set_element_material(id, material);
        true
    }

    /// Removes the paint, restoring the element's default display colour.
    pub fn clear_paint(&mut self, id: ElementId) -> bool {
        self.touch();
        self.painted.remove(&id).is_some()
    }

    /// Paint applied to an element with [`Self::paint_element`].
    pub fn paint_of(&self, id: ElementId) -> Option<&Material> {
        self.painted.get(&id)
    }

    /// Display colour override of an element: its paint, else its source-model colour.
    /// `None` means the viewer's palette colour for the element kind.
    pub fn display_rgba(&self, id: ElementId) -> Option<[u8; 4]> {
        self.painted
            .get(&id)
            .map(Material::rgba)
            .or_else(|| self.get_element(id).and_then(|e| e.display_color()))
    }

    /// Built-in presets followed by the user's custom materials.
    pub fn material_library(&self) -> Vec<Material> {
        let mut library = Material::presets();
        library.extend(self.custom_materials.iter().cloned());
        library
    }

    pub fn custom_materials(&self) -> &[Material] {
        &self.custom_materials
    }

    /// Adds a material to the document's library, replacing a custom one of the same name.
    pub fn add_custom_material(&mut self, material: Material) {
        self.touch();
        match self.custom_materials.iter_mut().find(|m| m.name == material.name) {
            Some(existing) => *existing = material,
            None => self.custom_materials.push(material),
        }
    }

    /// Adds the materials of the user's own library that the document does not carry yet,
    /// so saved projects stay self-contained. Does not count as an edit of the model.
    pub fn merge_custom_materials(&mut self, materials: &[Material]) {
        for material in materials {
            if !self.custom_materials.iter().any(|m| m.name == material.name) {
                self.custom_materials.push(material.clone());
            }
        }
    }

    /// Edits the custom material called `old_name` (it may be renamed) and applies the new
    /// definition to every element whose material or paint is that material. Returns the
    /// number of elements updated.
    pub fn update_material(&mut self, old_name: &str, material: Material) -> usize {
        self.touch();
        match self.custom_materials.iter_mut().find(|m| m.name == old_name) {
            Some(existing) => *existing = material.clone(),
            None => self.custom_materials.push(material.clone()),
        }
        let users: Vec<ElementId> = self
            .elements
            .keys()
            .copied()
            .filter(|id| {
                self.painted.get(id).is_some_and(|m| m.name == old_name)
                    || self.element_material(*id).is_some_and(|m| m.name == old_name)
            })
            .collect();
        for id in &users {
            if let Some(paint) = self.painted.get_mut(id) {
                if paint.name == old_name {
                    *paint = material.clone();
                }
            }
            if self.element_material(*id).is_some_and(|m| m.name == old_name) {
                self.set_element_material(*id, material.clone());
            }
        }
        users.len()
    }

    /// Removes a custom material from the library; elements already painted with it keep it.
    pub fn remove_custom_material(&mut self, name: &str) -> bool {
        self.touch();
        let before = self.custom_materials.len();
        self.custom_materials.retain(|m| m.name != name);
        self.custom_materials.len() != before
    }

    pub fn get_elements_by_kind(&self, kind: ElementKind) -> Vec<&dyn BimElement> {
        self.elements
            .values()
            .filter(|e| e.kind() == kind)
            .map(|b| b.as_ref())
            .collect()
    }

    /// Convert the document state into a serializable SketchCadProject
    pub fn to_project(&self) -> SketchCadProject {
        let mut serializable_elements = Vec::new();
        for elem in self.elements.values() {
            if let Some(ser) = element_to_serializable(elem.as_ref()) {
                serializable_elements.push(ser);
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        SketchCadProject {
            format_version: 1,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            name: self.name.clone(),
            created_at: now.clone(),
            modified_at: now,
            elements: serializable_elements,
            hidden_elements: self.hidden_elements.clone(),
            storeys: self.storeys.clone(),
            custom_materials: self.custom_materials.clone(),
            painted: self
                .painted
                .iter()
                .map(|(id, material)| PaintedElement { element: *id, material: material.clone() })
                .collect(),
            custom_models: self.custom_models_in_use(),
            layers: self.layer_table.layers.clone(),
            layer_assignments: self
                .layer_table
                .assignments
                .iter()
                .map(|(id, layer)| crate::project::LayerAssignment { element: *id, layer: layer.clone() })
                .collect(),
            current_layer: self.layer_table.current.clone(),
        }
    }

    /// User library models referenced by placed model components.
    pub fn custom_models_in_use(&self) -> Vec<crate::CustomModel> {
        let mut models: Vec<crate::CustomModel> = Vec::new();
        for elem in self.get_elements_by_kind(ElementKind::Model) {
            let Some(model) = elem.as_any().downcast_ref::<ModelComponent>().and_then(|m| m.model()) else { continue };
            if let crate::ModelRef::Custom(entry) = model {
                if !models.iter().any(|m| m.id == entry.model.id) {
                    models.push(entry.model.clone());
                }
            }
        }
        models
    }

    /// World mesh of an element with every triangle coloured as displayed (paint, model
    /// colours or the kind's palette), e.g. to turn elements into a library model.
    pub fn element_mesh_with_colors(&self, id: ElementId) -> Option<TriangleMesh> {
        let elem = self.get_element(id)?;
        let mut mesh = elem.to_3d_mesh();
        let base = self.display_rgba(id).unwrap_or_else(|| elem.kind().display_rgba());
        mesh.face_colors = (0..mesh.triangle_count()).map(|i| Some(mesh.face_color(i).unwrap_or(base))).collect();
        Some(mesh)
    }

    /// Reconstruct a BimDocument from a SketchCadProject
    pub fn from_project(project: SketchCadProject) -> Self {
        for model in project.custom_models {
            crate::register_custom_model(model);
        }
        let mut doc = Self::new(project.name);
        for ser_elem in project.elements {
            doc.add_boxed_element(ser_elem.into_boxed_element());
        }
        doc.hidden_elements = project.hidden_elements;
        doc.custom_materials = project.custom_materials;
        // Layers: projects from before layers keep the defaults; old drafting layer names migrate
        if !project.layers.is_empty() {
            doc.layer_table.layers = project.layers;
        }
        doc.layer_table.ensure(crate::layers::LAYER_ZERO);
        doc.layer_table.assignments = project
            .layer_assignments
            .into_iter()
            .filter(|a| doc.elements.contains_key(&a.element))
            .map(|a| (a.element, a.layer))
            .collect();
        let legacy: Vec<(ElementId, String)> = doc
            .elements
            .values()
            .filter_map(|e| e.as_any().downcast_ref::<DraftShape>())
            .filter(|d| d.layer != crate::layers::LAYER_ZERO && !doc.layer_table.assignments.contains_key(&d.id))
            .map(|d| (d.id, d.layer.clone()))
            .collect();
        for (id, layer) in legacy {
            doc.layer_table.ensure(&layer);
            doc.layer_table.assignments.insert(id, layer);
        }
        let names: Vec<String> = doc.layer_table.assignments.values().cloned().collect();
        for name in names {
            doc.layer_table.ensure(&name);
        }
        if !project.current_layer.is_empty() {
            doc.layer_table.ensure(&project.current_layer);
            doc.layer_table.current = project.current_layer;
        }
        doc.painted = project
            .painted
            .into_iter()
            .filter(|p| doc.elements.contains_key(&p.element))
            .map(|p| (p.element, p.material))
            .collect();
        doc.set_storeys(project.storeys);
        doc.sync();
        doc
    }

    /// Save current BIM project to a `.skc` or `.json` file on disk
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let project = self.to_project();
        project.save_to_file(path)
    }

    /// Load a BIM project from a `.skc` or `.json` file on disk
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let project = SketchCadProject::load_from_file(path)?;
        Ok(Self::from_project(project))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketchcad_core::Point2D;
    use crate::material::Material;

    #[test]
    fn test_bim_document_interactive_features() {
        let mut doc = BimDocument::new("Test Building");

        // 1. Add a Wall
        let wall = Wall::new(Point2D::new(0.0, 0.0), Point2D::new(6000.0, 0.0), 200.0, 3000.0, Material::concrete());
        let wall_id = doc.add_element(wall);
        assert_eq!(doc.element_count(), 1);

        // 2. Insert Column
        let col_id = doc.insert_column(Point2D::new(0.0, 0.0), 500.0, 500.0, 3500.0, Material::concrete());
        assert_eq!(doc.element_count(), 2);
        let col = doc.get_column(col_id).expect("Column should be retrievable");
        assert_eq!(col.width, 500.0);

        // 3. Insert Door with opening cutout
        let door_id = doc.insert_door(wall_id, 2000.0, 900.0, 2100.0, Material::wood(), 8500.0).expect("Door inserted");
        assert_eq!(doc.element_count(), 3);
        let updated_wall = doc.get_wall(wall_id).expect("Wall should exist");
        assert_eq!(updated_wall.openings.len(), 1, "Wall should have 1 opening void for door");
        let door = doc.get_door(door_id).expect("Door should exist");
        assert_eq!(door.width, 900.0);

        // 4. Insert Window with opening cutout
        let win_id = doc.insert_window(wall_id, 4500.0, 900.0, 1200.0, 1500.0, Material::glass(), 12000.0).expect("Window inserted");
        assert_eq!(doc.element_count(), 4);
        let updated_wall = doc.get_wall(wall_id).expect("Wall should exist");
        assert_eq!(updated_wall.openings.len(), 2, "Wall should have 2 opening voids");
        let win = doc.get_window(win_id).expect("Window should exist");
        assert_eq!(win.width, 1200.0);

        // 5. Push-Pull element
        let extruded = doc.push_pull_element(wall_id, 500.0);
        assert!(extruded);
        let updated_wall = doc.get_wall(wall_id).expect("Wall should exist");
        assert_eq!(updated_wall.height, 3500.0);

        // 6. Insert Slab
        let slab_id = doc.insert_slab(Point2D::new(0.0, 0.0), 6000.0, 4000.0, 200.0, 0.0, Material::concrete());
        assert_eq!(doc.element_count(), 5);
        let slab = doc.get_slab(slab_id).expect("Slab should exist");
        assert_eq!(slab.area_m2(), 24.0);

        // 7. Insert Ceiling
        let ceil_id = doc.insert_ceiling(Point2D::new(0.0, 0.0), 6000.0, 4000.0, 2800.0, Material::gypsum());
        assert_eq!(doc.element_count(), 6);
        let ceil = doc.get_ceiling(ceil_id).expect("Ceiling should exist");
        assert_eq!(ceil.elevation, 2800.0);

        // 8. Insert Stairs
        let stair_id = doc.insert_stairs(Point2D::new(0.0, 0.0), Point2D::new(3000.0, 0.0), 1000.0, 3000.0, 15, Material::concrete());
        assert_eq!(doc.element_count(), 7);
        let stairs = doc.get_stairs(stair_id).expect("Stairs should exist");
        assert_eq!(stairs.num_steps, 15);

        // 9. Insert Railing
        let railing_id = doc.insert_railing(
            sketchcad_core::Point3D::new(0.0, 500.0, 0.0),
            sketchcad_core::Point3D::new(3000.0, 500.0, 3000.0),
            1100.0,
            Material::structural_steel(),
        );
        assert_eq!(doc.element_count(), 8);
        let railing = doc.get_railing(railing_id).expect("Railing should exist");
        assert_eq!(railing.height, 1100.0);

        // 10. Test Move element
        let moved = doc.move_element(col_id, sketchcad_core::Vector3D::new(100.0, 200.0, 0.0));
        assert!(moved);
        let moved_col = doc.get_column(col_id).expect("Column exists");
        assert_eq!(moved_col.position.x, 100.0);
        assert_eq!(moved_col.position.y, 200.0);

        // 11. Test Remove element
        let removed = doc.remove_element(col_id);
        assert!(removed);
        assert_eq!(doc.element_count(), 7);
        assert!(doc.get_element(col_id).is_none());

        // 12. Test Clone & Snapshot Undo/Redo
        let snapshot = doc.clone();
        assert_eq!(snapshot.element_count(), doc.element_count());

        // 13. Test Copy & Paste Element
        let slab_copy = doc.get_element(slab_id).expect("Slab exists").clone_box();
        let pasted_id = doc.paste_element(slab_copy.as_ref(), sketchcad_core::Vector3D::new(1000.0, 1000.0, 0.0));
        assert_ne!(pasted_id, slab_id);
        assert_eq!(doc.element_count(), 8);
        let pasted_slab = doc.get_slab(pasted_id).expect("Pasted slab exists");
        assert_eq!(pasted_slab.origin.x, 1000.0);
        assert_eq!(pasted_slab.origin.y, 1000.0);

        // 14. Restore snapshot (Undo test)
        doc = snapshot;
        assert_eq!(doc.element_count(), 7);
        assert!(doc.get_element(pasted_id).is_none());

        // 15. Global sync & 2D / 3D / QTO generation
        let drawing = doc.generate_2d_drawing();
        assert!(!drawing.is_empty(), "2D drawing polylines should be generated");

        let mesh = doc.generate_3d_model();
        assert!(!mesh.positions.is_empty(), "3D composite mesh should be generated");

        let qto = doc.generate_master_qto();
        assert!(qto.total_cost() > 0.0, "Total cost should be greater than zero");

        // 16. Test Project Save & Load (.skc round-trip)
        let temp_file = std::env::temp_dir().join("test_roundtrip.skc");
        doc.save_to_file(&temp_file).expect("Failed to save project file");
        assert!(temp_file.exists());

        let loaded_doc = BimDocument::load_from_file(&temp_file).expect("Failed to load project file");
        assert_eq!(loaded_doc.element_count(), doc.element_count());
        assert_eq!(loaded_doc.name, doc.name);
        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn paint_bucket_applies_material_and_survives_paste_and_save() {
        use sketchcad_geom::Color;

        let mut doc = BimDocument::new("Paint");
        let wall_id = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(4000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        assert_eq!(doc.display_rgba(wall_id), None, "unpainted walls use the palette colour");

        let sage = Material::custom("Sage Paint 灰綠漆", Color::rgb(140, 160, 130));
        doc.add_custom_material(sage.clone());
        assert_eq!(doc.material_library().last(), Some(&sage));

        assert!(doc.paint_element(wall_id, sage.clone()));
        assert_eq!(doc.display_rgba(wall_id), Some([140, 160, 130, 255]));
        assert_eq!(doc.element_material(wall_id), Some(&sage), "physical material follows the paint");

        // Choosing another material for a painted element repaints it
        assert!(doc.set_element_material(wall_id, Material::brick()));
        assert_eq!(doc.paint_of(wall_id), Some(&Material::brick()));
        doc.paint_element(wall_id, sage.clone());

        let copy_src = doc.get_element(wall_id).unwrap().clone_box();
        let copy_id = doc.paste_element(copy_src.as_ref(), sketchcad_core::Vector3D::new(0.0, 1000.0, 0.0));
        assert_eq!(doc.paint_of(copy_id), Some(&sage));

        let loaded = BimDocument::from_project(doc.to_project());
        assert_eq!(loaded.paint_of(wall_id), Some(&sage));
        assert_eq!(loaded.custom_materials(), &[sage.clone()]);

        assert!(doc.clear_paint(wall_id));
        assert_eq!(doc.display_rgba(wall_id), None);
        doc.remove_element(copy_id);
        assert_eq!(doc.paint_of(copy_id), None);
    }

    #[test]
    fn editing_a_library_material_updates_every_element_using_it() {
        use sketchcad_geom::Color;

        let mut doc = BimDocument::new("Library");
        let painted = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(4000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        let plain = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 4000.0), 200.0, 3000.0, Material::concrete()));
        let untouched = doc.add_element(Wall::new(Point2D::new(4000.0, 0.0), Point2D::new(4000.0, 4000.0), 200.0, 3000.0, Material::concrete()));

        let sage = Material::custom("Sage 灰綠漆", Color::rgb(140, 160, 130));
        doc.merge_custom_materials(&[sage.clone()]);
        doc.merge_custom_materials(&[sage.clone()]);
        assert_eq!(doc.custom_materials(), &[sage.clone()], "merging is idempotent");

        doc.paint_element(painted, sage.clone());
        doc.set_element_material(plain, sage.clone());

        let mut olive = sage.clone();
        olive.name = "Olive 橄欖綠漆".to_string();
        olive.color = Color::rgb(110, 120, 60);
        assert_eq!(doc.update_material(&sage.name, olive.clone()), 2);
        assert_eq!(doc.custom_materials(), &[olive.clone()]);
        assert_eq!(doc.paint_of(painted), Some(&olive));
        assert_eq!(doc.element_material(painted), Some(&olive));
        assert_eq!(doc.element_material(plain), Some(&olive));
        assert_eq!(doc.paint_of(plain), None, "an unpainted element stays unpainted");
        assert_eq!(doc.element_material(untouched), Some(&Material::concrete()));
    }

    #[test]
    fn library_models_are_placed_and_saved() {
        let mut doc = BimDocument::new("Models");
        assert!(doc.insert_model("no_such_model", Point2D::ZERO, 0.0).is_none());
        let tree = doc.insert_model("broadleaf_tree", Point2D::new(3000.0, 0.0), 0.0).unwrap();
        let car = doc.insert_model("sedan", Point2D::new(0.0, 5000.0), 90.0).unwrap();
        doc.get_model_mut(car).unwrap().scale = 1.2;
        doc.paint_element(car, Material::custom("Pearl White 珍珠白", sketchcad_geom::Color::rgb(240, 240, 235)));

        let loaded = BimDocument::from_project(doc.to_project());
        let saved_car = loaded.get_model(car).expect("model survives save / load");
        assert_eq!((saved_car.model_id.as_str(), saved_car.rotation_deg, saved_car.scale), ("sedan", 90.0, 1.2));
        assert_eq!(loaded.display_rgba(car), Some([240, 240, 235, 255]), "paint recolours the body");
        assert_eq!(loaded.display_rgba(tree), crate::ModelSpec::find("broadleaf_tree").map(|s| s.body_color));
        assert_eq!(loaded.get_element(tree).unwrap().to_3d_mesh(), doc.get_element(tree).unwrap().to_3d_mesh());
        assert!(doc.to_project().custom_models.is_empty(), "built-in models are not embedded");
    }

    #[test]
    fn models_and_furniture_share_a_movable_placement() {
        let mut doc = BimDocument::new("Placement");
        let model = doc.insert_model("fridge", Point2D::new(0.0, 0.0), 0.0).unwrap();
        let sofa = doc.insert_furniture(crate::FurnitureKind::Sofa, Point2D::new(1000.0, 0.0), 0.0, 0.0, Material::fabric());
        let wall = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(2000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        assert!(doc.placement(wall).is_none() && !doc.set_placement(wall, Point2D::ZERO, 0.0, 0.0));

        let rev = doc.revision();
        assert!(doc.set_placement(sofa, Point2D::new(5.0, 6.0), 400.0, -90.0));
        assert_ne!(doc.revision(), rev, "moving marks the model changed");
        let p = doc.placement(sofa).unwrap();
        assert_eq!((p.position, p.base_z, p.rotation_deg, p.size), (Point2D::new(5.0, 6.0), 400.0, 270.0, (2200.0, 900.0, 850.0)));
        assert!(doc.set_placement(model, Point2D::new(1.0, 2.0), 3.0, 45.0));
        assert_eq!(doc.get_model(model).unwrap().position, Point2D::new(1.0, 2.0));
    }

    #[test]
    fn modify_transforms_carry_hosted_doors_and_respect_axis_aligned_kinds() {
        use crate::Transform2D;
        let mut doc = BimDocument::new("Modify");
        let wall = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(4000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        let door = doc.insert_door(wall, 2000.0, 900.0, 2100.0, Material::wood(), 0.0).unwrap();
        let before = doc.get_door(door).unwrap().world_pos;

        doc.transform_element(wall, &Transform2D::rotation(Point2D::ZERO, 90.0)).unwrap();
        let w = doc.get_wall(wall).unwrap();
        assert!(w.end.distance(Point2D::new(0.0, 4000.0)) < 1e-6);
        let d = doc.get_door(door).unwrap();
        assert!(d.world_pos.distance(Point2D::new(-before.y, before.x)) < 1e-6, "the door turns with its wall");
        assert!((d.wall_dir - sketchcad_core::Vector2D::Y).length() < 1e-9);

        let column = doc.insert_column(Point2D::new(1000.0, 0.0), 600.0, 300.0, 3000.0, Material::concrete());
        assert!(doc.transform_element(column, &Transform2D::rotation(Point2D::ZERO, 30.0)).is_err());
        doc.transform_element(column, &Transform2D::rotation(Point2D::ZERO, 90.0)).unwrap();
        let c = doc.get_column(column).unwrap();
        assert_eq!((c.width, c.depth), (300.0, 600.0));

        let slab = doc.insert_slab(Point2D::new(0.0, 0.0), 2000.0, 1000.0, 200.0, 0.0, Material::concrete());
        doc.transform_element(slab, &Transform2D::mirror(Point2D::ZERO, Point2D::new(0.0, 1.0))).unwrap();
        let sl = doc.get_slab(slab).unwrap();
        assert_eq!((sl.origin, sl.width, sl.depth), (Point2D::new(-2000.0, 0.0), 2000.0, 1000.0));

        let sofa = doc.insert_furniture(crate::FurnitureKind::Sofa, Point2D::new(1000.0, 0.0), 0.0, 0.0, Material::fabric());
        doc.transform_element(sofa, &Transform2D::scale(Point2D::ZERO, 2.0)).unwrap();
        let f = doc.get_furniture(sofa).unwrap();
        assert_eq!((f.position, f.width), (Point2D::new(2000.0, 0.0), 4400.0));
        assert!(doc.transform_element(door, &Transform2D::rotation(Point2D::ZERO, 90.0)).is_err(), "hosted openings follow their wall only");

        // Trim pieces keep the line's pen
        let line = doc.insert_draft(crate::DraftGeometry::Line { start: Point2D::new(0.0, -5000.0), end: Point2D::new(1000.0, -5000.0) }, 0.0);
        doc.get_draft_mut(line).unwrap().color = Some([255, 0, 0, 255]);
        let ids = doc.replace_draft(line, vec![
            crate::DraftGeometry::Line { start: Point2D::new(0.0, -5000.0), end: Point2D::new(300.0, -5000.0) },
            crate::DraftGeometry::Line { start: Point2D::new(700.0, -5000.0), end: Point2D::new(1000.0, -5000.0) },
        ]);
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.get_draft(ids[1]).unwrap().color, Some([255, 0, 0, 255]));
        assert!(doc.replace_draft(ids[0], Vec::new()).is_empty() && doc.get_draft(ids[0]).is_none());
    }

    #[test]
    fn layers_group_elements_control_visibility_and_survive_save() {
        let mut doc = BimDocument::new("Layers");
        let wall = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(4000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        assert_eq!(doc.layer_of(wall), "A-牆體", "BIM elements go on their category layer");
        doc.add_layer("A-註記").unwrap();
        doc.set_current_layer("A-註記");
        let line = doc.insert_draft(crate::DraftGeometry::Line { start: Point2D::ZERO, end: Point2D::new(100.0, 0.0) }, 0.0);
        assert_eq!(doc.layer_of(line), "A-註記", "drafting goes on the current layer");

        // Layer off hides; isolate shows only the selection's layer; unisolate restores
        doc.layer_mut("A-牆體").unwrap().visible = false;
        assert!(!doc.is_visible(wall) && doc.is_visible(line));
        doc.layer_mut("A-牆體").unwrap().visible = true;
        doc.isolate_layers_of(&[line]);
        assert!(!doc.is_visible(wall) && doc.is_visible(line));
        assert!(doc.unisolate_layers());
        assert!(doc.is_visible(wall));

        // Renaming a default layer keeps its walls on it; new walls re-create the default
        doc.rename_layer("A-牆體", "牆").unwrap();
        assert_eq!(doc.layer_of(wall), "牆");
        let wall2 = doc.add_element(Wall::new(Point2D::new(0.0, 5000.0), Point2D::new(4000.0, 5000.0), 200.0, 3000.0, Material::concrete()));
        assert_eq!(doc.layer_of(wall2), "A-牆體");
        assert!(doc.layers().iter().any(|l| l.name == "A-牆體"));

        let copies = doc.copy_to_layer(&[wall], "A-註記");
        assert_eq!((copies.len(), doc.layer_of(copies[0]).as_str()), (1, "A-註記"));
        assert!(doc.delete_layer("A-註記").is_err(), "current / non-empty layers cannot be deleted in LA");

        // Save and load
        let loaded = BimDocument::from_project(doc.to_project());
        assert_eq!(loaded.layer_of(wall), "牆");
        assert_eq!(loaded.layer_of(line), "A-註記");
        assert_eq!(loaded.current_layer(), "A-註記");

        // LAYDEL removes the layer with its objects
        assert_eq!(doc.delete_layer_with_objects("A-註記").unwrap(), 2);
        assert!(doc.get_element(line).is_none() && doc.get_element(copies[0]).is_none());
        assert_eq!(doc.current_layer(), "0");
        assert!(doc.delete_layer_with_objects("0").is_err());
    }

    #[test]
    fn projects_carry_the_custom_models_they_use() {
        let mut doc = BimDocument::new("Custom");
        let wall = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(2000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        let mesh = doc.element_mesh_with_colors(wall).unwrap();
        assert!(mesh.face_colors.iter().all(Option::is_some));
        let model = crate::CustomModel::from_mesh("Wall block", crate::ModelCategory::Architecture, mesh);
        crate::register_custom_model(model.clone());
        let placed = doc.insert_model(&model.id, Point2D::new(5000.0, 0.0), 0.0).unwrap();

        let project = doc.to_project();
        assert_eq!(project.custom_models, vec![model.clone()]);
        // Opening the file on a machine without the model registers it again
        crate::unregister_custom_model(&model.id);
        let json = serde_json::to_string(&project).unwrap();
        let loaded = BimDocument::from_project(serde_json::from_str(&json).unwrap());
        assert!(crate::find_model(&model.id).is_some());
        assert_eq!(loaded.get_element(placed).unwrap().to_3d_mesh().triangle_count(), 12);
        crate::unregister_custom_model(&model.id);
    }

    #[test]
    fn outlets_and_av_equipment_round_trip() {
        use crate::{FurnitureKind, OutletVoltage, PlugStandard};
        use sketchcad_core::Vector2D;

        let mut doc = BimDocument::new("AV");
        let wall_id = doc.add_element(Wall::new(Point2D::new(0.0, 0.0), Point2D::new(4000.0, 0.0), 200.0, 3000.0, Material::concrete()));
        let outlet_id = doc
            .insert_outlet(wall_id, Point2D::new(1000.0, 100.0), Vector2D::Y, PlugStandard::Eu, OutletVoltage::V220, 1)
            .expect("outlet on wall");
        assert!(doc.insert_outlet(ElementId::new(), Point2D::ZERO, Vector2D::Y, PlugStandard::Us, OutletVoltage::V110, 2).is_none(), "needs a host wall");

        let tv_id = doc.insert_furniture(FurnitureKind::Television, Point2D::new(2000.0, 500.0), 0.0, 0.0, FurnitureKind::Television.default_material());
        let tv = doc.get_element(tv_id).unwrap();
        assert_eq!(tv.display_color(), Some(Material::black_aluminum().rgba()), "electronics use their material colour");
        let mesh = tv.to_3d_mesh();
        assert!(mesh.face_colors.iter().any(Option::is_some), "screen has its own colour");
        assert!(mesh.face_colors.iter().any(Option::is_none), "bezel takes the element colour");

        let loaded = BimDocument::from_project(doc.to_project());
        let outlet = loaded.get_outlet(outlet_id).expect("outlet survives save / load");
        assert_eq!((outlet.standard, outlet.voltage, outlet.gangs), (PlugStandard::Eu, OutletVoltage::V220, 1));
        assert_eq!(loaded.get_furniture(tv_id).map(|f| f.kind), Some(FurnitureKind::Television));
        assert!(loaded.generate_master_qto().total_cost() > 0.0);
    }
}

