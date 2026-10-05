import type { ActionView } from "../types";

export type StepState = "done" | "warn" | "stop" | "wait" | "skip" | "todo" | "now";
export interface Step { key: string; label: string; state: StepState; note: string }

/** Human-friendly summary of an action's journey through the pipeline. */
export function deriveSteps(v: ActionView): Step[] {
  const ex = v.explorer;
  const hard = ex.hard_policy;
  const hardBlocked = hard?.result.result === "BLOCK";
  const jev = ex.jev;
  const escalated = ex.escalation?.triggered;
  const decision = v.policy_decision;
  const st = v.status;
  const isRead = v.proposed_action.type === "READ_BALANCE";
  const failed = hard?.checks.filter((c) => c.status === "FAIL") ?? [];
  const passed = hard?.checks.filter((c) => c.status === "PASS").length ?? 0;

  const riskState: StepState = hardBlocked ? "skip" : !jev ? "skip" : jev.status !== "OK" ? "warn" : escalated || (jev.confidence_sufficient === false) ? "warn" : "done";
  const riskNote = hardBlocked ? "Skipped — rules stopped it first" : !jev ? "Not needed for this action" : jev.status !== "OK" ? "Provider unavailable → held" : escalated ? "Not sure → asked a question" : "Looks normal";

  return [
    { key: "understood", label: "Understood", state: "done", note: "Your words became a structured request" },
    { key: "rules", label: "Safety rules", state: hardBlocked ? "stop" : "done", note: hardBlocked ? (failed[0]?.name ?? "Blocked") + " failed" : `${passed} checks passed` },
    { key: "risk", label: "Risk check", state: riskState, note: riskNote },
    { key: "decision", label: "Decision", state: decision === "BLOCK" ? "stop" : decision === "MANUAL_REVIEW" || decision === "ASK_FOR_CLARIFICATION" ? "warn" : decision ? "done" : "wait",
      note: ({ EXECUTE: "Safe to run", REQUIRE_CONFIRMATION: "Needs your approval", ASK_FOR_CLARIFICATION: "Needs one more detail", MANUAL_REVIEW: "Held for a human", BLOCK: "Not allowed" } as Record<string, string>)[decision ?? ""] ?? "Pending" },
    { key: "approval", label: "Your approval", state: isRead ? "skip" : st === "AWAITING_CONFIRMATION" ? "now" : st === "EXECUTED" ? "done" : st === "CANCELLED" ? "stop" : st === "EXPIRED" ? "stop" : "todo",
      note: isRead ? "Not needed to read" : st === "AWAITING_CONFIRMATION" ? "Waiting for you" : st === "EXECUTED" ? "Approved" : st === "CANCELLED" ? "Cancelled" : st === "EXPIRED" ? "Expired" : "Only if the decision allows" },
    { key: "done", label: "Executed", state: st === "EXECUTED" ? "done" : st === "BLOCKED" || st === "FAILED" ? "stop" : "todo", note: st === "EXECUTED" ? "Completed in one transaction" : "Nothing has moved" },
  ];
}

export interface Headline { tone: "ok" | "info" | "warn" | "bad" | "exec"; icon: "check" | "hand" | "question" | "pause" | "stop" | "clock"; title: string; body: string; next: string }

const fmtResultBalance = (v: ActionView) => String(v.result?.balance ?? "");

/** One plain-language sentence for what just happened, and what happens next. */
export function headline(v: ActionView): Headline {
  const p = v.proposed_action;
  const amt = p.amount_pretty ?? "";
  switch (v.status) {
    case "EXECUTED":
      if (p.type === "READ_BALANCE") return { tone: "exec", icon: "check", title: `Your balance is ${fmtResultBalance(v)}`, body: "Read directly from the database. No AI was needed.", next: "Nothing else to do." };
      if (p.type === "FREEZE_CARD") return { tone: "exec", icon: "check", title: "Your card is frozen", body: (v.result as { already_frozen?: boolean } | null)?.already_frozen ? "It was already frozen — freezing is safe to repeat." : "No new card payments will go through.", next: "You can open the trace to see exactly how this was approved." };
      return { tone: "exec", icon: "check", title: `${amt} sent to ${p.recipient}`, body: `Your new balance is ${fmtResultBalance(v)}. Two equal and opposite ledger entries were written.`, next: "Open the trace to see every step, including the database lock and ledger entries." };
    case "AWAITING_CONFIRMATION":
      return { tone: "info", icon: "hand", title: p.type === "TRANSFER_FUNDS" ? `Approve sending ${amt} to ${p.recipient}?` : `Approve: ${p.label.toLowerCase()}?`, body: "Nothing has happened yet. Your approval is tied to exactly this action and can be used once.", next: "Confirm to run it, or cancel. The backend re-checks everything the moment you confirm." };
    case "AWAITING_CLARIFICATION":
      return { tone: "warn", icon: "question", title: "I need one more detail", body: v.clarification?.question ?? "Something is missing from your request.", next: "Your answer starts a brand-new proposal, so the safety checks run again from scratch." };
    case "MANUAL_REVIEW":
      return { tone: "warn", icon: "pause", title: "Held for review — no funds moved", body: "The rules allow this transfer, but the risk signals are unusual, so it needs a person to look at it.", next: "In this demo there is no reviewer queue. Use Reset to try another scenario." };
    case "BLOCKED":
      return { tone: "bad", icon: "stop", title: v.error?.code === "INSUFFICIENT_FUNDS" ? "Not enough funds" : "This can't be done", body: blockedBody(v), next: "No transfer was created. Adjust the request and try again." };
    case "CANCELLED":
      return { tone: "warn", icon: "stop", title: "Cancelled", body: v.parent_action_id ? "This proposal was replaced by a newer, clarified one." : "You cancelled this action. No funds moved.", next: "Start a new request any time." };
    case "EXPIRED":
      return { tone: "warn", icon: "clock", title: "Approval expired", body: "The confirmation window passed, so nothing was executed.", next: "Send the request again." };
    default:
      return { tone: "info", icon: "clock", title: v.status.replaceAll("_", " ").toLowerCase(), body: "", next: "" };
  }
}

function blockedBody(v: ActionView): string {
  const fail = v.explorer.hard_policy?.checks.find((c) => c.status === "FAIL");
  if (fail) return `${fail.name}: ${fail.detail}.`;
  return v.error?.message ?? "Policy blocked this action.";
}
