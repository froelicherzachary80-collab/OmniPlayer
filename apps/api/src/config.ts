// Runtime configuration (environment variables; see .env.example).

import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(here, "..", "..", "..");

function env(name: string, def: string): string {
  return process.env[name] ?? def;
}

const dataDir = resolve(env("OMNI_DATA_DIR", join(ROOT, "data")));
mkdirSync(dataDir, { recursive: true });

/** Stable secret used to encrypt stored tokens; generated on first run if not provided. */
function secretKey(): Buffer {
  const fromEnv = process.env.OMNI_SECRET_KEY;
  if (fromEnv) return Buffer.from(fromEnv.padEnd(64, "0").slice(0, 64), "hex");
  const file = join(dataDir, ".secret-key");
  if (existsSync(file)) return Buffer.from(readFileSync(file, "utf8").trim(), "hex");
  const k = randomBytes(32);
  writeFileSync(file, k.toString("hex"), { mode: 0o600 });
  return k;
}

export const config = {
  host: env("OMNI_HOST", "0.0.0.0"),
  port: Number(env("OMNI_PORT", "8080")),
  dataDir,
  dbFile: join(dataDir, "omniplayer.db"),
  serversDir: join(dataDir, "servers"),
  backupsDir: join(dataDir, "backups"),
  uploadsDir: join(dataDir, "uploads"),
  clipsDir: join(dataDir, "clips"),
  recordingsDir: join(dataDir, "recordings"),
  webDist: resolve(env("OMNI_WEB_DIST", join(ROOT, "apps", "web", "dist"))),
  botBin: resolve(env("OMNI_BOT_BIN", join(ROOT, "target", "release", "omniplayer-bot"))),
  adminUser: env("OMNI_ADMIN_USER", "admin"),
  adminPassword: process.env.OMNI_ADMIN_PASSWORD ?? "",
  secretKey: secretKey(),
  publicHost: env("OMNI_PUBLIC_HOST", ""),
  // Microsoft OAuth (device code flow). Register a public client in Azure (see docs/ACCOUNTS.md).
  msClientId: env("OMNI_MS_CLIENT_ID", ""),
  // Streaming
  mediamtxBin: resolve(env("OMNI_MEDIAMTX_BIN", join(ROOT, "runtime", "bin", "mediamtx"))),
  mediamtxConfig: resolve(env("OMNI_MEDIAMTX_CONFIG", join(ROOT, "infra", "mediamtx", "mediamtx.yml"))),
  mediamtxManaged: env("OMNI_MEDIAMTX_MANAGED", "true") === "true",
  rtspUrl: env("OMNI_RTSP_URL", "rtsp://127.0.0.1:8554"),
  hlsUrl: env("OMNI_HLS_URL", "http://127.0.0.1:8888"),
  whepUrl: env("OMNI_WHEP_URL", "http://127.0.0.1:8889"),
  playbackUrl: env("OMNI_PLAYBACK_URL", "http://127.0.0.1:9996"),
  mediamtxApi: env("OMNI_MEDIAMTX_API", "http://127.0.0.1:9997"),
  // VoiceBox
  voiceboxUrl: env("OMNI_VOICEBOX_URL", "http://127.0.0.1:8095"),
  // Resource limits
  maxBots: Number(env("OMNI_MAX_BOTS", "16")),
  maxStreams: Number(env("OMNI_MAX_STREAMS", "2")),
  streamPreset: env("OMNI_STREAM_PRESET", "auto"),
  botThinkTicks: Number(env("OMNI_BOT_THINK_TICKS", "20")),
  botViewDistance: Number(env("OMNI_BOT_VIEW_DISTANCE", "6")),
  // Optional local LLM (Ollama-compatible) for dialogue; never required.
  llmEndpoint: env("OMNI_LLM_ENDPOINT", ""),
  llmModel: env("OMNI_LLM_MODEL", "llama3.2"),
  simulatedChat: env("OMNI_SIMULATED_CHAT", "false") === "true",
};

for (const d of [config.serversDir, config.backupsDir, config.uploadsDir, config.clipsDir, config.recordingsDir]) mkdirSync(d, { recursive: true });
