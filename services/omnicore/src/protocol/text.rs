//! Text component flattening (NBT or JSON) into readable plain text.

use super::nbt::Nbt;
use serde_json::Value;

/// The few vanilla translation keys a bot needs to understand chat and events.
fn translation(key: &str) -> Option<&'static str> {
    Some(match key {
        "chat.type.text" => "<%s> %s",
        "chat.type.announcement" => "[%s] %s",
        "chat.type.emote" => "* %s %s",
        "chat.type.admin" => "[%s: %s]",
        "commands.message.display.incoming" => "%s whispers to you: %s",
        "commands.message.display.outgoing" => "You whisper to %s: %s",
        "multiplayer.player.joined" => "%s joined the game",
        "multiplayer.player.left" => "%s left the game",
        "multiplayer.disconnect.not_whitelisted" => "You are not whitelisted on this server!",
        "multiplayer.disconnect.server_shutdown" => "Server closed",
        "multiplayer.disconnect.kicked" => "Kicked by an operator",
        "multiplayer.disconnect.duplicate_login" => "You logged in from another location",
        "multiplayer.disconnect.unverified_username" => "Failed to verify username!",
        "multiplayer.disconnect.outdated_client" => "Incompatible client! Please use %s",
        "multiplayer.disconnect.invalid_player_movement" => "Invalid move player packet received",
        "multiplayer.disconnect.flying" => "Flying is not enabled on this server",
        "disconnect.timeout" => "Timed out",
        "death.attack.generic" => "%s died",
        "death.attack.mob" => "%s was slain by %s",
        "death.attack.player" => "%s was slain by %s",
        "death.attack.fall" => "%s hit the ground too hard",
        "death.attack.lava" => "%s tried to swim in lava",
        "death.attack.drown" => "%s drowned",
        "death.attack.starve" => "%s starved to death",
        "death.attack.inFire" => "%s went up in flames",
        "death.attack.onFire" => "%s burned to death",
        "death.attack.explosion.player" => "%s was blown up by %s",
        "death.fell.accident.generic" => "%s fell from a high place",
        _ => return None,
    })
}

fn apply_translation(fmt: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut next = 0usize;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            // %s or %1$s
            let mut digits = String::new();
            while let Some(d) = chars.peek().copied() {
                if d.is_ascii_digit() {
                    digits.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            if !digits.is_empty() && chars.peek() == Some(&'$') {
                chars.next();
            }
            if chars.peek() == Some(&'s') {
                chars.next();
                let idx = if digits.is_empty() {
                    next += 1;
                    next - 1
                } else {
                    digits.parse::<usize>().unwrap_or(1).saturating_sub(1)
                };
                out.push_str(args.get(idx).map(|s| s.as_str()).unwrap_or(""));
                continue;
            }
            if chars.peek() == Some(&'%') {
                chars.next();
            }
            out.push('%');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn nbt_to_plain(n: &Nbt) -> String {
    match n {
        Nbt::String(s) => s.clone(),
        Nbt::List(items) => items.iter().map(nbt_to_plain).collect(),
        Nbt::Compound(_) => {
            let mut out = String::new();
            if let Some(t) = n.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            } else if let Some(Nbt::String(key)) = n.get("translate") {
                let args: Vec<String> = match n.get("with") {
                    Some(Nbt::List(w)) => w.iter().map(nbt_to_plain).collect(),
                    _ => vec![],
                };
                match translation(key) {
                    Some(fmt) => out.push_str(&apply_translation(fmt, &args)),
                    None => {
                        let fallback = n.get("fallback").and_then(|f| f.as_str());
                        match fallback {
                            Some(f) => out.push_str(&apply_translation(f, &args)),
                            None if args.is_empty() => out.push_str(key),
                            None => out.push_str(&format!("{key} {}", args.join(" "))),
                        }
                    }
                }
            } else if let Some(Nbt::String(s)) = n.get("selector") {
                out.push_str(s);
            } else if let Some(Nbt::Compound(_)) = n.get("score") {
                // scores are rendered client-side; skip
            } else if let Some(Nbt::String(k)) = n.get("keybind") {
                out.push_str(k);
            } else if let Some(v) = n.get("") {
                // Heterogeneous lists wrap primitives in {"": value}
                out.push_str(&nbt_to_plain(v));
            }
            if let Some(Nbt::List(extra)) = n.get("extra") {
                for e in extra {
                    out.push_str(&nbt_to_plain(e));
                }
            }
            out
        }
        Nbt::Int(v) => v.to_string(),
        Nbt::Byte(v) => v.to_string(),
        Nbt::Long(v) => v.to_string(),
        Nbt::Double(v) => v.to_string(),
        Nbt::Float(v) => v.to_string(),
        _ => String::new(),
    }
}

pub fn json_to_plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(json_to_plain).collect(),
        Value::Object(o) => {
            let mut out = String::new();
            if let Some(Value::String(t)) = o.get("text") {
                out.push_str(t);
            } else if let Some(Value::String(key)) = o.get("translate") {
                let args: Vec<String> = match o.get("with") {
                    Some(Value::Array(w)) => w.iter().map(json_to_plain).collect(),
                    _ => vec![],
                };
                match translation(key) {
                    Some(fmt) => out.push_str(&apply_translation(fmt, &args)),
                    None if args.is_empty() => out.push_str(key),
                    None => out.push_str(&format!("{key} {}", args.join(" "))),
                }
            }
            if let Some(Value::Array(extra)) = o.get("extra") {
                for e in extra {
                    out.push_str(&json_to_plain(e));
                }
            }
            out
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

pub fn json_str_to_plain(s: &str) -> String {
    match serde_json::from_str::<Value>(s) {
        Ok(v) => json_to_plain(&v),
        Err(_) => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn translates_chat() {
        let mut m = BTreeMap::new();
        m.insert("translate".into(), Nbt::String("chat.type.text".into()));
        m.insert("with".into(), Nbt::List(vec![Nbt::String("Steve".into()), Nbt::String("follow me".into())]));
        assert_eq!(nbt_to_plain(&Nbt::Compound(m)), "<Steve> follow me");
    }

    #[test]
    fn json_join() {
        assert_eq!(
            json_str_to_plain(r#"{"translate":"multiplayer.player.joined","with":[{"text":"Alex"}]}"#),
            "Alex joined the game"
        );
        assert_eq!(apply_translation("%2$s then %1$s", &["a".into(), "b".into()]), "b then a");
    }
}
