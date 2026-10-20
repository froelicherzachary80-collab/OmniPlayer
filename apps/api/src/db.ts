// SQLite persistence (node:sqlite). Schema is migration-friendly for a later
// move to PostgreSQL: plain columns + JSON text for nested documents.

import { DatabaseSync } from "node:sqlite";

export type Row = Record<string, any>;

const SCHEMA = [
  `CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT)`,
  `CREATE TABLE IF NOT EXISTS users (
     id TEXT PRIMARY KEY, username TEXT UNIQUE NOT NULL COLLATE NOCASE, password_hash TEXT NOT NULL,
     role TEXT NOT NULL DEFAULT 'viewer', created_at INTEGER NOT NULL, banned INTEGER NOT NULL DEFAULT 0)`,
  `CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS accounts (
     id TEXT PRIMARY KEY, type TEXT NOT NULL, mc_username TEXT NOT NULL, mc_uuid TEXT NOT NULL,
     secret TEXT, entitlement_ok INTEGER NOT NULL DEFAULT 0, skin_url TEXT, status TEXT NOT NULL DEFAULT 'ready',
     created_at INTEGER NOT NULL)`,
  `CREATE UNIQUE INDEX IF NOT EXISTS accounts_offline_name ON accounts(mc_username) WHERE type = 'offline'`,
  `CREATE TABLE IF NOT EXISTS creators (
     id TEXT PRIMARY KEY, slug TEXT UNIQUE NOT NULL, doc TEXT NOT NULL, created_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS servers (id TEXT PRIMARY KEY, doc TEXT NOT NULL, created_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS memories (
     creator_id TEXT NOT NULL, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL, updated_at INTEGER NOT NULL,
     PRIMARY KEY (creator_id, kind, key))`,
  `CREATE TABLE IF NOT EXISTS bot_sessions (
     id INTEGER PRIMARY KEY AUTOINCREMENT, creator_id TEXT NOT NULL, server_id TEXT, started_at INTEGER NOT NULL,
     ended_at INTEGER, end_reason TEXT)`,
  `CREATE TABLE IF NOT EXISTS streams (
     id TEXT PRIMARY KEY, creator_id TEXT NOT NULL, title TEXT, path TEXT NOT NULL, status TEXT NOT NULL,
     started_at INTEGER NOT NULL, ended_at INTEGER, peak_viewers INTEGER NOT NULL DEFAULT 0, scheduled INTEGER NOT NULL DEFAULT 0)`,
  `CREATE TABLE IF NOT EXISTS clips (
     id TEXT PRIMARY KEY, stream_id TEXT, creator_id TEXT NOT NULL, title TEXT NOT NULL, file TEXT NOT NULL,
     duration REAL NOT NULL, created_by TEXT, created_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS chat_messages (
     id INTEGER PRIMARY KEY AUTOINCREMENT, channel TEXT NOT NULL, user_id TEXT, username TEXT NOT NULL, text TEXT NOT NULL,
     kind TEXT NOT NULL, created_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0)`,
  `CREATE INDEX IF NOT EXISTS chat_by_channel ON chat_messages(channel, id)`,
  `CREATE TABLE IF NOT EXISTS follows (user_id TEXT NOT NULL, creator_id TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY (user_id, creator_id))`,
  `CREATE TABLE IF NOT EXISTS voices (id TEXT PRIMARY KEY, doc TEXT NOT NULL, created_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS invitations (
     id TEXT PRIMARY KEY, server_id TEXT NOT NULL, mc_username TEXT NOT NULL, code TEXT NOT NULL, created_at INTEGER NOT NULL,
     accepted_at INTEGER)`,
  `CREATE TABLE IF NOT EXISTS jobs (
     id TEXT PRIMARY KEY, type TEXT NOT NULL, status TEXT NOT NULL, message TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)`,
  `CREATE TABLE IF NOT EXISTS reserved_names (name TEXT PRIMARY KEY COLLATE NOCASE, reason TEXT, created_at INTEGER NOT NULL)`,
];

export class Db {
  sql: DatabaseSync;
  constructor(file: string) {
    this.sql = new DatabaseSync(file);
    this.sql.exec("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;");
    for (const s of SCHEMA) this.sql.exec(s);
  }
  all(q: string, ...p: any[]): Row[] {
    return this.sql.prepare(q).all(...p) as Row[];
  }
  get(q: string, ...p: any[]): Row | undefined {
    return this.sql.prepare(q).get(...p) as Row | undefined;
  }
  run(q: string, ...p: any[]) {
    return this.sql.prepare(q).run(...p);
  }
  /** Document helpers for JSON-in-column tables. */
  docs<T>(table: string): T[] {
    return this.all(`SELECT doc FROM ${table} ORDER BY created_at`).map((r) => JSON.parse(r.doc) as T);
  }
  doc<T>(table: string, id: string): T | undefined {
    const r = this.get(`SELECT doc FROM ${table} WHERE id = ?`, id);
    return r ? (JSON.parse(r.doc) as T) : undefined;
  }
  putDoc(table: string, id: string, doc: unknown, extra: Record<string, unknown> = {}) {
    const cols = ["id", "doc", "created_at", ...Object.keys(extra)];
    const vals = [id, JSON.stringify(doc), (doc as any).created_at ?? Date.now(), ...Object.values(extra)];
    this.run(
      `INSERT INTO ${table} (${cols.join(",")}) VALUES (${cols.map(() => "?").join(",")})
       ON CONFLICT(id) DO UPDATE SET ${cols.slice(1).map((c) => `${c}=excluded.${c}`).join(",")}`,
      ...(vals as any[]),
    );
  }
  delDoc(table: string, id: string) {
    this.run(`DELETE FROM ${table} WHERE id = ?`, id);
  }
}
