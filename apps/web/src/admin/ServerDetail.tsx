import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { adminEvents, api, duration } from "../api.ts";
import { Badge, Button, Card, Empty, Field, Input, Select, Stat, Tabs, stateTone, useAction, useData } from "../ui/kit.tsx";

type Tab = "dash" | "console" | "players" | "config" | "backups";

export function ServerDetail() {
  const { id } = useParams();
  const nav = useNavigate();
  const s = useData(() => api(`/api/servers/${id}`), [id], 3000);
  const [tab, setTab] = useState<Tab>("dash");
  const { busy, run } = useAction();
  const [eula, setEula] = useState(false);
  if (s.error) return <Empty>{s.error}</Empty>;
  if (!s.data) return <div className="text-muted">Loading…</div>;
  const srv = s.data;
  const managed = srv.software !== "external";
  const act = (a: string, msg: string) => run(a, () => api(`/api/servers/${id}/${a}`, { method: "POST" }).then(s.reload), msg);
  return (
    <>
      <div className="mb-4 flex items-center gap-3">
        <Link to="/admin/servers" className="min-h-11 min-w-11 pt-2 text-2xl text-muted" aria-label="Back">‹</Link>
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-2xl font-extrabold">{srv.name}</h1>
          <div className="text-sm text-muted">{srv.software} {srv.version} · {srv.auth_mode === "online" ? "Online (authenticated)" : "Private offline (identities not verified)"}</div>
        </div>
        <Badge tone={stateTone(srv.state)}>{srv.state}</Badge>
      </div>
      {managed && !srv.eula_accepted_at && srv.software !== "custom" && (
        <Card className="mb-4 border-warn/50">
          <label className="flex min-h-11 items-start gap-3 text-sm">
            <input type="checkbox" className="mt-1 size-5" checked={eula} onChange={(e) => setEula(e.target.checked)} />
            <span>Before this server can start you must read and accept the <a className="text-brand underline" href="https://aka.ms/MinecraftEULA" target="_blank" rel="noreferrer">Minecraft End User License Agreement</a>.</span>
          </label>
          <Button className="mt-2" disabled={!eula} onClick={() => run("eula", () => api(`/api/servers/${id}/eula`, { body: { accept: true } }).then(s.reload), "EULA accepted")}>I accept the Minecraft EULA</Button>
        </Card>
      )}
      {managed && (
        <div className="mb-4 flex flex-wrap gap-2">
          <Button busy={busy === "start"} disabled={["running", "starting", "installing"].includes(srv.state)} onClick={() => act("start", "Starting…")}>Start</Button>
          <Button variant="soft" busy={busy === "restart"} disabled={srv.state !== "running"} onClick={() => act("restart", "Restarting…")}>Restart</Button>
          <Button variant="danger" busy={busy === "stop"} disabled={!["running", "starting"].includes(srv.state)} onClick={() => act("stop", "Stopped")}>Stop</Button>
          <Button variant="soft" busy={busy === "join"} disabled={srv.state !== "running"} onClick={() => run("join", () => api(`/api/servers/${id}/join-ai`, { body: {} }), "AI players joining")}>Join AI players</Button>
        </div>
      )}
      <Tabs<Tab> value={tab} onChange={setTab} tabs={[["dash", "Dashboard"], ...(managed ? ([["console", "Console"], ["players", "Players & allowlist"], ["config", "Configuration"], ["backups", "Backups"]] as [Tab, string][]) : [])]} />
      {tab === "dash" && (
        <>
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Stat label="Players" value={managed ? `${srv.players.length}/${srv.max_players}` : srv.ping?.players ?? "—"} />
            <Stat label="CPU" value={srv.metrics?.pid ? `${srv.metrics.cpu_percent}%` : "—"} />
            <Stat label="RAM" value={srv.metrics?.pid ? `${srv.metrics.rss_mb} MB` : "—"} sub={`limit ${srv.memory_mb} MB`} />
            <Stat label="Uptime" value={srv.metrics?.uptime_s ? duration(srv.metrics.uptime_s * 1000) : "—"} />
          </div>
          <Card title="Connect" className="mt-4">
            <div className="flex items-center gap-2 rounded-xl bg-panel2 p-3 font-mono text-lg">
              <span className="flex-1 break-all">{srv.address}</span>
              <Button small variant="soft" onClick={() => navigator.clipboard?.writeText(srv.address)}>Copy</Button>
            </div>
            {srv.last_error && <p className="mt-2 text-sm text-live">{srv.last_error}</p>}
          </Card>
          <Card title="AI players on this server" className="mt-4">
            {!srv.ai_players.length ? <Empty>No AI creators assigned. Assign one from its page or use Play.</Empty> : (
              <ul className="divide-y divide-line">{srv.ai_players.map((a: any) => <li key={a.id} className="flex min-h-12 items-center justify-between"><Link className="font-semibold" to={`/admin/creators/${a.id}`}>{a.name}</Link><Badge tone={stateTone(a.state)}>{a.state}</Badge></li>)}</ul>
            )}
          </Card>
          <div className="mt-6">
            <Button variant="danger" onClick={() => confirm("Remove this server definition? World files are kept on disk.") && run("del", async () => { await api(`/api/servers/${id}`, { method: "DELETE" }); nav("/admin/servers"); })}>Remove server</Button>
          </div>
        </>
      )}
      {tab === "console" && <Console id={id!} running={srv.state === "running"} />}
      {tab === "players" && <Players srv={srv} reload={s.reload} />}
      {tab === "config" && <Config srv={srv} reload={s.reload} />}
      {tab === "backups" && <Backups id={id!} />}
    </>
  );
}

function Console({ id, running }: { id: string; running: boolean }) {
  const initial = useData(() => api<string[]>(`/api/servers/${id}/console`), [id]);
  const [lines, setLines] = useState<string[]>([]);
  const [cmd, setCmd] = useState("");
  const { run } = useAction();
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => setLines(initial.data ?? []), [initial.data]);
  useEffect(() => adminEvents((t, d) => { if (t === "console" && d.server_id === id) setLines((l) => [...l.slice(-800), d.line]); }), [id]);
  useEffect(() => { box.current?.scrollTo(0, box.current.scrollHeight); }, [lines]);
  return (
    <Card>
      <div ref={box} className="h-[55dvh] overflow-y-auto rounded-xl bg-black p-3 font-mono text-xs leading-relaxed text-green-200">
        {lines.map((l, i) => <div key={i} className="whitespace-pre-wrap break-all">{l}</div>)}
      </div>
      <form className="mt-2 flex gap-2" onSubmit={(e) => { e.preventDefault(); if (cmd) run("cmd", () => api(`/api/servers/${id}/command`, { body: { line: cmd } }).then(() => setCmd(""))); }}>
        <Input value={cmd} onChange={(e) => setCmd(e.target.value)} placeholder={running ? "Command, e.g. time set day" : "Server not running"} disabled={!running} autoCapitalize="none" />
        <Button disabled={!running}>Run</Button>
      </form>
    </Card>
  );
}

function Players({ srv, reload }: { srv: any; reload: () => void }) {
  const [name, setName] = useState("");
  const [invite, setInvite] = useState<any>(null);
  const inv = useData(() => api<any[]>(`/api/servers/${srv.id}/invitations`), [srv.id]);
  const { run } = useAction();
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card title="Online now">
        {!srv.players.length ? <Empty>Nobody online.</Empty> : <ul className="space-y-1">{srv.players.map((p: string) => <li key={p}>{p}</li>)}</ul>}
      </Card>
      <Card title="Allowlist">
        <form className="mb-3 flex gap-2" onSubmit={(e) => { e.preventDefault(); run("wl", () => api(`/api/servers/${srv.id}/whitelist`, { body: { name } }).then(() => { setName(""); reload(); }), "Added"); }}>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Minecraft username" autoCapitalize="none" />
          <Button>Add</Button>
        </form>
        <ul className="divide-y divide-line">
          {srv.whitelist.map((n: string) => (
            <li key={n} className="flex min-h-11 items-center justify-between">
              {n}
              <button className="min-h-11 px-3 text-muted" onClick={() => run("rm", () => api(`/api/servers/${srv.id}/whitelist/${n}`, { method: "DELETE" }).then(reload))} aria-label={`Remove ${n}`}>×</button>
            </li>
          ))}
        </ul>
      </Card>
      <Card title="Invite a human player" className="md:col-span-2">
        <form className="flex gap-2" onSubmit={async (e) => { e.preventDefault(); const f = new FormData(e.currentTarget); await run("inv", async () => { setInvite(await api(`/api/servers/${srv.id}/invite`, { body: { mc_username: f.get("n") } })); reload(); inv.reload(); }); }}>
          <Input name="n" placeholder="Friend's Minecraft username" autoCapitalize="none" />
          <Button>Invite</Button>
        </form>
        {invite && <div className="mt-3 rounded-xl bg-panel2 p-3 text-sm"><b>{invite.mc_username}</b> can now join <span className="font-mono">{invite.address}</span>.<br /><span className="text-muted">{invite.note}</span></div>}
        <ul className="mt-3 space-y-1 text-sm text-muted">{(inv.data ?? []).map((i) => <li key={i.id}>{i.mc_username} — invited {new Date(i.created_at).toLocaleString()}</li>)}</ul>
      </Card>
    </div>
  );
}

function Config({ srv, reload }: { srv: any; reload: () => void }) {
  const [f, setF] = useState<any>({ name: srv.name, version: srv.version, gamemode: srv.gamemode, difficulty: srv.difficulty, max_players: srv.max_players, memory_mb: srv.memory_mb, port: srv.port, auth_mode: srv.auth_mode, properties: srv.properties ?? {} });
  const [key, setKey] = useState("");
  const [val, setVal] = useState("");
  const { busy, run } = useAction();
  const set = (k: string) => (e: any) => setF({ ...f, [k]: e.target.type === "number" ? Number(e.target.value) : e.target.value });
  return (
    <Card title="Configuration" action={<span className="text-xs text-muted">Applied on next start/restart</span>}>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Name"><Input value={f.name} onChange={set("name")} /></Field>
        <Field label="Version" hint="Changing version requires deleting server.jar so the new one downloads."><Input value={f.version} onChange={set("version")} /></Field>
        <Field label="Port"><Input type="number" value={f.port} onChange={set("port")} /></Field>
        <Field label="Authentication"><Select value={f.auth_mode} onChange={set("auth_mode")} options={[["offline", "Private offline"], ["online", "Online"]]} /></Field>
        <Field label="Game mode"><Select value={f.gamemode} onChange={set("gamemode")} options={[["survival", "Survival"], ["creative", "Creative"], ["adventure", "Adventure"], ["hardcore", "Hardcore"]]} /></Field>
        <Field label="Difficulty"><Select value={f.difficulty} onChange={set("difficulty")} options={[["peaceful", "Peaceful"], ["easy", "Easy"], ["normal", "Normal"], ["hard", "Hard"]]} /></Field>
        <Field label="Max players"><Input type="number" value={f.max_players} onChange={set("max_players")} /></Field>
        <Field label="Memory (MB)"><Input type="number" value={f.memory_mb} onChange={set("memory_mb")} /></Field>
      </div>
      <h3 className="mb-2 mt-4 font-bold">Extra server.properties</h3>
      <ul className="mb-2 space-y-1 font-mono text-sm">
        {Object.entries(f.properties).map(([k, v]) => <li key={k} className="flex justify-between gap-2"><span>{k}={String(v)}</span><button className="text-muted" onClick={() => { const p = { ...f.properties }; delete p[k]; setF({ ...f, properties: p }); }}>×</button></li>)}
      </ul>
      <div className="flex gap-2">
        <Input value={key} onChange={(e) => setKey(e.target.value)} placeholder="key (e.g. pvp)" autoCapitalize="none" />
        <Input value={val} onChange={(e) => setVal(e.target.value)} placeholder="value" autoCapitalize="none" />
        <Button variant="soft" onClick={() => { if (key) { setF({ ...f, properties: { ...f.properties, [key]: val } }); setKey(""); setVal(""); } }}>Add</Button>
      </div>
      <Button className="mt-4" busy={busy === "s"} onClick={() => run("s", () => api(`/api/servers/${srv.id}`, { method: "PUT", body: f }).then(reload), "Saved")}>Save configuration</Button>
    </Card>
  );
}

function Backups({ id }: { id: string }) {
  const b = useData(() => api<any[]>(`/api/servers/${id}/backups`), [id]);
  const { busy, run } = useAction();
  return (
    <Card title="World backups" action={<Button small busy={busy === "b"} onClick={() => run("b", () => api(`/api/servers/${id}/backup`, { method: "POST" }).then(b.reload), "Backup created")}>Back up now</Button>}>
      {!b.data?.length ? <Empty>No backups yet. Saving is paused during the copy so the world stays consistent.</Empty> : (
        <ul className="divide-y divide-line text-sm">{b.data.map((x) => <li key={x.file} className="flex min-h-11 items-center justify-between"><span className="break-all font-mono">{x.file}</span><span className="text-muted">{(x.size / 1048576).toFixed(1)} MB</span></li>)}</ul>
      )}
    </Card>
  );
}
