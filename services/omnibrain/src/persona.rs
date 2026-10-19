//! Creator personality, gameplay-aware commentary and dialogue.
//!
//! Lines are always grounded in a structured `Situation` built from the real
//! game state. With an optional local LLM endpoint (Ollama-compatible), the
//! situation is turned into a prompt; otherwise personality-weighted templates
//! are used. No paid API is required.

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Persona {
    pub streamer_name: String,
    pub personality: String,
    pub speaking_style: String,
    /// 0..1: how often the creator talks unprompted.
    pub chattiness: f64,
    /// 0..1: calm → hyped.
    pub energy: f64,
    /// 0..1: careful → reckless.
    pub risk: f64,
    pub catchphrases: Vec<String>,
    pub likes: Vec<String>,
    pub goals: Vec<String>,
    pub friends: Vec<String>,
    pub rivals: Vec<String>,
    pub skill_level: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// e.g. http://127.0.0.1:11434 (Ollama). Empty = disabled.
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
}

/// Structured, real-game context for a line of dialogue.
#[derive(Clone, Debug, Serialize)]
pub struct Situation {
    pub event: String,
    pub detail: String,
    pub pos: [i64; 3],
    pub biome: String,
    pub health: f32,
    pub food: i32,
    pub goal: String,
    pub nearby_players: Vec<String>,
    pub inventory: Vec<(String, i32)>,
    pub recent_chat: Vec<String>,
    pub speaker: Option<String>,
}

fn pick<'a>(v: &[&'a str]) -> &'a str {
    v.choose(&mut rand::thread_rng()).copied().unwrap_or("")
}

pub struct Voice {
    pub persona: Persona,
    llm: LlmConfig,
    tx: mpsc::Sender<(String, Situation)>,
    pub rx: mpsc::Receiver<(String, String)>,
    last_lines: Vec<String>,
}

impl Voice {
    pub fn new(persona: Persona, llm: LlmConfig) -> Self {
        let (tx, worker_rx) = mpsc::channel::<(String, Situation)>();
        let (worker_tx, rx) = mpsc::channel::<(String, String)>();
        let llm2 = llm.clone();
        let persona2 = persona.clone();
        std::thread::spawn(move || {
            for (tag, sit) in worker_rx {
                if let Some(line) = llm_line(&llm2, &persona2, &sit) {
                    let _ = worker_tx.send((tag, line));
                }
            }
        });
        Self { persona, llm, tx, rx, last_lines: vec![] }
    }

    pub fn llm_enabled(&self) -> bool {
        !self.llm.endpoint.is_empty()
    }

    /// Requests an LLM line asynchronously (result arrives on `rx` with `tag`).
    pub fn request(&self, tag: &str, sit: Situation) {
        if self.llm_enabled() {
            let _ = self.tx.send((tag.to_string(), sit));
        }
    }

    fn hype(&self) -> bool {
        rand::thread_rng().gen_bool(self.persona.energy.clamp(0.05, 0.95))
    }

    fn remember(&mut self, s: String) -> Option<String> {
        if self.last_lines.iter().any(|l| l == &s) {
            return None; // avoid repeating ourselves
        }
        self.last_lines.push(s.clone());
        if self.last_lines.len() > 12 {
            self.last_lines.remove(0);
        }
        Some(s)
    }

    /// Template commentary for a real gameplay event.
    pub fn commentary(&mut self, sit: &Situation) -> Option<String> {
        let hype = self.hype();
        let d = sit.detail.as_str();
        let line = match sit.event.as_str() {
            "found_ore" if d.contains("diamond") => {
                if hype {
                    format!("WAIT, NO WAY! I actually found {d} down here!")
                } else {
                    format!("Oh nice, {d}. That's a good find.")
                }
            }
            "found_ore" => {
                if hype {
                    format!("Ooh, {d}! Let's grab that.")
                } else {
                    format!("Found some {d}, I'll mine it.")
                }
            }
            "hurt" => {
                if sit.health <= 6.0 {
                    format!("WHAT?! {} I'm on {} hearts!", if d.is_empty() { "Who hit me?".to_string() } else { format!("{d} hit me!") }, (sit.health / 2.0).ceil())
                } else if hype {
                    format!("Ow! {}", if d.is_empty() { "What was that?".to_string() } else { format!("{d}, really?") })
                } else {
                    "Ouch. Okay, careful.".into()
                }
            }
            "low_health" => pick(&["I really need to heal up.", "Okay chat, I'm low. Playing it safe.", "Not great, not great. Need food."]).into(),
            "died" => {
                if hype {
                    "NOOO. I can't believe that just happened. Okay, going back for my stuff.".into()
                } else {
                    "Well... that happened. Let's go get my items back.".into()
                }
            }
            "crafted" => format!("{} {d}!", pick(&["Just crafted a", "Got myself a", "Look at this,", "New gear:"])),
            "player_joined" => format!("{}", pick(&[
                &format!("Oh hey {d}! You finally got on."),
                &format!("Welcome {d}! Come check out what I've been doing!"),
                &format!("{d} is here! Let's go!"),
            ])),
            "player_near" => format!("{}", pick(&[&format!("Hey {d}!"), &format!("Oh, hi {d}. What are you up to?"), &format!("{d}! Perfect timing.")])),
            "night" => pick(&["It's getting dark, mobs are gonna spawn.", "Night time. Time to find some shelter.", "Okay it's night, let's be careful."]).into(),
            "morning" => pick(&["Sun's up! Let's get back to work.", "Morning! Good, we survived the night."]).into(),
            "built" => format!("Okay chat, {d} is done. {}", pick(&["I think it looks pretty good.", "Not bad, right?", "Maybe a second floor next?"])),
            "goal" => format!("{} {d}.", pick(&["Alright, next up:", "Okay, plan:", "New plan:", "Let's"])),
            "killed" => {
                if hype {
                    format!("Got it! {d} down!")
                } else {
                    format!("Took care of that {d}.")
                }
            }
            "achievement" => format!("Let's GO! {d}!"),
            "biome" => format!("Oh, a {d}. {}", pick(&["Pretty.", "Never seen this one in this world yet.", "Could be a good spot for a base."])),
            _ => return None,
        };
        let line = if !self.persona.catchphrases.is_empty() && rand::thread_rng().gen_bool(0.08) {
            format!("{line} {}", self.persona.catchphrases.choose(&mut rand::thread_rng()).unwrap())
        } else {
            line
        };
        self.remember(line)
    }
}

fn llm_line(cfg: &LlmConfig, p: &Persona, sit: &Situation) -> Option<String> {
    if cfg.endpoint.is_empty() {
        return None;
    }
    let prompt = format!(
        "You are {name}, an AI Minecraft streamer (openly AI-generated, fictional). Personality: {pers}. Speaking style: {style}.\n\
         Reply with ONE short spoken line (max 20 words) reacting to the real game situation below. Do not invent facts that are not in the situation.\n\
         Situation (JSON): {sit}\nLine:",
        name = p.streamer_name,
        pers = p.personality,
        style = p.speaking_style,
        sit = serde_json::to_string(sit).unwrap_or_default()
    );
    let model = if cfg.model.is_empty() { "llama3.2" } else { &cfg.model };
    let resp = ureq::post(&format!("{}/api/generate", cfg.endpoint.trim_end_matches('/')))
        .timeout(Duration::from_millis(if cfg.timeout_ms == 0 { 8000 } else { cfg.timeout_ms }))
        .send_json(json!({ "model": model, "prompt": prompt, "stream": false, "options": { "num_predict": 48, "temperature": 0.8 } }))
        .ok()?;
    let v: serde_json::Value = resp.into_json().ok()?;
    let text = v.get("response")?.as_str()?.trim().trim_matches('"').lines().next()?.to_string();
    if text.is_empty() {
        None
    } else {
        Some(text.chars().take(200).collect())
    }
}

/// Answers a direct question using the bot's actual state.
pub fn answer(q: &str, sit: &Situation) -> Option<String> {
    let q = q.to_lowercase();
    let inv: HashMap<&str, i32> = sit.inventory.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let count = |names: &[&str]| -> i32 { names.iter().map(|n| inv.get(n).copied().unwrap_or(0)).sum() };
    if q.contains("diamond") && (q.contains("how many") || q.contains("enough") || q.contains("have")) {
        let n = count(&["diamond"]);
        return Some(match n {
            0 => "I don't have any diamonds yet.".into(),
            1..=2 => format!("I've only got {n}. Not enough for armor."),
            n => format!("I've got {n} diamonds{}", if n >= 24 { ", that's enough for a full set!" } else { "." }),
        });
    }
    if q.contains("iron") && (q.contains("how many") || q.contains("have")) {
        return Some(format!("I have {} iron ingots and {} raw iron.", count(&["iron_ingot"]), count(&["raw_iron"])));
    }
    if q.contains("where are you") || q.contains("coords") || q.contains("coordinates") {
        return Some(format!("I'm at {} {} {} in a {}.", sit.pos[0], sit.pos[1], sit.pos[2], sit.biome));
    }
    if q.contains("what are you doing") || q.contains("what's the plan") || q.contains("whats the plan") {
        return Some(if sit.goal.is_empty() { "Just looking around right now.".into() } else { format!("Right now I'm working on: {}.", sit.goal) });
    }
    if q.contains("how are you") || q.contains("health") {
        return Some(format!("I'm on {:.0} hearts with {} hunger.", sit.health / 2.0, sit.food));
    }
    if q.contains("ender dragon") || q.contains("the dragon") {
        let armor = sit.inventory.iter().any(|(k, _)| k.contains("diamond_") || k.contains("netherite_"));
        return Some(if armor { "That's the plan! I think my gear is almost ready.".into() } else { "That's the plan! But honestly, I still need better armor.".into() });
    }
    if q.contains("sword") && q.contains("need") {
        let has = sit.inventory.iter().any(|(k, _)| k.ends_with("_sword"));
        return Some(if has { "I've got a sword already.".into() } else { "Yeah, I still need a sword.".into() });
    }
    None
}
