import { Link } from "react-router-dom";
import { api, duration } from "../api.ts";
import { Avatar, Badge, Button, Card, Empty, stateTone, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function LiveAdmin() {
  const creators = useData(() => api<any[]>("/api/creators"), [], 4000);
  const history = useData(() => api<any[]>("/api/streams"), [], 15000);
  const { busy, run } = useAction();
  return (
    <>
      <PageTitle title="MineLive" sub="Channels show each bot's real first-person view rendered from its live server connection." action={<Link to="/live"><Button variant="soft">Open MineLive ↗</Button></Link>} />
      <Card title="Channels">
        {!creators.data?.length ? <Empty>Create an AI creator first.</Empty> : (
          <ul className="divide-y divide-line">
            {creators.data.map((c) => {
              const online = c.runtime?.state === "online";
              return (
                <li key={c.id} className="flex flex-wrap items-center gap-3 py-3">
                  <Avatar src={c.avatar} name={c.streamer_name} />
                  <div className="min-w-0 flex-1">
                    <div className="font-semibold">{c.streamer_name}</div>
                    <div className="truncate text-xs text-muted">/live/creator/{c.slug} · schedule: {c.schedule?.mode}</div>
                  </div>
                  <Badge tone={stateTone(c.runtime?.state)}>{c.runtime?.state ?? "offline"}</Badge>
                  {c.live?.ready ? (
                    <>
                      <Badge tone="live">LIVE {duration(Date.now() - c.live.started_at)}</Badge>
                      <Link to={`/live/creator/${c.slug}`}><Button small variant="soft">Watch</Button></Link>
                      <Button small variant="danger" busy={busy === c.id} onClick={() => run(c.id, () => api(`/api/streams/${c.id}/stop`, { method: "POST" }).then(creators.reload), "Stream ended")}>End</Button>
                    </>
                  ) : (
                    <Button small disabled={!online} busy={busy === c.id} onClick={() => run(c.id, () => api(`/api/streams/${c.id}/start`, { body: {} }).then(creators.reload), "Live!")} title={online ? "" : "Bot must be in a server first"}>◉ Go live</Button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </Card>
      <Card title="Past broadcasts" className="mt-4">
        {!history.data?.length ? <Empty>No broadcasts yet.</Empty> : (
          <ul className="divide-y divide-line text-sm">
            {history.data.map((s) => (
              <li key={s.id} className="flex flex-wrap justify-between gap-2 py-2">
                <span className="font-semibold">{s.title}</span>
                <span className="text-muted">{new Date(s.started_at).toLocaleString()} · {s.ended_at ? duration(s.ended_at - s.started_at) : s.status} · peak {s.peak_viewers} viewers</span>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </>
  );
}
