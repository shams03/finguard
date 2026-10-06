import { FlowChart } from "../components/FlowChart";
import { Button, Card, Icon, Term } from "../components/ui";

const GUARANTEES = [
  { icon: "shield", t: "A model can't move money", d: "Models return drafts and evidence in closed schemas. The only code that changes balances is the executor, and it needs a typed action plus your matching approval." },
  { icon: "lock", t: "Approval is bound to the exact action", d: "Your approval carries a fingerprint of who, what and how much. Change the amount afterwards and it no longer matches. It works once and expires." },
  { icon: "refresh", t: "Retries are safe", d: "A lost response doesn't cause a double transfer: the same Idempotency-Key returns the original result, and the database enforces it." },
  { icon: "db", t: "Two transfers can't overspend", d: "The account row is locked and the balance re-checked. With ₱10,000, two simultaneous ₱8,000 transfers → exactly one succeeds. A CHECK constraint backs it up." },
  { icon: "list", t: "Everything is explainable", d: "Append-only audit events and model records let the Trace Viewer answer: what was asked, checked, assessed, approved, and did it happen once?" },
  { icon: "bolt", t: "Providers can fail safely", d: "If the risk model is down, money movement is held for review instead of skipping the check. Reads keep working. Timeouts are explicit and retries are bounded." },
];

const FAILS: [string, string, string][] = [
  ["Risk model (Jev) times out / rate-limits", "Money movement is held for manual review", "fails closed"],
  ["Language model (Groq) is down", "Rule-based parser takes over; clarification uses a fixed question", "falls back"],
  ["Response lost after the transfer committed", "Retry with the same key returns the stored result", "no double transfer"],
  ["Approval expired / reused / tampered", "Rejected; nothing executes", "fails closed"],
  ["Balance changed before you approved", "Re-checked under the lock → blocked", "fails closed"],
  ["Server crashes mid-request", "The transaction rolls back; no partial state", "all or nothing"],
];

export function How({ onGo }: { onGo: (to: string) => void }) {
  return (
    <div className="space-y-8">
      <section>
        <p className="text-xs font-semibold uppercase tracking-[0.18em] text-emerald-300">How it works</p>
        <h2 className="mt-1 text-3xl font-semibold tracking-tight text-slate-50">What happens to your request, step by step</h2>
        <p className="mt-2 max-w-3xl text-slate-300">Every request crosses three zones with strictly different powers: <b className="text-violet-300">models propose</b>, <b className="text-emerald-300">policy decides</b>, <b className="text-sky-300">executors execute</b>. Watch a request travel through them, then click any box to see who decides there and what happens if it fails.</p>
      </section>

      <Card title="Follow a request" right={<Button size="sm" tone="ghost" icon="play" onClick={() => onGo("#/try")}>Try it for real</Button>}>
        <FlowChart />
      </Card>

      <section>
        <h3 className="mb-3 text-xs font-semibold uppercase tracking-[0.12em] text-slate-400">What this design guarantees</h3>
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {GUARANTEES.map((g, i) => (
            <div key={g.t} className="reveal rounded-2xl border border-line bg-panel/70 p-5" style={{ ["--i" as string]: i }}>
              <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-slate-800 text-emerald-300"><Icon name={g.icon} className="h-5 w-5" /></span>
              <h4 className="mt-3 font-semibold text-slate-100">{g.t}</h4><p className="mt-1 text-sm text-slate-300">{g.d}</p>
            </div>
          ))}
        </div>
      </section>

      <Card title="When something goes wrong">
        <div className="overflow-x-auto"><table className="w-full min-w-[560px] text-left text-sm">
          <thead className="text-xs uppercase tracking-wider text-slate-400"><tr><th className="pb-2 pr-4 font-medium">What fails</th><th className="pb-2 pr-4 font-medium">What the system does</th><th className="pb-2 font-medium">Mode</th></tr></thead>
          <tbody className="divide-y divide-line">{FAILS.map(([a, b, c]) => <tr key={a}><td className="py-2.5 pr-4 text-slate-100">{a}</td><td className="py-2.5 pr-4 text-slate-300">{b}</td><td className="py-2.5 text-emerald-300">{c}</td></tr>)}</tbody>
        </table></div>
      </Card>

      <p className="text-sm text-slate-400">Terms you'll meet: <Term k="jev" />, <Term k="groq" />, <Term k="hardpolicy" />, <Term k="confidence" />, <Term k="escalation" />, <Term k="idempotency" />, <Term k="fingerprint" />, <Term k="ledger" />, <Term k="trace" />.</p>
    </div>
  );
}
