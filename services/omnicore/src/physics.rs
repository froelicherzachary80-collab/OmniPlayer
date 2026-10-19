//! Client-side player movement simulation using vanilla movement constants.
//! The server stays authoritative; this keeps the bot's claimed positions
//! physically plausible so they pass the server's movement checks.

use crate::data::GAME;
use crate::world::World;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    pub fn player(x: f64, y: f64, z: f64, height: f64) -> Self {
        Self { min: [x - 0.3, y, z - 0.3], max: [x + 0.3, y + height, z + 0.3] }
    }
    pub fn offset(&self, d: [f64; 3]) -> Self {
        Self {
            min: [self.min[0] + d[0], self.min[1] + d[1], self.min[2] + d[2]],
            max: [self.max[0] + d[0], self.max[1] + d[1], self.max[2] + d[2]],
        }
    }
    pub fn intersects(&self, o: &Aabb) -> bool {
        (0..3).all(|i| self.min[i] < o.max[i] && self.max[i] > o.min[i])
    }
    /// Clips movement `d` along `axis` so self does not penetrate `o`.
    fn clip(&self, o: &Aabb, axis: usize, mut d: f64) -> f64 {
        let (a, b) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        if self.max[a] <= o.min[a] || self.min[a] >= o.max[a] || self.max[b] <= o.min[b] || self.min[b] >= o.max[b] {
            return d;
        }
        if d > 0.0 && self.max[axis] <= o.min[axis] {
            d = d.min(o.min[axis] - self.max[axis]);
        } else if d < 0.0 && self.min[axis] >= o.max[axis] {
            d = d.max(o.max[axis] - self.min[axis]);
        }
        d
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Controls {
    pub forward: f64,
    pub strafe: f64,
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Body {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub fall_distance: f64,
    pub jump_cooldown: u8,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            pos: [0.0; 3],
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            horizontal_collision: false,
            in_water: false,
            in_lava: false,
            fall_distance: 0.0,
            jump_cooldown: 0,
        }
    }
}

pub const GRAVITY: f64 = 0.08;
pub const DRAG: f64 = 0.98;
pub const JUMP_VELOCITY: f64 = 0.42;
pub const WALK_SPEED: f64 = 0.1;
pub const STEP_HEIGHT: f64 = 0.6;

pub fn block_boxes(world: &World, area: &Aabb) -> Vec<Aabb> {
    let mut out = Vec::new();
    let x0 = area.min[0].floor() as i32;
    let x1 = area.max[0].floor() as i32;
    let y0 = area.min[1].floor() as i32 - 1;
    let y1 = area.max[1].floor() as i32;
    let z0 = area.min[2].floor() as i32;
    let z1 = area.max[2].floor() as i32;
    for x in x0..=x1 {
        for z in z0..=z1 {
            for y in y0..=y1 {
                let state = match world.get(x, y, z) {
                    Some(s) => s,
                    None => {
                        // Unloaded chunks are treated as solid walls, like the vanilla client.
                        out.push(Aabb { min: [x as f64, y as f64, z as f64], max: [x as f64 + 1.0, y as f64 + 1.0, z as f64 + 1.0] });
                        continue;
                    }
                };
                for b in GAME.collision(state) {
                    out.push(Aabb {
                        min: [x as f64 + b[0], y as f64 + b[1], z as f64 + b[2]],
                        max: [x as f64 + b[3], y as f64 + b[4], z as f64 + b[5]],
                    });
                }
            }
        }
    }
    out
}

/// Moves `bb` by `d` with collision; returns the actual movement.
pub fn collide(world: &World, bb: &Aabb, d: [f64; 3]) -> [f64; 3] {
    let swept = Aabb {
        min: [bb.min[0] + d[0].min(0.0), bb.min[1] + d[1].min(0.0), bb.min[2] + d[2].min(0.0)],
        max: [bb.max[0] + d[0].max(0.0), bb.max[1] + d[1].max(0.0), bb.max[2] + d[2].max(0.0)],
    };
    let boxes = block_boxes(world, &swept);
    let mut cur = *bb;
    let mut out = [0.0; 3];
    // Vanilla resolves Y first, then the larger horizontal axis.
    let order = if d[0].abs() < d[2].abs() { [1, 2, 0] } else { [1, 0, 2] };
    for axis in order {
        let mut v = d[axis];
        if v == 0.0 {
            continue;
        }
        for b in &boxes {
            v = cur.clip(b, axis, v);
        }
        let mut delta = [0.0; 3];
        delta[axis] = v;
        cur = cur.offset(delta);
        out[axis] = v;
    }
    out
}

pub fn is_colliding(world: &World, bb: &Aabb) -> bool {
    block_boxes(world, bb).iter().any(|b| b.intersects(bb))
}

fn fluid_at(world: &World, bb: &Aabb) -> (bool, bool) {
    let mut water = false;
    let mut lava = false;
    for x in bb.min[0].floor() as i32..=bb.max[0].floor() as i32 {
        for y in bb.min[1].floor() as i32..=(bb.min[1] + 0.4).floor() as i32 {
            for z in bb.min[2].floor() as i32..=bb.max[2].floor() as i32 {
                let s = world.get_or_air(x, y, z);
                match GAME.block_name(s) {
                    "water" | "bubble_column" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" => water = true,
                    "lava" => lava = true,
                    _ => {
                        if GAME.prop(s, "waterlogged") == Some("true") {
                            water = true
                        }
                    }
                }
            }
        }
    }
    (water, lava)
}

impl Body {
    pub fn height(&self, sneak: bool) -> f64 {
        if sneak {
            1.5
        } else {
            1.8
        }
    }

    pub fn aabb(&self) -> Aabb {
        Aabb::player(self.pos[0], self.pos[1], self.pos[2], 1.8)
    }

    pub fn eye(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + 1.62, self.pos[2]]
    }

    /// Advances one 50 ms game tick. Returns false if the current chunk is not loaded.
    pub fn tick(&mut self, world: &World, c: &Controls) -> bool {
        if !world.is_loaded(self.pos[0].floor() as i32, self.pos[2].floor() as i32) {
            return false;
        }
        let bb = self.aabb();
        let (water, lava) = fluid_at(world, &bb);
        self.in_water = water;
        self.in_lava = lava;
        if self.jump_cooldown > 0 {
            self.jump_cooldown -= 1;
        }

        let mut fwd = c.forward.clamp(-1.0, 1.0);
        let mut strafe = c.strafe.clamp(-1.0, 1.0);
        if c.sneak {
            fwd *= 0.3;
            strafe *= 0.3;
        }
        // Vanilla scales raw input by 0.98.
        fwd *= 0.98;
        strafe *= 0.98;
        let sprinting = c.sprint && fwd > 0.0 && !c.sneak;

        let below = world.get_or_air(self.pos[0].floor() as i32, (self.pos[1] - 0.5).floor() as i32, self.pos[2].floor() as i32);
        let slip = GAME.block_of_state(below).map(|b| b.friction as f64).unwrap_or(0.6);

        if water || lava {
            if c.jump {
                self.vel[1] += 0.04;
            }
            self.move_relative(strafe, fwd, 0.02);
            let m = self.do_move(world, c.sneak);
            let drag = if lava { 0.5 } else { 0.8 };
            self.vel = [m[0] * drag, self.vel[1] * drag, m[2] * drag];
            self.vel[1] -= if lava { 0.02 } else { 0.005 };
            if self.horizontal_collision && c.jump {
                self.vel[1] = 0.3;
            }
            self.fall_distance = 0.0;
            return true;
        }

        if c.jump && self.on_ground && self.jump_cooldown == 0 {
            self.vel[1] = JUMP_VELOCITY;
            if sprinting {
                let yaw = (self.yaw as f64).to_radians();
                self.vel[0] -= yaw.sin() * 0.2;
                self.vel[2] += yaw.cos() * 0.2;
            }
            self.jump_cooldown = 10;
        }

        let speed = if sprinting { WALK_SPEED * 1.3 } else { WALK_SPEED };
        let accel = if self.on_ground { speed * (0.21600002 / (slip * slip * slip)) } else if sprinting { 0.026 } else { 0.02 };
        self.move_relative(strafe, fwd, accel);

        // Ladders/vines: limit fall and allow climbing.
        let feet = world.get_or_air(self.pos[0].floor() as i32, self.pos[1].floor() as i32, self.pos[2].floor() as i32);
        let climbing = GAME.in_tag("climbable", GAME.block_name(feet));
        if climbing {
            self.vel[0] = self.vel[0].clamp(-0.15, 0.15);
            self.vel[2] = self.vel[2].clamp(-0.15, 0.15);
            self.vel[1] = self.vel[1].max(-0.15);
            if c.sneak && self.vel[1] < 0.0 {
                self.vel[1] = 0.0;
            }
        }

        let m = self.do_move(world, c.sneak);
        if climbing && (self.horizontal_collision || c.jump) {
            self.vel[1] = 0.2;
        }
        let friction = if self.on_ground { slip * 0.91 } else { 0.91 };
        self.vel[0] = m[0] * friction;
        self.vel[2] = m[2] * friction;
        self.vel[1] = (self.vel[1] - GRAVITY) * DRAG;
        if self.vel[0].abs() < 0.003 {
            self.vel[0] = 0.0;
        }
        if self.vel[2].abs() < 0.003 {
            self.vel[2] = 0.0;
        }
        true
    }

    fn move_relative(&mut self, strafe: f64, fwd: f64, accel: f64) {
        let len2 = strafe * strafe + fwd * fwd;
        if len2 < 1e-7 {
            return;
        }
        let len = len2.sqrt().max(1.0);
        let (s, f) = (strafe / len * accel, fwd / len * accel);
        let yaw = (self.yaw as f64).to_radians();
        let (sin, cos) = (yaw.sin(), yaw.cos());
        self.vel[0] += s * cos - f * sin;
        self.vel[2] += f * cos + s * sin;
    }

    /// Applies velocity with collision, step-up and sneak edge protection. Returns movement done.
    fn do_move(&mut self, world: &World, sneak: bool) -> [f64; 3] {
        let bb = self.aabb();
        let mut d = self.vel;
        if sneak && self.on_ground {
            // Don't walk off edges while sneaking.
            let step = 0.05;
            while d[0] != 0.0 && !is_colliding(world, &bb.offset([d[0], -1.0, 0.0])) {
                d[0] = if d[0].abs() < step { 0.0 } else { d[0] - step * d[0].signum() };
            }
            while d[2] != 0.0 && !is_colliding(world, &bb.offset([0.0, -1.0, d[2]])) {
                d[2] = if d[2].abs() < step { 0.0 } else { d[2] - step * d[2].signum() };
            }
        }
        let mut m = collide(world, &bb, d);
        let blocked_h = (m[0] - d[0]).abs() > 1e-7 || (m[2] - d[2]).abs() > 1e-7;
        if blocked_h && (self.on_ground || (d[1] < 0.0 && m[1] != d[1])) {
            // Try stepping up.
            let up = collide(world, &bb, [0.0, STEP_HEIGHT, 0.0]);
            let raised = bb.offset(up);
            let h = collide(world, &raised, [d[0], 0.0, d[2]]);
            let lowered = raised.offset(h);
            let down = collide(world, &lowered, [0.0, -up[1] + d[1].min(0.0), 0.0]);
            let stepped = [h[0], up[1] + down[1], h[2]];
            if stepped[0] * stepped[0] + stepped[2] * stepped[2] > m[0] * m[0] + m[2] * m[2] + 1e-7 {
                m = stepped;
            }
        }
        self.horizontal_collision = (m[0] - d[0]).abs() > 1e-7 || (m[2] - d[2]).abs() > 1e-7;
        let vertical_collision = (m[1] - d[1]).abs() > 1e-7;
        self.on_ground = vertical_collision && d[1] < 0.0;
        if vertical_collision {
            self.vel[1] = 0.0;
        }
        if self.on_ground {
            self.fall_distance = 0.0;
        } else if m[1] < 0.0 {
            self.fall_distance -= m[1];
        }
        for i in 0..3 {
            self.pos[i] += m[i];
        }
        m
    }
}

/// Yaw/pitch (degrees, Minecraft convention) to look from `from` at `to`.
pub fn look_at(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    let h = (dx * dx + dz * dz).sqrt();
    let yaw = (-dx).atan2(dz).to_degrees();
    let pitch = (-dy).atan2(h).to_degrees();
    (yaw as f32, pitch as f32)
}

/// Shortest signed angular difference in degrees.
pub fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::world::{Blocks, Chunk, Section};

    /// Flat test world: stone up to y=-61 (top surface at y=-60).
    pub fn flat_world() -> World {
        let mut w = World::new();
        for cx in -3..3 {
            for cz in -3..3 {
                let mut sections = vec![];
                for i in 0..24 {
                    let blocks = if i == 0 { Blocks::Full(Box::new([0u16; 4096])) } else { Blocks::Single(0) };
                    sections.push(Section { blocks, biomes: [0; 64], non_air: 0 });
                }
                if let Blocks::Full(a) = &mut sections[0].blocks {
                    for y in 0..4 {
                        for i in 0..256 {
                            a[(y << 8) | i] = 1;
                        }
                    }
                }
                w.chunks.insert((cx, cz), std::sync::Arc::new(Chunk { sections, block_entities: vec![] }));
            }
        }
        w
    }

    #[test]
    fn falls_and_lands() {
        let w = flat_world();
        let mut b = Body { pos: [0.5, -55.0, 0.5], ..Default::default() };
        for _ in 0..60 {
            b.tick(&w, &Controls::default());
        }
        assert!(b.on_ground);
        assert!((b.pos[1] - -60.0).abs() < 1e-6, "y = {}", b.pos[1]);
    }

    #[test]
    fn walks_forward_at_vanilla_speed() {
        let w = flat_world();
        let mut b = Body { pos: [0.5, -60.0, 0.5], on_ground: true, ..Default::default() };
        let c = Controls { forward: 1.0, ..Default::default() };
        for _ in 0..20 {
            b.tick(&w, &c);
        }
        // Vanilla walking speed is ~4.317 blocks/s; yaw 0 faces +Z.
        let dist = b.pos[2] - 0.5;
        assert!(dist > 3.6 && dist < 4.6, "walked {dist}");
        assert!(b.pos[0].abs() - 0.5 < 1e-6);
    }

    #[test]
    fn steps_up_slab_and_blocked_by_wall() {
        let mut w = flat_world();
        let slab = GAME.block_by_name("oak_slab").unwrap().default_state;
        w.set(0, -60, 3, slab);
        let mut b = Body { pos: [0.5, -60.0, 0.5], on_ground: true, ..Default::default() };
        let c = Controls { forward: 1.0, ..Default::default() };
        for _ in 0..20 {
            b.tick(&w, &c);
        }
        assert!(b.pos[2] > 3.5, "should have stepped onto the slab, z={}", b.pos[2]);
        // A two-high wall blocks.
        let mut w2 = flat_world();
        w2.set(0, -60, 3, 1);
        w2.set(0, -59, 3, 1);
        let mut b2 = Body { pos: [0.5, -60.0, 0.5], on_ground: true, ..Default::default() };
        for _ in 0..20 {
            b2.tick(&w2, &c);
        }
        assert!(b2.pos[2] <= 2.7 + 1e-6, "z={}", b2.pos[2]);
        assert!(b2.horizontal_collision);
    }

    #[test]
    fn jumps_one_block() {
        let mut w = flat_world();
        w.set(0, -60, 2, 1);
        let mut b = Body { pos: [0.5, -60.0, 0.5], on_ground: true, ..Default::default() };
        for _ in 0..30 {
            b.tick(&w, &Controls { forward: 1.0, jump: true, ..Default::default() });
        }
        assert!(b.pos[2] > 2.5 && b.pos[1] >= -59.0 - 1e-6, "pos {:?}", b.pos);
    }

    #[test]
    fn look_math() {
        let (yaw, pitch) = look_at([0.0, 0.0, 0.0], [0.0, 0.0, 5.0]);
        assert!(yaw.abs() < 1e-4 && pitch.abs() < 1e-4);
        let (yaw, _) = look_at([0.0, 0.0, 0.0], [-5.0, 0.0, 0.0]);
        assert!((yaw - 90.0).abs() < 1e-3);
        let (_, pitch) = look_at([0.0, 0.0, 0.0], [0.0, -5.0, 0.0001]);
        assert!((pitch - 90.0).abs() < 1e-2);
        assert_eq!(angle_diff(350.0, 10.0), 20.0);
    }
}
