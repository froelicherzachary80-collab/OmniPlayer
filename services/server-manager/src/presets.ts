// SMP presets. Each preset either maps to vanilla settings/gamerules, installs a
// datapack that OmniPlayer ships, or is marked unavailable with the reason —
// a mode is never "enabled" just because its name is shown.

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

export interface SmpPreset {
  id: string;
  name: string;
  description: string;
  available: boolean;
  unavailable_reason?: string;
  /** Overrides applied to server.properties. */
  properties: Record<string, string>;
  /** Console commands run once after the first successful start. */
  first_start_commands: string[];
  datapack?: "lifesteal";
  experimental?: boolean;
}

export const PRESETS: SmpPreset[] = [
  {
    id: "survival",
    name: "Normal Survival SMP",
    description: "Vanilla survival, normal difficulty, PvP on.",
    available: true,
    properties: { gamemode: "survival", difficulty: "normal", pvp: "true", hardcore: "false" },
    first_start_commands: [],
  },
  {
    id: "hardcore",
    name: "Hardcore SMP",
    description: "One life. Vanilla hardcore mode (hard difficulty, death = spectator).",
    available: true,
    properties: { gamemode: "survival", difficulty: "hard", hardcore: "true", pvp: "true" },
    first_start_commands: [],
  },
  {
    id: "pvp",
    name: "PvP SMP",
    description: "Survival with PvP emphasis: keepInventory off, natural regeneration on, hard difficulty.",
    available: true,
    properties: { gamemode: "survival", difficulty: "hard", pvp: "true" },
    first_start_commands: ["gamerule keep_inventory false", "gamerule show_death_messages true"],
  },
  {
    id: "creator",
    name: "Creator SMP",
    description: "Friendly creator server: keepInventory on, PvP off, spawn protection.",
    available: true,
    properties: { gamemode: "survival", difficulty: "normal", pvp: "false", "spawn-protection": "16" },
    first_start_commands: ["gamerule keep_inventory true"],
  },
  {
    id: "civilization",
    name: "Civilization SMP",
    description: "Team-based survival: teams are created as scoreboard teams with friendly fire off.",
    available: true,
    properties: { gamemode: "survival", difficulty: "normal", pvp: "true" },
    first_start_commands: [],
  },
  {
    id: "roleplay",
    name: "Roleplay SMP",
    description: "Peaceful-leaning survival for storylines: PvP off, easy difficulty.",
    available: true,
    properties: { gamemode: "survival", difficulty: "easy", pvp: "false" },
    first_start_commands: [],
  },
  {
    id: "lifesteal",
    name: "Lifesteal-style SMP",
    description: "Killing a player gives you a heart; dying costs one (min 1). Implemented by the bundled OmniPlayer datapack.",
    available: true,
    experimental: true,
    properties: { gamemode: "survival", difficulty: "normal", pvp: "true" },
    first_start_commands: [],
    datapack: "lifesteal",
  },
  {
    id: "origins",
    name: "Origins-style SMP",
    description: "Players pick an origin with unique powers.",
    available: false,
    unavailable_reason: "Requires the third-party Origins mod (Fabric). OmniPlayer cannot install client/server mods automatically.",
    properties: {},
    first_start_commands: [],
  },
  {
    id: "factions",
    name: "Factions SMP",
    description: "Land claims and faction wars.",
    available: false,
    unavailable_reason: "Requires a third-party Factions plugin on Paper. Install one manually into plugins/ to use this format.",
    properties: {},
    first_start_commands: [],
  },
  {
    id: "custom",
    name: "Custom SMP",
    description: "Start from vanilla survival and set your own rules.",
    available: true,
    properties: {},
    first_start_commands: [],
  },
];

export function preset(id: string): SmpPreset | undefined {
  return PRESETS.find((p) => p.id === id);
}

export function teamCommands(teams: { name: string; color?: string; members: string[] }[]): string[] {
  const cmds: string[] = [];
  for (const t of teams) {
    const id = t.name.toLowerCase().replace(/[^a-z0-9_]/g, "_").slice(0, 16) || "team";
    cmds.push(`team add ${id}`);
    cmds.push(`team modify ${id} friendlyFire false`);
    if (t.color) cmds.push(`team modify ${id} color ${t.color}`);
    for (const m of t.members) cmds.push(`team join ${id} ${m}`);
  }
  return cmds;
}

/**
 * Writes the Lifesteal datapack. Mechanics: on death, max health −2 (one heart,
 * floor 2 = one heart); on a player kill, max health +2 (cap 40).
 * Pack format 121 targets Minecraft 26.3. Marked experimental until verified
 * on your server (run `/datapack list` after the first start).
 */
export async function writeLifesteal(worldDir: string, packFormat = 121): Promise<void> {
  const root = join(worldDir, "datapacks", "omniplayer_lifesteal");
  const fn = join(root, "data", "lifesteal", "function");
  const tags = join(root, "data", "minecraft", "tags", "function");
  await mkdir(fn, { recursive: true });
  await mkdir(tags, { recursive: true });
  await writeFile(join(root, "pack.mcmeta"), JSON.stringify({ pack: { description: "OmniPlayer Lifesteal", min_format: packFormat, max_format: packFormat, pack_format: packFormat } }, null, 2));
  await writeFile(join(tags, "load.json"), JSON.stringify({ values: ["lifesteal:load"] }));
  await writeFile(join(tags, "tick.json"), JSON.stringify({ values: ["lifesteal:tick"] }));
  await writeFile(
    join(fn, "load.mcfunction"),
    [
      "scoreboard objectives add ls_deaths deathCount",
      "scoreboard objectives add ls_kills playerKillCount",
      "scoreboard objectives add ls_hearts dummy",
      'tellraw @a {"text":"[OmniPlayer] Lifesteal rules active","color":"red"}',
    ].join("\n") + "\n",
  );
  await writeFile(
    join(fn, "tick.mcfunction"),
    [
      "execute as @a unless score @s ls_hearts matches 1.. run scoreboard players set @s ls_hearts 10",
      "execute as @a[scores={ls_deaths=1..}] if score @s ls_hearts matches 2.. run scoreboard players remove @s ls_hearts 1",
      "scoreboard players set @a[scores={ls_deaths=1..}] ls_deaths 0",
      "execute as @a[scores={ls_kills=1..}] if score @s ls_hearts matches ..19 run scoreboard players add @s ls_hearts 1",
      "scoreboard players set @a[scores={ls_kills=1..}] ls_kills 0",
      "execute as @a run function lifesteal:apply",
    ].join("\n") + "\n",
  );
  // Apply hearts → max_health (2 HP per heart) with a macro.
  await writeFile(
    join(fn, "apply.mcfunction"),
    ["execute store result storage lifesteal:tmp hp int 2 run scoreboard players get @s ls_hearts", "function lifesteal:set with storage lifesteal:tmp"].join("\n") + "\n",
  );
  await writeFile(join(fn, "set.mcfunction"), "$attribute @s minecraft:max_health base set $(hp)\n");
}
