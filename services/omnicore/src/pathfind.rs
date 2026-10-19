//! A* navigation over the block grid (walk, diagonal walk, jump up, drop down,
//! optional digging through obstacles).

use crate::data::GAME;
use crate::world::World;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

pub type P = (i32, i32, i32);

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub pos: P,
    /// Blocks that must be broken before entering `pos`.
    pub dig: Vec<P>,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PathOptions {
    pub max_nodes: usize,
    pub allow_dig: bool,
    pub max_drop: i32,
    /// Accept a node within this distance of the goal.
    pub tolerance: f64,
}

impl Default for PathOptions {
    fn default() -> Self {
        Self { max_nodes: 20_000, allow_dig: false, max_drop: 3, tolerance: 0.0 }
    }
}

fn passable(world: &World, p: P) -> Option<bool> {
    let s = world.get(p.0, p.1, p.2)?;
    let name = GAME.block_name(s);
    if name == "lava" || name == "fire" || name == "soul_fire" || name == "magma_block" || name == "sweet_berry_bush" || name == "powder_snow" {
        return Some(false);
    }
    Some(GAME.collision(s).is_empty() || is_door_like(name))
}

fn is_door_like(name: &str) -> bool {
    name.ends_with("_door") && !name.starts_with("iron")
}

fn standable(world: &World, p: P) -> Option<bool> {
    let below = world.get(p.0, p.1 - 1, p.2)?;
    let name = GAME.block_name(below);
    if name == "lava" || name == "magma_block" || name == "cactus" || name == "campfire" {
        return Some(false);
    }
    let boxes = GAME.collision(below);
    let top = boxes.iter().map(|b| b[4]).fold(0.0, f64::max);
    Some((!boxes.is_empty() && top >= 0.5 && top <= 1.0 && !name.ends_with("fence") && !name.ends_with("wall"))
        || name == "water"
        || GAME.in_tag("climbable", name))
}

pub fn diggable(world: &World, p: P) -> bool {
    let Some(s) = world.get(p.0, p.1, p.2) else { return false };
    let Some(b) = GAME.block_of_state(s) else { return false };
    b.hardness >= 0.0 && b.hardness < 30.0 && !GAME.is_fluid(s) && !GAME.collision(s).is_empty()
}

#[derive(PartialEq)]
struct Node {
    f: f64,
    g: f64,
    p: P,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

fn h(a: P, b: P) -> f64 {
    let dx = (a.0 - b.0).abs() as f64;
    let dy = (a.1 - b.1).abs() as f64;
    let dz = (a.2 - b.2).abs() as f64;
    let (mn, mx) = if dx < dz { (dx, dz) } else { (dz, dx) };
    mx + mn * 0.414 + dy
}

/// Finds a path from `start` (feet block) to `goal`. Returns the steps excluding the start.
pub fn find_path(world: &World, start: P, goal: P, opt: PathOptions) -> Option<Vec<Step>> {
    let mut open = BinaryHeap::new();
    let mut came: HashMap<P, (P, Vec<P>, bool)> = HashMap::new();
    let mut g: HashMap<P, f64> = HashMap::new();
    g.insert(start, 0.0);
    open.push(Node { f: h(start, goal), g: 0.0, p: start });
    let mut best = (start, h(start, goal));
    let mut expanded = 0;
    while let Some(Node { g: cg, p, .. }) = open.pop() {
        let hd = h(p, goal);
        let reached = p == goal || (opt.tolerance > 0.0 && dist(p, goal) <= opt.tolerance);
        if reached {
            return Some(rebuild(&came, start, p));
        }
        if hd < best.1 {
            best = (p, hd);
        }
        if cg > *g.get(&p).unwrap_or(&f64::MAX) + 1e-9 {
            continue;
        }
        expanded += 1;
        if expanded > opt.max_nodes {
            break;
        }
        for (np, cost, dig, jump) in neighbors(world, p, &opt) {
            let ng = cg + cost;
            if ng + 1e-9 < *g.get(&np).unwrap_or(&f64::MAX) {
                g.insert(np, ng);
                came.insert(np, (p, dig, jump));
                open.push(Node { f: ng + h(np, goal) * 1.05, g: ng, p: np });
            }
        }
    }
    // Partial path toward the closest explored node, so the bot can make progress.
    if best.0 != start && opt.tolerance >= 0.0 {
        return Some(rebuild(&came, start, best.0));
    }
    None
}

fn dist(a: P, b: P) -> f64 {
    (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2) + (a.2 - b.2).pow(2)) as f64).sqrt()
}

fn rebuild(came: &HashMap<P, (P, Vec<P>, bool)>, start: P, end: P) -> Vec<Step> {
    let mut out = vec![];
    let mut cur = end;
    while cur != start {
        let Some((prev, dig, jump)) = came.get(&cur) else { break };
        out.push(Step { pos: cur, dig: dig.clone(), jump: *jump });
        cur = *prev;
    }
    out.reverse();
    out
}

fn neighbors(world: &World, p: P, opt: &PathOptions) -> Vec<(P, f64, Vec<P>, bool)> {
    let mut out = Vec::with_capacity(16);
    let in_water = world.get(p.0, p.1, p.2).map(|s| GAME.block_name(s) == "water").unwrap_or(false);
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
        let diag = dx != 0 && dz != 0;
        let base = if diag { 1.414 } else { 1.0 };
        let n = (p.0 + dx, p.1, p.2 + dz);
        if diag {
            // Avoid cutting corners through walls.
            let a = (p.0 + dx, p.1, p.2);
            let b = (p.0, p.1, p.2 + dz);
            if !(clear2(world, a) && clear2(world, b)) {
                continue;
            }
        }
        // Flat walk
        if clear2(world, n) {
            if standable(world, n) == Some(true) {
                out.push((n, base + water_cost(world, n), vec![], false));
                continue;
            }
            // Drop down
            for d in 1..=opt.max_drop {
                let m = (n.0, n.1 - d, n.2);
                if passable(world, m) != Some(true) {
                    break;
                }
                if standable(world, m) == Some(true) {
                    out.push((m, base + d as f64 * 0.5 + water_cost(world, m), vec![], false));
                    break;
                }
            }
        } else if !diag {
            // Jump up one block
            let up = (n.0, n.1 + 1, n.2);
            let head = (p.0, p.1 + 2, p.2);
            if clear2(world, up) && passable(world, head) == Some(true) && standable(world, up) == Some(true) {
                out.push((up, base + 0.8, vec![], true));
            } else if opt.allow_dig {
                let mut dig = vec![];
                for q in [n, (n.0, n.1 + 1, n.2)] {
                    if passable(world, q) != Some(true) {
                        if diggable(world, q) {
                            dig.push(q);
                        } else {
                            dig.clear();
                            dig.push((i32::MIN, 0, 0));
                            break;
                        }
                    }
                }
                if dig.first().map(|d| d.0 != i32::MIN).unwrap_or(false) && standable(world, n) == Some(true) {
                    let c = base + 2.5 * dig.len() as f64;
                    out.push((n, c, dig, false));
                }
            }
        }
    }
    // Swim/climb up and dig down
    let above = (p.0, p.1 + 1, p.2);
    let feet_name = world.get(p.0, p.1, p.2).map(GAME_block).unwrap_or("");
    if (in_water || GAME.in_tag("climbable", feet_name)) && clear2(world, above) {
        out.push((above, 1.5, vec![], true));
    }
    if opt.allow_dig {
        let below = (p.0, p.1 - 1, p.2);
        if diggable(world, below) && standable(world, below) == Some(true) {
            out.push((below, 3.0, vec![below], false));
        }
    }
    out
}

#[allow(non_snake_case)]
fn GAME_block(s: u32) -> &'static str {
    GAME.block_name(s)
}

fn water_cost(world: &World, p: P) -> f64 {
    match world.get(p.0, p.1, p.2) {
        Some(s) if GAME.block_name(s) == "water" => 2.0,
        _ => 0.0,
    }
}

fn clear2(world: &World, p: P) -> bool {
    passable(world, p) == Some(true) && passable(world, (p.0, p.1 + 1, p.2)) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::tests::flat_world;

    #[test]
    fn straight_path() {
        let w = flat_world();
        let path = find_path(&w, (0, -60, 0), (5, -60, 0), PathOptions::default()).unwrap();
        assert_eq!(path.last().unwrap().pos, (5, -60, 0));
        assert_eq!(path.len(), 5);
    }

    #[test]
    fn goes_around_wall() {
        let mut w = flat_world();
        for z in -3..=3 {
            w.set(2, -60, z, 1);
            w.set(2, -59, z, 1);
            w.set(2, -58, z, 1);
        }
        let path = find_path(&w, (0, -60, 0), (4, -60, 0), PathOptions::default()).unwrap();
        assert_eq!(path.last().unwrap().pos, (4, -60, 0));
        assert!(path.iter().all(|s| s.pos.0 != 2 || s.pos.2.abs() > 3));
    }

    #[test]
    fn jumps_onto_block_and_digs_when_allowed() {
        let mut w = flat_world();
        w.set(1, -60, 0, 1);
        let path = find_path(&w, (0, -60, 0), (1, -59, 0), PathOptions::default()).unwrap();
        assert!(path[0].jump);
        // Enclosed: needs digging
        let mut w2 = flat_world();
        for z in -20..=20 {
            for y in -60..=-55 {
                w2.set(2, y, z, 1);
            }
        }
        let opts = PathOptions { allow_dig: true, max_nodes: 5000, ..Default::default() };
        let path = find_path(&w2, (0, -60, 0), (4, -60, 0), opts).unwrap();
        assert_eq!(path.last().unwrap().pos, (4, -60, 0));
        assert!(path.iter().any(|s| !s.dig.is_empty()));
    }
}
