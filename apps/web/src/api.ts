// API client shared by the dashboard and MineLive.

const TOKEN_KEY = "omniplayer.token";

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setToken(t: string | null) {
  try {
    if (t) localStorage.setItem(TOKEN_KEY, t);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    /* private mode */
  }
}

export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

export async function api<T = any>(path: string, opts: { method?: string; body?: unknown; raw?: BodyInit; headers?: Record<string, string> } = {}): Promise<T> {
  const headers: Record<string, string> = { ...(opts.headers ?? {}) };
  const t = getToken();
  if (t) headers.Authorization = `Bearer ${t}`;
  let body: BodyInit | undefined = opts.raw;
  if (opts.body !== undefined) {
    headers["Content-Type"] = "application/json";
    body = JSON.stringify(opts.body);
  }
  const r = await fetch(path, { method: opts.method ?? (body ? "POST" : "GET"), headers, body });
  const ct = r.headers.get("content-type") ?? "";
  const data = ct.includes("json") ? await r.json() : await r.text();
  if (!r.ok) throw new ApiError((data as any)?.error ?? `Request failed (${r.status})`, r.status);
  return data as T;
}

export function wsUrl(path: string) {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const t = getToken();
  return `${proto}//${location.host}${path}${t ? (path.includes("?") ? "&" : "?") + "token=" + encodeURIComponent(t) : ""}`;
}

/** Admin event socket with automatic reconnect. */
export function adminEvents(onMessage: (type: string, data: any) => void): () => void {
  let ws: WebSocket | null = null;
  let closed = false;
  let retry = 1000;
  const connect = () => {
    ws = new WebSocket(wsUrl("/api/ws"));
    ws.onmessage = (e) => {
      try {
        const m = JSON.parse(e.data);
        onMessage(m.type, m.data);
      } catch {
        /* ignore */
      }
    };
    ws.onopen = () => (retry = 1000);
    ws.onclose = () => {
      if (!closed) setTimeout(connect, (retry = Math.min(retry * 2, 15000)));
    };
  };
  connect();
  return () => {
    closed = true;
    ws?.close();
  };
}

export function timeAgo(ts: number | string | null | undefined): string {
  if (!ts) return "";
  const t = typeof ts === "string" ? Date.parse(ts) : ts;
  const s = Math.round((Date.now() - t) / 1000);
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  return `${Math.round(s / 86400)}d ago`;
}

export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h ? `${h}h ${m}m` : `${m}m ${s % 60}s`;
}
