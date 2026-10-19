//! Per-block-state render information derived from vanilla data.

use omnicore::data::{GAME, FLAG_OCCLUDES};

#[derive(Clone, Debug)]
pub enum Shape {
    Air,
    Full,
    Water,
    Boxes(Vec<[f64; 6]>),
}

#[derive(Clone, Debug)]
pub struct RenderInfo {
    pub name: String,
    pub shape: Shape,
    pub top: [f32; 3],
    pub side: [f32; 3],
    pub bottom: [f32; 3],
    pub opaque: bool,
    pub emissive: bool,
    pub plant: bool,
}

pub fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

/// Hand-tuned colours for very common blocks (map colours are too flat for them).
fn override_colors(name: &str) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let s = |c: u32| rgb(c);
    Some(match name {
        "grass_block" => (s(0x6aa84f), s(0x8b6a43), s(0x8b6a43)),
        "dirt" | "coarse_dirt" | "rooted_dirt" => (s(0x8b6a43), s(0x8b6a43), s(0x8b6a43)),
        "stone" => (s(0x7f7f7f), s(0x7f7f7f), s(0x7f7f7f)),
        "cobblestone" => (s(0x6f6f6f), s(0x6f6f6f), s(0x6f6f6f)),
        "deepslate" | "cobbled_deepslate" => (s(0x4d4d52), s(0x4d4d52), s(0x4d4d52)),
        "bedrock" => (s(0x3a3a3a), s(0x3a3a3a), s(0x3a3a3a)),
        "sand" => (s(0xdbd3a0), s(0xdbd3a0), s(0xdbd3a0)),
        "gravel" => (s(0x857f7c), s(0x857f7c), s(0x857f7c)),
        "water" => (s(0x3f76e4), s(0x3f76e4), s(0x3f76e4)),
        "lava" => (s(0xff7a00), s(0xff7a00), s(0xff7a00)),
        "coal_ore" => (s(0x5a5a5a), s(0x5a5a5a), s(0x5a5a5a)),
        "iron_ore" => (s(0x9c8a7a), s(0x9c8a7a), s(0x9c8a7a)),
        "diamond_ore" | "deepslate_diamond_ore" => (s(0x5ecfd0), s(0x5ecfd0), s(0x5ecfd0)),
        "gold_ore" => (s(0xc9ae4a), s(0xc9ae4a), s(0xc9ae4a)),
        "crafting_table" => (s(0xa07040), s(0x8a5a32), s(0xa07040)),
        "furnace" => (s(0x6e6e6e), s(0x5c5c5c), s(0x6e6e6e)),
        "snow" | "snow_block" => (s(0xf5fbff), s(0xf5fbff), s(0xf5fbff)),
        "glass" => (s(0xc8e6f0), s(0xc8e6f0), s(0xc8e6f0)),
        "torch" | "wall_torch" => (s(0xffd060), s(0xffd060), s(0xffd060)),
        _ if name.ends_with("_log") || name.ends_with("_wood") => (s(0xb08c5a), s(0x6b5130), s(0xb08c5a)),
        _ if name.ends_with("_planks") => (s(0xb5935a), s(0xb5935a), s(0xb5935a)),
        _ if name.ends_with("_leaves") => (s(0x3f7a2a), s(0x3f7a2a), s(0x3f7a2a)),
        _ => return None,
    })
}

pub fn build_table() -> Vec<RenderInfo> {
    let mut out = Vec::with_capacity(GAME.states.len());
    for (sid, row) in GAME.states.iter().enumerate() {
        let b = &GAME.blocks[row.0 as usize];
        let name = b.name.clone();
        let base = rgb(b.color);
        let (top, side, bottom) = override_colors(&name).unwrap_or((base, base, base));
        let boxes = GAME.collision(sid as u32);
        let emissive = matches!(name.as_str(), "lava" | "glowstone" | "torch" | "wall_torch" | "lantern" | "sea_lantern" | "jack_o_lantern" | "shroomlight" | "fire" | "magma_block");
        let plant_like = !b.name.contains("air")
            && boxes.is_empty()
            && (GAME.in_tag("flowers", &name) || GAME.in_tag("crops", &name) || matches!(name.as_str(), "short_grass" | "tall_grass" | "fern" | "large_fern" | "dead_bush" | "sugar_cane" | "sweet_berry_bush" | "seagrass" | "tall_seagrass" | "kelp" | "kelp_plant" | "vine" | "torch" | "wall_torch" | "fire"));
        let shape = if matches!(name.as_str(), "air" | "cave_air" | "void_air" | "light" | "barrier" | "structure_void") {
            Shape::Air
        } else if name == "water" || name == "bubble_column" {
            Shape::Water
        } else if name == "lava" {
            Shape::Full
        } else if plant_like {
            let h = if name.contains("tall") || name == "sugar_cane" || name == "kelp_plant" { 1.0 } else { 0.75 };
            if name.contains("torch") {
                Shape::Boxes(vec![[0.44, 0.0, 0.44, 0.56, 0.62, 0.56]])
            } else {
                Shape::Boxes(vec![[0.15, 0.0, 0.48, 0.85, h, 0.52], [0.48, 0.0, 0.15, 0.52, h, 0.85]])
            }
        } else if boxes.is_empty() {
            // Other non-colliding blocks (rails, carpets, signs...) draw as a thin slab.
            if GAME.block_name(sid as u32).contains("carpet") || name.contains("rail") || name.contains("pressure_plate") || name == "snow" {
                Shape::Boxes(vec![[0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]])
            } else {
                Shape::Air
            }
        } else if boxes.len() == 1 && boxes[0] == [0.0, 0.0, 0.0, 1.0, 1.0, 1.0] {
            Shape::Full
        } else {
            Shape::Boxes(boxes.to_vec())
        };
        let opaque = matches!(shape, Shape::Full) && row.3 & FLAG_OCCLUDES != 0 || name.ends_with("_leaves");
        out.push(RenderInfo { name, shape, top, side, bottom, opaque, emissive, plant: plant_like });
    }
    out
}

pub fn entity_color(kind: &str, is_player: bool, rel_height: f32) -> [f32; 3] {
    if is_player {
        return if rel_height > 0.75 {
            rgb(0xc69c7a) // head
        } else if rel_height > 0.4 {
            rgb(0x2fa3b8) // shirt
        } else {
            rgb(0x3b3f9e) // trousers
        };
    }
    let c = match kind {
        "zombie" | "husk" | "drowned" | "zombie_villager" => {
            if rel_height > 0.75 {
                0x5d9e4c
            } else if rel_height > 0.4 {
                0x2fa3b8
            } else {
                0x3b3f9e
            }
        }
        "skeleton" | "stray" | "bogged" => 0xd6d6d6,
        "creeper" => 0x4fb43c,
        "spider" | "cave_spider" => 0x3a302a,
        "enderman" => 0x161616,
        "cow" | "mooshroom" => {
            if rel_height > 0.6 {
                0x4a3424
            } else {
                0xe8e3dc
            }
        }
        "pig" => 0xf0a5a2,
        "sheep" => 0xeeeeee,
        "chicken" => 0xfafafa,
        "villager" | "wandering_trader" => 0x9b7653,
        "item" => 0xf2c94c,
        "experience_orb" => 0x9fe85a,
        "wolf" => 0xd8d8d8,
        "horse" => 0x8a5a32,
        "slime" => 0x7ed36a,
        _ => 0xb04aa8,
    };
    rgb(c)
}
