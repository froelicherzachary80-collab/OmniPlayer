import { useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, duration, getToken, timeAgo } from "../api.ts";
import { Avatar, Badge, Button, Empty, Tabs, useAction, useData } from "../ui/kit.tsx";
import { LiveChat } from "./Chat.tsx";
import { LivePlayer } from "./Player.tsx";

type Tab = "about" | "past" | "clips" | "schedule";
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function Channel() {
  const { slug } = useParams();
  const ch = useData(() => api(`/api/live/channel/${slug}`), [slug], 10000);
  const [viewers, setViewers] = useState<number | null>(null);
  const [theater, setTheater] = useState(false);
  const [tab, setTab] = useState<Tab>("about");
  const [replay, setReplay] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const { busy, run } = useAction();
  if (ch.error) return <main className="p-8"><Empty>{ch.error}</Empty></main>;
  const c = ch.data;
  if (!c) return <div className="p-8 text-muted">Loading…</div>;
  return (
    <main className={theater ? "" : "mx-auto max-w-7xl md:px-4 md:py-4"}>
      <div className={`grid ${theater ? "lg:grid-cols-[1fr_340px]" : "lg:grid-cols-[1fr_340px]"} gap-0 lg:gap-4`}>
        <div className="min-w-0">
          <div ref={box} className="bg-black">
            {c.stream ? (
              <LivePlayer whep={c.stream.whep} hls={c.stream.hls} title={c.title} />
            ) : replay ? (
              <video src={replay} controls playsInline autoPlay className="aspect-video w-full" />
            ) : (
              <div className="flex aspect-video flex-col items-center justify-center gap-3 text-center text-white">
                <Avatar src={c.avatar} name={c.name} size={72} />
                <div className="text-lg font-bold">{c.name} is offline</div>
                {c.schedule?.length > 0 && <div className="text-sm text-white/70">Next: {DAYS.filter((_, i) => c.schedule[0].days.includes(i)).join(", ")} at {c.schedule[0].start} ({c.schedule[0].timezone})</div>}
              </div>
            )}
          </div>
          <div className="px-4 py-3 md:px-0">
            <div className="flex flex-wrap items-start gap-3">
              <Avatar src={c.avatar} name={c.name} size={52} />
              <div className="min-w-0 flex-1">
                <h1 className="text-xl font-extrabold">{c.name}</h1>
                <div className="text-sm font-semibold">{c.title}</div>
                <div className="mt-1 flex flex-wrap gap-2">
                  {c.live && <Badge tone="live">LIVE · {viewers ?? c.viewers} watching</Badge>}
                  {c.live && c.started_at && <Badge>{duration(Date.now() - c.started_at)}</Badge>}
                  <Badge tone="brand">{c.category}</Badge>
                  <Badge>AI-generated creator</Badge>
                  {c.server && <Badge>{c.server.name} · {c.server.version}</Badge>}
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                {getToken() ? (
                  <Button variant={c.following ? "soft" : "primary"} busy={busy === "f"} onClick={() => run("f", () => api(`/api/live/${slug}/follow`, { method: c.following ? "DELETE" : "POST" }).then(ch.reload))}>
                    {c.following ? "Following ✓" : "♥ Follow"}
                  </Button>
                ) : (
                  <Link to="/live/login"><Button>♥ Follow</Button></Link>
                )}
                {c.live && getToken() && <Button variant="soft" busy={busy === "clip"} onClick={() => run("clip", () => api("/api/clips", { body: { slug, seconds: 30, title: `${c.name} moment` } }).then(ch.reload), "Clipped the last 30 seconds")}>✂ Clip</Button>}
                <Button variant="soft" className="hidden lg:inline-flex" onClick={() => setTheater((t) => !t)}>{theater ? "Exit theater" : "Theater"}</Button>
                <Button variant="soft" onClick={() => (box.current as any)?.requestFullscreen?.() ?? (box.current?.querySelector("video") as any)?.webkitEnterFullscreen?.()}>⛶</Button>
              </div>
            </div>
            {c.in_game && (
              <div className="mt-3 rounded-xl bg-panel p-3 text-sm">
                <span className="text-muted">In game:</span> {c.in_game.goal || "playing"} · ❤ {c.in_game.health?.toFixed?.(0)} · {c.in_game.biome?.replace(/_/g, " ") ?? ""} · {c.in_game.dimension?.replace("minecraft:", "")}
              </div>
            )}
            <p className="mt-3 text-xs text-muted">{c.label}</p>
          </div>
          <div className="px-4 md:px-0">
            <Tabs<Tab> value={tab} onChange={setTab} tabs={[["about", "About"], ["past", `Past broadcasts (${c.recordings.length})`], ["clips", `Clips (${c.clips.length})`], ["schedule", "Schedule"]]} />
            {tab === "about" && (
              <div className="space-y-2 pb-8 text-sm">
                <p>{c.profile.personality}</p>
                <p className="text-muted">Style: {c.profile.speaking_style}</p>
                {c.profile.likes?.length > 0 && <p className="text-muted">Loves: {c.profile.likes.join(", ")}</p>}
                {c.profile.goals?.length > 0 && <p className="text-muted">Goals: {c.profile.goals.join(", ")}</p>}
                {c.profile.friends?.length > 0 && <p className="text-muted">Friends: {c.profile.friends.join(", ")}</p>}
                {c.profile.rivals?.length > 0 && <p className="text-muted">Rivals: {c.profile.rivals.join(", ")}</p>}
                <p className="text-muted">{c.followers} followers</p>
              </div>
            )}
            {tab === "past" && (
              <ul className="space-y-2 pb-8">
                {!c.recordings.length && <Empty>No recordings yet.</Empty>}
                {c.recordings.slice().reverse().map((r: any) => (
                  <li key={r.start}><button className="w-full rounded-xl border border-line bg-panel p-3 text-left hover:border-brand" onClick={() => { setReplay(r.url); window.scrollTo(0, 0); }}>▶ {new Date(r.start).toLocaleString()} · {duration(r.duration * 1000)}</button></li>
                ))}
              </ul>
            )}
            {tab === "clips" && (
              <div className="grid gap-3 pb-8 sm:grid-cols-2">
                {!c.clips.length && <Empty>No clips yet.</Empty>}
                {c.clips.map((k: any) => (
                  <div key={k.id} className="overflow-hidden rounded-xl border border-line bg-panel">
                    <video src={k.url} controls playsInline preload="metadata" className="aspect-video w-full bg-black" />
                    <div className="p-2 text-sm"><b>{k.title}</b> <span className="text-muted">· {Math.round(k.duration)}s · {timeAgo(k.created_at)}</span></div>
                  </div>
                ))}
              </div>
            )}
            {tab === "schedule" && (
              <ul className="space-y-2 pb-8 text-sm">
                {!c.schedule.length && <Empty>No fixed schedule.</Empty>}
                {c.schedule.map((e: any, i: number) => <li key={i} className="rounded-xl bg-panel p-3">{DAYS.filter((_, d) => e.days.includes(d)).join(", ")} · {e.start} for {e.duration_min} min ({e.timezone}){e.activity ? ` · ${e.activity}` : ""}</li>)}
              </ul>
            )}
          </div>
        </div>
        <div className="h-[70dvh] border-line lg:sticky lg:top-16 lg:h-[calc(100dvh-5rem)] lg:overflow-hidden lg:rounded-2xl lg:border">
          <LiveChat slug={slug!} creatorName={c.name} onViewers={setViewers} />
        </div>
      </div>
    </main>
  );
}
