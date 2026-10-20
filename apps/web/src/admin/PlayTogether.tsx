import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api.ts";
import { Avatar, Badge, Button, Card, Field, Input, Select, stateTone, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

/** The "play with your AI players" flow: server → creators → accounts → start → join. */
export function PlayTogether() {
  const servers = useData(() => api<any[]>("/api/servers"), [], 4000);
  const creators = useData(() => api<any[]>("/api/creators"), [], 4000);
  const accounts = useData(() => api<any[]>("/api/accounts"), [], 6000);
  const { busy, run } = useAction();
  const [serverId, setServerId] = useState<string>("");
  const [picked, setPicked] = useState<string[]>([]);
  const [me, setMe] = useState("");
  const [eula, setEula] = useState(false);
  const [newName, setNewName] = useState("My AI SMP");
  const s = servers.data?.find((x) => x.id === serverId) ?? servers.data?.[0];
  const sid = s?.id ?? "";

  const toggle = (id: string) => setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : p.length >= 16 ? p : [...p, id]));
  const freeAccounts = (c: any) => (accounts.data ?? []).filter((a) => !a.assigned_creator_id || a.assigned_creator_id === c.id);

  return (
    <>
      <PageTitle title="Play with your AI players" sub="Start a real server, connect AI creators, then join from your own Minecraft Java client." />
      <div className="space-y-4">
        <Card title="1 · Server">
          {servers.data?.length ? (
            <Field label="Server">
              <Select value={sid} onChange={(e) => setServerId(e.target.value)} options={servers.data.map((x) => [x.id, `${x.name} · ${x.software} ${x.version} · ${x.auth_mode} · ${x.state}`])} />
            </Field>
          ) : (
            <div className="flex gap-2">
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} aria-label="New server name" />
              <Button busy={busy === "create"} onClick={() => run("create", async () => { const n = await api("/api/servers", { body: { name: newName, software: "vanilla", version: "26.3", auth_mode: "offline" } }); setServerId(n.id); servers.reload(); }, "Server created")}>
                Create
              </Button>
            </div>
          )}
          {s && (
            <p className="mt-2 text-sm text-muted">
              {s.auth_mode === "offline" ? "Private offline-mode server: only allowlisted names can join, and Mojang does not verify identities." : "Online mode: every player, including bots, must use an authenticated Minecraft account."}{" "}
              <Link className="text-brand" to={`/admin/servers/${s.id}`}>Server dashboard →</Link>
            </p>
          )}
        </Card>

        <Card title={`2 · Choose AI creators (${picked.length} selected)`}>
          <ul className="divide-y divide-line">
            {(creators.data ?? []).map((c) => (
              <li key={c.id} className="flex flex-wrap items-center gap-3 py-2">
                <button className="flex min-h-12 flex-1 items-center gap-3 text-left" onClick={() => toggle(c.id)} aria-pressed={picked.includes(c.id)}>
                  <span className={`flex size-6 items-center justify-center rounded-md border ${picked.includes(c.id) ? "border-brand bg-brand text-white" : "border-line"}`}>{picked.includes(c.id) ? "✓" : ""}</span>
                  <Avatar src={c.avatar} name={c.streamer_name} size={36} />
                  <span className="font-semibold">{c.streamer_name}</span>
                  <Badge tone={stateTone(c.runtime?.state)}>{c.runtime?.state ?? "offline"}</Badge>
                </button>
                <div className="w-full sm:w-64">
                  <Select
                    aria-label={`Account for ${c.streamer_name}`}
                    value={c.account_id ?? ""}
                    onChange={(e) => run("assign", () => api(`/api/accounts/${e.target.value || c.account_id}/assign`, { body: { creator_id: e.target.value ? c.id : null } }).then(() => { creators.reload(); accounts.reload(); }))}
                    options={[["", "— choose account —"], ...freeAccounts(c).map((a: any) => [a.id, `${a.type === "microsoft" ? "Authenticated" : "Offline"} · ${a.mc_username}`] as [string, string])]}
                  />
                </div>
              </li>
            ))}
          </ul>
          {!creators.data?.length && <p className="text-muted">No creators yet — <Link className="text-brand" to="/admin/creators">create some</Link>.</p>}
          <p className="mt-2 text-xs text-muted">Need more accounts? <Link className="text-brand" to="/admin/accounts">Accounts</Link> (offline profiles or Microsoft sign-in).</p>
        </Card>

        <Card title="3 · Start the server">
          {s && s.software !== "external" && !s.eula_accepted_at && (
            <label className="mb-3 flex min-h-11 items-start gap-3 text-sm">
              <input type="checkbox" className="mt-1 size-5" checked={eula} onChange={(e) => setEula(e.target.checked)} />
              <span>
                I have read and accept the <a className="text-brand underline" href="https://aka.ms/MinecraftEULA" target="_blank" rel="noreferrer">Minecraft EULA</a>.
              </span>
            </label>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              disabled={!s || s.software === "external" || s.state === "running" || s.state === "starting"}
              busy={busy === "start"}
              onClick={() => run("start", async () => {
                if (!s.eula_accepted_at) {
                  if (!eula) throw new Error("Accept the Minecraft EULA first.");
                  await api(`/api/servers/${sid}/eula`, { body: { accept: true } });
                }
                await api(`/api/servers/${sid}/start`, { method: "POST" });
                servers.reload();
              }, "Server is starting — first start downloads the server jar and generates the world")}
            >
              Start Server
            </Button>
            {s && <Badge tone={stateTone(s.state)}>{s.state}</Badge>}
            {s?.last_error && <span className="text-sm text-live">{s.last_error}</span>}
          </div>
        </Card>

        <Card title="4 · Join AI players">
          <Field label="Your Minecraft username (so the AI can recognise you)">
            <Input value={me} onChange={(e) => setMe(e.target.value)} placeholder="e.g. Steve" autoCapitalize="none" />
          </Field>
          <Button
            className="mt-3"
            disabled={!picked.length || !s}
            busy={busy === "join"}
            onClick={() => run("join", async () => {
              if (me && s.auth_mode === "offline") await api(`/api/servers/${sid}/whitelist`, { body: { name: me } }).catch(() => {});
              const r = await api(`/api/servers/${sid}/join-ai`, { body: { creator_ids: picked, owner: me || undefined } });
              const failed = r.results.filter((x: any) => !x.ok);
              if (failed.length) throw new Error(failed.map((f: any) => `${f.creator}: ${f.error}`).join(" · "));
              creators.reload();
            }, "AI players are joining")}
          >
            Join AI Players
          </Button>
        </Card>

        <Card title="5 · Join from your Minecraft game">
          <p className="text-sm text-muted">Open Minecraft Java Edition {s?.version ?? "26.3"} → Multiplayer → Add Server, and use:</p>
          <div className="mt-2 flex items-center gap-2 rounded-xl bg-panel2 p-3 font-mono text-lg">
            <span className="flex-1 break-all">{s?.address ?? "—"}</span>
            {s && <Button small variant="soft" onClick={() => navigator.clipboard?.writeText(s.address)}>Copy</Button>}
          </div>
          <p className="mt-2 text-xs text-muted">On the same Wi-Fi use this computer's LAN IP. Over the internet, use a VPN such as Tailscale or forward the port (see docs/HOSTING.md). Try saying "follow me", "let's build a house" or "help me mine diamonds" in chat.</p>
        </Card>
      </div>
    </>
  );
}
