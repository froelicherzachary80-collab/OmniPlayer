//! Deterministic low-level skills. Each task is a small state machine stepped
//! once per game tick; higher-level planning decides *which* task to run.

use crate::ctx::{best_tool_for, break_ticks, Ctx};
use omnicore::data::GAME;
use omnicore::inventory::slots;
use omnicore::pathfind::{find_path, PathOptions, Step};

pub type B = (i32, i32, i32);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Running,
    Done,
    Failed(String),
}

pub trait Task: Send {
    fn name(&self) -> String;
    fn step(&mut self, ctx: &mut Ctx) -> Status;
    /// Called when the task is interrupted; must release held inputs.
    fn abort(&mut self, _ctx: &mut Ctx) {}
}

pub fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub fn center(b: B) -> [f64; 3] {
    [b.0 as f64 + 0.5, b.1 as f64 + 0.5, b.2 as f64 + 0.5]
}

/// Face (0..5) of block `b` visible from `eye`; also returns the face center.
pub fn face_toward(b: B, eye: [f64; 3]) -> u8 {
    let c = center(b);
    let d = [eye[0] - c[0], eye[1] - c[1], eye[2] - c[2]];
    let ax = d[0].abs();
    let ay = d[1].abs();
    let az = d[2].abs();
    if ay >= ax && ay >= az {
        if d[1] > 0.0 {
            1
        } else {
            0
        }
    } else if ax >= az {
        if d[0] > 0.0 {
            5
        } else {
            4
        }
    } else if d[2] > 0.0 {
        3
    } else {
        2
    }
}

pub fn face_offset(face: u8) -> B {
    match face {
        0 => (0, -1, 0),
        1 => (0, 1, 0),
        2 => (0, 0, -1),
        3 => (0, 0, 1),
        4 => (-1, 0, 0),
        _ => (1, 0, 0),
    }
}

/// Ensures the item in window-0 slot `slot` is in the hotbar and selected.
pub fn hold_slot(ctx: &mut Ctx, slot: usize) -> bool {
    if (slots::HOTBAR_START..slots::OFFHAND).contains(&slot) {
        let hb = (slot - slots::HOTBAR_START) as u8;
        if ctx.st().inventory.selected_hotbar != hb {
            ctx.client.set_hotbar(hb);
        }
        return true;
    }
    // Swap into the currently selected hotbar slot (mode 2 = hotbar swap).
    let hb = ctx.st().inventory.selected_hotbar;
    ctx.client.click_slot(0, slot as i16, hb as i8, 2);
    false
}

pub fn hold_item(ctx: &mut Ctx, pred: impl Fn(&str) -> bool) -> Option<bool> {
    let slot = ctx.st().inventory.find(|s| pred(s.name()))?;
    Some(hold_slot(ctx, slot))
}

// ─────────────────────────────── Goto ───────────────────────────────

pub struct Goto {
    pub goal: B,
    pub tolerance: f64,
    pub allow_dig: bool,
    pub sprint: bool,
    path: Vec<Step>,
    idx: usize,
    stuck: u32,
    last_pos: [f64; 3],
    replans: u32,
    dig: Option<Dig>,
    started: bool,
}

impl Goto {
    pub fn new(goal: B, tolerance: f64) -> Self {
        Self {
            goal,
            tolerance,
            allow_dig: false,
            sprint: false,
            path: vec![],
            idx: 0,
            stuck: 0,
            last_pos: [0.0; 3],
            replans: 0,
            dig: None,
            started: false,
        }
    }
    pub fn digging(mut self) -> Self {
        self.allow_dig = true;
        self
    }
    pub fn sprinting(mut self, s: bool) -> Self {
        self.sprint = s;
        self
    }

    fn plan(&mut self, ctx: &mut Ctx) -> bool {
        let start = ctx.feet_block();
        let opts = PathOptions { allow_dig: self.allow_dig, tolerance: self.tolerance.max(0.0), max_nodes: 12_000, ..Default::default() };
        let st = ctx.st();
        let p = find_path(&st.world, start, self.goal, opts);
        drop(st);
        self.replans += 1;
        match p {
            Some(p) if !p.is_empty() => {
                self.path = p;
                self.idx = 0;
                true
            }
            _ => false,
        }
    }
}

impl Task for Goto {
    fn name(&self) -> String {
        format!("goto {:?}", self.goal)
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let pos = ctx.pos();
        let goal_c = [self.goal.0 as f64 + 0.5, self.goal.1 as f64, self.goal.2 as f64 + 0.5];
        if dist3(pos, goal_c) <= self.tolerance.max(0.6) {
            return Status::Done;
        }
        if let Some(d) = self.dig.as_mut() {
            match d.step(ctx) {
                Status::Running => return Status::Running,
                _ => self.dig = None,
            }
        }
        if !self.started || self.idx >= self.path.len() {
            self.started = true;
            if self.replans > 25 || !self.plan(ctx) {
                return Status::Failed(format!("no path to {:?}", self.goal));
            }
        }
        let step = self.path[self.idx].clone();
        // Clear blocks in the way first.
        for b in &step.dig {
            let air = ctx.st().world.get(b.0, b.1, b.2).map(|s| GAME.collision(s).is_empty()).unwrap_or(false);
            if !air {
                self.dig = Some(Dig::new(*b));
                ctx.controls.forward = 0.0;
                return Status::Running;
            }
        }
        let target = [step.pos.0 as f64 + 0.5, step.pos.1 as f64, step.pos.2 as f64 + 0.5];
        let hd = ((pos[0] - target[0]).powi(2) + (pos[2] - target[2]).powi(2)).sqrt();
        if hd < 0.35 && (pos[1] - target[1]).abs() < 1.1 {
            self.idx += 1;
            self.stuck = 0;
            return Status::Running;
        }
        ctx.walk_toward(target, self.sprint && self.path.len() - self.idx > 4);
        // Look ahead (slightly down) like a player watching where they walk.
        if let Some((yaw, _)) = ctx.look {
            ctx.look = Some((yaw, 8.0));
        }
        let (horizontal_collision, on_ground, in_water) = {
            let st = ctx.st();
            (st.body.horizontal_collision, st.body.on_ground, st.body.in_water)
        };
        if step.jump || (horizontal_collision && on_ground) || in_water && target[1] > pos[1] - 0.2 {
            ctx.controls.jump = target[1] > pos[1] + 0.4 || horizontal_collision || in_water;
        }
        if dist3(pos, self.last_pos) < 0.02 {
            self.stuck += 1;
        } else {
            self.stuck = 0;
        }
        self.last_pos = pos;
        if self.stuck > 30 {
            self.stuck = 0;
            self.started = false; // replan
        }
        Status::Running
    }

    fn abort(&mut self, ctx: &mut Ctx) {
        if let Some(d) = self.dig.as_mut() {
            d.abort(ctx);
        }
    }
}

// ─────────────────────────────── Dig ───────────────────────────────

pub struct Dig {
    pub pos: B,
    face: u8,
    started_at: Option<u64>,
    needed: u32,
    finished_at: Option<u64>,
    equip_wait: u32,
}

impl Dig {
    pub fn new(pos: B) -> Self {
        Self { pos, face: 1, started_at: None, needed: 0, finished_at: None, equip_wait: 0 }
    }
}

impl Task for Dig {
    fn name(&self) -> String {
        format!("dig {:?}", self.pos)
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let state = match ctx.st().world.get(self.pos.0, self.pos.1, self.pos.2) {
            Some(s) => s,
            None => return Status::Failed("block not loaded".into()),
        };
        let name = GAME.block_name(state).to_string();
        if let Some(fin) = self.finished_at {
            // Wait for the server's block update confirming the break.
            if GAME.is_air(state) || GAME.is_fluid(state) {
                return Status::Done;
            }
            if ctx.tick - fin > 20 {
                return Status::Failed(format!("server did not accept breaking {name}"));
            }
            return Status::Running;
        }
        if GAME.is_air(state) {
            return Status::Done;
        }
        let eye = ctx.eye();
        if dist3(eye, center(self.pos)) > 5.0 {
            return Status::Failed("block out of reach".into());
        }
        ctx.look_at_block(self.pos);
        if self.started_at.is_none() {
            // Equip the best tool before starting.
            let tool = best_tool_for(&ctx.st().inventory, &name);
            if let Some(slot) = tool {
                if !hold_slot(ctx, slot) && self.equip_wait < 4 {
                    self.equip_wait += 1;
                    return Status::Running;
                }
            }
            if !ctx.facing(center(self.pos), 12.0) {
                return Status::Running;
            }
            let (held, on_ground, in_water) = {
                let st = ctx.st();
                (st.inventory.held().name().to_string(), st.body.on_ground, st.body.in_water)
            };
            let Some(ticks) = break_ticks(&name, &held, on_ground, in_water) else {
                return Status::Failed(format!("{name} is unbreakable"));
            };
            self.face = face_toward(self.pos, eye);
            ctx.client.start_digging(self.pos, self.face);
            ctx.client.swing();
            self.started_at = Some(ctx.tick);
            self.needed = ticks;
            {
                let mut st = ctx.st();
                let t = st.ticks;
                st.digging = Some((self.pos, t, ticks.max(1)));
            }
            if ticks == 0 {
                self.finished_at = Some(ctx.tick);
            }
            return Status::Running;
        }
        let elapsed = ctx.tick - self.started_at.unwrap();
        ctx.client.swing();
        if elapsed as u32 >= self.needed {
            ctx.client.finish_digging(self.pos, self.face);
            self.finished_at = Some(ctx.tick);
            ctx.st().digging = None;
        }
        Status::Running
    }

    fn abort(&mut self, ctx: &mut Ctx) {
        if self.started_at.is_some() && self.finished_at.is_none() {
            ctx.client.cancel_digging(self.pos, self.face);
        }
        ctx.st().digging = None;
    }
}

// ─────────────────────────────── Place ───────────────────────────────

pub struct Place {
    pub target: B,
    pub item: String,
    phase: u32,
    placed_at: Option<u64>,
}

impl Place {
    pub fn new(target: B, item: &str) -> Self {
        Self { target, item: item.to_string(), phase: 0, placed_at: None }
    }
}

/// Finds a solid neighbor of `target` to place against; returns (neighbor, face on neighbor).
pub fn support_for(ctx: &Ctx, target: B) -> Option<(B, u8)> {
    let st = ctx.st();
    // Prefer placing on top of the block below.
    let order: [(B, u8); 6] = [
        ((0, -1, 0), 1),
        ((0, 0, -1), 3),
        ((0, 0, 1), 2),
        ((-1, 0, 0), 5),
        ((1, 0, 0), 4),
        ((0, 1, 0), 0),
    ];
    for (d, face) in order {
        let n = (target.0 + d.0, target.1 + d.1, target.2 + d.2);
        if let Some(s) = st.world.get(n.0, n.1, n.2) {
            if GAME.is_solid(s) && !GAME.is_fluid(s) {
                return Some((n, face));
            }
        }
    }
    None
}

impl Task for Place {
    fn name(&self) -> String {
        format!("place {} at {:?}", self.item, self.target)
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let cur = match ctx.st().world.get(self.target.0, self.target.1, self.target.2) {
            Some(s) => s,
            None => return Status::Failed("not loaded".into()),
        };
        let cur_name = GAME.block_name(cur);
        if let Some(at) = self.placed_at {
            if !GAME.is_air(cur) && !GAME.is_fluid(cur) {
                return Status::Done;
            }
            if ctx.tick - at > 15 {
                return Status::Failed(format!("server rejected placing {}", self.item));
            }
            return Status::Running;
        }
        if !GAME.is_air(cur) && !GAME.is_fluid(cur) && !GAME.in_tag("replaceable", cur_name) {
            return if cur_name == self.item { Status::Done } else { Status::Failed(format!("occupied by {cur_name}")) };
        }
        // Don't place into our own body.
        let p = ctx.pos();
        let body = omnicore::physics::Aabb::player(p[0], p[1], p[2], 1.8);
        let tb = omnicore::physics::Aabb {
            min: [self.target.0 as f64, self.target.1 as f64, self.target.2 as f64],
            max: [self.target.0 as f64 + 1.0, self.target.1 as f64 + 1.0, self.target.2 as f64 + 1.0],
        };
        if body.intersects(&tb) {
            return Status::Failed("target overlaps the bot".into());
        }
        let item = self.item.clone();
        match hold_item(ctx, |n| n == item) {
            None => return Status::Failed(format!("no {} in inventory", self.item)),
            Some(false) => {
                self.phase += 1;
                return if self.phase > 6 { Status::Failed("could not equip item".into()) } else { Status::Running };
            }
            Some(true) => {}
        }
        let Some((n, face)) = support_for(ctx, self.target) else {
            return Status::Failed("nothing to place against".into());
        };
        let off = face_offset(face);
        let face_center = [n.0 as f64 + 0.5 + off.0 as f64 * 0.5, n.1 as f64 + 0.5 + off.1 as f64 * 0.5, n.2 as f64 + 0.5 + off.2 as f64 * 0.5];
        if dist3(ctx.eye(), face_center) > 4.8 {
            return Status::Failed("too far to place".into());
        }
        ctx.look_at_point(face_center);
        if !ctx.facing(face_center, 15.0) {
            return Status::Running;
        }
        let cursor = [0.5 + off.0 as f32 * 0.5, 0.5 + off.1 as f32 * 0.5, 0.5 + off.2 as f32 * 0.5];
        ctx.client.use_item_on(n, face, cursor);
        ctx.client.swing();
        self.placed_at = Some(ctx.tick);
        Status::Running
    }
}

// ─────────────────────────────── Attack ───────────────────────────────

pub struct Attack {
    pub target: i32,
    last_hit: u64,
    started: u64,
    hits: u32,
    goto: Option<Goto>,
}

impl Attack {
    pub fn new(target: i32) -> Self {
        Self { target, last_hit: 0, started: 0, hits: 0, goto: None }
    }
}

impl Task for Attack {
    fn name(&self) -> String {
        format!("attack #{}", self.target)
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.started == 0 {
            self.started = ctx.tick;
            // Equip the best weapon available.
            let _ = hold_item(ctx, |n| n.ends_with("_sword")).or_else(|| hold_item(ctx, |n| n.ends_with("_axe")));
        }
        let Some((epos, eh)) = ctx.st().entities.get(&self.target).map(|e| (e.pos, e.height())) else {
            return Status::Done; // gone (dead or despawned)
        };
        if ctx.tick - self.started > 20 * 30 {
            return Status::Failed("fight took too long".into());
        }
        let aim = [epos[0], epos[1] + eh * 0.8, epos[2]];
        let d = dist3(ctx.eye(), aim);
        ctx.look_at_point(aim);
        if d > 3.0 {
            if d > 24.0 {
                return Status::Failed("target escaped".into());
            }
            let goal = (epos[0].floor() as i32, epos[1].floor() as i32, epos[2].floor() as i32);
            let g = self.goto.get_or_insert_with(|| Goto::new(goal, 1.8).sprinting(true));
            if (g.goal.0 - goal.0).abs() + (g.goal.2 - goal.2).abs() > 2 {
                *g = Goto::new(goal, 1.8).sprinting(true);
            }
            if d < 7.0 {
                ctx.walk_toward(epos, true);
                ctx.look_at_point(aim);
            } else if let Status::Failed(_) = g.step(ctx) {
                ctx.walk_toward(epos, true);
            }
            return Status::Running;
        }
        self.goto = None;
        // Strafe a little while fighting, like a player.
        ctx.controls.strafe = if (ctx.tick / 15) % 2 == 0 { 0.4 } else { -0.4 };
        let held = ctx.st().inventory.held().name().to_string();
        let cooldown = if held.ends_with("_sword") { 12 } else if held.ends_with("_axe") { 20 } else { 8 };
        if ctx.tick - self.last_hit >= cooldown && ctx.facing(aim, 25.0) {
            ctx.client.attack(self.target);
            self.last_hit = ctx.tick;
            self.hits += 1;
        }
        Status::Running
    }
}

// ─────────────────────────────── Eat ───────────────────────────────

pub struct Eat {
    started: Option<u64>,
    equip: u32,
    food_before: i32,
}

impl Eat {
    pub fn new() -> Self {
        Self { started: None, equip: 0, food_before: 0 }
    }
}

pub fn is_food(name: &str) -> bool {
    GAME.item_by_name(name).map(|i| i.food > 0).unwrap_or(false) && !matches!(name, "rotten_flesh" | "spider_eye" | "poisonous_potato" | "pufferfish")
}

impl Task for Eat {
    fn name(&self) -> String {
        "eat".into()
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        match self.started {
            None => {
                match hold_item(ctx, is_food) {
                    None => return Status::Failed("no food".into()),
                    Some(false) => {
                        self.equip += 1;
                        return if self.equip > 6 { Status::Failed("could not equip food".into()) } else { Status::Running };
                    }
                    Some(true) => {}
                }
                self.food_before = ctx.st().food;
                ctx.client.use_item();
                self.started = Some(ctx.tick);
                Status::Running
            }
            Some(t) => {
                // Eating takes 32 ticks; look slightly down like a player.
                let yaw = ctx.st().body.yaw;
                ctx.look = Some((yaw, 20.0));
                if ctx.st().food > self.food_before {
                    return Status::Done;
                }
                if ctx.tick - t > 45 {
                    ctx.client.release_use_item();
                    return Status::Failed("eating was not confirmed".into());
                }
                Status::Running
            }
        }
    }
    fn abort(&mut self, ctx: &mut Ctx) {
        if self.started.is_some() {
            ctx.client.release_use_item();
        }
    }
}

// ─────────────────────────── Collect drops ───────────────────────────

/// Walks over nearby dropped item entities so the server picks them up.
pub struct Collect {
    radius: f64,
    deadline: u64,
    goto: Option<Goto>,
}

impl Collect {
    pub fn new(radius: f64) -> Self {
        Self { radius, deadline: 0, goto: None }
    }
}

impl Task for Collect {
    fn name(&self) -> String {
        "collect drops".into()
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.deadline == 0 {
            self.deadline = ctx.tick + 20 * 12;
        }
        if ctx.tick > self.deadline {
            return Status::Done;
        }
        let pos = ctx.pos();
        let nearest = {
            let st = ctx.st();
            st.entities
                .values()
                .filter(|e| e.is_item() && e.distance_to(pos) < self.radius)
                .min_by(|a, b| a.distance_to(pos).partial_cmp(&b.distance_to(pos)).unwrap())
                .map(|e| e.pos)
        };
        let Some(ip) = nearest else { return Status::Done };
        if dist3(pos, ip) < 1.2 {
            ctx.controls.forward = 0.0;
            return Status::Running;
        }
        let goal = (ip[0].floor() as i32, ip[1].floor() as i32, ip[2].floor() as i32);
        let g = self.goto.get_or_insert_with(|| Goto::new(goal, 0.8));
        if g.goal != goal {
            *g = Goto::new(goal, 0.8);
        }
        if let Status::Failed(_) = g.step(ctx) {
            ctx.walk_toward(ip, false);
        }
        Status::Running
    }
}

// ─────────────────────────────── Follow ───────────────────────────────

pub struct Follow {
    pub player: String,
    pub distance: f64,
    goto: Option<Goto>,
    lost: u32,
}

impl Follow {
    pub fn new(player: &str, distance: f64) -> Self {
        Self { player: player.to_string(), distance, goto: None, lost: 0 }
    }
}

impl Task for Follow {
    fn name(&self) -> String {
        format!("follow {}", self.player)
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let target = ctx.st().player_entity(&self.player).map(|e| (e.pos, e.height()));
        let Some((tp, h)) = target else {
            self.lost += 1;
            return if self.lost > 20 * 60 { Status::Failed(format!("lost {}", self.player)) } else { Status::Running };
        };
        self.lost = 0;
        let pos = ctx.pos();
        let d = dist3(pos, tp);
        ctx.look_at_point([tp[0], tp[1] + h * 0.9, tp[2]]);
        if d <= self.distance {
            self.goto = None;
            return Status::Running;
        }
        let goal = (tp[0].floor() as i32, tp[1].floor() as i32, tp[2].floor() as i32);
        let g = self.goto.get_or_insert_with(|| Goto::new(goal, self.distance - 0.5).sprinting(d > 8.0));
        if (g.goal.0 - goal.0).abs() + (g.goal.1 - goal.1).abs() + (g.goal.2 - goal.2).abs() > 2 {
            *g = Goto::new(goal, self.distance - 0.5).sprinting(d > 8.0);
        }
        match g.step(ctx) {
            Status::Failed(_) => {
                self.goto = None;
                ctx.walk_toward(tp, d > 8.0);
                let hc = ctx.st().body.horizontal_collision;
                ctx.controls.jump = hc;
            }
            _ => ctx.look_at_point([tp[0], tp[1] + h * 0.9, tp[2]]),
        }
        Status::Running
    }
}

// ─────────────────────────────── Wait ───────────────────────────────

pub struct Wait {
    ticks: u64,
    until: u64,
    look_around: bool,
}

impl Wait {
    pub fn new(ticks: u64, look_around: bool) -> Self {
        Self { ticks, until: 0, look_around }
    }
}

impl Task for Wait {
    fn name(&self) -> String {
        "pause".into()
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.until == 0 {
            self.until = ctx.tick + self.ticks;
        }
        if self.look_around && ctx.tick % 25 == 0 {
            let yaw = ctx.st().body.yaw + (ctx.rand() as f32 - 0.5) * 120.0;
            ctx.look = Some((yaw, (ctx.rand() as f32 - 0.5) * 30.0));
        }
        if ctx.tick >= self.until {
            Status::Done
        } else {
            Status::Running
        }
    }
}

/// Runs a list of tasks in order.
pub struct Sequence {
    pub label: String,
    pub tasks: std::collections::VecDeque<Box<dyn Task>>,
    pub tolerate_failures: bool,
}

impl Sequence {
    pub fn new(label: &str, tasks: Vec<Box<dyn Task>>) -> Self {
        Self { label: label.to_string(), tasks: tasks.into(), tolerate_failures: false }
    }
}

impl Task for Sequence {
    fn name(&self) -> String {
        match self.tasks.front() {
            Some(t) => format!("{} → {}", self.label, t.name()),
            None => self.label.clone(),
        }
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        let Some(t) = self.tasks.front_mut() else { return Status::Done };
        match t.step(ctx) {
            Status::Running => Status::Running,
            Status::Done => {
                self.tasks.pop_front();
                if self.tasks.is_empty() {
                    Status::Done
                } else {
                    Status::Running
                }
            }
            Status::Failed(e) => {
                if self.tolerate_failures {
                    self.tasks.pop_front();
                    Status::Running
                } else {
                    Status::Failed(e)
                }
            }
        }
    }
    fn abort(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.tasks.front_mut() {
            t.abort(ctx);
        }
    }
}
