import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, setToken } from "../api.ts";
import { Button, Card, Field, Input, useToast } from "../ui/kit.tsx";

export function LiveAuth() {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [u, setU] = useState("");
  const [p, setP] = useState("");
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const nav = useNavigate();
  return (
    <main className="mx-auto flex max-w-sm flex-col px-4 py-10">
      <Card>
        <div className="mb-4 grid grid-cols-2 gap-2">
          {(["login", "register"] as const).map((m) => (
            <button key={m} onClick={() => setMode(m)} className={`min-h-11 rounded-xl font-semibold ${mode === m ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>{m === "login" ? "Log in" : "Sign up"}</button>
          ))}
        </div>
        <form className="space-y-3" onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            const r = await api(mode === "login" ? "/api/auth/login" : "/api/auth/register", { body: { username: u, password: p } });
            setToken(r.token);
            toast(mode === "login" ? "Welcome back!" : "Account created");
            nav(-1);
            setTimeout(() => location.reload(), 50);
          } catch (err: any) {
            toast(err.message, "error");
          } finally {
            setBusy(false);
          }
        }}>
          <Field label="Username"><Input value={u} onChange={(e) => setU(e.target.value)} autoCapitalize="none" autoComplete="username" /></Field>
          <Field label="Password" hint={mode === "register" ? "At least 8 characters." : undefined}><Input type="password" value={p} onChange={(e) => setP(e.target.value)} autoComplete={mode === "login" ? "current-password" : "new-password"} /></Field>
          <Button className="w-full" busy={busy}>{mode === "login" ? "Log in" : "Create MineLive account"}</Button>
        </form>
      </Card>
    </main>
  );
}
