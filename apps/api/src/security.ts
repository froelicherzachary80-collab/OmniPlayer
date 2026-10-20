// Password hashing, session tokens and at-rest encryption for account tokens.

import { createCipheriv, createDecipheriv, createHash, randomBytes, scryptSync, timingSafeEqual } from "node:crypto";
import { config } from "./config.ts";
import type { Db } from "./db.ts";

export function hashPassword(pw: string): string {
  const salt = randomBytes(16);
  const h = scryptSync(pw, salt, 64, { N: 16384, r: 8, p: 1 });
  return `scrypt$${salt.toString("hex")}$${h.toString("hex")}`;
}

export function verifyPassword(pw: string, stored: string): boolean {
  const [alg, salt, hash] = stored.split("$");
  if (alg !== "scrypt" || !salt || !hash) return false;
  const h = scryptSync(pw, Buffer.from(salt, "hex"), 64, { N: 16384, r: 8, p: 1 });
  const want = Buffer.from(hash, "hex");
  return want.length === h.length && timingSafeEqual(h, want);
}

export function newId(prefix = ""): string {
  return prefix + randomBytes(9).toString("base64url");
}

function sha(s: string) {
  return createHash("sha256").update(s).digest("hex");
}

const SESSION_MS = 30 * 24 * 3600 * 1000;

export function createSession(db: Db, userId: string): string {
  const token = randomBytes(32).toString("base64url");
  const now = Date.now();
  db.run("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)", sha(token), userId, now, now + SESSION_MS);
  return token;
}

export function sessionUser(db: Db, token: string | undefined | null) {
  if (!token) return undefined;
  const s = db.get("SELECT user_id, expires_at FROM sessions WHERE token_hash = ?", sha(token));
  if (!s || s.expires_at < Date.now()) return undefined;
  const u = db.get("SELECT id, username, role, banned FROM users WHERE id = ?", s.user_id);
  if (!u || u.banned) return undefined;
  return u as { id: string; username: string; role: "admin" | "viewer"; banned: number };
}

export function revokeSession(db: Db, token: string) {
  db.run("DELETE FROM sessions WHERE token_hash = ?", sha(token));
}

/** AES-256-GCM encryption of secrets (OAuth refresh tokens, Minecraft access tokens). */
export function encrypt(obj: unknown): string {
  const iv = randomBytes(12);
  const c = createCipheriv("aes-256-gcm", config.secretKey, iv);
  const enc = Buffer.concat([c.update(JSON.stringify(obj), "utf8"), c.final()]);
  return `v1.${iv.toString("base64url")}.${c.getAuthTag().toString("base64url")}.${enc.toString("base64url")}`;
}

export function decrypt<T>(s: string | null | undefined): T | null {
  if (!s) return null;
  const [v, iv, tag, data] = s.split(".");
  if (v !== "v1") return null;
  const d = createDecipheriv("aes-256-gcm", config.secretKey, Buffer.from(iv, "base64url"));
  d.setAuthTag(Buffer.from(tag, "base64url"));
  return JSON.parse(Buffer.concat([d.update(Buffer.from(data, "base64url")), d.final()]).toString("utf8")) as T;
}

/** Fixed-window rate limiter keyed by string. */
export class RateLimiter {
  private hits = new Map<string, { n: number; reset: number }>();
  private limit: number;
  private windowMs: number;
  constructor(limit: number, windowMs: number) {
    this.limit = limit;
    this.windowMs = windowMs;
  }
  allow(key: string): boolean {
    const now = Date.now();
    const h = this.hits.get(key);
    if (!h || h.reset < now) {
      this.hits.set(key, { n: 1, reset: now + this.windowMs });
      return true;
    }
    h.n++;
    return h.n <= this.limit;
  }
}
