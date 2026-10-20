import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { adminEvents, api, timeAgo } from "../api.ts";
import { Avatar, Badge, Button, Card, Empty, Field, Input, Select, Tabs, TextArea, Toggle, stateTone, useAction, useData } from "../ui/kit.tsx";
import { SkinPreview } from "../ui/Skin.tsx";
import { MODES } from "./Creators.tsx";

type Tab = "control" | "profile" | "voice" | "schedule" | "memory" | "chat" | "logs";
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function CreatorDetail() {
  const { id } = useParams();
  const nav = useNavigate();
  const c = useData(() => api(`/api/creators/${id}`), [id], 3000);
  const [tab, setTab] = useState<Tab>("control");
  const { busy, run } = useAction();
  if (c.error) return <Empty>{c.error}</Empty>;
  if (!c.data) return <div className="text-muted">Loading…</div>;
  const cr = c.data;
  const save = (patch: any, msg = "Saved") => run("save", () => api(`/api/creators/${id}`, { method: "PUT", body: patch }).then(() => c.reload()), msg);
  return (
    <>
      <div className="mb-4 flex items-center gap-3">
        <Link to="/admin/creators" className="min-h-11 min-w-11 pt-2 text-2xl text-muted" aria-label="Back">‹</Link>
        <Avatar src={cr.avatar} name={cr.streamer_name} size={52} />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-2xl font-extrabold">{cr.streamer_name}</h1>
          <div className="flex flex-wrap gap-2">
            <Badge tone={stateTone(cr.runtime?.state)}>{cr.runtime?.state ?? "offline"}</Badge>
            {cr.live?.ready && <Badge tone="live">LIVE</Badge>}
            <Badge>AI-generated creator</Badge>
          </div>
        </div>
      </div>
      <Tabs<Tab> value={tab} onChange={setTab} tabs={[["control", "Control"], ["profile", "Profile & skin"], ["voice", "Voice"], ["schedule", "Schedule"], ["memory", "Memory"], ["chat", "Chat rules"], ["logs", "Logs"]]} />
      {tab === "control" && <Control cr={cr} reload={c.reload} />}
      {tab === "profile" && <Profile cr={cr} save={save} busy={busy} reload={c.reload} />}
      {tab === "voice" && <VoiceTab cr={cr} save={save} />}
      {tab === "schedule" && <ScheduleTab cr={cr} save={save} />}
      {tab === "memory" && <MemoryTab id={cr.id} />}
      {tab === "chat" && <ChatRules cr={cr} reload={c.reload} />}
      {tab === "logs" && <Logs id={cr.id} />}
      <div className="mt-8">
        <Button variant="danger" busy={busy === "del"} onClick={() => confirm(`Delete ${cr.streamer_name} and all of their memories?`) && run("del", async () => { await api(`/api/creators/${id}`, { method: "DELETE" }); nav("/admin/creators"); }, "Deleted")}>
          Delete creator
        </Button>
      </div>
    </>
  );
}

function Control({ cr, reload }: { cr: any; reload: () => void }) {
  const servers = useData(() => api<any[]>("/api/servers"), []);
  const accounts = useData(() => api<any[]>("/api/accounts"), []);
  const [sid, setSid] = useState<string>(cr.server_id ?? "");
  const check = useData(() => (sid ? api(`/api/creators/${cr.id}/check?server_id=${sid}`) : Promise.resolve(null)), [sid, cr.account_id, cr.runtime?.state], 6000);
  const { busy, run } = useAction();
  const [say, setSay] = useState("");
  const st = cr.runtime?.status;
  const running = cr.runtime && !["stopped", "error"].includes(cr.runtime.state);
  const cmd = (body: any, ok?: string) => run(body.cmd, () => api(`/api/creators/${cr.id}/command`, { body }), ok);
  const accs = (accounts.data ?? []).filter((a) => !a.assigned_creator_id || a.assigned_creator_id === cr.id);
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card title="Connection">
        <div className="space-y-3">
          <Field label="Minecraft account" hint={<Link className="text-brand" to="/admin/accounts">Manage accounts</Link>}>
            <Select
              value={cr.account_id ?? ""}
              disabled={running}
              onChange={(e) => run("acc", () => api(`/api/accounts/${e.target.value || cr.account_id}/assign`, { body: { creator_id: e.target.value ? cr.id : null } }).then(reload))}
              options={[["", "— none —"], ...accs.map((a: any) => [a.id, `${a.type === "microsoft" ? "Authenticated Minecraft" : "Offline Minecraft"} · ${a.mc_username}`] as [string, string])]}
            />
          </Field>
          <Field label="Server">
            <Select value={sid} disabled={running} onChange={(e) => setSid(e.target.value)} options={[["", "— choose —"], ...(servers.data ?? []).map((s) => [s.id, `${s.name} (${s.auth_mode}, ${s.state})`] as [string, string])]} />
          </Field>
          {check.data && (
            <div className="space-y-1 text-sm">
              {check.data.problems.map((p: string, i: number) => <div key={i} className="text-live">✗ {p}</div>)}
              {check.data.notes.map((p: string, i: number) => <div key={i} className="text-muted">• {p}</div>)}
              {check.data.ok && <div className="text-brand2">✓ Ready to join</div>}
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            {!running ? (
              <Button busy={busy === "start"} disabled={!check.data?.ok && !running} onClick={() => run("start", () => api(`/api/creators/${cr.id}/start`, { body: { server_id: sid } }).then(reload), "Joining the server…")}>
                Join server
              </Button>
            ) : (
              <Button variant="danger" busy={busy === "stop"} onClick={() => run("stop", () => api(`/api/creators/${cr.id}/stop`, { method: "POST" }).then(reload), "Disconnected")}>
                Leave server
              </Button>
            )}
            {running && cr.runtime.state === "online" && (cr.live ? (
              <Button variant="soft" busy={busy === "live"} onClick={() => run("live", () => api(`/api/streams/${cr.id}/stop`, { method: "POST" }).then(reload), "Stream ended")}>■ End stream</Button>
            ) : (
              <Button variant="soft" busy={busy === "live"} onClick={() => run("live", () => api(`/api/streams/${cr.id}/start`, { body: {} }).then(reload), "Live on MineLive")}>◉ Go live</Button>
            ))}
            {cr.live?.ready && <Link to={`/live/creator/${cr.slug}`}><Button variant="ghost">Watch ↗</Button></Link>}
          </div>
          {cr.runtime?.error && <p className="text-sm text-live">{cr.runtime.error}</p>}
        </div>
      </Card>
      <Card title="Behaviour">
        <div className="space-y-3">
          <Field label="AI mode">
            <Select value={cr.mode} onChange={(e) => run("mode", () => api(`/api/creators/${cr.id}/mode`, { body: { mode: e.target.value, target: cr.mode_target } }).then(reload), "Mode changed")} options={MODES} />
          </Field>
          {["follow", "cooperative", "pvp_training"].includes(cr.mode) && (
            <Field label="Target player">
              <Input defaultValue={cr.mode_target ?? ""} onBlur={(e) => run("mode", () => api(`/api/creators/${cr.id}/mode`, { body: { mode: cr.mode, target: e.target.value } }).then(reload))} placeholder="Your Minecraft name" />
            </Field>
          )}
          {running && (
            <>
              <div className="flex gap-2">
                <Input value={say} onChange={(e) => setSay(e.target.value)} placeholder="Say in Minecraft chat…" />
                <Button onClick={() => say && cmd({ cmd: "chat", text: say }).then(() => setSay(""))}>Send</Button>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Button variant="soft" small onClick={() => cmd({ cmd: "gather", what: "wood", count: 8 }, "Collecting wood")}>Collect wood</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "gather", what: "stone", count: 8 }, "Mining stone")}>Mine stone</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "build_house" }, "Building a house")}>Build house</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "attack_nearest" }, "Looking for a fight")}>Fight hostiles</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "eat" })}>Eat</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "stop", seconds: 60 }, "Paused for 60s")}>Pause</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "craft", item: "wooden_pickaxe" })}>Craft pickaxe</Button>
                <Button variant="soft" small onClick={() => cmd({ cmd: "say_commentary", event: "goal", detail: st?.goal || "explore" })}>Say something</Button>
              </div>
            </>
          )}
        </div>
      </Card>
      {st && (
        <Card title="In game (live)" className="md:col-span-2">
          <div className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
            <div><div className="text-muted">Position</div><div className="font-mono">{st.pos?.map((v: number) => Math.round(v)).join(" ")}</div></div>
            <div><div className="text-muted">Health / Food</div><div>❤ {st.health?.toFixed(1)} · 🍗 {st.food}</div></div>
            <div><div className="text-muted">Biome</div><div>{st.biome ?? "—"}</div></div>
            <div><div className="text-muted">Goal</div><div>{st.goal || "—"}</div></div>
          </div>
          <div className="mt-3 text-sm"><span className="text-muted">Inventory: </span>{st.inventory?.length ? st.inventory.map(([n, k]: [string, number]) => `${n.replace(/_/g, " ")} ×${k}`).join(", ") : "empty"}</div>
          <div className="mt-1 text-sm"><span className="text-muted">Nearby: </span>{st.nearby?.length ? st.nearby.map((e: any) => `${e.name ?? e.kind} (${e.distance}m)`).join(", ") : "nothing"}</div>
          <div className="mt-1 text-sm"><span className="text-muted">Players online: </span>{st.players_online?.join(", ")}</div>
        </Card>
      )}
    </div>
  );
}

function Profile({ cr, save, busy, reload }: { cr: any; save: (p: any, m?: string) => void; busy: string | null; reload: () => void }) {
  const [p, setP] = useState(cr.persona);
  const [name, setName] = useState(cr.streamer_name);
  const [title, setTitle] = useState(cr.stream_title);
  const [category, setCategory] = useState(cr.category);
  const [role, setRole] = useState(cr.role ?? "");
  const [transcript, setTranscript] = useState("");
  const [style, setStyle] = useState<any>(null);
  const { run } = useAction();
  const list = (k: string) => (p[k] ?? []).join(", ");
  const setList = (k: string, v: string) => setP({ ...p, [k]: v.split(",").map((s: string) => s.trim()).filter(Boolean) });
  const upload = (path: string, file: File) => {
    const fd = new FormData();
    fd.append("file", file);
    return run("up", () => api(path, { method: "POST", raw: fd }).then(reload), "Uploaded");
  };
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card title="Identity">
        <div className="space-y-3">
          <Field label="Streamer name"><Input value={name} onChange={(e) => setName(e.target.value)} /></Field>
          <Field label="Stream title"><Input value={title} onChange={(e) => setTitle(e.target.value)} /></Field>
          <Field label="Category"><Select value={category} onChange={(e) => setCategory(e.target.value)} options={["Survival", "PvP", "Building", "Adventure", "Hardcore", "SMP"].map((x) => [x, x])} /></Field>
          <Field label="Team role" hint="Used when several bots get the same request (builder builds, others gather)."><Select value={role} onChange={(e) => setRole(e.target.value)} options={[["", "Any"], ["builder", "Builder"], ["miner", "Miner"], ["gatherer", "Gatherer"], ["guard", "Guard"]]} /></Field>
          <div className="flex items-center gap-3">
            <Avatar src={cr.avatar} name={cr.streamer_name} size={56} />
            <label className="cursor-pointer text-sm font-semibold text-brand">
              Upload profile picture
              <input type="file" accept="image/*" className="hidden" onChange={(e) => e.target.files?.[0] && upload(`/api/creators/${cr.id}/avatar`, e.target.files[0])} />
            </label>
          </div>
        </div>
      </Card>
      <Card title="Minecraft skin">
        <div className="flex gap-4">
          <SkinPreview src={cr.skin} />
          <div className="space-y-2 text-sm">
            <label className="block cursor-pointer font-semibold text-brand">
              Upload 64×64 skin PNG
              <input type="file" accept="image/png" className="hidden" onChange={(e) => e.target.files?.[0] && upload(`/api/creators/${cr.id}/skin`, e.target.files[0])} />
            </label>
            <Button small variant="soft" onClick={() => run("skin", () => api(`/api/creators/${cr.id}/skin/generate`, { method: "POST" }).then(reload), "New original skin generated")}>Generate original skin</Button>
            {cr.skin && <a className="block text-brand" href={cr.skin} download={`${cr.slug}.png`}>Download skin</a>}
            <p className="text-xs text-muted">Authenticated accounts use their Mojang skin in game. Offline accounts show custom skins only on private servers with a skin plugin (e.g. SkinsRestorer on Paper) — see docs/ACCOUNTS.md.</p>
          </div>
        </div>
      </Card>
      <Card title="Personality" className="md:col-span-2">
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="Personality"><TextArea value={p.personality} onChange={(e) => setP({ ...p, personality: e.target.value })} /></Field>
          <Field label="Speaking style"><TextArea value={p.speaking_style} onChange={(e) => setP({ ...p, speaking_style: e.target.value })} /></Field>
          <Field label="Catchphrases (comma separated)"><Input value={list("catchphrases")} onChange={(e) => setList("catchphrases", e.target.value)} /></Field>
          <Field label="Gameplay preferences / likes"><Input value={list("likes")} onChange={(e) => setList("likes", e.target.value)} /></Field>
          <Field label="Personal goals"><Input value={list("goals")} onChange={(e) => setList("goals", e.target.value)} /></Field>
          <Field label="Friends (player names)"><Input value={list("friends")} onChange={(e) => setList("friends", e.target.value)} /></Field>
          <Field label="Rivals (player names)"><Input value={list("rivals")} onChange={(e) => setList("rivals", e.target.value)} /></Field>
          <Field label="Skill level"><Select value={p.skill_level} onChange={(e) => setP({ ...p, skill_level: e.target.value })} options={[["beginner", "Beginner"], ["intermediate", "Intermediate"], ["expert", "Expert"]]} /></Field>
          {(["chattiness", "energy", "risk"] as const).map((k) => (
            <Field key={k} label={`${k[0].toUpperCase() + k.slice(1)}: ${Math.round((p[k] ?? 0) * 100)}%`}>
              <input type="range" min={0} max={1} step={0.05} value={p[k] ?? 0} onChange={(e) => setP({ ...p, [k]: Number(e.target.value) })} className="h-11 w-full accent-[var(--color-brand)]" />
            </Field>
          ))}
        </div>
        <Button className="mt-3" busy={busy === "save"} onClick={() => save({ streamer_name: name, stream_title: title, category, role, persona: p })}>Save profile</Button>
      </Card>
      <Card title="Learn style from a transcript (optional)" className="md:col-span-2">
        <p className="mb-2 text-sm text-muted">Paste a gameplay transcript you are allowed to use. Only general commentary metrics and gameplay topics are extracted — names, voice and catchphrases of the source creator are not copied.</p>
        <TextArea value={transcript} onChange={(e) => setTranscript(e.target.value)} rows={5} placeholder="Paste transcript text…" />
        <div className="mt-2 flex gap-2">
          <Button variant="soft" onClick={() => run("tr", async () => setStyle(await api(`/api/creators/${cr.id}/transcript`, { raw: transcript, headers: { "Content-Type": "text/plain" } })))}>Analyse</Button>
          {style && <Button onClick={() => run("tr", () => api(`/api/creators/${cr.id}/transcript?apply=1`, { raw: transcript, headers: { "Content-Type": "text/plain" } }).then(reload), "Applied to personality")}>Apply</Button>}
        </div>
        {style && <pre className="mt-2 overflow-x-auto rounded-xl bg-panel2 p-3 text-xs">{JSON.stringify(style, null, 2)}</pre>}
      </Card>
    </div>
  );
}

function VoiceTab({ cr, save }: { cr: any; save: (p: any, m?: string) => void }) {
  const voices = useData(() => api<any[]>("/api/voices"), []);
  const [v, setV] = useState(cr.voice);
  const [text, setText] = useState("WAIT, NO WAY! I actually found diamonds down here!");
  const [audio, setAudio] = useState<string | null>(null);
  const { busy, run } = useAction();
  return (
    <Card title="VoiceBox voice">
      <div className="space-y-3">
        <Field label="Voice" hint={<Link className="text-brand" to="/admin/voices">Manage voices & uploads</Link>}>
          <Select value={v.voice_id ?? ""} onChange={(e) => setV({ ...v, voice_id: e.target.value })} options={(voices.data ?? []).map((x) => [x.id, `${x.name}${x.original ? " (original)" : " (uploaded, consented)"}`])} />
        </Field>
        {(["rate", "pitch", "volume"] as const).map((k) => (
          <Field key={k} label={`${k}: ${v[k]}`}>
            <input type="range" min={k === "pitch" ? -10 : 0.5} max={k === "pitch" ? 10 : 2} step={k === "pitch" ? 1 : 0.05} value={v[k]} onChange={(e) => setV({ ...v, [k]: Number(e.target.value) })} className="h-11 w-full accent-[var(--color-brand)]" />
          </Field>
        ))}
        <Field label="Test line"><Input value={text} onChange={(e) => setText(e.target.value)} /></Field>
        <div className="flex flex-wrap gap-2">
          <Button variant="soft" busy={busy === "test"} onClick={() => run("test", async () => {
            const r = await fetch("/api/voices/test", { method: "POST", headers: { "Content-Type": "application/json", Authorization: `Bearer ${localStorage.getItem("omniplayer.token")}` }, body: JSON.stringify({ voice_id: v.voice_id, text, emotion: "excited" }) });
            if (!r.ok) throw new Error((await r.json()).error);
            setAudio(URL.createObjectURL(await r.blob()));
          })}>▶ Test voice</Button>
          <Button onClick={() => save({ voice: v }, "Voice saved")}>Save</Button>
        </div>
        {audio && <audio src={audio} controls autoPlay className="w-full" />}
      </div>
    </Card>
  );
}

function ScheduleTab({ cr, save }: { cr: any; save: (p: any, m?: string) => void }) {
  const [s, setS] = useState(cr.schedule);
  const servers = useData(() => api<any[]>("/api/servers"), []);
  const upd = (i: number, patch: any) => setS({ ...s, entries: s.entries.map((e: any, j: number) => (j === i ? { ...e, ...patch } : e)) });
  return (
    <Card title="Streaming schedule">
      <div className="space-y-3">
        <Field label="Mode">
          <Select value={s.mode} onChange={(e) => setS({ ...s, mode: e.target.value })} options={[["manual", "Manual (Go live button)"], ["scheduled", "Scheduled"], ["auto_when_online", "Automatically when the bot is online"]]} />
        </Field>
        <Field label="Timezone" hint="IANA name, e.g. America/New_York"><Input value={s.timezone} onChange={(e) => setS({ ...s, timezone: e.target.value })} /></Field>
        {s.mode === "scheduled" && (
          <>
            {s.entries.map((e: any, i: number) => (
              <div key={i} className="space-y-2 rounded-xl border border-line p-3">
                <div className="flex flex-wrap gap-1">
                  {DAYS.map((d, di) => (
                    <button key={d} onClick={() => upd(i, { days: e.days.includes(di) ? e.days.filter((x: number) => x !== di) : [...e.days, di] })} className={`min-h-10 min-w-11 rounded-lg text-sm font-semibold ${e.days.includes(di) ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>{d}</button>
                  ))}
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <Field label="Start"><Input type="time" value={e.start} onChange={(ev) => upd(i, { start: ev.target.value })} /></Field>
                  <Field label="Minutes"><Input type="number" min={10} value={e.duration_min} onChange={(ev) => upd(i, { duration_min: Number(ev.target.value) })} /></Field>
                </div>
                <Field label="Server"><Select value={e.server_id ?? ""} onChange={(ev) => upd(i, { server_id: ev.target.value || null })} options={[["", "Creator's default server"], ...(servers.data ?? []).map((x) => [x.id, x.name] as [string, string])]} /></Field>
                <Field label="Activity"><Input value={e.activity ?? ""} onChange={(ev) => upd(i, { activity: ev.target.value })} placeholder="Building stream" /></Field>
                <Button variant="ghost" small onClick={() => setS({ ...s, entries: s.entries.filter((_: any, j: number) => j !== i) })}>Remove</Button>
              </div>
            ))}
            <Button variant="soft" onClick={() => setS({ ...s, entries: [...s.entries, { days: [1, 2, 3, 4, 5], start: "16:00", duration_min: 120, server_id: null }] })}>+ Add time slot</Button>
          </>
        )}
        <Button onClick={() => save({ schedule: s }, "Schedule saved")}>Save schedule</Button>
        <p className="text-xs text-muted">The background scheduler checks every 30 s. A stream starts only once the bot has joined the server and the video pipeline is ready.</p>
      </div>
    </Card>
  );
}

function MemoryTab({ id }: { id: string }) {
  const m = useData(() => api(`/api/creators/${id}/memory`), [id], 8000);
  const { run } = useAction();
  if (!m.data) return <div className="text-muted">Loading…</div>;
  const by = (k: string) => m.data.memories.filter((x: any) => x.kind === k);
  const del = (kind: string, key: string) => run("del", () => api(`/api/creators/${id}/memory/${encodeURIComponent(kind)}/${encodeURIComponent(key)}`, { method: "DELETE" }).then(m.reload));
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card title={`Players met (${by("player").length})`}>
        {!by("player").length ? <Empty>No one yet.</Empty> : (
          <ul className="divide-y divide-line text-sm">
            {by("player").map((x: any) => (
              <li key={x.key} className="flex items-center gap-2 py-2">
                <span className="flex-1 font-semibold">{x.key.replace(/^viewer:/, "👁 ")}</span>
                <span className="text-muted">{x.data.interactions ?? 0} chats · affinity {x.data.affinity ?? 0}</span>
                <button className="min-h-9 px-2 text-muted" onClick={() => del("player", x.key)} aria-label="Forget">×</button>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <Card title="Places">
        {!by("location").length ? <Empty>No remembered places.</Empty> : (
          <ul className="space-y-1 text-sm">{by("location").map((x: any) => <li key={x.key}><b>{x.key}</b> <span className="font-mono text-muted">{x.data.pos?.join(" ")}</span></li>)}</ul>
        )}
      </Card>
      <Card title="Learned strategies & stats">
        <ul className="space-y-1 text-sm">
          {[...by("strategy"), ...by("stat")].map((x: any) => <li key={x.kind + x.key}><b>{x.key}</b>: {String(x.data.value)}</li>)}
          {![...by("strategy"), ...by("stat")].length && <li className="text-muted">Nothing learned yet. Strategy weights change after deaths and wins.</li>}
        </ul>
      </Card>
      <Card title="Memorable events & sessions">
        <ul className="max-h-72 space-y-1 overflow-y-auto text-sm">
          {by("event").map((x: any) => <li key={x.key}><span className="text-muted">{timeAgo(Number(x.key))}</span> {x.data.text}</li>)}
          {m.data.sessions.map((s: any) => <li key={s.id} className="text-muted">Session {new Date(s.started_at).toLocaleString()} — {s.ended_at ? s.end_reason : "running"}</li>)}
        </ul>
      </Card>
    </div>
  );
}

function ChatRules({ cr, reload }: { cr: any; reload: () => void }) {
  const mod = cr.moderation ?? { banned_words: [], block_links: true, max_caps_ratio: 0.8, slow_mode_s: 0 };
  const [m, setM] = useState(mod);
  const { run } = useAction();
  return (
    <Card title={`${cr.streamer_name}'s chat moderation`}>
      <div className="space-y-3">
        <Field label="Blocked words (comma separated)"><Input value={m.banned_words.join(", ")} onChange={(e) => setM({ ...m, banned_words: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })} /></Field>
        <Toggle checked={m.block_links} onChange={(v) => setM({ ...m, block_links: v })} label="Remove messages with links" />
        <Field label={`Max capital letters: ${Math.round(m.max_caps_ratio * 100)}%`}><input type="range" min={0.3} max={1} step={0.05} value={m.max_caps_ratio} onChange={(e) => setM({ ...m, max_caps_ratio: Number(e.target.value) })} className="h-11 w-full" /></Field>
        <Field label="Slow mode (seconds between messages per viewer)"><Input type="number" min={0} value={m.slow_mode_s} onChange={(e) => setM({ ...m, slow_mode_s: Number(e.target.value) })} /></Field>
        <Button onClick={() => run("m", () => api(`/api/creators/${cr.id}/moderation`, { method: "PUT", body: m }).then(reload), "Chat rules saved")}>Save rules</Button>
      </div>
    </Card>
  );
}

function Logs({ id }: { id: string }) {
  const rt = useData(() => api(`/api/creators/${id}/runtime`), [id], 3000);
  const [live, setLive] = useState<string[]>([]);
  useEffect(() => adminEvents((type, d) => {
    if ((type === "bot_log" || type === "commentary") && d.creator_id === id) setLive((l) => [`${type === "commentary" ? "🎙 " + d.text : `[${d.level}] ${d.message}`}`, ...l].slice(0, 200));
  }), [id]);
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Card title="Activity">
        <ul className="max-h-[60dvh] space-y-1 overflow-y-auto text-sm">
          {[...(rt.data?.activity ?? [])].reverse().map((a: any, i: number) => <li key={i}><span className="text-muted">{new Date(a.ts).toLocaleTimeString()}</span> {a.text}</li>)}
        </ul>
      </Card>
      <Card title="Logs & commentary">
        <ul className="max-h-[60dvh] space-y-1 overflow-y-auto font-mono text-xs">
          {live.map((l, i) => <li key={"l" + i}>{l}</li>)}
          {[...(rt.data?.logs ?? [])].reverse().map((l: any, i: number) => <li key={i} className={l.level === "error" ? "text-live" : "text-muted"}>{new Date(l.ts).toLocaleTimeString()} [{l.level}] {l.message}</li>)}
        </ul>
      </Card>
    </div>
  );
}
