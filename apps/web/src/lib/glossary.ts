// Plain-English explanations shown in tooltips. Keep each to one or two sentences.
export const GLOSSARY: Record<string, { label: string; text: string }> = {
  jev: { label: "Jev", text: "A risk model that answers narrow questions (e.g. “how unusual is this?”) with probabilities. It only gives advice — it can never approve or move money." },
  groq: { label: "Groq", text: "A fast language model. Here it only turns your words into a structured request and writes clarification questions. It cannot approve anything." },
  confidence: { label: "Confidence", text: "How decisive the risk model's answers were (the weakest answer counts). It is not a fraud probability and never grants permission." },
  threshold: { label: "Required confidence", text: "The minimum confidence for this kind of action. Below it, the system asks you a question instead of trusting the model." },
  hardpolicy: { label: "Safety rules", text: "Plain checks of facts — do you own the account, is there enough money, is the recipient allowed. They run before any AI and again at the moment of execution." },
  policy: { label: "Policy", text: "Ordinary, testable code that makes the final decision. Models only supply evidence." },
  escalation: { label: "Escalation", text: "Extra handling when the system is unsure — usually asking you a clarifying question. A second AI never gets to approve." },
  idempotency: { label: "Idempotency key", text: "A unique id sent with each request so a retry (after a lost response) returns the original result instead of repeating the transfer." },
  fingerprint: { label: "Fingerprint", text: "A cryptographic hash of the exact action (who, what, how much). Your approval is bound to it, so a changed amount invalidates the approval." },
  ledger: { label: "Ledger", text: "An append-only record of money movements. Every transfer writes two equal and opposite entries, and the database refuses unbalanced ones." },
  trace: { label: "Trace", text: "The full, ordered history of one request — what was asked, checked, assessed, approved and executed." },
  review: { label: "Manual review", text: "The action is parked for a human. No money moves." },
  grounding: { label: "Grounding", text: "Checking a model's output against facts it can't control — e.g. an amount must literally appear in what you typed." },
  failclosed: { label: "Fail closed", text: "If a safety component is unavailable, the system becomes more cautious (holds the action) rather than skipping the check." },
};
