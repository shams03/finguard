import { useEffect, useRef, useState } from "react";
import { deriveSteps } from "../lib/stages";
import type { ActionView, Assessment, Scored } from "../types";
import { Bar, Card, Collapsible, Gauge, Mono, Pill, Stepper, Term, fmtMs, pct } from "./ui";
import type { Tone } from "./ui";

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Backend reasons can be codes (e.g. HARD_POLICY_BLOCK/INSUFFICIENT_FUNDS); show the failed rule in words instead. */
function reasonText(v: ActionView, r: { code: string; detail: string }): string {
  if (r.code === "HARD_POLICY_BLOCK") {
    const f = v.explorer.hard_policy?.checks.find((c) => c.status === "FAIL");
    if (f) return `Blocked by a safety rule — ${f.name}: ${f.detail}.`;
  }
  return cap(r.detail);
}

function YesNo({ q, yes, confidence }: { q: string; yes: number; confidence: number }) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between gap-2 text-sm"><span className="text-slate-100">{q}</span><span className="text-xs text-slate-400">decisiveness {pct(confidence)}</span></div>
      <Bar label="Yes" value={yes} tone="ok" /><Bar label="No" value={1 - yes} tone="mute" />
    </div>
  );
}

function Rubric({ q, s }: { q: string; s: Scored }) {
  const tone = (i: number): Tone => (i <= 1 ? "ok" : i === 2 ? "warn" : "bad");
  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between gap-2 text-sm"><span className="text-slate-100">{q}</span><span className="text-xs text-slate-400">score {s.score.toFixed(1)}/{s.labels.length - 1} · confidence {pct(s.confidence)}</span></div>
      {s.labels.map((l, i) => <Bar key={l} label={l} value={s.probabilities[i] ?? 0} tone={tone(i)} />)}
    </div>
  );
}

export function Explorer({ view }: { view: ActionView }) {
  const ex = view.explorer;
  const hard = ex.hard_policy;
  const hardBlocked = hard?.result.result === "BLOCK";
  const jev = ex.jev;
  const assessment = jev?.assessment && !("error" in jev.assessment) ? (jev.assessment as Assessment) : null;
  const esc = ex.escalation;
  const steps = deriveSteps(view);
  const pa = view.proposed_action;
  const passed = hard?.checks.filter((c) => c.status === "PASS").length ?? 0;
  const total = hard?.checks.length ?? 0;

  const [open, setOpen] = useState<Record<string, boolean>>({});
  const prev = useRef(view.action_id);
  useEffect(() => { if (prev.current !== view.action_id) { setOpen({}); prev.current = view.action_id; } }, [view.action_id]);
  const isOpen = (k: string, def: boolean) => open[k] ?? def;
  const toggle = (k: string) => (o: boolean) => setOpen((s) => ({ ...s, [k]: o }));
  const pick = (key: string) => {
    const target = ({ understood: "request", rules: "rules", risk: "risk", decision: "decision", approval: "decision", done: "decision" } as Record<string, string>)[key] ?? "decision";
    setOpen((s) => ({ ...s, [target]: true }));
    window.setTimeout(() => document.getElementById(`sec-${target}`)?.scrollIntoView({ behavior: "smooth", block: "center" }), 30);
  };

  const riskSummary = !jev ? (hardBlocked ? "Skipped — the safety rules stopped it first (0 AI calls)" : "Not needed for this action — plain code is enough")
    : jev.status !== "OK" ? `Risk provider unavailable (${jev.status}) → held for review, never silently approved`
    : `${jev.confidence_sufficient ? "Looks normal" : "Not sure"} · confidence ${pct(jev.overall_confidence)} (needs ${pct(jev.threshold)})`;

  return (
    <Card title="Why this decision?" right={<span className="text-xs text-slate-400">Click a step to jump to its details</span>}>
      <Stepper steps={steps} onPick={pick} />

      <div className="mt-5 space-y-3">
        <Collapsible anchor="sec-request" title="1 · What I understood" summary={`${pa.label}${pa.amount_pretty ? ` · ${pa.amount_pretty}` : ""}${pa.recipient ? ` → ${pa.recipient}` : ""}`} open={isOpen("request", false)} onOpenChange={toggle("request")}>
          <dl className="grid grid-cols-[7rem_1fr] gap-y-1.5 text-sm">
            <dt className="text-slate-400">You said</dt><dd>“{view.user_request}”</dd>
            <dt className="text-slate-400">Became</dt><dd className="font-medium">{pa.label} <Mono className="text-slate-400">{pa.type}</Mono></dd>
            {pa.from_account && <><dt className="text-slate-400">From</dt><dd>{pa.from_account}</dd></>}
            {pa.type === "TRANSFER_FUNDS" && <><dt className="text-slate-400">To</dt><dd>{pa.recipient ?? <span className="text-amber-300">not specified</span>}</dd><dt className="text-slate-400">Amount</dt><dd>{pa.amount_pretty ?? <span className="text-amber-300">not specified</span>}</dd></>}
            {pa.card && <><dt className="text-slate-400">Card</dt><dd>{pa.card}</dd></>}
            <dt className="text-slate-400">Risk class</dt><dd><Pill tone="mute">{pa.risk_class.replace("_", " ").toLowerCase()}</Pill></dd>
          </dl>
          <p className="mt-3 text-xs text-slate-400">Your words became a typed action <i>before</i> anything ran. Only three actions exist; anything else is rejected.</p>
        </Collapsible>

        <Collapsible anchor="sec-rules" title={<>2 · <Term k="hardpolicy" /></>} tone={hardBlocked ? "bad" : "ok"} summary={hard ? (hardBlocked ? `Stopped: ${hard.checks.find((c) => c.status === "FAIL")?.name}` : `${passed} of ${total} checks passed`) : "Not evaluated"} open={isOpen("rules", hardBlocked)} onOpenChange={toggle("rules")}>
          <ul className="space-y-1.5 text-sm">
            {hard?.checks.map((c) => (
              <li key={c.name} className="flex items-start gap-2">
                <span className={`mt-0.5 w-12 shrink-0 font-mono text-xs font-semibold ${c.status === "PASS" ? "text-emerald-400" : c.status === "FAIL" ? "text-rose-400" : "text-slate-500"}`}>{c.status === "SKIPPED" ? "SKIP" : c.status}</span>
                <span><span className="text-slate-100">{c.name}</span> <span className="text-xs text-slate-400">{c.detail}</span></span>
              </li>
            ))}
          </ul>
          {hardBlocked && <p className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-200"><b>No AI was consulted.</b> Facts are checked first, for free, with zero Jev and zero Groq calls.</p>}
        </Collapsible>

        <Collapsible anchor="sec-risk" title={<>3 · Risk check <span className="font-normal text-slate-400">(<Term k="jev" />)</span></>} tone={!jev ? "mute" : jev.status !== "OK" ? "warn" : jev.confidence_sufficient ? "ok" : "warn"} summary={riskSummary} open={isOpen("risk", false)} onOpenChange={toggle("risk")}>
          {jev && jev.overall_confidence != null && <div className="mb-4"><Gauge value={jev.overall_confidence} threshold={jev.threshold} /></div>}
          {assessment ? (
            <div className="space-y-4">
              {assessment.intent_alignment && <YesNo q="Does the action match what you asked?" yes={assessment.intent_alignment.yes} confidence={assessment.intent_alignment.confidence} />}
              {assessment.behavioral_anomaly && <Rubric q="How unusual is this for you?" s={assessment.behavioral_anomaly} />}
              {assessment.recipient_risk && <Rubric q="How risky is this recipient context?" s={assessment.recipient_risk} />}
              {assessment.context_sufficient && <YesNo q="Is there enough information to be safe?" yes={assessment.context_sufficient.yes} confidence={assessment.context_sufficient.confidence} />}
              <div className="flex flex-wrap gap-x-5 gap-y-1 border-t border-line pt-3 text-xs text-slate-400">
                <span>Model <Mono className="text-slate-200">{jev!.model ?? "—"}</Mono></span><span>Latency <Mono className="text-slate-200">{fmtMs(jev!.latency_ms)}</Mono></span>
                <span>Question set <Mono className="text-slate-200">{jev!.question_set_version}</Mono></span>{jev!.credits_charged != null && <span>Credits <Mono className="text-slate-200">{jev!.credits_charged}</Mono></span>}
              </div>
              <p className="text-xs text-slate-400">A model's rating of four narrow questions. It is <b>not</b> a fraud probability and it can never approve anything — it can only add caution.</p>
            </div>
          ) : jev && jev.status !== "OK" ? (
            <p className="text-sm text-amber-200">The risk provider didn't answer ({jev.status}). Because money is involved, the system <Term k="failclosed">fails closed</Term>: the action is held for review instead of skipping the check.</p>
          ) : <p className="text-sm text-slate-400">{riskSummary}.</p>}
        </Collapsible>

        {esc.triggered && (
          <Collapsible anchor="sec-escalation" title="3b · Why I asked you a question" tone="warn" summary={`${esc.reason?.replaceAll("_", " ").toLowerCase()} · confidence ${pct(esc.confidence)} < ${pct(esc.required_threshold)}`} open={isOpen("escalation", true)} onOpenChange={toggle("escalation")}>
            <p className="text-sm text-slate-300">When the system isn't sure, that means <b>extra handling</b> — here, a clarification. A second model never gets to decide for you (<Term k="escalation" />).</p>
            {esc.groq ? (
              <div className="mt-3 rounded-lg border border-line bg-slate-950/60 p-3 text-sm">
                <div className="mb-1 flex items-center justify-between"><b>Clarification written by <Term k="groq" /></b><Pill tone="info">{esc.groq.model ?? "model"}</Pill></div>
                {esc.groq.result?.clarification_question && <p className="rounded bg-amber-500/10 px-3 py-2 text-amber-100">“{esc.groq.result.clarification_question}”</p>}
                <p className="mt-2 text-xs text-slate-400">Recommendation: <Mono>{esc.groq.result?.recommended_handling ?? "—"}</Mono> (advisory) · {fmtMs(esc.groq.latency_ms)} · {esc.groq.input_tokens ?? "—"} in / {esc.groq.output_tokens ?? "—"} out tokens</p>
              </div>
            ) : <p className="mt-3 text-sm text-amber-200">The language provider was unavailable — the system used a fixed, safe question instead.</p>}
          </Collapsible>
        )}

        <Collapsible anchor="sec-decision" title={<>4 · Final decision by <Term k="policy" /></>} tone={view.policy_decision === "BLOCK" ? "bad" : view.policy_decision === "REQUIRE_CONFIRMATION" || view.policy_decision === "EXECUTE" ? "ok" : "warn"} summary={view.decision_reasons[0] ? reasonText(view, view.decision_reasons[0]) : "Pending"} open={isOpen("decision", true)} onOpenChange={toggle("decision")}>
          <ul className="list-disc space-y-1 pl-5 text-sm text-slate-200">{view.decision_reasons.map((r) => <li key={r.code}>{reasonText(view, r)}</li>)}</ul>
          <p className="mt-3 text-xs text-slate-400">Decided by ordinary, tested code ({ex.policy_version}). Models only supplied evidence. Every transfer needs your approval, even at 99% confidence.</p>
        </Collapsible>
      </div>
    </Card>
  );
}
