//! Line-delimited JSON control interface (stdin commands, stdout events).
//! The backend API spawns one bot process per AI player and talks to it here.

use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Output {
    inner: Arc<Mutex<std::io::Stdout>>,
    pub bot: String,
}

impl Output {
    pub fn new(bot: &str) -> Self {
        Self { inner: Arc::new(Mutex::new(std::io::stdout())), bot: bot.to_string() }
    }

    pub fn emit(&self, kind: &str, data: Value) {
        let line = json!({ "type": kind, "bot": self.bot, "ts": now_ms(), "data": data });
        let mut out = self.inner.lock().unwrap();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    pub fn log(&self, level: &str, msg: impl AsRef<str>) {
        self.emit("log", json!({ "level": level, "message": msg.as_ref() }));
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
