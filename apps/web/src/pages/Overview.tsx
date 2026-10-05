import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { Button, Card, Collapsible, Icon, Mono, Pill, Spinner, Term } from "../components/ui";
import type { DemoCommand } from "./Try";
import { SCENARIOS, TOUR } from "../lib/scenarios";
import type { TourId } from "../lib/scenarios";
import type { AccountResp, CardView, RecipientView, TransactionView } from "../types";

export function Overview({ refreshKey, done, onRun, onGo, onChanged }: {
  refreshKey: number; done: Record<string, boolean>; onRun: (c: DemoCommand) => void; onGo: (to: string) => void; onChanged: () => void;
}) {
  const [acct, setAcct] = useState<AccountResp | null>(null);
  const [cards, setCards] = useState<CardView[]>([]);
  const [rcps, setRcps] = useState<RecipientView[]>([]);
  const [txs, setTxs] = useState<TransactionView[]>([]);
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [a, c, r, t] = await Promise.all([api.account(), api.cards(), api.recipients(), api.transactions()]);
      setAcct(a); setCards(c); setRcps(r); setTxs(t); setErr(null);
    } catch (e) { setErr((e as Error).message); }
  }, []);
  useEffect(() => { void load(); }, [load, refreshKey]);

  const doneCount = TOUR.filter((t) => done[t.id]).length;
  const next = TOUR.find((t) => !done[t.id]);
  const startStep = (id: TourId) => {
    const map: Record<string, string> = { balance: "balance", transfer: "transfer", ambiguous: "ambiguous", suspicious: "suspicious" };
    if (id === "trace") { onGo("#/trace"); return; }
    const s = SCENARIOS.find((x) => x.id === map[id]);
    if (s) onRun({ text: s.text, scenario: s.setup });
  };

  if (err && !acct) {
    return (
      <Card title="Can't reach the backend">
        <p className="text-sm text-rose-300">{err}</p>
        <p className="mt-2 text-sm text-slate-300">Start it with <Mono>cargo run -p finguard-api</Mono>, or seed the demo data below.</p>
        <div className="mt-3"><Button tone="primary" onClick={async () => { await api.reset(); await load(); onChanged(); }}>Seed demo data</Button></div>
      </Card>
    );
  }
  if (!acct) return <div className="flex justify-center p-16"><Spinner className="h-8 w-8" /></div>;
  const p = acct.profile;

  return (
    <div className="space-y-6">
      <section className="grid items-stretch gap-5 lg:grid-cols-[1.4fr_1fr]">
        <div className="rounded-3xl border border-line bg-gradient-to-br from-panel to-slate-900/40 p-7">
          <p className="text-xs font-semibold uppercase tracking-[0.18em] text-emerald-300">Policy-controlled AI for money</p>
          <h2 className="mt-2 text-3xl font-semibold leading-tight tracking-tight text-slate-50 sm:text-4xl">AI can <span className="text-sky-300">propose</span>.<br />Only policy <span className="text-emerald-300">decides</span>.</h2>
          <p className="mt-3 max-w-xl text-slate-300">FinGuard lets an AI agent understand what you want — while ordinary, testable code decides whether it happens. Try it: nothing here is real money.</p>
          <div className="mt-5 flex flex-wrap gap-2">
            <Button tone="primary" size="lg" icon="play" onClick={() => (next ? startStep(next.id) : onGo("#/try"))}>{doneCount === 0 ? "Start the guided demo" : next ? `Next: ${next.title}` : "Try your own request"}</Button>
            <Button size="lg" icon="flow" onClick={() => onGo("#/how")}>See how a request travels</Button>
          </div>
        </div>
        <Card title="Your sandbox account">
          <div className="text-4xl font-semibold tracking-tight text-slate-50">{acct.account.balance}</div>
          <div className="mt-1 text-sm text-slate-400">{acct.account.name} · {p.customer_name} · synthetic data</div>
          <dl className="mt-4 space-y-2 text-sm">
            {cards.map((c) => <div key={c.id} className="flex items-center justify-between"><dt className="text-slate-400">Card •••• {c.last4}</dt><dd><Pill tone={c.state === "ACTIVE" ? "ok" : "bad"}>{c.state === "ACTIVE" ? "active" : "frozen"}</Pill></dd></div>)}
            <div className="flex items-center justify-between"><dt className="text-slate-400">Device</dt><dd><Pill tone={p.device_trusted ? "ok" : "bad"}>{p.device_trusted ? "trusted" : "unrecognised"}</Pill></dd></div>
            <div className="flex items-center justify-between"><dt className="text-slate-400">Location</dt><dd><Pill tone={p.location_familiar ? "ok" : "warn"}>{p.current_location}</Pill></dd></div>
            <div className="flex items-center justify-between"><dt className="text-slate-400">Usual transfers</dt><dd className="text-slate-200">{p.typical_transfer_min} – {p.typical_transfer_max}</dd></div>
          </dl>
        </Card>
      </section>

      <Card title="Guided demo" right={<span className="text-sm text-slate-300">{doneCount} of {TOUR.length} done</span>}>
        <div className="mb-4 h-1.5 overflow-hidden rounded-full bg-slate-800"><div className="h-full rounded-full bg-emerald-400 transition-all duration-700" style={{ width: `${(doneCount / TOUR.length) * 100}%` }} /></div>
        <ol className="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
          {TOUR.map((t, i) => {
            const isDone = !!done[t.id]; const isNext = next?.id === t.id;
            return (
              <li key={t.id} className="reveal" style={{ ["--i" as string]: i }}>
                <button onClick={() => startStep(t.id)} className={`flex h-full w-full flex-col items-start gap-2 rounded-xl border p-4 text-left transition hover:border-emerald-400/60 ${isNext ? "border-emerald-400/60 bg-emerald-500/[0.07]" : "border-line bg-slate-950/40"}`}>
                  <span className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-semibold ring-1 ring-inset ${isDone ? "bg-emerald-500/20 text-emerald-300 ring-emerald-500/50" : "bg-slate-800 text-slate-300 ring-slate-600"}`}>{isDone ? <Icon name="check" className="h-3.5 w-3.5" stroke={3} /> : i + 1}</span>
                  <span className="text-sm font-semibold text-slate-100">{t.title}</span>
                  <span className="text-xs text-slate-400">{t.hint}</span>
                </button>
              </li>
            );
          })}
        </ol>
      </Card>

      <div className="grid gap-5 lg:grid-cols-3">
        {[
          { icon: "brain", t: "Models propose", d: <>A language model (<Term k="groq" />) turns your words into a structured request, and a risk model (<Term k="jev" />) rates it. They only give <i>evidence</i>.</> },
          { icon: "shield", t: "Policy decides", d: <>Plain code checks facts first (<Term k="hardpolicy" />), then makes the final call. Money movement always needs <b>your</b> approval.</> },
          { icon: "db", t: "Executors execute", d: <>One database transaction re-checks everything, writes an append-only <Term k="ledger" />, and is safe to retry (<Term k="idempotency" />).</> },
        ].map((x, i) => (
          <Card key={x.t} className="reveal" ><div style={{ ["--i" as string]: i }} className="flex items-start gap-3"><span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-slate-800 text-emerald-300"><Icon name={x.icon} className="h-5 w-5" /></span><div><h3 className="font-semibold text-slate-100">{x.t}</h3><p className="mt-1 text-sm text-slate-300">{x.d}</p></div></div></Card>
        ))}
      </div>

      <Collapsible title="Account activity & recipients" summary={`${txs.length} recent transactions · ${rcps.length} recipients`}>
        <div className="grid gap-6 lg:grid-cols-[2fr_1fr]">
          <ul className="divide-y divide-line">
            {txs.slice(0, 10).map((t) => (
              <li key={t.id} className="flex items-center justify-between py-2 text-sm"><span><span className="text-slate-100">{t.description}</span> <span className="text-xs text-slate-400">{t.category} · {new Date(t.occurred_at).toLocaleDateString()}</span></span><Mono className={t.amount_minor < 0 ? "text-slate-200" : "text-emerald-300"}>{t.amount}</Mono></li>
            ))}
          </ul>
          <ul className="space-y-3">
            {rcps.map((r) => <li key={r.id} className="text-sm"><div className="flex items-center justify-between"><span className="text-slate-100">{r.display_name}</span><Pill tone={r.known ? "ok" : "warn"}>{r.known ? "known" : "new"}</Pill></div><div className="text-xs text-slate-400">{r.account_masked} · {r.age_days} days old</div></li>)}
          </ul>
        </div>
      </Collapsible>
    </div>
  );
}
