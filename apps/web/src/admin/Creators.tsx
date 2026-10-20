import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../api.ts";
import { Avatar, Badge, Button, Card, Empty, Field, Input, Modal, Select, TextArea, stateTone, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export const MODES: [string, string][] = [
  ["autonomous", "Fully autonomous"],
  ["follow", "Follow player"],
  ["cooperative", "Cooperative survival"],
  ["team", "Team survival"],
  ["pvp_training", "PvP training"],
  ["builder", "Builder"],
  ["explorer", "Explorer"],
  ["farmer", "Farmer"],
  ["guard", "Guard base"],
  ["streamer", "Streamer mode"],
  ["idle", "Idle (commands only)"],
];

export function Creators() {
  const list = useData(() => api<any[]>("/api/creators"), [], 5000);
  const [open, setOpen] = useState(false);
  return (
    <>
      <PageTitle title="AI Players" sub="Original fictional Minecraft creators. Each one is clearly labelled as AI-generated." action={<Button onClick={() => setOpen(true)}>+ New creator</Button>} />
      {!list.data?.length ? (
        <Empty>No creators yet. Tap “New creator” to design one or generate one from a prompt.</Empty>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {list.data.map((c) => (
            <Link key={c.id} to={`/admin/creators/${c.id}`} className="rounded-2xl border border-line bg-panel p-4 hover:border-brand">
              <div className="flex items-center gap-3">
                <Avatar src={c.avatar} name={c.streamer_name} size={48} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-lg font-bold">{c.streamer_name}</div>
                  <div className="truncate text-xs text-muted">{c.account ? `${c.account.type === "microsoft" ? "Authenticated" : "Offline"} · ${c.account.mc_username}` : "No account assigned"}</div>
                </div>
              </div>
              <p className="mt-3 line-clamp-2 text-sm text-muted">{c.persona?.personality}</p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Badge tone={stateTone(c.runtime?.state)}>{c.runtime?.state ?? "offline"}</Badge>
                {c.live?.ready && <Badge tone="live">LIVE</Badge>}
                <Badge tone="brand">{MODES.find((m) => m[0] === c.mode)?.[1] ?? c.mode}</Badge>
              </div>
            </Link>
          ))}
        </div>
      )}
      <NewCreator open={open} onClose={() => setOpen(false)} />
    </>
  );
}

function NewCreator({ open, onClose }: { open: boolean; onClose: () => void }) {
  const nav = useNavigate();
  const { busy, run } = useAction();
  const [tab, setTab] = useState<"prompt" | "manual">("prompt");
  const [prompt, setPrompt] = useState("Create a chaotic but talented Minecraft survival streamer who loves PvP, always jokes around, gets distracted by diamonds, and frequently collaborates with friends.");
  const [draft, setDraft] = useState<any>(null);
  const [name, setName] = useState("");
  const [personality, setPersonality] = useState("");
  const [mode, setMode] = useState("autonomous");

  const save = (body: any) =>
    run("save", async () => {
      const c = await api("/api/creators", { body });
      onClose();
      nav(`/admin/creators/${c.id}`);
    }, "Creator created");

  return (
    <Modal open={open} onClose={onClose} title="New AI creator">
      <div className="mb-4 grid grid-cols-2 gap-2">
        {(["prompt", "manual"] as const).map((t) => (
          <button key={t} onClick={() => setTab(t)} className={`min-h-11 rounded-xl font-semibold ${tab === t ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>
            {t === "prompt" ? "Generate from prompt" : "Build manually"}
          </button>
        ))}
      </div>
      {tab === "prompt" ? (
        <div className="space-y-3">
          <Field label="Describe your creator">
            <TextArea value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={4} />
          </Field>
          <Button variant="soft" className="w-full" busy={busy === "gen"} onClick={() => run("gen", async () => setDraft(await api("/api/creators/generate", { body: { prompt } })))}>
            ✨ Generate
          </Button>
          {draft && (
            <Card className="bg-panel2">
              <div className="flex items-center gap-3">
                <Avatar src={draft.avatar} name={draft.streamer_name} size={56} />
                <div>
                  <div className="text-lg font-bold">{draft.streamer_name}</div>
                  <div className="text-xs text-muted">{draft.category} · {MODES.find((m) => m[0] === draft.mode)?.[1]}</div>
                </div>
              </div>
              <p className="mt-2 text-sm">{draft.persona.personality}</p>
              <p className="mt-2 text-sm text-muted">Style: {draft.persona.speaking_style}</p>
              <p className="text-sm text-muted">Goals: {draft.persona.goals.join(", ")}</p>
              <p className="text-sm text-muted">Catchphrases: {draft.persona.catchphrases.join(" · ")}</p>
              <div className="mt-3 flex gap-2">
                <Button className="flex-1" busy={busy === "save"} onClick={() => save({ streamer_name: draft.streamer_name, mc_username: draft.mc_username, persona: draft.persona, mode: draft.mode, avatar: draft.avatar, stream_title: draft.stream_title, category: draft.category })}>
                  Create {draft.streamer_name}
                </Button>
                <Button variant="soft" onClick={() => run("gen", async () => setDraft(await api("/api/creators/generate", { body: { prompt } })))}>Retry</Button>
              </div>
            </Card>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          <Field label="Streamer name">
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="NovaCraft" />
          </Field>
          <Field label="Personality">
            <TextArea value={personality} onChange={(e) => setPersonality(e.target.value)} placeholder="Friendly, adventurous survival player who loves exploring with others." />
          </Field>
          <Field label="Default behaviour">
            <Select value={mode} onChange={(e) => setMode(e.target.value)} options={MODES} />
          </Field>
          <Button className="w-full" busy={busy === "save"} disabled={!name.trim()} onClick={() => save({ streamer_name: name.trim(), mode, persona: { personality: personality || `Original AI-generated Minecraft creator.` } })}>
            Create
          </Button>
        </div>
      )}
    </Modal>
  );
}
