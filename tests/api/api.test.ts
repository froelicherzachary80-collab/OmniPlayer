// End-to-end API test: real HTTP API → server manager → real server process → real bot process.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..", "..");
process.env.OMNI_DATA_DIR = mkdtempSync(join(tmpdir(), "omni-api-"));
process.env.OMNI_ADMIN_PASSWORD = "test-password-123";
// The Minestom test server is compiled for Java 25.
process.env.JAVA_HOME = process.env.JAVA_HOME_25 ?? "/usr/lib/jvm/java-25-openjdk-amd64";
process.env.OMNI_MEDIAMTX_MANAGED = "false";
process.env.OMNI_WEB_DIST = join(ROOT, "apps/web/dist-none");
const TEST_SERVER = join(ROOT, "tests/test-server/build/install/omniplayer-test-server/bin/omniplayer-test-server");
const PORT = 26000 + Math.floor(Math.random() * 1000);

const { buildApp } = await import("../../apps/api/src/main.ts");
let ctx: Awaited<ReturnType<typeof buildApp>>;
let base = "";
let token = "";

async function api(method: string, path: string, body?: unknown, auth = true) {
  const r = await fetch(base + path, {
    method,
    headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(auth && token ? { Authorization: `Bearer ${token}` } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await r.text();
  let json: any;
  try { json = JSON.parse(text); } catch { json = text; }
  return { status: r.status, json };
}

async function until<T>(fn: () => Promise<T | undefined | false>, ms: number, what: string): Promise<T> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = await fn();
    if (v) return v as T;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`timed out waiting for ${what}`);
}

before(async () => {
  ctx = await buildApp();
  await ctx.app.listen({ host: "127.0.0.1", port: 0 });
  const addr = ctx.app.server.address() as any;
  base = `http://127.0.0.1:${addr.port}`;
});
after(async () => {
  await ctx.shutdown();
});

test("admin auth is required and login works", async () => {
  assert.equal((await api("GET", "/api/creators", undefined, false)).status, 401);
  assert.equal((await api("POST", "/api/auth/login", { username: "admin", password: "wrong" }, false)).status, 401);
  const r = await api("POST", "/api/auth/login", { username: "admin", password: "test-password-123" }, false);
  assert.equal(r.status, 200);
  token = r.json.token;
  assert.equal((await api("GET", "/api/auth/me")).json.role, "admin");
});

test("offline account gets the vanilla offline UUID; duplicates and bad names are rejected", async () => {
  const r = await api("POST", "/api/accounts/offline", { username: "NovaCraft" });
  assert.equal(r.status, 200);
  assert.equal(r.json.mc_uuid, "57648c4a-7a2c-396b-989c-611e6abb621e");
  assert.equal((await api("POST", "/api/accounts/offline", { username: "novacraft" })).status, 409);
  assert.equal((await api("POST", "/api/accounts/offline", { username: "no spaces!" })).status, 400);
});

test("creator generator produces an original persona", async () => {
  const r = await api("POST", "/api/creators/generate", { prompt: "Create a chaotic but talented Minecraft survival streamer who loves PvP, always jokes around, gets distracted by diamonds, and frequently collaborates with friends." });
  assert.equal(r.status, 200);
  assert.ok(r.json.streamer_name.length >= 3);
  assert.ok(r.json.traits.includes("pvp") && r.json.traits.includes("diamonds") && r.json.traits.includes("chaotic"));
  assert.match(r.json.persona.personality, /AI-generated/);
});

test("server EULA gate: vanilla server refuses to start before acceptance", async () => {
  const s = await api("POST", "/api/servers", { name: "EULA test", software: "vanilla", version: "26.3", port: PORT + 1 });
  const r = await api("POST", `/api/servers/${s.json.id}/start`);
  assert.equal(r.status, 400);
  assert.match(JSON.stringify(r.json), /EULA/);
});

test("compatibility: offline account cannot join an online-mode server", async () => {
  const s = await api("POST", "/api/servers", { name: "Online", software: "external", host: "127.0.0.1", port: 1, auth_mode: "online" });
  const acc = (await api("GET", "/api/accounts")).json.find((a: any) => a.mc_username === "NovaCraft");
  const c = await api("POST", "/api/creators", { streamer_name: "CheckBot", server_id: s.json.id });
  await api("POST", `/api/accounts/${acc.id}/assign`, { creator_id: c.json.id });
  const chk = await api("GET", `/api/creators/${c.json.id}/check?server_id=${s.json.id}`);
  assert.equal(chk.json.ok, false);
  assert.match(chk.json.problems.join(" "), /offline accounts cannot join/);
  await api("POST", `/api/accounts/${acc.id}/assign`, { creator_id: null });
  await api("DELETE", `/api/creators/${c.json.id}`);
});

test("full flow: start a server, start the bot, bot joins and chats, stop the bot", { skip: !existsSync(TEST_SERVER) && "test server not built" }, async () => {
  const s = await api("POST", "/api/servers", { name: "Test SMP", software: "custom", custom_command: `${TEST_SERVER} {port}`, port: PORT, auth_mode: "offline" });
  assert.equal(s.status, 200);
  const sid = s.json.id;
  await api("POST", `/api/servers/${sid}/start`);
  await until(async () => (await api("GET", `/api/servers/${sid}`)).json.state === "running", 90_000, "server running");

  const acc = (await api("GET", "/api/accounts")).json.find((a: any) => a.mc_username === "NovaCraft");
  const c = await api("POST", "/api/creators", { streamer_name: "NovaCraft", mode: "idle", server_id: sid });
  assert.equal((await api("POST", `/api/accounts/${acc.id}/assign`, { creator_id: c.json.id })).status, 200);
  const chk = await api("GET", `/api/creators/${c.json.id}/check?server_id=${sid}`);
  assert.equal(chk.json.ok, true, JSON.stringify(chk.json));

  const st = await api("POST", `/api/creators/${c.json.id}/start`, { server_id: sid });
  assert.equal(st.status, 200, JSON.stringify(st.json));
  const rt = await until(async () => {
    const r = (await api("GET", `/api/creators/${c.json.id}/runtime`)).json;
    return r.state === "online" && r.status?.chunks_loaded > 10 ? r : false;
  }, 60_000, "bot online");
  assert.equal(rt.status.username, "NovaCraft");
  const console1 = (await api("GET", `/api/servers/${sid}/console`)).json.join("\n");
  assert.match(console1, /\[TEST\] JOIN NovaCraft 57648c4a-7a2c-396b-989c-611e6abb621e/);

  // The same account cannot be used by a second creator while in use.
  const c2 = await api("POST", "/api/creators", { streamer_name: "Imposter", server_id: sid });
  assert.equal((await api("POST", `/api/accounts/${acc.id}/assign`, { creator_id: c2.json.id })).status, 409);

  assert.equal((await api("POST", `/api/creators/${c.json.id}/command`, { cmd: "chat", text: "hello from the dashboard" })).status, 200);
  await until(async () => (await api("GET", `/api/servers/${sid}/console`)).json.some((l: string) => l.includes("CHAT NovaCraft hello from the dashboard")), 15_000, "chat on server");

  await api("POST", `/api/creators/${c.json.id}/stop`);
  await until(async () => (await api("GET", `/api/servers/${sid}/console`)).json.some((l: string) => l.includes("LEAVE NovaCraft")), 15_000, "bot left");
  const mem = await api("GET", `/api/creators/${c.json.id}/memory`);
  assert.ok(mem.json.sessions.length >= 1 && mem.json.sessions[0].ended_at);

  await api("POST", `/api/servers/${sid}/stop`);
  await until(async () => (await api("GET", `/api/servers/${sid}`)).json.state === "stopped", 30_000, "server stopped");
});

test("MineLive public endpoints and viewer registration", async () => {
  const home = await api("GET", "/api/live/home", undefined, false);
  assert.equal(home.status, 200);
  assert.ok(Array.isArray(home.json.channels));
  const reg = await api("POST", "/api/auth/register", { username: "viewer_one", password: "longenough1" }, false);
  assert.equal(reg.status, 200);
  const ch = await api("GET", "/api/live/channel/novacraft", undefined, false);
  assert.equal(ch.status, 200);
  assert.equal(ch.json.ai_generated, true);
  assert.equal(ch.json.live, false);
  assert.equal(ch.json.viewers, 0);
});
