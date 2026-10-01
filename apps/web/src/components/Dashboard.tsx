import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { AccountResp, CardView, RecipientView, TransactionView } from "../types";
import { Button, Card, Mono, Pill, Spinner } from "./ui";

export interface DemoCommand { text: string; scenario?: "normal" | "suspicious" | "low_balance" }

const DEMOS: { label: string; cmd: DemoCommand; hint: string }[] = [
  { label: "Check Balance", cmd: { text: "What's my available balance?", scenario: "normal" }, hint: "Deterministic. No AI called." },
  { label: "Freeze Card", cmd: { text: "Freeze my card.", scenario: "normal" }, hint: "Reversible → confirmation." },
  { label: "Normal Transfer", cmd: { text: "Send ₱2,000 to Alex.", scenario: "normal" }, hint: "Jev low risk → confirm." },
  { label: "Ambiguous Transfer", cmd: { text: "Send Alex the usual amount.", scenario: "normal" }, hint: "Low confidence → Groq clarification." },
  { label: "Suspicious Transfer", cmd: { text: "Send ₱75,000 to this new account.", scenario: "suspicious" }, hint: "New payee, odd device → review." },
  { label: "Insufficient Funds", cmd: { text: "Send ₱40,000 to Alex.", scenario: "low_balance" }, hint: "Hard block, 0 AI calls." },
];

export function Dashboard({ refreshKey, onRun, onChanged }: { refreshKey: number; onRun: (c: DemoCommand) => void; onChanged: () => void }) {
  const [acct, setAcct] = useState<AccountResp | null>(null);
  const [cards, setCards] = useState<CardView[]>([]);
  const [rcps, setRcps] = useState<RecipientView[]>([]);
  const [txs, setTxs] = useState<TransactionView[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [a, c, r, t] = await Promise.all([api.account(), api.cards(), api.recipients(), api.transactions()]);
      setAcct(a); setCards(c); setRcps(r); setTxs(t); setErr(null);
    } catch (e) {
      setErr((e as Error).message);
    }
  }, []);
  useEffect(() => { void load(); }, [load, refreshKey]);

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try { await fn(); onChanged(); await load(); } catch (e) { setErr((e as Error).message); } finally { setBusy(false); }
  };

  if (err && !acct) {
    return (
      <Card title="Cannot load dashboard">
        <p className="text-sm text-rose-300">{err}</p>
        <div className="mt-3"><Button tone="primary" onClick={() => act(api.reset)}>Seed demo data</Button></div>
      </Card>
    );
  }
  if (!acct) return <div className="p-8"><Spinner /></div>;
  const p = acct.profile;

  return (
    <div className="space-y-4">
      <div className="grid gap-4 md:grid-cols-3">
        <Card title="Available balance" className="md:col-span-1">
          <div className="text-3xl font-semibold tracking-tight">{acct.account.balance}</div>
          <div className="mt-1 text-xs text-slate-400">{acct.account.name} · {p.customer_name} · synthetic data</div>
        </Card>
        <Card title="Card">
          {cards.map((c) => (
            <div key={c.id} className="flex items-center justify-between">
              <Mono className="text-base">•••• {c.last4}</Mono>
              <Pill tone={c.state === "ACTIVE" ? "ok" : "bad"}>{c.state}</Pill>
            </div>
          ))}
        </Card>
        <Card title="Device & behaviour">
          <ul className="space-y-1 text-sm">
            <li className="flex justify-between"><span className="text-slate-400">Device</span><span>{p.device_label} <Pill tone={p.device_trusted ? "ok" : "bad"}>{p.device_trusted ? "trusted" : "unrecognised"}</Pill></span></li>
            <li className="flex justify-between"><span className="text-slate-400">Location</span><span>{p.current_location} <Pill tone={p.location_familiar ? "ok" : "warn"}>{p.location_familiar ? "familiar" : "unfamiliar"}</Pill></span></li>
            <li className="flex justify-between"><span className="text-slate-400">Normal transfer range</span><Mono>{p.typical_transfer_min} – {p.typical_transfer_max}</Mono></li>
          </ul>
        </Card>
      </div>

      <Card title="Quick demos" right={<span className="text-[11px] text-slate-500">each button prepares its scenario, then runs the command in the Agent Console</span>}>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {DEMOS.map((d) => (
            <button key={d.label} onClick={() => onRun(d.cmd)} className="rounded-lg border border-slate-700 bg-slate-800/60 px-3 py-2 text-left transition hover:border-emerald-500/50 hover:bg-slate-800">
              <div className="text-sm font-medium">{d.label}</div>
              <div className="text-[11px] text-slate-400">{d.hint}</div>
            </button>
          ))}
        </div>
        <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-slate-800 pt-3 text-xs text-slate-400">
          <span>Failure injection (mock providers):</span>
          {["none", "timeout", "429", "5xx"].map((m) => (
            <Button key={m} tone="ghost" disabled={busy} onClick={() => act(() => api.failure("jev", m))}>Jev: {m}</Button>
          ))}
          <span className="mx-1 text-slate-700">|</span>
          <Button tone="ghost" disabled={busy} onClick={() => act(() => api.failure("groq", "timeout"))}>Groq: timeout</Button>
          <Button tone="ghost" disabled={busy} onClick={() => act(() => api.failure("groq", "none"))}>Groq: none</Button>
          <span className="mx-1 text-slate-700">|</span>
          <Button tone="danger" disabled={busy} onClick={() => act(api.reset)}>Reset demo</Button>
        </div>
      </Card>

      <div className="grid gap-4 lg:grid-cols-3">
        <Card title="Recent transactions" className="lg:col-span-2">
          <ul className="divide-y divide-slate-800">
            {txs.slice(0, 12).map((t) => (
              <li key={t.id} className="flex items-center justify-between py-1.5 text-sm">
                <span><span className="text-slate-200">{t.description}</span> <span className="text-xs text-slate-500">{t.category} · {new Date(t.occurred_at).toLocaleDateString()}</span></span>
                <Mono className={t.amount_minor < 0 ? "text-slate-200" : "text-emerald-300"}>{t.amount}</Mono>
              </li>
            ))}
          </ul>
        </Card>
        <Card title="Known recipients">
          <ul className="space-y-2">
            {rcps.map((r) => (
              <li key={r.id} className="text-sm">
                <div className="flex items-center justify-between"><span>{r.display_name}</span><Pill tone={r.known ? "ok" : "warn"}>{r.known ? "known" : "new"}</Pill></div>
                <div className="text-xs text-slate-500">{r.account_masked} · {r.age_days} days old</div>
              </li>
            ))}
          </ul>
        </Card>
      </div>
    </div>
  );
}
