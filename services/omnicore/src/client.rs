//! OmniCore network client: handshake → login → configuration → play.

use crate::auth::{self, OnlineSession};
use crate::data::GAME;
use crate::entity::{Entity, TabPlayer};
use crate::inventory::{self, Inventory, ItemStack, Window};
use crate::physics::{Body, Controls};
use crate::protocol::codec::{PacketReader, PacketWriter};
use crate::protocol::text::{json_str_to_plain, nbt_to_plain};
use crate::protocol::version::{PacketIds, VersionAdapter};
use crate::protocol::{Nbt, Reader, Writer};
use crate::world::{Dimension, World};
use anyhow::{anyhow, bail, Context, Result};
use rand::RngCore;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Present for authenticated (online-mode) accounts.
    pub online: Option<OnlineSession>,
    pub adapter: VersionAdapter,
    pub view_distance: u8,
    pub brand: String,
    pub locale: String,
    pub connect_timeout: Duration,
}

impl ClientConfig {
    pub fn offline(host: &str, port: u16, username: &str) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            online: None,
            adapter: crate::protocol::version::adapter_26_3(),
            view_distance: 6,
            brand: "omnicore".into(),
            locale: "en_us".into(),
            connect_timeout: Duration::from_secs(10),
        }
    }

    pub fn uuid(&self) -> Uuid {
        match &self.online {
            Some(s) => s.profile_id,
            None => auth::offline_uuid(&self.username),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase {
    Login = 0,
    Configuration = 1,
    Play = 2,
    Closed = 3,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    LoggedIn { uuid: String, username: String },
    ConfigurationStarted,
    Joined { entity_id: i32, gamemode: i32, dimension: String },
    Spawned { pos: [f64; 3] },
    Teleported { pos: [f64; 3] },
    Chat { sender: Option<String>, sender_uuid: Option<String>, message: String, raw: String, kind: String },
    PlayerJoined { uuid: String, name: String },
    PlayerLeft { uuid: String, name: String },
    EntitySpawned { id: i32, kind: String, pos: [f64; 3], username: Option<String> },
    EntityRemoved { id: i32 },
    BlockChanged { pos: (i32, i32, i32), state: u32, name: String },
    ChunkLoaded { x: i32, z: i32 },
    Health { health: f32, food: i32, saturation: f32 },
    Hurt { source_entity: Option<i32>, damage_type: i32 },
    Died { message: String },
    Respawned { dimension: String },
    InventoryChanged,
    WindowOpened { id: i32, kind: i32, title: String },
    WindowClosed { id: i32 },
    ItemPickedUp { collector: i32, count: i32 },
    TimeUpdate { day_time: i64 },
    Weather { raining: bool },
    Disconnected { reason: String },
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ServerInfo {
    pub hardcore: bool,
    pub max_players: i32,
    pub view_distance: i32,
    pub online_mode: bool,
    pub enforces_secure_chat: bool,
    pub brand: Option<String>,
}

#[derive(Serialize)]
pub struct GameState {
    pub entity_id: i32,
    pub uuid: Uuid,
    pub username: String,
    pub body: Body,
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub xp_level: i32,
    pub gamemode: i32,
    pub dead: bool,
    pub spawned: bool,
    pub loaded_sent: bool,
    #[serde(skip)]
    pub world: World,
    pub entities: HashMap<i32, Entity>,
    pub players: HashMap<Uuid, TabPlayer>,
    pub inventory: Inventory,
    pub day_time: i64,
    pub raining: bool,
    pub server: ServerInfo,
    #[serde(skip)]
    pub dimension_types: Vec<Dimension>,
    pub ticks: u64,
    /// Block currently being mined: (pos, start tick, expected ticks). Set by the brain.
    pub digging: Option<((i32, i32, i32), u64, u32)>,
    pub last_swing_tick: u64,
    pub last_acked_sequence: i32,
    pub chunks_received: u64,
    pub packets_received: u64,
    /// Whether the server sent at least one chunk batch.
    pub chunk_batches: u64,
    #[serde(skip)]
    last_sent: Option<([f64; 3], f32, f32, bool)>,
    #[serde(skip)]
    ticks_since_pos: u32,
    #[serde(skip)]
    last_input: u8,
    #[serde(skip)]
    sprinting: bool,
    #[serde(skip)]
    chat: ChatTracker,
}

#[derive(Default)]
struct ChatTracker {
    /// Signed messages seen since our last acknowledgement.
    pending: i32,
    /// Total signed messages seen (window of the last 20 is acknowledged).
    total: u64,
    recent: VecDeque<[u8; 256]>,
}

impl GameState {
    fn new(uuid: Uuid, username: String, view_distance: u8) -> Self {
        let mut world = World::new();
        let r = view_distance as usize * 2 + 3;
        world.max_chunks = r * r;
        Self {
            entity_id: -1,
            uuid,
            username,
            body: Body::default(),
            health: 20.0,
            food: 20,
            saturation: 5.0,
            xp_level: 0,
            gamemode: 0,
            dead: false,
            spawned: false,
            loaded_sent: false,
            world,
            entities: HashMap::new(),
            players: HashMap::new(),
            inventory: Inventory::default(),
            day_time: 0,
            raining: false,
            server: ServerInfo::default(),
            dimension_types: vec![],
            ticks: 0,
            digging: None,
            last_swing_tick: 0,
            last_acked_sequence: 0,
            chunks_received: 0,
            packets_received: 0,
            chunk_batches: 0,
            last_sent: None,
            ticks_since_pos: 0,
            last_input: 0,
            sprinting: false,
            chat: ChatTracker::default(),
        }
    }

    pub fn player_name(&self, uuid: &Uuid) -> Option<&str> {
        self.players.get(uuid).map(|p| p.name.as_str())
    }

    pub fn player_entity(&self, name: &str) -> Option<&Entity> {
        self.entities.values().find(|e| e.username.as_deref().map(|u| u.eq_ignore_ascii_case(name)).unwrap_or(false))
    }
}

/// Handle to a connected bot. Cloneable; all clones share the same connection.
#[derive(Clone)]
pub struct Client {
    pub ids: Arc<PacketIds>,
    pub state: Arc<Mutex<GameState>>,
    tx: mpsc::UnboundedSender<Vec<u8>>,
    phase: Arc<AtomicU8>,
    sequence: Arc<AtomicI32>,
    closed: Arc<AtomicBool>,
}

pub type Events = mpsc::UnboundedReceiver<Event>;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// ───────────────────────────── server list ping ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct StatusInfo {
    pub version_name: String,
    pub protocol: i32,
    pub players_online: i64,
    pub players_max: i64,
    pub sample: Vec<String>,
    pub motd: String,
    pub latency_ms: u64,
    pub enforces_secure_chat: Option<bool>,
}

/// Server List Ping: detects the server's protocol version before joining.
pub async fn ping_status(host: &str, port: u16, timeout: Duration) -> Result<StatusInfo> {
    let fut = async {
        let stream = TcpStream::connect((host, port)).await?;
        stream.set_nodelay(true)?;
        let (r, w) = stream.into_split();
        let mut r = PacketReader::new(r);
        let mut w = PacketWriter::new(w);
        let mut hs = Writer::with_id(0x00);
        hs.varint(-1).string(host).u16(port).varint(1);
        w.write_packet(&hs.buf).await?;
        w.write_packet(&Writer::with_id(0x00).buf).await?;
        let pkt = r.read_packet().await?;
        let mut rd = Reader::new(&pkt);
        if rd.varint()? != 0 {
            bail!("unexpected status response id");
        }
        let json: serde_json::Value = serde_json::from_str(&rd.string()?)?;
        let start = std::time::Instant::now();
        let mut ping = Writer::with_id(0x01);
        ping.i64(now_ms());
        w.write_packet(&ping.buf).await?;
        let _ = r.read_packet().await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let motd = match json.get("description") {
            Some(v) => crate::protocol::text::json_to_plain(v),
            None => String::new(),
        };
        Ok(StatusInfo {
            version_name: json["version"]["name"].as_str().unwrap_or("").to_string(),
            protocol: json["version"]["protocol"].as_i64().unwrap_or(-1) as i32,
            players_online: json["players"]["online"].as_i64().unwrap_or(0),
            players_max: json["players"]["max"].as_i64().unwrap_or(0),
            sample: json["players"]["sample"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["name"].as_str().map(String::from)).collect())
                .unwrap_or_default(),
            motd,
            latency_ms,
            enforces_secure_chat: json.get("enforcesSecureChat").and_then(|v| v.as_bool()),
        })
    };
    tokio::time::timeout(timeout, fut).await.map_err(|_| anyhow!("status ping timed out"))?
}

// ───────────────────────────────── connect ──────────────────────────────────

struct Conn {
    r: PacketReader<OwnedReadHalf>,
    w: PacketWriter<OwnedWriteHalf>,
}

impl Client {
    /// Connects, logs in and completes the configuration phase.
    /// Returns once the connection has entered the play state.
    pub async fn connect(cfg: ClientConfig) -> Result<(Client, Events)> {
        let ids = Arc::new(cfg.adapter.ids.clone());
        let stream = tokio::time::timeout(cfg.connect_timeout, TcpStream::connect((cfg.host.as_str(), cfg.port)))
            .await
            .map_err(|_| anyhow!("connection to {}:{} timed out", cfg.host, cfg.port))?
            .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;
        stream.set_nodelay(true)?;
        let (r, w) = stream.into_split();
        let mut conn = Conn { r: PacketReader::new(r), w: PacketWriter::new(w) };
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();

        // Handshake (intent 2 = login)
        let mut hs = Writer::with_id(0x00);
        hs.varint(cfg.adapter.protocol).string(&cfg.host).u16(cfg.port).varint(2);
        conn.w.write_packet(&hs.buf).await?;

        let uuid = cfg.uuid();
        let mut ls = Writer::with_id(ids.login_start);
        ls.string(&cfg.username).uuid(&uuid);
        conn.w.write_packet(&ls.buf).await?;

        let state = Arc::new(Mutex::new(GameState::new(uuid, cfg.username.clone(), cfg.view_distance)));

        // ── Login ──
        loop {
            let pkt = tokio::time::timeout(Duration::from_secs(30), conn.r.read_packet())
                .await
                .map_err(|_| anyhow!("login timed out"))??;
            let mut r = Reader::new(&pkt);
            let id = r.varint()?;
            if id == ids.s_login_disconnect {
                let reason = json_str_to_plain(&r.string()?);
                bail!("server refused login: {reason}");
            } else if id == ids.s_login_encryption_request {
                let server_id = r.string()?;
                let pubkey = r.byte_array()?;
                let token = r.byte_array()?;
                let should_auth = r.bool().unwrap_or(true);
                let mut secret = [0u8; 16];
                rand::thread_rng().fill_bytes(&mut secret);
                if should_auth {
                    let session = cfg.online.clone().ok_or_else(|| {
                        anyhow!("this server is in online mode and requires an authenticated Microsoft/Minecraft account; offline accounts cannot join it")
                    })?;
                    let hash = auth::server_hash(&server_id, &secret, &pubkey);
                    tokio::task::spawn_blocking(move || auth::session_join(&session, &hash)).await??;
                }
                let (es, et) = rsa_encrypt(&pubkey, &secret, &token)?;
                let mut resp = Writer::with_id(ids.login_encryption_response);
                resp.byte_array(&es).byte_array(&et);
                conn.w.write_packet(&resp.buf).await?;
                conn.r.enable_encryption(&secret);
                conn.w.enable_encryption(&secret);
            } else if id == ids.s_login_compression {
                let t = r.varint()?;
                let c = if t >= 0 { Some(t as usize) } else { None };
                conn.r.compression = c;
                conn.w.compression = c;
            } else if id == ids.s_login_success {
                let puuid = r.uuid()?;
                let name = r.string()?;
                {
                    let mut st = state.lock().unwrap();
                    st.uuid = puuid;
                    st.username = name.clone();
                }
                let _ = ev_tx.send(Event::LoggedIn { uuid: puuid.to_string(), username: name });
                conn.w.write_packet(&Writer::with_id(ids.login_acknowledged).buf).await?;
                break;
            } else if id == ids.s_login_plugin_request {
                let mid = r.varint()?;
                let mut resp = Writer::with_id(ids.login_plugin_response);
                resp.varint(mid).bool(false);
                conn.w.write_packet(&resp.buf).await?;
            } else if id == ids.s_login_cookie_request {
                let key = r.string()?;
                let mut resp = Writer::with_id(ids.login_cookie_response);
                resp.string(&key).bool(false);
                conn.w.write_packet(&resp.buf).await?;
            } else {
                bail!("unexpected login packet 0x{id:02X}");
            }
        }

        // ── Configuration ──
        send_client_information(&mut conn.w, &ids, &cfg, ids.c_client_information).await?;
        let mut brand = Writer::with_id(ids.c_plugin_message);
        brand.string("minecraft:brand").string(&cfg.brand);
        conn.w.write_packet(&brand.buf).await?;
        configuration_loop(&mut conn, &ids, &state, &ev_tx, &cfg).await?;

        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let client = Client {
            ids: ids.clone(),
            state: state.clone(),
            tx,
            phase: Arc::new(AtomicU8::new(Phase::Play as u8)),
            sequence: Arc::new(AtomicI32::new(1)),
            closed: Arc::new(AtomicBool::new(false)),
        };
        let Conn { r, w } = conn;
        tokio::spawn(writer_task(w, rx, client.closed.clone()));
        tokio::spawn(reader_task(r, client.clone(), ev_tx, cfg));
        Ok((client, ev_rx))
    }

    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::SeqCst) {
            0 => Phase::Login,
            1 => Phase::Configuration,
            2 => Phase::Play,
            _ => Phase::Closed,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn send(&self, w: Writer) {
        if self.tx.send(w.buf).is_err() {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    fn in_play(&self) -> bool {
        self.phase() == Phase::Play
    }

    fn next_seq(&self) -> i32 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// Closes the connection (the server sees a normal disconnect).
    pub fn disconnect(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Vec::new()); // empty packet = shutdown sentinel
    }

    // ───────────── chat ─────────────

    pub fn chat(&self, msg: &str) {
        if !self.in_play() {
            return;
        }
        let msg: String = msg.chars().filter(|c| *c != '§' && !c.is_control()).take(256).collect();
        if msg.is_empty() {
            return;
        }
        if let Some(cmd) = msg.strip_prefix('/') {
            let mut w = Writer::with_id(self.ids.p_chat_command);
            w.string(cmd);
            self.send(w);
            return;
        }
        let (offset, bits) = {
            let mut st = self.state.lock().unwrap();
            let off = st.chat.pending;
            st.chat.pending = 0;
            let n = st.chat.recent.len().min(20);
            let mut bits: u32 = 0;
            for i in 0..n {
                bits |= 1 << (20 - n + i);
            }
            (off, bits)
        };
        let mut w = Writer::with_id(self.ids.p_chat_message);
        w.string(&msg).i64(now_ms()).i64(rand::random::<i64>()).bool(false).varint(offset);
        w.u8(bits as u8).u8((bits >> 8) as u8).u8((bits >> 16) as u8);
        w.u8(0); // checksum 0 = not provided
        self.send(w);
    }

    // ───────────── movement ─────────────

    /// Runs one movement tick (physics + movement packets + tick end).
    pub fn tick(&self, controls: &Controls) {
        if !self.in_play() {
            return;
        }
        let mut packets = Vec::new();
        {
            let mut st = self.state.lock().unwrap();
            st.ticks += 1;
            if !st.spawned || st.dead {
                drop(st);
                self.send(Writer::with_id(self.ids.p_tick_end));
                return;
            }
            // Player input packet (movement keys) when it changes.
            let mut flags = 0u8;
            if controls.forward > 0.0 {
                flags |= 1;
            }
            if controls.forward < 0.0 {
                flags |= 2;
            }
            if controls.strafe > 0.0 {
                flags |= 4;
            }
            if controls.strafe < 0.0 {
                flags |= 8;
            }
            if controls.jump {
                flags |= 16;
            }
            if controls.sneak {
                flags |= 32;
            }
            if controls.sprint {
                flags |= 64;
            }
            if flags != st.last_input {
                st.last_input = flags;
                let mut w = Writer::with_id(self.ids.p_player_input);
                w.u8(flags);
                packets.push(w);
            }
            let want_sprint = controls.sprint && controls.forward > 0.0 && st.food > 6;
            if want_sprint != st.sprinting {
                st.sprinting = want_sprint;
                let mut w = Writer::with_id(self.ids.p_player_command);
                w.varint(st.entity_id).varint(if want_sprint { 1 } else { 2 }).varint(0);
                packets.push(w);
            }
            let world = std::mem::take(&mut st.world);
            let moved = st.body.tick(&world, &Controls { sprint: want_sprint, ..*controls });
            st.world = world;
            if moved {
                let b = &st.body;
                let cur = (b.pos, b.yaw, b.pitch, b.on_ground);
                let flags = (b.on_ground as u8) | ((b.horizontal_collision as u8) << 1);
                st.ticks_since_pos += 1;
                let (pos_changed, rot_changed) = match st.last_sent {
                    None => (true, true),
                    Some((p, y, pi, _)) => {
                        let d2 = (p[0] - cur.0[0]).powi(2) + (p[1] - cur.0[1]).powi(2) + (p[2] - cur.0[2]).powi(2);
                        (d2 > 4e-8 || st.ticks_since_pos >= 20, y != cur.1 || pi != cur.2)
                    }
                };
                let ground_changed = st.last_sent.map(|l| l.3 != cur.3).unwrap_or(true);
                let pkt = if pos_changed && rot_changed {
                    let mut w = Writer::with_id(self.ids.p_move_pos_rot);
                    w.f64(cur.0[0]).f64(cur.0[1]).f64(cur.0[2]).f32(cur.1).f32(cur.2).u8(flags);
                    Some(w)
                } else if pos_changed {
                    let mut w = Writer::with_id(self.ids.p_move_pos);
                    w.f64(cur.0[0]).f64(cur.0[1]).f64(cur.0[2]).u8(flags);
                    Some(w)
                } else if rot_changed {
                    let mut w = Writer::with_id(self.ids.p_move_rot);
                    w.f32(cur.1).f32(cur.2).u8(flags);
                    Some(w)
                } else if ground_changed {
                    let mut w = Writer::with_id(self.ids.p_move_status);
                    w.u8(flags);
                    Some(w)
                } else {
                    None
                };
                if pos_changed {
                    st.ticks_since_pos = 0;
                }
                if let Some(p) = pkt {
                    packets.push(p);
                    st.last_sent = Some(cur);
                }
                let (cx, cz) = ((st.body.pos[0].floor() as i32) >> 4, (st.body.pos[2].floor() as i32) >> 4);
                let radius = (st.server.view_distance.max(2) + 2).min(34);
                st.world.prune(cx, cz, radius);
            }
        }
        for p in packets {
            self.send(p);
        }
        self.send(Writer::with_id(self.ids.p_tick_end));
    }

    pub fn set_rotation(&self, yaw: f32, pitch: f32) {
        let mut st = self.state.lock().unwrap();
        st.body.yaw = yaw;
        st.body.pitch = pitch.clamp(-90.0, 90.0);
    }

    // ───────────── world interaction ─────────────

    /// Player action (26.x numbering): 0 start dig, 1 change dig direction, 2 cancel, 3 finish,
    /// 4 drop stack, 5 drop item, 6 release use item, 7 swap hands, 8 stab.
    pub fn player_action(&self, status: i32, pos: (i32, i32, i32), face: u8) -> i32 {
        let seq = self.next_seq();
        let mut w = Writer::with_id(self.ids.p_player_action);
        w.varint(status).position(pos.0, pos.1, pos.2).u8(face).varint(seq);
        self.send(w);
        seq
    }

    pub fn start_digging(&self, pos: (i32, i32, i32), face: u8) -> i32 {
        self.player_action(0, pos, face)
    }
    pub fn cancel_digging(&self, pos: (i32, i32, i32), face: u8) -> i32 {
        self.player_action(2, pos, face)
    }
    pub fn finish_digging(&self, pos: (i32, i32, i32), face: u8) -> i32 {
        self.player_action(3, pos, face)
    }

    /// Arm swing (main hand). Other players see this animation.
    pub fn swing(&self) {
        {
            let mut st = self.state.lock().unwrap();
            st.last_swing_tick = st.ticks;
        }
        self.send(Writer::with_id(self.ids.p_punch));
    }

    /// Right-click a block face (place block / open container / use).
    pub fn use_item_on(&self, pos: (i32, i32, i32), face: u8, cursor: [f32; 3]) -> i32 {
        let seq = self.next_seq();
        let mut w = Writer::with_id(self.ids.p_use_item_on);
        w.varint(0).position(pos.0, pos.1, pos.2).varint(face as i32).f32(cursor[0]).f32(cursor[1]).f32(cursor[2]);
        w.bool(false).bool(false).varint(seq);
        self.send(w);
        seq
    }

    /// Right-click with the held item (eat, drink, throw...).
    pub fn use_item(&self) -> i32 {
        let seq = self.next_seq();
        let (yaw, pitch) = {
            let st = self.state.lock().unwrap();
            (st.body.yaw, st.body.pitch)
        };
        let mut w = Writer::with_id(self.ids.p_use_item);
        w.varint(0).varint(seq).f32(yaw).f32(pitch);
        self.send(w);
        seq
    }

    pub fn release_use_item(&self) {
        self.player_action(6, (0, 0, 0), 0);
    }

    pub fn attack(&self, entity_id: i32) {
        let mut w = Writer::with_id(self.ids.p_attack);
        w.varint(entity_id);
        self.send(w);
        self.swing();
    }

    pub fn interact_entity(&self, entity_id: i32, at: [f64; 3], sneaking: bool) {
        let mut w = Writer::with_id(self.ids.p_interact_entity);
        w.varint(entity_id).varint(0).lp_vec3(at[0], at[1], at[2]).bool(sneaking);
        self.send(w);
    }

    pub fn set_hotbar(&self, slot: u8) {
        let slot = slot.min(8);
        self.state.lock().unwrap().inventory.selected_hotbar = slot;
        let mut w = Writer::with_id(self.ids.p_set_held_item);
        w.i16(slot as i16);
        self.send(w);
    }

    /// Container click. mode: 0 pickup, 1 quick move (shift), 2 swap (button=hotbar), 4 throw.
    pub fn click_slot(&self, window: i32, slot: i16, button: i8, mode: i32) {
        let (state_id, carried) = {
            let st = self.state.lock().unwrap();
            (st.inventory.state_id, st.inventory.cursor.clone())
        };
        let mut w = Writer::with_id(self.ids.p_click_container);
        w.varint(window).varint(state_id).i16(slot).i8(button).varint(mode);
        w.varint(0); // changed slots: let the server resync
        inventory::write_hashed(&mut w, Some(&carried));
        self.send(w);
    }

    pub fn close_window(&self, window: i32) {
        let mut w = Writer::with_id(self.ids.p_close_container);
        w.varint(window);
        self.send(w);
        let mut st = self.state.lock().unwrap();
        if st.inventory.open.as_ref().map(|o| o.id) == Some(window) {
            st.inventory.open = None;
        }
    }

    pub fn respawn(&self) {
        let mut w = Writer::with_id(self.ids.p_client_status);
        w.varint(0);
        self.send(w);
    }

    pub fn drop_held(&self, whole_stack: bool) {
        self.player_action(if whole_stack { 4 } else { 5 }, (0, 0, 0), 0);
    }
}

fn rsa_encrypt(der: &[u8], secret: &[u8], token: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
    let key = RsaPublicKey::from_public_key_der(der).context("parsing server public key")?;
    let mut rng = rand::thread_rng();
    Ok((key.encrypt(&mut rng, Pkcs1v15Encrypt, secret)?, key.encrypt(&mut rng, Pkcs1v15Encrypt, token)?))
}

async fn send_client_information(w: &mut PacketWriter<OwnedWriteHalf>, _ids: &PacketIds, cfg: &ClientConfig, id: i32) -> Result<()> {
    w.write_packet(&client_information(cfg, id).buf).await
}

fn client_information(cfg: &ClientConfig, id: i32) -> Writer {
    let mut p = Writer::with_id(id);
    p.string(&cfg.locale)
        .u8(cfg.view_distance)
        .varint(0) // chat mode: full
        .bool(true) // chat colors
        .u8(0x7F) // skin parts
        .varint(1) // main hand: right
        .bool(false) // text filtering
        .bool(true) // server listing
        .varint(0); // particles: all
    p
}

fn default_dimension(name: &str) -> Dimension {
    match name {
        "minecraft:the_nether" | "minecraft:the_end" => Dimension { name: name.into(), min_y: 0, height: 256 },
        _ => Dimension { name: name.into(), min_y: -64, height: 384 },
    }
}

fn handle_registry_data(r: &mut Reader, state: &Arc<Mutex<GameState>>) -> Result<()> {
    let registry = r.string()?;
    let n = r.varint()?;
    let mut entries = Vec::new();
    for _ in 0..n {
        let id = r.string()?;
        let data = if r.bool()? { Some(Nbt::read_network(r)?) } else { None };
        entries.push((id, data));
    }
    let mut st = state.lock().unwrap();
    match registry.as_str() {
        "minecraft:dimension_type" => {
            st.dimension_types = entries
                .iter()
                .map(|(id, nbt)| {
                    let mut d = default_dimension(id);
                    if let Some(n) = nbt {
                        if let Some(v) = n.get("min_y").and_then(|v| v.as_i64()) {
                            d.min_y = v as i32;
                        }
                        if let Some(v) = n.get("height").and_then(|v| v.as_i64()) {
                            d.height = v as i32;
                        }
                    }
                    d
                })
                .collect();
        }
        "minecraft:worldgen/biome" => {
            st.world.biome_names = entries.iter().map(|(id, _)| id.trim_start_matches("minecraft:").to_string()).collect();
        }
        _ => {}
    }
    Ok(())
}

async fn configuration_loop(
    conn: &mut Conn,
    ids: &PacketIds,
    state: &Arc<Mutex<GameState>>,
    ev: &mpsc::UnboundedSender<Event>,
    cfg: &ClientConfig,
) -> Result<()> {
    loop {
        let pkt = tokio::time::timeout(Duration::from_secs(60), conn.r.read_packet())
            .await
            .map_err(|_| anyhow!("configuration timed out"))??;
        let mut r = Reader::new(&pkt);
        let id = r.varint()?;
        let reply = handle_configuration_packet(id, &mut r, ids, state, cfg)?;
        if let Some(reason) = reply.disconnect {
            let _ = ev.send(Event::Disconnected { reason: reason.clone() });
            bail!("disconnected during configuration: {reason}");
        }
        for p in reply.packets {
            conn.w.write_packet(&p.buf).await?;
        }
        if reply.finished {
            return Ok(());
        }
    }
}

#[derive(Default)]
struct ConfigReply {
    packets: Vec<Writer>,
    finished: bool,
    disconnect: Option<String>,
}

fn handle_configuration_packet(
    id: i32,
    r: &mut Reader,
    ids: &PacketIds,
    state: &Arc<Mutex<GameState>>,
    cfg: &ClientConfig,
) -> Result<ConfigReply> {
    let mut out = ConfigReply::default();
    if id == ids.sc_known_packs {
        let n = r.varint()?;
        let mut known = Writer::with_id(ids.c_known_packs);
        let mut entries = vec![];
        for _ in 0..n {
            let (ns, pid, ver) = (r.string()?, r.string()?, r.string()?);
            // OmniCore embeds the vanilla core data for its version, so it knows that pack.
            if ns == "minecraft" && pid == "core" && ver == cfg.adapter.name {
                entries.push((ns, pid, ver));
            }
        }
        known.varint(entries.len() as i32);
        for (a, b, c) in entries {
            known.string(&a).string(&b).string(&c);
        }
        out.packets.push(known);
    } else if id == ids.sc_registry_data {
        handle_registry_data(r, state)?;
    } else if id == ids.sc_keep_alive {
        let v = r.i64()?;
        let mut w = Writer::with_id(ids.c_keep_alive);
        w.i64(v);
        out.packets.push(w);
    } else if id == ids.sc_ping {
        let v = r.i32()?;
        let mut w = Writer::with_id(ids.c_pong);
        w.i32(v);
        out.packets.push(w);
    } else if id == ids.sc_code_of_conduct {
        let _text = r.string()?;
        out.packets.push(Writer::with_id(ids.c_accept_code_of_conduct));
    } else if id == ids.sc_resource_pack_push {
        let uuid = r.uuid()?;
        for status in [3, 4, 0] {
            let mut w = Writer::with_id(ids.c_resource_pack);
            w.uuid(&uuid).varint(status);
            out.packets.push(w);
        }
    } else if id == ids.sc_cookie_request {
        let key = r.string()?;
        let mut w = Writer::with_id(ids.c_cookie_response);
        w.string(&key).bool(false);
        out.packets.push(w);
    } else if id == ids.sc_plugin_message {
        let channel = r.string()?;
        if channel == "minecraft:brand" {
            if let Ok(b) = r.string() {
                state.lock().unwrap().server.brand = Some(b);
            }
        }
    } else if id == ids.sc_disconnect {
        out.disconnect = Some(nbt_to_plain(&Nbt::read_network(r)?));
    } else if id == ids.sc_finish_configuration {
        out.packets.push(Writer::with_id(ids.c_finish_configuration));
        out.finished = true;
    }
    // Other configuration packets (tags, feature flags, links, dialogs...) need no reply.
    Ok(out)
}

async fn writer_task(mut w: PacketWriter<OwnedWriteHalf>, mut rx: mpsc::UnboundedReceiver<Vec<u8>>, closed: Arc<AtomicBool>) {
    while let Some(pkt) = rx.recv().await {
        if pkt.is_empty() {
            break;
        }
        if w.write_packet(&pkt).await.is_err() {
            break;
        }
    }
    closed.store(true, Ordering::SeqCst);
}

async fn reader_task(mut r: PacketReader<OwnedReadHalf>, client: Client, ev: mpsc::UnboundedSender<Event>, cfg: ClientConfig) {
    let reason = loop {
        if client.is_closed() {
            break "client disconnected".to_string();
        }
        let pkt = match tokio::time::timeout(Duration::from_secs(45), r.read_packet()).await {
            Err(_) => break "timed out (no packets for 45s)".to_string(),
            Ok(Err(e)) => break format!("{e}"),
            Ok(Ok(p)) => p,
        };
        let mut rd = Reader::new(&pkt);
        let id = match rd.varint() {
            Ok(i) => i,
            Err(e) => break format!("bad packet: {e}"),
        };
        if client.phase() == Phase::Configuration {
            match handle_configuration_packet(id, &mut rd, &client.ids, &client.state, &cfg) {
                Ok(reply) => {
                    if let Some(reason) = reply.disconnect {
                        break reason;
                    }
                    for p in reply.packets {
                        client.send(p);
                    }
                    if reply.finished {
                        client.phase.store(Phase::Play as u8, Ordering::SeqCst);
                    }
                }
                Err(e) => break format!("configuration error: {e}"),
            }
            continue;
        }
        match handle_play_packet(id, &mut rd, &client, &ev, &cfg) {
            Ok(Some(reason)) => break reason,
            Ok(None) => {}
            Err(e) => {
                // A malformed/unsupported packet should not kill the session.
                let _ = e;
            }
        }
    };
    client.closed.store(true, Ordering::SeqCst);
    client.phase.store(Phase::Closed as u8, Ordering::SeqCst);
    let _ = client.tx.send(Vec::new());
    let _ = ev.send(Event::Disconnected { reason });
}

fn read_spawn_info(r: &mut Reader, st: &mut GameState) -> Result<String> {
    let dim_type = r.varint()?;
    let world_name = r.string()?;
    let _seed = r.i64()?;
    let gamemode = r.varint()?;
    let _prev = r.varint()?;
    let _debug = r.bool()?;
    let _flat = r.bool()?;
    if r.bool()? {
        r.string()?;
        r.i64()?;
    }
    let _portal = r.varint()?;
    let _sea = r.varint()?;
    st.gamemode = gamemode;
    let mut dim = st.dimension_types.get(dim_type as usize).cloned().unwrap_or_else(|| default_dimension(&world_name));
    dim.name = world_name.clone();
    st.world.dimension = dim;
    Ok(world_name)
}

/// Returns Some(reason) when the server disconnected us.
fn handle_play_packet(id: i32, r: &mut Reader, c: &Client, ev: &mpsc::UnboundedSender<Event>, cfg: &ClientConfig) -> Result<Option<String>> {
    let ids = &*c.ids;
    let emit = |e: Event| {
        let _ = ev.send(e);
    };
    let mut st = c.state.lock().unwrap();
    st.packets_received += 1;

    if id == ids.s_keep_alive {
        let v = r.i64()?;
        drop(st);
        let mut w = Writer::with_id(ids.p_keep_alive);
        w.i64(v);
        c.send(w);
    } else if id == ids.s_ping {
        let v = r.i32()?;
        drop(st);
        let mut w = Writer::with_id(ids.p_pong);
        w.i32(v);
        c.send(w);
    } else if id == ids.s_login {
        st.entity_id = r.i32()?;
        st.server.hardcore = r.bool()?;
        let worlds = r.varint()?;
        for _ in 0..worlds {
            r.string()?;
        }
        st.server.max_players = r.varint()?;
        st.server.view_distance = r.varint()?;
        let _sim = r.varint()?;
        let _reduced = r.bool()?;
        let _respawn_screen = r.bool()?;
        let _limited = r.bool()?;
        let dim = read_spawn_info(r, &mut st)?;
        st.server.online_mode = r.bool().unwrap_or(false);
        st.server.enforces_secure_chat = r.bool().unwrap_or(false);
        let (eid, gm) = (st.entity_id, st.gamemode);
        drop(st);
        emit(Event::Joined { entity_id: eid, gamemode: gm, dimension: dim });
        // Re-send client information in play so view distance applies.
        c.send(client_information(cfg, ids.p_client_information));
    } else if id == ids.s_respawn {
        let old = st.world.dimension.name.clone();
        let dim = read_spawn_info(r, &mut st)?;
        if old != dim {
            st.world.chunks.clear();
        }
        st.entities.clear();
        st.dead = false;
        st.spawned = false;
        st.health = 20.0;
        drop(st);
        emit(Event::Respawned { dimension: dim });
    } else if id == ids.s_player_position {
        let tid = r.varint()?;
        let (x, y, z) = (r.f64()?, r.f64()?, r.f64()?);
        let (dx, dy, dz) = (r.f64()?, r.f64()?, r.f64()?);
        let (yaw, pitch) = (r.f32()?, r.f32()?);
        let flags = r.i32()?;
        let b = &mut st.body;
        b.pos[0] = if flags & 1 != 0 { b.pos[0] + x } else { x };
        b.pos[1] = if flags & 2 != 0 { b.pos[1] + y } else { y };
        b.pos[2] = if flags & 4 != 0 { b.pos[2] + z } else { z };
        b.yaw = if flags & 8 != 0 { b.yaw + yaw } else { yaw };
        b.pitch = if flags & 16 != 0 { b.pitch + pitch } else { pitch };
        b.vel[0] = if flags & 32 != 0 { b.vel[0] + dx } else { dx };
        b.vel[1] = if flags & 64 != 0 { b.vel[1] + dy } else { dy };
        b.vel[2] = if flags & 128 != 0 { b.vel[2] + dz } else { dz };
        b.fall_distance = 0.0;
        let (pos, yaw, pitch, ground) = (b.pos, b.yaw, b.pitch, b.on_ground);
        let first = !st.spawned;
        st.spawned = true;
        st.last_sent = Some((pos, yaw, pitch, ground));
        st.ticks_since_pos = 0;
        let send_loaded = !st.loaded_sent;
        st.loaded_sent = true;
        drop(st);
        let mut w = Writer::with_id(ids.p_teleport_confirm);
        w.varint(tid).f64(pos[0]).f64(pos[1]).f64(pos[2]).f32(yaw).f32(pitch);
        c.send(w);
        let mut w = Writer::with_id(ids.p_move_pos_rot);
        w.f64(pos[0]).f64(pos[1]).f64(pos[2]).f32(yaw).f32(pitch).u8(ground as u8);
        c.send(w);
        if send_loaded {
            c.send(Writer::with_id(ids.p_player_loaded));
        }
        emit(if first { Event::Spawned { pos } } else { Event::Teleported { pos } });
    } else if id == ids.s_player_rotation {
        let yaw = r.f32()?;
        let ry = r.bool()?;
        let pitch = r.f32()?;
        let rp = r.bool()?;
        st.body.yaw = if ry { st.body.yaw + yaw } else { yaw };
        st.body.pitch = if rp { st.body.pitch + pitch } else { pitch };
    } else if id == ids.s_chunk_data {
        let cx = r.i32()?;
        let cz = r.i32()?;
        st.world.load_chunk(cx, cz, r)?;
        st.chunks_received += 1;
        drop(st);
        emit(Event::ChunkLoaded { x: cx, z: cz });
    } else if id == ids.s_unload_chunk {
        let cz = r.i32()?;
        let cx = r.i32()?;
        st.world.unload_chunk(cx, cz);
    } else if id == ids.s_chunk_batch_finished {
        st.chunk_batches += 1;
        drop(st);
        let mut w = Writer::with_id(ids.p_chunk_batch_received);
        w.f32(20.0);
        c.send(w);
    } else if id == ids.s_block_update {
        let (x, y, z) = r.position_packed()?;
        let state = r.varint()? as u32;
        st.world.set(x, y, z, state);
        drop(st);
        emit(Event::BlockChanged { pos: (x, y, z), state, name: GAME.block_name(state).to_string() });
    } else if id == ids.s_section_blocks_update {
        let sp = r.i64()?;
        let sx = (sp >> 42) as i32;
        let sy = ((sp << 44) >> 44) as i32;
        let sz = ((sp << 22) >> 42) as i32;
        let n = r.varint()?;
        let mut changed = vec![];
        for _ in 0..n {
            let v = r.varlong()?;
            let state = (v >> 12) as u32;
            let lx = ((v >> 8) & 15) as i32;
            let lz = ((v >> 4) & 15) as i32;
            let ly = (v & 15) as i32;
            let (x, y, z) = (sx * 16 + lx, sy * 16 + ly, sz * 16 + lz);
            st.world.set(x, y, z, state);
            changed.push(((x, y, z), state));
        }
        drop(st);
        for (pos, state) in changed {
            emit(Event::BlockChanged { pos, state, name: GAME.block_name(state).to_string() });
        }
    } else if id == ids.s_ack_block_change {
        st.last_acked_sequence = r.varint()?;
    } else if id == ids.s_spawn_entity {
        let eid = r.varint()?;
        let uuid = r.uuid()?;
        let kind = r.varint()? as u32;
        let pos = [r.f64()?, r.f64()?, r.f64()?];
        let vel = r.lp_vec3()?;
        let pitch = r.i8()? as f32 * 360.0 / 256.0;
        let yaw = r.i8()? as f32 * 360.0 / 256.0;
        let head = r.i8()? as f32 * 360.0 / 256.0;
        let mut e = Entity::new(eid, uuid, kind, pos);
        e.vel = [vel.0, vel.1, vel.2];
        e.yaw = yaw;
        e.pitch = pitch;
        e.head_yaw = head;
        e.username = st.player_name(&uuid).map(String::from);
        let ev_e = Event::EntitySpawned { id: eid, kind: e.kind_name.clone(), pos, username: e.username.clone() };
        st.entities.insert(eid, e);
        drop(st);
        emit(ev_e);
    } else if id == ids.s_move_entity_pos || id == ids.s_move_entity_pos_rot {
        let eid = r.varint()?;
        let props = r.varint()?;
        let steps = props >> 1;
        let on_ground = props & 1 != 0;
        let mut d = [0.0f64; 3];
        if steps <= 0 {
            d = [r.i16()? as f64 / 4096.0, r.i16()? as f64 / 4096.0, r.i16()? as f64 / 4096.0];
        } else {
            for _ in 0..steps {
                r.varint()?;
                d[0] += r.i16()? as f64 / 4096.0;
                d[1] += r.i16()? as f64 / 4096.0;
                d[2] += r.i16()? as f64 / 4096.0;
            }
        }
        let rot = if id == ids.s_move_entity_pos_rot {
            Some((r.i8()? as f32 * 360.0 / 256.0, r.i8()? as f32 * 360.0 / 256.0))
        } else {
            None
        };
        if let Some(e) = st.entities.get_mut(&eid) {
            for i in 0..3 {
                e.pos[i] += d[i];
            }
            e.on_ground = on_ground;
            if let Some((y, p)) = rot {
                e.yaw = y;
                e.pitch = p;
            }
        }
    } else if id == ids.s_move_entity_rot {
        let eid = r.varint()?;
        let on_ground = r.bool()?;
        let yaw = r.i8()? as f32 * 360.0 / 256.0;
        let pitch = r.i8()? as f32 * 360.0 / 256.0;
        if let Some(e) = st.entities.get_mut(&eid) {
            e.yaw = yaw;
            e.pitch = pitch;
            e.on_ground = on_ground;
        }
    } else if id == ids.s_teleport_entity {
        let eid = r.varint()?;
        let p = [r.f64()?, r.f64()?, r.f64()?];
        let _d = [r.f64()?, r.f64()?, r.f64()?];
        let (yaw, pitch) = (r.f32()?, r.f32()?);
        let flags = r.i32()?;
        let on_ground = r.bool()?;
        if let Some(e) = st.entities.get_mut(&eid) {
            for i in 0..3 {
                e.pos[i] = if flags & (1 << i) != 0 { e.pos[i] + p[i] } else { p[i] };
            }
            e.yaw = if flags & 8 != 0 { e.yaw + yaw } else { yaw };
            e.pitch = if flags & 16 != 0 { e.pitch + pitch } else { pitch };
            e.on_ground = on_ground;
        }
    } else if id == ids.s_entity_position_sync {
        let eid = r.varint()?;
        let kind = r.varint()?;
        let end = if kind == 0 {
            [r.f64()?, r.f64()?, r.f64()?]
        } else {
            let n = r.varint()?;
            let mut last = [0.0; 3];
            for _ in 0..n {
                last = [r.f64()?, r.f64()?, r.f64()?];
                r.varint()?;
            }
            last
        };
        let (yaw, pitch) = (r.f32()?, r.f32()?);
        let on_ground = r.bool()?;
        if let Some(e) = st.entities.get_mut(&eid) {
            e.pos = end;
            e.yaw = yaw;
            e.pitch = pitch;
            e.on_ground = on_ground;
        }
    } else if id == ids.s_set_entity_velocity {
        let eid = r.varint()?;
        let v = r.lp_vec3()?;
        let me = st.entity_id;
        if eid == me {
            // Knockback / explosions applied to us.
            st.body.vel = [v.0, v.1, v.2];
        } else if let Some(e) = st.entities.get_mut(&eid) {
            e.vel = [v.0, v.1, v.2];
        }
    } else if id == ids.s_head_rotation {
        let eid = r.varint()?;
        let y = r.i8()? as f32 * 360.0 / 256.0;
        if let Some(e) = st.entities.get_mut(&eid) {
            e.head_yaw = y;
        }
    } else if id == ids.s_remove_entities {
        let n = r.varint()?;
        let mut removed = vec![];
        for _ in 0..n {
            let eid = r.varint()?;
            st.entities.remove(&eid);
            removed.push(eid);
        }
        drop(st);
        for eid in removed {
            emit(Event::EntityRemoved { id: eid });
        }
    } else if id == ids.s_set_health {
        let health = r.f32()?;
        let food = r.varint()?;
        let sat = r.f32()?;
        st.health = health;
        st.food = food;
        st.saturation = sat;
        let died = health <= 0.0 && !st.dead;
        if died {
            st.dead = true;
        }
        drop(st);
        emit(Event::Health { health, food, saturation: sat });
    } else if id == ids.s_player_death {
        let eid = r.varint()?;
        let msg = nbt_to_plain(&Nbt::read_network(r)?);
        if eid == st.entity_id {
            st.dead = true;
            drop(st);
            emit(Event::Died { message: msg });
        }
    } else if id == ids.s_set_experience {
        let _bar = r.f32()?;
        st.xp_level = r.varint()?;
    } else if id == ids.s_damage_event {
        let target = r.varint()?;
        let ty = r.varint()?;
        let src = r.varint()?;
        let _direct = r.varint()?;
        let tick = st.ticks;
        if let Some(e) = st.entities.get_mut(&target) {
            e.last_hurt_tick = tick;
        }
        if target == st.entity_id {
            drop(st);
            emit(Event::Hurt { source_entity: if src > 0 { Some(src - 1) } else { None }, damage_type: ty });
        }
    } else if id == ids.s_container_content {
        let window = r.varint()?;
        let state_id = r.varint()?;
        let n = r.varint()?;
        let mut items = Vec::with_capacity(n as usize);
        let mut partial = false;
        for _ in 0..n {
            match inventory::read_slot(r) {
                Ok(s) => items.push(s),
                Err(_) => {
                    partial = true;
                    break;
                }
            }
        }
        let carried = if partial { None } else { inventory::read_slot(r).ok() };
        let inv = &mut st.inventory;
        inv.state_id = state_id;
        inv.desynced = partial;
        if window == 0 {
            for (i, s) in items.into_iter().enumerate().take(inventory::slots::TOTAL) {
                inv.slots[i] = s;
            }
        } else if let Some(open) = inv.open.as_mut().filter(|o| o.id == window) {
            // The last 36 slots of any container window mirror the player inventory.
            let len = items.len();
            open.slots = items.clone();
            if len >= 36 {
                for (k, s) in items[len - 36..].iter().enumerate() {
                    inv.slots[inventory::slots::MAIN_START + k] = s.clone();
                }
            }
        }
        if let Some(cur) = carried {
            inv.cursor = cur;
        }
        drop(st);
        emit(Event::InventoryChanged);
    } else if id == ids.s_container_slot {
        let window = r.varint()?;
        let state_id = r.varint()?;
        let slot = r.i16()?;
        let item = inventory::read_slot(r)?;
        let inv = &mut st.inventory;
        inv.state_id = state_id;
        if window == 0 && slot >= 0 && (slot as usize) < inventory::slots::TOTAL {
            inv.slots[slot as usize] = item;
        } else if window == -1 && slot == -1 {
            inv.cursor = item;
        } else if let Some(open) = inv.open.as_mut().filter(|o| o.id == window) {
            let len = open.slots.len();
            if (slot as usize) < len {
                open.slots[slot as usize] = item.clone();
                if len >= 36 && slot as usize >= len - 36 {
                    inv.slots[inventory::slots::MAIN_START + slot as usize - (len - 36)] = item;
                }
            }
        }
        drop(st);
        emit(Event::InventoryChanged);
    } else if id == ids.s_set_player_inventory_slot {
        let idx = r.varint()? as usize;
        let item = inventory::read_slot(r)?;
        if let Some(s) = inventory::inventory_index_to_slot(idx) {
            st.inventory.slots[s] = item;
        }
        drop(st);
        emit(Event::InventoryChanged);
    } else if id == ids.s_set_cursor_item {
        st.inventory.cursor = inventory::read_slot(r)?;
    } else if id == ids.s_set_held_item {
        let slot = r.varint()?;
        st.inventory.selected_hotbar = slot.clamp(0, 8) as u8;
    } else if id == ids.s_open_screen {
        let wid = r.varint()?;
        let kind = r.varint()?;
        let title = nbt_to_plain(&Nbt::read_network(r)?);
        st.inventory.open = Some(Window { id: wid, kind, title: title.clone(), slots: vec![] });
        drop(st);
        emit(Event::WindowOpened { id: wid, kind, title });
    } else if id == ids.s_close_container {
        let wid = r.varint()?;
        st.inventory.open = None;
        drop(st);
        emit(Event::WindowClosed { id: wid });
    } else if id == ids.s_take_item_entity {
        let _collected = r.varint()?;
        let collector = r.varint()?;
        let count = r.varint()?;
        drop(st);
        emit(Event::ItemPickedUp { collector, count });
    } else if id == ids.s_player_info_update {
        let actions = r.u8()?;
        let n = r.varint()?;
        let mut joined = vec![];
        for _ in 0..n {
            let uuid = r.uuid()?;
            let mut name = st.players.get(&uuid).map(|p| p.name.clone()).unwrap_or_default();
            let mut gm = st.players.get(&uuid).map(|p| p.gamemode).unwrap_or(0);
            let mut lat = 0;
            let mut chat_session = st.players.get(&uuid).map(|p| p.has_chat_session).unwrap_or(false);
            if actions & 1 != 0 {
                name = r.string()?;
                let props = r.varint()?;
                for _ in 0..props {
                    r.string()?;
                    r.string()?;
                    if r.bool()? {
                        r.string()?;
                    }
                }
            }
            if actions & 2 != 0 && r.bool()? {
                r.uuid()?;
                r.i64()?;
                r.byte_array()?;
                r.byte_array()?;
                chat_session = true;
            }
            if actions & 4 != 0 {
                gm = r.varint()?;
            }
            if actions & 8 != 0 {
                r.bool()?;
            }
            if actions & 16 != 0 {
                lat = r.varint()?;
            }
            if actions & 32 != 0 && r.bool()? {
                Nbt::read_network(r)?;
            }
            if actions & 64 != 0 {
                r.varint()?;
            }
            if actions & 128 != 0 {
                r.bool()?;
            }
            let is_new = !st.players.contains_key(&uuid);
            st.players.insert(uuid, TabPlayer { uuid, name: name.clone(), gamemode: gm, latency: lat, has_chat_session: chat_session });
            for e in st.entities.values_mut() {
                if e.uuid == uuid {
                    e.username = Some(name.clone());
                }
            }
            if is_new {
                joined.push((uuid, name));
            }
        }
        drop(st);
        for (u, n) in joined {
            emit(Event::PlayerJoined { uuid: u.to_string(), name: n });
        }
    } else if id == ids.s_player_info_remove {
        let n = r.varint()?;
        let mut left = vec![];
        for _ in 0..n {
            let u = r.uuid()?;
            if let Some(p) = st.players.remove(&u) {
                left.push((u, p.name));
            }
        }
        drop(st);
        for (u, n) in left {
            emit(Event::PlayerLeft { uuid: u.to_string(), name: n });
        }
    } else if id == ids.s_system_chat {
        let comp = Nbt::read_network(r)?;
        let overlay = r.bool().unwrap_or(false);
        drop(st);
        let text = nbt_to_plain(&comp);
        if !overlay {
            // Vanilla-format chat ("<name> msg") also arrives as system chat on some servers.
            let (sender, message) = split_angle_chat(&text);
            emit(Event::Chat { sender, sender_uuid: None, message, raw: text, kind: "system".into() });
        }
    } else if id == ids.s_player_chat {
        let _global = r.varint()?;
        let sender = r.uuid()?;
        let _index = r.varint()?;
        let sig = if r.bool()? { Some(r.bytes(256)?.to_vec()) } else { None };
        let content = r.string()?;
        let _ts = r.i64()?;
        let _salt = r.i64()?;
        let seen = r.varint()?;
        for _ in 0..seen {
            let v = r.varint()?;
            if v == 0 {
                r.bytes(256)?;
            }
        }
        let unsigned = r.optional(|r| Nbt::read_network(r))?;
        let filter = r.varint()?;
        if filter == 2 {
            r.bitset()?;
        }
        let _chat_type = r.varint()?;
        let name_comp = Nbt::read_network(r)?;
        let name = nbt_to_plain(&name_comp);
        if let Some(s) = sig {
            let mut arr = [0u8; 256];
            arr.copy_from_slice(&s);
            st.chat.recent.push_back(arr);
            while st.chat.recent.len() > 20 {
                st.chat.recent.pop_front();
            }
            st.chat.pending += 1;
            st.chat.total += 1;
        }
        let pending = st.chat.pending;
        let sender_name = st.player_name(&sender).map(String::from).unwrap_or(name);
        drop(st);
        if pending > 64 {
            let mut w = Writer::with_id(ids.p_chat_ack);
            w.varint(pending);
            c.send(w);
            c.state.lock().unwrap().chat.pending = 0;
        }
        let msg = unsigned.map(|u| nbt_to_plain(&u)).unwrap_or(content);
        emit(Event::Chat {
            sender: Some(sender_name.clone()),
            sender_uuid: Some(sender.to_string()),
            message: msg.clone(),
            raw: format!("<{sender_name}> {msg}"),
            kind: "player".into(),
        });
    } else if id == ids.s_disguised_chat {
        let msg = nbt_to_plain(&Nbt::read_network(r)?);
        let _ty = r.varint()?;
        let name = nbt_to_plain(&Nbt::read_network(r)?);
        drop(st);
        emit(Event::Chat { sender: Some(name.clone()), sender_uuid: None, raw: format!("<{name}> {msg}"), message: msg, kind: "disguised".into() });
    } else if id == ids.s_disconnect {
        let reason = nbt_to_plain(&Nbt::read_network(r)?);
        return Ok(Some(reason));
    } else if id == ids.s_start_configuration {
        drop(st);
        c.phase.store(Phase::Configuration as u8, Ordering::SeqCst);
        c.tx.send(Writer::with_id(ids.p_configuration_ack).buf).ok();
        emit(Event::ConfigurationStarted);
    } else if id == ids.s_game_event {
        let reason = r.u8()?;
        let value = r.f32()?;
        match reason {
            1 => st.raining = false,
            2 => st.raining = true,
            3 => st.gamemode = value as i32,
            _ => {}
        }
        let raining = st.raining;
        drop(st);
        if reason == 1 || reason == 2 {
            emit(Event::Weather { raining });
        }
    } else if id == ids.s_set_time {
        let _game_time = r.i64()?;
        let n = r.varint()?;
        let mut day = None;
        for _ in 0..n {
            let _clock = r.varint()?;
            let total = r.varlong()?;
            r.f32()?;
            r.f32()?;
            if day.is_none() {
                day = Some(total);
            }
        }
        if let Some(d) = day {
            st.day_time = d.rem_euclid(24000);
        }
    } else if id == ids.s_resource_pack_push {
        let uuid = r.uuid()?;
        drop(st);
        for s in [3, 4, 0] {
            let mut w = Writer::with_id(ids.p_resource_pack);
            w.uuid(&uuid).varint(s);
            c.send(w);
        }
    } else if id == ids.s_cookie_request {
        let key = r.string()?;
        drop(st);
        let mut w = Writer::with_id(ids.p_cookie_response);
        w.string(&key).bool(false);
        c.send(w);
    }
    Ok(None)
}

/// Splits "<Name> message" into (Some(Name), message).
pub fn split_angle_chat(text: &str) -> (Option<String>, String) {
    if let Some(rest) = text.strip_prefix('<') {
        if let Some(end) = rest.find("> ") {
            return (Some(rest[..end].to_string()), rest[end + 2..].to_string());
        }
    }
    (None, text.to_string())
}

/// Reports a held item for convenience in event payloads.
pub fn describe_item(s: &ItemStack) -> String {
    if s.is_empty() {
        "nothing".into()
    } else {
        format!("{} x{}", s.name(), s.count)
    }
}
