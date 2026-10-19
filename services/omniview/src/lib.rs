//! OmniView — reconstructs a bot's first-person view from the world state
//! OmniCore receives from the real server, and rasterises it to RGB frames.
//!
//! It is a CPU voxel ray caster (no GPU required) so it runs on headless
//! hosts. Textures are optional: point `OMNI_ASSETS_DIR` (or
//! `Renderer::load_textures`) at the `assets/minecraft/textures` folder
//! extracted from your own Minecraft client jar. Without textures, blocks are
//! drawn with vanilla map colours plus a procedural texel pattern. No Mojang
//! assets are stored in this repository.

pub mod hud;
pub mod palette;
pub mod textures;

use omnicore::data::GAME;
use omnicore::world::{Chunk, World};
use palette::{RenderInfo, Shape};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct EntityView {
    pub kind: String,
    pub pos: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub yaw: f32,
    pub is_player: bool,
    pub hurt: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HudState {
    pub hotbar: Vec<(String, i32)>,
    pub selected: usize,
    pub health: f32,
    pub food: i32,
    pub xp_level: i32,
    pub held: String,
    /// 0..1 swing animation progress (None when not swinging).
    pub swing: Option<f32>,
    pub dead: bool,
}

pub struct Scene {
    pub world: World,
    pub eye: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub entities: Vec<EntityView>,
    pub day_time: i64,
    pub raining: bool,
    /// Block being mined and progress 0..1.
    pub digging: Option<((i32, i32, i32), f32)>,
    pub hud: HudState,
    pub frame: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub width: usize,
    pub height: usize,
    pub fov_deg: f64,
    pub max_distance: f64,
    pub threads: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { width: 640, height: 360, fov_deg: 70.0, max_distance: 48.0, threads: 4 }
    }
}

pub struct Renderer {
    pub settings: Settings,
    info: Vec<RenderInfo>,
    pub textures: Option<textures::Textures>,
}

type V3 = [f64; 3];

fn norm(v: V3) -> V3 {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn hash3(x: i32, y: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^ (h >> 15)
}

/// Fast block access with a one-entry chunk cache.
struct Lookup<'a> {
    world: &'a World,
    last: Option<((i32, i32), Option<&'a Arc<Chunk>>)>,
    min_y: i32,
    height: i32,
}

impl<'a> Lookup<'a> {
    fn new(world: &'a World) -> Self {
        Self { world, last: None, min_y: world.dimension.min_y, height: world.dimension.height }
    }
    #[inline]
    fn get(&mut self, x: i32, y: i32, z: i32) -> Option<u32> {
        let key = (x >> 4, z >> 4);
        let ch = match self.last {
            Some((k, c)) if k == key => c,
            _ => {
                let c = self.world.chunks.get(&key);
                self.last = Some((key, c));
                c
            }
        }?;
        let rel = y - self.min_y;
        if rel < 0 || rel >= self.height {
            return Some(0);
        }
        Some(ch.sections[(rel >> 4) as usize].get((x & 15) as usize, (rel & 15) as usize, (z & 15) as usize))
    }
}

struct Hit {
    t: f64,
    color: [f32; 3],
    face: u8,
    block: (i32, i32, i32),
    emissive: bool,
}

impl Renderer {
    pub fn new(settings: Settings) -> Self {
        let mut r = Self { settings, info: palette::build_table(), textures: None };
        if let Ok(dir) = std::env::var("OMNI_ASSETS_DIR") {
            let _ = r.load_textures(&dir);
        }
        r
    }

    pub fn load_textures(&mut self, dir: &str) -> anyhow::Result<usize> {
        let t = textures::Textures::load(dir)?;
        let n = t.len();
        self.textures = Some(t);
        Ok(n)
    }

    pub fn frame_bytes(&self) -> usize {
        self.settings.width * self.settings.height * 3
    }

    /// Renders the scene into `out` (RGB24, row-major).
    pub fn render(&self, scene: &Scene, out: &mut [u8]) {
        let s = self.settings;
        assert_eq!(out.len(), s.width * s.height * 3);
        let yaw = (scene.yaw as f64).to_radians();
        let pitch = (scene.pitch as f64).to_radians();
        let f = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
        let r = [-yaw.cos(), 0.0, -yaw.sin()];
        let u = [r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0]];
        let tan = (s.fov_deg.to_radians() / 2.0).tan();
        let aspect = s.width as f64 / s.height as f64;
        let light = daylight(scene.day_time);
        let sun = sun_dir(scene.day_time);
        let ents: Vec<&EntityView> = scene.entities.iter().filter(|e| dist(e.pos, scene.eye) < s.max_distance).collect();
        let rows_per = s.height.div_ceil(s.threads.max(1));
        std::thread::scope(|sc| {
            for (band, chunk) in out.chunks_mut(rows_per * s.width * 3).enumerate() {
                let ents = &ents;
                sc.spawn(move || {
                    let mut look = Lookup::new(&scene.world);
                    let mut cover: HashMap<(i32, i32), i32> = HashMap::new();
                    let y0 = band * rows_per;
                    for (i, px) in chunk.chunks_mut(3).enumerate() {
                        let x = i % s.width;
                        let y = y0 + i / s.width;
                        let nx = (2.0 * (x as f64 + 0.5) / s.width as f64 - 1.0) * tan * aspect;
                        let ny = (1.0 - 2.0 * (y as f64 + 0.5) / s.height as f64) * tan;
                        let d = norm([f[0] + r[0] * nx + u[0] * ny, f[1] + r[1] * nx + u[1] * ny, f[2] + r[2] * nx + u[2] * ny]);
                        let c = self.shade_ray(scene, &mut look, &mut cover, d, light, sun, ents);
                        px[0] = (c[0].clamp(0.0, 1.0) * 255.0) as u8;
                        px[1] = (c[1].clamp(0.0, 1.0) * 255.0) as u8;
                        px[2] = (c[2].clamp(0.0, 1.0) * 255.0) as u8;
                    }
                });
            }
        });
        if scene.raining {
            hud::rain(out, s.width, s.height, scene.frame);
        }
        hud::draw(out, s.width, s.height, &scene.hud, scene.frame);
    }

    fn sky(&self, d: V3, light: f32, sun: V3, raining: bool) -> [f32; 3] {
        let day_top = [0.47, 0.65, 1.0];
        let day_hor = [0.75, 0.85, 1.0];
        let night_top = [0.01, 0.01, 0.04];
        let night_hor = [0.05, 0.06, 0.12];
        let t = (d[1].max(0.0) as f32).powf(0.6);
        let day = mix(day_hor, day_top, t);
        let night = mix(night_hor, night_top, t);
        let mut c = mix(night, day, light);
        let sd = d[0] * sun[0] + d[1] * sun[1] + d[2] * sun[2];
        if sd > 0.9985 && !raining {
            c = if sun[1] > -0.1 { [1.0, 0.97, 0.8] } else { c };
        }
        let md = -sd;
        if md > 0.9992 && !raining {
            c = [0.85, 0.87, 0.95];
        }
        if light < 0.5 && d[1] > 0.05 && !raining {
            let h = hash3((d[0] * 600.0) as i32, (d[1] * 600.0) as i32, (d[2] * 600.0) as i32);
            if h % 997 == 0 {
                c = [0.9, 0.9, 1.0];
            }
        }
        if raining {
            let g = (c[0] + c[1] + c[2]) / 3.0 * 0.7;
            c = [g, g, g * 1.05];
        }
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn shade_ray(&self, scene: &Scene, look: &mut Lookup, cover: &mut HashMap<(i32, i32), i32>, d: V3, light: f32, sun: V3, ents: &[&EntityView]) -> [f32; 3] {
        let maxd = self.settings.max_distance;
        let o = scene.eye;
        let mut tint: Option<([f32; 3], f64)> = None; // water tint
        let hit = self.cast(look, o, d, maxd, scene, &mut tint);
        // Entities
        let mut ent_hit: Option<(f64, [f32; 3], u8)> = None;
        for e in ents {
            if let Some((t, face, c)) = entity_hit(e, o, d) {
                if ent_hit.map(|h| t < h.0).unwrap_or(true) {
                    ent_hit = Some((t, c, face));
                }
            }
        }
        let mut color;
        let tdist;
        match (&hit, ent_hit) {
            (Some(h), Some(e)) if e.0 < h.t => {
                color = e.1;
                tdist = e.0;
                color = scale(color, face_shade(e.2) * (0.35 + 0.65 * light));
            }
            (None, Some(e)) => {
                color = scale(e.1, face_shade(e.2) * (0.35 + 0.65 * light));
                tdist = e.0;
            }
            (Some(h), _) => {
                tdist = h.t;
                let p = [o[0] + d[0] * h.t, o[1] + d[1] * h.t, o[2] + d[2] * h.t];
                let covered = self.covered(look, cover, h.block, o[1] as i32);
                let ambient = if h.emissive { 1.0 } else if covered { 0.22 + 0.08 * light } else { 0.25 + 0.75 * light };
                color = scale(h.color, face_shade(h.face) * ambient);
                // Mining crack overlay on the block being dug.
                if let Some((b, prog)) = scene.digging {
                    if b == h.block {
                        let stage = (prog * 10.0).floor() as u32;
                        let tex = hash3((p[0] * 16.0) as i32, (p[1] * 16.0) as i32, (p[2] * 16.0) as i32) % 10;
                        if tex < stage {
                            color = scale(color, 0.35);
                        }
                    }
                }
            }
            (None, None) => {
                let c = self.sky(d, light, sun, scene.raining);
                return match tint {
                    Some((t, _)) => mix(c, t, 0.6),
                    None => c,
                };
            }
        }
        // Fog
        let sky_c = self.sky([d[0], 0.0, d[2]], light, sun, scene.raining);
        let fog_start = maxd * 0.6;
        if tdist > fog_start {
            let k = (((tdist - fog_start) / (maxd - fog_start)) as f32).clamp(0.0, 1.0);
            color = mix(color, sky_c, k);
        }
        if let Some((t, wd)) = tint {
            if wd < tdist {
                let depth = ((tdist - wd) / 8.0).min(1.0) as f32;
                color = mix(color, scale(t, 0.35 + 0.65 * light), 0.45 + 0.4 * depth);
            }
        }
        color
    }

    fn covered(&self, look: &mut Lookup, cache: &mut HashMap<(i32, i32), i32>, b: (i32, i32, i32), eye_y: i32) -> bool {
        let top = *cache.entry((b.0, b.2)).or_insert_with(|| {
            let start = (eye_y + 48).min(look.min_y + look.height - 1);
            let end = eye_y - 64;
            let mut y = start;
            while y > end {
                if let Some(s) = look.get(b.0, y, b.2) {
                    if self.info.get(s as usize).map(|i| i.opaque).unwrap_or(false) {
                        return y;
                    }
                }
                y -= 1;
            }
            i32::MIN
        });
        top > b.1
    }

    fn cast(&self, look: &mut Lookup, o: V3, d: V3, maxd: f64, scene: &Scene, tint: &mut Option<([f32; 3], f64)>) -> Option<Hit> {
        let mut x = o[0].floor() as i32;
        let mut y = o[1].floor() as i32;
        let mut z = o[2].floor() as i32;
        let step = [d[0].signum() as i32, d[1].signum() as i32, d[2].signum() as i32];
        let inv = [1.0 / d[0].abs().max(1e-12), 1.0 / d[1].abs().max(1e-12), 1.0 / d[2].abs().max(1e-12)];
        let frac = |v: f64, s: i32| if s > 0 { v.floor() + 1.0 - v } else { v - v.floor() };
        let mut tmax = [frac(o[0], step[0]) * inv[0], frac(o[1], step[1]) * inv[1], frac(o[2], step[2]) * inv[2]];
        let mut t = 0.0;
        let mut face: u8 = 1;
        let _ = scene;
        loop {
            let st = look.get(x, y, z)?;
            if st != 0 {
                let info = &self.info[st as usize];
                match info.shape {
                    Shape::Air => {}
                    Shape::Full => {
                        let p = [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
                        return Some(Hit { t, color: self.texel(info, face, p, (x, y, z)), face, block: (x, y, z), emissive: info.emissive });
                    }
                    Shape::Water => {
                        if tint.is_none() {
                            *tint = Some((info.top, t));
                        }
                    }
                    Shape::Boxes(ref boxes) => {
                        let mut best: Option<(f64, u8)> = None;
                        for b in boxes.iter() {
                            let mn = [x as f64 + b[0], y as f64 + b[1], z as f64 + b[2]];
                            let mx = [x as f64 + b[3], y as f64 + b[4], z as f64 + b[5]];
                            if let Some((tb, fb)) = ray_box(o, d, mn, mx) {
                                if best.map(|bb| tb < bb.0).unwrap_or(true) {
                                    best = Some((tb, fb));
                                }
                            }
                        }
                        if let Some((tb, fb)) = best {
                            let p = [o[0] + d[0] * tb, o[1] + d[1] * tb, o[2] + d[2] * tb];
                            let mut c = self.texel(info, fb, p, (x, y, z));
                            if info.plant {
                                // cut-out pattern so plants read as foliage, not boxes
                                let h = hash3((p[0] * 16.0) as i32, (p[1] * 16.0) as i32, (p[2] * 16.0) as i32);
                                if h % 3 == 0 {
                                    // transparent texel: keep marching
                                    let _ = &mut c;
                                } else {
                                    return Some(Hit { t: tb, color: c, face: fb, block: (x, y, z), emissive: info.emissive });
                                }
                            } else {
                                return Some(Hit { t: tb, color: c, face: fb, block: (x, y, z), emissive: info.emissive });
                            }
                        }
                    }
                }
            }
            // advance
            if tmax[0] < tmax[1] && tmax[0] < tmax[2] {
                x += step[0];
                t = tmax[0];
                tmax[0] += inv[0];
                face = if step[0] > 0 { 4 } else { 5 };
            } else if tmax[1] < tmax[2] {
                y += step[1];
                t = tmax[1];
                tmax[1] += inv[1];
                face = if step[1] > 0 { 0 } else { 1 };
            } else {
                z += step[2];
                t = tmax[2];
                tmax[2] += inv[2];
                face = if step[2] > 0 { 2 } else { 3 };
            }
            if t > maxd {
                return None;
            }
        }
    }

    fn texel(&self, info: &RenderInfo, face: u8, p: V3, b: (i32, i32, i32)) -> [f32; 3] {
        let (u, v) = match face {
            0 | 1 => (p[0] - b.0 as f64, p[2] - b.2 as f64),
            2 | 3 => (p[0] - b.0 as f64, 1.0 - (p[1] - b.1 as f64)),
            _ => (p[2] - b.2 as f64, 1.0 - (p[1] - b.1 as f64)),
        };
        if let Some(tex) = &self.textures {
            if let Some(c) = tex.sample(&info.name, face, u, v) {
                return c;
            }
        }
        let base = match face {
            1 => info.top,
            0 => info.bottom,
            _ => info.side,
        };
        // Grass blocks: green fringe at the top of the side faces.
        let base = if info.name == "grass_block" && face > 1 && v < 0.22 { info.top } else { base };
        let tu = (u * 16.0).floor().clamp(0.0, 15.0) as i32;
        let tv = (v * 16.0).floor().clamp(0.0, 15.0) as i32;
        let n = (hash3(tu + b.0 * 16, tv + b.1 * 16, b.2 * 31 + face as i32) % 1000) as f32 / 1000.0;
        scale(base, 0.86 + 0.18 * n)
    }
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn face_shade(face: u8) -> f32 {
    match face {
        1 => 1.0,
        0 => 0.5,
        2 | 3 => 0.8,
        _ => 0.62,
    }
}

fn dist(a: V3, b: V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// 0 (midnight) .. 1 (noon) brightness from the world clock.
pub fn daylight(day_time: i64) -> f32 {
    let t = (day_time.rem_euclid(24000)) as f32 / 24000.0;
    // time 0 = sunrise(ish) 6:00; 6000 = noon; 18000 = midnight
    let a = (t - 0.25) * std::f32::consts::TAU;
    ((a.cos() * 0.5 + 0.5) * 1.15).clamp(0.08, 1.0)
}

fn sun_dir(day_time: i64) -> V3 {
    let t = (day_time.rem_euclid(24000)) as f64 / 24000.0;
    let a = t * std::f64::consts::TAU;
    norm([a.cos(), a.sin(), 0.15])
}

fn ray_box(o: V3, d: V3, mn: V3, mx: V3) -> Option<(f64, u8)> {
    let mut tmin = f64::NEG_INFINITY;
    let mut tmax = f64::INFINITY;
    let mut face = 0u8;
    for i in 0..3 {
        if d[i].abs() < 1e-12 {
            if o[i] < mn[i] || o[i] > mx[i] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[i];
        let (mut t1, mut t2) = ((mn[i] - o[i]) * inv, (mx[i] - o[i]) * inv);
        let mut f = match i {
            0 => 4,
            1 => 0,
            _ => 2,
        };
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            f += 1;
        }
        if t1 > tmin {
            tmin = t1;
            face = f;
        }
        tmax = tmax.min(t2);
        if tmin > tmax {
            return None;
        }
    }
    if tmax < 0.0 {
        return None;
    }
    Some((tmin.max(0.0), face))
}

fn entity_hit(e: &EntityView, o: V3, d: V3) -> Option<(f64, u8, [f32; 3])> {
    let hw = e.width / 2.0;
    let mn = [e.pos[0] - hw, e.pos[1], e.pos[2] - hw];
    let mx = [e.pos[0] + hw, e.pos[1] + e.height, e.pos[2] + hw];
    let (t, face) = ray_box(o, d, mn, mx)?;
    if t < 0.05 {
        return None;
    }
    let hy = o[1] + d[1] * t - e.pos[1];
    let rel = (hy / e.height) as f32;
    let mut c = palette::entity_color(&e.kind, e.is_player, rel);
    if e.hurt {
        c = mix(c, [1.0, 0.1, 0.1], 0.55);
    }
    Some((t, face, c))
}

/// Builds a render scene from the live OmniCore game state.
pub fn scene_from_state(st: &omnicore::GameState, frame: u64, radius_chunks: i32) -> Scene {
    let eye = st.body.eye();
    let cx = (eye[0].floor() as i32) >> 4;
    let cz = (eye[2].floor() as i32) >> 4;
    let entities = st
        .entities
        .values()
        .map(|e| EntityView {
            kind: e.kind_name.clone(),
            pos: e.pos,
            width: e.width(),
            height: e.height(),
            yaw: e.yaw,
            is_player: e.is_player(),
            hurt: st.ticks.saturating_sub(e.last_hurt_tick) < 8 && e.last_hurt_tick > 0,
        })
        .collect();
    let inv = &st.inventory;
    let hotbar = (0..9).map(|i| {
        let s = &inv.slots[omnicore::inventory::slots::HOTBAR_START + i];
        (s.name().to_string(), s.count)
    });
    let digging = st.digging.map(|(b, start, need)| (b, ((st.ticks.saturating_sub(start)) as f32 / need as f32).min(1.0)));
    let since_swing = st.ticks.saturating_sub(st.last_swing_tick);
    Scene {
        world: st.world.snapshot(cx, cz, radius_chunks),
        eye,
        yaw: st.body.yaw,
        pitch: st.body.pitch,
        entities,
        day_time: st.day_time,
        raining: st.raining,
        digging,
        hud: HudState {
            hotbar: hotbar.collect(),
            selected: inv.selected_hotbar as usize,
            health: st.health,
            food: st.food,
            xp_level: st.xp_level,
            held: inv.held().name().to_string(),
            swing: if since_swing < 6 && st.last_swing_tick > 0 { Some(since_swing as f32 / 6.0) } else { None },
            dead: st.dead,
        },
        frame,
    }
}

pub fn block_color(state: u32) -> [f32; 3] {
    palette::build_table().get(state as usize).map(|i| i.side).unwrap_or([1.0, 0.0, 1.0])
}

pub fn game_version() -> &'static str {
    &GAME.version
}

#[cfg(test)]
mod tests;
