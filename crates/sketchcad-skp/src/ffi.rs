use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;

#[repr(C)]
struct SkpScene {
    _private: [u8; 0],
}

#[repr(C)]
struct SkpPrimView {
    positions: *const f32,
    position_count: usize,
    indices: *const u32,
    index_count: usize,
    geom_name: *const c_char,
    name: *const c_char,
    definition_name: *const c_char,
    layer: *const c_char,
    path: *const c_char,
    color: [f32; 4],
    layer_hidden: u8,
    uses_layer_color: u8,
}

#[repr(C)]
struct SkpMaterialDesc {
    name: *const c_char,
    rgba: [u8; 4],
}

#[repr(C)]
struct SkpFaceDesc {
    loop_sizes: *const u32,
    loop_count: usize,
    points: *const f64,
    soft_edges: u8,
}

#[repr(C)]
struct SkpGroupDesc {
    name: *const c_char,
    layer: i32,
    material: i32,
    faces: *const SkpFaceDesc,
    face_count: usize,
}

extern "C" {
    fn skp_scene_load(path: *const c_char, err: *mut c_char, err_len: usize) -> *mut SkpScene;
    fn skp_scene_free(scene: *mut SkpScene);
    fn skp_scene_prim_count(scene: *const SkpScene) -> usize;
    fn skp_scene_prim(scene: *const SkpScene, index: usize, out: *mut SkpPrimView) -> c_int;
    #[allow(clippy::too_many_arguments)]
    fn skp_write(
        path: *const c_char,
        materials: *const SkpMaterialDesc,
        material_count: usize,
        layers: *const *const c_char,
        layer_count: usize,
        groups: *const SkpGroupDesc,
        group_count: usize,
        skipped_faces: *mut usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
}

/// One baked, world-space triangle mesh of a SketchUp model (one per entity path and material).
#[derive(Debug, Clone)]
pub struct SkpMesh {
    /// Unique path of the mesh within the scene hierarchy
    pub geom_name: String,
    /// Instance / group name (may be empty)
    pub name: String,
    pub definition_name: String,
    /// SketchUp tag (layer)
    pub layer: String,
    pub layer_hidden: bool,
    /// Entity path of the placed instance ("ROOT / House / Door#1"), unique per placement
    pub path: String,
    /// The faces have no explicit material and show their tag colour (typically the back side
    /// of a face whose front is painted)
    pub uses_layer_color: bool,
    /// Base colour, linear RGBA 0..1 as reported by OpenSKP
    pub color: [f32; 4],
    /// Vertex positions as baked by OpenSKP's scene builder
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct SkpMaterial {
    pub name: String,
    pub rgba: [u8; 4],
}

/// A planar face: outer loop plus optional hole loops, points in inches (SketchUp units).
#[derive(Debug, Clone, Default)]
pub struct SkpFace {
    pub outer: Vec<[f64; 3]>,
    pub holes: Vec<Vec<[f64; 3]>>,
    /// Hide the edges this face introduces (for triangulated curved surfaces)
    pub soft_edges: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SkpGroup {
    pub name: String,
    /// Index into [`SkpModel::layers`]
    pub layer: Option<usize>,
    /// Index into [`SkpModel::materials`]
    pub material: Option<usize>,
    pub faces: Vec<SkpFace>,
}

#[derive(Debug, Clone, Default)]
pub struct SkpModel {
    pub materials: Vec<SkpMaterial>,
    pub layers: Vec<String>,
    pub groups: Vec<SkpGroup>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkpWriteReport {
    /// Faces OpenSKP could not write (e.g. degenerate or self-intersecting loops)
    pub skipped_faces: usize,
}

fn c_string(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn path_cstring(path: &Path) -> Result<CString, String> {
    let utf8 = path.to_str().ok_or_else(|| format!("路徑不是有效的 UTF-8：{}", path.display()))?;
    Ok(c_string(utf8))
}

fn error_text(buf: &[c_char]) -> String {
    // SAFETY: the shim always NUL-terminates the buffer it writes into (and it starts zeroed)
    let msg = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned();
    if msg.is_empty() { "unknown OpenSKP error".to_string() } else { msg }
}

unsafe fn owned_str(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// Reads a `.skp` file (SketchUp 2013+, both legacy and VFF containers) into baked meshes.
///
/// Positions are world space in metres with **Y up** (glTF convention, as baked by OpenSKP).
/// A face whose front and back materials differ appears twice (front and reversed back copy).
pub fn read_skp(path: &Path) -> Result<Vec<SkpMesh>, String> {
    let c_path = path_cstring(path)?;
    let mut err = vec![0 as c_char; 1024];
    // SAFETY: valid NUL-terminated path and error buffer of the stated length
    let scene = unsafe { skp_scene_load(c_path.as_ptr(), err.as_mut_ptr(), err.len()) };
    if scene.is_null() {
        return Err(error_text(&err));
    }

    let mut meshes = Vec::new();
    // SAFETY: `scene` is a live handle until `skp_scene_free`; views borrow its memory and are
    // copied into owned Rust values before the handle is freed.
    unsafe {
        for i in 0..skp_scene_prim_count(scene) {
            let mut view = std::mem::zeroed::<SkpPrimView>();
            if skp_scene_prim(scene, i, &mut view) == 0 {
                continue;
            }
            let flat = std::slice::from_raw_parts(view.positions, view.position_count * 3);
            meshes.push(SkpMesh {
                geom_name: owned_str(view.geom_name),
                name: owned_str(view.name),
                definition_name: owned_str(view.definition_name),
                layer: owned_str(view.layer),
                layer_hidden: view.layer_hidden != 0,
                path: owned_str(view.path),
                uses_layer_color: view.uses_layer_color != 0,
                color: view.color,
                positions: flat.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect(),
                indices: std::slice::from_raw_parts(view.indices, view.index_count).to_vec(),
            });
        }
        skp_scene_free(scene);
    }
    Ok(meshes)
}

/// Writes `model` as a new `.skp` file (legacy format, opens in SketchUp 2013 and later).
/// Coordinates are SketchUp model space: inches, **Z up**.
pub fn write_skp(path: &Path, model: &SkpModel) -> Result<SkpWriteReport, String> {
    let c_path = path_cstring(path)?;

    let material_names: Vec<CString> = model.materials.iter().map(|m| c_string(&m.name)).collect();
    let materials: Vec<SkpMaterialDesc> = model
        .materials
        .iter()
        .zip(&material_names)
        .map(|(m, name)| SkpMaterialDesc { name: name.as_ptr(), rgba: m.rgba })
        .collect();
    let layer_names: Vec<CString> = model.layers.iter().map(|l| c_string(l)).collect();
    let layers: Vec<*const c_char> = layer_names.iter().map(|l| l.as_ptr()).collect();

    // Flatten every face into owned buffers that outlive the FFI call
    struct FaceBuffers {
        loop_sizes: Vec<u32>,
        points: Vec<f64>,
        soft: bool,
    }
    let group_names: Vec<CString> = model.groups.iter().map(|g| c_string(&g.name)).collect();
    let face_buffers: Vec<Vec<FaceBuffers>> = model
        .groups
        .iter()
        .map(|g| {
            g.faces
                .iter()
                .map(|f| {
                    let loops = std::iter::once(&f.outer).chain(&f.holes);
                    let mut fb = FaceBuffers { loop_sizes: Vec::new(), points: Vec::new(), soft: f.soft_edges };
                    for lp in loops {
                        fb.loop_sizes.push(lp.len() as u32);
                        fb.points.extend(lp.iter().flatten());
                    }
                    fb
                })
                .collect()
        })
        .collect();
    let face_descs: Vec<Vec<SkpFaceDesc>> = face_buffers
        .iter()
        .map(|faces| {
            faces
                .iter()
                .map(|fb| SkpFaceDesc {
                    loop_sizes: fb.loop_sizes.as_ptr(),
                    loop_count: fb.loop_sizes.len(),
                    points: fb.points.as_ptr(),
                    soft_edges: fb.soft as u8,
                })
                .collect()
        })
        .collect();
    let groups: Vec<SkpGroupDesc> = model
        .groups
        .iter()
        .zip(&group_names)
        .zip(&face_descs)
        .map(|((g, name), faces)| SkpGroupDesc {
            name: name.as_ptr(),
            layer: g.layer.map_or(-1, |l| l as i32),
            material: g.material.map_or(-1, |m| m as i32),
            faces: faces.as_ptr(),
            face_count: faces.len(),
        })
        .collect();

    let mut skipped = 0usize;
    let mut err = vec![0 as c_char; 1024];
    // SAFETY: every pointer refers to buffers owned above that live until this call returns
    let ok = unsafe {
        skp_write(
            c_path.as_ptr(),
            materials.as_ptr(),
            materials.len(),
            layers.as_ptr(),
            layers.len(),
            groups.as_ptr(),
            groups.len(),
            &mut skipped,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if ok == 0 {
        return Err(error_text(&err));
    }
    Ok(SkpWriteReport { skipped_faces: skipped })
}
