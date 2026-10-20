// Bot process supervisor: one `omniplayer-bot` process per running AI creator.

import { spawn, type ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import { existsSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compatible, type Creator, type ServerConfig } from "@omniplayer/shared";
import { config } from "./config.ts";
import type { Db } from "./db.ts";
import { decrypt, encrypt } from "./security.ts";
import { ensureFresh, type McSession } from "./microsoft.ts";

export interface BotRuntime {
  creatorId: string;
  proc: ChildProcess;
  serverId: string | null;
  sessionId: number;
  startedAt: number;
  status: Record<string, any> | null;
  state: "starting" | "online" | "reconnecting" | "stopping" | "stopped" | "error";
  lastError: string;
  logs: { ts: number; level: string; message: string }[];
  activity: { ts: number; text: string }[];
  streaming: boolean;
  cfgDir: string;
  pending: Map<string, (ok: boolean, err?: string) => void>;
}

export interface ServerLookup {
  get(id: string): ServerConfig | undefined;
  isRunning(id: string): boolean;
}

export class BotManager extends EventEmitter {
  bots = new Map<string, BotRuntime>();
  private cmdSeq = 0;
  private db: Db;
  private servers: ServerLookup;
  private creators: () => Creator[];
  constructor(db: Db, servers: ServerLookup, creators: () => Creator[]) {
    super();
    this.db = db;
    this.servers = servers;
    this.creators = creators;
  }

  running(): BotRuntime[] {
    return [...this.bots.values()].filter((b) => b.state !== "stopped");
  }

  get(id: string) {
    return this.bots.get(id);
  }

  /** Pre-flight compatibility check shown in the UI before joining. */
  check(c: Creator, serverId: string | null): { ok: boolean; problems: string[]; notes: string[] } {
    const problems: string[] = [];
    const notes: string[] = [];
    const sid = serverId ?? c.server_id;
    const s = sid ? this.servers.get(sid) : undefined;
    if (!s) problems.push("Choose a Minecraft server for this creator.");
    const acc = c.account_id ? this.db.get("SELECT * FROM accounts WHERE id = ?", c.account_id) : undefined;
    if (!acc) problems.push("Assign a Minecraft account (authenticated or offline) to this creator.");
    if (s && acc) {
      const cmp = compatible(acc.type, s.auth_mode);
      (cmp.ok ? notes : problems).push(cmp.reason);
      if (acc.type === "microsoft" && !acc.entitlement_ok) problems.push("This Microsoft account does not have a Minecraft: Java Edition entitlement.");
      if (s.software !== "external" && s.auth_mode === "offline" && !s.whitelist.map((n) => n.toLowerCase()).includes(acc.mc_username.toLowerCase())) {
        notes.push(`${acc.mc_username} will be added to this private server's allowlist.`);
      }
      const inUse = this.running().find((b) => b.creatorId !== c.id && this.creators().find((x) => x.id === b.creatorId)?.account_id === acc.id);
      if (inUse) problems.push("This account is already in use by another running bot. One account can only play one session at a time.");
      const sameName = this.running().find((b) => {
        const other = this.creators().find((x) => x.id === b.creatorId);
        const oa = other?.account_id ? this.db.get("SELECT mc_username FROM accounts WHERE id = ?", other.account_id) : undefined;
        return other && other.id !== c.id && oa?.mc_username?.toLowerCase() === acc.mc_username.toLowerCase() && b.serverId === s.id;
      });
      if (sameName) problems.push(`Another bot is already connected as ${acc.mc_username} on this server.`);
      if (s.software !== "external" && !this.servers.isRunning(s.id)) problems.push(`Server "${s.name}" is not running. Start it first.`);
    }
    if (!existsSync(config.botBin)) problems.push(`Bot binary not found at ${config.botBin}. Run "npm run build:rust".`);
    if (this.running().length >= config.maxBots && !this.bots.get(c.id)) problems.push(`Bot limit reached (${config.maxBots}). Raise OMNI_MAX_BOTS if your hardware allows.`);
    return { ok: problems.length === 0, problems, notes };
  }

  private memoryFor(creatorId: string) {
    const rows = this.db.all("SELECT kind, key, data FROM memories WHERE creator_id = ?", creatorId);
    const mem: any = { players: {}, locations: {}, stats: {}, strategy: {}, recent_events: [] };
    for (const r of rows) {
      const d = JSON.parse(r.data);
      if (r.kind === "player") mem.players[r.key] = d;
      else if (r.kind === "location") mem.locations[r.key] = d.pos ?? d;
      else if (r.kind === "stat") mem.stats[r.key] = d.value ?? d;
      else if (r.kind === "strategy") mem.strategy[r.key] = d.value ?? d;
      else if (r.kind === "event") mem.recent_events.push(d.text ?? String(d));
    }
    mem.recent_events = mem.recent_events.slice(-50);
    return mem;
  }

  remember(creatorId: string, kind: string, key: string, data: unknown) {
    this.db.run(
      `INSERT INTO memories (creator_id, kind, key, data, updated_at) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(creator_id, kind, key) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at`,
      creatorId, kind, key, JSON.stringify(data), Date.now(),
    );
  }

  async start(c: Creator, serverId: string | null, opts: { owner?: string } = {}): Promise<BotRuntime> {
    const existing = this.bots.get(c.id);
    if (existing && existing.state !== "stopped") return existing;
    const chk = this.check(c, serverId);
    if (!chk.ok) throw new Error(chk.problems.join(" "));
    const sid = (serverId ?? c.server_id)!;
    const s = this.servers.get(sid)!;
    const acc = this.db.get("SELECT * FROM accounts WHERE id = ?", c.account_id)!;
    const env: Record<string, string> = { ...process.env } as any;
    const account: any = { type: acc.type };
    if (acc.type === "microsoft") {
      let session = decrypt<McSession>(acc.secret);
      if (!session) throw new Error("Stored Microsoft session is missing; reconnect the account.");
      session = await ensureFresh(session);
      this.db.run("UPDATE accounts SET secret = ?, mc_username = ?, skin_url = ? WHERE id = ?", encrypt(session), session.name, session.skin_url, acc.id);
      env.OMNI_MC_ACCESS_TOKEN = session.mc_access_token;
      account.profile_id = session.profile_id;
      account.username = session.name;
    }
    const teammates = this.creators()
      .filter((o) => o.id !== c.id && o.server_id === sid && o.account_id)
      .map((o) => this.db.get("SELECT mc_username FROM accounts WHERE id = ?", o.account_id)?.mc_username)
      .filter(Boolean);
    const cfg = {
      bot_id: c.slug,
      username: acc.mc_username,
      host: s.software === "external" ? s.host : "127.0.0.1",
      port: s.port,
      version: s.version,
      account,
      mode: c.mode,
      mode_target: c.mode_target,
      persona: { ...c.persona, streamer_name: c.streamer_name },
      llm: config.llmEndpoint ? { endpoint: config.llmEndpoint, model: config.llmModel, timeout_ms: 8000 } : {},
      memory: this.memoryFor(c.id),
      brain: {
        owner: opts.owner ?? c.mode_target ?? "",
        teammates,
        aliases: [c.streamer_name, c.slug],
        role: c.role,
        chat_commentary: false,
        think_interval_ticks: config.botThinkTicks,
        allow_pvp: s.properties?.pvp !== "false",
      },
      reconnect: true,
      view_distance: config.botViewDistance,
    };
    const cfgDir = await mkdtemp(join(tmpdir(), "omnibot-"));
    const cfgFile = join(cfgDir, "bot.json");
    await writeFile(cfgFile, JSON.stringify(cfg), { mode: 0o600 });
    const proc = spawn(config.botBin, ["--config", cfgFile], { stdio: ["pipe", "pipe", "pipe"], env });
    const session = this.db.run("INSERT INTO bot_sessions (creator_id, server_id, started_at) VALUES (?, ?, ?)", c.id, sid, Date.now());
    const rt: BotRuntime = {
      creatorId: c.id,
      proc,
      serverId: sid,
      sessionId: Number(session.lastInsertRowid),
      startedAt: Date.now(),
      status: null,
      state: "starting",
      lastError: "",
      logs: [],
      activity: [],
      streaming: false,
      cfgDir,
      pending: new Map(),
    };
    this.bots.set(c.id, rt);
    let buf = "";
    proc.stdout!.on("data", (d: Buffer) => {
      buf += d.toString();
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, i);
        buf = buf.slice(i + 1);
        try {
          this.onEvent(rt, JSON.parse(line));
        } catch {
          this.log(rt, "warn", line);
        }
      }
    });
    proc.stderr!.on("data", (d: Buffer) => this.log(rt, "error", d.toString().trim()));
    proc.on("exit", (code) => {
      rt.state = code === 0 || rt.state === "stopping" ? "stopped" : "error";
      if (code && !rt.lastError) rt.lastError = `bot exited with code ${code}`;
      rt.streaming = false;
      this.db.run("UPDATE bot_sessions SET ended_at = ?, end_reason = ? WHERE id = ?", Date.now(), rt.lastError || "stopped", rt.sessionId);
      rm(cfgDir, { recursive: true, force: true }).catch(() => {});
      this.emit("bot", { creator_id: c.id, type: "exit", code, error: rt.lastError });
    });
    this.emit("bot", { creator_id: c.id, type: "starting" });
    return rt;
  }

  private log(rt: BotRuntime, level: string, message: string) {
    if (!message) return;
    rt.logs.push({ ts: Date.now(), level, message });
    if (rt.logs.length > 500) rt.logs.splice(0, rt.logs.length - 500);
    this.emit("log", { creator_id: rt.creatorId, level, message });
  }

  private activity(rt: BotRuntime, text: string) {
    rt.activity.push({ ts: Date.now(), text });
    if (rt.activity.length > 200) rt.activity.splice(0, rt.activity.length - 200);
  }

  private onEvent(rt: BotRuntime, ev: any) {
    const d = ev.data ?? {};
    switch (ev.type) {
      case "status":
        rt.status = d;
        if (rt.state === "starting" || rt.state === "reconnecting") rt.state = "online";
        this.emit("status", { creator_id: rt.creatorId, status: d });
        return;
      case "connected":
        rt.state = "online";
        this.activity(rt, `Connected as ${d.username} (protocol ${d.protocol})`);
        break;
      case "connect_failed":
      case "fatal":
        rt.lastError = d.reason ?? d.error ?? "unknown error";
        this.log(rt, "error", rt.lastError);
        break;
      case "disconnected":
      case "session_ended":
        this.activity(rt, `Disconnected: ${d.reason}`);
        break;
      case "reconnecting":
        rt.state = "reconnecting";
        this.activity(rt, `Reconnecting in ${d.in_seconds}s`);
        break;
      case "log":
        this.log(rt, d.level ?? "info", d.message ?? "");
        return;
      case "goal":
        this.activity(rt, `Goal: ${d.goal}`);
        break;
      case "goal_done":
        this.activity(rt, `Done: ${d.goal}`);
        break;
      case "goal_failed":
        this.activity(rt, `Couldn't ${d.goal}: ${d.reason}`);
        break;
      case "crafted":
        this.activity(rt, `Crafted ${d.item}`);
        break;
      case "memory":
        this.persistMemory(rt.creatorId, d);
        break;
      case "memory_snapshot":
        this.persistSnapshot(rt.creatorId, d);
        break;
      case "ack": {
        const cb = d.id && rt.pending.get(String(d.id));
        if (cb) {
          rt.pending.delete(String(d.id));
          cb(!!d.ok, d.error);
        }
        break;
      }
      case "stream_started":
        rt.streaming = true;
        break;
      case "stream_stopped":
        rt.streaming = false;
        break;
    }
    this.emit("event", { creator_id: rt.creatorId, ...ev });
  }

  private persistMemory(creatorId: string, d: any) {
    const now = Date.now();
    if (d.kind === "player" && d.name) {
      const old = this.db.get("SELECT data FROM memories WHERE creator_id = ? AND kind = 'player' AND key = ?", creatorId, d.name);
      const p = old ? JSON.parse(old.data) : { name: d.name, first_seen: now, interactions: 0, affinity: 0, notes: [] };
      p.last_seen = now;
      this.remember(creatorId, "player", d.name, p);
    } else if (d.kind === "location" && d.label) {
      this.remember(creatorId, "location", d.label, { pos: d.pos, at: now });
    } else if (d.kind === "strategy" && d.key) {
      this.remember(creatorId, "strategy", d.key, { value: d.value });
    } else if (d.kind === "event") {
      this.remember(creatorId, "event", String(now), { text: d.text, pos: d.pos });
    }
  }

  private persistSnapshot(creatorId: string, m: any) {
    for (const [k, v] of Object.entries(m.players ?? {})) this.remember(creatorId, "player", k, v);
    for (const [k, v] of Object.entries(m.locations ?? {})) this.remember(creatorId, "location", k, { pos: v });
    for (const [k, v] of Object.entries(m.stats ?? {})) this.remember(creatorId, "stat", k, { value: v });
    for (const [k, v] of Object.entries(m.strategy ?? {})) this.remember(creatorId, "strategy", k, { value: v });
  }

  send(creatorId: string, cmd: Record<string, unknown>): void {
    const rt = this.bots.get(creatorId);
    if (!rt || rt.state === "stopped" || !rt.proc.stdin?.writable) throw new Error("bot is not running");
    rt.proc.stdin.write(JSON.stringify(cmd) + "\n");
  }

  /** Sends a command and waits for its acknowledgement. */
  request(creatorId: string, cmd: Record<string, unknown>, timeoutMs = 20_000): Promise<void> {
    const rt = this.bots.get(creatorId);
    if (!rt) return Promise.reject(new Error("bot is not running"));
    const id = String(++this.cmdSeq);
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => {
        rt.pending.delete(id);
        reject(new Error("bot did not respond"));
      }, timeoutMs);
      rt.pending.set(id, (ok, err) => {
        clearTimeout(t);
        ok ? resolve() : reject(new Error(err ?? "command failed"));
      });
      try {
        this.send(creatorId, { ...cmd, id });
      } catch (e) {
        clearTimeout(t);
        rt.pending.delete(id);
        reject(e);
      }
    });
  }

  async stop(creatorId: string): Promise<void> {
    const rt = this.bots.get(creatorId);
    if (!rt || rt.state === "stopped") return;
    rt.state = "stopping";
    try {
      this.send(creatorId, { cmd: "disconnect" });
    } catch {
      /* already gone */
    }
    await new Promise<void>((resolve) => {
      const t = setTimeout(() => {
        rt.proc.kill("SIGKILL");
        resolve();
      }, 10_000);
      rt.proc.once("exit", () => {
        clearTimeout(t);
        resolve();
      });
      if (rt.proc.exitCode !== null) {
        clearTimeout(t);
        resolve();
      }
    });
  }

  async stopAll() {
    await Promise.all(this.running().map((b) => this.stop(b.creatorId)));
  }
}
