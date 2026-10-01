import type { ActionView, Assessment, Scored } from "../types";
import { Bar, Card, Mono, Pill, fmtMs, pct } from "./ui";
import type { Tone } from "./ui";

const DECISION_LABEL: Record<string, { label: string; tone: Tone }> = {
  EXECUTE: { label: "EXECUTE", tone: "ok" },
  REQUIRE_CONFIRMATION: { label: "REQUIRE CONFIRMATION", tone: "info" },
  ASK_FOR_CLARIFICATION: { label: "ASK FOR CLARIFICATION", tone: "warn" },
  MANUAL_REVIEW: { label: "MANUAL REVIEW", tone: "warn" },
  BLOCK: { label: "BLOCK", tone: "bad" },
};

function ScoreBlock({ title, s }: { title: string; s: Scored }) {
  const tone = (i: number): Tone => (i <= 1 ? "ok" : i === 2 ? "warn" : "bad");
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-xs">
        <span className="font-medium text-slate-200">{title}</span>
        <span className="text-slate-400">score <Mono>{s.score.toFixed(1)}/{s.labels.length - 1}</Mono> · confidence <Mono>{pct(s.confidence)}</Mono></span>
      </div>
      {s.labels.map((l, i) => <Bar key={l} label={l} value={s.probabilities[i] ?? 0} tone={tone(i)} />)}
    </div>
  );
}

function YesNo({ title, a }: { title: string; a: { yes: number; confidence: number } }) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-xs">
        <span className="font-medium text-slate-200">{title}</span>
        <span className="text-slate-400">confidence <Mono>{pct(a.confidence)}</Mono></span>
      </div>
      <Bar label="YES" value={a.yes} tone="ok" />
      <Bar label="NO" value={1 - a.yes} tone="mute" />
    </div>
  );
}

export function DecisionExplorer({ view }: { view: ActionView }) {
  const ex = view.explorer;
  const hard = ex.hard_policy;
  const hardBlocked = hard?.result.result === "BLOCK";
  const jev = ex.jev;
  const assessment = jev && jev.assessment && !("error" in jev.assessment) ? (jev.assessment as Assessment) : null;
  const dec = view.policy_decision ? DECISION_LABEL[view.policy_decision] : null;
  const pa = view.proposed_action;

  return (
    <div className="space-y-3">
      <Card title="Proposed action" right={<Pill tone="mute">{pa.risk_class.replace("_", " ")}</Pill>}>
        <dl className="grid grid-cols-[7rem_1fr] gap-y-1 text-sm">
          <dt className="text-slate-400">Action</dt><dd className="font-medium">{pa.label} <Mono className="text-slate-500">{pa.type}</Mono></dd>
          {pa.from_account && <><dt className="text-slate-400">From</dt><dd>{pa.from_account}</dd></>}
          {pa.type === "TRANSFER_FUNDS" && <><dt className="text-slate-400">To</dt><dd>{pa.recipient ?? <span className="text-amber-300">— not specified —</span>}</dd></>}
          {pa.type === "TRANSFER_FUNDS" && <><dt className="text-slate-400">Amount</dt><dd>{pa.amount_pretty ?? <span className="text-amber-300">— not specified —</span>} {pa.amount && <Mono className="text-slate-500">{pa.amount}</Mono>}</dd></>}
          {pa.card && <><dt className="text-slate-400">Card</dt><dd>{pa.card}</dd></>}
        </dl>
        <p className="mt-3 text-xs text-slate-500">Language → typed action. Nothing has executed.</p>
      </Card>

      <Card title="Hard policy" right={hard ? <Pill tone={hardBlocked ? "bad" : "ok"}>{hardBlocked ? "BLOCKED" : "PASSED"}</Pill> : undefined}>
        {hard ? (
          <ul className="space-y-1 text-sm">
            {hard.checks.map((c) => (
              <li key={c.name} className="flex items-start gap-2">
                <span className={`mt-0.5 w-14 shrink-0 font-mono text-[11px] font-semibold ${c.status === "PASS" ? "text-emerald-400" : c.status === "FAIL" ? "text-rose-400" : "text-slate-500"}`}>{c.status === "SKIPPED" ? "SKIP" : c.status}</span>
                <span><span className="text-slate-200">{c.name}</span> <span className="text-xs text-slate-500">{c.detail}</span></span>
              </li>
            ))}
          </ul>
        ) : <p className="text-sm text-slate-500">Not evaluated.</p>}
        {hardBlocked && (
          <div className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-200">
            <b>HARD_POLICY_BLOCK · {hard?.result.reason}</b><br />
            AI assessment skipped — 0 Jev calls, 0 Groq calls. Deterministic code was enough.
          </div>
        )}
      </Card>

      <Card title="Jev assessment" right={jev ? <Pill tone={jev.status === "OK" ? "info" : "bad"}>{jev.status === "OK" ? jev.provider : jev.status}</Pill> : <Pill tone="mute">SKIPPED</Pill>}>
        {!jev && <p className="text-sm text-slate-500">{hardBlocked ? "AI assessment skipped (hard policy blocked first)." : "Not required for this action class — deterministic code is enough."}</p>}
        {jev && jev.status !== "OK" && (
          <p className="text-sm text-amber-300">Provider unavailable ({jev.status}). Money movement fails closed → manual review; it is never silently approved.</p>
        )}
        {assessment && (
          <div className="space-y-4">
            {assessment.intent_alignment && <YesNo title="Intent alignment" a={assessment.intent_alignment} />}
            {assessment.behavioral_anomaly && <ScoreBlock title="Behavioural anomaly" s={assessment.behavioral_anomaly} />}
            {assessment.recipient_risk && <ScoreBlock title="Recipient context" s={assessment.recipient_risk} />}
            {assessment.context_sufficient && <YesNo title="Context sufficiency" a={assessment.context_sufficient} />}
            <div className="flex flex-wrap items-center gap-x-6 gap-y-1 border-t border-slate-800 pt-3 text-xs text-slate-400">
              <span>Overall confidence <Mono className="text-slate-100">{pct(jev!.overall_confidence)}</Mono> (weakest link)</span>
              <span>Required <Mono className="text-slate-100">{pct(jev!.threshold)}</Mono></span>
              <span>Latency <Mono className="text-slate-100">{fmtMs(jev!.latency_ms)}</Mono></span>
              <span>Model <Mono className="text-slate-100">{jev!.model ?? "—"}</Mono></span>
              <span>Question set <Mono className="text-slate-100">{jev!.question_set_version}</Mono></span>
              {jev!.credits_charged != null && <span>Credits <Mono className="text-slate-100">{jev!.credits_charged}</Mono></span>}
            </div>
            <p className="text-[11px] text-slate-500">{jev!.note}</p>
          </div>
        )}
      </Card>

      {ex.escalation.triggered && (
        <Card title="Confidence escalation triggered" right={<Pill tone="warn">{ex.escalation.reason}</Pill>}>
          <div className="mb-3 grid grid-cols-3 gap-3 text-sm">
            <div><div className="text-xs text-slate-400">Jev confidence</div><Mono className="text-lg">{pct(ex.escalation.confidence)}</Mono></div>
            <div><div className="text-xs text-slate-400">Required threshold</div><Mono className="text-lg">{pct(ex.escalation.required_threshold)}</Mono></div>
            <div><div className="text-xs text-slate-400">Escalation reason</div><Mono className="text-sm">{ex.escalation.reason}</Mono></div>
          </div>
          <p className="mb-3 text-xs text-slate-500">Low confidence means <b>additional handling</b>, not that a second model decides.</p>
          {ex.escalation.groq ? (
            <div className="rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-sm">
              <div className="mb-1 flex items-center justify-between"><b className="text-slate-200">Groq escalation</b><Pill tone="info">{ex.escalation.groq.model ?? "model"}</Pill></div>
              <dl className="grid grid-cols-[8rem_1fr] gap-y-1 text-xs">
                <dt className="text-slate-400">Purpose</dt><dd>{ex.escalation.groq.purpose}</dd>
                <dt className="text-slate-400">Result</dt><dd>{ex.escalation.groq.result?.missing_information?.length ? `Missing: ${ex.escalation.groq.result.missing_information.join(", ")}` : ex.escalation.groq.status}</dd>
                <dt className="text-slate-400">Recommendation</dt><dd><Mono>{ex.escalation.groq.result?.recommended_handling ?? "—"}</Mono> <span className="text-slate-500">(advisory only)</span></dd>
                <dt className="text-slate-400">Latency / tokens</dt><dd>{fmtMs(ex.escalation.groq.latency_ms)} · {ex.escalation.groq.input_tokens ?? "—"} in / {ex.escalation.groq.output_tokens ?? "—"} out</dd>
              </dl>
              {ex.escalation.groq.result?.clarification_question && (
                <p className="mt-2 rounded bg-amber-500/10 px-3 py-2 text-amber-200">“{ex.escalation.groq.result.clarification_question}”</p>
              )}
            </div>
          ) : <p className="text-sm text-amber-300">Escalation provider unavailable — policy failed safe with a deterministic question / manual review.</p>}
        </Card>
      )}

      <Card title="Final policy decision" right={<Mono className="text-slate-500">{ex.policy_version}</Mono>}>
        {dec ? <Pill tone={dec.tone}>{dec.label}</Pill> : <span className="text-sm text-slate-500">Pending</span>}
        <ul className="mt-3 list-disc space-y-1 pl-5 text-sm text-slate-300">
          {view.decision_reasons.map((r) => <li key={r.code}><span className="text-slate-200">{r.detail}</span> <Mono className="text-slate-600">{r.code}</Mono></li>)}
        </ul>
        <p className="mt-3 text-[11px] text-slate-500">Decided by deterministic Rust policy. Models supplied evidence only.</p>
      </Card>
    </div>
  );
}
