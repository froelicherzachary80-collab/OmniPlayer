# Architecture

```
iPhone / browser ──HTTPS/WS──▶ API (apps/api, Fastify + SQLite) ──serves──▶ web app (dashboard + MineLive)
                                  │
     ┌────────────────────────────┼──────────────────────────────┬───────────────────────┐
     ▼                            ▼                              ▼                       ▼
Server manager            Bot supervisor                  Stream manager           VoiceBox client
(java -jar server.jar)    one omniplayer-bot process       MediaMTX process          (HTTP → voicebox.py)
     │                    per creator (stdin/stdout JSON)  WHEP/HLS/playback proxy
     ▼                            │
Real Minecraft server ◀──TCP── OmniCore client ── world state ── OmniView renderer ──raw RGB──▶ FFmpeg ──RTSP──▶ MediaMTX ──WebRTC/HLS──▶ viewers
                                  │
                              OmniBrain (planner, skills, chat, commentary) ──commentary──▶ API ──▶ VoiceBox ──PCM──▶ bot audio pipe ──▶ FFmpeg
```

## Processes and interfaces
- **API ↔ bot**: newline-delimited JSON. Commands on stdin (`chat`, `goto`, `gather`, `craft`, `mode`, `stream_start`, `speak`, `viewer_chat`, …), events on stdout (`status` every second, `chat`, `goal`, `commentary`, `memory`, `stream_stats`, …). Bot config is a 0600 temp file; Minecraft access tokens go through an environment variable only.
- **API ↔ server**: process stdin (console commands) and stdout (log parsing for joins/leaves/readiness). RCON is never enabled.
- **API ↔ MediaMTX**: control API on 127.0.0.1:9997 (path readiness), playback server for archives/clips. Browsers reach WebRTC/HLS through `/api/media/*` on the same origin (works with one HTTPS reverse proxy).
- **API ↔ VoiceBox**: `POST /synthesize` → 24 kHz mono PCM.

## OmniCore (Rust)
`protocol/` (buf, nbt, text, codec, generated 26.3 packet IDs, `version.rs` adapters), `client.rs` (state machine, packet handlers, actions), `world.rs` (copy-on-write chunks, paletted containers), `entity.rs`, `inventory.rs` (122 data-component types decoded or skipped safely), `physics.rs`, `pathfind.rs`, `auth.rs`, `data.rs` (embedded vanilla data tables).

Version adapters: each Minecraft release maps logical packets to IDs (`tools/gen_packet_ids.py` generates them). The bot pings the server first and warns when the protocol doesn't match.

## OmniBrain (Rust)
Layering, from the bottom: deterministic **tasks** (`tasks.rs`, `crafting.rs`, `gameplay.rs`) → **reflexes** (death/respawn, threat → fight or flee using a learned `flee_health` weight) → **planner** (`agent.rs`: survival progression from the real inventory and world; mode-specific plans) → **social layer** (`intent.rs` chat intents, `persona.rs` commentary/answers built from a structured `Situation`; optional LLM). LLMs never produce movement packets; they only phrase lines.

Human-like behaviour: eased head rotation, looking where it walks, glancing at nearby players/mobs, idle "thinking" pauses, strafing in fights, celebrating crafts, greeting players it remembers, cooldowns after failures instead of loops.

## Data
SQLite tables: users, sessions, accounts (encrypted secrets), creators (JSON docs), servers, memories (creator/kind/key), bot_sessions, streams, clips, chat_messages, follows, voices, invitations, reserved_names. JSON-document columns keep a PostgreSQL migration straightforward.
