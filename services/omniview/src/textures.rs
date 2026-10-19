//! Optional block textures loaded from a user-supplied Minecraft client
//! assets folder (`.../assets/minecraft/textures`). Nothing is bundled.

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Texture {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<[f32; 3]>,
}

pub struct Textures {
    dir: PathBuf,
    cache: Mutex<HashMap<String, Option<std::sync::Arc<Texture>>>>,
}

fn decode_png(path: &Path) -> Option<Texture> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(file);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let ch = info.color_type.samples();
    let w = info.width as usize;
    // Animated textures are vertical strips: use the first frame only.
    let h = (info.height as usize).min(w);
    let mut rgb = Vec::with_capacity(w * h);
    for i in 0..w * h {
        let p = &buf[i * ch..i * ch + ch];
        let (r, g, b) = if ch >= 3 { (p[0], p[1], p[2]) } else { (p[0], p[0], p[0]) };
        rgb.push([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]);
    }
    Some(Texture { w, h, rgb })
}

fn tint_for(name: &str) -> Option<[f32; 3]> {
    if name == "grass_block_top" || name.starts_with("short_grass") || name == "fern" || name == "tall_grass_top" || name == "tall_grass_bottom" {
        Some([0.55, 0.74, 0.35])
    } else if name.ends_with("_leaves") && !name.starts_with("cherry") && !name.starts_with("azalea") && !name.starts_with("flowering") && !name.starts_with("pale") {
        Some([0.45, 0.68, 0.28])
    } else if name == "vine" {
        Some([0.45, 0.68, 0.28])
    } else if name.starts_with("water") {
        Some([0.25, 0.46, 0.89])
    } else {
        None
    }
}

impl Textures {
    pub fn load(dir: &str) -> Result<Self> {
        let p = PathBuf::from(dir);
        let block_dir = if p.join("block").is_dir() { p.join("block") } else { p.clone() };
        if !block_dir.join("stone.png").exists() {
            bail!("{} does not look like a Minecraft textures folder (block/stone.png missing)", dir);
        }
        Ok(Self { dir: block_dir, cache: Mutex::new(HashMap::new()) })
    }

    pub fn len(&self) -> usize {
        std::fs::read_dir(&self.dir).map(|d| d.count()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, name: &str) -> Option<std::sync::Arc<Texture>> {
        let mut c = self.cache.lock().unwrap();
        if let Some(t) = c.get(name) {
            return t.clone();
        }
        let t = decode_png(&self.dir.join(format!("{name}.png"))).map(|mut t| {
            if let Some(tint) = tint_for(name) {
                for px in t.rgb.iter_mut() {
                    *px = [px[0] * tint[0], px[1] * tint[1], px[2] * tint[2]];
                }
            }
            std::sync::Arc::new(t)
        });
        c.insert(name.to_string(), t.clone());
        t
    }

    /// Candidate texture names for a block face, most specific first.
    fn candidates(block: &str, face: u8) -> Vec<String> {
        let top = face == 1;
        let bottom = face == 0;
        let mut v = vec![];
        match block {
            "grass_block" if top => v.push("grass_block_top".into()),
            "grass_block" if bottom => v.push("dirt".into()),
            "grass_block" => v.push("grass_block_side".into()),
            "water" => v.push("water_still".into()),
            "lava" => v.push("lava_still".into()),
            "crafting_table" if top => v.push("crafting_table_top".into()),
            "crafting_table" => v.push("crafting_table_front".into()),
            "furnace" if top || bottom => v.push("furnace_top".into()),
            "furnace" => v.push("furnace_front".into()),
            _ => {}
        }
        if block.ends_with("_log") || block.ends_with("_stem") {
            if top || bottom {
                v.push(format!("{block}_top"));
            } else {
                v.push(block.to_string());
            }
        }
        if let Some(base) = block.strip_suffix("_wood") {
            v.push(format!("{base}_log"));
        }
        for suffix in ["_slab", "_stairs"] {
            if let Some(base) = block.strip_suffix(suffix) {
                v.push(base.to_string());
                v.push(format!("{base}s"));
                v.push(format!("{base}_planks"));
            }
        }
        if top {
            v.push(format!("{block}_top"));
        }
        if bottom {
            v.push(format!("{block}_bottom"));
        }
        if !top && !bottom {
            v.push(format!("{block}_side"));
        }
        v.push(block.to_string());
        v
    }

    pub fn sample(&self, block: &str, face: u8, u: f64, v: f64) -> Option<[f32; 3]> {
        for name in Self::candidates(block, face) {
            if let Some(t) = self.get(&name) {
                let x = ((u.clamp(0.0, 0.9999)) * t.w as f64) as usize;
                let y = ((v.clamp(0.0, 0.9999)) * t.h as f64) as usize;
                return Some(t.rgb[y * t.w + x]);
            }
        }
        None
    }
}
