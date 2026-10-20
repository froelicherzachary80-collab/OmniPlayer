import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { adminEvents, api } from "../api.ts";
import { Avatar, Badge, Button, Card, Empty, Stat, stateTone, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function Overview() {
  const ov = useData(() => api("/api/overview"), [], 5000);
  const creators = useData(() => api<any[]>("/api/creators"), [], 5000);
  const [feed, setFeed] = useState<{ ts: number; who: string; text: string }[]>([]);
  useEffect(
    () =>
      adminEvents((type, d) => {
        if (type === "commentary" || type === "bot") {
          const who = creators.data?.find((c) => c.id === d.creator_id)?.streamer_name ?? "bot";
          const text = type === "commentary" ? `🎙 ${d.text}` : `${d.type}${d.error ? `: ${d.error}` : ""}`;
          setFeed((f) => [{ ts: Date.now(), who, text }, ...f].slice(0, 40));
        }
      }),
    [creators.data],
  );
  const o = ov.data;
  return (
    <>
      <PageTitle title="Overview" sub="Everything running on this OmniPlayer host" action={<Link to="/admin/play"><Button>▶ Play with AI</Button></Link>} />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat label="Servers running" value={o ? `${o.servers.running}/${o.servers.total}` : "—"} />
        <Stat label="AI players online" value={o ? o.bots.online : "—"} sub={o && `${o.bots.running} processes · limit ${o.bots.max}`} />
        <Stat label="Live now" value={o ? o.streams.live : "—"} sub={o && `limit ${o.streams.max}`} />
        <Stat label="Host CPU / RAM" value={o ? `${o.host.cpu_percent}%` : "—"} sub={o && `${o.host.mem_total_mb - o.host.mem_free_mb} / ${o.host.mem_total_mb} MB · ${o.host.cpus} cores`} />
      </div>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <Card title="AI players" action={<Link to="/admin/creators" className="text-sm font-semibold text-brand">Manage</Link>}>
          {!creators.data?.length ? (
            <Empty>
              No AI creators yet. <Link to="/admin/creators" className="text-brand">Create one</Link>.
            </Empty>
          ) : (
            <ul className="divide-y divide-line">
              {creators.data.map((c) => (
                <li key={c.id}>
                  <Link to={`/admin/creators/${c.id}`} className="flex min-h-14 items-center gap-3 py-2">
                    <Avatar src={c.avatar} name={c.streamer_name} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-semibold">{c.streamer_name}</div>
                      <div className="truncate text-xs text-muted">{c.runtime?.status?.goal || c.persona?.personality || "—"}</div>
                    </div>
                    {c.live?.ready && <Badge tone="live">LIVE</Badge>}
                    <Badge tone={stateTone(c.runtime?.state)}>{c.runtime?.state ?? "offline"}</Badge>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </Card>
        <Card title="Live activity">
          {!feed.length ? <Empty>Commentary and bot events appear here in real time.</Empty> : (
            <ul className="max-h-96 space-y-2 overflow-y-auto text-sm">
              {feed.map((f, i) => (
                <li key={i}>
                  <span className="font-semibold">{f.who}</span> <span className="text-muted">{f.text}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </>
  );
}
