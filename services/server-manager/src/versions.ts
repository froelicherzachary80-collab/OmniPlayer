// Resolves and downloads official server software.
//   Vanilla: Mojang's launcher version manifest (piston-meta.mojang.com)
//   Paper:   PaperMC's Fill v3 API (fill.papermc.io)

import { createHash } from "node:crypto";
import { mkdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const MOJANG_MANIFEST = process.env.OMNI_MOJANG_MANIFEST ?? "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_API = process.env.OMNI_PAPER_API ?? "https://fill.papermc.io/v3";
const UA = "OmniPlayer-ServerManager/0.1 (+https://github.com/froelicherzachary80-collab/omniplayer)";

export interface ResolvedJar {
  software: "vanilla" | "paper";
  version: string;
  url: string;
  sha1?: string;
  sha256?: string;
  javaMajor: number;
  build?: number;
}

async function getJson(url: string): Promise<any> {
  const r = await fetch(url, { headers: { "User-Agent": UA } });
  if (!r.ok) throw new Error(`GET ${url} -> HTTP ${r.status}`);
  return r.json();
}

export async function listVanillaReleases(limit = 30): Promise<string[]> {
  const m = await getJson(MOJANG_MANIFEST);
  return m.versions.filter((v: any) => v.type === "release").slice(0, limit).map((v: any) => v.id);
}

export async function resolveVanilla(version: string): Promise<ResolvedJar> {
  const m = await getJson(MOJANG_MANIFEST);
  const entry = m.versions.find((v: any) => v.id === version);
  if (!entry) throw new Error(`Minecraft ${version} is not in Mojang's version manifest`);
  const v = await getJson(entry.url);
  const server = v.downloads?.server;
  if (!server) throw new Error(`Minecraft ${version} has no dedicated server download`);
  return { software: "vanilla", version, url: server.url, sha1: server.sha1, javaMajor: v.javaVersion?.majorVersion ?? 21 };
}

export async function resolvePaper(version: string): Promise<ResolvedJar> {
  const builds = await getJson(`${PAPER_API}/projects/paper/versions/${encodeURIComponent(version)}/builds`);
  const list: any[] = Array.isArray(builds) ? builds : builds.builds ?? [];
  const stable = list.filter((b) => (b.channel ?? "STABLE").toUpperCase() === "STABLE");
  const pick = (stable.length ? stable : list).sort((a, b) => b.id - a.id)[0];
  if (!pick) throw new Error(`Paper has no builds for ${version} yet`);
  const dl = pick.downloads?.["server:default"];
  if (!dl) throw new Error(`Paper build ${pick.id} has no server download`);
  let javaMajor = 21;
  try {
    const ver = await getJson(`${PAPER_API}/projects/paper/versions/${encodeURIComponent(version)}`);
    javaMajor = ver?.version?.java?.version?.minimum ?? ver?.java?.version?.minimum ?? javaMajor;
  } catch {
    /* optional metadata */
  }
  return { software: "paper", version, url: dl.url, sha256: dl.checksums?.sha256, javaMajor, build: pick.id };
}

export async function resolve(software: "vanilla" | "paper", version: string): Promise<ResolvedJar> {
  return software === "paper" ? resolvePaper(version) : resolveVanilla(version);
}

/** Downloads `jar` to `dest`, verifying the published checksum. */
export async function download(jar: ResolvedJar, dest: string, onProgress?: (pct: number) => void): Promise<void> {
  await mkdir(dirname(dest), { recursive: true });
  const r = await fetch(jar.url, { headers: { "User-Agent": UA } });
  if (!r.ok || !r.body) throw new Error(`download failed: HTTP ${r.status}`);
  const total = Number(r.headers.get("content-length") ?? 0);
  const chunks: Uint8Array[] = [];
  let got = 0;
  for await (const c of r.body as any as AsyncIterable<Uint8Array>) {
    chunks.push(c);
    got += c.length;
    if (total && onProgress) onProgress(Math.round((got / total) * 100));
  }
  const buf = Buffer.concat(chunks);
  if (jar.sha1) {
    const h = createHash("sha1").update(buf).digest("hex");
    if (h !== jar.sha1) throw new Error(`checksum mismatch (sha1 ${h} != ${jar.sha1})`);
  }
  if (jar.sha256) {
    const h = createHash("sha256").update(buf).digest("hex");
    if (h !== jar.sha256) throw new Error(`checksum mismatch (sha256 ${h} != ${jar.sha256})`);
  }
  const tmp = dest + ".part";
  await writeFile(tmp, buf);
  await rm(dest, { force: true });
  await rename(tmp, dest);
}

export async function exists(p: string): Promise<boolean> {
  try {
    await stat(p);
    return true;
  } catch {
    return false;
  }
}
