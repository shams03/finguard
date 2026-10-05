import { useEffect, useState } from "react";
import { headline } from "../lib/stages";
import type { ActionView } from "../types";
import { Button, Icon, Mono, TONE_PANEL, TONE_TEXT, Term } from "./ui";

function Countdown({ to }: { to: string }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => { const t = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(t); }, []);
  const s = Math.max(0, Math.floor((new Date(to).getTime() - now) / 1000));
  return <span className={s < 30 ? "text-amber-300" : ""}>{Math.floor(s / 60)}:{String(s % 60).padStart(2, "0")}</span>;
}

export interface CardHandlers {
  busy: boolean;
  onConfirm: () => void; onCancel: () => void; onReset: () => void;
  onClarify: (body: { accept_suggestion?: boolean; amount?: string }) => void;
  onRetryLost?: () => void; onTrace: () => void;
}

export function ActionCard({ view, h }: { view: ActionView; h: CardHandlers }) {
  const hl = headline(view);
  const pa = view.proposed_action;
  const [amount, setAmount] = useState("");
  const conf = view.confirmation;

  return (
    <section aria-live="polite" className={`animate-fade-up rounded-2xl border p-5 ${TONE_PANEL[hl.tone]}`}>
      <div className="flex items-start gap-4">
        <span className={`mt-0.5 flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-slate-950/60 ring-1 ring-inset ring-current ${TONE_TEXT[hl.tone]}`}><Icon name={hl.icon} className="h-5 w-5" stroke={2.2} /></span>
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-semibold leading-tight text-slate-50">{hl.title}</h2>
          <p className="mt-1 text-sm text-slate-300">{hl.body}</p>
        </div>
      </div>

      {view.status === "AWAITING_CONFIRMATION" && conf && (
        <div className="mt-4 rounded-xl border border-sky-500/30 bg-slate-950/50 p-4">
          {pa.type === "TRANSFER_FUNDS" ? (
            <dl className="grid grid-cols-[4.5rem_1fr] items-baseline gap-y-1 text-sm">
              <dt className="text-slate-400">Send</dt><dd className="text-3xl font-semibold tracking-tight text-slate-50">{pa.amount_pretty}</dd>
              <dt className="text-slate-400">To</dt><dd className="text-slate-100">{pa.recipient}</dd>
              <dt className="text-slate-400">From</dt><dd className="text-slate-100">{pa.from_account}</dd>
            </dl>
          ) : <p className="text-lg font-medium text-slate-50">{pa.label}{pa.card ? ` · ${pa.card}` : ""}</p>}
          <p className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-400">
            <span><Icon name="lock" className="mr-1 inline h-3.5 w-3.5" />Bound to this exact action by its <Term k="fingerprint">fingerprint</Term></span>
            <span>Single use</span><span>Expires in <Countdown to={conf.expires_at} /></span>
          </p>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button tone="primary" size="lg" icon="check" disabled={h.busy} onClick={h.onConfirm}>Approve</Button>
            <Button tone="ghost" size="lg" disabled={h.busy} onClick={h.onCancel}>Cancel</Button>
          </div>
        </div>
      )}

      {view.status === "AWAITING_CLARIFICATION" && view.clarification && (
        <div className="mt-4 rounded-xl border border-amber-500/30 bg-slate-950/50 p-4">
          <div className="flex flex-wrap items-center gap-2">
            {view.clarification.suggested_amount && <Button tone="primary" icon="check" disabled={h.busy} onClick={() => h.onClarify({ accept_suggestion: true })}>Yes, use {view.clarification.suggested_amount}</Button>}
            <span className="text-xs text-slate-400">{view.clarification.suggested_amount ? "or" : "Enter"}</span>
            <input value={amount} onChange={(e) => setAmount(e.target.value)} inputMode="decimal" placeholder="amount, e.g. 3000" aria-label="Amount in pesos"
              onKeyDown={(e) => { if (e.key === "Enter" && amount.trim()) h.onClarify({ amount }); }}
              className="w-44 rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm outline-none focus:border-amber-400" />
            <Button disabled={h.busy || !amount.trim()} onClick={() => h.onClarify({ amount })}>Use this amount</Button>
            <Button tone="ghost" disabled={h.busy} onClick={h.onCancel}>Cancel</Button>
          </div>
          {view.clarification.suggested_amount && <p className="mt-2 text-xs text-slate-400">The suggestion comes from your own past transfers (a database query), never from an AI's guess.</p>}
        </div>
      )}

      {view.status === "MANUAL_REVIEW" && (
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-amber-500/30 bg-slate-950/50 p-4">
          <ul className="list-disc space-y-0.5 pl-5 text-sm text-amber-100/90">{view.decision_reasons.slice(0, 4).map((r) => <li key={r.code}>{r.detail}</li>)}</ul>
          <Button tone="ghost" icon="refresh" disabled={h.busy} onClick={h.onReset}>Reset demo</Button>
        </div>
      )}

      {view.status === "EXECUTED" && view.result && pa.type === "TRANSFER_FUNDS" && (
        <div className="mt-4 rounded-xl border border-violet-500/30 bg-slate-950/50 p-4 text-sm">
          <p className="text-slate-300">Ledger transaction <Mono className="text-slate-100">{String(view.result.ledger_transaction_id ?? "").slice(0, 16)}…</Mono></p>
          {view.replayed && <p className="mt-2 flex items-center gap-2 text-sky-300"><Icon name="refresh" className="h-4 w-4" /> Duplicate retry recognised — the original result was returned. Nothing was transferred twice.</p>}
          {h.onRetryLost && <div className="mt-3"><Button tone="ghost" size="sm" icon="refresh" disabled={h.busy} onClick={h.onRetryLost} title="Resends the identical confirm request with the same Idempotency-Key">Simulate a lost response → client retries</Button></div>}
        </div>
      )}

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-white/10 pt-3 text-sm">
        <p className="text-slate-300"><span className="font-medium text-slate-100">Next: </span>{hl.next}</p>
        <Button tone="ghost" size="sm" icon="list" onClick={h.onTrace}>Open full trace</Button>
      </div>
    </section>
  );
}
