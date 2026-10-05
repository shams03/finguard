import { useEffect, useState } from "react";
import { api } from "../api";
import type { ActionListItem, TraceEvent, TraceStage, TraceView } from "../types";
import { Bar, Button, Card, Json, Mono, Pill, Spinner, Term, fmtMs, pct, statusLabel, statusTone } from "../components/ui";
import type { Tone } from "../components/ui";

const STAGE_TONE: Record<TraceStage["status"], Tone> = { passed: "ok", warning: "warn", blocked: "bad", skipped: "mute", executed: "exec", pending: "info" };
const CAT_DOT: Record<string, string> = {
  request: "bg-slate-400", intent: "bg-sky-400", action: "bg-slate-300", policy: "bg-emerald-400", jev: "bg-violet-400",
  escalation: "bg-amber-400", groq: "bg-orange-400", confirmation: "bg-cyan-400", execution: "bg-emerald-300", database: "bg-indigo-400", idempotency: "bg-sky-300", other: "bg-slate-500",
};

const clock = (iso: string) => {
  const d = new Date(iso);
  return `${d.toLocaleTimeString("en-GB")}.${String(d.getMilliseconds()).padStart(3, "0")}`;
};

export function Trace({ actionId, onSelect, onBack }: { actionId: string | null; onSelect: (id: string) => void; onBack: () => void }) {
  const [trace, setTrace] = useState<TraceView | null>(null);
  const [list, setList] = useState<ActionListItem[]>([]);
  const [sel, setSel] = useState<TraceEvent | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => { api.actions().then(setList).catch(() => undefined); }, [actionId]);
  useEffect(() => {
    setTrace(null); setSel(null); setErr(null);
    if (!actionId) return;
    api.trace(actionId).then(setTrace).catch((e: Error) => setErr(e.message));
  }, [actionId]);

  if (!actionId) {
    return (
      <Card title="Trace viewer">
        <p className="mb-3 text-sm text-slate-300">A <Term k="trace" /> reconstructs one request from the append-only audit log: what was asked, checked, assessed, approved and executed — with timings. Pick a request:</p>
        <ul className="space-y-1.5">
          {list.map((h) => (
            <li key={h.action_id}><button onClick={() => onSelect(h.action_id)} className="flex w-full items-center justify-between rounded-lg border border-slate-800 px-3 py-2 text-left text-sm hover:border-slate-600">
              <span>{h.user_request}</span><Pill tone={statusTone(h.status)}>{statusLabel(h.status)}</Pill></button></li>
          ))}
          {list.length === 0 && <li className="text-sm text-slate-500">No actions yet. Run something in the Agent Console.</li>}
        </ul>
      </Card>
    );
  }
  if (err) return <Card title="Trace"><p className="text-sm text-rose-300">{err}</p></Card>;
  if (!trace) return <div className="p-8"><Spinner /></div>;

  const L = trace.latency;
  const maxLat = Math.max(1, L.intent_parsing_ms, L.hard_policy_ms, L.jev_assessment_ms, L.groq_escalation_ms, L.db_execution_ms);
  const jev0 = trace.jev[0] as Record<string, unknown> | undefined;

  return (
    <div className="space-y-4">
      <Card title="Trace" right={<div className="flex items-center gap-2"><Pill tone={statusTone(trace.status)}>{statusLabel(trace.status)}</Pill><Button tone="ghost" size="sm" onClick={onBack}>← Back to the request</Button></div>}>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm md:grid-cols-4">
          <div><dt className="text-xs text-slate-400">Trace</dt><dd><Mono>{trace.trace_id}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Action</dt><dd><Mono>{trace.action_id}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Action type</dt><dd><Mono>{trace.action_type}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Policy</dt><dd><Mono>{trace.policy_version}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Started</dt><dd><Mono>{clock(trace.started_at)}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Completed</dt><dd><Mono>{trace.completed_at ? clock(trace.completed_at) : "—"}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Total</dt><dd><Mono>{fmtMs(trace.total_duration_ms)}</Mono></dd></div>
          <div><dt className="text-xs text-slate-400">Versions</dt><dd className="text-xs text-slate-400">{Object.values(trace.versions).join(" · ")}</dd></div>
        </dl>
        {trace.parent_action_id && <p className="mt-2 text-xs text-slate-500">Follows superseded action <button className="underline" onClick={() => onSelect(trace.parent_action_id!)}>{trace.parent_action_id}</button></p>}
      </Card>

      <Card title="Decision summary">
        <ol className="flex flex-wrap items-stretch gap-2">
          {trace.stages.map((s, i) => (
            <li key={s.key} className="flex items-center gap-2">
              <div className="rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2" title={s.detail}>
                <div className="text-xs uppercase tracking-wider text-slate-500">{s.label}</div>
                <Pill tone={STAGE_TONE[s.status]}>{s.status}</Pill>
              </div>
              {i < trace.stages.length - 1 && <span className="text-slate-600">→</span>}
            </li>
          ))}
        </ol>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[1fr_24rem]">
        <Card title="Timeline" right={<span className="text-xs text-slate-500">click an event for safe metadata</span>}>
          <ol className="relative space-y-0.5 border-l border-slate-800 pl-4">
            {trace.events.map((e) => (
              <li key={e.seq}>
                <button onClick={() => setSel(e)} className={`relative w-full rounded-md px-2 py-1.5 text-left hover:bg-slate-800/60 ${sel?.seq === e.seq ? "bg-slate-800" : ""}`}>
                  <span className={`absolute -left-[1.32rem] top-3 h-2 w-2 rounded-full ${CAT_DOT[e.category] ?? CAT_DOT.other}`} />
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="text-sm text-slate-100">{e.label}</span>
                    <Mono className="text-slate-500">{clock(e.occurred_at)}</Mono>
                  </div>
                  <div className="flex gap-3 text-xs text-slate-500">
                    <span>+{e.offset_ms} ms</span>
                    {e.duration_ms != null && <span className="text-slate-300">{e.duration_ms} ms</span>}
                    {typeof e.metadata.overall_confidence === "number" && <span>Confidence: {pct(e.metadata.overall_confidence)}</span>}
                    {typeof e.metadata.decision === "string" && <span className="text-slate-300">{e.metadata.decision}</span>}
                  </div>
                </button>
              </li>
            ))}
          </ol>
        </Card>

        <div className="space-y-4">
          <Card title="Event details">
            {sel ? (
              <div className="space-y-2 text-sm">
                <div className="font-medium">{sel.label}</div>
                <Mono className="text-slate-500">{sel.event_type} · #{sel.seq}</Mono>
                {sel.category === "jev" && jev0 && (
                  <dl className="grid grid-cols-[7rem_1fr] gap-y-0.5 text-xs">
                    <dt className="text-slate-400">Provider</dt><dd>{String(jev0.provider)}</dd>
                    <dt className="text-slate-400">Question set</dt><dd><Mono>{String(jev0.question_set_version)}</Mono></dd>
                    <dt className="text-slate-400">Model</dt><dd><Mono>{String(jev0.model ?? "—")}</Mono></dd>
                    <dt className="text-slate-400">Latency</dt><dd>{fmtMs(jev0.latency_ms as number)}</dd>
                    <dt className="text-slate-400">Confidence</dt><dd>{pct(jev0.overall_confidence as number)}</dd>
                  </dl>
                )}
                {sel.category === "groq" && trace.groq.length > 0 && (
                  <dl className="grid grid-cols-[7rem_1fr] gap-y-0.5 text-xs">
                    {trace.groq.filter((g) => String(g.purpose) === "ESCALATION_ANALYSIS" || sel.event_type.startsWith("groq")).slice(0, 1).map((g) => (
                      <div key="g" className="contents">
                        <dt className="text-slate-400">Purpose</dt><dd>{String(g.purpose)}</dd>
                        <dt className="text-slate-400">Model</dt><dd><Mono>{String(g.model ?? "—")}</Mono></dd>
                        <dt className="text-slate-400">Latency</dt><dd>{fmtMs(g.latency_ms as number)}</dd>
                      </div>
                    ))}
                  </dl>
                )}
                <Json value={sel.metadata} />
                <p className="text-xs text-slate-500">Metadata is allow-listed and redacted. No secrets, headers or raw provider payloads are stored.</p>
              </div>
            ) : <p className="text-sm text-slate-500">Select an event.</p>}
          </Card>

          <Card title="Latency breakdown">
            <div className="space-y-2">
              <Bar label="Intent parsing" value={L.intent_parsing_ms / maxLat} right={fmtMs(L.intent_parsing_ms)} />
              <Bar label="Hard policy" value={L.hard_policy_ms / maxLat} tone="ok" right={fmtMs(L.hard_policy_ms)} />
              <Bar label="Jev assessment" value={L.jev_assessment_ms / maxLat} tone="exec" right={fmtMs(L.jev_assessment_ms)} />
              <Bar label="Groq escalation" value={L.groq_escalation_ms / maxLat} tone="warn" right={fmtMs(L.groq_escalation_ms)} />
              <Bar label="DB execution" value={L.db_execution_ms / maxLat} tone="info" right={fmtMs(L.db_execution_ms)} />
            </div>
            <p className="mt-2 text-xs text-slate-500">External model latency vs deterministic policy vs execution.</p>
          </Card>

          <Card title="Provider usage">
            <ul className="space-y-1 text-sm">
              <li className="flex justify-between"><span className="text-slate-400">Jev credits</span><Mono>{trace.usage.jev_credits_charged ?? "n/a"}</Mono></li>
              <li className="flex justify-between"><span className="text-slate-400">Groq input tokens</span><Mono>{trace.usage.groq_input_tokens}</Mono></li>
              <li className="flex justify-between"><span className="text-slate-400">Groq output tokens</span><Mono>{trace.usage.groq_output_tokens}</Mono></li>
            </ul>
            <p className="mt-2 text-xs text-slate-500">Provider-reported usage only; no pricing is inferred.</p>
          </Card>
        </div>
      </div>

      {trace.ledger && (
        <Card title="Ledger" right={<Mono className="text-slate-500">{trace.ledger.ledger_transaction_id}</Mono>}>
          <ul className="text-sm">{trace.ledger.entries.map((e, i) => <li key={i} className="flex justify-between border-b border-slate-800 py-1 last:border-0"><span>{e.account}</span><Mono className={e.amount_minor < 0 ? "text-rose-300" : "text-emerald-300"}>{e.amount}</Mono></li>)}</ul>
          <p className="mt-2 text-xs text-slate-500">Append-only, balanced to zero (enforced by a deferred database trigger).</p>
        </Card>
      )}

      <Card title="What this trace can answer">
        <dl className="grid gap-x-8 gap-y-2 md:grid-cols-2">
          {trace.audit_answers.map(([q, a]) => <div key={q}><dt className="text-xs text-slate-400">{q}</dt><dd className="text-sm">{a}</dd></div>)}
        </dl>
      </Card>
    </div>
  );
}
