import { useEffect, useState } from "react";
import { api } from "../api.ts";
import { Badge, Button, Card, Empty, Field, Input, Modal, Select, stateTone, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function Accounts() {
  const accounts = useData(() => api<any[]>("/api/accounts"), [], 5000);
  const creators = useData(() => api<any[]>("/api/creators"), []);
  const reserved = useData(() => api<any[]>("/api/reserved-names"), []);
  const { busy, run } = useAction();
  const [offline, setOffline] = useState("");
  const [msOpen, setMsOpen] = useState(false);
  const [rname, setRname] = useState("");
  return (
    <>
      <PageTitle title="Minecraft accounts" sub="Assign one account to one AI player at a time." />
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Authenticated Minecraft (Microsoft)">
          <p className="mb-3 text-sm text-muted">Sign in on Microsoft's own page — OmniPlayer never asks for your Microsoft password. The account must own Minecraft: Java Edition. Tokens are stored encrypted and refreshed automatically.</p>
          <Button onClick={() => setMsOpen(true)}>Connect Microsoft account</Button>
        </Card>
        <Card title="Offline-mode AI player">
          <p className="mb-3 text-sm text-muted">For private servers that allow offline mode. The UUID is derived exactly like vanilla (OfflinePlayer:name). Offline names are not verified identities — never use someone else's name.</p>
          <div className="flex gap-2">
            <Input value={offline} onChange={(e) => setOffline(e.target.value)} placeholder="Username (3–16 chars)" autoCapitalize="none" />
            <Button busy={busy === "off"} onClick={() => run("off", () => api("/api/accounts/offline", { body: { username: offline } }).then(() => { setOffline(""); accounts.reload(); }), "Offline profile created")}>Add</Button>
          </div>
        </Card>
      </div>
      <Card title="All accounts" className="mt-4">
        {!accounts.data?.length ? <Empty>No accounts yet.</Empty> : (
          <ul className="divide-y divide-line">
            {accounts.data.map((a) => (
              <li key={a.id} className="flex flex-wrap items-center gap-3 py-3">
                {a.skin_url ? <img src={a.skin_url} alt="" className="pixel size-10 rounded-lg object-cover object-[-8px_-8px]" /> : <div className="flex size-10 items-center justify-center rounded-lg bg-panel2 font-bold">{a.mc_username[0]}</div>}
                <div className="min-w-0 flex-1">
                  <div className="font-semibold">{a.mc_username}</div>
                  <div className="truncate font-mono text-xs text-muted">{a.mc_uuid}</div>
                </div>
                <Badge tone={a.type === "microsoft" ? "brand" : "muted"}>{a.type === "microsoft" ? "Authenticated" : "Offline"}</Badge>
                {a.type === "microsoft" && <Badge tone={a.entitlement_ok ? "ok" : "live"}>{a.entitlement_ok ? "Owns Java Edition" : "No entitlement"}</Badge>}
                <Badge tone={stateTone(a.status)}>{a.status}</Badge>
                <div className="w-full sm:w-56">
                  <Select aria-label="Assign to creator" value={a.assigned_creator_id ?? ""} onChange={(e) => run("as", () => api(`/api/accounts/${a.id}/assign`, { body: { creator_id: e.target.value || null } }).then(accounts.reload), "Assignment updated")} options={[["", "— unassigned —"], ...(creators.data ?? []).map((c) => [c.id, c.streamer_name] as [string, string])]} />
                </div>
                {a.type === "microsoft" && <Button small variant="soft" onClick={() => run("rf", () => api(`/api/accounts/${a.id}/refresh`, { method: "POST" }).then(accounts.reload), "Refreshed")}>Refresh</Button>}
                <Button small variant="danger" onClick={() => confirm(`Remove ${a.mc_username}?`) && run("del", () => api(`/api/accounts/${a.id}`, { method: "DELETE" }).then(accounts.reload))}>Remove</Button>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <Card title="Reserved usernames" className="mt-4">
        <p className="mb-2 text-sm text-muted">Protects names (yours, your friends', your bots') from being claimed on private offline servers.</p>
        <div className="mb-2 flex gap-2">
          <Input value={rname} onChange={(e) => setRname(e.target.value)} placeholder="Name to reserve" />
          <Button onClick={() => run("rs", () => api("/api/reserved-names", { body: { name: rname } }).then(() => { setRname(""); reserved.reload(); }))}>Reserve</Button>
        </div>
        <div className="flex flex-wrap gap-2">
          {(reserved.data ?? []).map((r) => <button key={r.name} className="rounded-lg bg-panel2 px-3 py-2 text-sm" onClick={() => run("ur", () => api(`/api/reserved-names/${r.name}`, { method: "DELETE" }).then(reserved.reload))}>{r.name} ×</button>)}
        </div>
      </Card>
      <MicrosoftModal open={msOpen} onClose={() => { setMsOpen(false); accounts.reload(); }} />
    </>
  );
}

function MicrosoftModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [flow, setFlow] = useState<any>(null);
  const [status, setStatus] = useState<any>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    setFlow(null);
    setStatus(null);
    setErr(null);
    api("/api/accounts/microsoft/start", { method: "POST" }).then(setFlow).catch((e) => setErr(e.message));
  }, [open]);
  useEffect(() => {
    if (!flow) return;
    const t = setInterval(async () => {
      const s = await api(`/api/accounts/microsoft/flow/${flow.flow_id}`).catch(() => null);
      if (s) setStatus(s);
      if (s && ["done", "error", "expired"].includes(s.status)) clearInterval(t);
    }, 3000);
    return () => clearInterval(t);
  }, [flow]);
  return (
    <Modal open={open} onClose={onClose} title="Connect a Microsoft account">
      {err && <p className="text-live">{err}</p>}
      {flow && (
        <div className="space-y-3 text-center">
          <p className="text-sm text-muted">1. Open Microsoft's sign-in page:</p>
          <a className="block rounded-xl bg-panel2 p-3 font-semibold text-brand underline" href={flow.verification_uri} target="_blank" rel="noreferrer">{flow.verification_uri}</a>
          <p className="text-sm text-muted">2. Enter this code:</p>
          <div className="select-all rounded-xl bg-panel2 p-4 font-mono text-3xl font-extrabold tracking-widest">{flow.user_code}</div>
          <Button small variant="soft" onClick={() => navigator.clipboard?.writeText(flow.user_code)}>Copy code</Button>
          <p className="text-sm">
            Status: <Badge tone={stateTone(status?.status ?? "pending")}>{status?.status ?? "waiting for sign-in"}</Badge>
          </p>
          {status?.error && <p className="text-sm text-live">{status.error}</p>}
          {status?.account && <p className="text-brand2">Connected {status.account.mc_username} ✓</p>}
        </div>
      )}
      {!flow && !err && <p className="text-muted">Requesting a sign-in code…</p>}
    </Modal>
  );
}
