// VoiceBox client: voice profiles + synthesis requests to the VoiceBox service.

import { config } from "./config.ts";
import type { Db } from "./db.ts";
import { newId } from "./security.ts";

export interface VoiceProfile {
  id: string;
  name: string;
  /** espeak (built-in, offline), piper, http (Kokoro/Chatterbox-compatible server) */
  engine: "espeak" | "piper" | "http";
  preset: string;
  rate: number;
  pitch: number;
  volume: number;
  endpoint: string;
  model_file: string | null;
  rvc: { endpoint: string; model_file: string | null } | null;
  /** Required for uploaded/cloned voices: who owns the voice and their consent. */
  consent: { owner: string; statement: string; accepted_at: number } | null;
  original: boolean;
  created_at: number;
}

export const ORIGINAL_VOICES: Omit<VoiceProfile, "id" | "created_at">[] = [
  { name: "Nova (bright)", engine: "espeak", preset: "en-us+f3", rate: 1.1, pitch: 4, volume: 1, endpoint: "", model_file: null, rvc: null, consent: null, original: true },
  { name: "Ember (hyped)", engine: "espeak", preset: "en-us+m3", rate: 1.25, pitch: 2, volume: 1, endpoint: "", model_file: null, rvc: null, consent: null, original: true },
  { name: "Oak (calm)", engine: "espeak", preset: "en-gb+m1", rate: 0.9, pitch: -3, volume: 1, endpoint: "", model_file: null, rvc: null, consent: null, original: true },
  { name: "Pixel (playful)", engine: "espeak", preset: "en-us+f4", rate: 1.15, pitch: 6, volume: 1, endpoint: "", model_file: null, rvc: null, consent: null, original: true },
];

export class VoiceService {
  private db: Db;
  constructor(db: Db) {
    this.db = db;
    if (!db.docs<VoiceProfile>("voices").length) {
      for (const v of ORIGINAL_VOICES) this.save({ ...v, id: newId("vc_"), created_at: Date.now() });
    }
  }

  list(): VoiceProfile[] {
    return this.db.docs<VoiceProfile>("voices");
  }

  get(id: string | null | undefined): VoiceProfile | undefined {
    return id ? this.db.doc<VoiceProfile>("voices", id) : undefined;
  }

  save(v: VoiceProfile) {
    if (!v.original && v.model_file && !v.consent) {
      throw new Error("Uploaded voice models need a consent record: confirm you own this voice or have the speaker's permission.");
    }
    this.db.putDoc("voices", v.id, v);
  }

  async health(): Promise<any> {
    try {
      const r = await fetch(`${config.voiceboxUrl}/health`, { signal: AbortSignal.timeout(3000) });
      return await r.json();
    } catch (e: any) {
      return { ok: false, error: `VoiceBox unreachable at ${config.voiceboxUrl}: ${e.message}` };
    }
  }

  /** Returns 24 kHz mono 16-bit PCM for the text, using the creator's voice settings. */
  async synthesize(text: string, voice: VoiceProfile | undefined, overrides: { rate?: number; pitch?: number; volume?: number; emotion?: string } = {}): Promise<Buffer> {
    const v = voice ?? { ...ORIGINAL_VOICES[0], id: "default", created_at: 0 };
    const r = await fetch(`${config.voiceboxUrl}/synthesize`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        text,
        engine: v.engine,
        preset: v.preset,
        rate: overrides.rate ?? v.rate,
        pitch: overrides.pitch ?? v.pitch,
        volume: overrides.volume ?? v.volume,
        emotion: overrides.emotion ?? "neutral",
        endpoint: v.endpoint,
        model_file: v.model_file,
        rvc: v.rvc,
        format: "pcm_s16le_24000_mono",
      }),
      signal: AbortSignal.timeout(30_000),
    });
    if (!r.ok) throw new Error(`VoiceBox error ${r.status}: ${await r.text()}`);
    return Buffer.from(await r.arrayBuffer());
  }
}

/** Maps a gameplay event to delivery hints so speech sounds natural. */
export function emotionFor(event: string): { emotion: string; rate: number } {
  switch (event) {
    case "found_ore":
    case "achievement":
    case "killed":
      return { emotion: "excited", rate: 1.15 };
    case "hurt":
    case "low_health":
      return { emotion: "alarmed", rate: 1.2 };
    case "died":
      return { emotion: "sad", rate: 0.95 };
    case "night":
      return { emotion: "cautious", rate: 1.0 };
    default:
      return { emotion: "neutral", rate: 1.0 };
  }
}
