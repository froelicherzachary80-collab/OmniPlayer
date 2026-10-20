// Plugins and mods: Modrinth installs (with required dependencies), uploads,
// Geyser + Floodgate (lets iPhone/Bedrock players join Java servers) and the
// Fabric mod loader.

import { createHash } from "node:crypto";
import { mkdir, readdir, rm, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";

const MODRINTH = process.env.OMNI_MODRINTH_API ?? "https://api.modrinth.com/v2";
const GEYSER = process.env.OMNI_GEYSER_API ?? "https://download.geysermc.org/v2";
const FABRIC_META = process.env.OMNI_FABRIC_META ?? "https://meta.fabricmc.net/v2";
const UA = "OmniPlayer/0.1 (github.com/froelicherzachary80-collab/OmniPlayer)";

export type Loader = "paper" | "fabric";

export function loaderFor(software: string): Loader | null {
  if (software === "paper") return "paper";
  if (software === "fabric") return "fabric";
  return null;
}

/** Plugins live in plugins/ (Paper), mods in mods/ (Fabric). */
export function addonDir(serverDir: string, loader: Loader): string {
  return join(serverDir, loader === "paper" ? "plugins" : "mods");
}

async function getJson(url: string): Promise<any> {
  const r = await fetch(url, { headers: { "User-Agent": UA } });
  if (!r.ok) throw new Error(`GET ${url} → HTTP ${r.status}`);
  return r.json();
}

async function downloadTo(url: string, file: string, sha512?: string): Promise<number> {
  const r = await fetch(url, { headers: { "User-Agent": UA } });
  if (!r.ok) throw new Error(`download ${url} → HTTP ${r.status}`);
  const buf = Buffer.from(await r.arrayBuffer());
  if (sha512) {
    const h = createHash("sha512").update(buf).digest("hex");
    if (h !== sha512) throw new Error(`checksum mismatch for ${file}`);
  }
  await writeFile(file, buf);
  return buf.length;
}

export interface AddonFile {
  file: string;
  size: number;
  modified: number;
}

export async function listAddons(dir: string): Promise<AddonFile[]> {
  try {
    const out: AddonFile[] = [];
    for (const f of await readdir(dir)) {
      if (!f.endsWith(".jar")) continue;
      const s = await stat(join(dir, f));
      out.push({ file: f, size: s.size, modified: s.mtimeMs });
    }
    return out.sort((a, b) => a.file.localeCompare(b.file));
  } catch {
    return [];
  }
}

export function safeJarName(name: string): string {
  if (/[\\/]/.test(name) || name.includes("..") || !/^[\w.+-]{1,120}\.jar$/.test(name)) {
    throw new Error("Only .jar files with simple names are allowed");
  }
  return name;
}

export async function saveUpload(dir: string, name: string, data: Buffer): Promise<string> {
  const file = safeJarName(name);
  if (data.subarray(0, 2).toString("latin1") !== "PK") throw new Error("That file is not a valid .jar (zip) archive");
  await mkdir(dir, { recursive: true });
  await writeFile(join(dir, file), data);
  return file;
}

export async function removeAddon(dir: string, name: string): Promise<void> {
  await rm(join(dir, safeJarName(name)), { force: true });
}

export interface SearchHit {
  slug: string;
  title: string;
  description: string;
  downloads: number;
  icon_url: string | null;
  server_side: string;
  client_side: string;
}

/** Searches Modrinth for plugins (Paper) or mods (Fabric) that support the version. */
export async function searchModrinth(query: string, loader: Loader, mcVersion: string, limit = 20): Promise<SearchHit[]> {
  const facets = JSON.stringify([[`categories:${loader}`], [`versions:${mcVersion}`], [`project_type:${loader === "paper" ? "plugin" : "mod"}`]]);
  const j = await getJson(`${MODRINTH}/search?query=${encodeURIComponent(query)}&limit=${limit}&facets=${encodeURIComponent(facets)}`);
  return (j.hits ?? []).map((h: any) => ({
    slug: h.slug,
    title: h.title,
    description: h.description,
    downloads: h.downloads,
    icon_url: h.icon_url ?? null,
    server_side: h.server_side ?? "unknown",
    client_side: h.client_side ?? "unknown",
  }));
}

/**
 * Installs a Modrinth project and its required dependencies.
 * Returns the files written. Warns when a mod also has to be installed on players' clients.
 */
export async function installModrinth(slugOrId: string, loader: Loader, mcVersion: string, dir: string, seen = new Set<string>()): Promise<{ installed: string[]; warnings: string[] }> {
  const installed: string[] = [];
  const warnings: string[] = [];
  if (seen.has(slugOrId)) return { installed, warnings };
  seen.add(slugOrId);
  const loaders = loader === "paper" ? ["paper", "spigot", "bukkit", "purpur"] : ["fabric"];
  const project = await getJson(`${MODRINTH}/project/${encodeURIComponent(slugOrId)}`);
  const versions: any[] = await getJson(
    `${MODRINTH}/project/${encodeURIComponent(slugOrId)}/version?loaders=${encodeURIComponent(JSON.stringify(loaders))}&game_versions=${encodeURIComponent(JSON.stringify([mcVersion]))}`,
  );
  const v = versions.find((x) => x.version_type === "release") ?? versions[0];
  if (!v) throw new Error(`${project.title ?? slugOrId} has no ${loader} build for Minecraft ${mcVersion} yet`);
  const file = v.files.find((f: any) => f.primary) ?? v.files[0];
  await mkdir(dir, { recursive: true });
  const name = safeJarName(file.filename);
  await downloadTo(file.url, join(dir, name), file.hashes?.sha512);
  installed.push(name);
  if (loader === "fabric" && project.client_side === "required") {
    warnings.push(`${project.title} must also be installed on every player's game client; AI bots and unmodded players cannot join while it is installed.`);
  }
  for (const dep of v.dependencies ?? []) {
    if (dep.dependency_type !== "required" || !dep.project_id) continue;
    const r = await installModrinth(dep.project_id, loader, mcVersion, dir, seen);
    installed.push(...r.installed);
    warnings.push(...r.warnings);
  }
  return { installed, warnings };
}

/**
 * Installs Geyser + Floodgate so Minecraft Bedrock players (iPhone, Android,
 * consoles, Windows) can join this Java server.
 */
export async function installGeyser(serverDir: string, loader: Loader, bedrockPort: number): Promise<string[]> {
  const platform = loader === "paper" ? "spigot" : "fabric";
  const dir = addonDir(serverDir, loader);
  await mkdir(dir, { recursive: true });
  const files: string[] = [];
  for (const project of ["geyser", "floodgate"]) {
    const build = await getJson(`${GEYSER}/projects/${project}/versions/latest/builds/latest`);
    const dl = build.downloads?.[platform];
    if (!dl) throw new Error(`${project} has no ${platform} download`);
    const name = `${project === "geyser" ? "Geyser" : "floodgate"}-${platform}.jar`;
    await downloadTo(`${GEYSER}/projects/${project}/versions/latest/builds/latest/downloads/${platform}`, join(dir, name), undefined);
    if (dl.sha256) {
      // Geyser publishes sha256; verify after download.
      const buf = await import("node:fs/promises").then((f) => f.readFile(join(dir, name)));
      const h = createHash("sha256").update(buf).digest("hex");
      if (h !== dl.sha256) throw new Error(`checksum mismatch for ${name}`);
    }
    files.push(name);
  }
  await writeGeyserConfig(serverDir, loader, bedrockPort);
  return files;
}

export async function writeGeyserConfig(serverDir: string, loader: Loader, bedrockPort: number): Promise<void> {
  const cfgDir = loader === "paper" ? join(serverDir, "plugins", "Geyser-Spigot") : join(serverDir, "config", "Geyser-Fabric");
  await mkdir(cfgDir, { recursive: true });
  // Minimal config; Geyser fills in every other option with its defaults.
  await writeFile(
    join(cfgDir, "config.yml"),
    [
      "# Written by OmniPlayer. Bedrock (iPhone) players connect to the UDP port below.",
      "bedrock:",
      "  address: 0.0.0.0",
      `  port: ${bedrockPort}`,
      "  clone-remote-port: false",
      "  motd1: OmniPlayer SMP",
      "  motd2: AI players inside",
      "remote:",
      "  auth-type: floodgate",
      "java:",
      "  auth-type: floodgate",
      "passthrough-motd: true",
      "",
    ].join("\n"),
  );
}

/** Downloads the Fabric server launcher (it fetches the official server jar itself). */
export async function resolveFabric(mcVersion: string): Promise<{ url: string; loader: string; installer: string }> {
  const loaders: any[] = await getJson(`${FABRIC_META}/versions/loader/${encodeURIComponent(mcVersion)}`);
  const loader = (loaders.find((l) => l.loader?.stable) ?? loaders[0])?.loader?.version;
  if (!loader) throw new Error(`Fabric does not support Minecraft ${mcVersion} yet`);
  const installers: any[] = await getJson(`${FABRIC_META}/versions/installer`);
  const installer = (installers.find((i) => i.stable) ?? installers[0])?.version;
  return { url: `${FABRIC_META}/versions/loader/${encodeURIComponent(mcVersion)}/${loader}/${installer}/server/jar`, loader, installer };
}

export async function downloadFabric(mcVersion: string, dest: string): Promise<{ loader: string; installer: string }> {
  const f = await resolveFabric(mcVersion);
  await downloadTo(f.url, dest);
  return { loader: f.loader, installer: f.installer };
}
