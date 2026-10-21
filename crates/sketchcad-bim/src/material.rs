use serde::{Deserialize, Serialize};
use sketchcad_geom::Color;

/// Building material definition with physical and financial properties
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub density_kg_m3: f64,
    pub cost_per_m3: f64, // Currency amount per cubic meter
    pub color: Color,
    pub thermal_conductivity: f64, // W/(m·K)
}

impl Material {
    pub fn concrete() -> Self {
        Self {
            name: "Reinforced Concrete 鋼筋混凝土".to_string(),
            density_kg_m3: 2400.0,
            cost_per_m3: 3200.0, // approx TWD/m3
            color: Color::rgb(180, 180, 180),
            thermal_conductivity: 1.7,
        }
    }

    pub fn brick() -> Self {
        Self {
            name: "Red Brick 紅磚".to_string(),
            density_kg_m3: 1800.0,
            cost_per_m3: 2600.0,
            color: Color::rgb(190, 85, 65),
            thermal_conductivity: 0.8,
        }
    }

    pub fn structural_steel() -> Self {
        Self {
            name: "Structural Steel 結構鋼材".to_string(),
            density_kg_m3: 7850.0,
            cost_per_m3: 65000.0,
            color: Color::rgb(100, 110, 120),
            thermal_conductivity: 50.0,
        }
    }

    pub fn glass() -> Self {
        Self {
            name: "Double Glazed Glass 雙層中空玻璃".to_string(),
            density_kg_m3: 2500.0,
            cost_per_m3: 45000.0,
            color: Color::rgba(150, 220, 240, 160),
            thermal_conductivity: 1.1,
        }
    }

    pub fn wood() -> Self {
        Self {
            name: "Solid Oak Timber 實木橡木".to_string(),
            density_kg_m3: 720.0,
            cost_per_m3: 38000.0,
            color: Color::rgb(160, 120, 80),
            thermal_conductivity: 0.13,
        }
    }

    pub fn gypsum() -> Self {
        Self {
            name: "Gypsum Plaster Board 輕鋼架石膏板".to_string(),
            density_kg_m3: 850.0,
            cost_per_m3: 15000.0,
            color: Color::rgb(240, 240, 235),
            thermal_conductivity: 0.25,
        }
    }

    pub fn marble() -> Self {
        Self {
            name: "Carrara Marble 卡拉拉白大理石".to_string(),
            density_kg_m3: 2700.0,
            cost_per_m3: 95000.0,
            color: Color::rgb(245, 248, 252),
            thermal_conductivity: 2.8,
        }
    }

    pub fn wood_flooring() -> Self {
        Self {
            name: "Oak Wood Flooring 超耐磨橡木地板".to_string(),
            density_kg_m3: 750.0,
            cost_per_m3: 42000.0,
            color: Color::rgb(205, 165, 115),
            thermal_conductivity: 0.14,
        }
    }

    pub fn polished_tile() -> Self {
        Self {
            name: "Polished Tile 拋光石英磚".to_string(),
            density_kg_m3: 2300.0,
            cost_per_m3: 28000.0,
            color: Color::rgb(230, 235, 240),
            thermal_conductivity: 1.3,
        }
    }

    pub fn walnut() -> Self {
        Self {
            name: "Walnut Veneer 北美黑胡桃木".to_string(),
            density_kg_m3: 680.0,
            cost_per_m3: 52000.0,
            color: Color::rgb(115, 75, 50),
            thermal_conductivity: 0.12,
        }
    }

    pub fn fabric() -> Self {
        Self {
            name: "Linen Fabric 亞麻布藝軟裝".to_string(),
            density_kg_m3: 450.0,
            cost_per_m3: 22000.0,
            color: Color::rgb(180, 185, 190),
            thermal_conductivity: 0.08,
        }
    }

    pub fn microcement() -> Self {
        Self {
            name: "Microcement 仿清水模微水泥".to_string(),
            density_kg_m3: 2100.0,
            cost_per_m3: 36000.0,
            color: Color::rgb(160, 165, 170),
            thermal_conductivity: 1.2,
        }
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
