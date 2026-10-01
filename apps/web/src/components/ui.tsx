import type { ReactNode } from "react";
import type { Status } from "../types";

export type Tone = "ok" | "warn" | "bad" | "info" | "mute" | "exec";

const TONES: Record<Tone, string> = {
  ok: "bg-emerald-500/15 text-emerald-300 ring-emerald-500/30",
  warn: "bg-amber-500/15 text-amber-300 ring-amber-500/30",
  bad: "bg-rose-500/15 text-rose-300 ring-rose-500/30",
  info: "bg-sky-500/15 text-sky-300 ring-sky-500/30",
  mute: "bg-slate-500/15 text-slate-300 ring-slate-500/30",
  exec: "bg-violet-500/15 text-violet-300 ring-violet-500/30",
};

export function Pill({ tone = "mute", children }: { tone?: Tone; children: ReactNode }) {
  return (
    <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium tracking-wide ring-1 ring-inset ${TONES[tone]}`}>
      {children}
    </span>
  );
}

export function Card({ title, right, children, className = "" }: { title?: ReactNode; right?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-xl border border-slate-800 bg-slate-900/60 ${className}`}>
      {(title || right) && (
        <header className="flex items-center justify-between gap-3 border-b border-slate-800 px-4 py-2.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-400">{title}</h3>
          {right}
        </header>
      )}
      <div className="p-4">{children}</div>
    </section>
  );
}

export function Bar({ label, value, tone = "info", right }: { label: string; value: number; tone?: Tone; right?: string }) {
  const color = { ok: "bg-emerald-400", warn: "bg-amber-400", bad: "bg-rose-400", info: "bg-sky-400", mute: "bg-slate-500", exec: "bg-violet-400" }[tone];
  return (
    <div className="grid grid-cols-[9.5rem_1fr_3.2rem] items-center gap-3 text-xs">
      <span className="truncate text-slate-300" title={label}>{label}</span>
      <div className="h-2 overflow-hidden rounded bg-slate-800">
        <div className={`h-full ${color}`} style={{ width: `${Math.max(0, Math.min(100, value * 100))}%` }} />
      </div>
      <span className="text-right font-mono text-slate-300">{right ?? pct(value)}</span>
    </div>
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

export function Mono({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <span className={`font-mono text-[12px] ${className}`}>{children}</span>;
}

export function Button({ children, onClick, tone = "default", disabled, type = "button" }: {
  children: ReactNode; onClick?: () => void; tone?: "default" | "primary" | "danger" | "ghost"; disabled?: boolean; type?: "button" | "submit";
}) {
  const t = {
    default: "bg-slate-800 hover:bg-slate-700 text-slate-100 border-slate-700",
    primary: "bg-emerald-500 hover:bg-emerald-400 text-slate-950 border-emerald-400 font-semibold",
    danger: "bg-rose-500/20 hover:bg-rose-500/30 text-rose-200 border-rose-500/40",
    ghost: "bg-transparent hover:bg-slate-800 text-slate-300 border-slate-700",
  }[tone];
  return (
    <button type={type} onClick={onClick} disabled={disabled}
      className={`rounded-lg border px-3 py-1.5 text-sm transition disabled:cursor-not-allowed disabled:opacity-40 ${t}`}>
      {children}
    </button>
  );
}

export function Spinner() {
  return <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-slate-600 border-t-emerald-400" />;
}

export function Json({ value }: { value: unknown }) {
  return (
    <pre className="max-h-72 overflow-auto rounded-lg bg-slate-950 p-3 font-mono text-[11px] leading-relaxed text-slate-300">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}
