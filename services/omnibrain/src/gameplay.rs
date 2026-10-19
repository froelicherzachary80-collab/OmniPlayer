//! Composite gameplay behaviours built from the low-level tasks.

use crate::ctx::Ctx;
use crate::tasks::*;
use omnicore::data::{is_food_animal, GAME};

/// Gather `count` items by mining blocks matching `block_pred` (e.g. logs, stone).
pub struct Gather {
    pub label: String,
    block_pred: fn(&str) -> bool,
    item_pred: fn(&str) -> bool,
    pub count: i32,
    radius: i32,
    cur: Option<Box<dyn Task>>,
    target: Option<B>,
    blacklist: Vec<B>,
    failures: u32,
    allow_dig: bool,
}

impl Gather {
    pub fn new(label: &str, block_pred: fn(&str) -> bool, item_pred: fn(&str) -> bool, count: i32) -> Self {
        Self { label: label.into(), block_pred, item_pred, count, radius: 24, cur: None, target: None, blacklist: vec![], failures: 0, allow_dig: false }
    }
    pub fn digging(mut self) -> Self {
        self.allow_dig = true;
        self
    }
}

impl Task for Gather {
    fn name(&self) -> String {
        match &self.cur {
            Some(t) => format!("{} ({})", self.label, t.name()),
            None => self.label.clone(),
        }
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let have = ctx.st().inventory.count_where(self.item_pred);
        if have >= self.count && self.cur.is_none() {
            return Status::Done;
        }
        if let Some(t) = self.cur.as_mut() {
            match t.step(ctx) {
                Status::Running => return Status::Running,
                Status::Done => {
                    self.cur = None;
                }
                Status::Failed(_) => {
                    self.cur = None;
                    if let Some(b) = self.target.take() {
                        self.blacklist.push(b);
                    }
                    self.failures += 1;
                    if self.failures > 8 {
                        return Status::Failed(format!("{}: too many failures", self.label));
                    }
                }
            }
            return Status::Running;
        }
        let feet = ctx.feet_block();
        let pred = self.block_pred;
        let found = {
            let st = ctx.st();
            st.world.find_blocks(feet, self.radius, 40, |s| pred(GAME.block_name(s)))
        };
        let Some(target) = found.into_iter().find(|b| !self.blacklist.contains(b)) else {
            return Status::Failed(format!("{}: none found nearby", self.label));
        };
        self.target = Some(target);
        let eye = ctx.eye();
        let near = dist3(eye, center(target)) <= 4.2;
        let mut seq: Vec<Box<dyn Task>> = vec![];
        if !near {
            let mut g = Goto::new(target, 3.2);
            if self.allow_dig {
                g = g.digging();
            }
            seq.push(Box::new(g));
        }
        seq.push(Box::new(Dig::new(target)));
        seq.push(Box::new(Collect::new(6.0)));
        self.cur = Some(Box::new(Sequence::new("mine", seq)));
        Status::Running
    }

    fn abort(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.cur.as_mut() {
            t.abort(ctx);
        }
    }
}

pub fn is_log(n: &str) -> bool {
    GAME.in_tag("logs", n)
}
pub fn is_stone_block(n: &str) -> bool {
    matches!(n, "stone" | "cobblestone" | "deepslate" | "cobbled_deepslate")
}
pub fn is_stone_item(n: &str) -> bool {
    matches!(n, "cobblestone" | "cobbled_deepslate" | "blackstone")
}
pub fn is_dirt_block(n: &str) -> bool {
    matches!(n, "dirt" | "grass_block" | "coarse_dirt")
}
pub fn is_build_item(n: &str) -> bool {
    n == "dirt" || is_stone_item(n) || n.ends_with("_planks")
}
pub fn is_diamond_ore(n: &str) -> bool {
    n == "diamond_ore" || n == "deepslate_diamond_ore"
}
pub fn is_diamond(n: &str) -> bool {
    n == "diamond"
}
pub fn is_coal_ore(n: &str) -> bool {
    n == "coal_ore" || n == "deepslate_coal_ore"
}
pub fn is_coal(n: &str) -> bool {
    n == "coal"
}
pub fn is_iron_ore(n: &str) -> bool {
    n == "iron_ore" || n == "deepslate_iron_ore"
}
pub fn is_raw_iron(n: &str) -> bool {
    n == "raw_iron"
}

/// Hunts a nearby food animal and collects the drops.
pub struct Hunt {
    cur: Option<Box<dyn Task>>,
    kills: u32,
    want: u32,
    attempts: u32,
}

impl Hunt {
    pub fn new(want: u32) -> Self {
        Self { cur: None, kills: 0, want, attempts: 0 }
    }
}

impl Task for Hunt {
    fn name(&self) -> String {
        "hunt for food".into()
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if let Some(t) = self.cur.as_mut() {
            match t.step(ctx) {
                Status::Running => return Status::Running,
                Status::Done => {
                    self.kills += 1;
                    self.cur = None;
                }
                Status::Failed(_) => self.cur = None,
            }
            if self.kills >= self.want {
                return Status::Done;
            }
        }
        self.attempts += 1;
        if self.attempts > 6 {
            return if self.kills > 0 { Status::Done } else { Status::Failed("no animals".into()) };
        }
        let pos = ctx.pos();
        let target = {
            let st = ctx.st();
            st.entities
                .values()
                .filter(|e| is_food_animal(&e.kind_name) && e.distance_to(pos) < 32.0)
                .min_by(|a, b| a.distance_to(pos).partial_cmp(&b.distance_to(pos)).unwrap())
                .map(|e| e.id)
        };
        let Some(id) = target else { return Status::Failed("no animals nearby".into()) };
        self.cur = Some(Box::new(Sequence::new("hunt", vec![Box::new(Attack::new(id)), Box::new(Collect::new(8.0))])));
        Status::Running
    }
}

/// Builds a 5×5 house with a door gap and a flat roof around `origin` (floor level).
pub struct BuildHouse {
    origin: B,
    queue: Vec<B>,
    cur: Option<Box<dyn Task>>,
    started: bool,
    failures: u32,
    pub placed: u32,
}

impl BuildHouse {
    pub fn new(origin: B) -> Self {
        let (ox, oy, oz) = origin;
        let mut q = vec![];
        for y in 0..3 {
            for dx in -2..=2i32 {
                for dz in -2..=2i32 {
                    let edge = dx.abs() == 2 || dz.abs() == 2;
                    let door = dz == -2 && dx == 0 && y < 2;
                    if edge && !door {
                        q.push((ox + dx, oy + y, oz + dz));
                    }
                }
            }
        }
        // Roof: outer ring first, then inward so every block has support.
        for ring in [2, 1, 0] {
            for dx in -2..=2i32 {
                for dz in -2..=2i32 {
                    if dx.abs().max(dz.abs()) == ring {
                        q.push((ox + dx, oy + 3, oz + dz));
                    }
                }
            }
        }
        q.reverse(); // pop() from the end
        Self { origin, queue: q, cur: None, started: false, failures: 0, placed: 0 }
    }

    pub fn blocks_needed() -> i32 {
        16 * 3 - 2 + 25
    }
}

impl Task for BuildHouse {
    fn name(&self) -> String {
        format!("build house at {:?} ({} left)", self.origin, self.queue.len())
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if !self.started {
            // Stand in the middle of the footprint.
            let feet = ctx.feet_block();
            if feet != self.origin {
                let p = ctx.pos();
                let target = [self.origin.0 as f64 + 0.5, self.origin.1 as f64, self.origin.2 as f64 + 0.5];
                if dist3(p, target) > 0.4 {
                    let g = self.cur.get_or_insert_with(|| Box::new(Goto::new(self.origin, 0.3)));
                    if let Status::Failed(e) = g.step(ctx) {
                        return Status::Failed(format!("cannot reach build site: {e}"));
                    }
                    return Status::Running;
                }
            }
            self.cur = None;
            self.started = true;
        }
        if let Some(t) = self.cur.as_mut() {
            match t.step(ctx) {
                Status::Running => return Status::Running,
                Status::Done => self.placed += 1,
                Status::Failed(_) => self.failures += 1,
            }
            self.cur = None;
            if self.failures > 12 {
                return Status::Failed("too many placement failures".into());
            }
        }
        let Some(next) = self.queue.pop() else {
            ctx.out.emit("memory", serde_json::json!({"kind":"location","label":"house","pos":[self.origin.0,self.origin.1,self.origin.2]}));
            return Status::Done;
        };
        let item = {
            let st = ctx.st();
            st.inventory.find(|s| is_build_item(s.name())).map(|i| st.inventory.slots[i].name().to_string())
        };
        let Some(item) = item else { return Status::Failed("out of building blocks".into()) };
        self.cur = Some(Box::new(Place::new(next, &item)));
        Status::Running
    }
}

/// Emergency 1×1 shelter: wall in the four sides at feet and head level plus a roof.
pub fn panic_box(feet: B, item: &str) -> Sequence {
    let (x, y, z) = feet;
    let mut v: Vec<Box<dyn Task>> = vec![];
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        v.push(Box::new(Place::new((x + dx, y, z + dz), item)));
        v.push(Box::new(Place::new((x + dx, y + 1, z + dz), item)));
    }
    v.push(Box::new(Place::new((x, y + 2, z), item)));
    let mut s = Sequence::new("emergency shelter", v);
    s.tolerate_failures = true;
    s
}

/// Walks to a random spot some distance away (spontaneous exploration).
pub fn explore_target(ctx: &Ctx, dist: f64) -> Option<B> {
    let p = ctx.pos();
    for _ in 0..12 {
        let a = ctx.rand() * std::f64::consts::TAU;
        let d = dist * (0.6 + ctx.rand() * 0.6);
        let x = (p[0] + a.cos() * d).floor() as i32;
        let z = (p[2] + a.sin() * d).floor() as i32;
        let st = ctx.st();
        if let Some(top) = st.world.surface_y(x, z) {
            let s = st.world.get_or_air(x, top, z);
            if !GAME.is_fluid(s) && GAME.block_name(s) != "lava" {
                return Some((x, top + 1, z));
            }
        }
    }
    None
}
