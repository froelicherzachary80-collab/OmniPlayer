# Hardware
Measured in the development container (4 vCPU, 15 GB RAM):

| Component | Cost |
|---|---|
| Bot process (no stream) | ~25–60 MB RAM, a few % of one core |
| Vanilla 26.3 server | 2–4 GB RAM recommended, 1–2 cores |
| One 720p30 stream | ~3 cores (renderer adapts 480×270 → lower when CPU is short) + ~0.5–1 core for x264 |
| VoiceBox (eSpeak) | negligible; neural TTS needs a capable CPU or GPU |

Practical guidance: **5 bots + 1 stream** fits a modern 6–8 core desktop. **16 bots** without streams fits 8 cores / 16 GB alongside the server. 32–100 bots need multiple hosts or a strong server and have **not been load-tested**. Limit streams with `OMNI_MAX_STREAMS` and pick `OMNI_STREAM_PRESET=low` on small machines. Bots keep playing when streaming or VoiceBox are disabled.
