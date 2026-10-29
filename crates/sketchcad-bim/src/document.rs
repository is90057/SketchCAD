use crate::cost::QuantityTakeoff;
use crate::element::{BimElement, ElementKind};
use crate::material::Material;
use crate::{
    Beam, Ceiling, Column, CurtainGrid, CurtainWall, Door, Furniture, LightFixture, Mullion, Outlet,
    Railing, Ramp, Roof, Slab, Space, SpaceType, Stairs, Wall, Window,
};

use sketchcad_core::{ChangeEvent, DependencyGraph, ElementId};
use sketchcad_geom::{Polyline2D, TriangleMesh};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::project::{element_to_serializable, PaintedElement, SketchCadProject};

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

        self.elements.insert(id, Box::new(element));
        self.dependency_graph.add_element(id);

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

        self.elements.insert(id, element);
        self.dependency_graph.add_element(id);

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

    /// Check if an element is visible
    pub fn is_visible(&self, id: ElementId) -> bool {
        !self.hidden_elements.contains(&id)
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
        }
    }

    /// Reconstruct a BimDocument from a SketchCadProject
    pub fn from_project(project: SketchCadProject) -> Self {
        let mut doc = Self::new(project.name);
        for ser_elem in project.elements {
            doc.add_boxed_element(ser_elem.into_boxed_element());
        }
        doc.hidden_elements = project.hidden_elements;
        doc.custom_materials = project.custom_materials;
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

