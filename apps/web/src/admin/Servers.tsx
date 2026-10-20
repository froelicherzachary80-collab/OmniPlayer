import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../api.ts";
import { Badge, Button, Card, Empty, Field, Input, Modal, Select, Toggle, stateTone, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function Servers() {
  const list = useData(() => api<any[]>("/api/servers"), [], 4000);
  const [open, setOpen] = useState(false);
  return (
    <>
      <PageTitle title="Minecraft servers" sub="Real Java Edition dedicated servers running on this host." action={<Button onClick={() => setOpen(true)}>+ Create server</Button>} />
      {!list.data?.length ? <Empty>No servers yet.</Empty> : (
        <div className="grid gap-3 sm:grid-cols-2">
          {list.data.map((s) => (
            <Link key={s.id} to={`/admin/servers/${s.id}`} className="rounded-2xl border border-line bg-panel p-4 hover:border-brand">
              <div className="flex items-center justify-between gap-2">
                <div className="text-lg font-bold">{s.name}</div>
                <Badge tone={stateTone(s.state)}>{s.state}</Badge>
              </div>
              <div className="mt-1 text-sm text-muted">{s.software} {s.version} · {s.auth_mode === "online" ? "Online (verified)" : "Private offline"} · {s.gamemode} · {s.difficulty}</div>
              <div className="mt-2 font-mono text-sm">{s.address}</div>
              <div className="mt-2 text-sm text-muted">{s.players.length} players · {s.ai_players.length} AI assigned{s.metrics?.pid ? ` · ${s.metrics.cpu_percent}% CPU · ${s.metrics.rss_mb} MB` : ""}</div>
            </Link>
          ))}
        </div>
      )}
      <ServerWizard open={open} onClose={() => setOpen(false)} />
    </>
  );
}

export function ServerWizard({ open, onClose }: { open: boolean; onClose: () => void }) {
  const nav = useNavigate();
  const versions = useData(() => api("/api/servers/versions"), []);
  const { busy, run } = useAction();
  const [f, setF] = useState<any>({ name: "My AI SMP", version: "26.3", software: "vanilla", auth_mode: "offline", gamemode: "survival", difficulty: "normal", seed: "", world_type: "default", max_players: 20, ai_count: 5, auto_start_bots: false, memory_mb: 3072, host: "", port: 25565 });
  const set = (k: string) => (e: any) => setF({ ...f, [k]: e?.target ? (e.target.type === "number" ? Number(e.target.value) : e.target.value) : e });
  return (
    <Modal open={open} onClose={onClose} title="Create a Minecraft server">
      <div className="space-y-3">
        <Field label="Server name"><Input value={f.name} onChange={set("name")} /></Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Software"><Select value={f.software} onChange={set("software")} options={[["vanilla", "Vanilla (official)"], ["paper", "Paper (plugins)"], ["fabric", "Fabric (mods)"], ["external", "Existing server (external)"]]} /></Field>
          <Field label="Version"><Select value={f.version} onChange={set("version")} options={(versions.data?.versions ?? ["26.3"]).map((v: string) => [v, v])} /></Field>
        </div>
        {versions.data?.warning && <p className="text-xs text-warn">{versions.data.warning}</p>}
        {f.software === "external" ? (
          <div className="grid grid-cols-2 gap-3">
            <Field label="Host"><Input value={f.host} onChange={set("host")} placeholder="mc.example.com" /></Field>
            <Field label="Port"><Input type="number" value={f.port} onChange={set("port")} /></Field>
          </div>
        ) : null}
        <Field label="Authentication mode" hint={f.auth_mode === "offline" ? "Private offline: allowlist enforced, identities NOT verified by Mojang. Lets offline AI accounts join with licensed players." : "Online: every player must use a legitimate Minecraft account."}>
          <Select value={f.auth_mode} onChange={set("auth_mode")} options={[["offline", "Private Offline (mixed accounts)"], ["online", "Online (authenticated only)"]]} />
        </Field>
        {f.software !== "external" && (
          <>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Game mode"><Select value={f.gamemode} onChange={set("gamemode")} options={[["survival", "Survival"], ["creative", "Creative"], ["adventure", "Adventure"], ["hardcore", "Hardcore"]]} /></Field>
              <Field label="Difficulty"><Select value={f.difficulty} onChange={set("difficulty")} options={[["peaceful", "Peaceful"], ["easy", "Easy"], ["normal", "Normal"], ["hard", "Hard"]]} /></Field>
              <Field label="World type"><Select value={f.world_type} onChange={set("world_type")} options={[["default", "Default"], ["flat", "Flat"], ["large_biomes", "Large biomes"], ["amplified", "Amplified"]]} /></Field>
              <Field label="Seed"><Input value={f.seed} onChange={set("seed")} placeholder="Random" /></Field>
              <Field label="Max players"><Input type="number" min={1} value={f.max_players} onChange={set("max_players")} /></Field>
              <Field label="AI player count"><Input type="number" min={0} value={f.ai_count} onChange={set("ai_count")} /></Field>
              <Field label="Memory (MB)"><Input type="number" min={1024} step={512} value={f.memory_mb} onChange={set("memory_mb")} /></Field>
            </div>
            <Toggle checked={f.auto_start_bots} onChange={set("auto_start_bots")} label="Auto-start assigned AI players" />
            {["paper", "fabric"].includes(f.software) && <Toggle checked={!!f.bedrock} onChange={set("bedrock")} label="Let iPhone / Bedrock Minecraft players join (Geyser)" />}
          </>
        )}
        <Button className="w-full" busy={busy === "c"} onClick={() => run("c", async () => { const s = await api("/api/servers", { body: f }); onClose(); nav(`/admin/servers/${s.id}`); }, "Server created")}>Create server</Button>
      </div>
    </Modal>
  );
}
