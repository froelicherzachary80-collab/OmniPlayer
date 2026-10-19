//! Crafting through real container clicks: the server performs the crafting,
//! OmniBrain only moves items like a player would.

use crate::ctx::Ctx;
use crate::tasks::{center, dist3, Goto, Place, Status, Task, B};
use omnicore::data::GAME;
use omnicore::inventory::slots;

#[derive(Clone, Debug)]
pub struct Recipe {
    pub output: String,
    pub count: i32,
    /// Row-major grid; `None` = empty cell. Patterns: exact item, "planks", "log", "stone_mat".
    pub grid: Vec<Vec<Option<&'static str>>>,
}

impl Recipe {
    pub fn width(&self) -> usize {
        self.grid.iter().map(|r| r.len()).max().unwrap_or(0)
    }
    pub fn needs_table(&self) -> bool {
        self.width() > 2 || self.grid.len() > 2
    }
    pub fn ingredients(&self) -> Vec<(&'static str, i32)> {
        let mut m: Vec<(&'static str, i32)> = vec![];
        for c in self.grid.iter().flatten().flatten() {
            match m.iter_mut().find(|(k, _)| k == c) {
                Some(e) => e.1 += 1,
                None => m.push((c, 1)),
            }
        }
        m
    }
}

pub fn matches(pattern: &str, item: &str) -> bool {
    match pattern {
        "planks" => item.ends_with("_planks"),
        "log" => GAME.in_tag("logs", item),
        "stone_mat" => matches!(item, "cobblestone" | "cobbled_deepslate" | "blackstone"),
        "coal" => item == "coal" || item == "charcoal",
        p => p == item,
    }
}

pub fn planks_for(log: &str) -> String {
    let base = log.trim_start_matches("stripped_");
    for suf in ["_log", "_wood", "_stem", "_hyphae"] {
        if let Some(b) = base.strip_suffix(suf) {
            return format!("{b}_planks");
        }
    }
    "oak_planks".into()
}

pub fn recipe(name: &str, inv: &omnicore::inventory::Inventory) -> Option<Recipe> {
    let r = |output: &str, count: i32, grid: Vec<Vec<Option<&'static str>>>| Recipe { output: output.into(), count, grid };
    let p = Some("planks");
    let s = Some("stick");
    let c = Some("stone_mat");
    let n = None;
    Some(match name {
        "planks" => {
            let log = inv.slots[slots::MAIN_START..slots::OFFHAND].iter().find(|s| !s.is_empty() && matches("log", s.name()))?;
            r(&planks_for(log.name()), 4, vec![vec![Some("log")]])
        }
        "stick" => r("stick", 4, vec![vec![p], vec![p]]),
        "crafting_table" => r("crafting_table", 1, vec![vec![p, p], vec![p, p]]),
        "wooden_pickaxe" => r("wooden_pickaxe", 1, vec![vec![p, p, p], vec![n, s, n], vec![n, s, n]]),
        "wooden_axe" => r("wooden_axe", 1, vec![vec![p, p], vec![p, s], vec![n, s]]),
        "wooden_sword" => r("wooden_sword", 1, vec![vec![p], vec![p], vec![s]]),
        "wooden_shovel" => r("wooden_shovel", 1, vec![vec![p], vec![s], vec![s]]),
        "stone_pickaxe" => r("stone_pickaxe", 1, vec![vec![c, c, c], vec![n, s, n], vec![n, s, n]]),
        "stone_axe" => r("stone_axe", 1, vec![vec![c, c], vec![c, s], vec![n, s]]),
        "stone_sword" => r("stone_sword", 1, vec![vec![c], vec![c], vec![s]]),
        "stone_shovel" => r("stone_shovel", 1, vec![vec![c], vec![s], vec![s]]),
        "furnace" => r("furnace", 1, vec![vec![c, c, c], vec![c, n, c], vec![c, c, c]]),
        "chest" => r("chest", 1, vec![vec![p, p, p], vec![p, n, p], vec![p, p, p]]),
        "torch" => r("torch", 4, vec![vec![Some("coal")], vec![s]]),
        "iron_pickaxe" => r("iron_pickaxe", 1, vec![vec![Some("iron_ingot"); 3], vec![n, s, n], vec![n, s, n]]),
        "iron_sword" => r("iron_sword", 1, vec![vec![Some("iron_ingot")], vec![Some("iron_ingot")], vec![s]]),
        "iron_helmet" => r("iron_helmet", 1, vec![vec![Some("iron_ingot"); 3], vec![Some("iron_ingot"), n, Some("iron_ingot")]]),
        "iron_chestplate" => r(
            "iron_chestplate",
            1,
            vec![vec![Some("iron_ingot"), n, Some("iron_ingot")], vec![Some("iron_ingot"); 3], vec![Some("iron_ingot"); 3]],
        ),
        "iron_leggings" => r(
            "iron_leggings",
            1,
            vec![vec![Some("iron_ingot"); 3], vec![Some("iron_ingot"), n, Some("iron_ingot")], vec![Some("iron_ingot"), n, Some("iron_ingot")]],
        ),
        "iron_boots" => r("iron_boots", 1, vec![vec![Some("iron_ingot"), n, Some("iron_ingot")], vec![Some("iron_ingot"), n, Some("iron_ingot")]]),
        "bread" => r("bread", 1, vec![vec![Some("wheat"); 3]]),
        _ => return None,
    })
}

pub fn can_craft(name: &str, inv: &omnicore::inventory::Inventory) -> bool {
    let Some(r) = recipe(name, inv) else { return false };
    r.ingredients().iter().all(|(pat, n)| inv.count_where(|i| matches(pat, i)) >= *n)
}

enum Phase {
    FindTable,
    Approach(Goto),
    PlaceTable(Place),
    Open { since: u64 },
    Fill { cell: usize, step: u8, wait: u64 },
    TakeResult { since: u64 },
    Finish { since: u64 },
}

pub struct Craft {
    pub item: String,
    recipe: Option<Recipe>,
    phase: Phase,
    table: Option<B>,
    window: i32,
    before: i32,
    src: usize,
}

impl Craft {
    pub fn new(item: &str) -> Self {
        Self { item: item.into(), recipe: None, phase: Phase::FindTable, table: None, window: 0, before: 0, src: 0 }
    }

}

fn grid_slot(window: i32, r: usize, c: usize) -> i16 {
    let w = if window == 0 { 2 } else { 3 };
    (1 + r * w + c) as i16
}

/// Window slot index corresponding to a window-0 inventory slot.
fn win_slot(window: i32, inv_slot: usize) -> i16 {
    if window == 0 {
        inv_slot as i16
    } else {
        (inv_slot + 1) as i16
    }
}

impl Task for Craft {
    fn name(&self) -> String {
        format!("craft {}", self.item)
    }

    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.recipe.is_none() {
            let inv = ctx.st().inventory.clone();
            let Some(r) = recipe(&self.item, &inv) else { return Status::Failed(format!("unknown recipe {}", self.item)) };
            if !can_craft(&self.item, &inv) {
                return Status::Failed(format!("missing ingredients for {}", self.item));
            }
            self.before = inv.count(&r.output);
            self.recipe = Some(r);
        }
        let recipe = self.recipe.clone().unwrap();
        let window = self.window;
        let src_slot = self.src;
        let src_ref = &mut self.src;
        let mut next: Option<(Phase, i32)> = None;
        let result = match &mut self.phase {
            Phase::FindTable => {
                if !recipe.needs_table() {
                    self.window = 0;
                    self.phase = Phase::Fill { cell: 0, step: 0, wait: ctx.tick };
                    return Status::Running;
                }
                let feet = ctx.feet_block();
                let table_state = GAME.block_by_name("crafting_table").map(|b| b.default_state).unwrap_or(0);
                let found = ctx.st().world.find_blocks(feet, 24, 1, |s| s == table_state).first().copied();
                match found {
                    Some(t) => {
                        self.table = Some(t);
                        self.phase = Phase::Approach(Goto::new(t, 3.0));
                    }
                    None => {
                        if ctx.st().inventory.count("crafting_table") == 0 {
                            if can_craft("crafting_table", &ctx.st().inventory) {
                                return Status::Failed("need crafting_table (craft it first)".into());
                            }
                            return Status::Failed("no crafting table available".into());
                        }
                        // Place one next to us.
                        let yaw = ctx.st().body.yaw as f64;
                        let (dx, dz) = (-(yaw.to_radians().sin()).round() as i32, (yaw.to_radians().cos()).round() as i32);
                        let (dx, dz) = if dx == 0 && dz == 0 { (1, 0) } else { (dx, dz) };
                        let t = (feet.0 + dx * 2, feet.1, feet.2 + dz * 2);
                        self.table = Some(t);
                        self.phase = Phase::PlaceTable(Place::new(t, "crafting_table"));
                    }
                }
                Status::Running
            }
            Phase::PlaceTable(p) => match p.step(ctx) {
                Status::Running => Status::Running,
                Status::Done => {
                    ctx.out.emit("memory", serde_json::json!({"kind":"location","label":"crafting table","pos": self.table}));
                    self.phase = Phase::Open { since: 0 };
                    Status::Running
                }
                Status::Failed(e) => Status::Failed(format!("placing crafting table: {e}")),
            },
            Phase::Approach(g) => {
                let t = self.table.unwrap();
                if dist3(ctx.eye(), center(t)) <= 4.0 {
                    self.phase = Phase::Open { since: 0 };
                    return Status::Running;
                }
                match g.step(ctx) {
                    Status::Failed(e) => Status::Failed(format!("reaching table: {e}")),
                    _ => Status::Running,
                }
            }
            Phase::Open { since } => {
                let t = self.table.unwrap();
                ctx.look_at_block(t);
                if *since == 0 {
                    if !ctx.facing(center(t), 15.0) {
                        return Status::Running;
                    }
                    ctx.client.use_item_on(t, 1, [0.5, 1.0, 0.5]);
                    ctx.client.swing();
                    *since = ctx.tick;
                    return Status::Running;
                }
                let open = ctx.st().inventory.open.as_ref().map(|w| (w.id, w.slots.len()));
                if let Some((id, n)) = open {
                    if n >= 46 {
                        next = Some((Phase::Fill { cell: 0, step: 0, wait: ctx.tick }, id));
                    }
                }
                if ctx.tick - *since > 40 {
                    return Status::Failed("crafting table did not open".into());
                }
                Status::Running
            }
            Phase::Fill { cell, step, wait } => {
                if ctx.tick < *wait {
                    return Status::Running;
                }
                let w = recipe.width();
                let cells: Vec<(usize, usize, &str)> = recipe
                    .grid
                    .iter()
                    .enumerate()
                    .flat_map(|(r, row)| row.iter().enumerate().filter_map(move |(c, p)| p.map(|p| (r, c, p))))
                    .collect();
                let _ = w;
                if *cell >= cells.len() {
                    self.phase = Phase::TakeResult { since: ctx.tick };
                    return Status::Running;
                }
                let (r, c, pat) = cells[*cell];
                match *step {
                    0 => {
                        let src = ctx.st().inventory.find(|s| matches(pat, s.name()));
                        let Some(src) = src else { return Status::Failed(format!("ran out of {pat}")) };
                        *src_ref = src;
                        ctx.client.click_slot(window, win_slot(window, src), 0, 0); // pick up stack
                        *step = 1;
                    }
                    1 => {
                        ctx.client.click_slot(window, grid_slot(window, r, c), 1, 0); // place one
                        *step = 2;
                    }
                    _ => {
                        ctx.client.click_slot(window, win_slot(window, src_slot), 0, 0); // put rest back
                        *step = 0;
                        *cell += 1;
                    }
                }
                *wait = ctx.tick + 2;
                Status::Running
            }
            Phase::TakeResult { since } => {
                if ctx.tick - *since < 3 {
                    return Status::Running;
                }
                ctx.client.click_slot(self.window, 0, 0, 1); // shift-click result
                self.phase = Phase::Finish { since: ctx.tick };
                Status::Running
            }
            Phase::Finish { since } => {
                let now = ctx.st().inventory.count(&recipe.output);
                if now > self.before {
                    if self.window != 0 {
                        ctx.client.close_window(self.window);
                    }
                    ctx.out.emit("crafted", serde_json::json!({ "item": recipe.output, "count": now - self.before, "recipe_yield": recipe.count }));
                    return Status::Done;
                }
                if ctx.tick - *since > 30 {
                    if self.window != 0 {
                        ctx.client.close_window(self.window);
                    } else {
                        // Return anything left in the 2x2 grid to the inventory.
                        for s in 1..=4 {
                            ctx.client.click_slot(0, s, 0, 1);
                        }
                    }
                    return Status::Failed(format!("crafting {} produced no result", recipe.output));
                }
                Status::Running
            }
        };
        if let Some((phase, win)) = next {
            self.phase = phase;
            self.window = win;
        }
        result
    }

    fn abort(&mut self, ctx: &mut Ctx) {
        if self.window != 0 {
            ctx.client.close_window(self.window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plank_names() {
        assert_eq!(planks_for("oak_log"), "oak_planks");
        assert_eq!(planks_for("stripped_birch_log"), "birch_planks");
        assert_eq!(planks_for("crimson_stem"), "crimson_planks");
    }

    #[test]
    fn recipe_shapes() {
        let inv = omnicore::inventory::Inventory::default();
        let r = recipe("wooden_pickaxe", &inv).unwrap();
        assert!(r.needs_table());
        assert_eq!(r.ingredients(), vec![("planks", 3), ("stick", 2)]);
        assert!(!recipe("crafting_table", &inv).unwrap().needs_table());
        assert!(!recipe("stick", &inv).unwrap().needs_table());
    }
}
