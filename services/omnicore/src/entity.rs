//! Tracked entities and tab-list players.

use crate::data::GAME;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct Entity {
    pub id: i32,
    pub uuid: Uuid,
    pub kind: u32,
    pub kind_name: String,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    /// Player username when the entity is a player listed in the tab list.
    pub username: Option<String>,
    pub last_hurt_tick: u64,
}

impl Entity {
    pub fn new(id: i32, uuid: Uuid, kind: u32, pos: [f64; 3]) -> Self {
        Self {
            id,
            uuid,
            kind,
            kind_name: GAME.entity(kind).map(|e| e.name.clone()).unwrap_or_else(|| format!("entity_{kind}")),
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            on_ground: false,
            username: None,
            last_hurt_tick: 0,
        }
    }

    pub fn height(&self) -> f64 {
        GAME.entity(self.kind).map(|e| e.height as f64).unwrap_or(1.8)
    }

    pub fn width(&self) -> f64 {
        GAME.entity(self.kind).map(|e| e.width as f64).unwrap_or(0.6)
    }

    pub fn is_player(&self) -> bool {
        self.kind_name == "player"
    }

    pub fn is_item(&self) -> bool {
        self.kind_name == "item"
    }

    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        ((self.pos[0] - p[0]).powi(2) + (self.pos[1] - p[1]).powi(2) + (self.pos[2] - p[2]).powi(2)).sqrt()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TabPlayer {
    pub uuid: Uuid,
    pub name: String,
    pub gamemode: i32,
    pub latency: i32,
    pub has_chat_session: bool,
}
