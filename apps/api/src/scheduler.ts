// Background scheduler: starts/stops creator streams according to their
// schedules in the configured timezone. Streams only go live after the bot is
// in the server and the video pipeline reports ready.

import type { Creator } from "@omniplayer/shared";
import type { BotManager } from "./bots.ts";
import type { StreamManager } from "./streams.ts";

export function localParts(now: Date, timeZone: string) {
  const f = new Intl.DateTimeFormat("en-US", { timeZone, weekday: "short", hour: "2-digit", minute: "2-digit", hour12: false });
  const parts = Object.fromEntries(f.formatToParts(now).map((p) => [p.type, p.value]));
  const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  return { day: days.indexOf(parts.weekday), minutes: (Number(parts.hour) % 24) * 60 + Number(parts.minute) };
}

/** Returns the schedule entry active at `now`, if any. */
export function activeEntry(c: Creator, now = new Date()) {
  if (c.schedule?.mode !== "scheduled") return null;
  const { day, minutes } = localParts(now, c.schedule.timezone || "UTC");
  for (const e of c.schedule.entries ?? []) {
    const [h, m] = e.start.split(":").map(Number);
    const start = h * 60 + m;
    // Support windows that cross midnight.
    for (const offset of [0, -1]) {
      const d = (day + offset + 7) % 7;
      if (!e.days.includes(d)) continue;
      const rel = minutes + (offset === -1 ? 1440 : 0) - start;
      if (rel >= 0 && rel < e.duration_min) return e;
    }
  }
  return null;
}

export class Scheduler {
  private timer: NodeJS.Timeout | null = null;
  startedBySchedule = new Set<string>();
  log: { ts: number; text: string }[] = [];
  private creators: () => Creator[];
  private bots: BotManager;
  private streams: StreamManager;
  constructor(creators: () => Creator[], bots: BotManager, streams: StreamManager) {
    this.creators = creators;
    this.bots = bots;
    this.streams = streams;
  }

  start(intervalMs = 30_000) {
    this.timer = setInterval(() => this.tick().catch((e) => this.note(`scheduler error: ${e.message}`)), intervalMs);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
  }

  private note(text: string) {
    this.log.push({ ts: Date.now(), text });
    if (this.log.length > 200) this.log.shift();
  }

  async tick(now = new Date()) {
    for (const c of this.creators()) {
      const entry = activeEntry(c, now);
      const live = this.streams.live.has(c.id);
      const rt = this.bots.get(c.id);
      const botOnline = !!rt && rt.state === "online" && !!rt.status?.online;
      if (entry && !live) {
        if (!rt || rt.state === "stopped" || rt.state === "error") {
          this.note(`${c.streamer_name}: scheduled stream starting, launching bot`);
          await this.bots.start(c, entry.server_id ?? c.server_id).catch((e) => this.note(`${c.streamer_name}: ${e.message}`));
          this.startedBySchedule.add(c.id);
        } else if (botOnline) {
          await this.streams.start(c.id, { scheduled: true }).then(() => this.note(`${c.streamer_name}: went live (scheduled)`)).catch((e) => this.note(`${c.streamer_name}: ${e.message}`));
        }
      } else if (!entry && live && this.streams.live.get(c.id)?.scheduled) {
        await this.streams.stop(c.id, "schedule ended");
        this.note(`${c.streamer_name}: scheduled stream ended`);
        if (this.startedBySchedule.delete(c.id)) await this.bots.stop(c.id);
      } else if (c.schedule?.mode === "auto_when_online" && botOnline && !live) {
        await this.streams.start(c.id).then(() => this.note(`${c.streamer_name}: auto-live`)).catch(() => {});
      }
    }
  }
}
