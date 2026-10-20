// Shared data contracts between the API, the web app and the services.

export type AccountType = "microsoft" | "offline";
export type AuthMode = "online" | "offline";
export type ServerSoftware = "vanilla" | "paper" | "fabric" | "custom" | "external";
export type GameMode = "survival" | "creative" | "adventure" | "hardcore";
export type Difficulty = "peaceful" | "easy" | "normal" | "hard";
export type BotMode =
  | "autonomous"
  | "follow"
  | "cooperative"
  | "team"
  | "pvp_training"
  | "builder"
  | "explorer"
  | "farmer"
  | "guard"
  | "streamer"
  | "idle";

export const BOT_MODES: BotMode[] = [
  "autonomous", "follow", "cooperative", "team", "pvp_training", "builder", "explorer", "farmer", "guard", "streamer", "idle",
];

export interface Persona {
  streamer_name: string;
  personality: string;
  speaking_style: string;
  chattiness: number;
  energy: number;
  risk: number;
  catchphrases: string[];
  likes: string[];
  goals: string[];
  friends: string[];
  rivals: string[];
  skill_level: "beginner" | "intermediate" | "expert";
}

export interface VoiceSettings {
  voice_id: string | null;
  rate: number; // words per minute multiplier 0.5..2
  pitch: number; // -10..10
  volume: number; // 0..2
}

export interface ScheduleEntry {
  days: number[]; // 0 = Sunday
  start: string; // "16:00"
  duration_min: number;
  server_id: string | null;
  activity?: string;
}

export interface Schedule {
  mode: "manual" | "scheduled" | "auto_when_online";
  timezone: string;
  entries: ScheduleEntry[];
}

export interface Creator {
  id: string;
  slug: string;
  streamer_name: string;
  mc_username: string;
  account_id: string | null;
  account_type: AccountType;
  avatar: string | null;
  skin: string | null;
  persona: Persona;
  voice: VoiceSettings;
  mode: BotMode;
  mode_target: string | null;
  role: string;
  server_id: string | null;
  schedule: Schedule;
  stream_title: string;
  category: string;
  created_at: number;
}

export interface Account {
  id: string;
  type: AccountType;
  mc_username: string;
  mc_uuid: string;
  entitlement_ok: boolean;
  skin_url: string | null;
  assigned_creator_id: string | null;
  status: string;
  created_at: number;
}

export interface ServerConfig {
  id: string;
  name: string;
  software: ServerSoftware;
  version: string;
  port: number;
  auth_mode: AuthMode;
  gamemode: GameMode;
  difficulty: Difficulty;
  seed: string;
  world_type: "default" | "flat" | "large_biomes" | "amplified";
  max_players: number;
  ai_count: number;
  auto_start_bots: boolean;
  memory_mb: number;
  host: string; // for external servers
  custom_command: string; // for software=custom
  whitelist: string[];
  ops: string[];
  smp_preset: string | null;
  eula_accepted_at: number | null;
  properties: Record<string, string>;
  /** Geyser + Floodgate: lets iPhone/Bedrock Minecraft players join (Paper/Fabric only). */
  bedrock: boolean;
  bedrock_port: number;
  bedrock_players: string[];
  created_at: number;
}

export interface BotStatus {
  online: boolean;
  username: string;
  pos: [number, number, number];
  health: number;
  food: number;
  dimension: string;
  biome: string | null;
  mode: string;
  goal: string;
  inventory: [string, number][];
  nearby: { id: number; kind: string; name: string | null; distance: number }[];
  [k: string]: unknown;
}

export interface ChatMessage {
  id: number;
  channel: string;
  username: string;
  text: string;
  kind: "user" | "creator" | "system" | "simulated";
  created_at: number;
}

/** Offline-mode UUID exactly as the vanilla server computes it (UUID v3 of "OfflinePlayer:<name>"). */
export function offlineUuid(name: string, md5: (s: string) => Uint8Array): string {
  const h = md5("OfflinePlayer:" + name);
  h[6] = (h[6] & 0x0f) | 0x30;
  h[8] = (h[8] & 0x3f) | 0x80;
  const hex = Array.from(h, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function validUsername(name: string): boolean {
  return /^[A-Za-z0-9_]{3,16}$/.test(name);
}

export function slugify(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 40) || "creator";
}

/** Whether an account type may join a server with the given auth mode. */
export function compatible(account: AccountType, auth: AuthMode): { ok: boolean; reason: string } {
  if (auth === "online" && account === "offline") {
    return { ok: false, reason: "Online-mode servers verify every player with Mojang; offline accounts cannot join them." };
  }
  if (auth === "offline" && account === "microsoft") {
    return { ok: true, reason: "Allowed, but this server does not verify identities (offline mode)." };
  }
  return { ok: true, reason: "Compatible." };
}
