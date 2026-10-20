import { useEffect, useState } from "react";
import { Link, NavLink, Outlet, useNavigate } from "react-router-dom";
import { api, getToken, setToken } from "../api.ts";

export function LiveLayout() {
  const [me, setMe] = useState<any>(null);
  const [q, setQ] = useState("");
  const [notif, setNotif] = useState<any[]>([]);
  const nav = useNavigate();
  useEffect(() => {
    if (getToken()) api("/api/auth/me").then(setMe).catch(() => setMe(null));
  }, []);
  // Live notifications for followed channels (polling; works in Safari without push permissions).
  useEffect(() => {
    if (!me) return;
    const load = () => api<any[]>("/api/live/notifications").then(setNotif).catch(() => {});
    load();
    const t = setInterval(load, 30000);
    return () => clearInterval(t);
  }, [me]);
  return (
    <div className="min-h-dvh">
      <header className="pt-safe sticky top-0 z-40 border-b border-line bg-bg/90 backdrop-blur">
        <div className="mx-auto flex max-w-7xl items-center gap-3 px-4 py-2">
          <Link to="/live" className="flex items-center gap-2 font-extrabold">
            <span className="flex size-8 items-center justify-center rounded-lg bg-live text-white">◉</span>
            <span className="text-lg">Mine<span className="text-live">Live</span></span>
          </Link>
          <form className="hidden flex-1 sm:block" onSubmit={(e) => { e.preventDefault(); nav(`/live?q=${encodeURIComponent(q)}`); }}>
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search creators" className="min-h-10 w-full max-w-md rounded-xl border border-line bg-panel2 px-3 outline-none focus:border-brand" aria-label="Search" />
          </form>
          <div className="ml-auto flex items-center gap-2">
            <NavLink to="/live/multi" className="hidden min-h-10 items-center rounded-xl px-3 text-sm font-semibold text-muted hover:text-text sm:flex">Multiview</NavLink>
            {notif.length > 0 && <Link to={`/live/creator/${notif[0].slug}`} className="rounded-full bg-live px-2 py-1 text-xs font-bold text-white" title="Followed channels live">{notif.length} live</Link>}
            {me ? (
              <button className="min-h-10 rounded-xl bg-panel2 px-3 text-sm font-semibold" onClick={() => { setToken(null); setMe(null); }}>{me.username} · Log out</button>
            ) : (
              <Link to="/live/login" className="flex min-h-10 items-center rounded-xl bg-brand px-3 text-sm font-semibold text-white">Log in</Link>
            )}
          </div>
        </div>
      </header>
      <Outlet context={{ me }} />
      <footer className="pb-safe mx-auto max-w-7xl px-4 py-8 text-xs text-muted">
        MineLive is part of OmniPlayer. All MineLive creators are AI-generated fictional characters playing real Minecraft sessions. Not affiliated with Mojang, Microsoft or any streaming platform. Viewer counts are real connected viewers.
      </footer>
    </div>
  );
}
