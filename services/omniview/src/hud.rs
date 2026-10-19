//! HUD overlay: crosshair, hotbar, hearts, hunger, XP bar, held item.
//! (Text such as names/coordinates/captions is added by the encoder's
//! drawtext filter so no font is embedded here.)

use crate::palette::rgb;
use crate::HudState;
use omnicore::data::GAME;

fn put(buf: &mut [u8], w: usize, h: usize, x: i64, y: i64, c: [f32; 3], a: f32) {
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        return;
    }
    let i = (y as usize * w + x as usize) * 3;
    for k in 0..3 {
        let old = buf[i + k] as f32 / 255.0;
        buf[i + k] = ((old * (1.0 - a) + c[k] * a).clamp(0.0, 1.0) * 255.0) as u8;
    }
}

fn rect(buf: &mut [u8], w: usize, h: usize, x: i64, y: i64, rw: i64, rh: i64, c: [f32; 3], a: f32) {
    for yy in y..y + rh {
        for xx in x..x + rw {
            put(buf, w, h, xx, yy, c, a);
        }
    }
}

pub fn item_color(name: &str) -> [f32; 3] {
    if name == "air" {
        return [0.0, 0.0, 0.0];
    }
    if let Some(b) = GAME.block_by_name(name) {
        if b.color != 0 {
            return rgb(b.color);
        }
    }
    let base = name.split('_').next().unwrap_or("");
    let c = match base {
        "wooden" | "stick" => 0x9c7a46,
        "stone" => 0x8a8a8a,
        "iron" => 0xd8d8d8,
        "golden" | "gold" => 0xf2d04b,
        "diamond" => 0x5ee8e0,
        "netherite" => 0x4a3f40,
        "coal" => 0x222222,
        "bread" | "wheat" => 0xd6a43c,
        "cooked" | "beef" | "porkchop" | "mutton" | "chicken" => 0xb5533c,
        "apple" => 0xd8352c,
        "torch" => 0xffd060,
        _ => {
            let h = name.bytes().fold(5381u32, |a, b| a.wrapping_mul(33) ^ b as u32);
            0x404040 | (h & 0xbfbfbf)
        }
    };
    rgb(c)
}

pub fn draw(buf: &mut [u8], w: usize, h: usize, hud: &HudState, frame: u64) {
    let s = (h as f32 / 360.0).max(0.5); // scale relative to 360p
    let px = |v: f32| (v * s).round() as i64;
    let (wi, hi) = (w as i64, h as i64);
    // crosshair
    let cx = wi / 2;
    let cy = hi / 2;
    rect(buf, w, h, cx - px(5.0), cy, px(10.0).max(1), px(1.0).max(1), [1.0, 1.0, 1.0], 0.85);
    rect(buf, w, h, cx, cy - px(5.0), px(1.0).max(1), px(10.0).max(1), [1.0, 1.0, 1.0], 0.85);
    // hotbar
    let slot = px(20.0);
    let bar_w = slot * 9;
    let bx = cx - bar_w / 2;
    let by = hi - slot - px(4.0);
    rect(buf, w, h, bx - 2, by - 2, bar_w + 4, slot + 4, [0.05, 0.05, 0.05], 0.55);
    for i in 0..9 {
        let x = bx + i as i64 * slot;
        let sel = i == hud.selected;
        rect(buf, w, h, x + 1, by + 1, slot - 2, slot - 2, [0.35, 0.35, 0.35], 0.5);
        if sel {
            rect(buf, w, h, x - 1, by - 1, slot + 2, px(2.0).max(1), [1.0, 1.0, 1.0], 0.95);
            rect(buf, w, h, x - 1, by + slot - 1, slot + 2, px(2.0).max(1), [1.0, 1.0, 1.0], 0.95);
            rect(buf, w, h, x - 1, by - 1, px(2.0).max(1), slot + 2, [1.0, 1.0, 1.0], 0.95);
            rect(buf, w, h, x + slot - 1, by - 1, px(2.0).max(1), slot + 2, [1.0, 1.0, 1.0], 0.95);
        }
        if let Some((name, count)) = hud.hotbar.get(i) {
            if name != "air" && *count > 0 {
                let m = px(4.0);
                rect(buf, w, h, x + m, by + m, slot - 2 * m, slot - 2 * m, item_color(name), 1.0);
                // stack size as a small bar
                if *count > 1 {
                    let len = ((slot - 2 * m) as f32 * (*count as f32 / 64.0).min(1.0)) as i64;
                    rect(buf, w, h, x + m, by + slot - m, len.max(1), px(1.5).max(1), [1.0, 1.0, 1.0], 0.9);
                }
            }
        }
    }
    // hearts & hunger
    let icon = px(7.0);
    let gap = px(8.0);
    let hy = by - icon - px(4.0);
    for i in 0..10 {
        let hp = hud.health - i as f32 * 2.0;
        let x = bx + i as i64 * gap;
        rect(buf, w, h, x, hy, icon, icon, [0.15, 0.0, 0.0], 0.8);
        if hp >= 2.0 {
            rect(buf, w, h, x + 1, hy + 1, icon - 2, icon - 2, [0.9, 0.08, 0.1], 1.0);
        } else if hp >= 1.0 {
            rect(buf, w, h, x + 1, hy + 1, (icon - 2) / 2, icon - 2, [0.9, 0.08, 0.1], 1.0);
        }
        let food = hud.food - i * 2;
        let fx = bx + bar_w - (i as i64 + 1) * gap;
        rect(buf, w, h, fx, hy, icon, icon, [0.12, 0.07, 0.02], 0.8);
        if food >= 2 {
            rect(buf, w, h, fx + 1, hy + 1, icon - 2, icon - 2, [0.72, 0.45, 0.2], 1.0);
        } else if food >= 1 {
            rect(buf, w, h, fx + icon / 2, hy + 1, (icon - 2) / 2, icon - 2, [0.72, 0.45, 0.2], 1.0);
        }
    }
    // XP bar
    let xy = by - px(3.0);
    rect(buf, w, h, bx, xy, bar_w, px(2.0).max(1), [0.1, 0.2, 0.05], 0.8);
    let lvl = (hud.xp_level as f32 / 30.0).min(1.0);
    rect(buf, w, h, bx, xy, (bar_w as f32 * lvl) as i64, px(2.0).max(1), [0.5, 0.95, 0.2], 1.0);
    // held item / hand (bottom right) with swing animation
    let swing = hud.swing.map(|p| (p * std::f32::consts::PI).sin()).unwrap_or(0.0);
    let bob = ((frame as f32) * 0.15).sin() * px(1.5) as f32;
    let hx = wi - px(90.0) - (swing * px(30.0) as f32) as i64;
    let hy2 = hi - px(70.0) + (swing * px(25.0) as f32) as i64 + bob as i64;
    let arm = rgb(0xc69c7a);
    rect(buf, w, h, hx + px(20.0), hy2 + px(20.0), px(26.0), px(70.0), arm, 1.0);
    if hud.held != "air" {
        rect(buf, w, h, hx, hy2, px(30.0), px(30.0), item_color(&hud.held), 1.0);
    }
    if hud.dead {
        rect(buf, w, h, 0, 0, wi, hi, [0.6, 0.0, 0.0], 0.45);
    }
}

pub fn rain(buf: &mut [u8], w: usize, h: usize, frame: u64) {
    let mut seed = (frame as u32).wrapping_mul(2654435761);
    for _ in 0..(w * h / 900) {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let x = (seed as usize) % w;
        let y = ((seed >> 11) as usize) % h;
        for k in 0..(h / 40).max(4) {
            let yy = y + k;
            let xx = x + k / 4;
            if yy < h && xx < w {
                let i = (yy * w + xx) * 3;
                for c in 0..3 {
                    buf[i + c] = ((buf[i + c] as u16 * 3 + 200) / 4) as u8;
                }
            }
        }
    }
}
