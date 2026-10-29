pub mod app;
pub mod doc_watch;
pub mod gpu_scene;
pub mod inspector;
pub mod material_library;
pub mod model_browser;
pub mod plan_sheets;
pub mod viewport_2d;
pub mod viewport_3d;

pub use app::SketchCadApp;

#[cfg(test)]
mod glyph_tests {
    /// Emoji in UI strings must exist in egui's bundled fonts, otherwise they
    /// render as empty boxes (e.g. 🧱, 🛋 or the U+FE0F emoji variation selector).
    /// CJK text comes from the system font and is not checked here.
    #[test]
    fn ui_symbols_have_glyphs_in_the_bundled_fonts() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let font = egui::FontId::proportional(13.0);
        // Pictographic emoji only exist in egui's emoji fonts (arrows and geometric symbols are
        // also covered by the system CJK font the app loads)
        let is_symbol = |c: char| matches!(c as u32, 0x1F000..=0x1FAFF | 0xFE0F);
        let mut files = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from("src/gui"), "../sketchcad-bim/src".into(), "../sketchcad-io/src".into()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let mut missing = Vec::new();
        for path in files {
            for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for c in line.chars().filter(|&c| is_symbol(c)) {
                    if !ctx.fonts(|f| f.has_glyph(&font, c)) {
                        missing.push(format!("U+{:04X} {c} at {}:{}", c as u32, path.display(), n + 1));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "symbols without a glyph:\n{}", missing.join("\n"));
    }
}
