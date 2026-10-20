// MineLive stream orchestration on top of MediaMTX.

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Creator } from "@omniplayer/shared";
import type { BotManager } from "./bots.ts";
import { config } from "./config.ts";
import type { Db } from "./db.ts";
import { newId } from "./security.ts";

export interface LiveInfo {
  stream_id: string;
  creator_id: string;
  path: string;
  started_at: number;
  title: string;
  ready: boolean;
  scheduled: boolean;
}

export class StreamManager {
  mtx: ChildProcess | null = null;
  live = new Map<string, LiveInfo>(); // creator_id → info
  viewers = new Map<string, Set<string>>(); // slug → connection ids
  private thumbTimer: NodeJS.Timeout | null = null;

  private db: Db;
  private bots: BotManager;
  private creator: (id: string) => Creator | undefined;
  constructor(db: Db, bots: BotManager, creator: (id: string) => Creator | undefined) {
    this.db = db;
    this.bots = bots;
    this.creator = creator;
    bots.on("bot", (e) => {
      if (e.type === "exit") this.markEnded(e.creator_id, "bot stopped");
    });
  }

  async startMediaMtx(): Promise<void> {
    if (!config.mediamtxManaged || this.mtx) return;
    if (!existsSync(config.mediamtxBin)) {
      console.warn(`[streams] MediaMTX not found at ${config.mediamtxBin}; streaming disabled until it is installed (see docs/STREAMING.md)`);
      return;
    }
    const base = await readFile(config.mediamtxConfig, "utf8");
    const runtime = base.replace(/recordPath: .*/, `recordPath: ${join(config.recordingsDir, "%path", "%Y-%m-%d_%H-%M-%S-%f")}`);
    const file = join(config.dataDir, "mediamtx.runtime.yml");
    await writeFile(file, runtime);
    this.mtx = spawn(config.mediamtxBin, [file], { cwd: config.dataDir, stdio: ["ignore", "pipe", "pipe"] });
    this.mtx.stdout!.on("data", () => {});
    this.mtx.stderr!.on("data", () => {});
    this.mtx.on("exit", (c) => {
      console.warn(`[streams] MediaMTX exited (${c})`);
      this.mtx = null;
    });
    this.thumbTimer = setInterval(() => this.refreshThumbnails().catch(() => {}), 60_000);
  }

  async stopMediaMtx() {
    if (this.thumbTimer) clearInterval(this.thumbTimer);
    this.mtx?.kill("SIGTERM");
  }

  pathFor(c: Creator) {
    return `live/${c.slug}`;
  }

  async pathReady(path: string): Promise<boolean> {
    try {
      const r = await fetch(`${config.mediamtxApi}/v3/paths/get/${path}`);
      if (!r.ok) return false;
      const j: any = await r.json();
      return !!j.ready;
    } catch {
      return false;
    }
  }

  presetConfig() {
    const presets: Record<string, any> = {
      low: { width: 320, height: 180, out_width: 854, out_height: 480, fps: 30, bitrate_kbps: 1200 },
      medium: { width: 480, height: 270, out_width: 1280, out_height: 720, fps: 30, bitrate_kbps: 2500 },
      high: { width: 640, height: 360, out_width: 1280, out_height: 720, fps: 30, bitrate_kbps: 3500 },
    };
    return presets[config.streamPreset] ?? presets.medium;
  }

  /** Starts the real POV pipeline. The stream is only "live" once frames reach MediaMTX. */
  async start(creatorId: string, opts: { title?: string; scheduled?: boolean } = {}): Promise<LiveInfo> {
    const c = this.creator(creatorId);
    if (!c) throw new Error("creator not found");
    const existing = this.live.get(creatorId);
    if (existing) return existing;
    const rt = this.bots.get(creatorId);
    if (!rt || rt.state === "stopped") throw new Error(`${c.streamer_name} is not in a Minecraft server. Start the bot first.`);
    if (!rt.status?.online || rt.status?.dead === undefined) throw new Error(`${c.streamer_name} has not finished joining the server yet.`);
    if (this.live.size >= config.maxStreams) throw new Error(`Live stream limit reached (${config.maxStreams}). Raise OMNI_MAX_STREAMS if your CPU allows.`);
    if (!this.mtx && config.mediamtxManaged) await this.startMediaMtx();
    const path = this.pathFor(c);
    const title = opts.title || c.stream_title || `${c.streamer_name} plays survival`;
    await this.bots.request(creatorId, { cmd: "stream_start", config: { ...this.presetConfig(), output: `${config.rtspUrl}/${path}`, title: c.streamer_name } });
    const info: LiveInfo = { stream_id: newId("st_"), creator_id: creatorId, path, started_at: Date.now(), title, ready: false, scheduled: !!opts.scheduled };
    this.live.set(creatorId, info);
    this.db.run("INSERT INTO streams (id, creator_id, title, path, status, started_at, scheduled) VALUES (?, ?, ?, ?, 'starting', ?, ?)", info.stream_id, creatorId, title, path, info.started_at, info.scheduled ? 1 : 0);
    for (let i = 0; i < 40 && !info.ready; i++) {
      await new Promise((r) => setTimeout(r, 500));
      info.ready = await this.pathReady(path);
    }
    if (!info.ready) {
      await this.stop(creatorId, "stream never became ready");
      throw new Error("The video pipeline did not start (check ffmpeg / MediaMTX logs in System).");
    }
    this.db.run("UPDATE streams SET status = 'live' WHERE id = ?", info.stream_id);
    this.refreshThumbnail(c).catch(() => {});
    return info;
  }

  async stop(creatorId: string, reason = "stopped"): Promise<void> {
    try {
      await this.bots.request(creatorId, { cmd: "stream_stop" }, 10_000);
    } catch {
      /* bot may be gone */
    }
    this.markEnded(creatorId, reason);
  }

  private markEnded(creatorId: string, _reason: string) {
    const info = this.live.get(creatorId);
    if (!info) return;
    this.live.delete(creatorId);
    this.db.run("UPDATE streams SET status = 'ended', ended_at = ? WHERE id = ?", Date.now(), info.stream_id);
  }

  viewerCount(slug: string): number {
    return this.viewers.get(slug)?.size ?? 0;
  }

  join(slug: string, conn: string) {
    let s = this.viewers.get(slug);
    if (!s) this.viewers.set(slug, (s = new Set()));
    s.add(conn);
    const c = this.findBySlug(slug);
    const info = c && this.live.get(c.id);
    if (info) this.db.run("UPDATE streams SET peak_viewers = MAX(peak_viewers, ?) WHERE id = ?", s.size, info.stream_id);
  }

  leave(slug: string, conn: string) {
    this.viewers.get(slug)?.delete(conn);
  }

  findBySlug(slug: string): Creator | undefined {
    const r = this.db.get("SELECT doc FROM creators WHERE slug = ?", slug);
    return r ? JSON.parse(r.doc) : undefined;
  }

  /** Recorded segments for a creator (from MediaMTX's playback server). */
  async recordings(c: Creator): Promise<{ start: string; duration: number; url: string }[]> {
    try {
      const r = await fetch(`${config.playbackUrl}/list?path=${encodeURIComponent(this.pathFor(c))}`);
      if (!r.ok) return [];
      const segs: any[] = (await r.json()) as any[];
      return segs.map((s) => ({
        start: s.start,
        duration: s.duration,
        url: `/api/media/playback?path=${encodeURIComponent(this.pathFor(c))}&start=${encodeURIComponent(s.start)}&duration=${s.duration}`,
      }));
    } catch {
      return [];
    }
  }

  /** Saves the last `seconds` of a creator's recording as a clip (MP4). */
  async clip(c: Creator, seconds: number, title: string, by: string | null) {
    const recs = await this.recordings(c);
    const last = recs.at(-1);
    if (!last) throw new Error("No recording available yet for this channel.");
    const startMs = Date.parse(last.start) + Math.max(0, last.duration - seconds) * 1000;
    const url = `${config.playbackUrl}/get?path=${encodeURIComponent(this.pathFor(c))}&start=${encodeURIComponent(new Date(startMs).toISOString())}&duration=${seconds}&format=mp4`;
    const r = await fetch(url);
    if (!r.ok) throw new Error(`playback server returned ${r.status}`);
    const id = newId("clip_");
    await mkdir(config.clipsDir, { recursive: true });
    const file = join(config.clipsDir, `${id}.mp4`);
    await writeFile(file, Buffer.from(await r.arrayBuffer()));
    const stream = this.live.get(c.id)?.stream_id ?? null;
    this.db.run("INSERT INTO clips (id, stream_id, creator_id, title, file, duration, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)", id, stream, c.id, title || `${c.streamer_name} clip`, `${id}.mp4`, seconds, by, Date.now());
    return { id, url: `/api/clips/${id}/file` };
  }

  async refreshThumbnail(c: Creator) {
    const dir = join(config.uploadsDir, "thumbs");
    await mkdir(dir, { recursive: true });
    await new Promise<void>((resolve) => {
      const p = spawn("ffmpeg", ["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i", `${config.rtspUrl}/${this.pathFor(c)}`, "-frames:v", "1", "-vf", "scale=640:-2", "-y", join(dir, `${c.slug}.jpg`)], { stdio: "ignore" });
      const t = setTimeout(() => p.kill("SIGKILL"), 20_000);
      p.on("exit", () => {
        clearTimeout(t);
        resolve();
      });
      p.on("error", () => resolve());
    });
  }

  async refreshThumbnails() {
    for (const info of this.live.values()) {
      const c = this.creator(info.creator_id);
      if (c) await this.refreshThumbnail(c);
    }
  }
}
