# Verification report

Environment: 4 vCPU Linux container, Rust 1.97, Node 22.22, Java 25, FFmpeg, MediaMTX v1.21.2. Network policy blocked piston-meta/piston-data.mojang.com, PaperMC and Microsoft auth hosts.

**Test server:** because Mojang's server jar could not be downloaded, live tests ran against `tests/test-server` — a small program on **Minestom** (open-source, independent implementation of the 26.3 / protocol 777 server). It is only a test counterpart; OmniCore has no dependency on it. Every suite also runs against the official jar: `MC_SERVER_JAR=/path/server.jar MC_ACCEPT_EULA=1`.

| Suite | Command | Result |
|---|---|---|
| Rust unit tests | `cargo test --workspace` | 39 passed |
| v0.1 client | `cd tests/integration && python3 -m unittest test_v01_client` | 8/8 |
| Gameplay | `python3 -m unittest test_v03_gameplay` | 5 passed, 1 skipped (crafting needs vanilla) |
| Streaming | `python3 -m unittest test_streaming` (release bot) | 5/5 — 30.7 fps steady at viewer |
| API end-to-end + plugins/mods/Geyser/Fabric installers | `npm run test:node` | 14/14 (installers against a local mock of the Modrinth, GeyserMC and Fabric APIs) |
| Docker image | `docker build -f infra/docker/Dockerfile .` | builds; inside the container a server starts, a bot joins and chats, a stream is published and HLS is served |
| Cloud setup script | `scripts/cloud-setup.sh` in a clean `ubuntu:24.04` container (Docker/git/network stubbed) | writes `.env` (20-char password, 64-hex key, `<ip>.sslip.io`), files mode 600, runs compose with the HTTPS override, safe to re-run |
| VoiceBox | `python3 -m unittest services/voicebox/test_voicebox.py` | 3/3 |
| Full stack, iPhone emulation | `node tests/e2e/fullstack.mjs` | 15/15 checks, in-browser decode skipped |

Assertions use the **server's** view (its log/console: joins, positions, rotation, chat, block breaks, pickups, attacks, kills), not the bot's own claims.

## Not yet verified
- Official Vanilla/Paper jar download and run (code path implemented; blocked here).
- Crafting (Minestom doesn't compute recipe results) — test exists, skipped without `MC_SERVER_JAR`.
- Microsoft device-code sign-in, entitlement check, online-mode encryption + session join against a real server.
- In-browser H.264 playback: the sandbox Chromium lacks H.264 (WebRTC offers only VP8/VP9/AV1). Verified instead: frames reach an FFmpeg viewer at 1280×720/30 fps, WHEP proxy reaches MediaMTX, the player degrades gracefully. Real Safari/Chrome include H.264.
- Real iPhone Safari (emulated with Chromium only), Lifesteal datapack on vanilla, Piper/Kokoro/RVC engines, 32–100 bot scale.
- Live Modrinth, GeyserMC and Fabric downloads (hosts blocked here; tested against mocks of their documented APIs), an actual Bedrock client joining through Geyser, Caddy certificate issuance on a real VPS, Cloudflare tunnels.
