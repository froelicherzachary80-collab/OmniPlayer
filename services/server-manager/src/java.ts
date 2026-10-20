// Locates a Java runtime that satisfies a Minecraft version's requirement.

import { execFile } from "node:child_process";
import { readdir } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);

export interface JavaRuntime {
  path: string;
  major: number;
  version: string;
}

async function probe(bin: string): Promise<JavaRuntime | null> {
  try {
    const { stderr, stdout } = await run(bin, ["-version"], { timeout: 10_000, env: { ...process.env, JAVA_TOOL_OPTIONS: "" } });
    const text = stderr + stdout;
    const m = text.match(/version "([^"]+)"/);
    if (!m) return null;
    const v = m[1];
    const major = v.startsWith("1.") ? Number(v.split(".")[1]) : Number(v.split(/[.+-]/)[0]);
    return { path: bin, major, version: v };
  } catch {
    return null;
  }
}

export async function findJavas(): Promise<JavaRuntime[]> {
  const candidates = new Set<string>();
  if (process.env.OMNI_JAVA) candidates.add(process.env.OMNI_JAVA);
  if (process.env.JAVA_HOME) candidates.add(join(process.env.JAVA_HOME, "bin", "java"));
  for (const root of ["/usr/lib/jvm", "/opt/java", "/Library/Java/JavaVirtualMachines"]) {
    try {
      for (const d of await readdir(root)) {
        candidates.add(join(root, d, "bin", "java"));
        candidates.add(join(root, d, "Contents", "Home", "bin", "java"));
      }
    } catch {
      /* not present */
    }
  }
  candidates.add("java");
  const found: JavaRuntime[] = [];
  for (const c of candidates) {
    const j = await probe(c);
    if (j && !found.some((f) => f.version === j.version && f.major === j.major)) found.push(j);
  }
  return found.sort((a, b) => b.major - a.major);
}

/** Picks the lowest installed Java that is >= `minMajor`. */
export async function pickJava(minMajor: number): Promise<JavaRuntime> {
  const all = await findJavas();
  const ok = all.filter((j) => j.major >= minMajor).sort((a, b) => a.major - b.major);
  if (!ok.length) {
    const have = all.map((j) => j.major).join(", ") || "none";
    throw new Error(`This Minecraft version needs Java ${minMajor}+ (installed: ${have}). Install a JDK/JRE ${minMajor} or set OMNI_JAVA.`);
  }
  return ok[0];
}
