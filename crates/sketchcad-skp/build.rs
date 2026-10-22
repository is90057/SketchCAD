//! Compiles the vendored OpenSKP C++17 sources (MIT, https://github.com/iamahsanmehmood/openskp),
//! its miniz dependency and the C shim into one static library.

use std::path::Path;

const OPENSKP_SOURCES: &[&str] = &[
    "core", "create", "earcut", "edit", "errors", "face_groups", "geometry", "instanced_scene",
    "legacy", "model", "observability", "pages_dimensions", "parser", "scene", "tlv", "transforms",
    "triangulator", "vff",
];
const MINIZ_SOURCES: &[&str] = &["miniz", "miniz_tdef", "miniz_tinfl", "miniz_zip"];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let openskp = root.join("vendor/openskp");
    let miniz = root.join("vendor/miniz");

    let mut c = cc::Build::new();
    c.files(MINIZ_SOURCES.iter().map(|f| miniz.join(format!("{f}.c"))))
        .include(&miniz)
        .warnings(false);
    c.compile("openskp_miniz");

    let mut cpp = cc::Build::new();
    cpp.cpp(true)
        .std("c++17")
        .files(OPENSKP_SOURCES.iter().map(|f| openskp.join("src").join(format!("{f}.cpp"))))
        .file(root.join("cpp/skp_shim.cpp"))
        .include(root.join("cpp/include"))
        .include(openskp.join("include"))
        .include(openskp.join("src"))
        .include(&miniz)
        .define("OPENSKP_STATIC_DEFINE", None)
        .warnings(false);
    if cpp.get_compiler().is_like_msvc() {
        cpp.define("_CRT_SECURE_NO_WARNINGS", None).flag("/utf-8").flag("/bigobj");
    }
    cpp.compile("openskp");

    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=vendor");
}
