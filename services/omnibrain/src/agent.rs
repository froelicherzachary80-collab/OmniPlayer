//! The OmniBrain agent: reflexes, goal planning, chat handling and commentary,
//! layered over deterministic OmniCore controllers.

use crate::crafting::{can_craft, Craft};
use crate::ctx::Ctx;
use crate::gameplay::*;
use crate::intent::{self, Intent};
use crate::io::{now_ms, Output};
use crate::memory::Memory;
use crate::persona::{answer, Situation, Voice};
use crate::tasks::*;
use omnicore::data::{is_hostile, GAME};
use omnicore::physics::{angle_diff, Controls};
use omnicore::{Client, Event};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "target")]
pub enum Mode {
    Autonomous,
    Follow(String),
    Cooperative(String),
    Team,
    PvpTraining(String),
    Builder,
    Explorer,
    Farmer,
    Guard([i32; 3]),
    Streamer,
    Idle,
}

impl Mode {
    pub fn parse(s: &str, target: Option<&str>, here: [i32; 3]) -> Option<Mode> {
        let t = target.unwrap_or("").to_string();
        Some(match s {
            "autonomous" | "fully_autonomous" => Mode::Autonomous,
            "follow" => Mode::Follow(t),
            "cooperative" => Mode::Cooperative(t),
            "team" => Mode::Team,
            "pvp_training" | "pvp" => Mode::PvpTraining(t),
            "builder" => Mode::Builder,
            "explorer" => Mode::Explorer,
            "farmer" => Mode::Farmer,
            "guard" => Mode::Guard(here),
            "streamer" => Mode::Streamer,
            "idle" => Mode::Idle,
            _ => return None,
        })
    }
    pub fn label(&self) -> String {
        match self {
            Mode::Follow(p) => format!("follow {p}"),
            Mode::Cooperative(p) => format!("cooperative with {p}"),
            Mode::PvpTraining(p) => format!("pvp training with {p}"),
            Mode::Guard(p) => format!("guard {p:?}"),
            m => format!("{m:?}").to_lowercase(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct BrainConfig {
    pub owner: String,
    pub teammates: Vec<String>,
    pub aliases: Vec<String>,
    pub role: String,
    /// Speak gameplay commentary in Minecraft chat too (always sent to the stream).
    pub chat_commentary: bool,
    pub think_interval_ticks: u64,
    pub allow_pvp: bool,
}

pub struct Agent {
    pub client: Client,
    pub out: Output,
    pub cfg: BrainConfig,
    pub voice: Voice,
    pub memory: Memory,
    pub mode: Mode,
    task: Option<(String, Box<dyn Task>, bool)>, // (goal, task, from_command)
    pub tick: u64,
    last_spoken: u64,
    last_reply: HashMap<String, u64>,
    recent_chat: VecDeque<String>,
    seen_players_near: HashSet<String>,
    goal_cooldown: HashMap<String, u64>,
    was_night: bool,
    sheltered_night: bool,
    death_pos: Option<[i32; 3]>,
    respawn_at: Option<u64>,
    idle_look: (f32, f32),
    next_idle_look: u64,
    last_biome: String,
    hurt_by: Option<(i32, u64)>,
    pending_viewer: VecDeque<(String, String)>,
    seen_ores: HashSet<(i32, i32, i32)>,
    stopped_until: u64,
    pub status_extra: Value,
}

impl Agent {
    pub fn new(client: Client, out: Output, cfg: BrainConfig, voice: Voice, memory: Memory, mode: Mode) -> Self {
        Self {
            client,
            out,
            cfg,
            voice,
            memory,
            mode,
            task: None,
            tick: 0,
            last_spoken: 0,
            last_reply: HashMap::new(),
            recent_chat: VecDeque::new(),
            seen_players_near: HashSet::new(),
            goal_cooldown: HashMap::new(),
            was_night: false,
            sheltered_night: false,
            death_pos: None,
            respawn_at: None,
            idle_look: (0.0, 0.0),
            next_idle_look: 0,
            last_biome: String::new(),
            hurt_by: None,
            pending_viewer: VecDeque::new(),
            seen_ores: HashSet::new(),
            stopped_until: 0,
            status_extra: json!({}),
        }
    }

    fn my_name(&self) -> String {
        self.client.state.lock().unwrap().username.clone()
    }

    pub fn current_goal(&self) -> String {
        self.task.as_ref().map(|t| t.0.clone()).unwrap_or_default()
    }

    pub fn situation(&self, event: &str, detail: &str) -> Situation {
        let st = self.client.state.lock().unwrap();
        let p = st.body.pos;
        let biome = st.world.biome_at(p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32).unwrap_or("unknown").to_string();
        let nearby_players = st
            .entities
            .values()
            .filter(|e| e.is_player() && e.distance_to(p) < 24.0)
            .filter_map(|e| e.username.clone())
            .collect();
        Situation {
            event: event.into(),
            detail: detail.into(),
            pos: [p[0].floor() as i64, p[1].floor() as i64, p[2].floor() as i64],
            biome,
            health: st.health,
            food: st.food,
            goal: self.current_goal(),
            nearby_players,
            inventory: st.inventory.summary(),
            recent_chat: self.recent_chat.iter().cloned().collect(),
            speaker: None,
        }
    }

    /// Gameplay-grounded commentary for the stream (and optionally chat).
    pub fn comment(&mut self, event: &str, detail: &str, important: bool) {
        let gap = if important { 40 } else { (20.0 * (8.0 + 20.0 * (1.0 - self.voice.persona.chattiness.clamp(0.0, 1.0)))) as u64 };
        if self.tick < self.last_spoken + gap {
            return;
        }
        let sit = self.situation(event, detail);
        if self.voice.llm_enabled() {
            self.voice.request(&format!("commentary:{event}"), sit.clone());
            self.last_spoken = self.tick;
            return;
        }
        if let Some(line) = self.voice.commentary(&sit) {
            self.last_spoken = self.tick;
            self.speak(&line, event, &sit, important && self.cfg.chat_commentary);
        }
    }

    fn speak(&mut self, line: &str, event: &str, sit: &Situation, in_chat: bool) {
        self.out.emit("commentary", json!({ "text": line, "event": event, "situation": sit }));
        if in_chat {
            self.client.chat(line);
        }
    }

    fn reply(&mut self, to: &str, text: &str) {
        self.last_reply.insert(to.to_string(), self.tick);
        self.client.chat(text);
        self.out.emit("said", json!({ "text": text, "to": to }));
        let sit = self.situation("reply", to);
        self.out.emit("commentary", json!({ "text": text, "event": "reply", "situation": sit }));
    }

    fn set_task(&mut self, goal: &str, task: Box<dyn Task>, from_command: bool) {
        self.abort_task();
        self.out.emit("goal", json!({ "goal": goal, "task": task.name(), "from_command": from_command }));
        self.task = Some((goal.to_string(), task, from_command));
    }

    fn abort_task(&mut self) {
        if let Some((_, mut t, _)) = self.task.take() {
            let mut ctx = Ctx { client: &self.client, controls: Controls::default(), look: None, tick: self.tick, out: &self.out };
            t.abort(&mut ctx);
        }
    }

    // ───────────────────────────── events ─────────────────────────────

    pub fn on_event(&mut self, e: &Event) {
        let now = now_ms();
        match e {
            Event::Chat { sender, message, raw, .. } => {
                self.recent_chat.push_back(raw.clone());
                while self.recent_chat.len() > 12 {
                    self.recent_chat.pop_front();
                }
                self.out.emit("chat", json!({ "sender": sender, "message": message, "raw": raw }));
                if let Some(s) = sender.clone() {
                    if s != self.my_name() {
                        self.on_player_chat(&s, message);
                    }
                }
            }
            Event::PlayerJoined { name, .. } => {
                if *name != self.my_name() {
                    let new = self.memory.meet(name, now);
                    self.out.emit("memory", json!({ "kind": "player", "name": name, "new": new }));
                    if self.tick > 100 {
                        let detail = name.clone();
                        self.comment("player_joined", &detail, true);
                        if !self.cfg.teammates.contains(name) && self.ok_to_reply(name) {
                            let greet = if new { format!("Welcome {name}!") } else { format!("Hey {name}, welcome back!") };
                            self.reply(name, &greet);
                        }
                    }
                }
            }
            Event::PlayerLeft { name, .. } => {
                self.out.emit("memory", json!({ "kind": "player_left", "name": name }));
            }
            Event::Hurt { source_entity, .. } => {
                let (health, src_name) = {
                    let st = self.client.state.lock().unwrap();
                    let name = source_entity
                        .and_then(|id| st.entities.get(&id))
                        .map(|e| e.username.clone().unwrap_or_else(|| e.kind_name.replace('_', " ")))
                        .unwrap_or_default();
                    (st.health, name)
                };
                if let Some(id) = source_entity {
                    self.hurt_by = Some((*id, self.tick));
                }
                self.comment("hurt", &src_name, health < 8.0);
            }
            Event::Died { message } => {
                let p = self.client.state.lock().unwrap().body.pos;
                self.death_pos = Some([p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]);
                self.memory.bump("deaths", 1);
                // Learn: dying in a fight makes the bot flee earlier next time.
                if self.hurt_by.map(|h| self.tick - h.1 < 200).unwrap_or(false) {
                    let v = self.memory.learn("flee_health", 8.0, 2.0, 4.0, 16.0);
                    self.out.emit("memory", json!({ "kind": "strategy", "key": "flee_health", "value": v }));
                }
                self.memory.event(format!("died: {message}"));
                self.out.emit("memory", json!({ "kind": "event", "text": format!("died: {message}"), "pos": self.death_pos }));
                self.abort_task();
                self.respawn_at = Some(self.tick + 30);
                self.comment("died", message, true);
            }
            Event::Respawned { .. } => {}
            Event::BlockChanged { .. } | Event::ChunkLoaded { .. } | Event::InventoryChanged => {}
            Event::Disconnected { reason } => self.out.emit("disconnected", json!({ "reason": reason })),
            other => self.out.emit("event", serde_json::to_value(other).unwrap_or(Value::Null)),
        }
    }

    fn ok_to_reply(&self, who: &str) -> bool {
        let gap = if self.cfg.teammates.iter().any(|t| t == who) { 20 * 12 } else { 20 * 2 };
        self.last_reply.get(who).map(|t| self.tick > t + gap).unwrap_or(true)
    }

    fn on_player_chat(&mut self, sender: &str, msg: &str) {
        let me = self.my_name();
        let addressed = intent::addressed_to(msg, &me, &self.cfg.aliases);
        let is_bot = self.cfg.teammates.iter().any(|t| t == sender);
        let near = {
            let st = self.client.state.lock().unwrap();
            let p = st.body.pos;
            st.player_entity(sender).map(|e| e.distance_to(p) < 24.0).unwrap_or(false)
        };
        self.memory.interact(sender, 1, now_ms());
        let it = intent::parse(msg);
        // Bots only respond to other bots when addressed by name (prevents chat loops).
        let should = if is_bot { addressed } else { addressed || (near && it != Intent::None) || sender == self.cfg.owner && it != Intent::None };
        if !should || !self.ok_to_reply(sender) {
            return;
        }
        self.out.emit("intent", json!({ "from": sender, "intent": format!("{it:?}"), "message": msg }));
        let sender_pos = {
            let st = self.client.state.lock().unwrap();
            st.player_entity(sender).map(|e| e.pos)
        };
        let sp = sender_pos.map(|p| (p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32));
        match it {
            Intent::Follow => {
                self.mode = Mode::Follow(sender.into());
                self.abort_task();
                self.reply(sender, &format!("Sure {sender}, right behind you!"));
            }
            Intent::Come => match sp {
                Some(p) => {
                    self.reply(sender, "On my way!");
                    self.set_task(&format!("go to {sender}"), Box::new(Goto::new(p, 2.0).sprinting(true)), true);
                }
                None => self.reply(sender, "I can't see you, where are you?"),
            },
            Intent::Stop => {
                self.abort_task();
                if matches!(self.mode, Mode::Follow(_) | Mode::PvpTraining(_)) {
                    self.mode = Mode::Cooperative(sender.into());
                }
                self.stopped_until = self.tick + 20 * 60;
                self.reply(sender, "Okay, stopping.");
            }
            Intent::BuildHouse => {
                let role_builder = self.cfg.role.is_empty() || self.cfg.role == "builder";
                if !role_builder {
                    self.reply(sender, "I'll gather more wood for the build!");
                    self.set_task("gather wood for the house", Box::new(Gather::new("collect logs", is_log, is_log, 16)), true);
                } else {
                    self.reply(sender, "Let's do it! I'll get materials and build it next to you.");
                    let origin = sp.map(|p| (p.0 + 4, p.1, p.2)).unwrap_or_else(|| self.feet());
                    let task = self.house_plan(origin);
                    self.set_task("build a house", task, true);
                }
            }
            Intent::MineDiamonds => {
                self.reply(sender, "Diamonds? I'm in. Let's dig!");
                let mut s = Sequence::new(
                    "mine diamonds",
                    vec![
                        Box::new(Gather::new("find diamonds", is_diamond_ore, is_diamond, 1).digging()),
                    ],
                );
                s.tolerate_failures = false;
                self.set_task("mine diamonds", Box::new(s), true);
            }
            Intent::ProtectBase => {
                let here = sp.map(|p| [p.0, p.1, p.2]).or_else(|| self.memory.locations.get("home").copied()).unwrap_or_else(|| {
                    let f = self.feet();
                    [f.0, f.1, f.2]
                });
                self.mode = Mode::Guard(here);
                self.abort_task();
                self.reply(sender, "I'll guard the base. Nothing's getting past me.");
            }
            Intent::GoNether => {
                let portal = {
                    let st = self.client.state.lock().unwrap();
                    let f = self.feet();
                    st.world.find_blocks(f, 32, 1, |s| GAME.block_name(s) == "nether_portal").first().copied()
                };
                match portal {
                    Some(p) => {
                        self.reply(sender, "To the Nether! Follow me to the portal.");
                        self.set_task("enter the nether portal", Box::new(Goto::new(p, 0.3)), true);
                    }
                    None => self.reply(sender, "I'm down, but there's no portal nearby. We need 10 obsidian and flint and steel first."),
                }
            }
            Intent::FightMe => {
                if !self.cfg.allow_pvp {
                    self.reply(sender, "PvP is turned off for me on this server.");
                } else {
                    self.mode = Mode::PvpTraining(sender.into());
                    self.abort_task();
                    self.reply(sender, "Oh you're ON. Say stop when you've had enough!");
                }
            }
            Intent::GiveFood => {
                let has_food = self.client.state.lock().unwrap().inventory.find(|s| Eat::is_food_name(s.name())).is_some();
                match (has_food, sp) {
                    (true, Some(p)) => {
                        self.reply(sender, "Here, take some food!");
                        self.set_task("give food", Box::new(GiveItems::new(p, sender, Eat::is_food_name, 3)), true);
                    }
                    (false, _) => self.reply(sender, "I don't have any food right now, sorry! I'll hunt some."),
                    (true, None) => self.reply(sender, "Come closer and I'll toss you some food."),
                }
            }
            Intent::StartSmp => {
                self.reply(sender, "An SMP? YES. Let's set up bases and make it a whole thing!");
                self.out.emit("smp_request", json!({ "from": sender }));
                self.mode = Mode::Cooperative(sender.into());
            }
            Intent::CollectWood => {
                self.reply(sender, "Getting wood!");
                self.set_task("collect wood", Box::new(Gather::new("collect logs", is_log, is_log, 16)), true);
            }
            Intent::Explore => {
                self.mode = Mode::Explorer;
                self.abort_task();
                self.reply(sender, "Let's see what's out there!");
            }
            Intent::Craft(item) => {
                let inv = self.client.state.lock().unwrap().inventory.clone();
                if can_craft(&item, &inv) {
                    self.reply(sender, &format!("Crafting {}!", item.replace('_', " ")));
                    self.set_task(&format!("craft {item}"), Box::new(Craft::new(&item)), true);
                } else {
                    self.reply(sender, &format!("I can't craft {} right now, missing materials.", item.replace('_', " ")));
                }
            }
            Intent::SetHome => {
                if let Some(p) = sp {
                    self.memory.locations.insert("home".into(), [p.0, p.1, p.2]);
                    self.out.emit("memory", json!({ "kind": "location", "label": "home", "pos": [p.0, p.1, p.2] }));
                    self.reply(sender, "Got it, this is home base.");
                }
            }
            Intent::Greeting => {
                let mem = self.memory.players.get(sender).map(|p| p.interactions).unwrap_or(0);
                let line = if mem > 5 { format!("Hey {sender}! Good to see you again.") } else { format!("Hi {sender}!") };
                self.reply(sender, &line);
            }
            Intent::Question(q) => {
                let mut sit = self.situation("question", &q);
                sit.speaker = Some(sender.to_string());
                if let Some(a) = answer(&q, &sit) {
                    self.reply(sender, &a);
                } else if self.voice.llm_enabled() {
                    self.voice.request(&format!("reply:{sender}"), sit);
                    self.last_reply.insert(sender.to_string(), self.tick);
                } else if addressed {
                    let g = self.current_goal();
                    let a = if g.is_empty() { "Good question! I'm not sure yet.".to_string() } else { format!("Not sure! I'm busy with {g} right now.") };
                    self.reply(sender, &a);
                }
            }
            Intent::None => {
                if addressed && !is_bot {
                    self.reply(sender, "Yeah?");
                }
            }
        }
    }

    /// Viewer chat from MineLive: answer selected messages on stream.
    pub fn on_viewer_chat(&mut self, user: &str, text: &str) {
        let key = format!("viewer:{user}");
        let first = self.memory.meet(&key, now_ms());
        self.memory.interact(&key, 1, now_ms());
        let it = intent::parse(text);
        let selected = matches!(it, Intent::Question(_) | Intent::Greeting) || intent::addressed_to(text, &self.my_name(), &self.cfg.aliases);
        if !selected || self.pending_viewer.len() > 5 {
            return;
        }
        self.pending_viewer.push_back((user.to_string(), text.to_string()));
        let _ = first;
    }

    fn answer_viewers(&mut self) {
        if self.tick % 60 != 0 {
            return;
        }
        let Some((user, text)) = self.pending_viewer.pop_front() else { return };
        let mut sit = self.situation("viewer_question", &text);
        sit.speaker = Some(user.clone());
        let returning = self.memory.players.get(&format!("viewer:{user}")).map(|p| p.interactions > 3).unwrap_or(false);
        let line = match intent::parse(&text) {
            Intent::Greeting => Some(if returning { format!("{user}! Welcome back to the stream!") } else { format!("Hey {user}, welcome in!") }),
            Intent::Question(q) => answer(&q, &sit),
            _ => None,
        };
        match line {
            Some(l) => {
                self.out.emit("viewer_reply", json!({ "user": user, "question": text, "text": l }));
                self.out.emit("commentary", json!({ "text": l, "event": "viewer_reply", "situation": sit }));
            }
            None if self.voice.llm_enabled() => self.voice.request(&format!("viewer:{user}"), sit),
            None => {
                let l = format!("{user}, good question! Honestly not sure yet.");
                self.out.emit("viewer_reply", json!({ "user": user, "question": text, "text": l }));
            }
        }
    }

    fn drain_llm(&mut self) {
        while let Ok((tag, line)) = self.voice.rx.try_recv() {
            let sit = self.situation("llm", &tag);
            if let Some(who) = tag.strip_prefix("reply:") {
                let who = who.to_string();
                self.reply(&who, &line);
            } else if let Some(user) = tag.strip_prefix("viewer:") {
                self.out.emit("viewer_reply", json!({ "user": user, "text": line }));
                self.out.emit("commentary", json!({ "text": line, "event": "viewer_reply", "situation": sit }));
            } else {
                let ev = tag.trim_start_matches("commentary:").to_string();
                self.speak(&line, &ev, &sit, false);
            }
        }
    }

    fn feet(&self) -> (i32, i32, i32) {
        let p = self.client.state.lock().unwrap().body.pos;
        (p[0].floor() as i32, (p[1] + 0.01).floor() as i32, p[2].floor() as i32)
    }

    fn house_plan(&self, origin: (i32, i32, i32)) -> Box<dyn Task> {
        let need = BuildHouse::blocks_needed();
        let mut steps: Vec<Box<dyn Task>> = vec![];
        let have = self.client.state.lock().unwrap().inventory.count_where(is_build_item);
        if have < need {
            let logs = ((need - have) as f64 / 4.0).ceil() as i32;
            steps.push(Box::new(Gather::new("collect logs for the house", is_log, is_log, logs)));
            for _ in 0..logs {
                steps.push(Box::new(Craft::new("planks")));
            }
        }
        steps.push(Box::new(BuildHouse::new(origin)));
        let mut s = Sequence::new("build a house", steps);
        s.tolerate_failures = true;
        Box::new(s)
    }

    // ───────────────────────────── planning ─────────────────────────────

    fn cool(&self, key: &str) -> bool {
        self.goal_cooldown.get(key).map(|t| self.tick < *t).unwrap_or(false)
    }

    fn propose(&self, key: &str) -> bool {
        !self.cool(key)
    }

    /// Survival progression: picks the next useful goal from the real inventory/world.
    fn plan_survival(&mut self) -> Option<(String, Box<dyn Task>)> {
        let (inv, food, health, day, feet) = {
            let st = self.client.state.lock().unwrap();
            (st.inventory.clone(), st.food, st.health, st.day_time, st.body.pos)
        };
        let _ = (health, feet);
        let has = |pred: &dyn Fn(&str) -> bool| inv.count_where(|n| pred(n)) > 0;
        let planks = inv.count_where(|n| n.ends_with("_planks"));
        let logs = inv.count_where(is_log);
        let sticks = inv.count("stick");
        let cobble = inv.count_where(is_stone_item);
        let table_near = {
            let st = self.client.state.lock().unwrap();
            let s = GAME.block_by_name("crafting_table").map(|b| b.default_state).unwrap_or(0);
            !st.world.find_blocks(self.feet(), 20, 1, |x| x == s).is_empty()
        };
        let table = table_near || inv.count("crafting_table") > 0;
        let has_pick = has(&|n: &str| n.ends_with("_pickaxe"));
        let has_stone_pick = has(&|n: &str| matches!(n, "stone_pickaxe" | "iron_pickaxe" | "diamond_pickaxe" | "netherite_pickaxe"));
        let has_sword = has(&|n: &str| n.ends_with("_sword"));
        let has_food = inv.find(|s| Eat::is_food_name(s.name())).is_some();

        if food <= 14 && has_food && self.propose("eat") {
            return Some(("eat".into(), Box::new(Eat::new())));
        }
        if food <= 8 && !has_food && self.propose("hunt") {
            return Some(("hunt for food".into(), Box::new(Hunt::new(2))));
        }
        let night = (12800..23200).contains(&day);
        if night && !self.sheltered_night && inv.count_where(is_build_item) >= 9 && self.propose("shelter") {
            self.sheltered_night = true;
            let item = inv.find(|s| is_build_item(s.name())).map(|i| inv.slots[i].name().to_string()).unwrap_or_default();
            return Some(("build an emergency shelter for the night".into(), Box::new(panic_box(self.feet(), &item))));
        }

        // Wood → table → sticks → wooden pickaxe
        let crafts_needed = |p: i32| p - planks;
        let wood_goal = if !has_pick { 5 + if table { 0 } else { 4 } } else { 2 };
        if !has_pick || (!has_stone_pick && sticks < 2) {
            if planks < wood_goal && logs > 0 && self.propose("planks") {
                return Some(("craft planks".into(), Box::new(Craft::new("planks"))));
            }
            if planks < wood_goal && logs == 0 && self.propose("logs") {
                let n = ((crafts_needed(wood_goal) as f64) / 4.0).ceil().max(2.0) as i32;
                return Some(("collect wood".into(), Box::new(Gather::new("collect logs", is_log, is_log, n))));
            }
        }
        if !has_pick {
            if !table && planks >= 4 && self.propose("table") {
                return Some(("craft a crafting table".into(), Box::new(Craft::new("crafting_table"))));
            }
            if sticks < 2 && planks >= 2 && self.propose("sticks") {
                return Some(("craft sticks".into(), Box::new(Craft::new("stick"))));
            }
            if can_craft("wooden_pickaxe", &inv) && self.propose("wooden_pickaxe") {
                return Some(("craft a wooden pickaxe".into(), Box::new(Craft::new("wooden_pickaxe"))));
            }
        }
        if has_pick && !has_stone_pick {
            if cobble < 3 && self.propose("stone") {
                return Some(("mine stone".into(), Box::new(Gather::new("mine cobblestone", is_stone_block, is_stone_item, 5).digging())));
            }
            if sticks < 2 && planks >= 2 && self.propose("sticks") {
                return Some(("craft sticks".into(), Box::new(Craft::new("stick"))));
            }
            if !table && planks >= 4 && self.propose("table") {
                return Some(("craft a crafting table".into(), Box::new(Craft::new("crafting_table"))));
            }
            if can_craft("stone_pickaxe", &inv) && self.propose("stone_pickaxe") {
                return Some(("craft a stone pickaxe".into(), Box::new(Craft::new("stone_pickaxe"))));
            }
        }
        if has_stone_pick && !has_sword {
            if cobble >= 2 && sticks >= 1 && can_craft("stone_sword", &inv) && self.propose("stone_sword") {
                return Some(("craft a stone sword".into(), Box::new(Craft::new("stone_sword"))));
            }
            if cobble < 2 && self.propose("stone") {
                return Some(("mine stone".into(), Box::new(Gather::new("mine cobblestone", is_stone_block, is_stone_item, 4).digging())));
            }
        }
        if has_stone_pick && inv.count("coal") < 4 && self.propose("coal") {
            return Some(("mine coal".into(), Box::new(Gather::new("mine coal", is_coal_ore, is_coal, 4).digging())));
        }
        if has_stone_pick && inv.count("furnace") == 0 && cobble >= 8 && self.propose("furnace") {
            return Some(("craft a furnace".into(), Box::new(Craft::new("furnace"))));
        }
        if has_stone_pick && inv.count("torch") < 8 && inv.count("coal") >= 1 && sticks >= 1 && self.propose("torch") {
            return Some(("craft torches".into(), Box::new(Craft::new("torch"))));
        }
        if has_stone_pick && inv.count_where(is_raw_iron) < 8 && self.propose("iron") {
            return Some(("mine iron".into(), Box::new(Gather::new("mine iron", is_iron_ore, is_raw_iron, 8).digging())));
        }
        if has_stone_pick && self.propose("diamonds") {
            return Some(("look for diamonds".into(), Box::new(Gather::new("find diamonds", is_diamond_ore, is_diamond, 3).digging())));
        }
        if !has_food && self.propose("hunt") {
            return Some(("hunt for food".into(), Box::new(Hunt::new(2))));
        }
        None
    }

    fn plan(&mut self) -> Option<(String, Box<dyn Task>)> {
        if self.tick < self.stopped_until {
            return None;
        }
        // Recover items after dying.
        if let Some(dp) = self.death_pos.take() {
            let here = self.feet();
            let d = ((dp[0] - here.0).pow(2) + (dp[2] - here.2).pow(2)) as f64;
            if d.sqrt() < 300.0 {
                let s = Sequence::new("recover dropped items", vec![Box::new(Goto::new((dp[0], dp[1], dp[2]), 2.0).sprinting(true)), Box::new(Collect::new(10.0))]);
                return Some(("recover my items".into(), Box::new(s)));
            }
        }
        match self.mode.clone() {
            Mode::Idle => None,
            Mode::Follow(p) => Some((format!("follow {p}"), Box::new(Follow::new(&p, 3.0)))),
            Mode::PvpTraining(p) => {
                let id = self.client.state.lock().unwrap().player_entity(&p).map(|e| e.id);
                id.map(|id| (format!("pvp training with {p}"), Box::new(Attack::new(id)) as Box<dyn Task>))
            }
            Mode::Guard(pos) => {
                let here = self.feet();
                if (here.0 - pos[0]).abs() + (here.2 - pos[2]).abs() > 10 {
                    Some(("return to guard post".into(), Box::new(Goto::new((pos[0], pos[1], pos[2]), 3.0))))
                } else {
                    Some(("guard the base".into(), Box::new(Wait::new(60, true))))
                }
            }
            Mode::Explorer => self.explore(),
            Mode::Builder => {
                if self.memory.locations.contains_key("house") {
                    self.plan_survival().or_else(|| self.explore())
                } else {
                    let f = self.feet();
                    Some(("build a house".into(), self.house_plan((f.0 + 3, f.1, f.2))))
                }
            }
            Mode::Farmer => {
                if self.propose("harvest") {
                    Some((
                        "harvest crops".into(),
                        Box::new(Gather::new("harvest wheat", |n| n == "wheat", |n| n == "wheat", 64)) as Box<dyn Task>,
                    ))
                } else {
                    self.plan_survival()
                }
            }
            Mode::Cooperative(p) => {
                // Stay within ~24 blocks of the partner while progressing.
                let (dist, ppos) = {
                    let st = self.client.state.lock().unwrap();
                    let me = st.body.pos;
                    st.player_entity(&p).map(|e| (e.distance_to(me), e.pos)).unwrap_or((0.0, me))
                };
                if dist > 24.0 {
                    let g = (ppos[0].floor() as i32, ppos[1].floor() as i32, ppos[2].floor() as i32);
                    return Some((format!("regroup with {p}"), Box::new(Goto::new(g, 4.0).sprinting(true))));
                }
                self.plan_survival().or_else(|| Some((format!("hang out with {p}"), Box::new(Follow::new(&p, 4.0)))))
            }
            Mode::Autonomous | Mode::Team | Mode::Streamer => self.plan_survival().or_else(|| self.explore()),
        }
    }

    fn explore(&mut self) -> Option<(String, Box<dyn Task>)> {
        let ctx = Ctx { client: &self.client, controls: Controls::default(), look: None, tick: self.tick, out: &self.out };
        let t = explore_target(&ctx, 28.0)?;
        Some(("explore".into(), Box::new(Goto::new(t, 2.0))))
    }

    // ───────────────────────────── reflexes ─────────────────────────────

    fn reflexes(&mut self) -> bool {
        let (dead, health, me, attacker, threat) = {
            let st = self.client.state.lock().unwrap();
            let me = st.body.pos;
            let threat = st
                .entities
                .values()
                .filter(|e| is_hostile(&e.kind_name) && e.distance_to(me) < 6.0)
                .min_by(|a, b| a.distance_to(me).partial_cmp(&b.distance_to(me)).unwrap())
                .map(|e| (e.id, e.kind_name.clone(), e.pos));
            let attacker = self.hurt_by.filter(|h| self.tick - h.1 < 100).and_then(|h| st.entities.get(&h.0).map(|e| (e.id, e.kind_name.clone(), e.pos, e.username.clone())));
            (st.dead, st.health, me, attacker, threat)
        };
        if dead {
            if let Some(at) = self.respawn_at {
                if self.tick >= at {
                    self.client.respawn();
                    self.respawn_at = Some(self.tick + 60);
                }
            } else {
                self.respawn_at = Some(self.tick + 30);
            }
            return true;
        }
        self.respawn_at = None;
        let busy_fighting = self.task.as_ref().map(|t| t.0.starts_with("fight") || t.0.starts_with("flee") || t.0.starts_with("pvp")).unwrap_or(false);
        if busy_fighting {
            return false;
        }
        // Being attacked by a player in PvP training is expected; otherwise react.
        let enemy = attacker.map(|a| (a.0, a.3.unwrap_or(a.1), a.2)).or(threat);
        if let Some((id, name, pos)) = enemy {
            if matches!(&self.mode, Mode::PvpTraining(p) if *p == name) {
                return false;
            }
            let is_player = self.client.state.lock().unwrap().players.values().any(|p| p.name == name);
            if is_player && !self.cfg.allow_pvp {
                return false;
            }
            let flee_at = self.memory.strat("flee_health", 8.0) as f32;
            if health <= flee_at || name == "creeper" && health < 14.0 || name == "warden" {
                let away = [me[0] + (me[0] - pos[0]) * 3.0, me[1], me[2] + (me[2] - pos[2]) * 3.0];
                let g = (away[0].floor() as i32, away[1].floor() as i32, away[2].floor() as i32);
                self.comment("low_health", "", true);
                self.set_task(&format!("flee from {name}"), Box::new(Goto::new(g, 4.0).sprinting(true)), false);
            } else {
                self.set_task(&format!("fight {name}"), Box::new(Attack::new(id)), false);
            }
            return false;
        }
        false
    }

    // ───────────────────────────── observation ─────────────────────────────

    fn observe(&mut self) {
        if self.tick % 20 != 0 {
            return;
        }
        let (day, me, biome, players, ores) = {
            let st = self.client.state.lock().unwrap();
            let me = st.body.pos;
            let fb = (me[0].floor() as i32, me[1].floor() as i32, me[2].floor() as i32);
            let biome = st.world.biome_at(fb.0, fb.1, fb.2).unwrap_or("").to_string();
            let players: Vec<String> = st.entities.values().filter(|e| e.is_player() && e.distance_to(me) < 10.0).filter_map(|e| e.username.clone()).collect();
            let ores = if self.tick % 100 == 0 {
                st.world.find_blocks(fb, 6, 5, |s| {
                    let n = GAME.block_name(s);
                    n.contains("diamond_ore") || n.contains("emerald_ore") || n.contains("gold_ore") || n.contains("iron_ore")
                })
            } else {
                vec![]
            };
            (st.day_time, me, biome, players, ores)
        };
        let _ = me;
        let night = (12800..23200).contains(&day);
        if night && !self.was_night {
            self.comment("night", "", false);
        }
        if !night && self.was_night {
            self.sheltered_night = false;
            self.comment("morning", "", false);
        }
        self.was_night = night;
        if !biome.is_empty() && biome != self.last_biome {
            if !self.last_biome.is_empty() {
                let b = biome.replace('_', " ");
                self.comment("biome", &b, false);
            }
            self.last_biome = biome;
        }
        let now: HashSet<String> = players.into_iter().filter(|p| *p != self.my_name()).collect();
        for p in now.difference(&self.seen_players_near.clone()) {
            self.memory.meet(p, now_ms());
            self.comment("player_near", p, false);
        }
        self.seen_players_near = now;
        for o in ores {
            if self.seen_ores.insert(o) {
                let name = self.client.state.lock().unwrap().world.block_name(o.0, o.1, o.2).replace("deepslate_", "").replace("_ore", "");
                let label = if name == "diamond" { "diamonds".to_string() } else { name.clone() };
                self.comment("found_ore", &label, name == "diamond");
                self.out.emit("memory", json!({ "kind": "location", "label": format!("{name} ore"), "pos": [o.0, o.1, o.2] }));
                if name == "diamond" {
                    self.client.chat(&format!("Found diamonds at {} {} {}!", o.0, o.1, o.2));
                }
            }
        }
    }

    // ───────────────────────────── tick ─────────────────────────────

    pub fn tick(&mut self) {
        self.tick += 1;
        self.drain_llm();
        self.answer_viewers();
        let reflex_busy = self.reflexes();
        self.observe();

        let mut ctx = Ctx { client: &self.client, controls: Controls::default(), look: None, tick: self.tick, out: &self.out };
        if !reflex_busy {
            if self.task.is_none() {
                let think = self.cfg.think_interval_ticks.max(10);
                if self.tick % think == 0 {
                    drop(ctx);
                    if let Some((goal, task)) = self.plan() {
                        if !goal.starts_with("follow") && !goal.starts_with("guard") && !goal.starts_with("hang out") {
                            let g = goal.clone();
                            self.comment("goal", &g, false);
                        }
                        self.set_task(&goal, task, false);
                    } else if rand::random::<f64>() < 0.3 {
                        // Occasionally stop to think / look around like a person.
                        self.set_task("look around", Box::new(Wait::new(40 + (rand::random::<u64>() % 60), true)), false);
                    }
                    ctx = Ctx { client: &self.client, controls: Controls::default(), look: None, tick: self.tick, out: &self.out };
                }
            }
            if let Some((goal, task, from_cmd)) = self.task.as_mut() {
                let status = task.step(&mut ctx);
                match status {
                    Status::Running => {}
                    Status::Done => {
                        let g = goal.clone();
                        let fc = *from_cmd;
                        self.out.emit("goal_done", json!({ "goal": g, "from_command": fc }));
                        self.memory.bump("goals_completed", 1);
                        if g.contains("craft a") {
                            let item = g.trim_start_matches("craft a ").trim_start_matches("n ").to_string();
                            let controls = ctx.controls;
                            let look = ctx.look;
                            drop(ctx);
                            self.comment("crafted", &item, true);
                            ctx = Ctx { client: &self.client, controls, look, tick: self.tick, out: &self.out };
                        }
                        if g == "build a house" {
                            let controls = ctx.controls;
                            drop(ctx);
                            self.comment("built", "the house", true);
                            ctx = Ctx { client: &self.client, controls, look: None, tick: self.tick, out: &self.out };
                        }
                        self.task = None;
                    }
                    Status::Failed(why) => {
                        let g = goal.clone();
                        self.out.emit("goal_failed", json!({ "goal": g, "reason": why }));
                        // Cool down this kind of goal to avoid repeating the same mistake.
                        let key = cooldown_key(&g);
                        self.goal_cooldown.insert(key, self.tick + 20 * 45);
                        self.task = None;
                    }
                }
            }
        }
        let (controls, look) = (ctx.controls, ctx.look);
        drop(ctx);
        self.apply_look(look);
        self.client.tick(&controls);
        if self.tick % 20 == 0 {
            self.emit_status();
        }
    }

    /// Smooth, human-like head rotation toward the desired view.
    fn apply_look(&mut self, look: Option<(f32, f32)>) {
        let (cy, cp) = {
            let st = self.client.state.lock().unwrap();
            (st.body.yaw, st.body.pitch)
        };
        let target = match look {
            Some(l) => l,
            None => {
                if self.tick >= self.next_idle_look {
                    // Idle: glance at a nearby player/mob, or look around.
                    let st = self.client.state.lock().unwrap();
                    let me = st.body.eye();
                    let near = st.entities.values().filter(|e| (e.is_player() || is_hostile(&e.kind_name)) && e.distance_to(me) < 10.0).min_by(|a, b| a.distance_to(me).partial_cmp(&b.distance_to(me)).unwrap()).map(|e| [e.pos[0], e.pos[1] + e.height() * 0.85, e.pos[2]]);
                    drop(st);
                    self.idle_look = match near {
                        Some(p) => omnicore::physics::look_at(me, p),
                        None => (cy + (rand::random::<f32>() - 0.5) * 90.0, (rand::random::<f32>() - 0.5) * 40.0),
                    };
                    self.next_idle_look = self.tick + 30 + rand::random::<u64>() % 70;
                }
                self.idle_look
            }
        };
        let dy = angle_diff(cy, target.0);
        let dp = target.1 - cp;
        let max = 32.0;
        let step_y = (dy * 0.45).clamp(-max, max);
        let step_p = (dp * 0.45).clamp(-max, max);
        let ny = if dy.abs() < 0.5 { target.0 } else { cy + step_y };
        let np = if dp.abs() < 0.5 { target.1 } else { cp + step_p };
        self.client.set_rotation(ny, np);
    }

    pub fn emit_status(&self) {
        let st = self.client.state.lock().unwrap();
        let p = st.body.pos;
        let fb = (p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32);
        let nearby: Vec<Value> = {
            let mut v: Vec<_> = st.entities.values().filter(|e| e.distance_to(p) < 32.0 && !e.is_item()).collect();
            v.sort_by(|a, b| a.distance_to(p).partial_cmp(&b.distance_to(p)).unwrap());
            v.into_iter().take(12).map(|e| json!({ "id": e.id, "kind": e.kind_name, "name": e.username, "distance": (e.distance_to(p) * 10.0).round() / 10.0 })).collect()
        };
        let data = json!({
            "online": !self.client.is_closed(),
            "username": st.username,
            "uuid": st.uuid.to_string(),
            "pos": [round2(p[0]), round2(p[1]), round2(p[2])],
            "yaw": st.body.yaw, "pitch": st.body.pitch,
            "on_ground": st.body.on_ground,
            "health": st.health, "food": st.food, "saturation": st.saturation, "xp_level": st.xp_level,
            "gamemode": st.gamemode,
            "dead": st.dead,
            "dimension": st.world.dimension.name,
            "biome": st.world.biome_at(fb.0, fb.1, fb.2),
            "block_below": st.world.block_name(fb.0, fb.1 - 1, fb.2),
            "day_time": st.day_time,
            "raining": st.raining,
            "mode": self.mode.label(),
            "goal": self.current_goal(),
            "task": self.task.as_ref().map(|t| t.1.name()),
            "inventory": st.inventory.summary(),
            "held": st.inventory.held().name(),
            "players_online": st.players.values().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "nearby": nearby,
            "chunks_loaded": st.world.chunks.len(),
            "packets_received": st.packets_received,
            "server": st.server,
            "ticks": self.tick,
            "extra": self.status_extra,
        });
        drop(st);
        self.out.emit("status", data);
    }

    // ───────────────────────────── commands ─────────────────────────────

    pub fn on_command(&mut self, cmd: &Value) -> Result<Value, String> {
        let c = cmd.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
        let num = |k: &str| cmd.get(k).and_then(|v| v.as_f64());
        let s = |k: &str| cmd.get(k).and_then(|v| v.as_str()).map(String::from);
        match c {
            "chat" => {
                let text = s("text").ok_or("text required")?;
                self.client.chat(&text);
                self.out.emit("said", json!({ "text": text }));
            }
            "mode" => {
                let f = self.feet();
                let m = Mode::parse(&s("mode").unwrap_or_default(), s("target").as_deref(), [f.0, f.1, f.2]).ok_or("unknown mode")?;
                self.abort_task();
                self.mode = m;
                self.stopped_until = 0;
            }
            "goto" => {
                let (x, y, z) = (num("x").ok_or("x")?, num("y").ok_or("y")?, num("z").ok_or("z")?);
                let g = Goto::new((x.floor() as i32, y.floor() as i32, z.floor() as i32), num("tolerance").unwrap_or(1.0)).sprinting(cmd.get("sprint").and_then(|v| v.as_bool()).unwrap_or(false));
                let g = if cmd.get("dig").and_then(|v| v.as_bool()).unwrap_or(false) { g.digging() } else { g };
                self.set_task("go to a location", Box::new(g), true);
            }
            "walk" => {
                // Walk forward for N ticks in the current direction (raw movement test).
                let ticks = num("ticks").unwrap_or(40.0) as u64;
                self.set_task("walk", Box::new(WalkFor::new(ticks, num("yaw").map(|v| v as f32))), true);
            }
            "look" => {
                let yaw = num("yaw").unwrap_or(0.0) as f32;
                let pitch = num("pitch").unwrap_or(0.0) as f32;
                self.set_task("look", Box::new(LookFor::new(yaw, pitch, 30)), true);
            }
            "dig" => {
                let b = (num("x").ok_or("x")? as i32, num("y").ok_or("y")? as i32, num("z").ok_or("z")? as i32);
                let mut seq: Vec<Box<dyn Task>> = vec![];
                let eye = self.client.state.lock().unwrap().body.eye();
                if dist3(eye, center(b)) > 4.2 {
                    seq.push(Box::new(Goto::new(b, 3.0)));
                }
                seq.push(Box::new(Dig::new(b)));
                seq.push(Box::new(Collect::new(5.0)));
                self.set_task("dig a block", Box::new(Sequence::new("dig", seq)), true);
            }
            "dig_below" => {
                let f = self.feet();
                self.set_task("dig a block", Box::new(Sequence::new("dig", vec![Box::new(Dig::new((f.0, f.1 - 1, f.2))), Box::new(Collect::new(4.0))])), true);
            }
            "place" => {
                let b = (num("x").ok_or("x")? as i32, num("y").ok_or("y")? as i32, num("z").ok_or("z")? as i32);
                self.set_task("place a block", Box::new(Place::new(b, &s("item").ok_or("item")?)), true);
            }
            "craft" => {
                let item = s("item").ok_or("item")?;
                self.set_task(&format!("craft {item}"), Box::new(Craft::new(&item)), true);
            }
            "gather" => {
                let what = s("what").unwrap_or_else(|| "wood".into());
                let n = num("count").unwrap_or(8.0) as i32;
                let task: Box<dyn Task> = match what.as_str() {
                    "wood" | "logs" => Box::new(Gather::new("collect logs", is_log, is_log, n)),
                    "stone" | "cobblestone" => Box::new(Gather::new("mine cobblestone", is_stone_block, is_stone_item, n).digging()),
                    "dirt" => Box::new(Gather::new("dig dirt", is_dirt_block, |x| x == "dirt", n)),
                    "coal" => Box::new(Gather::new("mine coal", is_coal_ore, is_coal, n).digging()),
                    "iron" => Box::new(Gather::new("mine iron", is_iron_ore, is_raw_iron, n).digging()),
                    "diamonds" => Box::new(Gather::new("find diamonds", is_diamond_ore, is_diamond, n).digging()),
                    _ => return Err("unknown resource".into()),
                };
                self.set_task(&format!("gather {what}"), task, true);
            }
            "attack" => {
                let id = num("entity").ok_or("entity")? as i32;
                self.set_task("fight", Box::new(Attack::new(id)), true);
            }
            "attack_nearest" => {
                let kind = s("kind");
                let id = {
                    let st = self.client.state.lock().unwrap();
                    let me = st.body.pos;
                    st.entities
                        .values()
                        .filter(|e| match &kind {
                            Some(k) => &e.kind_name == k,
                            None => is_hostile(&e.kind_name),
                        })
                        .min_by(|a, b| a.distance_to(me).partial_cmp(&b.distance_to(me)).unwrap())
                        .map(|e| e.id)
                };
                let id = id.ok_or("no matching entity")?;
                self.set_task("fight", Box::new(Attack::new(id)), true);
            }
            "eat" => self.set_task("eat", Box::new(Eat::new()), true),
            "build_house" => {
                let f = self.feet();
                let t = self.house_plan((f.0, f.1, f.2));
                self.set_task("build a house", t, true);
            }
            "follow" => {
                self.mode = Mode::Follow(s("player").ok_or("player")?);
                self.abort_task();
            }
            "stop" => {
                self.abort_task();
                self.stopped_until = self.tick + num("seconds").unwrap_or(60.0) as u64 * 20;
            }
            "hotbar" => self.client.set_hotbar(num("slot").unwrap_or(0.0) as u8),
            "respawn" => self.client.respawn(),
            "viewer_chat" => {
                let (u, t) = (s("user").unwrap_or_default(), s("text").unwrap_or_default());
                self.on_viewer_chat(&u, &t);
            }
            "status" => self.emit_status(),
            "memory" => self.out.emit("memory_snapshot", serde_json::to_value(&self.memory).unwrap_or_default()),
            "say_commentary" => {
                let ev = s("event").unwrap_or_else(|| "goal".into());
                let d = s("detail").unwrap_or_default();
                self.last_spoken = 0;
                self.comment(&ev, &d, true);
            }
            _ => return Err(format!("unknown command {c}")),
        }
        Ok(json!({ "ok": true }))
    }
}

fn cooldown_key(goal: &str) -> String {
    let g = goal.to_lowercase();
    for (k, pat) in [
        ("logs", "wood"),
        ("planks", "planks"),
        ("table", "crafting table"),
        ("sticks", "sticks"),
        ("wooden_pickaxe", "wooden pickaxe"),
        ("stone_pickaxe", "stone pickaxe"),
        ("stone_sword", "stone sword"),
        ("stone", "stone"),
        ("coal", "coal"),
        ("iron", "iron"),
        ("diamonds", "diamond"),
        ("furnace", "furnace"),
        ("torch", "torch"),
        ("hunt", "hunt"),
        ("eat", "eat"),
        ("harvest", "harvest"),
        ("shelter", "shelter"),
    ] {
        if g.contains(pat) {
            return k.into();
        }
    }
    g
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl Eat {
    pub fn is_food_name(n: &str) -> bool {
        is_food(n)
    }
}

/// Walk straight for a number of ticks (used by tests and manual control).
pub struct WalkFor {
    ticks: u64,
    yaw: Option<f32>,
    end: u64,
}
impl WalkFor {
    pub fn new(ticks: u64, yaw: Option<f32>) -> Self {
        Self { ticks, yaw, end: 0 }
    }
}
impl Task for WalkFor {
    fn name(&self) -> String {
        "walk forward".into()
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.end == 0 {
            self.end = ctx.tick + self.ticks;
        }
        let yaw = self.yaw.unwrap_or_else(|| ctx.st().body.yaw);
        ctx.look = Some((yaw, 0.0));
        ctx.controls.forward = 1.0;
        let hc = ctx.st().body.horizontal_collision;
        ctx.controls.jump = hc;
        if ctx.tick >= self.end {
            Status::Done
        } else {
            Status::Running
        }
    }
}

pub struct LookFor {
    yaw: f32,
    pitch: f32,
    ticks: u64,
    end: u64,
}
impl LookFor {
    pub fn new(yaw: f32, pitch: f32, ticks: u64) -> Self {
        Self { yaw, pitch, ticks, end: 0 }
    }
}
impl Task for LookFor {
    fn name(&self) -> String {
        format!("look yaw={} pitch={}", self.yaw, self.pitch)
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        if self.end == 0 {
            self.end = ctx.tick + self.ticks;
        }
        ctx.look = Some((self.yaw, self.pitch));
        if ctx.tick >= self.end {
            Status::Done
        } else {
            Status::Running
        }
    }
}

/// Walks to a player and tosses them items matching a predicate.
pub struct GiveItems {
    goto: Goto,
    player: String,
    pred: fn(&str) -> bool,
    count: u32,
    given: u32,
    next: u64,
}
impl GiveItems {
    pub fn new(p: (i32, i32, i32), player: &str, pred: fn(&str) -> bool, count: u32) -> Self {
        Self { goto: Goto::new(p, 2.5), player: player.into(), pred, count, given: 0, next: 0 }
    }
}
impl Task for GiveItems {
    fn name(&self) -> String {
        format!("give items to {}", self.player)
    }
    fn step(&mut self, ctx: &mut Ctx) -> Status {
        match self.goto.step(ctx) {
            Status::Running => return Status::Running,
            _ => {}
        }
        let target = ctx.st().player_entity(&self.player).map(|e| [e.pos[0], e.pos[1] + 1.4, e.pos[2]]);
        if let Some(t) = target {
            ctx.look_at_point(t);
        }
        if ctx.tick < self.next {
            return Status::Running;
        }
        let pred = self.pred;
        match hold_item(ctx, pred) {
            None => return if self.given > 0 { Status::Done } else { Status::Failed("nothing to give".into()) },
            Some(false) => {
                self.next = ctx.tick + 3;
                return Status::Running;
            }
            Some(true) => {}
        }
        ctx.client.drop_held(false);
        ctx.client.swing();
        self.given += 1;
        self.next = ctx.tick + 6;
        if self.given >= self.count {
            Status::Done
        } else {
            Status::Running
        }
    }
}
