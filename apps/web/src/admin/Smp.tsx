import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../api.ts";
import { Avatar, Badge, Button, Card, Field, Input, Select, Toggle, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function Smp() {
  const presets = useData(() => api<any[]>("/api/smp/presets"), []);
  const creators = useData(() => api<any[]>("/api/creators"), []);
  const nav = useNavigate();
  const { busy, run } = useAction();
  const [f, setF] = useState<any>({ name: "AI Creator SMP", version: "26.3", software: "paper", preset: "creator", seed: "", player_limit: 20, difficulty: "normal", humans: "", creator_ids: [] as string[], pvp: true, auto_live: true, bedrock: true, bedrock_players: "", teams: [] as { name: string; members: string }[] });
  const p = presets.data?.find((x) => x.id === f.preset);
  const toggle = (id: string) => setF({ ...f, creator_ids: f.creator_ids.includes(id) ? f.creator_ids.filter((x: string) => x !== id) : [...f.creator_ids, id] });
  return (
    <>
      <PageTitle title="AI SMP generator" sub="Creates a real joinable server with the preset's rules, your humans and your AI creators." />
      <div className="space-y-4">
        <Card title="Format">
          <div className="grid gap-2 sm:grid-cols-2">
            {(presets.data ?? []).map((x) => (
              <button key={x.id} disabled={!x.available} onClick={() => setF({ ...f, preset: x.id })} className={`rounded-2xl border p-3 text-left disabled:opacity-50 ${f.preset === x.id ? "border-brand bg-brand/10" : "border-line bg-panel2"}`}>
                <div className="flex items-center justify-between gap-2 font-bold">{x.name}{x.experimental && <Badge tone="warn">experimental</Badge>}{!x.available && <Badge>not available</Badge>}</div>
                <div className="mt-1 text-xs text-muted">{x.available ? x.description : x.unavailable_reason}</div>
              </button>
            ))}
          </div>
        </Card>
        <Card title="Server">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="SMP name"><Input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></Field>
            <Field label="Version"><Input value={f.version} onChange={(e) => setF({ ...f, version: e.target.value })} /></Field>
            <Field label="Software"><Select value={f.software} onChange={(e) => setF({ ...f, software: e.target.value })} options={[["paper", "Paper (plugins)"], ["fabric", "Fabric (mods)"], ["vanilla", "Vanilla"]]} /></Field>
            <Field label="World seed"><Input value={f.seed} onChange={(e) => setF({ ...f, seed: e.target.value })} placeholder="Random" /></Field>
            <Field label="Player limit"><Input type="number" value={f.player_limit} onChange={(e) => setF({ ...f, player_limit: Number(e.target.value) })} /></Field>
            <Field label="Difficulty"><Select value={f.difficulty} onChange={(e) => setF({ ...f, difficulty: e.target.value })} options={[["peaceful", "Peaceful"], ["easy", "Easy"], ["normal", "Normal"], ["hard", "Hard"]]} /></Field>
          </div>
          <Field label="Human players (comma separated Minecraft names)" hint="Added to the allowlist; the first one becomes operator."><Input value={f.humans} onChange={(e) => setF({ ...f, humans: e.target.value })} /></Field>
          <div className="mt-2"><Toggle checked={f.pvp} onChange={(v) => setF({ ...f, pvp: v })} label="PvP allowed" /></div>
          <Toggle checked={f.auto_live} onChange={(v) => setF({ ...f, auto_live: v })} label="Creators go live automatically when they join" />
          {f.software !== "vanilla" && <Toggle checked={f.bedrock} onChange={(v) => setF({ ...f, bedrock: v })} label="iPhone / Bedrock players can join (Geyser)" />}
          {f.software !== "vanilla" && f.bedrock && <Field label="Your Xbox gamertag(s), comma separated" hint="Bedrock players sign in with Xbox; these get added to the allowlist."><Input value={f.bedrock_players} onChange={(e) => setF({ ...f, bedrock_players: e.target.value })} /></Field>}
        </Card>
        <Card title={`AI creators (${f.creator_ids.length})`}>
          <div className="grid gap-2 sm:grid-cols-2">
            {(creators.data ?? []).map((c) => (
              <button key={c.id} onClick={() => toggle(c.id)} className={`flex min-h-14 items-center gap-3 rounded-xl border p-2 text-left ${f.creator_ids.includes(c.id) ? "border-brand bg-brand/10" : "border-line"}`}>
                <Avatar src={c.avatar} name={c.streamer_name} size={36} />
                <span className="flex-1 font-semibold">{c.streamer_name}</span>
                {!c.account && <Badge tone="warn">needs account</Badge>}
              </button>
            ))}
          </div>
        </Card>
        <Card title="Teams (optional)">
          {f.teams.map((t: any, i: number) => (
            <div key={i} className="mb-2 grid grid-cols-2 gap-2">
              <Input value={t.name} onChange={(e) => setF({ ...f, teams: f.teams.map((x: any, j: number) => (j === i ? { ...x, name: e.target.value } : x)) })} placeholder="Team name" />
              <Input value={t.members} onChange={(e) => setF({ ...f, teams: f.teams.map((x: any, j: number) => (j === i ? { ...x, members: e.target.value } : x)) })} placeholder="Members (names, comma separated)" />
            </div>
          ))}
          <Button variant="soft" small onClick={() => setF({ ...f, teams: [...f.teams, { name: "", members: "" }] })}>+ Add team</Button>
        </Card>
        <Button
          className="w-full"
          disabled={!p?.available}
          busy={busy === "go"}
          onClick={() => run("go", async () => {
            const r = await api("/api/smp", {
              body: {
                name: f.name, version: f.version, software: f.software, preset: f.preset, seed: f.seed, player_limit: f.player_limit, difficulty: f.difficulty,
                humans: f.humans.split(",").map((s: string) => s.trim()).filter(Boolean), creator_ids: f.creator_ids, rules: { pvp: f.pvp }, streaming: { auto_live: f.auto_live },
                bedrock: f.software !== "vanilla" && f.bedrock, bedrock_players: f.bedrock_players.split(",").map((s: string) => s.trim()).filter(Boolean),
                teams: f.teams.filter((t: any) => t.name).map((t: any) => ({ name: t.name, members: t.members.split(",").map((s: string) => s.trim()).filter(Boolean) })),
              },
            });
            nav(`/admin/servers/${r.server.id}`);
          }, "SMP created — accept the EULA and start it")}
        >
          Create SMP server
        </Button>
      </div>
    </>
  );
}
