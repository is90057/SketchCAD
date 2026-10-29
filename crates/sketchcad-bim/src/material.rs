use std::path::Path;

use serde::{Deserialize, Serialize};
use sketchcad_geom::Color;

/// Material family used to group the material library (材質分類).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MaterialCategory {
    Concrete,
    Masonry,
    Stone,
    Tile,
    Wood,
    Board,
    Metal,
    Glass,
    Plastic,
    Paint,
    Fabric,
    #[default]
    Other,
}

impl MaterialCategory {
    pub const ALL: [Self; 12] = [
        Self::Concrete,
        Self::Masonry,
        Self::Stone,
        Self::Tile,
        Self::Wood,
        Self::Board,
        Self::Metal,
        Self::Glass,
        Self::Plastic,
        Self::Paint,
        Self::Fabric,
        Self::Other,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Concrete => "混凝土 / 水泥",
            Self::Masonry => "磚材 / 砌體",
            Self::Stone => "天然石材",
            Self::Tile => "磁磚",
            Self::Wood => "木材 / 木作",
            Self::Board => "板材 / 隔間",
            Self::Metal => "金屬",
            Self::Glass => "玻璃",
            Self::Plastic => "塑膠 / 地坪",
            Self::Paint => "塗料 / 油漆",
            Self::Fabric => "布料 / 皮革",
            Self::Other => "其他",
        }
    }
}

/// Building material definition with physical and financial properties
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub density_kg_m3: f64,
    pub cost_per_m3: f64, // Currency amount per cubic meter
    pub color: Color,
    pub thermal_conductivity: f64, // W/(m·K)
    #[serde(default)]
    pub category: MaterialCategory,
}

impl Material {
    fn spec(
        name: &str,
        category: MaterialCategory,
        density_kg_m3: f64,
        cost_per_m3: f64,
        color: Color,
        thermal_conductivity: f64,
    ) -> Self {
        Self {
            name: name.to_string(),
            density_kg_m3,
            cost_per_m3,
            color,
            thermal_conductivity,
            category,
        }
    }

    /// User-defined paint / finish material. Physical properties default to a light
    /// finishing layer and can be edited afterwards.
    pub fn custom(name: impl Into<String>, color: Color) -> Self {
        Self {
            name: name.into(),
            density_kg_m3: 1500.0,
            cost_per_m3: 20000.0,
            color,
            thermal_conductivity: 0.5,
            category: MaterialCategory::Other,
        }
    }

    /// Built-in material library (內建材質庫), ordered by category.
    pub fn presets() -> Vec<Self> {
        vec![
            // 混凝土 / 水泥
            Self::concrete(),
            Self::exposed_concrete(),
            Self::microcement(),
            Self::terrazzo(),
            Self::cement_screed(),
            // 磚材 / 砌體
            Self::brick(),
            Self::cultured_stone(),
            Self::aac_block(),
            // 天然石材
            Self::marble(),
            Self::granite(),
            Self::travertine(),
            Self::slate(),
            // 磁磚
            Self::polished_tile(),
            Self::wood_look_tile(),
            Self::subway_tile(),
            Self::mosaic_tile(),
            // 木材 / 木作
            Self::wood(),
            Self::wood_flooring(),
            Self::walnut(),
            Self::white_oak_veneer(),
            Self::melamine_board(),
            Self::bamboo(),
            // 板材 / 隔間
            Self::gypsum(),
            Self::calcium_silicate_board(),
            Self::plywood(),
            // 金屬
            Self::structural_steel(),
            Self::aluminum(),
            Self::black_aluminum(),
            Self::stainless_steel(),
            Self::galvanized_steel(),
            Self::brass(),
            // 玻璃
            Self::clear_glass(),
            Self::glass(),
            Self::low_e_glass(),
            Self::tinted_glass(),
            Self::frosted_glass(),
            Self::laminated_glass(),
            // 塑膠 / 地坪
            Self::upvc(),
            Self::vinyl_plank(),
            Self::epoxy_floor(),
            // 塗料 / 油漆
            Self::white_latex_paint(),
            Self::warm_grey_paint(),
            Self::sage_green_paint(),
            Self::morandi_blue_paint(),
            Self::matte_black_paint(),
            Self::diatomite_plaster(),
            // 布料 / 皮革
            Self::fabric(),
            Self::leather(),
            Self::wool_carpet(),
            Self::velvet(),
        ]
    }

    /// Whether `name` belongs to a built-in preset (custom materials may not reuse it).
    pub fn is_preset_name(name: &str) -> bool {
        Self::presets().iter().any(|m| m.name == name)
    }

    /// Display colour as sRGB RGBA bytes (alpha < 255 = translucent).
    pub fn rgba(&self) -> [u8; 4] {
        [self.color.r, self.color.g, self.color.b, self.color.a]
    }

    /// Window / door frame materials (窗框材質)
    pub fn window_frames() -> Vec<Self> {
        vec![
            Self::aluminum(),
            Self::black_aluminum(),
            Self::upvc(),
            Self::wood(),
            Self::structural_steel(),
        ]
    }

    /// Glazing types (玻璃種類)
    pub fn glazings() -> Vec<Self> {
        vec![
            Self::clear_glass(),
            Self::glass(),
            Self::low_e_glass(),
            Self::tinted_glass(),
            Self::frosted_glass(),
            Self::laminated_glass(),
        ]
    }

    // ── 混凝土 / 水泥 ── (costs approx. TWD/m3)

    pub fn concrete() -> Self {
        Self::spec("Reinforced Concrete 鋼筋混凝土", MaterialCategory::Concrete, 2400.0, 3200.0, Color::rgb(180, 180, 180), 1.7)
    }

    pub fn exposed_concrete() -> Self {
        Self::spec("Fair-faced Concrete 清水模", MaterialCategory::Concrete, 2400.0, 9800.0, Color::rgb(150, 152, 150), 1.7)
    }

    pub fn microcement() -> Self {
        Self::spec("Microcement 仿清水模微水泥", MaterialCategory::Concrete, 2100.0, 36000.0, Color::rgb(160, 165, 170), 1.2)
    }

    pub fn terrazzo() -> Self {
        Self::spec("Terrazzo 磨石子", MaterialCategory::Concrete, 2300.0, 42000.0, Color::rgb(215, 210, 200), 1.6)
    }

    pub fn cement_screed() -> Self {
        Self::spec("Cement Screed 水泥砂漿粉光", MaterialCategory::Concrete, 2000.0, 12000.0, Color::rgb(170, 168, 160), 1.4)
    }

    // ── 磚材 / 砌體 ──

    pub fn brick() -> Self {
        Self::spec("Red Brick 紅磚", MaterialCategory::Masonry, 1800.0, 2600.0, Color::rgb(190, 85, 65), 0.8)
    }

    pub fn cultured_stone() -> Self {
        Self::spec("Cultured Stone 文化石", MaterialCategory::Masonry, 1400.0, 48000.0, Color::rgb(200, 190, 175), 0.7)
    }

    pub fn aac_block() -> Self {
        Self::spec("AAC Block 輕質白磚", MaterialCategory::Masonry, 550.0, 4200.0, Color::rgb(232, 230, 224), 0.16)
    }

    // ── 天然石材 ──

    pub fn marble() -> Self {
        Self::spec("Carrara Marble 卡拉拉白大理石", MaterialCategory::Stone, 2700.0, 95000.0, Color::rgb(245, 248, 252), 2.8)
    }

    pub fn granite() -> Self {
        Self::spec("Black Granite 黑色花崗岩", MaterialCategory::Stone, 2750.0, 85000.0, Color::rgb(55, 55, 58), 2.9)
    }

    pub fn travertine() -> Self {
        Self::spec("Travertine 米黃洞石", MaterialCategory::Stone, 2500.0, 78000.0, Color::rgb(222, 205, 175), 2.3)
    }

    pub fn slate() -> Self {
        Self::spec("Slate 天然板岩", MaterialCategory::Stone, 2800.0, 70000.0, Color::rgb(85, 92, 98), 2.0)
    }

    // ── 磁磚 ──

    pub fn polished_tile() -> Self {
        Self::spec("Polished Tile 拋光石英磚", MaterialCategory::Tile, 2300.0, 28000.0, Color::rgb(230, 235, 240), 1.3)
    }

    pub fn wood_look_tile() -> Self {
        Self::spec("Wood-look Tile 木紋磚", MaterialCategory::Tile, 2300.0, 32000.0, Color::rgb(170, 130, 95), 1.3)
    }

    pub fn subway_tile() -> Self {
        Self::spec("Subway Tile 白色亮面壁磚", MaterialCategory::Tile, 2000.0, 30000.0, Color::rgb(248, 248, 245), 1.0)
    }

    pub fn mosaic_tile() -> Self {
        Self::spec("Mosaic Tile 藍綠馬賽克磚", MaterialCategory::Tile, 2200.0, 60000.0, Color::rgb(70, 150, 160), 1.0)
    }

    // ── 木材 / 木作 ──

    pub fn wood() -> Self {
        Self::spec("Solid Oak Timber 實木橡木", MaterialCategory::Wood, 720.0, 38000.0, Color::rgb(160, 120, 80), 0.13)
    }

    pub fn wood_flooring() -> Self {
        Self::spec("Oak Wood Flooring 超耐磨橡木地板", MaterialCategory::Wood, 750.0, 42000.0, Color::rgb(205, 165, 115), 0.14)
    }

    pub fn walnut() -> Self {
        Self::spec("Walnut Veneer 北美黑胡桃木", MaterialCategory::Wood, 680.0, 52000.0, Color::rgb(115, 75, 50), 0.12)
    }

    pub fn white_oak_veneer() -> Self {
        Self::spec("White Oak Veneer 白橡木皮", MaterialCategory::Wood, 700.0, 48000.0, Color::rgb(215, 190, 155), 0.13)
    }

    pub fn melamine_board() -> Self {
        Self::spec("Melamine Board 梧桐木紋系統板", MaterialCategory::Wood, 650.0, 26000.0, Color::rgb(190, 175, 150), 0.12)
    }

    pub fn bamboo() -> Self {
        Self::spec("Bamboo 竹材", MaterialCategory::Wood, 700.0, 36000.0, Color::rgb(205, 180, 120), 0.17)
    }

    // ── 板材 / 隔間 ──

    pub fn gypsum() -> Self {
        Self::spec("Gypsum Plaster Board 輕鋼架石膏板", MaterialCategory::Board, 850.0, 15000.0, Color::rgb(240, 240, 235), 0.25)
    }

    pub fn calcium_silicate_board() -> Self {
        Self::spec("Calcium Silicate Board 矽酸鈣板", MaterialCategory::Board, 1000.0, 18000.0, Color::rgb(236, 236, 230), 0.22)
    }

    pub fn plywood() -> Self {
        Self::spec("Plywood 木夾板", MaterialCategory::Board, 600.0, 22000.0, Color::rgb(210, 180, 135), 0.13)
    }

    // ── 金屬 ──

    pub fn structural_steel() -> Self {
        Self::spec("Structural Steel 結構鋼材", MaterialCategory::Metal, 7850.0, 65000.0, Color::rgb(100, 110, 120), 50.0)
    }

    pub fn aluminum() -> Self {
        Self::spec("Anodized Aluminum 鋁合金框", MaterialCategory::Metal, 2700.0, 120000.0, Color::rgb(190, 195, 200), 160.0)
    }

    pub fn black_aluminum() -> Self {
        Self::spec("Black Powder-Coated Aluminum 黑色烤漆鋁框", MaterialCategory::Metal, 2700.0, 135000.0, Color::rgb(45, 48, 52), 160.0)
    }

    pub fn stainless_steel() -> Self {
        Self::spec("Brushed Stainless Steel 髮絲紋不鏽鋼", MaterialCategory::Metal, 7900.0, 180000.0, Color::rgb(200, 203, 206), 16.0)
    }

    pub fn galvanized_steel() -> Self {
        Self::spec("Galvanized Steel Sheet 鍍鋅鋼板", MaterialCategory::Metal, 7850.0, 90000.0, Color::rgb(160, 168, 172), 50.0)
    }

    pub fn brass() -> Self {
        Self::spec("Brushed Brass 拉絲黃銅", MaterialCategory::Metal, 8500.0, 420000.0, Color::rgb(190, 155, 80), 110.0)
    }

    // ── 玻璃 ──

    pub fn clear_glass() -> Self {
        Self::spec("Clear Float Glass 單層清玻璃", MaterialCategory::Glass, 2500.0, 30000.0, Color::rgba(185, 225, 240, 110), 1.0)
    }

    pub fn glass() -> Self {
        Self::spec("Double Glazed Glass 雙層中空玻璃", MaterialCategory::Glass, 2500.0, 45000.0, Color::rgba(150, 220, 240, 160), 1.1)
    }

    pub fn low_e_glass() -> Self {
        Self::spec("Low-E Insulated Glass 低輻射節能玻璃", MaterialCategory::Glass, 2500.0, 68000.0, Color::rgba(140, 200, 190, 150), 0.9)
    }

    pub fn tinted_glass() -> Self {
        Self::spec("Grey Tinted Glass 灰色隔熱玻璃", MaterialCategory::Glass, 2500.0, 40000.0, Color::rgba(90, 100, 110, 175), 1.0)
    }

    pub fn frosted_glass() -> Self {
        Self::spec("Frosted Glass 霧面隱私玻璃", MaterialCategory::Glass, 2500.0, 42000.0, Color::rgba(225, 232, 236, 215), 1.0)
    }

    pub fn laminated_glass() -> Self {
        Self::spec("Laminated Safety Glass 膠合安全玻璃", MaterialCategory::Glass, 2500.0, 55000.0, Color::rgba(165, 215, 225, 140), 1.0)
    }

    // ── 塑膠 / 地坪 ──

    pub fn upvc() -> Self {
        Self::spec("uPVC 塑鋼窗框", MaterialCategory::Plastic, 1400.0, 90000.0, Color::rgb(245, 245, 242), 0.17)
    }

    pub fn vinyl_plank() -> Self {
        Self::spec("SPC Vinyl Plank 石塑卡扣地板", MaterialCategory::Plastic, 2000.0, 30000.0, Color::rgb(180, 150, 120), 0.25)
    }

    pub fn epoxy_floor() -> Self {
        Self::spec("Epoxy Floor Coating 環氧樹脂地坪", MaterialCategory::Plastic, 1500.0, 60000.0, Color::rgb(130, 145, 150), 0.2)
    }

    // ── 塗料 / 油漆 ──

    pub fn white_latex_paint() -> Self {
        Self::spec("White Latex Paint 白色乳膠漆", MaterialCategory::Paint, 1300.0, 250000.0, Color::rgb(246, 245, 240), 0.5)
    }

    pub fn warm_grey_paint() -> Self {
        Self::spec("Warm Grey Paint 暖灰色乳膠漆", MaterialCategory::Paint, 1300.0, 260000.0, Color::rgb(190, 184, 175), 0.5)
    }

    pub fn sage_green_paint() -> Self {
        Self::spec("Sage Green Paint 鼠尾草綠乳膠漆", MaterialCategory::Paint, 1300.0, 260000.0, Color::rgb(160, 175, 150), 0.5)
    }

    pub fn morandi_blue_paint() -> Self {
        Self::spec("Morandi Blue Paint 莫蘭迪藍乳膠漆", MaterialCategory::Paint, 1300.0, 260000.0, Color::rgb(130, 150, 170), 0.5)
    }

    pub fn matte_black_paint() -> Self {
        Self::spec("Matte Black Paint 黑色平光漆", MaterialCategory::Paint, 1300.0, 280000.0, Color::rgb(38, 38, 40), 0.5)
    }

    pub fn diatomite_plaster() -> Self {
        Self::spec("Diatomite Plaster 珪藻土", MaterialCategory::Paint, 900.0, 180000.0, Color::rgb(225, 215, 195), 0.1)
    }

    // ── 布料 / 皮革 ──

    pub fn fabric() -> Self {
        Self::spec("Linen Fabric 亞麻布藝軟裝", MaterialCategory::Fabric, 450.0, 22000.0, Color::rgb(180, 185, 190), 0.08)
    }

    pub fn leather() -> Self {
        Self::spec("Leather 頭層牛皮", MaterialCategory::Fabric, 900.0, 120000.0, Color::rgb(140, 90, 60), 0.16)
    }

    pub fn wool_carpet() -> Self {
        Self::spec("Wool Carpet 羊毛地毯", MaterialCategory::Fabric, 300.0, 40000.0, Color::rgb(150, 140, 130), 0.06)
    }

    pub fn velvet() -> Self {
        Self::spec("Velvet 絨布", MaterialCategory::Fabric, 400.0, 35000.0, Color::rgb(70, 95, 85), 0.07)
    }
}

/// On-disk material library (`*.json`): the user's own materials, shared between projects
/// and exchangeable with other users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialLibraryFile {
    pub format: String,
    pub version: u32,
    pub materials: Vec<Material>,
}

impl MaterialLibraryFile {
    pub const FORMAT: &'static str = "sketchcad-material-library";

    /// Reads a library file; a bare JSON array of materials is accepted too.
    pub fn load(path: &Path) -> std::io::Result<Vec<Material>> {
        let text = std::fs::read_to_string(path)?;
        if let Ok(file) = serde_json::from_str::<Self>(&text) {
            return Ok(file.materials);
        }
        serde_json::from_str::<Vec<Material>>(&text).map_err(std::io::Error::other)
    }

    pub fn save(path: &Path, materials: &[Material]) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = Self {
            format: Self::FORMAT.to_string(),
            version: 1,
            materials: materials.to_vec(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn presets_have_unique_names_and_a_category() {
        let presets = Material::presets();
        let mut names: Vec<_> = presets.iter().map(|m| m.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), presets.len(), "preset names must be unique");
        assert!(presets.iter().all(|m| m.category != MaterialCategory::Other));
        for category in MaterialCategory::ALL.iter().filter(|c| **c != MaterialCategory::Other) {
            assert!(presets.iter().any(|m| m.category == *category), "{category:?} has no preset");
        }
    }

    #[test]
    fn library_file_round_trips_and_old_materials_load_without_category() {
        let dir = std::env::temp_dir().join(format!("sketchcad-matlib-{}", std::process::id()));
        let path = dir.join("materials.json");
        let mut tile = Material::custom("Terracotta Tile 陶磚", Color::rgb(180, 95, 60));
        tile.category = MaterialCategory::Tile;
        MaterialLibraryFile::save(&path, &[tile.clone()]).unwrap();
        assert_eq!(MaterialLibraryFile::load(&path).unwrap(), vec![tile]);

        std::fs::write(
            &path,
            r#"[{"name":"Old","density_kg_m3":1.0,"cost_per_m3":2.0,"color":{"r":1,"g":2,"b":3,"a":255},"thermal_conductivity":0.5}]"#,
        )
        .unwrap();
        let old = MaterialLibraryFile::load(&path).unwrap();
        assert_eq!(old[0].category, MaterialCategory::Other);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// A composite layered material for building enclosures (e.g. walls, slabs, roofs)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialLayer {
    pub material: Material,
    pub thickness_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeMaterial {
    pub name: String,
    pub layers: Vec<MaterialLayer>,
}

impl CompositeMaterial {
    pub fn new(name: impl Into<String>, layers: Vec<MaterialLayer>) -> Self {
        Self {
            name: name.into(),
            layers,
        }
    }

    pub fn total_thickness_mm(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness_mm).sum()
    }

    /// Blended cost per m3 of the composite assembly
    pub fn average_cost_per_m3(&self) -> f64 {
        let total_t = self.total_thickness_mm();
        if total_t <= 0.0 {
            return 0.0;
        }
        let total_weighted_cost: f64 = self
            .layers
            .iter()
            .map(|l| l.material.cost_per_m3 * l.thickness_mm)
            .sum();
        total_weighted_cost / total_t
    }
}
