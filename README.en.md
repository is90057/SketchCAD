# 📐 SketchCAD

**English** | [繁體中文](README.md)

> **A next-generation parametric Building Information Modeling (BIM) core, written in Rust.**
> Precise 2D vector drafting, interactive 3D solid modeling, smart parametric BIM elements and a
> reactive dependency engine that keeps every view in sync.

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-blue.svg)](#-license)
[![BIM Standard](https://img.shields.io/badge/OpenBIM-IFC4%20%7C%20BCF%202.1-green.svg)](https://www.buildingsmart.org/)

**Version:** 0.4 · **Developer:** James Chang

---

## 🌟 Design Philosophy

In traditional building design, **2D drawings**, the **3D model** and the **quantity / cost schedules**
drift apart. Changing one dimension means redrawing plans, elevations and sections by hand and
recalculating the budget.

**SketchCAD** is built from scratch in Rust on three pillars:

1. **Precise vector geometry**: double-precision (`f64`) coordinates, layers, endpoint snapping
   and engineering dimensions.
2. **Direct 3D solid modeling**: half-edge topology meshes and one-click **Push-Pull** extrusion
   of any planar polygon in 3D space.
3. **Smart BIM objects with reactive sync (SSOT)**: a directed acyclic dependency graph tracks
   every element. Any change to geometry or properties automatically recomputes all views
   (plan, 3D and schedules).

---

## ⚡ Features

### 1. 🔄 Reactive Dependency Graph
* **Single Source of Truth**: every solid, opening, annotation and quantity line is a node in
  the dependency graph.
* **Incremental topological updates**: moving a wall or resizing a door automatically
  re-evaluates the dependent opening cuts, plan symbols and cost schedule, in order.

### 2. 🏛️ Smart Parametric BIM Elements
Every element implements the common `BimElement` trait and provides a 2D drafting symbol, a 3D
solid mesh and an automatic quantity take-off (QTO):

* **Wall**: multi-layer composite materials, base elevation and thickness.
* **Column** and **Beam**: concrete / steel section specs, standard structural cross-hatch
  symbols and 3D solids.
* **Slab**: elevation, thickness, area and concrete pour volume.
* **Suspended ceiling**: clear height, hangers and board area.
* **Straight stairs**: risers and treads computed automatically, 25 mm nosings, stepped stringers
  and a 120 mm waist slab, so they read as real steps (never a ramp) from every angle.
* **Railing**: along a stair slope or a floor edge, with handrail, bottom rail, balusters and
  laminated safety-glass panels.
* **Door**: hosted in a wall, with a 2D 90° swing arc, a 3D door leaf and a boolean wall
  opening.
* **Window**: sill height, frame and double-glazed insulated units.
* **Gable roof**: eave overhang, ridge line, pitch (0°–60°) and insulation thickness.
* **Accessible ramp**: code-compliant 1:12 slope, direction arrow and anti-slip surface.
* **Unitized curtain wall**: aluminium mullions, transom grid and Low-E insulated glass.
* **Spaces / rooms**: living room, bedrooms, kitchen, bathroom and more, with net area,
  **Taiwanese *ping* (坪 = m² × 0.3025)**, skirting perimeter and room volume.
* **Detailed interior furniture (FF&E)**: multi-part parametric models instead of plain boxes:
  double bed, wardrobe, dining table with six chairs, kitchen cabinets with sink and hob,
  toilet, sofa, and a study desk with an ergonomic office chair.
* **Lighting fixtures**: downlights, pendants, track spots, ceiling lights and LED strips with
  detailed housings, plus a soft floor illumination pool tinted by colour temperature (Kelvin).

### 3. 🖥️ Graphical User Interface
* **GPU 3D viewport (wgpu)**:
  * Real depth buffer (reverse-Z) with 4× MSAA, so faces never pop through or overlap wrongly.
  * Crisp screen-space feature edges with adjustable width. Coplanar triangulation lines are
    hidden.
  * Supersampling (1.0×, 1.5× or 2.0×) and three shading modes: realistic materials,
    hidden-line blueprint and X-ray.
  * Correctly ordered translucency for glass and the optional see-through ceiling.
  * **Real-time section box** on the X, Y or Z axis, cut per pixel, to look inside at
    interiors, furniture and circulation.
  * Navigation ViewCube, click-to-pick selection and smooth wheel or pinch zoom.
  * Geometry is cached on the GPU and rebuilt only when the model changes.
* **2D floor-plan sheets**:
  * **One sheet per building storey**, built from the storeys stored in IFC files. With
    several storeys, all sheets are tiled side by side; pick a single sheet to edit it.
  * Each storey is a true plan section at 1.2 m above the floor: thick lines for cut walls and
    columns, thin lines for visible geometry below the cut, and dashed lines for roofs,
    ceilings and beams overhead.
  * Room tags with area in m² and ping, plus architectural dimension lines.
* **Navigation**:
  * Zoom at the cursor with the mouse wheel, pinch, Ctrl/⌘ + scroll, or `+` / `-`.
  * Pan with middle- or right-drag, Space + left-drag, two-finger trackpad scroll, the
    arrow keys or the ✋ hand tool.
  * `0` / Home fits the drawing.
* **Sheet tabs**: switch between the 2D plan, the 3D model and a 2D + 3D split view.
* **Visual Studio–style property grid**:
  * Categorized properties: geometry, placement and constraints, materials, cost and
    quantities, and BIM identity.
  * Live value editors (DragValue / ComboBox), categorized or alphabetical sorting and a
    search filter.
  * A help pane that explains each BIM parameter.
* **Project explorer with layer visibility**:
  * Elements grouped by discipline in a tree.
  * Photoshop-style 👁 / ⊘ toggles that show or hide elements in both 2D and 3D at once.
* **About dialog** (menu *關於 / About*) showing the version and the developer.

### 4. 🛠️ Geometry Kernel
* **Push-Pull extrusion**: Newell's method gives the normal of any tilted planar polygon.
  The side quads and the top and bottom caps form a strict **watertight solid**.
* **Ear-clipping triangulation**: concave (L- or U-shaped) profiles and IFC polygon faces
  triangulate correctly, with no overlapping or flipped triangles.
* **CSG boolean subtraction**: inserting a door or window splits the host wall into segments,
  a sill and a lintel, leaving a real opening through the 3D solid.
* **Net quantity take-off**: openings are deducted, which gives exact net concrete volume and
  both-side plaster / formwork areas.

### 5. 🌐 OpenBIM & CAD Interoperability
| Format | Support | Notes |
| :---: | :---: | :--- |
| **SKC (.skc)** | **Save & open** | Native SketchCAD project format. Keeps every parametric element, material, layer visibility, storey and the quantity schedule. `Ctrl+S` / `Ctrl+Shift+S`. |
| **IFC4 (.ifc)** | **Import & export** | ISO-10303-21 STEP. Round-trips `IFCWALL`, `IFCDOOR`, `IFCWINDOW`, `IFCCOLUMN`, `IFCBEAM`, `IFCSLAB`, `IFCCOVERING` and `IFCSTAIR` with 22-character IFC GUIDs. Building storeys become floor-plan sheets. |
| **DXF (.dxf)** | **Export & open** | Standard vector entities (LINE, LWPOLYLINE), converted into editable smart BIM walls on open. |
| **DWG (.dwg)** | **Native binary reader** | Pure-Rust bit-stream decoder. 2D CAD plan entities become 3D BIM elements on open. |
| **BCF 2.1 (.bcf / .bcfzip)** | **Export & open** | BIM collaboration issues with viewpoints, status tracking and element references. |

---

## 🏗️ Architecture (Cargo Workspace)

```text
SketchCAD/
├── crates/
│   ├── sketchcad-core/      # Math, units (mm / m / ping), UUIDs, reactive dependency graph (DAG)
│   ├── sketchcad-geom/      # 2D sketches, half-edge meshes, triangulation, Push-Pull, boolean openings
│   ├── sketchcad-bim/       # Smart elements, storeys, material library, quantity take-off (QTO)
│   ├── sketchcad-io/        # DXF, DWG, IFC4 STEP import/export, BCF packages
│   ├── sketchcad-render/    # 2D drafting camera and 3D orbit camera transforms
│   └── sketchcad-app/       # Desktop app (eframe + egui + wgpu) and CLI pipeline
├── samples/                 # Sample DWG / DXF / IFC / BCF / SKC files
├── exports/                 # Output of export runs (.dxf, .ifc, .bcfzip)
└── Cargo.toml               # Workspace manifest
```

---

## 🚀 Getting Started

### Requirements
* [Rust](https://www.rust-lang.org/) 1.80.0 or newer (2021 edition).
* A GPU supported by `wgpu` (Metal, Vulkan, DirectX 12 or OpenGL).

### 1. Run the test suite
Runs all unit tests, covering dependency propagation, IFC GUID round-trips, boolean openings,
triangulation and STEP parsing:
```bash
cargo test --workspace
```

### 2. Launch the desktop GUI
Opens the `wgpu` + `egui` application with the 2D plan canvas, the 3D orbit viewport and the
live quantity panel:
```bash
cargo run -p sketchcad-app
```
For the smoothest interaction with large IFC models, use a release build:
```bash
cargo run -p sketchcad-app --release
```
Sample files are created in `samples/` on first launch. Open them from
*檔案 (File) → 載入範例 (Samples)*.

### 3. Headless CLI pipeline
Verifies the full pipeline without a GUI (DWG decoding, IFC round-trip, boolean cuts and
quantities), for example on a CI server:
```bash
cargo run -p sketchcad-app -- --cli
```

---

## 🗺️ Roadmap

- [x] **Phase 1: Core engine**
  - [x] Multi-crate workspace with math and unit models
  - [x] Reactive dependency graph (DAG)
  - [x] First smart elements (wall, door, window, column, beam)
  - [x] Quantity take-off (QTO) and material library
- [x] **Phase 2: Geometry algorithms & OpenBIM**
  - [x] STEP ISO-10303-21 lexer and IFC4 importer
  - [x] Push-Pull on arbitrary 3D planes
  - [x] CSG boolean openings with net quantity deduction
  - [x] DXF and BCF 2.1 read / write
  - [x] Native pure-Rust DWG reader (R13 to 2024+ version detection)
- [x] **Phase 3: Graphical user interface (wgpu + egui)**
  - [x] `wgpu` + `egui` (`eframe`) integration
  - [x] 2D drafting canvas (live wall drawing, grid, crosshair, snapping)
  - [x] 3D orbit viewport with shading and live opening previews
  - [x] BIM property inspector and live quantity schedule
  - [x] GPU depth-buffered 3D renderer with MSAA and per-pixel section cut
  - [x] Per-storey floor-plan sheets with plan-section line weights
  - [x] Zoom at cursor, free panning and trackpad gestures
- [ ] **Phase 4: Construction documents & coordination**
  - [ ] Automatic sections and elevations
  - [ ] Clash detection with automatic BCF issue reporting

---

## 📄 License

SketchCAD is dual-licensed under **MIT**.
