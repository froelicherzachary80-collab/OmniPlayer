// Persistent server definitions + live instances.

import { EventEmitter } from "node:events";
import { createConnection } from "node:net";
import type { ServerConfig } from "@omniplayer/shared";
import { ServerInstance, type InstanceSpec } from "@omniplayer/server-manager";
import { config } from "./config.ts";
import type { Db } from "./db.ts";
import { newId } from "./security.ts";

export const DEFAULT_VERSION = "26.3";

export function defaultServer(partial: Partial<ServerConfig>): ServerConfig {
  return {
    id: newId("srv_"),
    name: "My AI SMP",
    software: "vanilla",
    version: DEFAULT_VERSION,
    port: 25565,
    auth_mode: "offline",
    gamemode: "survival",
    difficulty: "normal",
    seed: "",
    world_type: "default",
    max_players: 20,
    ai_count: 5,
    auto_start_bots: false,
    memory_mb: 3072,
    host: "",
    custom_command: "",
    whitelist: [],
    ops: [],
    smp_preset: null,
    eula_accepted_at: null,
    properties: {},
    created_at: Date.now(),
    ...partial,
  };
}

/** Minimal Server List Ping used for external servers and health checks. */
export function pingServer(host: string, port: number, timeoutMs = 4000): Promise<{ online: boolean; version?: string; protocol?: number; players?: number; max?: number; motd?: string; error?: string }> {
  return new Promise((resolve) => {
    const sock = createConnection({ host, port });
    const done = (r: any) => {
      sock.destroy();
      resolve(r);
    };
    const t = setTimeout(() => done({ online: false, error: "timeout" }), timeoutMs);
    const varint = (n: number) => {
      const out: number[] = [];
      do {
        let b = n & 0x7f;
        n >>>= 7;
        if (n) b |= 0x80;
        out.push(b);
      } while (n);
      return Buffer.from(out);
    };
    const packet = (body: Buffer) => Buffer.concat([varint(body.length), body]);
    sock.on("connect", () => {
      const hostB = Buffer.from(host);
      const port2 = Buffer.alloc(2);
      port2.writeUInt16BE(port);
      // handshake: id 0, protocol 0 (any value is accepted for status), host, port, intent 1 = status
      sock.write(packet(Buffer.concat([varint(0), varint(0), varint(hostB.length), hostB, port2, varint(1)])));
      sock.write(packet(varint(0)));
    });
    let buf = Buffer.alloc(0);
    sock.on("data", (d) => {
      buf = Buffer.concat([buf, d]);
      try {
        let i = 0;
        const readVar = () => {
          let v = 0, s = 0, b;
          do {
            if (i >= buf.length) throw new Error("more");
            b = buf[i++];
            v |= (b & 0x7f) << s;
            s += 7;
          } while (b & 0x80);
          return v;
        };
        const len = readVar();
        if (buf.length < i + len) return;
        readVar(); // id
        const slen = readVar();
        const json = JSON.parse(buf.subarray(i, i + slen).toString());
        clearTimeout(t);
        const desc = typeof json.description === "string" ? json.description : json.description?.text ?? "";
        done({ online: true, version: json.version?.name, protocol: json.version?.protocol, players: json.players?.online, max: json.players?.max, motd: desc });
      } catch {
        /* wait for more data */
      }
    });
    sock.on("error", (e) => {
      clearTimeout(t);
      done({ online: false, error: e.message });
    });
  });
}

export class ServerRegistry extends EventEmitter {
  instances = new Map<string, ServerInstance>();
  private db: Db;
  constructor(db: Db) {
    super();
    this.db = db;
  }

  list(): ServerConfig[] {
    return this.db.docs<ServerConfig>("servers");
  }

  get(id: string): ServerConfig | undefined {
    return this.db.doc<ServerConfig>("servers", id);
  }

  save(s: ServerConfig) {
    this.db.putDoc("servers", s.id, s);
    const inst = this.instances.get(s.id);
    if (inst) inst.spec = this.spec(s);
  }

  create(partial: Partial<ServerConfig>): ServerConfig {
    const used = new Set(this.list().map((x) => x.port));
    let port = partial.port ?? 25565;
    while (used.has(port) && partial.software !== "external") port++;
    const s = defaultServer({ ...partial, port });
    this.save(s);
    return s;
  }

  async remove(id: string) {
    await this.stop(id);
    this.instances.delete(id);
    this.db.delDoc("servers", id);
  }

  isRunning(id: string): boolean {
    const s = this.get(id);
    if (s?.software === "external") return true;
    return this.instances.get(id)?.state === "running";
  }

  spec(s: ServerConfig): InstanceSpec {
    return {
      id: s.id,
      name: s.name,
      software: s.software === "external" ? "custom" : s.software,
      version: s.version,
      port: s.port,
      auth_mode: s.auth_mode,
      gamemode: s.gamemode,
      difficulty: s.difficulty,
      seed: s.seed,
      world_type: s.world_type,
      max_players: s.max_players,
      memory_mb: s.memory_mb,
      custom_command: s.custom_command,
      whitelist: [...s.whitelist],
      ops: [...s.ops],
      smp_preset: s.smp_preset,
      teams: (s as any).teams,
      properties: s.properties,
      eula_accepted_at: s.eula_accepted_at,
      bind_ip: (s as any).bind_ip,
    };
  }

  instance(id: string): ServerInstance {
    const s = this.get(id);
    if (!s) throw new Error("server not found");
    if (s.software === "external") throw new Error("external servers are not managed by OmniPlayer");
    let inst = this.instances.get(id);
    if (!inst) {
      inst = new ServerInstance(this.spec(s), config.serversDir);
      inst.on("console", (line) => this.emit("console", { server_id: id, line }));
      inst.on("state", (state) => this.emit("state", { server_id: id, state }));
      inst.on("player", (p) => this.emit("player", { server_id: id, ...p }));
      this.instances.set(id, inst);
    } else {
      inst.spec = this.spec(s);
    }
    return inst;
  }

  async start(id: string) {
    const inst = this.instance(id);
    await inst.start();
  }

  async stop(id: string) {
    const inst = this.instances.get(id);
    if (inst) await inst.stop();
  }

  async stopAll() {
    await Promise.all([...this.instances.keys()].map((id) => this.stop(id)));
  }

  /** Adds a name to the allowlist, refusing reserved names claimed by someone else. */
  async allow(id: string, name: string, by: "bot" | "human" = "human") {
    const s = this.get(id);
    if (!s) throw new Error("server not found");
    const reserved = this.db.get("SELECT reason FROM reserved_names WHERE name = ?", name);
    if (reserved && by === "human") throw new Error(`"${name}" is a reserved name (${reserved.reason}).`);
    if (!s.whitelist.some((n) => n.toLowerCase() === name.toLowerCase())) s.whitelist.push(name);
    this.save(s);
    const inst = this.instances.get(id);
    if (inst) await inst.whitelistAdd(name);
  }

  async disallow(id: string, name: string) {
    const s = this.get(id);
    if (!s) throw new Error("server not found");
    s.whitelist = s.whitelist.filter((n) => n.toLowerCase() !== name.toLowerCase());
    this.save(s);
    const inst = this.instances.get(id);
    if (inst) await inst.whitelistRemove(name);
  }

  connectAddress(s: ServerConfig): string {
    const host = s.software === "external" ? s.host : config.publicHost || "<this computer's LAN IP>";
    return s.port === 25565 ? host : `${host}:${s.port}`;
  }
}
