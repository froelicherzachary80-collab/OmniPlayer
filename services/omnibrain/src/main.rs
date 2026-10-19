//! `omniplayer-bot` — one autonomous AI Minecraft player.
//!
//! Configuration: `--config <file.json>` or the `OMNI_BOT_CONFIG` env var (JSON).
//! Authenticated accounts receive their Minecraft access token through the
//! `OMNI_MC_ACCESS_TOKEN` environment variable (never on disk or argv).
//! Control: JSON commands on stdin, JSON events on stdout (one per line).

mod agent;
mod crafting;
mod ctx;
mod gameplay;
mod intent;
mod io;
mod memory;
mod persona;
mod stream;
mod tasks;

use agent::{Agent, BrainConfig, Mode};
use anyhow::{bail, Context, Result};
use omnicore::auth::OnlineSession;
use omnicore::protocol::version;
use omnicore::{Client, ClientConfig};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Account {
    #[serde(rename = "type")]
    kind: String,
    profile_id: String,
    username: String,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
struct Config {
    bot_id: String,
    username: String,
    host: String,
    port: u16,
    version: String,
    account: Account,
    mode: String,
    mode_target: Option<String>,
    persona: persona::Persona,
    llm: persona::LlmConfig,
    memory: memory::Memory,
    brain: BrainConfig,
    reconnect: bool,
    max_reconnects: u32,
    view_distance: u8,
    stream: Option<stream::StreamConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bot_id: String::new(),
            username: "OmniBot".into(),
            host: "127.0.0.1".into(),
            port: 25565,
            version: "26.3".into(),
            account: Account { kind: "offline".into(), ..Default::default() },
            mode: "autonomous".into(),
            mode_target: None,
            persona: Default::default(),
            llm: Default::default(),
            memory: Default::default(),
            brain: Default::default(),
            reconnect: true,
            max_reconnects: 50,
            view_distance: 6,
            stream: None,
        }
    }
}

fn load_config() -> Result<Config> {
    let args: Vec<String> = std::env::args().collect();
    let text = if let Some(i) = args.iter().position(|a| a == "--config") {
        std::fs::read_to_string(args.get(i + 1).context("--config needs a path")?)?
    } else if let Ok(v) = std::env::var("OMNI_BOT_CONFIG") {
        v
    } else {
        bail!("usage: omniplayer-bot --config <file.json>  (or set OMNI_BOT_CONFIG)");
    };
    let mut cfg: Config = serde_json::from_str(&text).context("invalid bot config")?;
    if cfg.bot_id.is_empty() {
        cfg.bot_id = cfg.username.clone();
    }
    Ok(cfg)
}

fn client_config(cfg: &Config) -> Result<ClientConfig> {
    let adapter = version::by_name(&cfg.version).with_context(|| {
        format!("Minecraft {} is not supported by this OmniCore build (supported: {})", cfg.version, version::supported().iter().map(|a| a.name).collect::<Vec<_>>().join(", "))
    })?;
    let mut cc = ClientConfig::offline(&cfg.host, cfg.port, &cfg.username);
    cc.adapter = adapter;
    cc.view_distance = cfg.view_distance.clamp(2, 16);
    match cfg.account.kind.as_str() {
        "offline" | "" => {
            if !omnicore::auth::valid_username(&cfg.username) {
                bail!("invalid Minecraft username '{}'", cfg.username);
            }
        }
        "microsoft" | "authenticated" => {
            let token = std::env::var("OMNI_MC_ACCESS_TOKEN").context("authenticated account selected but OMNI_MC_ACCESS_TOKEN is not set")?;
            let id = uuid::Uuid::parse_str(&cfg.account.profile_id).context("account.profile_id must be the Minecraft profile UUID")?;
            let name = if cfg.account.username.is_empty() { cfg.username.clone() } else { cfg.account.username.clone() };
            cc.username = name.clone();
            cc.online = Some(OnlineSession { access_token: token, profile_id: id, username: name });
        }
        other => bail!("unknown account type {other}"),
    }
    Ok(cc)
}

/// Kicks that should not trigger an automatic reconnect.
fn is_permanent(reason: &str) -> bool {
    let r = reason.to_lowercase();
    ["whitelist", "banned", "logged in from another location", "outdated", "incompatible", "online mode", "authenticated", "verify username", "invalid bot"].iter().any(|k| r.contains(k))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    };
    let out = io::Output::new(&cfg.bot_id);
    if let Err(e) = run(cfg, out.clone()).await {
        out.emit("fatal", json!({ "error": format!("{e:#}") }));
        std::process::exit(1);
    }
}

async fn run(cfg: Config, out: io::Output) -> Result<()> {
    // stdin reader thread → command channel
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(v) => {
                    if cmd_tx.send(v).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("bad command json: {e}"),
            }
        }
        let _ = cmd_tx.send(json!({ "cmd": "__stdin_closed" }));
    });

    let cc = client_config(&cfg)?;
    let mut memory = cfg.memory.clone();
    let mut mode: Option<Mode> = None;
    let mut attempts = 0u32;
    let mut streamer: Option<stream::Streamer> = None;
    loop {
        // Protocol version detection via server list ping before joining.
        out.emit("connecting", json!({ "host": cfg.host, "port": cfg.port, "attempt": attempts + 1 }));
        match omnicore::client::ping_status(&cfg.host, cfg.port, Duration::from_secs(5)).await {
            Ok(s) => {
                out.emit("server_status", serde_json::to_value(&s)?);
                if s.protocol != cc.adapter.protocol {
                    let msg = format!(
                        "server runs protocol {} ({}), this bot speaks {} ({})",
                        s.protocol, s.version_name, cc.adapter.protocol, cc.adapter.name
                    );
                    match version::by_protocol(s.protocol) {
                        Some(_) => out.log("warn", format!("{msg}; another adapter exists, set version accordingly")),
                        None => out.log("warn", format!("{msg}; no adapter for it, joining will likely fail")),
                    }
                }
            }
            Err(e) => out.log("warn", format!("status ping failed: {e:#}")),
        }
        let result = Client::connect(cc.clone()).await;
        let (client, mut events) = match result {
            Ok(x) => x,
            Err(e) => {
                let reason = format!("{e:#}");
                out.emit("connect_failed", json!({ "reason": reason }));
                attempts += 1;
                if !cfg.reconnect || is_permanent(&reason) || attempts > cfg.max_reconnects {
                    bail!("could not connect: {reason}");
                }
                let wait = (5u64 << attempts.min(4)).min(60);
                if wait_or_quit(&mut cmd_rx, Duration::from_secs(wait)).await {
                    return Ok(());
                }
                continue;
            }
        };
        attempts = 0;
        out.emit("connected", json!({ "username": cc.username, "uuid": cc.uuid().to_string(), "protocol": cc.adapter.protocol, "version": cc.adapter.name }));
        let voice = persona::Voice::new(cfg.persona.clone(), cfg.llm.clone());
        let here = [0, 0, 0];
        let m = mode.clone().unwrap_or_else(|| Mode::parse(&cfg.mode, cfg.mode_target.as_deref(), here).unwrap_or(Mode::Autonomous));
        let mut agent = Agent::new(client.clone(), out.clone(), cfg.brain.clone(), voice, memory.clone(), m);
        if let Some(s) = streamer.as_mut() {
            s.attach(client.clone());
        }
        let mut ticker = tokio::time::interval(Duration::from_millis(50));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut quit = false;
        let mut disconnect_reason = String::from("unknown");
        let mut slow_ticks = 0u32;
        'session: loop {
            ticker.tick().await;
            let started = Instant::now();
            while let Ok(e) = events.try_recv() {
                if let omnicore::Event::Disconnected { reason } = &e {
                    disconnect_reason = reason.clone();
                }
                if let Some(s) = streamer.as_mut() {
                    s.on_event(&e);
                }
                agent.on_event(&e);
            }
            if client.is_closed() {
                break 'session;
            }
            while let Ok(cmd) = cmd_rx.try_recv() {
                let c = cmd.get("cmd").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let id = cmd.get("id").cloned().unwrap_or(Value::Null);
                match c.as_str() {
                    "disconnect" | "quit" | "__stdin_closed" => {
                        quit = true;
                        client.disconnect();
                        disconnect_reason = "stopped by controller".into();
                        break 'session;
                    }
                    "reconnect" => {
                        client.disconnect();
                        disconnect_reason = "reconnect requested".into();
                        break 'session;
                    }
                    "stream_start" => {
                        let sc: stream::StreamConfig = serde_json::from_value(cmd.get("config").cloned().unwrap_or(json!({}))).unwrap_or_default();
                        match stream::Streamer::start(sc, client.clone(), out.clone()) {
                            Ok(s) => {
                                streamer = Some(s);
                                out.emit("ack", json!({ "id": id, "ok": true }));
                            }
                            Err(e) => out.emit("ack", json!({ "id": id, "ok": false, "error": format!("{e:#}") })),
                        }
                    }
                    "stream_stop" => {
                        if let Some(mut s) = streamer.take() {
                            s.stop();
                        }
                        out.emit("ack", json!({ "id": id, "ok": true }));
                    }
                    "speak" => {
                        // Audio for the stream produced by VoiceBox (PCM s16le mono 24 kHz, base64).
                        if let Some(s) = streamer.as_mut() {
                            if let Some(b64) = cmd.get("pcm_b64").and_then(|v| v.as_str()) {
                                s.queue_audio_b64(b64);
                            }
                            if let Some(t) = cmd.get("caption").and_then(|v| v.as_str()) {
                                s.set_caption(t);
                            }
                        }
                    }
                    _ => {
                        let r = agent.on_command(&cmd);
                        out.emit("ack", match r {
                            Ok(_) => json!({ "id": id, "ok": true, "cmd": c }),
                            Err(e) => json!({ "id": id, "ok": false, "cmd": c, "error": e }),
                        });
                    }
                }
            }
            agent.tick();
            if started.elapsed() > Duration::from_millis(45) {
                slow_ticks += 1;
                if slow_ticks % 100 == 1 {
                    out.log("warn", format!("brain tick took {:?}", started.elapsed()));
                }
            }
        }
        memory = agent.memory.clone();
        mode = Some(agent.mode.clone());
        out.emit("memory_snapshot", serde_json::to_value(&memory)?);
        out.emit("session_ended", json!({ "reason": disconnect_reason }));
        if let Some(s) = streamer.as_mut() {
            s.detach();
        }
        if quit {
            if let Some(mut s) = streamer.take() {
                s.stop();
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            return Ok(());
        }
        if !cfg.reconnect || is_permanent(&disconnect_reason) {
            bail!("disconnected: {disconnect_reason}");
        }
        attempts += 1;
        if attempts > cfg.max_reconnects {
            bail!("giving up after {attempts} reconnect attempts");
        }
        let wait = (2u64 << attempts.min(4)).min(60);
        out.emit("reconnecting", json!({ "in_seconds": wait, "reason": disconnect_reason }));
        if wait_or_quit(&mut cmd_rx, Duration::from_secs(wait)).await {
            return Ok(());
        }
    }
}

/// Waits, but returns true early if the controller asks the bot to quit.
async fn wait_or_quit(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>, d: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + d;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return false,
            c = rx.recv() => {
                match c.as_ref().and_then(|v| v.get("cmd")).and_then(|v| v.as_str()) {
                    Some("disconnect") | Some("quit") | Some("__stdin_closed") | None => return true,
                    Some("reconnect") => return false,
                    _ => {}
                }
            }
        }
    }
}
