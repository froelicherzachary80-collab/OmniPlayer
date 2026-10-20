// OmniPlayer API — management backend + MineLive site server.

import Fastify, { type FastifyReply, type FastifyRequest } from "fastify";
import fastifyMultipart from "@fastify/multipart";
import fastifyStatic from "@fastify/static";
import fastifyWebsocket from "@fastify/websocket";
import { existsSync } from "node:fs";
import { readFile, stat, writeFile, mkdir } from "node:fs/promises";
import os from "node:os";
import { join } from "node:path";
import { randomBytes, createHash } from "node:crypto";
import { BOT_MODES, compatible, slugify, validUsername, type Creator, type ServerConfig } from "@omniplayer/shared";
import { offlineUuid, PRESETS, preset, listVanillaReleases, findJavas } from "@omniplayer/server-manager";
import { config } from "./config.ts";
import { Db } from "./db.ts";
import { createSession, decrypt, encrypt, hashPassword, newId, revokeSession, sessionUser, verifyPassword, RateLimiter } from "./security.ts";
import { BotManager } from "./bots.ts";
import { ServerRegistry, pingServer, DEFAULT_VERSION } from "./servers.ts";
import { StreamManager } from "./streams.ts";
import { ChatHub, DEFAULT_MODERATION } from "./chat.ts";
import { VoiceService, emotionFor, type VoiceProfile } from "./voice.ts";
import { Scheduler, activeEntry } from "./scheduler.ts";
import { analyseTranscript, avatarSvg, generateCreator, skinPng } from "./creatorgen.ts";
import * as ms from "./microsoft.ts";

export async function buildApp(opts: { logger?: boolean } = {}) {
  const db = new Db(config.dbFile);
  const servers = new ServerRegistry(db);
  const creators = () => db.docs<Creator>("creators");
  const creator = (id: string) => db.doc<Creator>("creators", id);
  const bots = new BotManager(db, servers, creators);
  const streams = new StreamManager(db, bots, creator);
  const chat = new ChatHub(db, (slug) => streams.findBySlug(slug));
  const voices = new VoiceService(db);
  const scheduler = new Scheduler(creators, bots, streams);
  const systemLog: { ts: number; level: string; source: string; message: string }[] = [];
  const sysLog = (level: string, source: string, message: string) => {
    systemLog.push({ ts: Date.now(), level, source, message });
    if (systemLog.length > 2000) systemLog.splice(0, systemLog.length - 2000);
  };

  // ── bootstrap admin ──
  if (!db.get("SELECT id FROM users WHERE role = 'admin'")) {
    const pw = config.adminPassword || randomBytes(9).toString("base64url");
    db.run("INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, 'admin', ?)", newId("u_"), config.adminUser, hashPassword(pw), Date.now());
    if (!config.adminPassword) {
      console.log(`\n  OmniPlayer admin account created\n    username: ${config.adminUser}\n    password: ${pw}\n  (set OMNI_ADMIN_PASSWORD to choose your own; change it in System → Security)\n`);
      await writeFile(join(config.dataDir, "initial-admin-password.txt"), `${config.adminUser}\n${pw}\n`, { mode: 0o600 });
    }
  }

  const app = Fastify({ logger: opts.logger ?? false, bodyLimit: 8 * 1024 * 1024, trustProxy: true });
  await app.register(fastifyWebsocket);
  await app.register(fastifyMultipart, { limits: { fileSize: 500 * 1024 * 1024 } });
  app.addContentTypeParser("application/sdp", { parseAs: "string" }, (_req, body, done) => done(null, body));
  app.addContentTypeParser("text/plain", { parseAs: "string" }, (_req, body, done) => done(null, body));

  const tokenOf = (req: FastifyRequest) => {
    const h = req.headers.authorization;
    if (h?.startsWith("Bearer ")) return h.slice(7);
    return (req.query as any)?.token as string | undefined;
  };
  const user = (req: FastifyRequest) => sessionUser(db, tokenOf(req));
  const admin = (req: FastifyRequest, reply: FastifyReply) => {
    const u = user(req);
    if (!u || u.role !== "admin") {
      reply.code(401).send({ error: "Admin login required" });
      return null;
    }
    return u;
  };
  const fail = (reply: FastifyReply, code: number, error: string) => reply.code(code).send({ error });
  const loginLimiter = new RateLimiter(10, 60_000);

  app.setErrorHandler((err: any, _req, reply) => {
    sysLog("error", "api", err.message);
    reply.code(err.statusCode && err.statusCode < 500 ? err.statusCode : 400).send({ error: err.message });
  });

  // ── event fan-out to admin dashboards ──
  const adminSockets = new Set<any>();
  const broadcast = (type: string, data: unknown) => {
    const s = JSON.stringify({ type, data });
    for (const ws of adminSockets) {
      try {
        ws.send(s);
      } catch {
        /* closed */
      }
    }
  };
  bots.on("status", (e) => broadcast("bot_status", e));
  bots.on("bot", (e) => broadcast("bot", e));
  bots.on("log", (e) => {
    sysLog(e.level, `bot:${creator(e.creator_id)?.slug}`, e.message);
    broadcast("bot_log", e);
  });
  servers.on("console", (e) => broadcast("console", e));
  servers.on("state", (e) => {
    sysLog("info", `server:${e.server_id}`, `state → ${e.state}`);
    broadcast("server_state", e);
  });

  // ── commentary → VoiceBox → stream audio; viewer replies → MineLive chat ──
  const voiceQueues = new Map<string, Promise<void>>();
  bots.on("event", (e: any) => {
    const c = creator(e.creator_id);
    if (!c) return;
    if (e.type === "viewer_reply" && e.data?.text) chat.post(c.slug, c.streamer_name, e.data.text, "creator");
    if (e.type === "smp_request") sysLog("info", `bot:${c.slug}`, `${e.data.from} asked ${c.streamer_name} to start an SMP (use the SMP creator).`);
    if (e.type === "commentary" && e.data?.text) {
      broadcast("commentary", { creator_id: c.id, ...e.data });
      if (!streams.live.has(c.id)) return;
      const prev = voiceQueues.get(c.id) ?? Promise.resolve();
      const next = prev.then(async () => {
        try {
          const em = emotionFor(e.data.event);
          const v = voices.get(c.voice?.voice_id);
          const pcm = await voices.synthesize(e.data.text, v, { rate: (c.voice?.rate ?? 1) * em.rate, pitch: c.voice?.pitch, volume: c.voice?.volume, emotion: em.emotion });
          bots.send(c.id, { cmd: "speak", pcm_b64: pcm.toString("base64"), caption: e.data.text });
          await new Promise((r) => setTimeout(r, (pcm.length / 2 / 24000) * 1000));
        } catch (err: any) {
          // Without VoiceBox the line is still shown as a caption.
          try {
            bots.send(c.id, { cmd: "speak", caption: e.data.text });
          } catch {
            /* bot gone */
          }
          sysLog("warn", "voicebox", err.message);
        }
      });
      voiceQueues.set(c.id, next);
    }
  });
  chat.onViewerMessage = (slug, username, text) => {
    const c = streams.findBySlug(slug);
    if (c && bots.get(c.id)?.state === "online") {
      try {
        bots.send(c.id, { cmd: "viewer_chat", user: username, text });
      } catch {
        /* offline */
      }
    }
  };
  chat.onPresence = (slug, id, joined) => (joined ? streams.join(slug, id) : streams.leave(slug, id));

  // ────────────────────────────── auth ──────────────────────────────
  app.post("/api/auth/login", async (req, reply) => {
    const { username, password } = (req.body ?? {}) as any;
    if (!loginLimiter.allow(req.ip)) return fail(reply, 429, "Too many login attempts, wait a minute.");
    const u = db.get("SELECT * FROM users WHERE username = ?", String(username ?? ""));
    if (!u || u.banned || !verifyPassword(String(password ?? ""), u.password_hash)) return fail(reply, 401, "Wrong username or password");
    return { token: createSession(db, u.id), user: { id: u.id, username: u.username, role: u.role } };
  });
  app.post("/api/auth/register", async (req, reply) => {
    const { username, password } = (req.body ?? {}) as any;
    if (!/^[A-Za-z0-9_]{3,20}$/.test(String(username ?? ""))) return fail(reply, 400, "Username: 3-20 letters, numbers or _");
    if (String(password ?? "").length < 8) return fail(reply, 400, "Password must be at least 8 characters");
    if (db.get("SELECT id FROM users WHERE username = ?", username)) return fail(reply, 409, "That username is taken");
    if (db.get("SELECT doc FROM creators WHERE lower(json_extract(doc, '$.streamer_name')) = lower(?)", username)) return fail(reply, 409, "That name belongs to an AI creator");
    const id = newId("u_");
    db.run("INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, 'viewer', ?)", id, username, hashPassword(password), Date.now());
    return { token: createSession(db, id), user: { id, username, role: "viewer" } };
  });
  app.post("/api/auth/logout", async (req) => {
    const t = tokenOf(req);
    if (t) revokeSession(db, t);
    return { ok: true };
  });
  app.get("/api/auth/me", async (req, reply) => user(req) ?? fail(reply, 401, "not logged in"));
  app.post("/api/auth/password", async (req, reply) => {
    const u = user(req);
    if (!u) return fail(reply, 401, "not logged in");
    const { current, next } = req.body as any;
    const row = db.get("SELECT password_hash FROM users WHERE id = ?", u.id)!;
    if (!verifyPassword(String(current ?? ""), row.password_hash)) return fail(reply, 401, "Current password is wrong");
    if (String(next ?? "").length < 8) return fail(reply, 400, "New password must be at least 8 characters");
    db.run("UPDATE users SET password_hash = ? WHERE id = ?", hashPassword(next), u.id);
    return { ok: true };
  });

  // ────────────────────────────── overview / system ──────────────────────────────
  const cpuSample = { t: Date.now(), idle: 0, total: 0 };
  const hostCpu = () => {
    let idle = 0, total = 0;
    for (const c of os.cpus()) {
      for (const v of Object.values(c.times)) total += v;
      idle += c.times.idle;
    }
    const pct = cpuSample.total ? 100 * (1 - (idle - cpuSample.idle) / Math.max(1, total - cpuSample.total)) : 0;
    Object.assign(cpuSample, { t: Date.now(), idle, total });
    return Math.round(pct * 10) / 10;
  };
  const procStats = async (pid: number | undefined) => {
    if (!pid) return null;
    try {
      const st = (await readFile(`/proc/${pid}/status`, "utf8")).match(/VmRSS:\s+(\d+)/);
      return { rss_mb: Math.round(Number(st?.[1] ?? 0) / 1024) };
    } catch {
      return null;
    }
  };

  app.get("/api/overview", async (req, reply) => {
    if (!admin(req, reply)) return;
    const srv = servers.list();
    return {
      servers: { total: srv.length, running: srv.filter((s) => servers.isRunning(s.id) && s.software !== "external").length },
      bots: { online: bots.running().filter((b) => b.state === "online").length, running: bots.running().length, max: config.maxBots },
      streams: { live: streams.live.size, max: config.maxStreams },
      creators: creators().length,
      host: { cpu_percent: hostCpu(), cpus: os.cpus().length, mem_total_mb: Math.round(os.totalmem() / 1048576), mem_free_mb: Math.round(os.freemem() / 1048576), load: os.loadavg(), uptime_s: Math.round(os.uptime()) },
      live: [...streams.live.values()].map((l) => ({ ...l, creator: creator(l.creator_id)?.streamer_name, viewers: streams.viewerCount(creator(l.creator_id)?.slug ?? "") })),
    };
  });

  app.get("/api/system", async (req, reply) => {
    if (!admin(req, reply)) return;
    const workers = await Promise.all(
      bots.running().map(async (b) => ({ creator: creator(b.creatorId)?.streamer_name, creator_id: b.creatorId, state: b.state, pid: b.proc.pid, started_at: b.startedAt, streaming: b.streaming, ...(await procStats(b.proc.pid)) })),
    );
    const javas = await findJavas().catch(() => []);
    return {
      workers,
      scheduler: scheduler.log.slice(-100),
      limits: { max_bots: config.maxBots, max_streams: config.maxStreams, stream_preset: config.streamPreset, think_ticks: config.botThinkTicks, view_distance: config.botViewDistance },
      services: {
        mediamtx: streams.mtx ? "running" : existsSync(config.mediamtxBin) ? "stopped" : "not installed",
        voicebox: await voices.health(),
        bot_binary: existsSync(config.botBin) ? config.botBin : "missing (run npm run build:rust)",
        llm: config.llmEndpoint || "disabled (template dialogue)",
        microsoft_auth: config.msClientId ? "configured" : "not configured (OMNI_MS_CLIENT_ID)",
        java: javas.map((j) => `${j.major} (${j.path})`),
      },
      host: { platform: os.platform(), cpus: os.cpus().length, mem_total_mb: Math.round(os.totalmem() / 1048576) },
    };
  });
  app.get("/api/system/logs", async (req, reply) => {
    if (!admin(req, reply)) return;
    const level = (req.query as any).level;
    return systemLog.filter((l) => !level || l.level === level).slice(-500);
  });

  // ────────────────────────────── creators ──────────────────────────────
  const blankCreator = (p: Partial<Creator>): Creator => ({
    id: newId("cr_"),
    slug: slugify(p.streamer_name ?? "creator"),
    streamer_name: "New Creator",
    mc_username: "",
    account_id: null,
    account_type: "offline",
    avatar: null,
    skin: null,
    persona: { streamer_name: "", personality: "", speaking_style: "friendly", chattiness: 0.6, energy: 0.55, risk: 0.4, catchphrases: [], likes: [], goals: [], friends: [], rivals: [], skill_level: "intermediate" },
    voice: { voice_id: voices.list()[0]?.id ?? null, rate: 1, pitch: 0, volume: 1 },
    mode: "autonomous",
    mode_target: null,
    role: "",
    server_id: null,
    schedule: { mode: "manual", timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC", entries: [] },
    stream_title: "",
    category: "Survival",
    created_at: Date.now(),
    ...p,
  });
  const publicCreator = (c: Creator) => {
    const rt = bots.get(c.id);
    const acc = c.account_id ? db.get("SELECT id, type, mc_username, mc_uuid, entitlement_ok, skin_url FROM accounts WHERE id = ?", c.account_id) : null;
    return { ...c, account: acc, runtime: rt ? { state: rt.state, error: rt.lastError, streaming: rt.streaming, status: rt.status } : null, live: streams.live.get(c.id) ?? null };
  };
  const uniqueSlug = (base: string, id?: string) => {
    let s = slugify(base), i = 2;
    while (db.get("SELECT id FROM creators WHERE slug = ? AND id != ?", s, id ?? "")) s = `${slugify(base)}-${i++}`;
    return s;
  };
  const saveCreator = (c: Creator) => db.run(
    `INSERT INTO creators (id, slug, doc, created_at) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET slug = excluded.slug, doc = excluded.doc`,
    c.id, c.slug, JSON.stringify(c), c.created_at,
  );

  app.get("/api/creators", async (req, reply) => (admin(req, reply) ? creators().map(publicCreator) : undefined));
  app.get("/api/creators/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    return c ? publicCreator(c) : fail(reply, 404, "not found");
  });
  app.post("/api/creators/generate", async (req, reply) => {
    if (!admin(req, reply)) return;
    const { prompt } = req.body as any;
    if (!prompt || String(prompt).length < 5) return fail(reply, 400, "Describe the creator in a sentence or two.");
    const taken = new Set(creators().map((c) => c.streamer_name.toLowerCase()));
    const g = await generateCreator(String(prompt), taken);
    return { ...g, avatar: avatarSvg(g.streamer_name) };
  });
  app.post("/api/creators", async (req, reply) => {
    if (!admin(req, reply)) return;
    const b = (req.body ?? {}) as any;
    const name = String(b.streamer_name ?? "").trim();
    if (!name) return fail(reply, 400, "Streamer name is required");
    const c = blankCreator({ ...b, id: newId("cr_"), slug: uniqueSlug(b.slug || name), streamer_name: name, created_at: Date.now() });
    c.persona = { ...blankCreator({}).persona, ...(b.persona ?? {}), streamer_name: name };
    c.avatar = b.avatar ?? avatarSvg(name);
    if (c.mc_username && !validUsername(c.mc_username)) return fail(reply, 400, "Minecraft usernames are 3-16 letters, numbers or _");
    if (!BOT_MODES.includes(c.mode)) c.mode = "autonomous";
    saveCreator(c);
    return publicCreator(c);
  });
  app.put("/api/creators/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const b = (req.body ?? {}) as any;
    for (const k of ["id", "created_at", "account_id", "account"]) delete b[k];
    const merged: Creator = { ...c, ...b, persona: { ...c.persona, ...(b.persona ?? {}) }, voice: { ...c.voice, ...(b.voice ?? {}) }, schedule: { ...c.schedule, ...(b.schedule ?? {}) } };
    if (b.streamer_name && b.streamer_name !== c.streamer_name) merged.slug = uniqueSlug(b.streamer_name, c.id);
    if (merged.mc_username && !validUsername(merged.mc_username)) return fail(reply, 400, "Minecraft usernames are 3-16 letters, numbers or _");
    saveCreator(merged);
    return publicCreator(merged);
  });
  app.delete("/api/creators/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    await streams.stop(id).catch(() => {});
    await bots.stop(id);
    db.run("UPDATE accounts SET status = 'ready' WHERE id = (SELECT json_extract(doc, '$.account_id') FROM creators WHERE id = ?)", id);
    db.run("DELETE FROM creators WHERE id = ?", id);
    db.run("DELETE FROM memories WHERE creator_id = ?", id);
    return { ok: true };
  });
  app.post("/api/creators/:id/avatar", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const f = await req.file();
    if (!f || !/^image\/(png|jpeg|webp|gif)$/.test(f.mimetype)) return fail(reply, 400, "Upload a PNG, JPEG, WebP or GIF image");
    const buf = await f.toBuffer();
    if (buf.length > 2 * 1024 * 1024) return fail(reply, 400, "Avatar must be under 2 MB");
    c.avatar = `data:${f.mimetype};base64,${buf.toString("base64")}`;
    saveCreator(c);
    return { ok: true };
  });
  app.post("/api/creators/:id/skin", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const f = await req.file();
    if (!f) return fail(reply, 400, "Upload a 64x64 PNG skin");
    const buf = await f.toBuffer();
    if (buf.subarray(1, 4).toString() !== "PNG") return fail(reply, 400, "Skins must be PNG files");
    const w = buf.readUInt32BE(16), h = buf.readUInt32BE(20);
    if (w !== 64 || (h !== 64 && h !== 32)) return fail(reply, 400, `Minecraft skins are 64x64 (got ${w}x${h})`);
    await mkdir(join(config.uploadsDir, "skins"), { recursive: true });
    await writeFile(join(config.uploadsDir, "skins", `${c.id}.png`), buf);
    c.skin = `/api/creators/${c.id}/skin.png?v=${Date.now()}`;
    saveCreator(c);
    return { ok: true, skin: c.skin };
  });
  app.post("/api/creators/:id/skin/generate", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    await mkdir(join(config.uploadsDir, "skins"), { recursive: true });
    await writeFile(join(config.uploadsDir, "skins", `${c.id}.png`), skinPng(c.streamer_name + Date.now()));
    c.skin = `/api/creators/${c.id}/skin.png?v=${Date.now()}`;
    saveCreator(c);
    return { ok: true, skin: c.skin };
  });
  app.get("/api/creators/:id/skin.png", async (req, reply) => {
    const file = join(config.uploadsDir, "skins", `${(req.params as any).id.replace(/[^\w-]/g, "")}.png`);
    if (!existsSync(file)) return fail(reply, 404, "no skin");
    reply.type("image/png").header("Cache-Control", "public, max-age=300");
    return readFile(file);
  });
  app.post("/api/creators/:id/transcript", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const text = typeof req.body === "string" ? req.body : String((req.body as any)?.text ?? "");
    if (text.length < 200) return fail(reply, 400, "Paste at least a few paragraphs of transcript.");
    const style = analyseTranscript(text);
    if ((req.query as any).apply === "1") {
      c.persona.energy = Math.round(style.energy * 100) / 100;
      c.persona.chattiness = Math.round(style.chattiness * 100) / 100;
      c.persona.likes = [...new Set([...c.persona.likes, ...style.gameplay_topics])].slice(0, 12);
      saveCreator(c);
    }
    return style;
  });

  // Bot control
  app.get("/api/creators/:id/check", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    return bots.check(c, (req.query as any).server_id ?? null);
  });
  app.post("/api/creators/:id/start", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const sid = (req.body as any)?.server_id ?? c.server_id;
    if (sid && sid !== c.server_id) {
      c.server_id = sid;
      saveCreator(c);
    }
    const s = sid ? servers.get(sid) : undefined;
    const acc = c.account_id ? db.get("SELECT mc_username FROM accounts WHERE id = ?", c.account_id) : undefined;
    if (s && acc && s.software !== "external" && s.auth_mode === "offline") await servers.allow(s.id, acc.mc_username, "bot");
    const rt = await bots.start(c, sid, { owner: (req.body as any)?.owner });
    return { ok: true, state: rt.state };
  });
  app.post("/api/creators/:id/stop", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    await streams.stop(id).catch(() => {});
    await bots.stop(id);
    return { ok: true };
  });
  app.post("/api/creators/:id/command", async (req, reply) => {
    if (!admin(req, reply)) return;
    const body = (req.body ?? {}) as any;
    if (!body.cmd) return fail(reply, 400, "cmd required");
    await bots.request((req.params as any).id, body);
    return { ok: true };
  });
  app.post("/api/creators/:id/mode", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id);
    if (!c) return fail(reply, 404, "not found");
    const { mode, target } = req.body as any;
    if (!BOT_MODES.includes(mode)) return fail(reply, 400, "unknown mode");
    c.mode = mode;
    c.mode_target = target ?? null;
    saveCreator(c);
    if (bots.get(c.id)?.state === "online") await bots.request(c.id, { cmd: "mode", mode, target });
    return { ok: true };
  });
  app.get("/api/creators/:id/runtime", async (req, reply) => {
    if (!admin(req, reply)) return;
    const rt = bots.get((req.params as any).id);
    if (!rt) return { state: "stopped", logs: [], activity: [] };
    return { state: rt.state, error: rt.lastError, status: rt.status, logs: rt.logs.slice(-200), activity: rt.activity.slice(-100), streaming: rt.streaming, started_at: rt.startedAt };
  });
  app.get("/api/creators/:id/memory", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    const rows = db.all("SELECT kind, key, data, updated_at FROM memories WHERE creator_id = ? ORDER BY updated_at DESC", id);
    const sessions = db.all("SELECT * FROM bot_sessions WHERE creator_id = ? ORDER BY id DESC LIMIT 50", id);
    return { memories: rows.map((r) => ({ ...r, data: JSON.parse(r.data) })), sessions };
  });
  app.put("/api/creators/:id/memory", async (req, reply) => {
    if (!admin(req, reply)) return;
    const { kind, key, data } = req.body as any;
    if (!kind || !key) return fail(reply, 400, "kind and key required");
    bots.remember((req.params as any).id, kind, key, data);
    return { ok: true };
  });
  app.delete("/api/creators/:id/memory/:kind/:key", async (req, reply) => {
    if (!admin(req, reply)) return;
    const p = req.params as any;
    db.run("DELETE FROM memories WHERE creator_id = ? AND kind = ? AND key = ?", p.id, p.kind, p.key);
    return { ok: true };
  });
  app.put("/api/creators/:id/moderation", async (req, reply) => {
    if (!admin(req, reply)) return;
    const c = creator((req.params as any).id) as any;
    if (!c) return fail(reply, 404, "not found");
    c.moderation = { ...DEFAULT_MODERATION, ...(req.body as any) };
    saveCreator(c);
    return c.moderation;
  });

  // ────────────────────────────── accounts ──────────────────────────────
  const publicAccount = (a: any) => {
    const assigned = creators().find((c) => c.account_id === a.id);
    const inUse = assigned && bots.get(assigned.id) && bots.get(assigned.id)!.state !== "stopped";
    return { id: a.id, type: a.type, mc_username: a.mc_username, mc_uuid: a.mc_uuid, entitlement_ok: !!a.entitlement_ok, skin_url: a.skin_url, status: inUse ? "in use" : a.status, assigned_creator_id: assigned?.id ?? null, assigned_creator: assigned?.streamer_name ?? null, created_at: a.created_at };
  };
  app.get("/api/accounts", async (req, reply) => (admin(req, reply) ? db.all("SELECT * FROM accounts ORDER BY created_at").map(publicAccount) : undefined));
  app.post("/api/accounts/offline", async (req, reply) => {
    if (!admin(req, reply)) return;
    const name = String((req.body as any)?.username ?? "").trim();
    if (!validUsername(name)) return fail(reply, 400, "Minecraft usernames are 3-16 letters, numbers or _");
    const reserved = db.get("SELECT reason FROM reserved_names WHERE name = ?", name);
    if (reserved) return fail(reply, 409, `"${name}" is reserved (${reserved.reason}).`);
    if (db.get("SELECT id FROM accounts WHERE lower(mc_username) = lower(?)", name)) return fail(reply, 409, "An account with this username already exists");
    const id = newId("acc_");
    db.run("INSERT INTO accounts (id, type, mc_username, mc_uuid, entitlement_ok, status, created_at) VALUES (?, 'offline', ?, ?, 0, 'ready', ?)", id, name, offlineUuid(name), Date.now());
    return publicAccount(db.get("SELECT * FROM accounts WHERE id = ?", id));
  });
  const flows = new Map<string, { device: ms.DeviceCode; started: number; status: string; account_id?: string; error?: string }>();
  app.post("/api/accounts/microsoft/start", async (req, reply) => {
    if (!admin(req, reply)) return;
    const device = await ms.startDeviceCode();
    const id = newId("flow_");
    flows.set(id, { device, started: Date.now(), status: "pending" });
    // Poll Microsoft in the background until the user finishes on microsoft.com/link.
    (async () => {
      const f = flows.get(id)!;
      while (Date.now() - f.started < device.expires_in * 1000 && f.status === "pending") {
        await new Promise((r) => setTimeout(r, Math.max(5, device.interval) * 1000));
        try {
          const tok = await ms.pollDeviceCode(device.device_code);
          if (!tok) continue;
          f.status = "verifying";
          const s = await ms.minecraftFromMs(tok.access_token, tok.refresh_token);
          if (!s.entitled) throw new Error("This Microsoft account does not own Minecraft: Java Edition.");
          const existing = db.get("SELECT id FROM accounts WHERE type = 'microsoft' AND mc_uuid = ?", s.profile_id);
          const accId = existing?.id ?? newId("acc_");
          db.run(
            `INSERT INTO accounts (id, type, mc_username, mc_uuid, secret, entitlement_ok, skin_url, status, created_at) VALUES (?, 'microsoft', ?, ?, ?, 1, ?, 'ready', ?)
             ON CONFLICT(id) DO UPDATE SET mc_username = excluded.mc_username, secret = excluded.secret, skin_url = excluded.skin_url, entitlement_ok = 1`,
            accId, s.name, s.profile_id, encrypt(s), s.skin_url, Date.now(),
          );
          f.status = "done";
          f.account_id = accId;
        } catch (e: any) {
          f.status = "error";
          f.error = e.message;
        }
      }
      if (f.status === "pending") f.status = "expired";
    })();
    return { flow_id: id, user_code: device.user_code, verification_uri: device.verification_uri, expires_in: device.expires_in, message: device.message };
  });
  app.get("/api/accounts/microsoft/flow/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const f = flows.get((req.params as any).id);
    if (!f) return fail(reply, 404, "unknown sign-in flow");
    return { status: f.status, error: f.error, account: f.account_id ? publicAccount(db.get("SELECT * FROM accounts WHERE id = ?", f.account_id)) : null };
  });
  app.post("/api/accounts/:id/refresh", async (req, reply) => {
    if (!admin(req, reply)) return;
    const a = db.get("SELECT * FROM accounts WHERE id = ?", (req.params as any).id);
    if (!a) return fail(reply, 404, "not found");
    if (a.type !== "microsoft") return publicAccount(a);
    const s = await ms.ensureFresh({ ...decrypt<ms.McSession>(a.secret)!, mc_expires_at: 0 });
    db.run("UPDATE accounts SET secret = ?, mc_username = ?, skin_url = ?, entitlement_ok = ? WHERE id = ?", encrypt(s), s.name, s.skin_url, s.entitled ? 1 : 0, a.id);
    return publicAccount(db.get("SELECT * FROM accounts WHERE id = ?", a.id));
  });
  app.post("/api/accounts/:id/assign", async (req, reply) => {
    if (!admin(req, reply)) return;
    const a = db.get("SELECT * FROM accounts WHERE id = ?", (req.params as any).id);
    if (!a) return fail(reply, 404, "account not found");
    const cid = (req.body as any)?.creator_id ?? null;
    const current = creators().find((c) => c.account_id === a.id);
    if (current && current.id !== cid) {
      if (bots.get(current.id)?.state && bots.get(current.id)!.state !== "stopped") return fail(reply, 409, `${current.streamer_name} is playing on this account right now. Stop that bot first.`);
      current.account_id = null;
      saveCreator(current);
    }
    if (cid) {
      const c = creator(cid);
      if (!c) return fail(reply, 404, "creator not found");
      if (c.account_id && c.account_id !== a.id && bots.get(c.id)?.state === "online") return fail(reply, 409, "Stop this creator's bot before changing its account.");
      c.account_id = a.id;
      c.account_type = a.type;
      c.mc_username = a.mc_username;
      saveCreator(c);
    }
    return publicAccount(db.get("SELECT * FROM accounts WHERE id = ?", a.id));
  });
  app.delete("/api/accounts/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    const c = creators().find((x) => x.account_id === id);
    if (c && bots.get(c.id)?.state && bots.get(c.id)!.state !== "stopped") return fail(reply, 409, "Account is in use by a running bot");
    if (c) {
      c.account_id = null;
      saveCreator(c);
    }
    db.run("DELETE FROM accounts WHERE id = ?", id);
    return { ok: true };
  });
  app.get("/api/reserved-names", async (req, reply) => (admin(req, reply) ? db.all("SELECT * FROM reserved_names ORDER BY name") : undefined));
  app.post("/api/reserved-names", async (req, reply) => {
    if (!admin(req, reply)) return;
    const { name, reason } = req.body as any;
    if (!validUsername(String(name ?? ""))) return fail(reply, 400, "invalid name");
    db.run("INSERT OR REPLACE INTO reserved_names (name, reason, created_at) VALUES (?, ?, ?)", name, reason ?? "reserved by admin", Date.now());
    return { ok: true };
  });
  app.delete("/api/reserved-names/:name", async (req, reply) => {
    if (!admin(req, reply)) return;
    db.run("DELETE FROM reserved_names WHERE name = ?", (req.params as any).name);
    return { ok: true };
  });

  // ────────────────────────────── servers ──────────────────────────────
  const serverView = async (s: ServerConfig) => {
    const inst = servers.instances.get(s.id);
    const external = s.software === "external";
    const ping = external ? await pingServer(s.host, s.port, 2500) : null;
    return {
      ...s,
      state: external ? (ping?.online ? "online" : "unreachable") : inst?.state ?? "stopped",
      players: external ? [] : [...(inst?.players ?? [])],
      metrics: inst ? await inst.metrics() : null,
      address: servers.connectAddress(s),
      ping,
      ai_players: creators().filter((c) => c.server_id === s.id).map((c) => ({ id: c.id, name: c.streamer_name, state: bots.get(c.id)?.state ?? "stopped" })),
      last_error: inst?.lastError ?? "",
    };
  };
  app.get("/api/servers", async (req, reply) => (admin(req, reply) ? Promise.all(servers.list().map(serverView)) : undefined));
  app.get("/api/servers/versions", async (req, reply) => {
    if (!admin(req, reply)) return;
    try {
      return { versions: await listVanillaReleases(25), default: DEFAULT_VERSION };
    } catch (e: any) {
      return { versions: [DEFAULT_VERSION], default: DEFAULT_VERSION, warning: `Could not reach Mojang's version list: ${e.message}` };
    }
  });
  app.get("/api/servers/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const s = servers.get((req.params as any).id);
    return s ? serverView(s) : fail(reply, 404, "not found");
  });
  app.post("/api/servers", async (req, reply) => {
    if (!admin(req, reply)) return;
    const b = (req.body ?? {}) as any;
    if (!b.name) return fail(reply, 400, "Server name required");
    if (b.software === "external" && !b.host) return fail(reply, 400, "External servers need a host");
    if (b.gamemode === "hardcore") b.difficulty = "hard";
    const s = servers.create({ ...b, eula_accepted_at: null, whitelist: b.whitelist ?? [], ops: b.ops ?? [] });
    return serverView(s);
  });
  app.put("/api/servers/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const s = servers.get((req.params as any).id);
    if (!s) return fail(reply, 404, "not found");
    const b = { ...(req.body as any) };
    for (const k of ["id", "created_at", "eula_accepted_at"]) delete b[k];
    const next = { ...s, ...b, properties: { ...s.properties, ...(b.properties ?? {}) } };
    servers.save(next);
    return serverView(next);
  });
  app.delete("/api/servers/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    for (const c of creators().filter((c) => c.server_id === id)) await bots.stop(c.id);
    await servers.remove(id);
    return { ok: true, note: "Server definition removed. World files remain in data/servers until you delete them." };
  });
  app.post("/api/servers/:id/eula", async (req, reply) => {
    const u = admin(req, reply);
    if (!u) return;
    const s = servers.get((req.params as any).id);
    if (!s) return fail(reply, 404, "not found");
    if ((req.body as any)?.accept !== true) return fail(reply, 400, "You must explicitly accept the Minecraft EULA (https://aka.ms/MinecraftEULA).");
    s.eula_accepted_at = Date.now();
    servers.save(s);
    sysLog("info", "servers", `${u.username} accepted the Minecraft EULA for ${s.name}`);
    return { ok: true, accepted_at: s.eula_accepted_at };
  });
  for (const action of ["start", "stop", "restart"] as const) {
    app.post(`/api/servers/:id/${action}`, async (req, reply) => {
      if (!admin(req, reply)) return;
      const id = (req.params as any).id;
      const s = servers.get(id);
      if (!s) return fail(reply, 404, "not found");
      if (s.software === "external") return fail(reply, 400, "External servers are managed elsewhere");
      if (action === "stop" || action === "restart") for (const c of creators().filter((c) => c.server_id === id)) await bots.stop(c.id);
      if (action === "start") {
        servers.start(id).catch((e) => sysLog("error", `server:${id}`, e.message));
        await new Promise((r) => setTimeout(r, 300));
        const inst = servers.instances.get(id);
        if (inst?.state === "stopped" && !s.eula_accepted_at && s.software !== "custom") return fail(reply, 400, "Accept the Minecraft EULA first.");
      } else if (action === "stop") await servers.stop(id);
      else await servers.instance(id).restart();
      return serverView(servers.get(id)!);
    });
  }
  app.post("/api/servers/:id/command", async (req, reply) => {
    if (!admin(req, reply)) return;
    const line = String((req.body as any)?.line ?? "").trim();
    if (!line) return fail(reply, 400, "command required");
    servers.instance((req.params as any).id).command(line.replace(/^\//, ""));
    return { ok: true };
  });
  app.get("/api/servers/:id/console", async (req, reply) => {
    if (!admin(req, reply)) return;
    return servers.instances.get((req.params as any).id)?.console.slice(-500) ?? [];
  });
  app.post("/api/servers/:id/whitelist", async (req, reply) => {
    if (!admin(req, reply)) return;
    const name = String((req.body as any)?.name ?? "");
    if (!validUsername(name)) return fail(reply, 400, "invalid Minecraft username");
    await servers.allow((req.params as any).id, name, "human");
    return { ok: true };
  });
  app.delete("/api/servers/:id/whitelist/:name", async (req, reply) => {
    if (!admin(req, reply)) return;
    await servers.disallow((req.params as any).id, (req.params as any).name);
    return { ok: true };
  });
  app.post("/api/servers/:id/backup", async (req, reply) => {
    if (!admin(req, reply)) return;
    const file = await servers.instance((req.params as any).id).backup(config.backupsDir);
    return { ok: true, file: file.split("/").pop() };
  });
  app.get("/api/servers/:id/backups", async (req, reply) => {
    if (!admin(req, reply)) return;
    return servers.instance((req.params as any).id).listBackups(config.backupsDir);
  });
  app.post("/api/servers/:id/join-ai", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    const ids: string[] = (req.body as any)?.creator_ids ?? creators().filter((c) => c.server_id === id).map((c) => c.id);
    const results = [];
    for (const cid of ids) {
      const c = creator(cid);
      if (!c) continue;
      try {
        c.server_id = id;
        saveCreator(c);
        const s = servers.get(id)!;
        const acc = c.account_id ? db.get("SELECT mc_username FROM accounts WHERE id = ?", c.account_id) : undefined;
        if (acc && s.software !== "external" && s.auth_mode === "offline") await servers.allow(id, acc.mc_username, "bot");
        await bots.start(c, id, { owner: (req.body as any)?.owner });
        results.push({ creator: c.streamer_name, ok: true });
        await new Promise((r) => setTimeout(r, 1500)); // stagger logins
      } catch (e: any) {
        results.push({ creator: c.streamer_name, ok: false, error: e.message });
      }
    }
    return { results, address: servers.connectAddress(servers.get(id)!) };
  });
  app.post("/api/servers/:id/invite", async (req, reply) => {
    if (!admin(req, reply)) return;
    const id = (req.params as any).id;
    const name = String((req.body as any)?.mc_username ?? "");
    if (!validUsername(name)) return fail(reply, 400, "invalid Minecraft username");
    await servers.allow(id, name, "human");
    const code = randomBytes(4).toString("hex");
    db.run("INSERT INTO invitations (id, server_id, mc_username, code, created_at) VALUES (?, ?, ?, ?, ?)", newId("inv_"), id, name, code, Date.now());
    const s = servers.get(id)!;
    return { ok: true, mc_username: name, address: servers.connectAddress(s), auth_mode: s.auth_mode, note: s.auth_mode === "offline" ? "This is a private offline-mode server: identities are not verified by Mojang. Only share the address with people you trust." : "Online mode: the player must own Minecraft." };
  });
  app.get("/api/servers/:id/invitations", async (req, reply) => (admin(req, reply) ? db.all("SELECT * FROM invitations WHERE server_id = ? ORDER BY created_at DESC", (req.params as any).id) : undefined));

  // SMP generator
  app.get("/api/smp/presets", async () => PRESETS);
  app.post("/api/smp", async (req, reply) => {
    if (!admin(req, reply)) return;
    const b = (req.body ?? {}) as any;
    const pr = preset(b.preset ?? "survival");
    if (!pr) return fail(reply, 400, "unknown preset");
    if (!pr.available) return fail(reply, 400, `${pr.name} is not available: ${pr.unavailable_reason}`);
    const humans: string[] = (b.humans ?? []).filter(validUsername);
    const creatorIds: string[] = b.creator_ids ?? [];
    const botNames = creatorIds.map((id) => creator(id)).filter(Boolean).map((c) => (c!.account_id ? db.get("SELECT mc_username FROM accounts WHERE id = ?", c!.account_id)?.mc_username : null)).filter(Boolean);
    const s = servers.create({
      name: b.name || pr.name,
      version: b.version || DEFAULT_VERSION,
      software: b.software ?? "paper",
      seed: b.seed ?? "",
      max_players: b.player_limit ?? 20,
      difficulty: b.difficulty ?? (pr.properties.difficulty as any) ?? "normal",
      gamemode: pr.id === "hardcore" ? "hardcore" : "survival",
      auth_mode: b.auth_mode ?? "offline",
      smp_preset: pr.id,
      whitelist: [...humans, ...botNames],
      ops: b.ops ?? humans.slice(0, 1),
      ai_count: creatorIds.length,
      auto_start_bots: true,
      properties: { ...(b.rules?.pvp === false ? { pvp: "false" } : {}) },
      ...(b.teams?.length ? { teams: b.teams } : {}),
    } as any);
    for (const id of creatorIds) {
      const c = creator(id);
      if (c) {
        c.server_id = s.id;
        if (b.streaming?.auto_live) c.schedule = { ...c.schedule, mode: "auto_when_online" };
        saveCreator(c);
      }
    }
    return { server: await serverView(s), preset: pr, next: "Accept the EULA, start the server, then press Join AI Players." };
  });

  // ────────────────────────────── streams & MineLive ──────────────────────────────
  app.post("/api/streams/:creatorId/start", async (req, reply) => {
    if (!admin(req, reply)) return;
    return streams.start((req.params as any).creatorId, { title: (req.body as any)?.title });
  });
  app.post("/api/streams/:creatorId/stop", async (req, reply) => {
    if (!admin(req, reply)) return;
    await streams.stop((req.params as any).creatorId);
    return { ok: true };
  });
  app.get("/api/streams", async (req, reply) => (admin(req, reply) ? db.all("SELECT * FROM streams ORDER BY started_at DESC LIMIT 200") : undefined));

  const channelCard = (c: Creator) => {
    const live = streams.live.get(c.id);
    const rt = bots.get(c.id);
    const thumb = join(config.uploadsDir, "thumbs", `${c.slug}.jpg`);
    return {
      slug: c.slug,
      name: c.streamer_name,
      avatar: c.avatar,
      category: c.category,
      ai_generated: true,
      live: !!live?.ready,
      title: live?.title ?? c.stream_title,
      started_at: live?.started_at ?? null,
      viewers: live ? streams.viewerCount(c.slug) : 0,
      thumbnail: existsSync(thumb) ? `/api/thumbs/${c.slug}.jpg?t=${Math.floor(Date.now() / 60000)}` : null,
      activity: rt?.status?.goal || null,
      followers: Number(db.get("SELECT COUNT(*) AS n FROM follows WHERE creator_id = ?", c.id)?.n ?? 0),
    };
  };
  app.get("/api/live/home", async (req) => {
    const u = user(req);
    const all = creators().map(channelCard);
    const live = all.filter((c) => c.live).sort((a, b) => b.viewers - a.viewers);
    const followed = u ? new Set(db.all("SELECT creator_id FROM follows WHERE user_id = ?", u.id).map((r) => r.creator_id)) : new Set();
    const recorded = db
      .all("SELECT s.id, s.creator_id, s.title, s.started_at, s.ended_at, s.peak_viewers FROM streams s WHERE s.status = 'ended' ORDER BY s.ended_at DESC LIMIT 12")
      .map((s) => ({ ...s, creator: creator(s.creator_id)?.streamer_name, slug: creator(s.creator_id)?.slug }));
    const categories = [...new Set(all.map((c) => c.category))].map((name) => ({ name, live: all.filter((c) => c.category === name && c.live).length }));
    return {
      live,
      featured: live[0] ?? all[0] ?? null,
      channels: all,
      following: all.filter((c) => followed.has(creators().find((x) => x.slug === c.slug)?.id)),
      recommended: all.filter((c) => !c.live).slice(0, 8),
      categories,
      recorded,
    };
  });
  app.get("/api/live/search", async (req) => {
    const q = String((req.query as any).q ?? "").toLowerCase();
    return creators().filter((c) => [c.streamer_name, c.category, c.stream_title, c.persona.personality].some((f) => f?.toLowerCase().includes(q))).map(channelCard);
  });
  app.get("/api/live/channel/:slug", async (req, reply) => {
    const c = streams.findBySlug((req.params as any).slug);
    if (!c) return fail(reply, 404, "Channel not found");
    const u = user(req);
    const live = streams.live.get(c.id);
    const upcoming = (c.schedule?.entries ?? []).map((e) => ({ ...e, timezone: c.schedule.timezone }));
    const past = db.all("SELECT id, title, started_at, ended_at, peak_viewers FROM streams WHERE creator_id = ? AND status = 'ended' ORDER BY started_at DESC LIMIT 20", c.id);
    const clips = db.all("SELECT id, title, duration, created_at FROM clips WHERE creator_id = ? ORDER BY created_at DESC LIMIT 30", c.id).map((x) => ({ ...x, url: `/api/clips/${x.id}/file` }));
    const rt = bots.get(c.id);
    const server = c.server_id ? servers.get(c.server_id) : undefined;
    return {
      ...channelCard(c),
      profile: { personality: c.persona.personality, speaking_style: c.persona.speaking_style, likes: c.persona.likes, goals: c.persona.goals, friends: c.persona.friends, rivals: c.persona.rivals, skill_level: c.persona.skill_level },
      skin: c.skin,
      server: server ? { name: server.name, version: server.version } : null,
      in_game: rt?.status ? { pos: rt.status.pos, biome: rt.status.biome, health: rt.status.health, food: rt.status.food, goal: rt.status.goal, dimension: rt.status.dimension } : null,
      stream: live?.ready ? { path: live.path, whep: `/api/media/whep/${live.path}`, hls: `/api/media/hls/${live.path}/index.m3u8`, started_at: live.started_at } : null,
      schedule: upcoming,
      schedule_now: activeEntry(c) ? true : false,
      past_broadcasts: past,
      recordings: await streams.recordings(c),
      clips,
      following: u ? !!db.get("SELECT 1 FROM follows WHERE user_id = ? AND creator_id = ?", u.id, c.id) : false,
      label: "AI-generated creator. Gameplay is a real Minecraft session; commentary is generated by AI.",
    };
  });
  app.post("/api/live/:slug/follow", async (req, reply) => {
    const u = user(req);
    if (!u) return fail(reply, 401, "Log in to follow channels");
    const c = streams.findBySlug((req.params as any).slug);
    if (!c) return fail(reply, 404, "not found");
    db.run("INSERT OR IGNORE INTO follows (user_id, creator_id, created_at) VALUES (?, ?, ?)", u.id, c.id, Date.now());
    return { ok: true };
  });
  app.delete("/api/live/:slug/follow", async (req, reply) => {
    const u = user(req);
    if (!u) return fail(reply, 401, "Log in first");
    const c = streams.findBySlug((req.params as any).slug);
    if (c) db.run("DELETE FROM follows WHERE user_id = ? AND creator_id = ?", u.id, c.id);
    return { ok: true };
  });
  app.get("/api/live/notifications", async (req, reply) => {
    const u = user(req);
    if (!u) return fail(reply, 401, "Log in first");
    return db.all("SELECT creator_id FROM follows WHERE user_id = ?", u.id).map((r) => creator(r.creator_id)).filter((c) => c && streams.live.get(c.id)?.ready).map((c) => channelCard(c!));
  });
  app.delete("/api/live/:slug/messages/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    chat.remove((req.params as any).slug, Number((req.params as any).id));
    return { ok: true };
  });
  app.post("/api/clips", async (req, reply) => {
    const u = user(req);
    if (!u) return fail(reply, 401, "Log in to create clips");
    const { slug, seconds, title } = req.body as any;
    const c = streams.findBySlug(String(slug ?? ""));
    if (!c) return fail(reply, 404, "channel not found");
    return streams.clip(c, Math.min(60, Math.max(5, Number(seconds) || 30)), String(title ?? "").slice(0, 80), u.id);
  });
  app.get("/api/clips/:id/file", async (req, reply) => {
    const row = db.get("SELECT file FROM clips WHERE id = ?", (req.params as any).id);
    if (!row) return fail(reply, 404, "clip not found");
    reply.type("video/mp4");
    return readFile(join(config.clipsDir, row.file));
  });
  app.get("/api/thumbs/:file", async (req, reply) => {
    const f = join(config.uploadsDir, "thumbs", String((req.params as any).file).replace(/[^\w.-]/g, ""));
    if (!existsSync(f)) return fail(reply, 404, "no thumbnail");
    reply.type("image/jpeg");
    return readFile(f);
  });

  // Media proxies: one origin for the iPhone (HLS fallback, WebRTC WHEP signalling, playback).
  app.get("/api/media/hls/*", async (req, reply) => {
    const path = (req.params as any)["*"];
    const r = await fetch(`${config.hlsUrl}/${path}${req.url.includes("?") ? req.url.slice(req.url.indexOf("?")) : ""}`);
    reply.code(r.status).type(r.headers.get("content-type") ?? "application/octet-stream").header("Cache-Control", "no-cache");
    return Buffer.from(await r.arrayBuffer());
  });
  app.post("/api/media/whep/*", async (req, reply) => {
    const path = (req.params as any)["*"];
    const r = await fetch(`${config.whepUrl}/${path}/whep`, { method: "POST", headers: { "Content-Type": "application/sdp" }, body: String(req.body ?? "") });
    reply.code(r.status).type("application/sdp");
    const loc = r.headers.get("location");
    if (loc) reply.header("Location", `/api/media/whep-session?u=${encodeURIComponent(loc)}`);
    return r.text();
  });
  app.delete("/api/media/whep-session", async (req) => {
    const u = String((req.query as any).u ?? "");
    if (u.startsWith("/")) await fetch(`${config.whepUrl}${u}`, { method: "DELETE" }).catch(() => {});
    return { ok: true };
  });
  app.get("/api/media/playback", async (req, reply) => {
    const q = new URLSearchParams(req.query as any);
    q.set("format", "mp4");
    const r = await fetch(`${config.playbackUrl}/get?${q}`);
    reply.code(r.status).type(r.headers.get("content-type") ?? "video/mp4");
    return Buffer.from(await r.arrayBuffer());
  });

  // WebSockets
  app.register(async (scope) => {
    scope.get("/api/live/:slug/ws", { websocket: true }, (socket, req) => {
      const slug = (req.params as any).slug;
      if (!streams.findBySlug(slug)) return socket.close(4404, "channel not found");
      const u = user(req);
      chat.attach(slug, socket as any, newId("conn_"), u ? { id: u.id, username: u.username, role: u.role } : null);
    });
    scope.get("/api/ws", { websocket: true }, (socket, req) => {
      const u = user(req);
      if (!u || u.role !== "admin") return socket.close(4401, "admin login required");
      adminSockets.add(socket);
      socket.on("close", () => adminSockets.delete(socket));
    });
  });

  // ────────────────────────────── voice ──────────────────────────────
  app.get("/api/voices", async (req, reply) => (admin(req, reply) ? voices.list() : undefined));
  app.get("/api/voices/health", async (req, reply) => (admin(req, reply) ? voices.health() : undefined));
  app.post("/api/voices", async (req, reply) => {
    if (!admin(req, reply)) return;
    let body: any = {};
    let model: string | null = null;
    if (req.isMultipart()) {
      for await (const part of req.parts()) {
        if (part.type === "file") {
          const ext = (part.filename.split(".").pop() ?? "bin").replace(/[^\w]/g, "");
          if (!["pth", "onnx", "index", "zip", "wav", "bin", "json"].includes(ext)) return fail(reply, 400, "Unsupported voice model file type");
          await mkdir(join(config.uploadsDir, "voices"), { recursive: true });
          const name = `${newId("vm_")}.${ext}`;
          await writeFile(join(config.uploadsDir, "voices", name), await part.toBuffer());
          model = join(config.uploadsDir, "voices", name);
        } else body[part.fieldname] = part.value;
      }
      if (typeof body.json === "string") body = { ...JSON.parse(body.json), ...body };
    } else body = req.body ?? {};
    const consent = body.consent_owner
      ? { owner: String(body.consent_owner), statement: String(body.consent_statement ?? "I own this voice or have written permission from the speaker to use it."), accepted_at: Date.now() }
      : null;
    if (model && (!consent || String(body.consent_confirm) !== "true")) return fail(reply, 400, "Uploading a voice model requires confirming you own the voice or have the speaker's consent.");
    const v: VoiceProfile = {
      id: newId("vc_"),
      name: String(body.name || "Custom voice"),
      engine: ["espeak", "piper", "http"].includes(body.engine) ? body.engine : "espeak",
      preset: String(body.preset ?? "en-us"),
      rate: Number(body.rate ?? 1),
      pitch: Number(body.pitch ?? 0),
      volume: Number(body.volume ?? 1),
      endpoint: String(body.endpoint ?? ""),
      model_file: model,
      rvc: body.rvc_endpoint ? { endpoint: String(body.rvc_endpoint), model_file: model } : null,
      consent,
      original: !model,
      created_at: Date.now(),
    };
    voices.save(v);
    return v;
  });
  app.put("/api/voices/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    const v = voices.get((req.params as any).id);
    if (!v) return fail(reply, 404, "not found");
    const b = { ...(req.body as any) };
    delete b.id;
    delete b.model_file;
    delete b.consent;
    voices.save({ ...v, ...b });
    return voices.get(v.id);
  });
  app.delete("/api/voices/:id", async (req, reply) => {
    if (!admin(req, reply)) return;
    db.delDoc("voices", (req.params as any).id);
    return { ok: true };
  });
  app.post("/api/voices/test", async (req, reply) => {
    if (!admin(req, reply)) return;
    const { voice_id, text, emotion } = req.body as any;
    const pcm = await voices.synthesize(String(text || "Okay chat, I think we should add a second floor to this thing."), voices.get(voice_id), { emotion });
    // Wrap PCM in a WAV header for the browser.
    const header = Buffer.alloc(44);
    header.write("RIFF", 0);
    header.writeUInt32LE(36 + pcm.length, 4);
    header.write("WAVEfmt ", 8);
    header.writeUInt32LE(16, 16);
    header.writeUInt16LE(1, 20);
    header.writeUInt16LE(1, 22);
    header.writeUInt32LE(24000, 24);
    header.writeUInt32LE(48000, 28);
    header.writeUInt16LE(2, 32);
    header.writeUInt16LE(16, 34);
    header.write("data", 36);
    header.writeUInt32LE(pcm.length, 40);
    reply.type("audio/wav");
    return Buffer.concat([header, pcm]);
  });

  // Simulated chat participants (private testing only; always labelled).
  if (config.simulatedChat) {
    setInterval(() => {
      for (const info of streams.live.values()) {
        const c = creator(info.creator_id);
        const st = bots.get(info.creator_id)?.status;
        if (!c || !st) continue;
        const lines = [`what are you doing right now?`, `nice ${st.biome ?? "spot"}!`, `how many hearts do you have?`, `${c.streamer_name} where are you?`];
        const text = lines[Math.floor(Math.random() * lines.length)];
        chat.post(c.slug, `[SIM] tester${Math.floor(Math.random() * 90 + 10)}`, text, "simulated");
        try {
          bots.send(c.id, { cmd: "viewer_chat", user: "sim_tester", text });
        } catch {
          /* offline */
        }
      }
    }, 30_000);
  }

  // ────────────────────────────── web app ──────────────────────────────
  if (existsSync(config.webDist)) {
    await app.register(fastifyStatic, { root: config.webDist, wildcard: false, setHeaders: (res: any, p: string) => { if (p.endsWith("sw.js")) res.setHeader("Cache-Control", "no-cache"); } });
    app.setNotFoundHandler(async (req, reply) => {
      if (req.url.startsWith("/api/")) return fail(reply, 404, "Not found");
      reply.type("text/html");
      return readFile(join(config.webDist, "index.html"));
    });
  } else {
    app.get("/", async () => ({ ok: true, message: "OmniPlayer API is running. Build the web app with `npm run build:web`." }));
  }

  app.get("/api/health", async () => ({ ok: true, version: "0.1.0", bots: bots.running().length, uptime_s: Math.round(process.uptime()) }));

  const shutdown = async () => {
    scheduler.stop();
    for (const id of [...streams.live.keys()]) await streams.stop(id).catch(() => {});
    await bots.stopAll();
    await servers.stopAll();
    await streams.stopMediaMtx();
    await app.close();
  };
  return { app, db, servers, bots, streams, chat, voices, scheduler, shutdown };
}

// Entry point
if (import.meta.url === `file://${process.argv[1]}`) {
  const ctx = await buildApp({ logger: process.env.OMNI_LOG === "1" });
  await ctx.streams.startMediaMtx();
  ctx.scheduler.start();
  await ctx.app.listen({ host: config.host, port: config.port });
  console.log(`OmniPlayer listening on http://${config.host}:${config.port}`);
  for (const sig of ["SIGINT", "SIGTERM"]) {
    process.once(sig, async () => {
      console.log("Shutting down: stopping bots, streams and servers...");
      await ctx.shutdown();
      process.exit(0);
    });
  }
}

// Silence unused-import lints for helpers kept for API completeness.
void stat;
void createHash;
void compatible;
