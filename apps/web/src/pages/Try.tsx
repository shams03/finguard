import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, newKey } from "../api";
import { ActionCard } from "../components/ActionCard";
import { Explorer } from "../components/Explorer";
import { Button, Card, Collapsible, Icon, Pill, Stepper, statusLabel, statusTone, useToast } from "../components/ui";
import { SCENARIOS } from "../lib/scenarios";
import type { Scenario, TourId } from "../lib/scenarios";
import type { Step } from "../lib/stages";
import type { ActionListItem, ActionView } from "../types";

export interface DemoCommand { text: string; scenario?: Scenario["setup"] }

const PENDING_STEPS: Step[] = [
  { key: "u", label: "Understood", state: "todo", note: "" }, { key: "r", label: "Safety rules", state: "todo", note: "" },
  { key: "k", label: "Risk check", state: "todo", note: "" }, { key: "d", label: "Decision", state: "todo", note: "" },
  { key: "a", label: "Your approval", state: "todo", note: "" }, { key: "e", label: "Executed", state: "todo", note: "" },
];

export function Try({ pending, onPendingDone, onChanged, onTrace, onOutcome }: {
  pending: DemoCommand | null; onPendingDone: () => void; onChanged: () => void; onTrace: (id: string) => void; onOutcome: (id: TourId) => void;
}) {
  const toast = useToast();
  const [text, setText] = useState("");
  const [view, setView] = useState<ActionView | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<number | undefined>();
  const [history, setHistory] = useState<ActionListItem[]>([]);
  const lastConfirm = useRef<{ actionId: string; confirmationId: string; key: string } | null>(null);
  const timers = useRef<number[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const refreshHistory = useCallback(() => { api.actions().then(setHistory).catch(() => undefined); }, []);
  useEffect(refreshHistory, [refreshHistory]);

  // Animate the pipeline while the server works: honest "we're on it" progress, then the real result replaces it.
  const startProgress = () => {
    timers.current.forEach(clearTimeout);
    setPhase(0);
    timers.current = [window.setTimeout(() => setPhase(1), 350), window.setTimeout(() => setPhase(2), 900)];
  };
  const stopProgress = () => { timers.current.forEach(clearTimeout); setPhase(undefined); };

  const noteOutcome = useCallback((v: ActionView) => {
    const t = v.proposed_action.type;
    if (t === "READ_BALANCE" && v.status === "EXECUTED") onOutcome("balance");
    if (t === "TRANSFER_FUNDS" && v.status === "EXECUTED") onOutcome("transfer");
    if (v.status === "AWAITING_CLARIFICATION") onOutcome("ambiguous");
    if (v.status === "MANUAL_REVIEW") onOutcome("suspicious");
  }, [onOutcome]);

  const guard = useCallback(async (fn: () => Promise<void>, withProgress = false) => {
    setBusy(true);
    if (withProgress) startProgress();
    try { await fn(); } catch (e) {
      toast("bad", e instanceof ApiError ? `${e.body.message} (${e.body.code})` : (e as Error).message);
    } finally { stopProgress(); setBusy(false); refreshHistory(); onChanged(); }
  }, [toast, refreshHistory, onChanged]);

  const run = useCallback((cmd: string, scenario?: Scenario["setup"]) => guard(async () => {
    lastConfirm.current = null;
    if (scenario) await api.scenario(scenario);
    const v = await api.command(cmd, newKey());
    setView(v); noteOutcome(v);
  }, true), [guard, noteOutcome]);

  useEffect(() => {
    if (!pending) return;
    const c = pending; onPendingDone(); setText(c.text); void run(c.text, c.scenario);
  }, [pending, onPendingDone, run]);

  const confirm = (v: ActionView) => guard(async () => {
    if (!v.confirmation) return;
    const key = newKey();
    lastConfirm.current = { actionId: v.action_id, confirmationId: v.confirmation.confirmation_id, key };
    try {
      const done = await api.confirm(v.action_id, v.confirmation.confirmation_id, key);
      setView(done); noteOutcome(done); toast("ok", "Executed in one database transaction.");
    } catch (e) { setView(await api.action(v.action_id)); throw e; }
  });
  const retryLost = () => guard(async () => {
    const l = lastConfirm.current; if (!l) return;
    const again = await api.confirm(l.actionId, l.confirmationId, l.key);
    setView(again); toast("info", "Same Idempotency-Key + same request → the stored result came back. No second transfer.");
  });
  const cancel = (v: ActionView) => guard(async () => { setView(await api.cancel(v.action_id)); toast("warn", "Cancelled. No funds moved."); });
  const clarify = (v: ActionView, body: { accept_suggestion?: boolean; amount?: string }) => guard(async () => {
    const nv = await api.clarify(v.action_id, body, newKey());
    setView(nv); noteOutcome(nv); toast("info", "A brand-new proposal was created from your answer, and every check ran again.");
  }, true);
  const reset = () => guard(async () => { await api.reset(); setView(null); toast("info", "Demo reset."); });
  const open = (id: string) => guard(async () => { setView(await api.action(id)); });

  const showGuide = !view && phase === undefined;
  return (
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)]">
      <div className="space-y-5">
        <Card title="What would you like to do?">
          <form onSubmit={(e) => { e.preventDefault(); if (text.trim()) void run(text); }} className="flex flex-col gap-2 sm:flex-row">
            <label htmlFor="cmd" className="sr-only">Your request</label>
            <input id="cmd" ref={inputRef} value={text} onChange={(e) => setText(e.target.value)} placeholder="e.g. Send ₱2,000 to Alex" maxLength={500} autoComplete="off"
              className="flex-1 rounded-xl border border-slate-700 bg-slate-950 px-4 py-3 text-base outline-none focus:border-emerald-400" />
            <Button type="submit" tone="primary" size="lg" icon="arrow" disabled={busy || !text.trim()}>{busy ? "Working…" : "Send"}</Button>
          </form>
          <p className="mt-2 text-xs text-slate-400">Type in plain English, or pick a scenario below. Nothing moves until you approve.</p>
        </Card>

        {phase !== undefined && (
          <Card title="Working on it…"><Stepper steps={PENDING_STEPS} running={phase} /><p className="mt-3 text-xs text-slate-400">Live providers can take a few seconds. Safety rules run first; AI is only consulted when it can change the outcome.</p></Card>
        )}

        {view && phase === undefined && (
          <ActionCard view={view} h={{
            busy, onConfirm: () => confirm(view), onCancel: () => cancel(view), onReset: reset,
            onClarify: (b) => clarify(view, b), onRetryLost: lastConfirm.current ? retryLost : undefined, onTrace: () => onTrace(view.action_id),
          }} />
        )}

        {showGuide && (
          <div className="space-y-4">
            {(["Simple", "Needs a human", "Stopped"] as const).map((g) => (
              <section key={g}>
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-[0.12em] text-slate-400">{g === "Simple" ? "Simple — answered instantly" : g === "Needs a human" ? "Needs a human in the loop" : "Stopped by the rules"}</h3>
                <div className="grid gap-2 sm:grid-cols-2">
                  {SCENARIOS.filter((s) => s.group === g).map((s, i) => (
                    <button key={s.id} onClick={() => { setText(s.text); void run(s.text, s.setup); }} disabled={busy} className="reveal group rounded-xl border border-line bg-panel/70 p-3 text-left transition hover:border-emerald-400/50 hover:bg-slate-800/60" style={{ ["--i" as string]: i }}>
                      <div className="flex items-center gap-2 text-sm font-medium text-slate-100"><Icon name={s.icon} className="h-4 w-4 text-emerald-300" />{s.title}</div>
                      <div className="mt-1 text-xs text-slate-400">{s.why}</div>
                      <div className="mt-2"><Pill tone="mute">→ {s.expect}</Pill></div>
                    </button>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}

        {history.length > 0 && (
          <Collapsible title="Recent requests" summary={`${history.length} so far — click one to reopen it`}>
            <ul className="space-y-1">
              {history.slice(0, 10).map((h) => (
                <li key={h.action_id}><button onClick={() => open(h.action_id)} className="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-slate-800/60">
                  <span className="truncate text-slate-200">{h.user_request}</span><Pill tone={statusTone(h.status)}>{statusLabel(h.status)}</Pill></button></li>
              ))}
            </ul>
          </Collapsible>
        )}
      </div>

      <div className="xl:sticky xl:top-4 xl:max-h-[calc(100vh-2rem)] xl:self-start xl:overflow-y-auto">
        {view && phase === undefined ? <Explorer view={view} /> : (
          <Card title="Why this decision?"><p className="text-sm text-slate-300">After you send a request, this panel explains — step by step — what the system understood, which rules it checked, what the risk model said, and why the final decision was made.</p>
            <p className="mt-3 text-xs text-slate-400">Tip: hover dotted terms such as <i>Jev</i> or <i>fingerprint</i> for a plain-English explanation.</p></Card>
        )}
      </div>
    </div>
  );
}
