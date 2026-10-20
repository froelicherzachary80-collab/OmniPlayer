import { useState } from "react";
import { api } from "../api.ts";
import { Badge, Button, Card, Field, Input, Select, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function System() {
  const sys = useData(() => api("/api/system"), [], 5000);
  const [level, setLevel] = useState("");
  const logs = useData(() => api<any[]>(`/api/system/logs${level ? `?level=${level}` : ""}`), [level], 5000);
  const { run } = useAction();
  const [pw, setPw] = useState({ current: "", next: "" });
  const s = sys.data;
  return (
    <>
      <PageTitle title="System" sub="Workers, services, limits and logs on the host running OmniPlayer." />
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Services">
          {s && (
            <ul className="space-y-2 text-sm">
              <li>MediaMTX streaming: <Badge tone={s.services.mediamtx === "running" ? "ok" : "warn"}>{s.services.mediamtx}</Badge></li>
              <li>VoiceBox: <Badge tone={s.services.voicebox?.ok ? "ok" : "warn"}>{s.services.voicebox?.ok ? "online" : "offline"}</Badge></li>
              <li>Bot binary: <span className="break-all text-muted">{s.services.bot_binary}</span></li>
              <li>Dialogue LLM: <span className="text-muted">{s.services.llm}</span></li>
              <li>Microsoft sign-in: <span className="text-muted">{s.services.microsoft_auth}</span></li>
              <li>Java runtimes: <span className="text-muted">{s.services.java.join(", ") || "none found"}</span></li>
              <li>Host: <span className="text-muted">{s.host.platform}, {s.host.cpus} cores, {s.host.mem_total_mb} MB RAM</span></li>
            </ul>
          )}
        </Card>
        <Card title="Resource limits">
          {s && (
            <ul className="space-y-1 text-sm">
              <li>Max simultaneous bots: <b>{s.limits.max_bots}</b> (OMNI_MAX_BOTS)</li>
              <li>Max live streams: <b>{s.limits.max_streams}</b> (OMNI_MAX_STREAMS)</li>
              <li>Stream quality preset: <b>{s.limits.stream_preset}</b> (OMNI_STREAM_PRESET: low / medium / high)</li>
              <li>AI thinking interval: <b>{s.limits.think_ticks}</b> ticks (OMNI_BOT_THINK_TICKS)</li>
              <li>Bot view distance (world cache): <b>{s.limits.view_distance}</b> chunks</li>
              <li className="pt-2 text-xs text-muted">Each bot is its own process; voice and video run separately, so gameplay continues when streaming or VoiceBox is off. Capacity depends on your hardware — see docs/HARDWARE.md.</li>
            </ul>
          )}
        </Card>
        <Card title="Running workers" className="md:col-span-2">
          {!s?.workers?.length ? <p className="text-sm text-muted">No bot processes running.</p> : (
            <ul className="divide-y divide-line text-sm">
              {s.workers.map((w: any) => <li key={w.creator_id} className="flex flex-wrap justify-between gap-2 py-2"><b>{w.creator}</b><span className="text-muted">pid {w.pid} · {w.state} · {w.rss_mb ?? "?"} MB{w.streaming ? " · streaming" : ""}</span></li>)}
            </ul>
          )}
        </Card>
        <Card title="Scheduler" className="md:col-span-2">
          <ul className="max-h-48 space-y-1 overflow-y-auto text-sm">{(s?.scheduler ?? []).slice().reverse().map((l: any, i: number) => <li key={i}><span className="text-muted">{new Date(l.ts).toLocaleString()}</span> {l.text}</li>)}{!s?.scheduler?.length && <li className="text-muted">No scheduled activity yet.</li>}</ul>
        </Card>
        <Card title="Logs" className="md:col-span-2" action={<div className="w-36"><Select value={level} onChange={(e) => setLevel(e.target.value)} options={[["", "All"], ["error", "Errors"], ["warn", "Warnings"], ["info", "Info"]]} /></div>}>
          <ul className="max-h-[50dvh] space-y-1 overflow-y-auto font-mono text-xs">
            {(logs.data ?? []).slice().reverse().map((l, i) => <li key={i} className={l.level === "error" ? "text-live" : l.level === "warn" ? "text-warn" : "text-muted"}>{new Date(l.ts).toLocaleTimeString()} [{l.source}] {l.message}</li>)}
          </ul>
        </Card>
        <Card title="Security">
          <div className="space-y-2">
            <Field label="Current password"><Input type="password" value={pw.current} onChange={(e) => setPw({ ...pw, current: e.target.value })} /></Field>
            <Field label="New password"><Input type="password" value={pw.next} onChange={(e) => setPw({ ...pw, next: e.target.value })} /></Field>
            <Button onClick={() => run("pw", () => api("/api/auth/password", { body: pw }).then(() => setPw({ current: "", next: "" })), "Password changed")}>Change password</Button>
          </div>
        </Card>
      </div>
    </>
  );
}
