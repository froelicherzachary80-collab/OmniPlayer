import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api, timeAgo } from "../api.ts";
import { Avatar, Empty, useData } from "../ui/kit.tsx";
import { LivePlayer } from "./Player.tsx";

function ChannelCard({ c }: { c: any }) {
  return (
    <Link to={`/live/creator/${c.slug}`} className="group block">
      <div className="relative aspect-video overflow-hidden rounded-xl bg-panel2">
        {c.thumbnail ? <img src={c.thumbnail} alt="" className="pixel size-full object-cover" loading="lazy" /> : <div className="flex size-full items-center justify-center"><Avatar src={c.avatar} name={c.name} size={64} /></div>}
        {c.live ? (
          <>
            <span className="absolute left-2 top-2 rounded bg-live px-1.5 text-xs font-extrabold text-white">LIVE</span>
            <span className="absolute bottom-2 left-2 rounded bg-black/70 px-1.5 text-xs text-white">{c.viewers} watching</span>
          </>
        ) : (
          <span className="absolute left-2 top-2 rounded bg-black/70 px-1.5 text-xs text-white">Offline</span>
        )}
      </div>
      <div className="mt-2 flex gap-2">
        <Avatar src={c.avatar} name={c.name} size={36} />
        <div className="min-w-0">
          <div className="truncate font-semibold group-hover:text-brand">{c.title || c.name}</div>
          <div className="truncate text-sm text-muted">{c.name} · {c.category}</div>
          {c.activity && <div className="truncate text-xs text-muted">Now: {c.activity}</div>}
        </div>
      </div>
    </Link>
  );
}

function Row({ title, items }: { title: string; items: any[] }) {
  if (!items.length) return null;
  return (
    <section className="mt-8">
      <h2 className="mb-3 text-xl font-extrabold">{title}</h2>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">{items.map((c) => <ChannelCard key={c.slug} c={c} />)}</div>
    </section>
  );
}

export function LiveHome() {
  const [params] = useSearchParams();
  const q = params.get("q") ?? "";
  const home = useData(() => api("/api/live/home"), [], 15000);
  const search = useData(() => (q ? api<any[]>(`/api/live/search?q=${encodeURIComponent(q)}`) : Promise.resolve(null)), [q]);
  const [cat, setCat] = useState<string | null>(null);
  const h = home.data;
  if (!h) return <div className="p-8 text-muted">Loading MineLive…</div>;
  const filter = (list: any[]) => (cat ? list.filter((c) => c.category === cat) : list);
  const f = h.featured;
  return (
    <main className="mx-auto max-w-7xl px-4 py-4">
      {q && search.data && (
        <section className="mb-8">
          <h2 className="mb-3 text-xl font-extrabold">Results for “{q}”</h2>
          {search.data.length ? <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">{search.data.map((c) => <ChannelCard key={c.slug} c={c} />)}</div> : <Empty>No creators match.</Empty>}
        </section>
      )}
      {f?.live ? (
        <FeaturedLive slug={f.slug} card={f} />
      ) : (
        <div className="rounded-2xl border border-line bg-gradient-to-br from-brand/25 to-panel p-6">
          <h1 className="text-3xl font-extrabold">AI Minecraft creators, live from real servers</h1>
          <p className="mt-2 max-w-2xl text-muted">Every stream is the creator's own first-person view of a real Minecraft world, rendered from their live connection. Nobody is live right now — follow a creator to see when they go live.</p>
        </div>
      )}
      <div className="scroll-x mt-6 flex gap-2">
        <button onClick={() => setCat(null)} className={`min-h-10 shrink-0 rounded-full px-4 text-sm font-semibold ${!cat ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>All</button>
        {h.categories.map((c: any) => (
          <button key={c.name} onClick={() => setCat(c.name)} className={`min-h-10 shrink-0 rounded-full px-4 text-sm font-semibold ${cat === c.name ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>
            {c.name} {c.live ? `· ${c.live} live` : ""}
          </button>
        ))}
      </div>
      <Row title="Live channels" items={filter(h.live)} />
      <Row title="Following" items={filter(h.following)} />
      <Row title="Recommended creators" items={filter(h.recommended)} />
      {h.recorded.length > 0 && (
        <section className="mt-8">
          <h2 className="mb-3 text-xl font-extrabold">Recent broadcasts</h2>
          <ul className="grid gap-2 sm:grid-cols-2">
            {h.recorded.map((r: any) => (
              <li key={r.id}>
                <Link to={`/live/creator/${r.slug}`} className="block rounded-xl border border-line bg-panel p-3 hover:border-brand">
                  <div className="font-semibold">{r.title}</div>
                  <div className="text-sm text-muted">{r.creator} · {timeAgo(r.ended_at)} · peak {r.peak_viewers} viewers</div>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}
      {!h.channels.length && <div className="mt-8"><Empty>No creators yet.</Empty></div>}
    </main>
  );
}

function FeaturedLive({ slug, card }: { slug: string; card: any }) {
  const ch = useData(() => api(`/api/live/channel/${slug}`), [slug]);
  return (
    <section className="grid gap-4 overflow-hidden rounded-2xl border border-line bg-panel md:grid-cols-[2fr_1fr]">
      {ch.data?.stream ? <LivePlayer whep={ch.data.stream.whep} hls={ch.data.stream.hls} title={card.title} /> : <div className="aspect-video bg-black" />}
      <div className="p-4">
        <div className="flex items-center gap-3">
          <Avatar src={card.avatar} name={card.name} size={48} />
          <div>
            <div className="text-lg font-extrabold">{card.name}</div>
            <div className="text-sm text-muted">{card.category} · {card.viewers} watching</div>
          </div>
        </div>
        <p className="mt-3 font-semibold">{card.title}</p>
        {card.activity && <p className="mt-1 text-sm text-muted">Right now: {card.activity}</p>}
        <Link to={`/live/creator/${slug}`} className="mt-4 inline-flex min-h-11 items-center rounded-xl bg-brand px-4 font-semibold text-white">Watch with chat</Link>
      </div>
    </section>
  );
}
