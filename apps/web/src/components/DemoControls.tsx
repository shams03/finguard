import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Button, Pill, useToast } from "./ui";

/** Developer/demo tools, tucked away so they don't compete with the product. */
export function DemoControls({ providers, onChanged }: { providers: Record<string, string | null>; onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const toast = useToast();
  const mockRisk = String(providers.decision_provider ?? "").startsWith("mock");
  const mockLlm = String(providers.escalation_analyzer ?? "").startsWith("mock");

  useEffect(() => {
    const on = (e: MouseEvent) => { if (open && ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", on); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", on); document.removeEventListener("keydown", esc); };
  }, [open]);

  const run = async (fn: () => Promise<unknown>, msg: string) => {
    setBusy(true);
    try { await fn(); toast("info", msg); onChanged(); } catch (e) { toast("bad", (e as Error).message); } finally { setBusy(false); }
  };

  return (
    <div className="relative" ref={ref}>
      <Button tone="ghost" size="sm" icon="gear" onClick={() => setOpen((o) => !o)}>Demo tools</Button>
      {open && (
        <div role="dialog" aria-label="Demo tools" className="absolute right-0 z-40 mt-2 w-80 animate-fade-up rounded-2xl border border-slate-600 bg-slate-900 p-4 shadow-2xl">
          <h4 className="text-sm font-semibold text-slate-100">Demo tools</h4>
          <p className="mb-3 text-xs text-slate-400">For rehearsing and testing. Not part of the product.</p>
          <div className="space-y-3">
            <div>
              <p className="mb-1 text-xs font-medium text-slate-300">Simulate a risk-provider outage {!mockRisk && <Pill tone="warn">live provider</Pill>}</p>
              <div className="flex flex-wrap gap-1.5">
                {["none", "timeout", "429", "5xx"].map((m) => <Button key={m} size="sm" tone="ghost" disabled={busy || !mockRisk} onClick={() => run(() => api.failure("jev", m), m === "none" ? "Risk provider healthy." : `Risk provider now fails with ${m}. Transfers will be held for review.`)}>{m === "none" ? "healthy" : m}</Button>)}
              </div>
              {!mockRisk && <p className="mt-1 text-xs text-slate-500">Available only with DECISION_PROVIDER=mock.</p>}
            </div>
            <div>
              <p className="mb-1 text-xs font-medium text-slate-300">Simulate a language-model outage {!mockLlm && <Pill tone="warn">live provider</Pill>}</p>
              <div className="flex gap-1.5">
                <Button size="sm" tone="ghost" disabled={busy || !mockLlm} onClick={() => run(() => api.failure("groq", "timeout"), "Language model now times out. Clarifications fall back to a fixed question.")}>timeout</Button>
                <Button size="sm" tone="ghost" disabled={busy || !mockLlm} onClick={() => run(() => api.failure("groq", "none"), "Language model healthy.")}>healthy</Button>
              </div>
            </div>
            <div className="border-t border-line pt-3">
              <Button tone="danger" size="sm" icon="refresh" disabled={busy} onClick={() => run(api.reset, "Demo reset: balance, card, history and audit restored.")}>Reset demo data</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export function ModePills({ providers }: { providers: Record<string, string | null> }) {
  const live = (v?: string | null) => !!v && !v.startsWith("mock") && v !== "deterministic";
  const item = (label: string, v?: string | null) => <Pill tone={live(v) ? "ok" : "mute"} icon={live(v) ? "bolt" : undefined}>{label}: {live(v) ? "live" : "offline demo"}</Pill>;
  return <div className="flex flex-wrap items-center gap-1.5">{item("Language", providers.intent_parser)}{item("Risk", providers.decision_provider)}</div>;
}

export function FilmLink({ onClick }: { onClick: () => void }) {
  return <Button tone="ghost" size="sm" icon="film" onClick={onClick}>Watch the 90-second video</Button>;
}
