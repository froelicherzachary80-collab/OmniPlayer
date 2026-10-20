import { useEffect, useState } from "react";
import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { api, getToken, setToken } from "../api.ts";
import { Button, Card, Field, Input, useToast } from "../ui/kit.tsx";

const NAV: [string, string, string][] = [
  ["/admin", "Overview", "▦"],
  ["/admin/play", "Play", "▶"],
  ["/admin/creators", "AI Players", "☺"],
  ["/admin/servers", "Servers", "⛏"],
  ["/admin/minelive", "MineLive", "◉"],
  ["/admin/accounts", "Accounts", "⚿"],
  ["/admin/smp", "SMP", "⚑"],
  ["/admin/voices", "VoiceBox", "♪"],
  ["/admin/system", "System", "⚙"],
];
// Five primary tabs on the phone tab bar; the rest live in "More".
const MOBILE = ["/admin", "/admin/play", "/admin/creators", "/admin/servers", "/admin/minelive"];

function Login({ onDone }: { onDone: () => void }) {
  const [u, setU] = useState("admin");
  const [p, setP] = useState("");
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  return (
    <div className="pt-safe flex min-h-dvh items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <div className="mb-4 flex items-center gap-3">
          <img src="/icon.svg" alt="" className="size-10" />
          <div>
            <h1 className="text-xl font-extrabold">OmniPlayer AI</h1>
            <p className="text-sm text-muted">Sign in to your control panel</p>
          </div>
        </div>
        <form
          className="space-y-3"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              const r = await api("/api/auth/login", { body: { username: u, password: p } });
              if (r.user.role !== "admin") throw new Error("This account is a MineLive viewer account, not an admin.");
              setToken(r.token);
              onDone();
            } catch (err: any) {
              toast(err.message, "error");
            } finally {
              setBusy(false);
            }
          }}
        >
          <Field label="Username">
            <Input value={u} onChange={(e) => setU(e.target.value)} autoCapitalize="none" autoComplete="username" />
          </Field>
          <Field label="Password" hint="First run: the password is printed in the server log and saved to data/initial-admin-password.txt.">
            <Input type="password" value={p} onChange={(e) => setP(e.target.value)} autoComplete="current-password" />
          </Field>
          <Button busy={busy} className="w-full" type="submit">
            Sign in
          </Button>
        </form>
      </Card>
    </div>
  );
}

export function AdminLayout() {
  const [ok, setOk] = useState<boolean | null>(null);
  const [more, setMore] = useState(false);
  const nav = useNavigate();
  const check = () =>
    getToken()
      ? api("/api/auth/me").then((u) => setOk(u.role === "admin")).catch(() => setOk(false))
      : setOk(false);
  useEffect(() => {
    check();
  }, []);
  if (ok === null) return <div className="p-8 text-muted">Loading…</div>;
  if (!ok) return <Login onDone={() => setOk(true)} />;
  const link = ({ isActive }: { isActive: boolean }) => `flex min-h-11 items-center gap-3 rounded-xl px-3 font-semibold ${isActive ? "bg-brand/15 text-brand" : "text-muted hover:text-text"}`;
  return (
    <div className="min-h-dvh md:flex">
      <aside className="sticky top-0 hidden h-dvh w-60 shrink-0 flex-col gap-1 border-r border-line bg-panel p-3 md:flex">
        <div className="mb-3 flex items-center gap-2 px-2 py-2">
          <img src="/icon.svg" alt="" className="size-8" />
          <span className="text-lg font-extrabold">OmniPlayer</span>
        </div>
        {NAV.map(([to, label, icon]) => (
          <NavLink key={to} to={to} end={to === "/admin"} className={link}>
            <span className="w-5 text-center">{icon}</span>
            {label}
          </NavLink>
        ))}
        <div className="mt-auto space-y-1">
          <NavLink to="/live" className={link}>
            <span className="w-5 text-center">↗</span>Open MineLive
          </NavLink>
          <button
            className="min-h-11 w-full rounded-xl px-3 text-left font-semibold text-muted hover:text-text"
            onClick={() => {
              api("/api/auth/logout", { method: "POST" }).catch(() => {});
              setToken(null);
              setOk(false);
            }}
          >
            Sign out
          </button>
        </div>
      </aside>
      <main className="pt-safe min-w-0 flex-1 px-4 pb-28 pt-4 md:pb-8">
        <div className="mx-auto max-w-5xl">
          <Outlet />
        </div>
      </main>
      <nav className="pb-safe fixed inset-x-0 bottom-0 z-40 grid grid-cols-6 border-t border-line bg-panel/95 backdrop-blur md:hidden" aria-label="Main">
        {NAV.filter(([to]) => MOBILE.includes(to)).map(([to, label, icon]) => (
          <NavLink key={to} to={to} end={to === "/admin"} className={({ isActive }) => `flex min-h-14 flex-col items-center justify-center text-[11px] font-semibold ${isActive ? "text-brand" : "text-muted"}`}>
            <span className="text-lg leading-none">{icon}</span>
            {label}
          </NavLink>
        ))}
        <button className="flex min-h-14 flex-col items-center justify-center text-[11px] font-semibold text-muted" onClick={() => setMore(true)}>
          <span className="text-lg leading-none">⋯</span>More
        </button>
      </nav>
      {more && (
        <div className="fixed inset-0 z-50 bg-black/60 md:hidden" onClick={() => setMore(false)}>
          <div className="pb-safe absolute inset-x-0 bottom-0 rounded-t-3xl border-t border-line bg-panel p-4" onClick={(e) => e.stopPropagation()}>
            <div className="grid grid-cols-3 gap-2">
              {[...NAV.filter(([to]) => !MOBILE.includes(to)), ["/live", "MineLive site", "↗"] as [string, string, string]].map(([to, label, icon]) => (
                <button key={to} className="flex min-h-20 flex-col items-center justify-center gap-1 rounded-2xl bg-panel2 text-sm font-semibold" onClick={() => { setMore(false); nav(to); }}>
                  <span className="text-xl">{icon}</span>
                  {label}
                </button>
              ))}
            </div>
            <Button variant="ghost" className="mt-3 w-full" onClick={() => { setToken(null); setOk(false); setMore(false); }}>
              Sign out
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

export function PageTitle({ title, sub, action }: { title: string; sub?: string; action?: React.ReactNode }) {
  return (
    <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-2xl font-extrabold">{title}</h1>
        {sub && <p className="text-sm text-muted">{sub}</p>}
      </div>
      {action}
    </div>
  );
}
