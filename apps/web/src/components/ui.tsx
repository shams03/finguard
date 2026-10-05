import { createContext, useCallback, useContext, useEffect, useId, useState } from "react";
import type { ReactNode } from "react";
import { GLOSSARY } from "../lib/glossary";
import type { Step, StepState } from "../lib/stages";
import type { Status } from "../types";

export type Tone = "ok" | "warn" | "bad" | "info" | "mute" | "exec";

export const TONE_TEXT: Record<Tone, string> = { ok: "text-emerald-300", warn: "text-amber-300", bad: "text-rose-300", info: "text-sky-300", mute: "text-slate-300", exec: "text-violet-300" };
const TONE_PILL: Record<Tone, string> = {
  ok: "bg-emerald-500/15 text-emerald-300 ring-emerald-500/30", warn: "bg-amber-500/15 text-amber-300 ring-amber-500/30",
  bad: "bg-rose-500/15 text-rose-300 ring-rose-500/30", info: "bg-sky-500/15 text-sky-300 ring-sky-500/30",
  mute: "bg-slate-500/15 text-slate-300 ring-slate-500/30", exec: "bg-violet-500/15 text-violet-300 ring-violet-500/30",
};
export const TONE_PANEL: Record<Tone, string> = {
  ok: "border-emerald-500/30 bg-emerald-500/[0.07]", warn: "border-amber-500/30 bg-amber-500/[0.07]", bad: "border-rose-500/30 bg-rose-500/[0.07]",
  info: "border-sky-500/30 bg-sky-500/[0.07]", mute: "border-slate-700 bg-slate-800/40", exec: "border-violet-500/30 bg-violet-500/[0.07]",
};
const TONE_BAR: Record<Tone, string> = { ok: "bg-emerald-400", warn: "bg-amber-400", bad: "bg-rose-400", info: "bg-sky-400", mute: "bg-slate-500", exec: "bg-violet-400" };

// ---------------------------------------------------------------- icons ----
const PATHS: Record<string, ReactNode> = {
  check: <path d="M5 12.5l4.2 4.2L19 7" />,
  x: <path d="M6 6l12 12M18 6L6 18" />,
  hand: <><path d="M8 11V6.5a1.5 1.5 0 013 0V11" /><path d="M11 10.5V5a1.5 1.5 0 013 0v5.5" /><path d="M14 10.5V7a1.5 1.5 0 013 0v7a6 6 0 01-6 6h-.5a5 5 0 01-4-2L4.5 14a1.5 1.5 0 012.3-1.9L8 13.5V11" /></>,
  question: <><circle cx="12" cy="12" r="9" /><path d="M9.5 9.5a2.5 2.5 0 114 2c-.9.6-1.5 1.1-1.5 2.2M12 17h.01" /></>,
  pause: <><rect x="7" y="5" width="3.5" height="14" rx="1" /><rect x="13.5" y="5" width="3.5" height="14" rx="1" /></>,
  stop: <><circle cx="12" cy="12" r="9" /><path d="M6 6l12 12" /></>,
  clock: <><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>,
  arrow: <path d="M5 12h14M13 6l6 6-6 6" />,
  info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></>,
  play: <path d="M8 5l11 7-11 7z" />,
  chevron: <path d="M7 10l5 5 5-5" />,
  shield: <path d="M12 3l8 3v6c0 4.5-3.2 8-8 9-4.8-1-8-4.5-8-9V6z" />,
  bolt: <path d="M13 3L5 14h6l-1 7 8-11h-6z" />,
  user: <><circle cx="12" cy="8" r="3.5" /><path d="M5 20c.8-3.6 3.6-5.5 7-5.5s6.2 1.9 7 5.5" /></>,
  brain: <><path d="M9 4a3 3 0 00-3 3v.5A3.5 3.5 0 004 11a3.5 3.5 0 002 3.2V15a3 3 0 003 3h.5V4z" /><path d="M15 4a3 3 0 013 3v.5a3.5 3.5 0 012 3.5 3.5 3.5 0 01-2 3.2V15a3 3 0 01-3 3h-.5V4z" /></>,
  db: <><ellipse cx="12" cy="6" rx="7" ry="3" /><path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" /></>,
  list: <path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01" />,
  lock: <><rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 018 0v3" /></>,
  refresh: <path d="M20 11a8 8 0 00-14.5-3M4 5v4h4M4 13a8 8 0 0014.5 3M20 19v-4h-4" />,
  gear: <><circle cx="12" cy="12" r="3" /><path d="M12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M18.4 5.6l-1.8 1.8M7.4 16.6l-1.8 1.8" /></>,
  film: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M10 9.5v5l4.5-2.5z" /></>,
  flow: <><rect x="3" y="4" width="6" height="5" rx="1" /><rect x="15" y="4" width="6" height="5" rx="1" /><rect x="9" y="15" width="6" height="5" rx="1" /><path d="M6 9v3h12V9M12 12v3" /></>,
};

export function Icon({ name, className = "h-4 w-4", stroke = 2 }: { name: keyof typeof PATHS | string; className?: string; stroke?: number }) {
  const fillIcons = ["play", "pause"];
  return (
    <svg viewBox="0 0 24 24" className={className} fill={fillIcons.includes(name) ? "currentColor" : "none"} stroke="currentColor" strokeWidth={stroke} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[name]}
    </svg>
  );
}

// ---------------------------------------------------------- primitives ----
export function Pill({ tone = "mute", children, icon }: { tone?: Tone; children: ReactNode; icon?: string }) {
  return (
    <span className={`inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium ring-1 ring-inset ${TONE_PILL[tone]}`}>
      {icon && <Icon name={icon} className="h-3 w-3" />}{children}
    </span>
  );
}

export function Card({ title, right, children, className = "", id }: { title?: ReactNode; right?: ReactNode; children: ReactNode; className?: string; id?: string }) {
  return (
    <section id={id} className={`rounded-2xl border border-line bg-panel/80 ${className}`}>
      {(title || right) && (
        <header className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-5 py-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-slate-300">{title}</h3>
          {right}
        </header>
      )}
      <div className="p-5">{children}</div>
    </section>
  );
}

export const pct = (v: number | null | undefined) => (v == null ? "—" : `${Math.round(v * 100)}%`);
export const fmtMs = (v: number | null | undefined) => (v == null ? "—" : v >= 1000 ? `${(v / 1000).toFixed(2)}s` : `${Math.round(v)} ms`);

export function statusTone(s: Status | string): Tone {
  switch (s) {
    case "EXECUTED": return "exec";
    case "BLOCKED": case "FAILED": return "bad";
    case "MANUAL_REVIEW": case "AWAITING_CLARIFICATION": case "EXPIRED": return "warn";
    case "AWAITING_CONFIRMATION": return "info";
    case "CANCELLED": return "mute";
    default: return "ok";
  }
}
export const STATUS_LABEL: Record<string, string> = {
  EXECUTED: "Done", BLOCKED: "Blocked", FAILED: "Failed", MANUAL_REVIEW: "Held for review", AWAITING_CLARIFICATION: "Needs a detail",
  AWAITING_CONFIRMATION: "Needs your approval", CANCELLED: "Cancelled", EXPIRED: "Expired",
};
export const statusLabel = (s: string) => STATUS_LABEL[s] ?? s.replaceAll("_", " ").toLowerCase();

export function Mono({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <span className={`font-mono text-[13px] ${className}`}>{children}</span>;
}

export function Button({ children, onClick, tone = "default", disabled, type = "button", icon, size = "md", title }: {
  children: ReactNode; onClick?: () => void; tone?: "default" | "primary" | "danger" | "ghost"; disabled?: boolean; type?: "button" | "submit"; icon?: string; size?: "sm" | "md" | "lg"; title?: string;
}) {
  const t = {
    default: "bg-slate-800 hover:bg-slate-700 text-slate-100 border-slate-700",
    primary: "bg-emerald-400 hover:bg-emerald-300 text-slate-950 border-emerald-300 font-semibold shadow-[0_0_0_1px_rgba(16,185,129,0.3),0_8px_24px_-8px_rgba(16,185,129,0.5)]",
    danger: "bg-rose-500/15 hover:bg-rose-500/25 text-rose-200 border-rose-500/40",
    ghost: "bg-transparent hover:bg-slate-800 text-slate-200 border-slate-700",
  }[tone];
  const s = { sm: "px-2.5 py-1 text-xs", md: "px-3.5 py-2 text-sm", lg: "px-5 py-3 text-base" }[size];
  return (
    <button type={type} onClick={onClick} disabled={disabled} title={title}
      className={`inline-flex items-center justify-center gap-2 rounded-xl border transition active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40 ${t} ${s}`}>
      {icon && <Icon name={icon} className="h-4 w-4" />}{children}
    </button>
  );
}

export function Spinner({ className = "h-4 w-4" }: { className?: string }) {
  return <span className={`inline-block animate-spin rounded-full border-2 border-slate-600 border-t-emerald-400 ${className}`} role="status" aria-label="Loading" />;
}

export function Json({ value }: { value: unknown }) {
  return <pre className="max-h-72 overflow-auto rounded-lg bg-slate-950 p-3 font-mono text-xs leading-relaxed text-slate-300">{JSON.stringify(value, null, 2)}</pre>;
}

// ------------------------------------------------------ glossary tooltip ----
/** Dotted-underline term with an accessible explanation. */
export function Term({ k, children }: { k: keyof typeof GLOSSARY; children?: ReactNode }) {
  const g = GLOSSARY[k];
  const id = useId();
  return (
    <span className="group relative">
      <span tabIndex={0} aria-describedby={id} className="cursor-help border-b border-dotted border-slate-400 text-inherit">{children ?? g.label}</span>
      <span id={id} role="tooltip" className="pointer-events-none invisible absolute bottom-full left-1/2 z-30 mb-2 w-64 -translate-x-1/2 rounded-lg border border-slate-600 bg-slate-900 p-3 text-xs leading-relaxed text-slate-200 opacity-0 shadow-xl transition group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100">
        <b className="mb-0.5 block text-slate-50">{g.label}</b>{g.text}
      </span>
    </span>
  );
}

// -------------------------------------------------------- collapsible ------
export function Collapsible({ title, summary, defaultOpen = false, children, tone, open: controlled, onOpenChange, anchor }: {
  title: ReactNode; summary?: ReactNode; defaultOpen?: boolean; children: ReactNode; tone?: Tone; open?: boolean; onOpenChange?: (o: boolean) => void; anchor?: string;
}) {
  const [inner, setInner] = useState(defaultOpen);
  const open = controlled ?? inner;
  const setOpen = (f: (o: boolean) => boolean) => { const n = f(open); setInner(n); onOpenChange?.(n); };
  const id = useId();
  return (
    <div id={anchor} className="scroll-mt-24 rounded-xl border border-line bg-slate-950/40">
      <button type="button" aria-expanded={open} aria-controls={id} onClick={() => setOpen((o) => !o)} className="flex w-full items-center gap-3 px-4 py-3 text-left">
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-semibold text-slate-100">{title}</span>
          {summary && <span className={`block truncate text-xs ${tone ? TONE_TEXT[tone] : "text-slate-400"}`}>{summary}</span>}
        </span>
        <Icon name="chevron" className={`h-4 w-4 shrink-0 text-slate-400 transition ${open ? "rotate-180" : ""}`} />
      </button>
      {open && <div id={id} className="border-t border-line px-4 py-4">{children}</div>}
    </div>
  );
}

// ---------------------------------------------------------- confidence ----
/** Confidence bar with the required threshold marked. Categorical colour, not just numbers. */
export function Gauge({ value, threshold }: { value: number; threshold: number }) {
  const ok = value >= threshold;
  return (
    <div>
      <div className="relative h-3 rounded-full bg-slate-800" role="img" aria-label={`Confidence ${pct(value)}, required ${pct(threshold)}`}>
        <div className={`h-full rounded-full transition-all duration-700 ${ok ? "bg-emerald-400" : "bg-amber-400"}`} style={{ width: `${Math.min(100, value * 100)}%` }} />
        <div className="absolute -top-1 h-5 w-0.5 bg-slate-100" style={{ left: `${threshold * 100}%` }} title={`Required: ${pct(threshold)}`} />
      </div>
      <p className="mt-1.5 text-xs text-slate-300">
        Confidence <b className={ok ? "text-emerald-300" : "text-amber-300"}>{pct(value)}</b> · required <b>{pct(threshold)}</b> {ok ? "✓ enough to continue" : "→ not enough, so it asks instead of trusting the model"}
      </p>
    </div>
  );
}

export function Bar({ label, value, tone = "info", right }: { label: string; value: number; tone?: Tone; right?: string }) {
  return (
    <div className="grid grid-cols-[minmax(7rem,11rem)_1fr_3rem] items-center gap-3 text-xs">
      <span className="truncate text-slate-300" title={label}>{label}</span>
      <div className="h-2 overflow-hidden rounded bg-slate-800"><div className={`h-full transition-all duration-700 ${TONE_BAR[tone]}`} style={{ width: `${Math.max(0, Math.min(100, value * 100))}%` }} /></div>
      <span className="text-right font-mono text-slate-300">{right ?? pct(value)}</span>
    </div>
  );
}

// ------------------------------------------------------------ stepper ------
const STEP_STYLE: Record<StepState, { ring: string; icon: string; text: string }> = {
  done: { ring: "bg-emerald-500/20 text-emerald-300 ring-emerald-500/50", icon: "check", text: "text-slate-100" },
  warn: { ring: "bg-amber-500/20 text-amber-300 ring-amber-500/50", icon: "pause", text: "text-slate-100" },
  stop: { ring: "bg-rose-500/20 text-rose-300 ring-rose-500/50", icon: "x", text: "text-slate-100" },
  now: { ring: "bg-sky-500/20 text-sky-300 ring-sky-400/70 animate-ring", icon: "hand", text: "text-slate-50" },
  wait: { ring: "bg-slate-800 text-slate-500 ring-slate-700", icon: "clock", text: "text-slate-400" },
  skip: { ring: "bg-slate-800/60 text-slate-500 ring-slate-700", icon: "arrow", text: "text-slate-500" },
  todo: { ring: "bg-slate-800/60 text-slate-500 ring-slate-700", icon: "clock", text: "text-slate-500" },
};

/** The six-step journey of a request. `running` animates the active step while waiting on the server. */
export function Stepper({ steps, running, onPick, active }: { steps: Step[]; running?: number; onPick?: (key: string) => void; active?: string }) {
  return (
    <ol className="grid grid-cols-2 gap-3 sm:grid-cols-3" aria-label="Request progress">
      {steps.map((s, i) => {
        const isRunning = running === i;
        const st = isRunning ? STEP_STYLE.now : STEP_STYLE[s.state];
        const future = running !== undefined && i > running;
        return (
          <li key={s.key} className="reveal" style={{ ["--i" as string]: i }}>
            <button type="button" disabled={!onPick} onClick={() => onPick?.(s.key)} aria-current={active === s.key}
              className={`flex w-full flex-col items-start gap-2 rounded-xl border p-3 text-left transition ${active === s.key ? "border-slate-500 bg-slate-800/70" : "border-line bg-slate-950/40 hover:border-slate-600"} ${future ? "opacity-40" : ""}`}>
              <span className={`flex h-7 w-7 items-center justify-center rounded-full ring-1 ring-inset ${st.ring}`}>
                {isRunning ? <Spinner className="h-3.5 w-3.5" /> : <Icon name={st.icon} className="h-3.5 w-3.5" />}
              </span>
              <span className={`text-sm font-semibold leading-tight ${st.text}`}>{s.label}</span>
              <span className="text-xs leading-snug text-slate-400">{isRunning ? "Working…" : future ? "Waiting" : s.note}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

// -------------------------------------------------------------- toasts ----
interface Toast { id: number; tone: Tone; text: string }
const ToastCtx = createContext<(tone: Tone, text: string) => void>(() => undefined);
export const useToast = () => useContext(ToastCtx);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([]);
  const push = useCallback((tone: Tone, text: string) => {
    const id = Date.now() + Math.random();
    setItems((s) => [...s.slice(-3), { id, tone, text }]);
    window.setTimeout(() => setItems((s) => s.filter((t) => t.id !== id)), 4500);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-2" aria-live="polite" aria-atomic="false">
        {items.map((t) => (
          <div key={t.id} role="status" className={`pointer-events-auto animate-fade-up rounded-xl border px-4 py-3 text-sm shadow-xl backdrop-blur ${TONE_PANEL[t.tone]} ${TONE_TEXT[t.tone]} bg-slate-900/90`}>{t.text}</div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

export function useLocalStorage<T>(key: string, initial: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => { try { const s = localStorage.getItem(key); return s ? (JSON.parse(s) as T) : initial; } catch { return initial; } });
  useEffect(() => { try { localStorage.setItem(key, JSON.stringify(v)); } catch { /* storage unavailable */ } }, [key, v]);
  return [v, setV];
}
