// MineLive live chat: WebSocket hub, history, moderation, creator replies.

import type { WebSocket } from "ws";
import type { Creator } from "@omniplayer/shared";
import type { Db } from "./db.ts";
import { RateLimiter } from "./security.ts";

export interface Moderation {
  banned_words: string[];
  block_links: boolean;
  max_caps_ratio: number;
  slow_mode_s: number;
}

export const DEFAULT_MODERATION: Moderation = { banned_words: [], block_links: true, max_caps_ratio: 0.8, slow_mode_s: 0 };

export const EMOTES: Record<string, string> = {
  ":creeper:": "🟩",
  ":diamond:": "💎",
  ":pickaxe:": "⛏️",
  ":heart:": "❤️",
  ":gg:": "🎉",
  ":lol:": "😂",
  ":wow:": "😮",
  ":fire:": "🔥",
  ":sword:": "🗡️",
  ":tnt:": "🧨",
};

interface Conn {
  ws: WebSocket;
  id: string;
  user: { id: string; username: string; role: string } | null;
}

export class ChatHub {
  channels = new Map<string, Set<Conn>>();
  private limiter = new RateLimiter(5, 10_000);
  private lastByUser = new Map<string, number>();
  onViewerMessage: (slug: string, username: string, text: string) => void = () => {};
  onPresence: (slug: string, connId: string, joined: boolean) => void = () => {};

  private db: Db;
  private creatorBySlug: (slug: string) => Creator | undefined;
  constructor(db: Db, creatorBySlug: (slug: string) => Creator | undefined) {
    this.db = db;
    this.creatorBySlug = creatorBySlug;
  }

  history(slug: string, limit = 50) {
    return this.db
      .all("SELECT id, channel, username, text, kind, created_at FROM chat_messages WHERE channel = ? AND deleted = 0 ORDER BY id DESC LIMIT ?", slug, limit)
      .reverse();
  }

  moderation(c: Creator): Moderation {
    return { ...DEFAULT_MODERATION, ...((c as any).moderation ?? {}) };
  }

  /** Applies the creator's own chat rules. Returns a reason when the message is blocked. */
  moderate(c: Creator, text: string): string | null {
    const m = this.moderation(c);
    const lower = text.toLowerCase();
    for (const w of m.banned_words) if (w && lower.includes(w.toLowerCase())) return "contains a word this creator does not allow";
    if (m.block_links && /(https?:\/\/|www\.|\.(com|net|gg|io)\b)/i.test(text)) return "links are not allowed in this chat";
    const letters = text.replace(/[^a-zA-Z]/g, "");
    if (letters.length >= 12 && letters.replace(/[^A-Z]/g, "").length / letters.length > m.max_caps_ratio) return "please don't shout (too many capitals)";
    return null;
  }

  attach(slug: string, ws: WebSocket, connId: string, user: Conn["user"]) {
    const conn: Conn = { ws, id: connId, user };
    let set = this.channels.get(slug);
    if (!set) this.channels.set(slug, (set = new Set()));
    set.add(conn);
    this.onPresence(slug, connId, true);
    ws.send(JSON.stringify({ type: "history", messages: this.history(slug), emotes: EMOTES }));
    this.broadcastViewers(slug);
    ws.on("message", (raw: any) => {
      let msg: any;
      try {
        msg = JSON.parse(raw.toString());
      } catch {
        return;
      }
      if (msg.type === "chat") this.handleChat(slug, conn, String(msg.text ?? ""));
    });
    ws.on("close", () => {
      set!.delete(conn);
      this.onPresence(slug, connId, false);
      this.broadcastViewers(slug);
    });
  }

  private send(conn: Conn, obj: unknown) {
    try {
      conn.ws.send(JSON.stringify(obj));
    } catch {
      /* closed */
    }
  }

  private handleChat(slug: string, conn: Conn, text: string) {
    if (!conn.user) return this.send(conn, { type: "error", error: "Log in to chat." });
    const c = this.creatorBySlug(slug);
    if (!c) return;
    text = text.replace(/[\u0000-\u001f]/g, "").trim().slice(0, 300);
    if (!text) return;
    if (!this.limiter.allow(conn.user.id)) return this.send(conn, { type: "error", error: "You're sending messages too quickly." });
    const slow = this.moderation(c).slow_mode_s;
    const last = this.lastByUser.get(`${slug}:${conn.user.id}`) ?? 0;
    if (slow && Date.now() - last < slow * 1000) return this.send(conn, { type: "error", error: `Slow mode: wait ${slow}s between messages.` });
    const blocked = this.moderate(c, text);
    if (blocked) return this.send(conn, { type: "moderated", error: `Message removed by ${c.streamer_name}'s chat rules: ${blocked}.` });
    this.lastByUser.set(`${slug}:${conn.user.id}`, Date.now());
    for (const [k, v] of Object.entries(EMOTES)) text = text.split(k).join(v);
    this.post(slug, conn.user.username, text, "user", conn.user.id);
    this.onViewerMessage(slug, conn.user.username, text);
  }

  post(slug: string, username: string, text: string, kind: "user" | "creator" | "system" | "simulated", userId: string | null = null) {
    const now = Date.now();
    const r = this.db.run("INSERT INTO chat_messages (channel, user_id, username, text, kind, created_at) VALUES (?, ?, ?, ?, ?, ?)", slug, userId, username, text, kind, now);
    const message = { id: Number(r.lastInsertRowid), channel: slug, username, text, kind, created_at: now };
    for (const conn of this.channels.get(slug) ?? []) this.send(conn, { type: "message", message });
    return message;
  }

  remove(slug: string, id: number) {
    this.db.run("UPDATE chat_messages SET deleted = 1 WHERE id = ? AND channel = ?", id, slug);
    for (const conn of this.channels.get(slug) ?? []) this.send(conn, { type: "deleted", id });
  }

  broadcastViewers(slug: string) {
    const n = this.channels.get(slug)?.size ?? 0;
    for (const conn of this.channels.get(slug) ?? []) this.send(conn, { type: "viewers", count: n });
  }
}
