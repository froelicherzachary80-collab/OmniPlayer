// Stream quality presets and MediaMTX runtime config used by the API.
// The frame pipeline itself lives in the bot (services/omnibrain/src/stream.rs,
// rendering by services/omniview); MediaMTX handles WebRTC/HLS/recordings.

export interface StreamPreset {
  width: number; // internal render size (CPU ray caster)
  height: number;
  out_width: number; // encoded output size
  out_height: number;
  fps: number;
  bitrate_kbps: number;
}

/** Measured on a 4-core host: "medium" holds 30 fps with adaptive render scaling. */
export const PRESETS: Record<string, StreamPreset> = {
  low: { width: 320, height: 180, out_width: 854, out_height: 480, fps: 30, bitrate_kbps: 1200 },
  medium: { width: 480, height: 270, out_width: 1280, out_height: 720, fps: 30, bitrate_kbps: 2500 },
  high: { width: 640, height: 360, out_width: 1280, out_height: 720, fps: 30, bitrate_kbps: 3500 },
};

export function preset(name: string): StreamPreset {
  return PRESETS[name] ?? PRESETS.medium;
}

/** Points MediaMTX recordings at the data directory. */
export function runtimeMediaMtxConfig(base: string, recordingsDir: string): string {
  return base.replace(/recordPath: .*/, `recordPath: ${recordingsDir.replace(/\/$/, "")}/%path/%Y-%m-%d_%H-%M-%S-%f`);
}
