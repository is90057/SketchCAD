//! The model catalogue. Each model is authored in millimetres with Z up and its front
//! facing −Y (vehicles run along X); the library recentres it on the origin afterwards.
//! Parts painted [`BODY`] take the model's body colour and can be repainted.

use std::f64::consts::PI;

use super::{Kit, ModelCategory, ModelSpec, Paint, BODY};

const fn rgb(r: u8, g: u8, b: u8) -> Paint {
    Some([r, g, b, 255])
}

const WOOD: Paint = rgb(150, 105, 65);
const DARK_WOOD: Paint = rgb(95, 65, 45);
const METAL: Paint = rgb(120, 125, 130);
const DARK_METAL: Paint = rgb(55, 58, 62);
const CHROME: Paint = rgb(205, 208, 212);
const BLACK: Paint = rgb(28, 28, 30);
const WHITE: Paint = rgb(244, 244, 242);
const DARK: Paint = rgb(40, 42, 46);
const GLASS: Paint = rgb(55, 75, 92);
const SCREEN: Paint = rgb(55, 105, 165);
const GLOW: Paint = rgb(120, 220, 255);
const LAMP_GLOW: Paint = rgb(255, 236, 170);
const RED: Paint = rgb(200, 40, 40);
const BLUE: Paint = rgb(45, 95, 185);
const GREEN: Paint = rgb(60, 140, 75);
const YELLOW: Paint = rgb(235, 190, 50);
const ORANGE: Paint = rgb(235, 130, 40);
const BRASS: Paint = rgb(190, 155, 80);
const STONE: Paint = rgb(170, 168, 160);
const ROOF_TILE: Paint = rgb(150, 60, 45);
const WATER: Paint = rgb(150, 200, 230);
const MIRROR: Paint = rgb(185, 205, 215);
const SOIL: Paint = rgb(85, 62, 45);
const TRUNK: Paint = rgb(105, 78, 55);
const STEM: Paint = rgb(85, 125, 60);
const LEAF: Paint = rgb(55, 120, 55);
const LEAF_DARK: Paint = rgb(35, 95, 50);
const LEAF_LIGHT: Paint = rgb(110, 160, 75);
const OLIVE_LEAF: Paint = rgb(130, 150, 110);
const SKIN: Paint = rgb(225, 185, 150);
const HAIR: Paint = rgb(50, 38, 30);
const DENIM: Paint = rgb(55, 70, 100);
const TIRE: Paint = rgb(32, 32, 34);
const FABRIC_GREY: Paint = rgb(150, 150, 155);

macro_rules! model {
    ($id:literal, $name:literal, $cat:ident, $body:expr, $price:expr, $elev:expr, $build:ident) => {
        ModelSpec {
            id: $id,
            name: $name,
            category: ModelCategory::$cat,
            body_color: $body,
            price: $price,
            elevation: $elev,
            build: $build,
        }
    };
}

pub static CATALOG: &[ModelSpec] = &[
    // 建築構件
    model!("roman_column", "羅馬柱 (Classical Column)", Architecture, [238, 236, 230, 255], 68000.0, 0.0, roman_column),
    model!("arch_doorway", "石砌拱門 (Stone Arch)", Architecture, [205, 195, 175, 255], 120000.0, 0.0, arch_doorway),
    model!("spiral_stair", "旋轉樓梯 (Spiral Staircase)", Architecture, [165, 120, 80, 255], 280000.0, 0.0, spiral_stair),
    model!("gazebo", "六角涼亭 (Gazebo)", Architecture, [150, 75, 55, 255], 450000.0, 0.0, gazebo),
    model!("pergola", "木作花架 (Pergola)", Architecture, [160, 120, 80, 255], 120000.0, 0.0, pergola),
    model!("fence_panel", "木圍籬 (Wooden Fence 1.8m)", Architecture, [175, 135, 95, 255], 9800.0, 0.0, fence_panel),
    model!("wood_deck", "戶外木平台 (Wood Deck 3×2.4m)", Architecture, [165, 120, 85, 255], 86000.0, 0.0, wood_deck),
    model!("fireplace", "壁爐 (Fireplace)", Architecture, [170, 85, 65, 255], 150000.0, 0.0, fireplace),
    // 家具
    model!("armchair", "單人扶手椅 (Armchair)", Furniture, [120, 135, 150, 255], 22000.0, 0.0, armchair),
    model!("dining_chair", "實木餐椅 (Dining Chair)", Furniture, [150, 105, 65, 255], 4800.0, 0.0, dining_chair),
    model!("bar_stool", "吧台椅 (Bar Stool)", Furniture, [60, 45, 40, 255], 3600.0, 0.0, bar_stool),
    model!("coffee_table", "圓形茶几 (Round Coffee Table)", Furniture, [115, 75, 50, 255], 12000.0, 0.0, coffee_table),
    model!("nightstand", "床頭櫃 (Nightstand)", Furniture, [215, 190, 155, 255], 6800.0, 0.0, nightstand),
    model!("bookshelf", "開放式書櫃 (Bookshelf)", Furniture, [235, 232, 225, 255], 15000.0, 0.0, bookshelf),
    model!("floor_lamp", "落地燈 (Floor Lamp)", Furniture, [235, 225, 205, 255], 5600.0, 0.0, floor_lamp),
    model!("upright_piano", "直立式鋼琴 (Upright Piano)", Furniture, [25, 25, 28, 255], 180000.0, 0.0, upright_piano),
    model!("bathtub", "獨立式浴缸 (Bathtub)", Furniture, [245, 245, 243, 255], 46000.0, 0.0, bathtub),
    model!("vanity", "浴櫃洗手台 (Bathroom Vanity)", Furniture, [150, 110, 75, 255], 28000.0, 0.0, vanity),
    // 電器用品
    model!("fridge", "雙門冰箱 (Refrigerator)", Appliance, [200, 203, 206, 255], 52000.0, 0.0, fridge),
    model!("washing_machine", "滾筒洗衣機 (Washing Machine)", Appliance, [242, 242, 240, 255], 32000.0, 0.0, washing_machine),
    model!("ac_indoor", "分離式冷氣 室內機 (Split AC Indoor)", Appliance, [246, 246, 244, 255], 28000.0, 2200.0, ac_indoor),
    model!("ac_outdoor", "冷氣室外機 (AC Outdoor Unit)", Appliance, [215, 215, 210, 255], 22000.0, 0.0, ac_outdoor),
    model!("microwave", "微波爐 (Microwave)", Appliance, [55, 55, 58, 255], 5900.0, 850.0, microwave),
    model!("rice_cooker", "電鍋 (Rice Cooker)", Appliance, [110, 165, 125, 255], 3200.0, 850.0, rice_cooker),
    model!("electric_fan", "立扇 (Electric Fan)", Appliance, [235, 235, 232, 255], 2400.0, 0.0, electric_fan),
    model!("desktop_pc", "桌上型電腦 (Desktop PC)", Appliance, [40, 40, 44, 255], 42000.0, 750.0, desktop_pc),
    model!("water_dispenser", "飲水機 (Water Dispenser)", Appliance, [240, 240, 238, 255], 9800.0, 0.0, water_dispenser),
    // 盆栽
    model!("monstera", "龜背芋 (Monstera)", PottedPlant, [190, 110, 80, 255], 2800.0, 0.0, monstera),
    model!("snake_plant", "虎尾蘭 (Snake Plant)", PottedPlant, [120, 120, 118, 255], 1500.0, 0.0, snake_plant),
    model!("fiddle_leaf_fig", "琴葉榕 (Fiddle-leaf Fig)", PottedPlant, [235, 232, 225, 255], 3600.0, 0.0, fiddle_leaf_fig),
    model!("cactus", "仙人掌 (Cactus)", PottedPlant, [195, 115, 80, 255], 680.0, 0.0, cactus),
    model!("succulents", "多肉植物組盆 (Succulents)", PottedPlant, [150, 150, 148, 255], 1200.0, 0.0, succulents),
    model!("olive_tree", "橄欖樹盆栽 (Potted Olive Tree)", PottedPlant, [200, 120, 85, 255], 6800.0, 0.0, olive_tree),
    // 植物景觀
    model!("broadleaf_tree", "闊葉喬木 (Broadleaf Tree)", Landscape, [85, 140, 70, 255], 18000.0, 0.0, broadleaf_tree),
    model!("conifer", "針葉松樹 (Conifer)", Landscape, [45, 95, 60, 255], 15000.0, 0.0, conifer),
    model!("palm_tree", "棕櫚樹 (Palm Tree)", Landscape, [80, 140, 70, 255], 22000.0, 0.0, palm_tree),
    model!("shrub", "灌木球 (Shrub)", Landscape, [70, 130, 60, 255], 1800.0, 0.0, shrub),
    model!("hedge", "綠籬 (Hedge 2m)", Landscape, [60, 115, 55, 255], 4500.0, 0.0, hedge),
    model!("flower_bed", "花圃 (Flower Bed)", Landscape, [160, 155, 150, 255], 6500.0, 0.0, flower_bed),
    model!("bamboo", "竹叢 (Bamboo Grove)", Landscape, [95, 150, 70, 255], 8000.0, 0.0, bamboo),
    // 車輛
    model!("sedan", "轎車 (Sedan)", Vehicle, [180, 30, 35, 255], 0.0, 0.0, sedan),
    model!("suv", "休旅車 (SUV)", Vehicle, [40, 70, 110, 255], 0.0, 0.0, suv),
    model!("pickup", "小貨車 (Pickup Truck)", Vehicle, [235, 235, 232, 255], 0.0, 0.0, pickup),
    model!("scooter", "機車 (Scooter)", Vehicle, [200, 40, 40, 255], 0.0, 0.0, scooter),
    model!("bicycle", "自行車 (Bicycle)", Vehicle, [30, 110, 160, 255], 0.0, 0.0, bicycle),
    // 戶外設施
    model!("park_bench", "公園長椅 (Park Bench)", Outdoor, [160, 115, 75, 255], 12000.0, 0.0, park_bench),
    model!("street_lamp", "路燈 (Street Lamp)", Outdoor, [90, 95, 100, 255], 35000.0, 0.0, street_lamp),
    model!("fire_hydrant", "消防栓 (Fire Hydrant)", Outdoor, [200, 35, 35, 255], 18000.0, 0.0, fire_hydrant),
    model!("patio_umbrella", "遮陽傘桌椅組 (Patio Umbrella Set)", Outdoor, [235, 225, 200, 255], 16000.0, 0.0, patio_umbrella),
    model!("recycle_bins", "資源回收桶 (Recycling Bins)", Outdoor, [120, 125, 130, 255], 4500.0, 0.0, recycle_bins),
    model!("swing_set", "鞦韆 (Swing Set)", Outdoor, [200, 60, 50, 255], 26000.0, 0.0, swing_set),
    // 人物配景
    model!("person_standing", "成人 站姿 (Standing Adult)", People, [70, 110, 160, 255], 0.0, 0.0, person_standing),
    model!("person_walking", "成人 行走 (Walking Adult)", People, [190, 90, 70, 255], 0.0, 0.0, person_walking),
    model!("child", "兒童 (Child)", People, [230, 180, 60, 255], 0.0, 0.0, child),
    model!("dog", "寵物狗 (Dog)", People, [190, 140, 90, 255], 0.0, 0.0, dog),
];

fn polar(angle_deg: f64, r: f64) -> (f64, f64) {
    let (s, c) = angle_deg.to_radians().sin_cos();
    (c * r, s * r)
}

/// Tapered pot with a rim and a soil surface; the pot is the model body.
fn pot(k: &mut Kit, r_bottom: f64, r_top: f64, h: f64) {
    k.frustum([0.0, 0.0, 0.0], [0.0, 0.0, h], r_bottom, r_top, 18, BODY);
    k.cylinder([0.0, 0.0, h - 40.0], [0.0, 0.0, h + 10.0], r_top + 12.0, 18, BODY);
    k.cylinder([0.0, 0.0, h + 10.0], [0.0, 0.0, h + 16.0], r_top - 5.0, 18, SOIL);
}

// ───────────────────────────── 建築構件 ─────────────────────────────

fn roman_column(k: &mut Kit) {
    k.cuboid([-330.0, -330.0, 0.0], [330.0, 330.0, 120.0], BODY);
    k.cylinder([0.0, 0.0, 120.0], [0.0, 0.0, 200.0], 290.0, 24, BODY);
    k.frustum([0.0, 0.0, 200.0], [0.0, 0.0, 2900.0], 240.0, 205.0, 24, BODY);
    k.cylinder([0.0, 0.0, 2900.0], [0.0, 0.0, 2950.0], 220.0, 24, BODY);
    k.frustum([0.0, 0.0, 2950.0], [0.0, 0.0, 3050.0], 215.0, 290.0, 24, BODY);
    k.cuboid([-330.0, -330.0, 3050.0], [330.0, 330.0, 3150.0], BODY);
}

fn arch_doorway(k: &mut Kit) {
    for side in [-1.0, 1.0] {
        let (x0, x1) = if side < 0.0 { (-1000.0, -600.0) } else { (600.0, 1000.0) };
        k.cuboid([x0, -250.0, 0.0], [x1, 250.0, 2000.0], BODY);
        k.cuboid([x0 - 40.0, -280.0, 2000.0], [x1 + 40.0, 280.0, 2080.0], BODY);
    }
    let segments = 11;
    for i in 0..segments {
        let (a0, a1) = (i as f64 / segments as f64 * PI, (i + 1) as f64 / segments as f64 * PI);
        let at = |a: f64, r: f64| (a.cos() * r, 2080.0 + a.sin() * r);
        k.extrude_y(&[at(a0, 600.0), at(a0, 1000.0), at(a1, 1000.0), at(a1, 600.0)], -250.0, 250.0, BODY);
    }
    k.cuboid([-110.0, -270.0, 3030.0], [110.0, 270.0, 3160.0], BODY);
}

fn spiral_stair(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 3150.0], 70.0, 16, DARK_METAL);
    let steps = 16;
    let mut previous_top: Option<[f64; 3]> = None;
    for i in 0..steps {
        let angle = i as f64 * 24.0;
        let z = 190.0 * (i + 1) as f64;
        let (cx, cy) = polar(angle, 480.0);
        k.tilted_box([cx, cy, z - 20.0], [880.0, 330.0, 40.0], angle, 0.0, BODY);
        let (bx, by) = polar(angle, 900.0);
        k.cylinder([bx, by, z], [bx, by, z + 900.0], 12.0, 8, DARK_METAL);
        let top = [bx, by, z + 900.0];
        if let Some(prev) = previous_top {
            k.cylinder(prev, top, 22.0, 8, DARK_METAL);
        }
        previous_top = Some(top);
    }
}

fn gazebo(k: &mut Kit) {
    k.frustum([0.0, 0.0, 0.0], [0.0, 0.0, 300.0], 1900.0, 1900.0, 6, STONE);
    for i in 0..6 {
        let (x, y) = polar(i as f64 * 60.0 + 30.0, 1500.0);
        k.cylinder([x, y, 300.0], [x, y, 2700.0], 90.0, 12, BODY);
    }
    k.frustum([0.0, 0.0, 2700.0], [0.0, 0.0, 2850.0], 1850.0, 1850.0, 6, BODY);
    k.frustum([0.0, 0.0, 2850.0], [0.0, 0.0, 4200.0], 2400.0, 0.0, 6, ROOF_TILE);
    k.cylinder([0.0, 0.0, 4100.0], [0.0, 0.0, 4350.0], 30.0, 8, BRASS);
    k.sphere([0.0, 0.0, 4380.0], 70.0, BRASS);
}

fn pergola(k: &mut Kit) {
    for (x, y) in [(-1400.0, -1200.0), (1400.0, -1200.0), (-1400.0, 1200.0), (1400.0, 1200.0)] {
        k.cuboid([x - 75.0, y - 75.0, 0.0], [x + 75.0, y + 75.0, 2600.0], BODY);
    }
    for y in [-1200.0, 1200.0] {
        k.cuboid([-1700.0, y - 50.0, 2450.0], [1700.0, y + 50.0, 2650.0], BODY);
    }
    for i in 0..9 {
        let x = -1500.0 + i as f64 * 375.0;
        k.cuboid([x - 30.0, -1450.0, 2650.0], [x + 30.0, 1450.0, 2800.0], BODY);
    }
}

fn fence_panel(k: &mut Kit) {
    for x in [-900.0, 900.0] {
        k.cuboid([x - 50.0, -50.0, 0.0], [x + 50.0, 50.0, 1250.0], BODY);
    }
    for z in [300.0, 850.0] {
        k.cuboid([-850.0, -10.0, z], [850.0, 30.0, z + 80.0], BODY);
    }
    for i in 0..12 {
        let x = -790.0 + i as f64 * 143.6;
        k.cuboid([x - 40.0, -30.0, 100.0], [x + 40.0, -10.0, 1150.0], BODY);
    }
}

fn wood_deck(k: &mut Kit) {
    for i in 0..4 {
        let x = -1350.0 + i as f64 * 900.0;
        k.cuboid([x - 40.0, -1200.0, 0.0], [x + 40.0, 1200.0, 180.0], DARK_WOOD);
    }
    for i in 0..13 {
        let y = -1200.0 + i as f64 * 185.0;
        k.cuboid([-1500.0, y, 180.0], [1500.0, y + 170.0, 210.0], BODY);
    }
}

fn fireplace(k: &mut Kit) {
    k.cuboid([-900.0, -450.0, 0.0], [900.0, 0.0, 100.0], STONE);
    k.cuboid([-750.0, 0.0, 0.0], [750.0, 450.0, 2700.0], BODY);
    k.cuboid([-450.0, -5.0, 150.0], [450.0, 10.0, 800.0], BLACK);
    k.cuboid([-850.0, -150.0, 1100.0], [850.0, 50.0, 1170.0], WOOD);
    k.cylinder([-300.0, -40.0, 210.0], [300.0, -40.0, 210.0], 60.0, 10, TRUNK);
    k.cylinder([-250.0, -60.0, 320.0], [250.0, -20.0, 300.0], 50.0, 10, TRUNK);
    k.ellipsoid([0.0, -35.0, 450.0], [200.0, 25.0, 170.0], ORANGE);
}

// ───────────────────────────── 家具 ─────────────────────────────

fn armchair(k: &mut Kit) {
    for (x, y) in [(-340.0, -340.0), (340.0, -340.0), (-340.0, 340.0), (340.0, 340.0)] {
        k.cylinder([x, y, 0.0], [x, y, 150.0], 25.0, 10, WOOD);
    }
    k.cuboid([-400.0, -400.0, 150.0], [400.0, 400.0, 420.0], BODY);
    k.cuboid([-310.0, -390.0, 420.0], [310.0, 300.0, 520.0], BODY);
    k.cuboid([-400.0, 300.0, 420.0], [400.0, 420.0, 880.0], BODY);
    for (x0, x1) in [(-400.0, -310.0), (310.0, 400.0)] {
        k.cuboid([x0, -400.0, 420.0], [x1, 300.0, 640.0], BODY);
    }
}

fn dining_chair(k: &mut Kit) {
    for (x, y) in [(-200.0, -200.0), (200.0, -200.0), (-200.0, 200.0), (200.0, 200.0)] {
        k.cylinder([x, y, 0.0], [x, y, 440.0], 18.0, 10, BODY);
    }
    k.cuboid([-225.0, -225.0, 440.0], [225.0, 225.0, 480.0], FABRIC_GREY);
    for x in [-200.0, 200.0] {
        k.cylinder([x, 200.0, 480.0], [x, 210.0, 900.0], 18.0, 10, BODY);
    }
    k.cuboid([-215.0, 190.0, 700.0], [215.0, 220.0, 880.0], BODY);
}

fn bar_stool(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 25.0], 220.0, 20, CHROME);
    k.cylinder([0.0, 0.0, 25.0], [0.0, 0.0, 680.0], 32.0, 12, CHROME);
    let corners = [(-170.0, -170.0), (170.0, -170.0), (170.0, 170.0), (-170.0, 170.0)];
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        k.cylinder([a.0, a.1, 300.0], [b.0, b.1, 300.0], 12.0, 8, CHROME);
    }
    for (x, y) in corners {
        k.cylinder([x, y, 300.0], [x * 0.15, y * 0.15, 300.0], 10.0, 6, CHROME);
    }
    k.cylinder([0.0, 0.0, 680.0], [0.0, 0.0, 760.0], 200.0, 20, BODY);
}

fn coffee_table(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 30.0], 260.0, 24, BODY);
    k.frustum([0.0, 0.0, 30.0], [0.0, 0.0, 400.0], 90.0, 60.0, 16, BODY);
    k.cylinder([0.0, 0.0, 400.0], [0.0, 0.0, 440.0], 450.0, 32, BODY);
}

fn nightstand(k: &mut Kit) {
    for (x, y) in [(-210.0, -160.0), (210.0, -160.0), (-210.0, 160.0), (210.0, 160.0)] {
        k.cylinder([x, y, 0.0], [x, y, 100.0], 15.0, 8, DARK_WOOD);
    }
    k.cuboid([-250.0, -200.0, 100.0], [250.0, 200.0, 560.0], BODY);
    k.cuboid([-235.0, -205.0, 325.0], [235.0, -199.0, 335.0], DARK_WOOD);
    for z in [200.0, 420.0] {
        k.cuboid([-60.0, -220.0, z], [60.0, -205.0, z + 20.0], BRASS);
    }
}

fn bookshelf(k: &mut Kit) {
    const BOOKS: [Paint; 6] = [rgb(160, 50, 45), rgb(45, 85, 140), rgb(220, 200, 150), rgb(60, 110, 80), rgb(40, 40, 45), rgb(200, 120, 60)];
    for x0 in [-500.0, 480.0] {
        k.cuboid([x0, -175.0, 0.0], [x0 + 20.0, 175.0, 2000.0], BODY);
    }
    k.cuboid([-480.0, 165.0, 0.0], [480.0, 175.0, 2000.0], BODY);
    let shelves = [0.0, 380.0, 780.0, 1180.0, 1580.0, 1975.0];
    for z in shelves {
        k.cuboid([-480.0, -175.0, z], [480.0, 165.0, z + 25.0], BODY);
    }
    let mut n = 0usize;
    for z in &shelves[..4] {
        let mut x = -470.0;
        let mut on_shelf = 0;
        while x < 420.0 {
            n += 1;
            on_shelf += 1;
            if on_shelf % 9 == 0 {
                x += 90.0; // gap between book groups
                continue;
            }
            let w = 28.0 + (n * 37 % 25) as f64;
            let h = 220.0 + (n * 53 % 110) as f64;
            k.cuboid([x, -150.0, z + 25.0], [(x + w).min(470.0), 80.0, z + 25.0 + h], BOOKS[n % BOOKS.len()]);
            x += w + 2.0;
        }
    }
}

fn floor_lamp(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 25.0], 170.0, 20, BLACK);
    k.cylinder([0.0, 0.0, 25.0], [0.0, 0.0, 1500.0], 12.0, 8, BLACK);
    k.frustum([0.0, 0.0, 1400.0], [0.0, 0.0, 1720.0], 230.0, 160.0, 24, BODY);
}

fn upright_piano(k: &mut Kit) {
    k.cuboid([-750.0, -50.0, 0.0], [750.0, 300.0, 1250.0], BODY);
    k.cuboid([-760.0, -60.0, 1250.0], [760.0, 310.0, 1280.0], BODY);
    k.cuboid([-750.0, -330.0, 650.0], [750.0, -50.0, 720.0], BODY);
    for x0 in [-750.0, 640.0] {
        k.cuboid([x0, -330.0, 720.0], [x0 + 110.0, -50.0, 800.0], BODY);
        k.cuboid([x0 + 20.0, -330.0, 0.0], [x0 + 80.0, -50.0, 80.0], BODY);
    }
    k.cuboid([-640.0, -320.0, 720.0], [640.0, -90.0, 740.0], WHITE);
    let key = 1280.0 / 52.0;
    for w in 0..51 {
        if matches!(w % 7, 0 | 1 | 3 | 4 | 5) {
            let x = -640.0 + (w + 1) as f64 * key;
            k.cuboid([x - 7.0, -230.0, 740.0], [x + 7.0, -90.0, 755.0], BLACK);
        }
    }
    for x in [-60.0, 0.0, 60.0] {
        k.cuboid([x - 15.0, -140.0, 60.0], [x + 15.0, -50.0, 75.0], BRASS);
    }
}

fn bathtub(k: &mut Kit) {
    k.cuboid([-850.0, -375.0, 0.0], [850.0, 375.0, 120.0], BODY);
    k.cuboid([-850.0, -375.0, 120.0], [850.0, -295.0, 580.0], BODY);
    k.cuboid([-850.0, 295.0, 120.0], [850.0, 375.0, 580.0], BODY);
    k.cuboid([-850.0, -295.0, 120.0], [-770.0, 295.0, 580.0], BODY);
    k.cuboid([770.0, -295.0, 120.0], [850.0, 295.0, 580.0], BODY);
    k.cuboid([-770.0, -295.0, 120.0], [770.0, 295.0, 420.0], WATER);
    k.cylinder([810.0, 0.0, 580.0], [810.0, 0.0, 720.0], 18.0, 10, CHROME);
    k.cylinder([810.0, 0.0, 720.0], [690.0, 0.0, 720.0], 15.0, 10, CHROME);
}

fn vanity(k: &mut Kit) {
    k.cuboid([-400.0, -250.0, 0.0], [400.0, 250.0, 820.0], BODY);
    k.cuboid([-4.0, -256.0, 60.0], [4.0, -249.0, 780.0], DARK_WOOD);
    k.cuboid([-410.0, -260.0, 820.0], [410.0, 260.0, 850.0], WHITE);
    k.frustum([0.0, -40.0, 850.0], [0.0, -40.0, 980.0], 170.0, 210.0, 24, WHITE);
    k.cylinder([0.0, -40.0, 970.0], [0.0, -40.0, 981.0], 190.0, 24, WATER);
    k.cylinder([0.0, 200.0, 850.0], [0.0, 200.0, 1060.0], 16.0, 10, CHROME);
    k.cylinder([0.0, 200.0, 1060.0], [0.0, 60.0, 1060.0], 14.0, 10, CHROME);
    k.cuboid([-350.0, 230.0, 1080.0], [350.0, 250.0, 1850.0], MIRROR);
}

// ───────────────────────────── 電器用品 ─────────────────────────────

fn fridge(k: &mut Kit) {
    k.cuboid([-450.0, -350.0, 0.0], [450.0, 350.0, 1800.0], BODY);
    k.cuboid([-4.0, -356.0, 520.0], [4.0, -349.0, 1795.0], DARK);
    k.cuboid([-445.0, -356.0, 510.0], [445.0, -349.0, 520.0], DARK);
    for x in [-40.0, 25.0] {
        k.cuboid([x, -380.0, 800.0], [x + 15.0, -356.0, 1500.0], CHROME);
    }
    k.cuboid([-300.0, -380.0, 420.0], [300.0, -356.0, 440.0], CHROME);
    k.cuboid([-320.0, -354.0, 1050.0], [-120.0, -349.0, 1350.0], DARK);
}

fn washing_machine(k: &mut Kit) {
    k.cuboid([-300.0, -300.0, 0.0], [300.0, 300.0, 850.0], BODY);
    k.cuboid([-295.0, -305.0, 720.0], [295.0, -300.0, 830.0], rgb(215, 215, 212));
    k.cuboid([-200.0, -307.0, 750.0], [50.0, -304.0, 800.0], DARK);
    k.cylinder([180.0, -305.0, 775.0], [180.0, -330.0, 775.0], 30.0, 14, CHROME);
    k.torus_y([0.0, -312.0, 400.0], 200.0, 28.0, CHROME);
    k.cylinder([0.0, -300.0, 400.0], [0.0, -316.0, 400.0], 185.0, 24, GLASS);
}

fn ac_indoor(k: &mut Kit) {
    k.cuboid([-450.0, -110.0, 60.0], [450.0, 110.0, 300.0], BODY);
    k.cylinder([-450.0, -30.0, 110.0], [450.0, -30.0, 110.0], 100.0, 16, BODY);
    k.cuboid([-420.0, -125.0, 70.0], [420.0, -95.0, 100.0], DARK);
    k.cuboid([300.0, -133.0, 150.0], [360.0, -126.0, 170.0], GLOW);
}

fn ac_outdoor(k: &mut Kit) {
    for x in [-380.0, 320.0] {
        k.cuboid([x, -170.0, 0.0], [x + 60.0, 170.0, 40.0], DARK_METAL);
    }
    k.cuboid([-400.0, -150.0, 40.0], [400.0, 150.0, 640.0], BODY);
    k.cylinder([-100.0, -150.0, 340.0], [-100.0, -158.0, 340.0], 230.0, 24, DARK);
    k.cuboid([-105.0, -165.0, 110.0], [-95.0, -158.0, 570.0], CHROME);
    k.cuboid([-330.0, -165.0, 335.0], [130.0, -158.0, 345.0], CHROME);
    for i in 0..5 {
        let z = 120.0 + i as f64 * 100.0;
        k.cuboid([200.0, -156.0, z], [370.0, -151.0, z + 12.0], DARK_METAL);
    }
}

fn microwave(k: &mut Kit) {
    k.cuboid([-250.0, -190.0, 10.0], [250.0, 190.0, 300.0], BODY);
    for x in [-220.0, 200.0] {
        k.cuboid([x, -170.0, 0.0], [x + 20.0, 170.0, 10.0], BLACK);
    }
    k.cuboid([-230.0, -195.0, 40.0], [110.0, -190.0, 270.0], GLASS);
    k.cuboid([130.0, -195.0, 40.0], [230.0, -190.0, 270.0], rgb(80, 80, 84));
    k.cuboid([145.0, -198.0, 220.0], [215.0, -195.0, 250.0], GLOW);
    for row in 0..3 {
        let z = 90.0 + row as f64 * 40.0;
        k.cuboid([150.0, -199.0, z], [210.0, -195.0, z + 22.0], DARK);
    }
}

fn rice_cooker(k: &mut Kit) {
    for i in 0..3 {
        let (x, y) = polar(i as f64 * 120.0 + 90.0, 110.0);
        k.cylinder([x, y, 0.0], [x, y, 20.0], 18.0, 8, BLACK);
    }
    k.cylinder([0.0, 0.0, 20.0], [0.0, 0.0, 230.0], 160.0, 24, BODY);
    k.cylinder([0.0, 0.0, 200.0], [0.0, 0.0, 215.0], 168.0, 24, CHROME);
    k.frustum([0.0, 0.0, 230.0], [0.0, 0.0, 300.0], 160.0, 100.0, 24, BODY);
    k.cylinder([0.0, 0.0, 300.0], [0.0, 0.0, 325.0], 25.0, 10, BLACK);
    k.sphere([0.0, 0.0, 335.0], 25.0, BLACK);
    for x in [-205.0, 160.0] {
        k.cuboid([x, -30.0, 170.0], [x + 45.0, 30.0, 200.0], BLACK);
    }
    k.cuboid([-25.0, -172.0, 50.0], [25.0, -158.0, 90.0], ORANGE);
}

fn electric_fan(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 40.0], 180.0, 20, BODY);
    k.cylinder([0.0, 0.0, 40.0], [0.0, 0.0, 1050.0], 16.0, 10, BODY);
    k.ellipsoid([0.0, 40.0, 1120.0], [90.0, 140.0, 90.0], BODY);
    k.sphere([0.0, -70.0, 1120.0], 40.0, BODY);
    k.torus_y([0.0, -80.0, 1120.0], 230.0, 8.0, CHROME);
    for i in 0..3 {
        let a = (i as f64 * 120.0).to_radians();
        let (d, w) = ([a.cos(), 0.0, a.sin()], [-a.sin(), 0.0, a.cos()]);
        let c = [d[0] * 125.0, -75.0, 1120.0 + d[2] * 125.0];
        k.oriented_box(c, [d[0] * 95.0, 0.0, d[2] * 95.0], [0.0, 4.0, 0.0], [w[0] * 45.0, 0.0, w[2] * 45.0], rgb(150, 190, 230));
    }
}

fn desktop_pc(k: &mut Kit) {
    k.cuboid([-120.0, -80.0, 0.0], [120.0, 120.0, 15.0], BLACK);
    k.cuboid([-25.0, 60.0, 15.0], [25.0, 90.0, 200.0], BLACK);
    k.cuboid([-300.0, 20.0, 150.0], [300.0, 50.0, 500.0], BODY);
    k.cuboid([-290.0, 15.0, 160.0], [290.0, 20.0, 490.0], SCREEN);
    k.cuboid([-220.0, -300.0, 0.0], [220.0, -160.0, 20.0], BODY);
    k.ellipsoid([300.0, -230.0, 15.0], [35.0, 55.0, 15.0], BODY);
    k.cuboid([400.0, -200.0, 0.0], [600.0, 250.0, 450.0], BODY);
    k.cuboid([480.0, -205.0, 400.0], [520.0, -200.0, 410.0], GLOW);
}

fn water_dispenser(k: &mut Kit) {
    k.cuboid([-165.0, -175.0, 0.0], [165.0, 175.0, 1000.0], BODY);
    k.cuboid([-120.0, -180.0, 450.0], [120.0, -170.0, 750.0], DARK);
    k.cuboid([-70.0, -200.0, 680.0], [-30.0, -170.0, 720.0], RED);
    k.cuboid([30.0, -200.0, 680.0], [70.0, -170.0, 720.0], BLUE);
    k.cuboid([-110.0, -200.0, 450.0], [110.0, -170.0, 480.0], CHROME);
    k.cylinder([0.0, 0.0, 1000.0], [0.0, 0.0, 1380.0], 135.0, 20, WATER);
    k.frustum([0.0, 0.0, 1380.0], [0.0, 0.0, 1450.0], 135.0, 50.0, 20, WATER);
}

// ───────────────────────────── 盆栽 ─────────────────────────────

fn monstera(k: &mut Kit) {
    pot(k, 170.0, 220.0, 380.0);
    for i in 0..7 {
        let a = i as f64 * 51.4 + (i % 2) as f64 * 15.0;
        let dist = 230.0 + (i % 3) as f64 * 60.0;
        let z = 650.0 + (i % 4) as f64 * 110.0;
        let (sx, sy) = polar(a, dist * 0.8);
        k.cylinder([0.0, 0.0, 390.0], [sx, sy, z], 10.0, 6, STEM);
        let (lx, ly) = polar(a, dist);
        k.leaf([lx, ly, z + 10.0], [150.0, 210.0, 12.0], a - 90.0, -30.0, LEAF_DARK);
    }
}

fn snake_plant(k: &mut Kit) {
    k.cuboid([-160.0, -160.0, 0.0], [160.0, 160.0, 330.0], BODY);
    k.cuboid([-145.0, -145.0, 330.0], [145.0, 145.0, 345.0], SOIL);
    for i in 0..9 {
        let a = i as f64 * 40.0;
        let (x, y) = polar(a, 40.0 + (i % 3) as f64 * 30.0);
        let h = 550.0 + (i * 97 % 350) as f64;
        k.tilted_box([x, y, 330.0 + h * 0.5], [70.0, 14.0, h], a - 90.0, -(6.0 + (i % 3) as f64 * 4.0), rgb(60, 110, 60));
    }
}

fn fiddle_leaf_fig(k: &mut Kit) {
    pot(k, 190.0, 240.0, 420.0);
    k.cylinder([0.0, 0.0, 420.0], [20.0, 0.0, 1600.0], 28.0, 10, TRUNK);
    for i in 0..16 {
        let z = 900.0 + i as f64 * 50.0;
        let a = i as f64 * 137.5;
        let (x, y) = polar(a, 120.0 + (i % 3) as f64 * 40.0);
        k.leaf([x + 20.0 * i as f64 / 16.0, y, z], [110.0, 160.0, 12.0], a - 90.0, -15.0 + (i % 2) as f64 * 25.0, if i % 2 == 0 { LEAF } else { LEAF_LIGHT });
    }
}

fn cactus(k: &mut Kit) {
    const CACTUS: Paint = rgb(75, 135, 80);
    pot(k, 120.0, 150.0, 220.0);
    k.cylinder([0.0, 0.0, 230.0], [0.0, 0.0, 800.0], 75.0, 14, CACTUS);
    k.sphere([0.0, 0.0, 800.0], 75.0, CACTUS);
    for (side, z, top) in [(1.0, 480.0, 680.0), (-1.0, 560.0, 720.0)] {
        k.cylinder([side * 60.0, 0.0, z], [side * 170.0, 0.0, z], 45.0, 12, CACTUS);
        k.cylinder([side * 170.0, 0.0, z], [side * 170.0, 0.0, top], 45.0, 12, CACTUS);
        k.sphere([side * 170.0, 0.0, top], 45.0, CACTUS);
    }
}

fn succulents(k: &mut Kit) {
    const TONES: [Paint; 3] = [rgb(130, 170, 140), rgb(170, 140, 170), rgb(110, 150, 100)];
    k.cuboid([-240.0, -130.0, 0.0], [240.0, 130.0, 130.0], BODY);
    k.cuboid([-225.0, -115.0, 130.0], [225.0, 115.0, 140.0], SOIL);
    for (n, cx) in [-140.0, 10.0, 150.0].into_iter().enumerate() {
        for ring in 0..3 {
            let count = 6 + ring * 2;
            for i in 0..count {
                let a = i as f64 * 360.0 / count as f64 + ring as f64 * 20.0;
                let (x, y) = polar(a, (ring + 1) as f64 * 20.0);
                k.leaf([cx + x, y, 150.0 + (2 - ring) as f64 * 12.0], [16.0, 34.0, 9.0], a - 90.0, 25.0, TONES[n]);
            }
        }
    }
}

fn olive_tree(k: &mut Kit) {
    pot(k, 240.0, 300.0, 520.0);
    k.cylinder([0.0, 0.0, 520.0], [40.0, 0.0, 1200.0], 45.0, 10, TRUNK);
    k.cylinder([40.0, 0.0, 1200.0], [-150.0, 60.0, 1500.0], 25.0, 8, TRUNK);
    k.cylinder([40.0, 0.0, 1200.0], [200.0, -50.0, 1550.0], 25.0, 8, TRUNK);
    k.ellipsoid([-150.0, 60.0, 1600.0], [330.0, 300.0, 260.0], OLIVE_LEAF);
    k.ellipsoid([200.0, -50.0, 1650.0], [300.0, 300.0, 250.0], OLIVE_LEAF);
    k.ellipsoid([30.0, 0.0, 1850.0], [320.0, 320.0, 260.0], OLIVE_LEAF);
}

// ───────────────────────────── 植物景觀 ─────────────────────────────

fn broadleaf_tree(k: &mut Kit) {
    k.frustum([0.0, 0.0, 0.0], [0.0, 0.0, 2600.0], 200.0, 120.0, 12, TRUNK);
    k.cylinder([0.0, 0.0, 2200.0], [700.0, 200.0, 3300.0], 70.0, 8, TRUNK);
    k.cylinder([0.0, 0.0, 2400.0], [-600.0, -300.0, 3400.0], 70.0, 8, TRUNK);
    k.cylinder([0.0, 0.0, 2500.0], [100.0, -500.0, 3600.0], 60.0, 8, TRUNK);
    k.sphere([0.0, 0.0, 4200.0], 1500.0, BODY);
    k.sphere([900.0, 300.0, 3700.0], 1100.0, BODY);
    k.sphere([-800.0, -400.0, 3800.0], 1100.0, BODY);
    k.sphere([200.0, -700.0, 3900.0], 1000.0, BODY);
    k.sphere([-300.0, 700.0, 4000.0], 1000.0, BODY);
}

fn conifer(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1300.0], 130.0, 10, TRUNK);
    let tiers = [(900.0, 2600.0, 1500.0, 300.0), (2000.0, 3600.0, 1200.0, 200.0), (3000.0, 4500.0, 900.0, 100.0), (3900.0, 5400.0, 600.0, 0.0)];
    for (z0, z1, r0, r1) in tiers {
        k.frustum([0.0, 0.0, z0], [0.0, 0.0, z1], r0, r1, 14, BODY);
    }
}

fn palm_tree(k: &mut Kit) {
    let segments = 8;
    let at = |i: usize| [(i * i) as f64 * 12.0, 0.0, i as f64 * 700.0];
    for i in 0..segments {
        k.frustum(at(i), at(i + 1), 220.0 - i as f64 * 12.0, 220.0 - (i + 1) as f64 * 12.0, 12, rgb(140, 115, 85));
    }
    let top = at(segments);
    for i in 0..10 {
        let a = i as f64 * 36.0;
        let droop = 20.0f64 + (i % 3) as f64 * 8.0;
        let (dx, dy) = polar(a, 1250.0 * droop.to_radians().cos());
        let c = [top[0] + dx, top[1] + dy, top[2] - 1250.0 * droop.to_radians().sin()];
        k.tilted_box(c, [360.0, 2600.0, 30.0], a - 90.0, -droop, BODY);
    }
    for i in 0..3 {
        let (x, y) = polar(i as f64 * 120.0, 150.0);
        k.sphere([top[0] + x, top[1] + y, top[2] - 150.0], 90.0, rgb(120, 90, 50));
    }
}

fn shrub(k: &mut Kit) {
    k.ellipsoid([0.0, 0.0, 550.0], [650.0, 650.0, 550.0], BODY);
    for (x, y, z, r) in [(350.0, 200.0, 750.0, 350.0), (-300.0, 250.0, 700.0, 330.0), (100.0, -350.0, 680.0, 340.0), (-250.0, -250.0, 820.0, 300.0)] {
        k.sphere([x, y, z], r, BODY);
    }
}

fn hedge(k: &mut Kit) {
    k.cuboid([-1000.0, -300.0, 0.0], [1000.0, 300.0, 1000.0], BODY);
    for i in 0..5 {
        k.ellipsoid([-800.0 + i as f64 * 400.0, 0.0, 1000.0], [260.0, 290.0, 110.0], BODY);
    }
}

fn flower_bed(k: &mut Kit) {
    const PETALS: [Paint; 4] = [rgb(220, 50, 70), rgb(245, 205, 60), rgb(235, 140, 190), rgb(140, 90, 200)];
    k.cuboid([-900.0, -450.0, 0.0], [900.0, -380.0, 280.0], BODY);
    k.cuboid([-900.0, 380.0, 0.0], [900.0, 450.0, 280.0], BODY);
    k.cuboid([-900.0, -380.0, 0.0], [-830.0, 380.0, 280.0], BODY);
    k.cuboid([830.0, -380.0, 0.0], [900.0, 380.0, 280.0], BODY);
    k.cuboid([-830.0, -380.0, 0.0], [830.0, 380.0, 220.0], SOIL);
    for ix in 0..6 {
        for iy in 0..3 {
            let (x, y) = (-650.0 + ix as f64 * 260.0, -240.0 + iy as f64 * 240.0);
            k.ellipsoid([x, y, 250.0], [90.0, 90.0, 35.0], LEAF);
            k.cylinder([x, y, 230.0], [x, y, 400.0], 8.0, 6, STEM);
            k.sphere([x, y, 420.0], 50.0, PETALS[(ix + iy) % PETALS.len()]);
        }
    }
}

fn bamboo(k: &mut Kit) {
    for i in 0..9 {
        let (x, y) = polar(i as f64 * 80.0, 120.0 + i as f64 * 35.0);
        let h = 3200.0 + (i * 370 % 1300) as f64;
        k.cylinder([x, y, 0.0], [x, y, h], 35.0, 8, rgb(120, 160, 80));
        let mut z = 400.0;
        while z < h - 200.0 {
            k.cylinder([x, y, z], [x, y, z + 20.0], 42.0, 8, rgb(100, 135, 65));
            z += 400.0;
        }
        for (drop, out, r) in [(150.0, 1.3, 300.0), (900.0, 1.8, 260.0), (1650.0, 1.5, 220.0)] {
            k.ellipsoid([x * out, y * out, h - drop], [r, r, r * 1.5], BODY);
        }
    }
}

// ───────────────────────────── 車輛 ─────────────────────────────

fn wheels(k: &mut Kit, xs: [f64; 2], y_outer: f64, y_inner: f64, r: f64) {
    for x in xs {
        for side in [-1.0, 1.0] {
            k.cylinder([x, side * y_outer, r], [x, side * y_inner, r], r, 18, TIRE);
            k.cylinder([x, side * (y_outer + 4.0), r], [x, side * (y_outer - 10.0), r], r * 0.55, 14, CHROME);
        }
    }
}

fn car_lights(k: &mut Kit, front_x: f64, rear_x: f64, half_w: f64, z: f64) {
    for side in [-1.0, 1.0] {
        let (y0, y1) = if side < 0.0 { (-half_w + 80.0, -half_w + 380.0) } else { (half_w - 380.0, half_w - 80.0) };
        k.cuboid([front_x - 30.0, y0, z], [front_x + 8.0, y1, z + 100.0], LAMP_GLOW);
        k.cuboid([rear_x - 8.0, y0, z], [rear_x + 30.0, y1, z + 90.0], RED);
    }
}

fn sedan(k: &mut Kit) {
    let body = [(-2350.0, 280.0), (2350.0, 280.0), (2380.0, 620.0), (2150.0, 820.0), (-2200.0, 860.0), (-2370.0, 650.0)];
    k.extrude_y(&body, -880.0, 880.0, BODY);
    k.extrude_y(&[(-1500.0, 840.0), (1050.0, 840.0), (450.0, 1400.0), (-1150.0, 1400.0)], -780.0, 780.0, GLASS);
    k.cuboid([-1120.0, -760.0, 1395.0], [420.0, 760.0, 1440.0], BODY);
    k.cuboid([2360.0, -350.0, 360.0], [2392.0, 350.0, 540.0], BLACK);
    car_lights(k, 2370.0, -2370.0, 880.0, 600.0);
    wheels(k, [-1450.0, 1450.0], 900.0, 690.0, 330.0);
}

fn suv(k: &mut Kit) {
    let body = [(-2300.0, 350.0), (2300.0, 350.0), (2320.0, 800.0), (2050.0, 1050.0), (-2250.0, 1080.0), (-2320.0, 800.0)];
    k.extrude_y(&body, -900.0, 900.0, BODY);
    k.extrude_y(&[(-2200.0, 1060.0), (1300.0, 1060.0), (800.0, 1650.0), (-2150.0, 1650.0)], -820.0, 820.0, GLASS);
    k.cuboid([-2170.0, -830.0, 1640.0], [820.0, 830.0, 1700.0], BODY);
    for side in [-1.0, 1.0] {
        k.cuboid([-1900.0, side * 700.0 - 25.0, 1700.0], [500.0, side * 700.0 + 25.0, 1740.0], BLACK);
    }
    k.cuboid([2310.0, -420.0, 450.0], [2335.0, 420.0, 720.0], BLACK);
    car_lights(k, 2315.0, -2315.0, 900.0, 820.0);
    wheels(k, [-1400.0, 1400.0], 940.0, 700.0, 370.0);
}

fn pickup(k: &mut Kit) {
    let body = [(-2650.0, 350.0), (2650.0, 350.0), (2670.0, 800.0), (2450.0, 1000.0), (-2650.0, 1000.0)];
    k.extrude_y(&body, -880.0, 880.0, BODY);
    k.extrude_y(&[(0.0, 980.0), (1600.0, 980.0), (1100.0, 1650.0), (0.0, 1650.0)], -800.0, 800.0, GLASS);
    k.cuboid([0.0, -800.0, 1640.0], [1120.0, 800.0, 1700.0], BODY);
    k.cuboid([-50.0, -800.0, 980.0], [0.0, 800.0, 1650.0], BODY);
    for side in [-1.0, 1.0] {
        let (y0, y1) = if side < 0.0 { (-880.0, -820.0) } else { (820.0, 880.0) };
        k.cuboid([-2650.0, y0, 1000.0], [-100.0, y1, 1450.0], BODY);
    }
    k.cuboid([-2650.0, -820.0, 1000.0], [-2590.0, 820.0, 1450.0], BODY);
    k.cuboid([2640.0, -400.0, 480.0], [2678.0, 400.0, 720.0], CHROME);
    car_lights(k, 2660.0, -2660.0, 880.0, 820.0);
    wheels(k, [-1700.0, 1700.0], 930.0, 690.0, 380.0);
}

fn scooter(k: &mut Kit) {
    for x in [-640.0, 640.0] {
        k.torus_y([x, 0.0, 240.0], 190.0, 55.0, TIRE);
        k.cylinder([x, -40.0, 240.0], [x, 40.0, 240.0], 120.0, 14, CHROME);
    }
    k.cuboid([-350.0, -180.0, 280.0], [350.0, 180.0, 360.0], BODY);
    k.ellipsoid([-450.0, 0.0, 600.0], [480.0, 200.0, 260.0], BODY);
    k.ellipsoid([-480.0, 0.0, 850.0], [380.0, 170.0, 60.0], BLACK);
    k.tilted_box([520.0, 0.0, 650.0], [420.0, 60.0, 650.0], 90.0, -20.0, BODY);
    k.cylinder([640.0, 0.0, 240.0], [540.0, 0.0, 1000.0], 25.0, 10, CHROME);
    k.cylinder([520.0, -330.0, 1050.0], [520.0, 330.0, 1050.0], 18.0, 10, BLACK);
    k.ellipsoid([600.0, 0.0, 980.0], [60.0, 90.0, 60.0], LAMP_GLOW);
}

fn bicycle(k: &mut Kit) {
    for x in [-540.0, 540.0] {
        k.torus_y([x, 0.0, 340.0], 320.0, 18.0, TIRE);
        k.cylinder([x, -40.0, 340.0], [x, 40.0, 340.0], 25.0, 10, CHROME);
    }
    let (rear, bracket, seat, head, head_low, front) = (
        [-540.0, 0.0, 340.0],
        [-20.0, 0.0, 300.0],
        [-180.0, 0.0, 800.0],
        [400.0, 0.0, 820.0],
        [430.0, 0.0, 700.0],
        [540.0, 0.0, 340.0],
    );
    for (a, b) in [(rear, bracket), (rear, seat), (bracket, seat), (seat, head), (bracket, head_low), (head_low, head)] {
        k.cylinder(a, b, 18.0, 8, BODY);
    }
    k.cylinder(head_low, front, 14.0, 8, CHROME);
    k.cylinder(seat, [-200.0, 0.0, 900.0], 12.0, 8, CHROME);
    k.ellipsoid([-210.0, 0.0, 920.0], [130.0, 60.0, 25.0], BLACK);
    k.cylinder(head, [380.0, 0.0, 900.0], 14.0, 8, CHROME);
    k.cylinder([370.0, -280.0, 900.0], [370.0, 280.0, 900.0], 14.0, 8, BLACK);
    k.cylinder([-20.0, -90.0, 300.0], [-20.0, 90.0, 300.0], 15.0, 8, DARK_METAL);
}

// ───────────────────────────── 戶外設施 ─────────────────────────────

fn park_bench(k: &mut Kit) {
    for x in [-800.0, 800.0] {
        k.cuboid([x - 30.0, -280.0, 0.0], [x + 30.0, 280.0, 60.0], DARK_METAL);
        k.cuboid([x - 30.0, -280.0, 0.0], [x + 30.0, -220.0, 450.0], DARK_METAL);
        k.cuboid([x - 30.0, 180.0, 0.0], [x + 30.0, 240.0, 850.0], DARK_METAL);
        k.cuboid([x - 30.0, -300.0, 600.0], [x + 30.0, 240.0, 650.0], DARK_METAL);
    }
    for i in 0..5 {
        let y = -260.0 + i as f64 * 92.0;
        k.cuboid([-950.0, y, 430.0], [950.0, y + 70.0, 470.0], BODY);
    }
    for i in 0..3 {
        let z = 520.0 + i as f64 * 110.0;
        k.cuboid([-950.0, 240.0, z], [950.0, 270.0, z + 80.0], BODY);
    }
}

fn street_lamp(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 400.0], 180.0, 16, DARK_METAL);
    k.frustum([0.0, 0.0, 400.0], [0.0, 0.0, 6000.0], 90.0, 55.0, 16, BODY);
    k.cylinder([0.0, 0.0, 5850.0], [1300.0, 0.0, 5950.0], 35.0, 10, BODY);
    k.cuboid([1100.0, -180.0, 5880.0], [1700.0, 180.0, 6020.0], BODY);
    k.cuboid([1150.0, -150.0, 5860.0], [1650.0, 150.0, 5880.0], LAMP_GLOW);
}

fn fire_hydrant(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 60.0], 170.0, 16, BODY);
    k.cylinder([0.0, 0.0, 60.0], [0.0, 0.0, 600.0], 120.0, 16, BODY);
    k.sphere([0.0, 0.0, 600.0], 120.0, BODY);
    k.cylinder([0.0, 0.0, 700.0], [0.0, 0.0, 760.0], 35.0, 6, CHROME);
    k.cylinder([0.0, -100.0, 420.0], [0.0, -210.0, 420.0], 55.0, 12, BODY);
    k.cylinder([0.0, -210.0, 420.0], [0.0, -230.0, 420.0], 45.0, 6, CHROME);
    for side in [-1.0, 1.0] {
        k.cylinder([side * 100.0, 0.0, 440.0], [side * 190.0, 0.0, 440.0], 40.0, 12, BODY);
        k.cylinder([side * 190.0, 0.0, 440.0], [side * 205.0, 0.0, 440.0], 35.0, 6, CHROME);
    }
}

fn patio_umbrella(k: &mut Kit) {
    k.cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 30.0], 250.0, 20, DARK_METAL);
    k.cylinder([0.0, 0.0, 30.0], [0.0, 0.0, 700.0], 30.0, 10, DARK_METAL);
    k.cylinder([0.0, 0.0, 700.0], [0.0, 0.0, 730.0], 500.0, 28, WHITE);
    k.cylinder([0.0, 0.0, 730.0], [0.0, 0.0, 2550.0], 22.0, 8, METAL);
    k.frustum([0.0, 0.0, 2150.0], [0.0, 0.0, 2550.0], 1400.0, 50.0, 8, BODY);
    k.sphere([0.0, 0.0, 2580.0], 40.0, METAL);
    for i in 0..4 {
        let a = 45.0 + i as f64 * 90.0;
        let (cx, cy) = polar(a, 850.0);
        let yaw = a + 90.0;
        for (lx, ly) in [(-180.0, -180.0), (180.0, -180.0), (-180.0, 180.0), (180.0, 180.0)] {
            let (s, c) = yaw.to_radians().sin_cos();
            let (x, y) = (cx + lx * c - ly * s, cy + lx * s + ly * c);
            k.cylinder([x, y, 0.0], [x, y, 430.0], 14.0, 6, DARK_METAL);
        }
        k.tilted_box([cx, cy, 450.0], [440.0, 440.0, 40.0], yaw, 0.0, WHITE);
        let (bx, by) = polar(a, 850.0 + 210.0);
        k.tilted_box([bx, by, 700.0], [440.0, 30.0, 450.0], yaw, -10.0, WHITE);
    }
}

fn recycle_bins(k: &mut Kit) {
    for (x, lid) in [(-450.0, BLUE), (0.0, GREEN), (450.0, YELLOW)] {
        k.cuboid([x - 200.0, -200.0, 0.0], [x + 200.0, 200.0, 800.0], BODY);
        k.cuboid([x - 210.0, -210.0, 800.0], [x + 210.0, 210.0, 840.0], lid);
        k.cuboid([x - 120.0, -150.0, 840.0], [x + 120.0, -60.0, 846.0], BLACK);
    }
}

fn swing_set(k: &mut Kit) {
    for x in [-1200.0, 1200.0] {
        k.cylinder([x, -900.0, 0.0], [x, 0.0, 2200.0], 40.0, 10, BODY);
        k.cylinder([x, 900.0, 0.0], [x, 0.0, 2200.0], 40.0, 10, BODY);
    }
    k.cylinder([-1300.0, 0.0, 2200.0], [1300.0, 0.0, 2200.0], 45.0, 12, BODY);
    for x in [-500.0, 500.0] {
        for dx in [-200.0, 200.0] {
            k.cylinder([x + dx, 0.0, 2200.0], [x + dx, 0.0, 450.0], 6.0, 6, CHROME);
        }
        k.cuboid([x - 230.0, -100.0, 420.0], [x + 230.0, 100.0, 450.0], BLACK);
    }
}

// ───────────────────────────── 人物配景 ─────────────────────────────

/// Low-poly scale figure `h` mm tall; `stride` swings the arms and legs (walking).
fn person(k: &mut Kit, h: f64, stride: f64, pants: Paint) {
    let s = h / 1700.0;
    for side in [-1.0f64, 1.0] {
        let foot_y = -side * stride * s;
        k.frustum([side * 90.0 * s, foot_y, 70.0 * s], [side * 90.0 * s, 0.0, 900.0 * s], 55.0 * s, 78.0 * s, 10, pants);
        k.cuboid([side * 90.0 * s - 50.0 * s, foot_y - 150.0 * s, 0.0], [side * 90.0 * s + 50.0 * s, foot_y + 70.0 * s, 75.0 * s], BLACK);
        let hand = [side * 245.0 * s, side * stride * 0.8 * s, 860.0 * s];
        k.cylinder([side * 215.0 * s, 0.0, 1400.0 * s], hand, 42.0 * s, 8, BODY);
        k.sphere(hand, 45.0 * s, SKIN);
    }
    k.frustum([0.0, 0.0, 860.0 * s], [0.0, 0.0, 1430.0 * s], 150.0 * s, 185.0 * s, 14, BODY);
    k.ellipsoid([0.0, 0.0, 1430.0 * s], [215.0 * s, 110.0 * s, 50.0 * s], BODY);
    k.cylinder([0.0, 0.0, 1430.0 * s], [0.0, 0.0, 1500.0 * s], 45.0 * s, 8, SKIN);
    k.sphere([0.0, 0.0, 1580.0 * s], 105.0 * s, SKIN);
    k.ellipsoid([0.0, 15.0 * s, 1615.0 * s], [110.0 * s, 112.0 * s, 85.0 * s], HAIR);
}

fn person_standing(k: &mut Kit) {
    person(k, 1700.0, 0.0, DENIM);
}

fn person_walking(k: &mut Kit) {
    person(k, 1650.0, 260.0, BLACK);
}

fn child(k: &mut Kit) {
    person(k, 1150.0, 120.0, DENIM);
}

fn dog(k: &mut Kit) {
    k.ellipsoid([0.0, 0.0, 420.0], [350.0, 140.0, 150.0], BODY);
    for (x, y) in [(-230.0, -80.0), (-230.0, 80.0), (230.0, -80.0), (230.0, 80.0)] {
        k.cylinder([x, y, 0.0], [x, y, 400.0], 40.0, 8, BODY);
    }
    k.cylinder([300.0, 0.0, 450.0], [400.0, 0.0, 580.0], 70.0, 10, BODY);
    k.sphere([420.0, 0.0, 600.0], 120.0, BODY);
    k.ellipsoid([530.0, 0.0, 570.0], [85.0, 60.0, 55.0], BODY);
    k.sphere([610.0, 0.0, 585.0], 22.0, BLACK);
    for side in [-1.0, 1.0] {
        k.leaf([400.0, side * 100.0, 650.0], [45.0, 25.0, 85.0], 0.0, 0.0, DARK_WOOD);
        k.sphere([510.0, side * 60.0, 630.0], 14.0, BLACK);
    }
    k.cylinder([-330.0, 0.0, 480.0], [-480.0, 0.0, 650.0], 25.0, 8, BODY);
}
