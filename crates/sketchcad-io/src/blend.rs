//! Blender (`.blend`) import. The `.blend` format is a memory dump of Blender's internal
//! data structures that changes with every release, so instead of parsing it SketchCAD
//! asks an installed Blender to export the scene as OBJ (with materials) in the
//! background and reads that.

use std::path::{Path, PathBuf};
use std::process::Command;

use sketchcad_geom::TriangleMesh;

#[derive(Debug, thiserror::Error)]
pub enum BlendError {
    #[error("找不到 Blender。請安裝 Blender 3.x / 4.x (blender.org)，或以環境變數 SKETCHCAD_BLENDER 指定 blender 執行檔路徑")]
    BlenderNotFound,
    #[error("無法執行 Blender: {0}")]
    Launch(std::io::Error),
    #[error("Blender 轉換失敗: {0}")]
    Conversion(String),
    #[error("讀取轉換結果失敗: {0}")]
    Obj(#[from] crate::obj::ObjError),
}

/// Export script run inside Blender; the OBJ path follows `--` on the command line.
/// Blender ≥ 3.2 has the built-in `wm.obj_export`; older versions the Python add-on.
const EXPORT_SCRIPT: &str = r#"
import bpy, sys
out = sys.argv[sys.argv.index("--") + 1]
try:
    bpy.ops.wm.obj_export(filepath=out, export_selected_objects=False, apply_modifiers=True,
                          export_materials=True, export_uv=False, export_triangulated_mesh=True)
except AttributeError:
    bpy.ops.export_scene.obj(filepath=out, use_selection=False, use_mesh_modifiers=True,
                             use_materials=True, use_uvs=False, use_triangles=True)
"#;

/// Locates the Blender executable: `SKETCHCAD_BLENDER`, the PATH, then the default
/// install locations of each platform (newest version first).
pub fn find_blender() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SKETCHCAD_BLENDER").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(path);
    }
    let exe = if cfg!(windows) { "blender.exe" } else { "blender" };
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(exe)).find(|p| p.is_file())) {
        return Some(path);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push("/Applications/Blender.app/Contents/MacOS/Blender".into());
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(Path::new(&home).join("Applications/Blender.app/Contents/MacOS/Blender"));
        }
    } else if cfg!(windows) {
        for root in [std::env::var_os("ProgramFiles"), std::env::var_os("ProgramFiles(x86)")].into_iter().flatten() {
            let foundation = Path::new(&root).join("Blender Foundation");
            let mut versions: Vec<PathBuf> = std::fs::read_dir(&foundation)
                .map(|dir| dir.flatten().map(|e| e.path().join("blender.exe")).collect())
                .unwrap_or_default();
            versions.sort();
            candidates.extend(versions.into_iter().rev());
        }
    } else {
        candidates.extend(["/usr/bin/blender", "/usr/local/bin/blender", "/snap/bin/blender"].map(PathBuf::from));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Converts a `.blend` file to an OBJ mesh with Blender. Blender writes metres, Y up.
/// This blocks for a few seconds; run it off the UI thread.
pub fn read_blend(path: &Path) -> Result<TriangleMesh, BlendError> {
    let blender = find_blender().ok_or(BlendError::BlenderNotFound)?;
    let work_dir = std::env::temp_dir().join(format!("sketchcad-blend-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&work_dir).map_err(BlendError::Launch)?;
    let obj_path = work_dir.join("scene.obj");
    let output = Command::new(&blender)
        .arg("--background")
        .arg("--factory-startup")
        .arg(path)
        .args(["--python-exit-code", "1", "--python-expr", EXPORT_SCRIPT, "--"])
        .arg(&obj_path)
        .output()
        .map_err(BlendError::Launch);
    let result = output.and_then(|out| {
        if out.status.success() && obj_path.is_file() {
            Ok(crate::obj::read_obj(&obj_path)?)
        } else {
            let log = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
            let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
            Err(BlendError::Conversion(tail[tail.len().saturating_sub(4)..].join(" | ")))
        }
    });
    let _ = std::fs::remove_dir_all(&work_dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_blender_path_must_exist() {
        // A missing override falls through to the normal search instead of being used
        std::env::set_var("SKETCHCAD_BLENDER", "/definitely/not/blender");
        assert_ne!(find_blender(), Some(PathBuf::from("/definitely/not/blender")));
        std::env::remove_var("SKETCHCAD_BLENDER");
    }
}
