// AI Creator Studio: generates original fictional streamer personas.
// Works fully offline (trait extraction + templates); a local LLM, when
// configured, only enriches the free-text description.

import { createHash } from "node:crypto";
import { deflateSync } from "node:zlib";
import { slugify, type BotMode, type Persona } from "@omniplayer/shared";
import { config } from "./config.ts";

const TRAITS: { key: string; words: string[]; persona: Partial<Persona>; mode?: BotMode; likes?: string[]; goals?: string[]; style?: string }[] = [
  { key: "pvp", words: ["pvp", "fight", "combat", "duel", "competitive", "warrior"], persona: { risk: 0.8, energy: 0.8 }, mode: "pvp_training", likes: ["PvP", "swords"], goals: ["win a 1v1 against every player on the server", "get full diamond armor"], style: "competitive trash talk, but friendly" },
  { key: "chaotic", words: ["chaotic", "chaos", "random", "crazy", "wild", "distracted", "improvise"], persona: { energy: 0.9, chattiness: 0.85, risk: 0.7 }, mode: "explorer", likes: ["TNT", "random adventures"], goals: ["find something nobody has seen before"], style: "fast, excitable, changes topic mid-sentence" },
  { key: "funny", words: ["joke", "jokes", "funny", "comedy", "goofy", "silly"], persona: { chattiness: 0.85 }, likes: ["pranks"], style: "jokes around constantly, self-deprecating humour" },
  { key: "builder", words: ["build", "builder", "architect", "architecture", "creative", "design"], persona: { energy: 0.35, risk: 0.2 }, mode: "builder", likes: ["architecture", "big bases"], goals: ["build the best base on the server", "finish a mega build with friends"], style: "calm, thoughtful, explains what they are building" },
  { key: "explorer", words: ["explore", "explorer", "adventure", "adventurous", "travel", "caves"], persona: { risk: 0.55 }, mode: "explorer", likes: ["caves", "new biomes"], goals: ["map every biome nearby", "find a stronghold"], style: "curious and descriptive" },
  { key: "diamonds", words: ["diamond", "diamonds", "mining", "miner", "ores"], persona: {}, likes: ["diamonds", "mining"], goals: ["stack of diamonds"], style: "gets loud about rare ores" },
  { key: "farmer", words: ["farm", "farming", "cozy", "chill", "relaxed", "animals"], persona: { energy: 0.3, risk: 0.15 }, mode: "farmer", likes: ["farms", "animals"], goals: ["build an automatic farm"], style: "relaxed and warm" },
  { key: "team", words: ["friends", "collaborat", "team", "together", "smp", "community"], persona: { chattiness: 0.75 }, mode: "cooperative", likes: ["teamwork"], goals: ["help everyone on the SMP"], style: "inclusive, asks others to join in" },
  { key: "survival", words: ["survival", "hardcore", "speedrun", "dragon"], persona: { risk: 0.45 }, mode: "autonomous", likes: ["survival progression"], goals: ["defeat the Ender Dragon"], style: "focused on progress" },
  { key: "calm", words: ["calm", "quiet", "chill", "soft-spoken", "peaceful"], persona: { energy: 0.25, chattiness: 0.4 }, style: "calm and soft-spoken" },
  { key: "talented", words: ["talented", "skilled", "pro", "expert", "good at"], persona: { skill_level: "expert" } },
];

const NAME_A = ["Nova", "Ember", "Oak", "Pixel", "Blaze", "Frost", "Echo", "Rune", "Moss", "Volt", "Cinder", "Lumen", "Quill", "Sable", "Tide", "Zephyr", "Juniper", "Ash", "Comet", "Fable"];
const NAME_B = ["Craft", "PvP", "Builder", "Fox", "Miner", "Blocks", "Quest", "Byte", "Spark", "Forge", "Wolf", "Lantern", "Pick", "Stone", "Rover"];

function pickSeeded<T>(arr: T[], seed: number) {
  return arr[Math.abs(seed) % arr.length];
}

function seedOf(s: string) {
  return createHash("sha256").update(s).digest().readInt32BE(0);
}

export interface GeneratedCreator {
  streamer_name: string;
  mc_username: string;
  slug: string;
  persona: Persona;
  mode: BotMode;
  stream_title: string;
  category: string;
  description: string;
  traits: string[];
}

export async function generateCreator(prompt: string, taken: Set<string>): Promise<GeneratedCreator> {
  const p = prompt.toLowerCase();
  const matched = TRAITS.filter((t) => t.words.some((w) => p.includes(w)));
  const seed = seedOf(prompt + Date.now());
  const persona: Persona = {
    streamer_name: "",
    personality: "",
    speaking_style: "",
    chattiness: 0.6,
    energy: 0.55,
    risk: 0.4,
    catchphrases: [],
    likes: [],
    goals: [],
    friends: [],
    rivals: [],
    skill_level: "intermediate",
  };
  let mode: BotMode = "autonomous";
  const styles: string[] = [];
  for (const t of matched) {
    Object.assign(persona, t.persona);
    if (t.mode && mode === "autonomous") mode = t.mode;
    persona.likes.push(...(t.likes ?? []));
    persona.goals.push(...(t.goals ?? []));
    if (t.style) styles.push(t.style);
  }
  if (!persona.goals.length) persona.goals.push("survive and build a cozy base", "make friends on the server");
  let name = "";
  for (let i = 0; i < 50 && (!name || taken.has(name.toLowerCase())); i++) {
    const a = pickSeeded(NAME_A, seed + i * 7);
    const b = matched.some((t) => t.key === "pvp") ? "PvP" : matched.some((t) => t.key === "builder") ? "Builder" : pickSeeded(NAME_B, seed >> 3 + i);
    name = `${a}${b}`.slice(0, 16);
  }
  persona.streamer_name = name;
  const vibe = matched.map((t) => t.key);
  persona.personality = describe(prompt, vibe);
  persona.speaking_style = styles.join("; ") || "friendly and conversational";
  persona.catchphrases = catchphrases(vibe, seed);
  if (config.llmEndpoint) {
    const enriched = await llmDescribe(prompt, name).catch(() => null);
    if (enriched) persona.personality = enriched;
  }
  const category = vibe.includes("pvp") ? "PvP" : vibe.includes("builder") ? "Building" : vibe.includes("explorer") || vibe.includes("chaotic") ? "Adventure" : "Survival";
  return {
    streamer_name: name,
    mc_username: name.replace(/[^A-Za-z0-9_]/g, "").slice(0, 16),
    slug: slugify(name),
    persona,
    mode,
    stream_title: `${name} — ${category === "PvP" ? "PvP and survival" : category.toLowerCase()} stream`,
    category,
    description: persona.personality,
    traits: vibe,
  };
}

function describe(prompt: string, vibe: string[]): string {
  const parts: string[] = [];
  if (vibe.includes("chaotic")) parts.push("chaotic and spontaneous");
  if (vibe.includes("talented")) parts.push("genuinely skilled");
  if (vibe.includes("pvp")) parts.push("loves PvP");
  if (vibe.includes("funny")) parts.push("always joking around");
  if (vibe.includes("diamonds")) parts.push("easily distracted by diamonds");
  if (vibe.includes("builder")) parts.push("a patient builder");
  if (vibe.includes("explorer")) parts.push("an eager explorer");
  if (vibe.includes("team")) parts.push("happiest collaborating with friends");
  if (vibe.includes("calm")) parts.push("calm under pressure");
  const lead = parts.length ? parts.join(", ") : "an upbeat survival player";
  return `Original AI-generated Minecraft creator: ${lead}. Inspired by the prompt: "${prompt.slice(0, 200)}".`;
}

function catchphrases(vibe: string[], seed: number): string[] {
  const pool: string[] = [
    ...(vibe.includes("pvp") ? ["Run it back!", "Too easy.", "That was a warm-up."] : []),
    ...(vibe.includes("chaotic") ? ["New plan!", "What could possibly go wrong?"] : []),
    ...(vibe.includes("builder") ? ["Details matter.", "One more block..."] : []),
    ...(vibe.includes("diamonds") ? ["Shiny!", "Diamonds don't lie."] : []),
    "Let's go!",
    "Okay chat, hear me out.",
  ];
  return [pickSeeded(pool, seed), pickSeeded(pool, seed >> 5)].filter((v, i, a) => a.indexOf(v) === i);
}

async function llmDescribe(prompt: string, name: string): Promise<string | null> {
  const r = await fetch(`${config.llmEndpoint.replace(/\/$/, "")}/api/generate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model: config.llmModel,
      stream: false,
      prompt: `Write a two-sentence personality description for an ORIGINAL fictional AI Minecraft streamer named ${name}. Do not reference real people or streamers. Brief: ${prompt}`,
    }),
    signal: AbortSignal.timeout(15_000),
  });
  if (!r.ok) return null;
  const j: any = await r.json();
  const t = String(j.response ?? "").trim();
  return t ? `Original AI-generated Minecraft creator. ${t}` : null;
}

/** Analyses an uploaded transcript for *generic* commentary style (no identity copying). */
export function analyseTranscript(text: string) {
  const sentences = text.split(/[.!?]+\s/).filter((s) => s.trim().length > 2);
  const words: string[] = text.toLowerCase().match(/[a-z']+/g) ?? [];
  const excl = (text.match(/!/g) ?? []).length / Math.max(1, sentences.length);
  const questions = (text.match(/\?/g) ?? []).length / Math.max(1, sentences.length);
  const avgLen = words.length / Math.max(1, sentences.length);
  const mcTerms = ["diamond", "creeper", "nether", "mine", "build", "craft", "sword", "pickaxe", "zombie", "village", "farm", "redstone", "enderman", "base"];
  const techniques = mcTerms.filter((t) => words.includes(t) || words.includes(t + "s"));
  // Only generic interjections are kept; names and distinctive phrases are discarded.
  const generic = ["wait", "no way", "let's go", "okay", "oh my", "chat", "guys", "bro", "dude", "yo"];
  const interjections = generic.filter((g) => text.toLowerCase().includes(g));
  return {
    energy: Math.min(1, 0.3 + excl * 0.6),
    chattiness: Math.min(1, 0.4 + avgLen / 40),
    asks_audience: questions > 0.15,
    avg_sentence_words: Math.round(avgLen * 10) / 10,
    gameplay_topics: techniques,
    interjections,
    note: "Only general style metrics and gameplay topics are extracted. Voice, names, catchphrases and identity of the source creator are not copied.",
  };
}

// ── Original art generators ───────────────────────────────────────────────

function palette(seed: number) {
  const hues = [(seed & 0xff) * 1.41, ((seed >> 8) & 0xff) * 1.41, ((seed >> 16) & 0xff) * 1.41].map((h) => Math.round(h % 360));
  const skinTones = ["#f1c7a5", "#d8a47f", "#b97a56", "#8d5a3b", "#5c3a24", "#e9b48b"];
  return { shirt: `hsl(${hues[0]} 65% 48%)`, pants: `hsl(${hues[1]} 45% 30%)`, hair: `hsl(${hues[2]} 40% ${18 + (seed & 15)}%)`, skin: pickSeeded(skinTones, seed >> 4) };
}

/** Deterministic original pixel-art avatar (SVG data URL). */
export function avatarSvg(name: string): string {
  const seed = seedOf(name);
  const c = palette(seed);
  const eye = (seed >> 9) & 1 ? "#2b6cb0" : "#3b2f2f";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8" shape-rendering="crispEdges">
<rect width="8" height="8" fill="${c.shirt}"/><rect x="1" y="1" width="6" height="6" fill="${c.skin}"/>
<rect x="1" y="1" width="6" height="2" fill="${c.hair}"/><rect x="1" y="3" width="1" height="1" fill="${c.hair}"/><rect x="6" y="3" width="1" height="1" fill="${c.hair}"/>
<rect x="2" y="4" width="1" height="1" fill="#fff"/><rect x="5" y="4" width="1" height="1" fill="#fff"/>
<rect x="3" y="4" width="1" height="1" fill="${eye}"/><rect x="4" y="4" width="1" height="1" fill="${eye}"/>
<rect x="3" y="6" width="2" height="1" fill="#7a3e2b"/></svg>`;
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
}

function hslToRgb(hsl: string): [number, number, number] {
  if (hsl.startsWith("#")) return [parseInt(hsl.slice(1, 3), 16), parseInt(hsl.slice(3, 5), 16), parseInt(hsl.slice(5, 7), 16)];
  const [h, s, l] = hsl.match(/[\d.]+/g)!.map(Number);
  const a = (s / 100) * Math.min(l / 100, 1 - l / 100);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    return Math.round(255 * (l / 100 - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

function crc32(buf: Buffer) {
  let c, crc = 0xffffffff;
  for (let n = 0; n < buf.length; n++) {
    c = (crc ^ buf[n]) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function png(w: number, h: number, rgba: Buffer): Buffer {
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(td));
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

/** Generates an original 64×64 Minecraft skin (standard layout) as PNG bytes. */
export function skinPng(name: string): Buffer {
  const seed = seedOf(name);
  const c = palette(seed);
  const [skin, shirt, pants, hair] = [c.skin, c.shirt, c.pants, c.hair].map(hslToRgb);
  const img = Buffer.alloc(64 * 64 * 4);
  const fill = (x0: number, y0: number, w: number, h: number, col: number[], noise = 10) => {
    for (let y = y0; y < y0 + h; y++)
      for (let x = x0; x < x0 + w; x++) {
        const n = ((x * 31 + y * 17 + seed) % noise) - noise / 2;
        const i = (y * 64 + x) * 4;
        img[i] = Math.max(0, Math.min(255, col[0] + n));
        img[i + 1] = Math.max(0, Math.min(255, col[1] + n));
        img[i + 2] = Math.max(0, Math.min(255, col[2] + n));
        img[i + 3] = 255;
      }
  };
  // Head (8..24 x 0..16 region incl. top/bottom + sides)
  fill(0, 8, 32, 8, skin);
  fill(8, 0, 16, 8, hair);
  fill(0, 8, 32, 2, hair);
  fill(8, 12, 2, 1, [255, 255, 255], 1);
  fill(13, 12, 2, 1, [255, 255, 255], 1);
  fill(9, 12, 1, 1, [40, 60, 120], 1);
  fill(13, 12, 1, 1, [40, 60, 120], 1);
  fill(10, 14, 4, 1, [120, 60, 45], 1);
  // Body, arms (shirt) and legs (pants)
  fill(16, 16, 24, 16, shirt);
  fill(40, 16, 16, 16, shirt);
  fill(44, 20, 4, 2, skin);
  fill(40, 28, 16, 4, skin);
  fill(0, 16, 16, 16, pants);
  fill(16, 48, 16, 16, pants);
  fill(32, 48, 16, 16, shirt);
  fill(36, 60, 8, 4, skin);
  return png(64, 64, img);
}
