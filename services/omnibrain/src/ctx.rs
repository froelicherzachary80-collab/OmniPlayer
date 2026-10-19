//! Per-tick context handed to tasks: the connection, a world reference and the
//! movement controls the tick will apply.

use omnicore::data::GAME;
use omnicore::physics::{angle_diff, look_at, Controls};
use omnicore::{Client, GameState};
use rand::Rng;
use std::sync::MutexGuard;

pub struct Ctx<'a> {
    pub client: &'a Client,
    pub controls: Controls,
    /// Desired look direction; the agent rotates toward it smoothly.
    pub look: Option<(f32, f32)>,
    pub tick: u64,
    pub out: &'a crate::io::Output,
}

impl<'a> Ctx<'a> {
    pub fn st(&self) -> MutexGuard<'_, GameState> {
        self.client.state.lock().unwrap()
    }

    pub fn pos(&self) -> [f64; 3] {
        self.st().body.pos
    }

    pub fn eye(&self) -> [f64; 3] {
        self.st().body.eye()
    }

    pub fn feet_block(&self) -> (i32, i32, i32) {
        let p = self.pos();
        (p[0].floor() as i32, (p[1] + 0.01).floor() as i32, p[2].floor() as i32)
    }

    pub fn look_at_point(&mut self, target: [f64; 3]) {
        self.look = Some(look_at(self.eye(), target));
    }

    pub fn look_at_block(&mut self, b: (i32, i32, i32)) {
        self.look_at_point([b.0 as f64 + 0.5, b.1 as f64 + 0.5, b.2 as f64 + 0.5]);
    }

    /// True once the current view is within `tol` degrees of the target.
    pub fn facing(&self, target: [f64; 3], tol: f32) -> bool {
        let (yaw, pitch) = look_at(self.eye(), target);
        let st = self.st();
        angle_diff(st.body.yaw, yaw).abs() < tol && (st.body.pitch - pitch).abs() < tol
    }

    /// Steer horizontally toward a point (sets yaw and forward input).
    pub fn walk_toward(&mut self, target: [f64; 3], sprint: bool) {
        let p = self.pos();
        let (yaw, _) = look_at([p[0], 0.0, p[2]], [target[0], 0.0, target[2]]);
        let pitch = self.look.map(|l| l.1).unwrap_or(0.0);
        self.look = Some((yaw, pitch));
        let cur = self.st().body.yaw;
        // Only push forward once roughly facing the direction (like a human).
        let d = angle_diff(cur, yaw).abs();
        self.controls.forward = if d < 50.0 { 1.0 } else if d < 100.0 { 0.4 } else { 0.0 };
        self.controls.sprint = sprint && d < 20.0;
    }

    pub fn rand(&self) -> f64 {
        rand::thread_rng().gen()
    }
}

/// Tool families used for mining-speed math.
pub fn tool_tier(item: &str) -> Option<(&'static str, f32, u8)> {
    let (kind, rest) = if let Some(m) = item.strip_suffix("_pickaxe") {
        ("pickaxe", m)
    } else if let Some(m) = item.strip_suffix("_axe") {
        ("axe", m)
    } else if let Some(m) = item.strip_suffix("_shovel") {
        ("shovel", m)
    } else if let Some(m) = item.strip_suffix("_hoe") {
        ("hoe", m)
    } else if let Some(m) = item.strip_suffix("_sword") {
        ("sword", m)
    } else {
        return None;
    };
    let (speed, tier) = match rest {
        "wooden" => (2.0, 1),
        "stone" => (4.0, 2),
        "copper" => (5.0, 2),
        "iron" => (6.0, 3),
        "diamond" => (8.0, 4),
        "netherite" => (9.0, 5),
        "golden" => (12.0, 1),
        _ => return None,
    };
    Some((kind, speed, tier))
}

/// Mining speed multiplier & harvest check for `block` using `tool`.
pub fn dig_speed(block: &str, tool: &str) -> (f32, bool) {
    let requires = GAME.block_by_name(block).map(|b| b.requires_tool).unwrap_or(false);
    let needed_tier = if GAME.in_tag("needs_diamond_tool", block) {
        4
    } else if GAME.in_tag("needs_iron_tool", block) {
        3
    } else if GAME.in_tag("needs_stone_tool", block) {
        2
    } else {
        1
    };
    let mut speed = 1.0;
    let mut correct = false;
    if let Some((kind, s, tier)) = tool_tier(tool) {
        let tag = format!("mineable/{kind}");
        if GAME.in_tag(&tag, block) {
            speed = s;
            correct = tier >= needed_tier;
        } else if kind == "sword" && block == "cobweb" {
            speed = 15.0;
            correct = true;
        }
    }
    (speed, !requires || correct)
}

/// Vanilla break time in ticks (client side, assumes on-ground and out of water).
pub fn break_ticks(block: &str, tool: &str, on_ground: bool, in_water: bool) -> Option<u32> {
    let hardness = GAME.block_by_name(block)?.hardness;
    if hardness < 0.0 {
        return None;
    }
    if hardness == 0.0 {
        return Some(0);
    }
    let (mut speed, harvest) = dig_speed(block, tool);
    if !on_ground {
        speed /= 5.0;
    }
    if in_water {
        speed /= 5.0;
    }
    let per_tick = speed / hardness / if harvest { 30.0 } else { 100.0 };
    if per_tick >= 1.0 {
        return Some(0);
    }
    Some((1.0 / per_tick).ceil() as u32)
}

pub fn best_tool_for(inv: &omnicore::inventory::Inventory, block: &str) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for i in omnicore::inventory::slots::MAIN_START..omnicore::inventory::slots::OFFHAND {
        let s = &inv.slots[i];
        if s.is_empty() {
            continue;
        }
        let (speed, harvest) = dig_speed(block, s.name());
        let score = speed + if harvest { 100.0 } else { 0.0 };
        if speed > 1.0 && best.map(|b| score > b.1).unwrap_or(true) {
            best = Some((i, score));
        }
    }
    best.map(|b| b.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_break_times() {
        // Dirt by hand: hardness 0.5 → 0.75 s = 15 ticks.
        assert_eq!(break_ticks("dirt", "air", true, false), Some(15));
        // Stone by hand cannot be harvested: 1.5 hardness, /100 → 150 ticks (7.5 s).
        assert_eq!(break_ticks("stone", "air", true, false), Some(150));
        // Stone with wooden pickaxe: 2/1.5/30 → 23 ticks (1.15 s).
        assert_eq!(break_ticks("stone", "wooden_pickaxe", true, false), Some(23));
        // Oak log by hand: hardness 2 → 60 ticks (3 s).
        assert_eq!(break_ticks("oak_log", "air", true, false), Some(60));
        assert_eq!(break_ticks("short_grass", "air", true, false), Some(0));
        assert_eq!(break_ticks("bedrock", "diamond_pickaxe", true, false), None);
        // Iron ore needs stone tier.
        assert!(!dig_speed("iron_ore", "wooden_pickaxe").1);
        assert!(dig_speed("iron_ore", "stone_pickaxe").1);
    }
}
