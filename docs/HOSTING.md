# Hosting

OmniPlayer runs on a computer or Linux server; your iPhone is the remote control. A suspended Safari tab cannot run Minecraft servers, bots, video encoding or TTS — nothing in this project pretends otherwise.

## Single computer (simplest)
`./scripts/start.sh` after `npm run build`. Open `http://<LAN-IP>:8080` on the iPhone (same Wi-Fi). Set `OMNI_PUBLIC_HOST=<LAN-IP>` so the Play page shows the right address.

## Docker / VPS
```bash
cp .env.example .env   # set OMNI_ADMIN_PASSWORD, OMNI_PUBLIC_HOST
docker compose -f infra/docker/docker-compose.yml up -d --build
```
The image bundles Java 21/25, FFmpeg, eSpeak NG and MediaMTX. Data (worlds, database, recordings) lives in the `omniplayer-data` volume.

**HTTPS without buying a domain:** set `OMNI_DOMAIN=<ip-with-dashes>.sslip.io` (or your own domain) and add `-f infra/docker/docker-compose.domain.yml`; Caddy gets a Let's Encrypt certificate. `scripts/cloud-setup.sh` does all of this on a fresh Ubuntu VPS. See docs/IPHONE_ONLY.md.

**Ports:** TCP 80/443 (website), 25565–25575 (Java), UDP 19132–19140 (Bedrock/iPhone via Geyser), UDP 8189 (WebRTC video).

**Quick public link from a home computer:** `./scripts/share.sh` starts everything and opens a Cloudflare quick tunnel (`https://<random>.trycloudflare.com`) for the website. Minecraft ports are not tunnelled.

## Reaching it from outside your home
Prefer a private network: **Tailscale/WireGuard** — install on the host and the iPhone, then use the tailnet name for both the dashboard and Minecraft. If you must expose it publicly:
- Put the dashboard behind HTTPS (Caddy/nginx → :8080). Installable PWA features and WebRTC work best over HTTPS.
- Forward only the Minecraft ports you use (25565+) and UDP 8189 for WebRTC media.
- Keep offline-mode servers private: Mojang does not verify identities there. The manager enforces an allowlist and reserved names, but anyone who knows an allowlisted name could claim it. Bind such servers to the VPN interface (`bind_ip`) or firewall them.
- Admin ports (MediaMTX API 9997) bind to localhost only.

# Hardware
Measured in the development container (4 vCPU, 15 GB RAM):

| Component | Cost |
|---|---|
| Bot process (no stream) | ~25–60 MB RAM, a few % of one core |
| Vanilla 26.3 server | 2–4 GB RAM recommended, 1–2 cores |
| One 720p30 stream | ~3 cores (renderer adapts 480×270 → lower when CPU is short) + ~0.5–1 core for x264 |
| VoiceBox (eSpeak) | negligible; neural TTS needs a capable CPU or GPU |

Practical guidance: **5 bots + 1 stream** fits a modern 6–8 core desktop. **16 bots** without streams fits 8 cores / 16 GB alongside the server. 32–100 bots need multiple hosts or a strong server and have **not been load-tested**. Limit streams with `OMNI_MAX_STREAMS` and pick `OMNI_STREAM_PRESET=low` on small machines. Bots keep playing when streaming or VoiceBox are disabled.
