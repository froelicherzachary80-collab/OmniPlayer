# Streaming (MineLive)

Pipeline per creator: **real server → OmniCore → world snapshot → OmniView → raw RGB → FFmpeg (libx264 baseline + Opus) → RTSP → MediaMTX → WebRTC (WHEP) / HLS → browser.**

- Install MediaMTX: download `mediamtx_<ver>_linux_amd64.tar.gz` from github.com/bluenviron/mediamtx/releases and put the binary at `runtime/bin/mediamtx` (tested with v1.21.2). The API starts it with `infra/mediamtx/mediamtx.yml` (publishing allowed only from local/private networks; recordings kept 7 days).
- A stream is marked live only after MediaMTX reports the path ready, i.e. real frames arrived.
- Render quality adapts automatically: if fps drops below 93 % of target the internal resolution steps down; it steps back up when there is headroom. Output is always the fixed encoded size (1280×720 for `medium`).
- Textures: point `OMNI_ASSETS_DIR` at `assets/minecraft/textures` extracted from **your own** Minecraft client jar. Without it OmniView uses vanilla map colours with a procedural texel pattern. No Mojang art is bundled.
- Audio: VoiceBox PCM is mixed into a real-time pipe; silence otherwise. Captions are burned in with FFmpeg drawtext.
- Archives: MediaMTX fMP4 recordings; past broadcasts play through `/api/media/playback`, clips save the last 5–60 s as MP4.
- Viewer counts are live WebSocket connections on the channel page, nothing else.
- iPhone Safari: WebRTC first; falls back to native HLS.
