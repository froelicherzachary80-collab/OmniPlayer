import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

type BtnProps = React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "ghost" | "danger" | "soft"; busy?: boolean; small?: boolean };

export function Button({ variant = "primary", busy, small, className = "", children, disabled, ...rest }: BtnProps) {
  const base = "inline-flex items-center justify-center gap-2 rounded-xl font-semibold transition active:scale-[.98] disabled:opacity-50 disabled:pointer-events-none";
  const size = small ? "min-h-9 px-3 text-sm" : "min-h-11 px-4";
  const v = {
    primary: "bg-brand text-white hover:brightness-110",
    soft: "bg-panel2 text-text border border-line hover:border-brand",
    ghost: "text-muted hover:text-text",
    danger: "bg-live/15 text-live border border-live/40 hover:bg-live/25",
  }[variant];
  return (
    <button className={`${base} ${size} ${v} ${className}`} disabled={disabled || busy} {...rest}>
      {busy && <span className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" />}
      {children}
    </button>
  );
}

export function Card({ children, className = "", title, action }: { children: ReactNode; className?: string; title?: ReactNode; action?: ReactNode }) {
  return (
    <section className={`rounded-2xl border border-line bg-panel p-4 ${className}`}>
      {(title || action) && (
        <div className="mb-3 flex items-center justify-between gap-2">
          {title && <h2 className="text-base font-bold">{title}</h2>}
          {action}
        </div>
      )}
      {children}
    </section>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-sm font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-muted">{hint}</span>}
    </label>
  );
}

export const inputCls = "w-full min-h-11 rounded-xl border border-line bg-panel2 px-3 text-text outline-none focus:border-brand";

export function Input(props: React.InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${inputCls} ${props.className ?? ""}`} />;
}

export function Select({ options, ...props }: React.SelectHTMLAttributes<HTMLSelectElement> & { options: [string, string][] }) {
  return (
    <select {...props} className={`${inputCls} ${props.className ?? ""}`}>
      {options.map(([v, l]) => (
        <option key={v} value={v}>
          {l}
        </option>
      ))}
    </select>
  );
}

export function TextArea(props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...props} className={`${inputCls} min-h-24 py-2 ${props.className ?? ""}`} />;
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button type="button" role="switch" aria-checked={checked} onClick={() => onChange(!checked)} className="flex min-h-11 w-full items-center justify-between gap-3 text-left">
      <span>{label}</span>
      <span className={`relative h-7 w-12 rounded-full transition ${checked ? "bg-brand" : "bg-line"}`}>
        <span className={`absolute top-1 size-5 rounded-full bg-white transition ${checked ? "left-6" : "left-1"}`} />
      </span>
    </button>
  );
}

export function Badge({ tone = "muted", children }: { tone?: "live" | "ok" | "warn" | "muted" | "brand"; children: ReactNode }) {
  const t = {
    live: "bg-live text-white",
    ok: "bg-brand2/15 text-brand2",
    warn: "bg-warn/15 text-warn",
    muted: "bg-panel2 text-muted",
    brand: "bg-brand/15 text-brand",
  }[tone];
  return <span className={`inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-bold ${t}`}>{children}</span>;
}

export function stateTone(s: string | undefined): "live" | "ok" | "warn" | "muted" {
  if (!s) return "muted";
  if (["online", "running", "live", "ready"].includes(s)) return "ok";
  if (["starting", "installing", "reconnecting", "stopping", "verifying", "pending", "in use"].includes(s)) return "warn";
  if (["error", "crashed", "unreachable"].includes(s)) return "live";
  return "muted";
}

export function Modal({ open, onClose, title, children }: { open: boolean; onClose: () => void; title: string; children: ReactNode }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/60 sm:items-center" onClick={onClose}>
      <div className="pb-safe max-h-[92dvh] w-full max-w-lg overflow-y-auto rounded-t-3xl border border-line bg-panel p-5 sm:rounded-3xl" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={title}>
        <div className="mb-4 flex items-center justify-between">
          <h3 className="text-lg font-bold">{title}</h3>
          <button className="min-h-11 min-w-11 text-2xl text-muted" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

export function Tabs<T extends string>({ tabs, value, onChange }: { tabs: [T, string][]; value: T; onChange: (t: T) => void }) {
  return (
    <div className="scroll-x -mx-4 mb-4 flex gap-2 px-4">
      {tabs.map(([k, l]) => (
        <button key={k} onClick={() => onChange(k)} className={`min-h-10 shrink-0 rounded-full px-4 text-sm font-semibold ${value === k ? "bg-brand text-white" : "bg-panel2 text-muted"}`}>
          {l}
        </button>
      ))}
    </div>
  );
}

export function Stat({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="rounded-2xl border border-line bg-panel p-4">
      <div className="text-xs font-semibold uppercase tracking-wide text-muted">{label}</div>
      <div className="mt-1 text-2xl font-extrabold">{value}</div>
      {sub && <div className="mt-1 text-xs text-muted">{sub}</div>}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="rounded-2xl border border-dashed border-line p-6 text-center text-muted">{children}</div>;
}

// ── toasts ──
const ToastCtx = createContext<(msg: string, tone?: "ok" | "error") => void>(() => {});
export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<{ id: number; msg: string; tone: string }[]>([]);
  const push = useCallback((msg: string, tone: "ok" | "error" = "ok") => {
    const id = Date.now() + Math.random();
    setItems((x) => [...x, { id, msg, tone }]);
    setTimeout(() => setItems((x) => x.filter((i) => i.id !== id)), tone === "error" ? 7000 : 3500);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 top-0 z-[60] flex flex-col items-center gap-2 p-3 pt-safe">
        {items.map((i) => (
          <div key={i.id} role="status" className={`pointer-events-auto max-w-md rounded-xl px-4 py-3 text-sm font-semibold shadow-lg ${i.tone === "error" ? "bg-live text-white" : "bg-brand2 text-black"}`}>
            {i.msg}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}
export const useToast = () => useContext(ToastCtx);

/** Loads data and re-polls on an interval. */
export function useData<T>(fn: () => Promise<T>, deps: unknown[] = [], pollMs = 0) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const reload = useCallback(async () => {
    try {
      setData(await fnRef.current());
      setError(null);
    } catch (e: any) {
      setError(e.message);
    }
  }, []);
  useEffect(() => {
    reload();
    if (!pollMs) return;
    const t = setInterval(reload, pollMs);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { data, error, reload, setData };
}

/** Wraps an async action with busy state + toast feedback. */
export function useAction() {
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const run = useCallback(
    async (key: string, fn: () => Promise<unknown>, ok?: string) => {
      setBusy(key);
      try {
        await fn();
        if (ok) toast(ok);
        return true;
      } catch (e: any) {
        toast(e.message, "error");
        return false;
      } finally {
        setBusy(null);
      }
    },
    [toast],
  );
  return { busy, run };
}

export function Avatar({ src, name, size = 40 }: { src?: string | null; name: string; size?: number }) {
  return src ? (
    <img src={src} alt="" width={size} height={size} className="pixel shrink-0 rounded-xl bg-panel2" style={{ width: size, height: size }} />
  ) : (
    <div className="flex shrink-0 items-center justify-center rounded-xl bg-brand font-bold text-white" style={{ width: size, height: size }}>
      {name.slice(0, 1)}
    </div>
  );
}
