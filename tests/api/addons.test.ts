// Plugins/mods/Geyser/Fabric installers against a local mock of the public APIs.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer, type Server } from "node:http";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const jar = (name: string) => Buffer.concat([Buffer.from("PK\x03\x04"), Buffer.from(name)]);
const sha512 = (b: Buffer) => createHash("sha512").update(b).digest("hex");
const sha256 = (b: Buffer) => createHash("sha256").update(b).digest("hex");
const files: Record<string, Buffer> = {
  "/files/luckperms.jar": jar("luckperms"),
  "/files/lib.jar": jar("lib"),
  "/files/fabric-api.jar": jar("fabric-api"),
  "/files/content-mod.jar": jar("content"),
  "/geyser.jar": jar("geyser"),
  "/floodgate.jar": jar("floodgate"),
  "/fabric-server.jar": jar("fabric-launcher"),
};
let srv: Server;
let base = "";
const requested: string[] = [];

before(async () => {
  srv = createServer((req, res) => {
    const u = new URL(req.url!, "http://x");
    requested.push(u.pathname + u.search);
    const json = (o: unknown) => { res.setHeader("Content-Type", "application/json"); res.end(JSON.stringify(o)); };
    const fileVersion = (path: string, deps: any[] = []) => [{ version_type: "release", files: [{ primary: true, filename: path.split("/").pop(), url: base + path, hashes: { sha512: sha512(files[path]) } }], dependencies: deps }];
    const p = u.pathname;
    if (files[p]) return res.end(files[p]);
    if (p === "/modrinth/search") return json({ hits: [{ slug: "luckperms", title: "LuckPerms", description: "Permissions", downloads: 10, server_side: "required", client_side: "unsupported" }] });
    if (p === "/modrinth/project/luckperms") return json({ title: "LuckPerms", client_side: "unsupported" });
    if (p === "/modrinth/project/luckperms/version") return json(fileVersion("/files/luckperms.jar", [{ dependency_type: "required", project_id: "LIBID" }, { dependency_type: "optional", project_id: "NOPE" }]));
    if (p === "/modrinth/project/LIBID") return json({ title: "Lib", client_side: "unsupported" });
    if (p === "/modrinth/project/LIBID/version") return json(fileVersion("/files/lib.jar"));
    if (p === "/modrinth/project/content-mod") return json({ title: "Content Mod", client_side: "required" });
    if (p === "/modrinth/project/content-mod/version") return json(fileVersion("/files/content-mod.jar", [{ dependency_type: "required", project_id: "fabric-api" }]));
    if (p === "/modrinth/project/fabric-api") return json({ title: "Fabric API", client_side: "optional" });
    if (p === "/modrinth/project/fabric-api/version") return json(fileVersion("/files/fabric-api.jar"));
    if (p === "/modrinth/project/oldmod/version") return json([]);
    if (p === "/modrinth/project/oldmod") return json({ title: "Old Mod" });
    const g = p.match(/^\/geyser\/projects\/(geyser|floodgate)\/versions\/latest\/builds\/latest(\/downloads\/(spigot|fabric))?$/);
    if (g && !g[2]) return json({ downloads: { spigot: { sha256: sha256(files[`/${g[1]}.jar`]) }, fabric: { sha256: sha256(files[`/${g[1]}.jar`]) } } });
    if (g) return res.end(files[`/${g[1]}.jar`]);
    if (p === "/fabric/versions/loader/26.3") return json([{ loader: { version: "0.18.0", stable: true } }]);
    if (p === "/fabric/versions/installer") return json([{ version: "1.1.0", stable: true }]);
    if (p === "/fabric/versions/loader/26.3/0.18.0/1.1.0/server/jar") return res.end(files["/fabric-server.jar"]);
    res.statusCode = 404;
    res.end("{}");
  });
  await new Promise<void>((r) => srv.listen(0, "127.0.0.1", () => r()));
  base = `http://127.0.0.1:${(srv.address() as any).port}`;
  process.env.OMNI_MODRINTH_API = `${base}/modrinth`;
  process.env.OMNI_GEYSER_API = `${base}/geyser`;
  process.env.OMNI_FABRIC_META = `${base}/fabric`;
});
after(() => srv.close());

const load = () => import("../../services/server-manager/src/addons.ts");

test("Modrinth search uses loader/version facets", async () => {
  const { searchModrinth } = await load();
  const hits = await searchModrinth("perms", "paper", "26.3");
  assert.equal(hits[0].slug, "luckperms");
  const q = decodeURIComponent(requested.find((r) => r.startsWith("/modrinth/search"))!);
  assert.match(q, /categories:paper/);
  assert.match(q, /versions:26\.3/);
  assert.match(q, /project_type:plugin/);
});

test("installs a plugin plus its required dependencies with checksum verification", async () => {
  const { installModrinth, listAddons } = await load();
  const dir = join(mkdtempSync(join(tmpdir(), "omni-addons-")), "plugins");
  const r = await installModrinth("luckperms", "paper", "26.3", dir);
  assert.deepEqual(r.installed, ["luckperms.jar", "lib.jar"]);
  assert.deepEqual((await listAddons(dir)).map((f) => f.file), ["lib.jar", "luckperms.jar"]);
  assert.ok(!requested.some((x) => x.includes("NOPE")), "optional dependencies are skipped");
});

test("warns when a Fabric mod is required on players' clients", async () => {
  const { installModrinth } = await load();
  const dir = join(mkdtempSync(join(tmpdir(), "omni-mods-")), "mods");
  const r = await installModrinth("content-mod", "fabric", "26.3", dir);
  assert.deepEqual(r.installed, ["content-mod.jar", "fabric-api.jar"]);
  assert.match(r.warnings[0], /every player's game client/);
});

test("refuses projects without a build for the version", async () => {
  const { installModrinth } = await load();
  await assert.rejects(installModrinth("oldmod", "paper", "26.3", mkdtempSync(join(tmpdir(), "omni-x-"))), /no paper build for Minecraft 26\.3/);
});

test("Geyser + Floodgate install for iPhone/Bedrock players, with config", async () => {
  const { installGeyser } = await load();
  const dir = mkdtempSync(join(tmpdir(), "omni-geyser-"));
  const out = await installGeyser(dir, "paper", 19133);
  assert.deepEqual(out, ["Geyser-spigot.jar", "floodgate-spigot.jar"]);
  const cfg = readFileSync(join(dir, "plugins", "Geyser-Spigot", "config.yml"), "utf8");
  assert.match(cfg, /port: 19133/);
  assert.match(cfg, /auth-type: floodgate/);
});

test("Fabric server launcher download", async () => {
  const { downloadFabric } = await load();
  const dest = join(mkdtempSync(join(tmpdir(), "omni-fabric-")), "server.jar");
  const r = await downloadFabric("26.3", dest);
  assert.equal(r.loader, "0.18.0");
  assert.ok(existsSync(dest));
});

test("uploads must be real .jar files with safe names", async () => {
  const { saveUpload } = await load();
  const dir = mkdtempSync(join(tmpdir(), "omni-up-"));
  assert.equal(await saveUpload(dir, "MyPlugin-1.0.jar", jar("x")), "MyPlugin-1.0.jar");
  await assert.rejects(saveUpload(dir, "../evil.jar", jar("x")));
  await assert.rejects(saveUpload(dir, "notes.txt", jar("x")));
  await assert.rejects(saveUpload(dir, "fake.jar", Buffer.from("hello")));
});
