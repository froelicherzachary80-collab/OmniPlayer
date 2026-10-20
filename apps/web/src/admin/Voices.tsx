import { useState } from "react";
import { api } from "../api.ts";
import { Badge, Button, Card, Field, Input, Select, TextArea, useAction, useData } from "../ui/kit.tsx";
import { PageTitle } from "./Layout.tsx";

export function Voices() {
  const voices = useData(() => api<any[]>("/api/voices"), []);
  const health = useData(() => api("/api/voices/health"), [], 15000);
  const { busy, run } = useAction();
  const [f, setF] = useState<any>({ name: "", engine: "espeak", preset: "en-us+m2", rate: 1, pitch: 0, volume: 1, endpoint: "", rvc_endpoint: "", consent_owner: "", consent_confirm: false });
  const [file, setFile] = useState<File | null>(null);
  const [audio, setAudio] = useState<string | null>(null);
  const test = (voice_id: string) =>
    run("t" + voice_id, async () => {
      const r = await fetch("/api/voices/test", { method: "POST", headers: { "Content-Type": "application/json", Authorization: `Bearer ${localStorage.getItem("omniplayer.token")}` }, body: JSON.stringify({ voice_id, text: "Oh hey! You finally got on. Come check out what I've been building!" }) });
      if (!r.ok) throw new Error((await r.json()).error);
      setAudio(URL.createObjectURL(await r.blob()));
    });
  return (
    <>
      <PageTitle title="VoiceBox AI" sub="Local, pluggable text-to-speech for your creators. No paid API needed." />
      <Card className="mb-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          VoiceBox service: <Badge tone={health.data?.ok ? "ok" : "live"}>{health.data?.ok ? "online" : "offline"}</Badge>
          {health.data?.engines && Object.entries(health.data.engines).map(([k, v]) => <Badge key={k} tone={v ? "ok" : "muted"}>{k}{v ? "" : " (not installed)"}</Badge>)}
          {health.data?.error && <span className="text-live">{health.data.error}</span>}
        </div>
      </Card>
      {audio && <audio src={audio} controls autoPlay className="mb-4 w-full" />}
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Voices">
          <ul className="divide-y divide-line">
            {(voices.data ?? []).map((v) => (
              <li key={v.id} className="flex flex-wrap items-center gap-2 py-3">
                <div className="min-w-0 flex-1">
                  <div className="font-semibold">{v.name}</div>
                  <div className="text-xs text-muted">{v.engine} · {v.preset} {v.consent ? `· consent: ${v.consent.owner}` : ""}</div>
                </div>
                <Badge tone={v.original ? "brand" : "warn"}>{v.original ? "original" : "uploaded"}</Badge>
                <Button small variant="soft" busy={busy === "t" + v.id} onClick={() => test(v.id)}>▶</Button>
                {!v.original && <Button small variant="danger" onClick={() => run("d", () => api(`/api/voices/${v.id}`, { method: "DELETE" }).then(voices.reload))}>×</Button>}
              </li>
            ))}
          </ul>
        </Card>
        <Card title="Add a voice">
          <div className="space-y-3">
            <Field label="Name"><Input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></Field>
            <Field label="Engine" hint="Neural engines run on your own hardware (Kokoro/Chatterbox via an OpenAI-compatible server, or Piper).">
              <Select value={f.engine} onChange={(e) => setF({ ...f, engine: e.target.value })} options={[["espeak", "eSpeak NG (built-in)"], ["piper", "Piper (local neural)"], ["http", "Local speech server (Kokoro/Chatterbox, OpenAI-compatible)"]]} />
            </Field>
            <Field label={f.engine === "http" ? "Voice name on that server" : "Voice preset"}><Input value={f.preset} onChange={(e) => setF({ ...f, preset: e.target.value })} /></Field>
            {f.engine === "http" && <Field label="Server URL"><Input value={f.endpoint} onChange={(e) => setF({ ...f, endpoint: e.target.value })} placeholder="http://127.0.0.1:8880" /></Field>}
            <Field label="Voice model file (Piper .onnx or RVC .pth) — optional"><input type="file" className="block min-h-11 w-full text-sm" onChange={(e) => setFile(e.target.files?.[0] ?? null)} /></Field>
            {f.engine !== "http" && file && <Field label="RVC conversion server (optional)"><Input value={f.rvc_endpoint} onChange={(e) => setF({ ...f, rvc_endpoint: e.target.value })} placeholder="http://127.0.0.1:7865/convert" /></Field>}
            {file && (
              <div className="space-y-2 rounded-xl border border-warn/40 p-3">
                <Field label="Whose voice is this?"><Input value={f.consent_owner} onChange={(e) => setF({ ...f, consent_owner: e.target.value })} placeholder="Your name or the consenting speaker" /></Field>
                <label className="flex items-start gap-2 text-sm"><input type="checkbox" className="mt-1 size-5" checked={f.consent_confirm} onChange={(e) => setF({ ...f, consent_confirm: e.target.checked })} />I own this voice or have the speaker's explicit permission. It does not imitate a real streamer or celebrity without consent.</label>
              </div>
            )}
            <Button className="w-full" busy={busy === "add"} onClick={() => run("add", async () => {
              const fd = new FormData();
              Object.entries(f).forEach(([k, v]) => fd.append(k, String(v)));
              if (file) fd.append("model", file);
              await api("/api/voices", { method: "POST", raw: fd });
              voices.reload();
            }, "Voice added")}>Add voice</Button>
          </div>
        </Card>
      </div>
    </>
  );
}
