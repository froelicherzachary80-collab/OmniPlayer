//! Creator memory. The bot keeps a working copy and streams every change to
//! the API (`{"type":"memory"}` lines), which persists it in the database and
//! hands it back on the next start.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerMemory {
    pub name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub interactions: u32,
    /// -100 (rival) .. 100 (close friend)
    pub affinity: i32,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    pub players: HashMap<String, PlayerMemory>,
    pub locations: HashMap<String, [i32; 3]>,
    pub stats: HashMap<String, i64>,
    /// Learned strategy weights, e.g. "flee_threshold".
    pub strategy: HashMap<String, f64>,
    pub recent_events: Vec<String>,
}

impl Memory {
    pub fn meet(&mut self, name: &str, now: i64) -> bool {
        let new = !self.players.contains_key(name);
        let p = self.players.entry(name.to_string()).or_insert_with(|| PlayerMemory { name: name.to_string(), first_seen: now, ..Default::default() });
        p.last_seen = now;
        new
    }

    pub fn interact(&mut self, name: &str, delta: i32, now: i64) {
        self.meet(name, now);
        if let Some(p) = self.players.get_mut(name) {
            p.interactions += 1;
            p.affinity = (p.affinity + delta).clamp(-100, 100);
        }
    }

    pub fn bump(&mut self, stat: &str, by: i64) -> i64 {
        let v = self.stats.entry(stat.to_string()).or_default();
        *v += by;
        *v
    }

    pub fn event(&mut self, e: String) {
        self.recent_events.push(e);
        if self.recent_events.len() > 50 {
            self.recent_events.remove(0);
        }
    }

    pub fn strat(&self, key: &str, default: f64) -> f64 {
        self.strategy.get(key).copied().unwrap_or(default)
    }

    /// Simple reinforcement: nudge a strategy weight after an outcome.
    pub fn learn(&mut self, key: &str, default: f64, delta: f64, lo: f64, hi: f64) -> f64 {
        let v = (self.strat(key, default) + delta).clamp(lo, hi);
        self.strategy.insert(key.to_string(), v);
        v
    }
}
