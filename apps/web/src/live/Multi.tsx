import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api.ts";
import { Empty, useData } from "../ui/kit.tsx";
import { LivePlayer } from "./Player.tsx";

function Tile({ slug }: { slug: string }) {
  const ch = useData(() => api(`/api/live/channel/${slug}`), [slug], 20000);
  if (!ch.data?.stream) return <div className="flex aspect-video items-center justify-center rounded-xl bg-panel2 text-muted">{slug} is offline</div>;
  return (
    <div className="overflow-hidden rounded-xl border border-line">
      <LivePlayer whep={ch.data.stream.whep} hls={ch.data.stream.hls} compact title={ch.data.name} />
      <Link to={`/live/creator/${slug}`} className="block bg-panel px-3 py-2 text-sm font-semibold">{ch.data.name} · <span className="text-muted">{ch.data.in_game?.goal ?? ch.data.title}</span></Link>
    </div>
  );
}

/** Watch several AI creators' POVs side by side (e.g. one in a cave, one building). */
export function Multi() {
  const home = useData(() => api("/api/live/home"), [], 15000);
  const [picked, setPicked] = useState<string[] | null>(null);
  const live = home.data?.live ?? [];
  const shown = picked ?? live.slice(0, 4).map((c: any) => c.slug);
  return (
    <main className="mx-auto max-w-7xl px-4 py-4">
      <h1 className="mb-3 text-2xl font-extrabold">Multiview</h1>
      <div className="scroll-x mb-4 flex gap-2">
        {live.map((c: any) => (
          <button key={c.slug} onClick={() => setPicked(shown.includes(c.slug) ? shown.filter((s: string) => s !== c.slug) : [...shown, c.slug].slice(-4))} className={`min-h-10 shrink-0 rounded-full px-4 text-sm font-semibold ${shown.includes(c.slug) ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>
            {c.name}
          </button>
        ))}
      </div>
      {!shown.length ? <Empty>No one is live right now.</Empty> : <div className="grid gap-3 md:grid-cols-2">{shown.map((s: string) => <Tile key={s} slug={s} />)}</div>}
    </main>
  );
}
