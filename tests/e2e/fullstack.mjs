// Full-stack check in a real browser engine (Chromium with iPhone emulation):
// VoiceBox + API (managing MediaMTX) + a live Minecraft server + two AI bots.
// Usage: node tests/e2e/fullstack.mjs [screenshotDir]
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium, devices } from "playwright-core";

const ROOT = resolve(import.meta.dirname, "..", "..");
const SHOTS = resolve(process.argv[2] ?? join(ROOT, "docs", "screenshots"));
mkdirSync(SHOTS, { recursive: true });
const DATA = mkdtempSync(join(tmpdir(), "omni-e2e-"));
const API_PORT = 18080;
const MC_PORT = 25700 + Math.floor(Math.random() * 200);
const BASE = `http://127.0.0.1:${API_PORT}`;
const results = [];
const check = (name, ok, detail = "") => {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`);
};
const procs = [];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function start(cmd, args, env = {}) {
  const p = spawn(cmd, args, { cwd: ROOT, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.stdout.on("data", (d) => process.env.E2E_VERBOSE && process.stdout.write(d));
  p.stderr.on("data", (d) => process.env.E2E_VERBOSE && process.stderr.write(d));
  procs.push(p);
  return p;
}

let token = "";
async function api(method, path, body) {
  const r = await fetch(BASE + path, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await r.text();
  let j;
  try { j = JSON.parse(t); } catch { j = t; }
  if (!r.ok) throw new Error(`${method} ${path}: ${r.status} ${t}`);
  return j;
}
async function until(fn, ms, what) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try { const v = await fn(); if (v) return v; } catch { /* retry */ }
    await sleep(700);
  }
  throw new Error(`timeout: ${what}`);
}

try {
  start("python3", [join(ROOT, "services/voicebox/voicebox.py")], { VOICEBOX_PORT: "18095" });
  start("node", ["apps/api/src/main.ts"], {
    OMNI_PORT: String(API_PORT), OMNI_DATA_DIR: DATA, OMNI_ADMIN_PASSWORD: "e2e-password-1", OMNI_VOICEBOX_URL: "http://127.0.0.1:18095",
    OMNI_PUBLIC_HOST: "192.168.1.50", JAVA_HOME: "/usr/lib/jvm/java-25-openjdk-amd64",
  });
  await until(() => fetch(`${BASE}/api/health`).then((r) => r.ok), 30000, "api up");
  token = (await api("POST", "/api/auth/login", { username: "admin", password: "e2e-password-1" })).token;

  const testServer = join(ROOT, "tests/test-server/build/install/omniplayer-test-server/bin/omniplayer-test-server");
  const srv = await api("POST", "/api/servers", { name: "Creator SMP", software: "custom", custom_command: `${testServer} {port}`, port: MC_PORT, auth_mode: "offline" });
  await api("POST", `/api/servers/${srv.id}/start`);
  await until(async () => (await api("GET", `/api/servers/${srv.id}`)).state === "running", 90000, "server running");
  check("server manager starts a real server process", true, `port ${MC_PORT}`);

  const made = [];
  for (const [name, prompt] of [["NovaCraft", "friendly adventurous survival explorer who loves caves and collaborating with friends"], ["EmberPvP", "competitive PvP specialist who enjoys risky fights and dramatic commentary"]]) {
    const acc = await api("POST", "/api/accounts/offline", { username: name });
    const g = await api("POST", "/api/creators/generate", { prompt });
    const c = await api("POST", "/api/creators", { streamer_name: name, mc_username: name, persona: g.persona, mode: "idle", avatar: g.avatar, category: g.category, stream_title: `${name} live on the Creator SMP`, server_id: srv.id });
    await api("POST", `/api/accounts/${acc.id}/assign`, { creator_id: c.id });
    await api("POST", `/api/creators/${c.id}/skin/generate`);
    made.push(c);
  }
  const join_ = await api("POST", `/api/servers/${srv.id}/join-ai`, { creator_ids: made.map((c) => c.id) });
  check("join AI players", join_.results.every((r) => r.ok), JSON.stringify(join_.results));
  for (const c of made) await until(async () => { const r = await api("GET", `/api/creators/${c.id}/runtime`); return r.state === "online" && r.status?.chunks_loaded > 10; }, 60000, `${c.streamer_name} online`);
  const console1 = (await api("GET", `/api/servers/${srv.id}/console`)).join("\n");
  check("both bots visible to the server", /JOIN NovaCraft/.test(console1) && /JOIN EmberPvP/.test(console1));

  await api("POST", `/api/creators/${made[0].id}/command`, { cmd: "walk", ticks: 30, yaw: 0 });
  const live = await api("POST", `/api/streams/${made[0].id}/start`, {});
  check("go live only after MediaMTX reports the path ready", live.ready === true, live.path);

  const browser = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium-1194/chrome-linux/chrome", args: ["--autoplay-policy=no-user-gesture-required"] });
  const iphone = devices["iPhone 15 Pro"] ?? devices["iPhone 13"];
  const ctx = await browser.newContext({ ...iphone, baseURL: BASE });
  await ctx.addInitScript((t) => localStorage.setItem("omniplayer.token", t), token);
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page.goto("/admin");
  await page.getByText("AI players online").waitFor();
  await sleep(1500);
  await page.screenshot({ path: join(SHOTS, "iphone-overview.png") });
  check("dashboard overview renders on iPhone viewport", true);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  check("no horizontal page scroll on phone width", overflow <= 1, `overflow ${overflow}px`);

  await page.goto("/admin/play");
  await page.getByText("Join from your Minecraft game").waitFor();
  await page.getByText(`:${MC_PORT}`).first().waitFor({ timeout: 15000 });
  const addr = await page.getByText(`:${MC_PORT}`).first().innerText();
  check("Play page shows the server address", addr.includes(String(MC_PORT)), addr);
  await page.screenshot({ path: join(SHOTS, "iphone-play.png"), fullPage: true });

  await page.goto(`/admin/creators/${made[0].id}`);
  await page.getByText("In game (live)").waitFor({ timeout: 15000 });
  await page.screenshot({ path: join(SHOTS, "iphone-creator.png"), fullPage: true });
  check("creator page shows live in-game state", true);

  // Dashboard button → bot action → server sees it.
  await page.getByPlaceholder("Say in Minecraft chat…").fill("hi from my iPhone");
  await page.getByRole("button", { name: "Send" }).click();
  await until(async () => (await api("GET", `/api/servers/${srv.id}/console`)).some((l) => l.includes("CHAT NovaCraft hi from my iPhone")), 15000, "chat from dashboard");
  check("iPhone dashboard chat reaches the Minecraft server", true);

  await page.goto(`/admin/servers/${srv.id}`);
  await page.getByText("Connect").first().waitFor();
  await page.screenshot({ path: join(SHOTS, "iphone-server.png"), fullPage: true });

  // MineLive as a viewer.
  const viewer = await fetch(`${BASE}/api/auth/register`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ username: "phone_viewer", password: "viewerpass1" }) }).then((r) => r.json());
  const vctx = await browser.newContext({ ...iphone, baseURL: BASE });
  await vctx.addInitScript((t) => localStorage.setItem("omniplayer.token", t), viewer.token);
  const vp = await vctx.newPage();
  vp.on("pageerror", (e) => errors.push(e.message));
  await vp.goto("/live");
  await vp.getByText("Live channels").waitFor({ timeout: 15000 });
  await sleep(3000);
  await vp.screenshot({ path: join(SHOTS, "iphone-minelive-home.png") });
  await vp.goto(`/live/creator/novacraft`);
  vp.on("console", (m) => process.env.E2E_VERBOSE && console.log("[browser]", m.text()));
  const caps = await vp.evaluate(() => ({
    webrtcH264: RTCRtpReceiver.getCapabilities("video").codecs.some((c) => c.mimeType === "video/H264"),
    mseH264: typeof MediaSource !== "undefined" && MediaSource.isTypeSupported('video/mp4; codecs="avc1.42E01E"'),
    nativeHls: !!document.createElement("video").canPlayType("application/vnd.apple.mpegurl"),
  }));
  if (caps.webrtcH264 || caps.mseH264 || caps.nativeHls) {
    const playing = await until(() => vp.evaluate(() => {
      const v = document.querySelector("video");
      return v && v.videoWidth > 0 && v.currentTime > 1 ? { w: v.videoWidth, h: v.videoHeight } : null;
    }), 40000, "video playing");
    check("MineLive player plays the live POV in the browser", playing.w === 1280 && playing.h === 720, JSON.stringify(playing));
    const t1 = await vp.evaluate(() => document.querySelector("video").currentTime);
    await sleep(3000);
    const t2 = await vp.evaluate(() => document.querySelector("video").currentTime);
    check("live video keeps advancing", t2 - t1 > 2, `${(t2 - t1).toFixed(2)}s in 3s`);
  } else {
    // This browser build cannot decode H.264 at all (open-source Chromium). Verify the
    // stream is reachable through the WHEP proxy and the page degrades gracefully.
    const whep = await vp.evaluate(async () => {
      const pc = new RTCPeerConnection();
      pc.addTransceiver("video", { direction: "recvonly" });
      await pc.setLocalDescription(await pc.createOffer());
      const r = await fetch("/api/media/whep/live/novacraft", { method: "POST", headers: { "Content-Type": "application/sdp" }, body: pc.localDescription.sdp });
      return r.status + " " + (await r.text());
    });
    check("WHEP proxy reaches the live stream (server reports codec mismatch for this browser)", /codecs not supported by client/.test(whep), whep.slice(0, 90));
    await vp.getByText(/cannot play|unavailable/).first().waitFor({ timeout: 30000 });
    check("player falls back gracefully without H.264", true);
    console.log(`SKIP in-browser video decode: this Chromium build lacks H.264 (${JSON.stringify(caps)}); iPhone Safari decodes it natively. Frame delivery is proven by tests/integration/test_streaming.py.`);
  }
  const viewersText = await vp.getByText(/watching/).first().innerText();
  check("viewer count reflects real connections", /1 watching/.test(viewersText), viewersText);

  await vp.getByPlaceholder("Send a message").fill("NovaCraft where are you?");
  await vp.getByRole("button", { name: "Chat" }).click();
  await vp.getByText("AI CREATOR").first().waitFor({ timeout: 20000 });
  const reply = await vp.locator(".bg-brand\\/10").first().innerText();
  check("AI creator answers viewer chat from real game state", /I'm at -?\d+ -?\d+ -?\d+/.test(reply), reply.replace(/\s+/g, " "));
  await sleep(1500);
  await vp.screenshot({ path: join(SHOTS, "iphone-minelive-channel.png") });
  // Grab a frame of the live POV exactly as a viewer receives it (RTSP from MediaMTX).
  await new Promise((res) => spawn("ffmpeg", ["-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i", "rtsp://127.0.0.1:8554/live/novacraft", "-frames:v", "1", "-y", join(SHOTS, "live-pov-frame.png")]).on("exit", res));
  check("captured a frame of the live POV from the stream server", (await import("node:fs")).statSync(join(SHOTS, "live-pov-frame.png")).size > 10000);

  check("no JavaScript errors in the pages", errors.length === 0, errors.join(" | "));
  await browser.close();
  await api("POST", `/api/streams/${made[0].id}/stop`);
} catch (e) {
  check("e2e run", false, e.message);
} finally {
  for (const p of procs.reverse()) p.kill("SIGTERM");
  await sleep(4000);
  for (const p of procs) if (p.exitCode === null) p.kill("SIGKILL");
  const failed = results.filter((r) => !r.ok);
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  process.exit(failed.length ? 1 : 0);
}
