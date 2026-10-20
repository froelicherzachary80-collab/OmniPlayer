import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { getToken, wsUrl } from "../api.ts";

interface Msg {
  id: number;
  username: string;
  text: string;
  kind: string;
  created_at: number;
}

export function LiveChat({ slug, creatorName, onViewers }: { slug: string; creatorName: string; onViewers?: (n: number) => void }) {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [emotes, setEmotes] = useState<Record<string, string>>({});
  const [text, setText] = useState("");
  const [note, setNote] = useState("");
  const [showEmotes, setShowEmotes] = useState(false);
  const ws = useRef<WebSocket | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const loggedIn = !!getToken();

  useEffect(() => {
    let closed = false;
    let retry = 1000;
    const connect = () => {
      const s = new WebSocket(wsUrl(`/api/live/${slug}/ws`));
      ws.current = s;
      s.onmessage = (e) => {
        const m = JSON.parse(e.data);
        if (m.type === "history") {
          setMsgs(m.messages);
          setEmotes(m.emotes ?? {});
        } else if (m.type === "message") setMsgs((x) => [...x.slice(-300), m.message]);
        else if (m.type === "deleted") setMsgs((x) => x.filter((y) => y.id !== m.id));
        else if (m.type === "viewers") onViewers?.(m.count);
        else if (m.type === "error" || m.type === "moderated") {
          setNote(m.error);
          setTimeout(() => setNote(""), 5000);
        }
      };
      s.onopen = () => (retry = 1000);
      s.onclose = () => {
        if (!closed) setTimeout(connect, (retry = Math.min(retry * 2, 15000)));
      };
    };
    connect();
    return () => {
      closed = true;
      ws.current?.close();
    };
  }, [slug]);

  useEffect(() => {
    list.current?.scrollTo(0, list.current.scrollHeight);
  }, [msgs]);

  const send = () => {
    if (!text.trim()) return;
    ws.current?.send(JSON.stringify({ type: "chat", text }));
    setText("");
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-panel">
      <div className="border-b border-line px-3 py-2 text-sm font-bold">Stream chat</div>
      <div ref={list} className="min-h-0 flex-1 space-y-1.5 overflow-y-auto p-3 text-sm" aria-live="polite">
        {msgs.map((m) => (
          <div key={m.id} className={m.kind === "creator" ? "rounded-lg bg-brand/10 p-1.5" : ""}>
            {m.kind === "creator" && <span className="mr-1 rounded bg-brand px-1 text-[10px] font-bold text-white">AI CREATOR</span>}
            {m.kind === "simulated" && <span className="mr-1 rounded bg-warn px-1 text-[10px] font-bold text-black">SIMULATED</span>}
            {m.kind === "system" && <span className="mr-1 text-muted">•</span>}
            <span className="font-bold" style={{ color: m.kind === "creator" ? "var(--color-brand)" : `hsl(${[...m.username].reduce((a, c) => a + c.charCodeAt(0), 0) % 360} 70% 65%)` }}>
              {m.username}
            </span>
            <span className="text-muted">: </span>
            <span className="break-words">{m.text}</span>
          </div>
        ))}
        {!msgs.length && <div className="text-muted">Say hi to {creatorName}! They read selected messages and reply on stream.</div>}
      </div>
      {note && <div className="bg-warn/15 px-3 py-2 text-xs text-warn">{note}</div>}
      <div className="pb-safe border-t border-line p-2">
        {loggedIn ? (
          <>
            {showEmotes && (
              <div className="mb-2 flex flex-wrap gap-1">
                {Object.entries(emotes).map(([k, v]) => (
                  <button key={k} className="min-h-10 min-w-10 rounded-lg bg-panel2 text-lg" title={k} onClick={() => setText((t) => `${t}${t ? " " : ""}${k} `)}>
                    {v}
                  </button>
                ))}
              </div>
            )}
            <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); send(); }}>
              <button type="button" className="min-h-11 min-w-11 rounded-xl bg-panel2 text-lg" onClick={() => setShowEmotes((s) => !s)} aria-label="Emotes">☺</button>
              <input value={text} onChange={(e) => setText(e.target.value)} maxLength={300} placeholder="Send a message" className="min-h-11 min-w-0 flex-1 rounded-xl border border-line bg-panel2 px-3 outline-none focus:border-brand" />
              <button className="min-h-11 rounded-xl bg-brand px-4 font-semibold text-white">Chat</button>
            </form>
          </>
        ) : (
          <Link to="/live/login" className="block min-h-11 rounded-xl bg-brand py-3 text-center font-semibold text-white">Log in to chat</Link>
        )}
      </div>
    </div>
  );
}
