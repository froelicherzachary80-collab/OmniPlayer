import { useEffect, useRef, useState } from "react";

/**
 * Low-latency live player: WebRTC (WHEP) first, HLS fallback.
 * iPhone Safari plays HLS natively; other browsers use hls.js.
 */
export function LivePlayer({ whep, hls, title, compact = false }: { whep: string; hls: string; title?: string; compact?: boolean }) {
  const video = useRef<HTMLVideoElement>(null);
  const [mode, setMode] = useState<"connecting" | "webrtc" | "hls" | "error">("connecting");
  const [muted, setMuted] = useState(true);
  const [err, setErr] = useState("");

  useEffect(() => {
    const v = video.current!;
    let pc: RTCPeerConnection | null = null;
    let hlsInst: any = null;
    let cancelled = false;
    let session: string | null = null;

    const startHls = async () => {
      if (cancelled) return;
      pc?.close();
      setMode("hls");
      if (v.canPlayType("application/vnd.apple.mpegurl")) {
        v.src = hls;
        v.play().catch(() => {});
        return;
      }
      const { default: Hls } = await import("hls.js");
      if (!Hls.isSupported()) {
        setMode("error");
        setErr("This browser cannot play the stream.");
        return;
      }
      hlsInst = new Hls({ lowLatencyMode: true, liveSyncDurationCount: 2 });
      hlsInst.loadSource(hls);
      hlsInst.attachMedia(v);
      hlsInst.on(Hls.Events.MANIFEST_PARSED, () => v.play().catch(() => {}));
      hlsInst.on(Hls.Events.ERROR, (_: any, d: any) => {
        if (d.fatal) {
          setMode("error");
          setErr("Stream unavailable. It may have just ended.");
        }
      });
    };

    const startWebRtc = async () => {
      if (typeof RTCPeerConnection === "undefined") return startHls();
      try {
        pc = new RTCPeerConnection();
        pc.addTransceiver("video", { direction: "recvonly" });
        pc.addTransceiver("audio", { direction: "recvonly" });
        const ms = new MediaStream();
        pc.ontrack = (e) => {
          ms.addTrack(e.track);
          v.srcObject = ms;
          v.play().catch(() => {});
          setMode("webrtc");
        };
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        // Wait for ICE gathering (no trickle) so a single POST is enough.
        await new Promise<void>((res) => {
          if (pc!.iceGatheringState === "complete") return res();
          const t = setTimeout(res, 2000);
          pc!.addEventListener("icegatheringstatechange", () => {
            if (pc!.iceGatheringState === "complete") {
              clearTimeout(t);
              res();
            }
          });
        });
        const r = await fetch(whep, { method: "POST", headers: { "Content-Type": "application/sdp" }, body: pc.localDescription!.sdp });
        if (!r.ok) throw new Error(`WHEP ${r.status}`);
        session = r.headers.get("Location");
        await pc.setRemoteDescription({ type: "answer", sdp: await r.text() });
        setTimeout(() => {
          if (!cancelled && (!v.srcObject || pc?.connectionState === "failed")) {
            console.warn("WebRTC did not connect in time (state " + pc?.connectionState + "), falling back to HLS");
            startHls();
          }
        }, 6000);
        pc.onconnectionstatechange = () => {
          if (pc?.connectionState === "failed") startHls();
        };
      } catch (e) {
        console.warn("WebRTC playback failed, falling back to HLS:", e);
        startHls();
      }
    };
    startWebRtc();
    return () => {
      cancelled = true;
      pc?.close();
      hlsInst?.destroy();
      if (session) fetch(session, { method: "DELETE" }).catch(() => {});
    };
  }, [whep, hls]);

  return (
    <div className="relative aspect-video w-full overflow-hidden bg-black">
      <video ref={video} className="size-full" playsInline autoPlay muted={muted} controls={!compact} aria-label={title ?? "Live stream"} />
      <div className="pointer-events-none absolute left-2 top-2 flex gap-2">
        <span className="rounded bg-live px-2 py-0.5 text-xs font-extrabold text-white">LIVE</span>
        {!compact && <span className="rounded bg-black/60 px-2 py-0.5 text-xs text-white">{mode === "webrtc" ? "Low latency" : mode === "hls" ? "HLS" : mode}</span>}
      </div>
      {muted && (
        <button className="absolute bottom-3 right-3 min-h-11 rounded-xl bg-black/70 px-3 text-sm font-semibold text-white" onClick={() => setMuted(false)}>
          🔇 Tap to unmute
        </button>
      )}
      {mode === "error" && <div className="absolute inset-0 flex items-center justify-center p-4 text-center text-white">{err}</div>}
    </div>
  );
}
