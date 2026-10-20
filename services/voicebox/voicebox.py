#!/usr/bin/env python3
"""VoiceBox AI — local, pluggable text-to-speech service for OmniPlayer creators.

POST /synthesize  {text, engine, preset, rate, pitch, volume, emotion, endpoint, model_file, rvc}
  → raw PCM s16le, 24 kHz, mono (what the OmniView stream mixer expects)
GET  /health

Engines (all local / self-hosted; no paid API):
  espeak  - eSpeak NG (always available, robotic but fully offline)
  piper   - Piper neural TTS if the `piper` binary and an .onnx voice are installed
  http    - any OpenAI-compatible local speech server (e.g. Kokoro-FastAPI,
            Chatterbox servers): POST {endpoint}/v1/audio/speech
Optional RVC post-processing: POST WAV to rvc.endpoint, expects WAV back. Only
use RVC/cloned voices you own or have the speaker's consent for — the API
enforces a consent record before such models can be attached.

Natural delivery: text is split into sentences, each synthesised separately,
and joined with short pauses; emotion adjusts rate/pitch.
Stdlib only; uses ffmpeg for resampling.
"""
import io
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.request
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

RATE = 24000
MAX_TEXT = 600

EMOTION = {
    "neutral": (1.0, 0),
    "excited": (1.12, 6),
    "alarmed": (1.18, 8),
    "sad": (0.9, -6),
    "cautious": (0.95, -2),
}


def to_pcm(audio: bytes, volume: float = 1.0, tempo: float = 1.0) -> bytes:
    """Any audio container → s16le 24 kHz mono via ffmpeg."""
    filters = [f"volume={max(0.0, min(2.0, volume)):.2f}"]
    if abs(tempo - 1.0) > 0.01:
        filters.append(f"atempo={max(0.5, min(2.0, tempo)):.3f}")
    p = subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", "pipe:0", "-af", ",".join(filters), "-f", "s16le", "-ac", "1", "-ar", str(RATE), "pipe:1"],
        input=audio, capture_output=True, timeout=60,
    )
    if p.returncode != 0:
        raise RuntimeError("ffmpeg: " + p.stderr.decode(errors="replace")[-300:])
    return p.stdout


def espeak(text, preset, rate, pitch):
    exe = shutil.which("espeak-ng") or shutil.which("espeak")
    if not exe:
        raise RuntimeError("eSpeak NG is not installed (apt install espeak-ng)")
    wpm = int(175 * rate)
    p = subprocess.run([exe, "-v", preset or "en-us", "-s", str(wpm), "-p", str(max(0, min(99, 50 + pitch * 4))), "--stdout", text], capture_output=True, timeout=30)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode(errors="replace"))
    return p.stdout, 1.0


def piper(text, model_file, rate):
    exe = shutil.which("piper")
    if not exe or not model_file or not os.path.exists(model_file):
        raise RuntimeError("Piper is not installed or no .onnx voice model was configured")
    p = subprocess.run([exe, "--model", model_file, "--length_scale", f"{1 / max(0.5, rate):.2f}", "--output_file", "-"], input=text.encode(), capture_output=True, timeout=60)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode(errors="replace")[-300:])
    return p.stdout, 1.0


def http_engine(text, endpoint, preset, rate):
    if not endpoint:
        raise RuntimeError("http engine needs an endpoint (e.g. http://127.0.0.1:8880)")
    body = json.dumps({"model": "tts-1", "input": text, "voice": preset or "af_heart", "response_format": "wav", "speed": rate}).encode()
    req = urllib.request.Request(endpoint.rstrip("/") + "/v1/audio/speech", data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read(), 1.0


def rvc(audio_wav: bytes, cfg) -> bytes:
    req = urllib.request.Request(cfg["endpoint"], data=audio_wav, headers={"Content-Type": "audio/wav", "X-Model-File": cfg.get("model_file") or ""})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def pcm_to_wav(pcm: bytes) -> bytes:
    b = io.BytesIO()
    with wave.open(b, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm)
    return b.getvalue()


def sentences(text):
    parts = re.split(r"(?<=[.!?…])\s+", text.strip())
    return [p for p in parts if p.strip()]


def synthesize(req):
    text = re.sub(r"\s+", " ", str(req.get("text", ""))).strip()[:MAX_TEXT]
    if not text:
        raise ValueError("text is required")
    engine = req.get("engine", "espeak")
    emotion = EMOTION.get(req.get("emotion", "neutral"), EMOTION["neutral"])
    rate = float(req.get("rate", 1.0)) * emotion[0]
    pitch = float(req.get("pitch", 0)) + emotion[1]
    volume = float(req.get("volume", 1.0))
    out = bytearray()
    for i, s in enumerate(sentences(text)):
        if engine == "piper":
            audio, tempo = piper(s, req.get("model_file"), rate)
        elif engine == "http":
            audio, tempo = http_engine(s, req.get("endpoint", ""), req.get("preset", ""), rate)
        else:
            audio, tempo = espeak(s, req.get("preset", "en-us"), rate, pitch)
        out += to_pcm(audio, volume, tempo)
        # natural pause between sentences (longer after questions/exclamations)
        if i < len(sentences(text)) - 1:
            pause = 0.32 if s.endswith(("?", "!")) else 0.22
            out += b"\x00\x00" * int(RATE * pause)
    pcm = bytes(out)
    if req.get("rvc") and req["rvc"].get("endpoint"):
        pcm = to_pcm(rvc(pcm_to_wav(pcm), req["rvc"]))
    return pcm


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write("voicebox: " + (fmt % args) + "\n")

    def _send(self, code, body: bytes, ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            engines = {
                "espeak": bool(shutil.which("espeak-ng") or shutil.which("espeak")),
                "piper": bool(shutil.which("piper")),
                "http": True,
                "ffmpeg": bool(shutil.which("ffmpeg")),
            }
            self._send(200, json.dumps({"ok": engines["espeak"] and engines["ffmpeg"], "engines": engines, "format": "pcm_s16le_24000_mono"}).encode())
        else:
            self._send(404, b'{"error":"not found"}')

    def do_POST(self):
        if self.path != "/synthesize":
            return self._send(404, b'{"error":"not found"}')
        try:
            n = int(self.headers.get("Content-Length", "0"))
            req = json.loads(self.rfile.read(n) or b"{}")
            pcm = synthesize(req)
            self._send(200, pcm, "application/octet-stream")
        except Exception as e:  # report engine errors to the API
            self._send(400, json.dumps({"error": str(e)}).encode())


def main():
    host = os.environ.get("VOICEBOX_HOST", "127.0.0.1")
    port = int(os.environ.get("VOICEBOX_PORT", "8095"))
    srv = ThreadingHTTPServer((host, port), Handler)
    print(f"VoiceBox listening on http://{host}:{port}", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
