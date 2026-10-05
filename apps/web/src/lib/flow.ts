// The request journey as data: nodes, edges, scenario paths and per-step explanations.
export type Lane = "entry" | "models" | "policy" | "exec";
export type NodeKind = "normal" | "stop" | "warn" | "ok";

export interface FlowNode { id: string; lane: Lane; x: number; y: number; w: number; h: number; n?: string; title: string; sub: string; kind?: NodeKind }

const M = 35, P = 425, E = 815, W = 330, H = 76;
export const NODES: FlowNode[] = [
  { id: "user", lane: "entry", x: 35, y: 16, w: 220, h: H, title: "You", sub: "type a request" },
  { id: "api", lane: "entry", x: 300, y: 16, w: 260, h: H, title: "Rust API", sub: "request id · trace id · who you are" },
  { id: "idem", lane: "entry", x: 605, y: 16, w: 260, h: H, title: "Retry guard", sub: "Idempotency-Key seen before?" },
  { id: "intent", lane: "models", x: M, y: 150, w: W, h: H, n: "1", title: "Understand", sub: "Groq (or plain rules) turn words into a draft" },
  { id: "typed", lane: "policy", x: P, y: 150, w: W, h: H, n: "2", title: "Typed action", sub: "Only 3 allowed tools · recipient found in SQL" },
  { id: "hard", lane: "policy", x: P, y: 270, w: W, h: H, n: "3", title: "Safety rules", sub: "Facts only: owner, funds, limits — no AI" },
  { id: "blocked", lane: "exec", x: E, y: 270, w: W, h: H, title: "Blocked", sub: "0 AI calls · nothing created", kind: "stop" },
  { id: "risk", lane: "models", x: M, y: 390, w: W, h: H, n: "4", title: "Risk check", sub: "Jev: 4 narrow questions → probabilities" },
  { id: "escal", lane: "models", x: M, y: 510, w: W, h: H, n: "5", title: "Clarify", sub: "Low confidence → Groq writes a question" },
  { id: "policy", lane: "policy", x: P, y: 510, w: W, h: H, n: "6", title: "Final decision", sub: "Plain code. Models are only evidence." },
  { id: "ask", lane: "models", x: M, y: 640, w: W, h: H, title: "Asks you", sub: "your answer = a NEW proposal", kind: "warn" },
  { id: "review", lane: "exec", x: E, y: 390, w: W, h: H, title: "Held for review", sub: "No funds move", kind: "warn" },
  { id: "confirm", lane: "exec", x: E, y: 510, w: W, h: H, n: "7", title: "Your approval", sub: "Single-use · bound to this exact action" },
  { id: "exec", lane: "exec", x: E, y: 640, w: W, h: H, n: "8", title: "Executor", sub: "ONE DB transaction: lock · re-check · ledger" },
  { id: "ledger", lane: "exec", x: E, y: 760, w: W, h: H, n: "9", title: "Ledger + audit", sub: "Append-only · balanced · every step logged", kind: "ok" },
  { id: "trace", lane: "policy", x: P, y: 760, w: W, h: H, n: "10", title: "Trace", sub: "Rebuild the whole story from the audit log" },
];

export interface FlowEdge { from: string; to: string; label?: string; dashed?: boolean }
export const EDGES: FlowEdge[] = [
  { from: "user", to: "api" }, { from: "api", to: "idem" }, { from: "idem", to: "intent" }, { from: "intent", to: "typed" },
  { from: "typed", to: "hard" }, { from: "hard", to: "blocked", label: "a rule fails" }, { from: "hard", to: "risk", label: "passes (money)" },
  { from: "hard", to: "policy", label: "read-only: no AI", dashed: true }, { from: "risk", to: "policy", label: "confident" },
  { from: "risk", to: "escal", label: "not sure" }, { from: "escal", to: "policy", label: "advice only" },
  { from: "policy", to: "ask", label: "needs a detail" }, { from: "policy", to: "review", label: "risky" },
  { from: "policy", to: "confirm", label: "needs approval" }, { from: "confirm", to: "exec" }, { from: "policy", to: "exec", label: "read-only", dashed: true },
  { from: "exec", to: "ledger" }, { from: "ledger", to: "trace" }, { from: "ask", to: "intent", label: "answer", dashed: true },
];

export interface FlowScenario { id: string; title: string; text: string; outcome: string; tone: "ok" | "warn" | "bad" | "info"; path: string[]; notes: Record<string, string> }
const HEAD = ["user", "api", "idem", "intent", "typed", "hard"];
export const FLOW_SCENARIOS: FlowScenario[] = [
  { id: "transfer", title: "Send ₱2,000 to Alex", text: "Send ₱2,000 to Alex.", outcome: "Approved by you, executed once", tone: "ok",
    path: [...HEAD, "risk", "policy", "confirm", "exec", "ledger", "trace"],
    notes: { intent: "amount 200000 centavos, recipient “Alex”", risk: "Normal: ~98% intent match, anomaly 0.2/4. Confidence ≈ 82% ≥ 70%.", policy: "REQUIRE_CONFIRMATION — transfers always need you, even at 99% confidence.", exec: "Balance re-checked under a row lock, then −₱2,000 / +₱2,000 ledger entries." } },
  { id: "balance", title: "What's my balance?", text: "What's my available balance?", outcome: "Answered instantly — no AI at decision time", tone: "ok",
    path: [...HEAD, "policy", "exec", "ledger", "trace"],
    notes: { hard: "All checks pass → straight to the decision, skipping the risk model.", policy: "EXECUTE — a read-only action.", exec: "A SELECT. Nothing changes." } },
  { id: "ambiguous", title: "Send Alex “the usual amount”", text: "Send Alex the usual amount.", outcome: "Asks one question instead of guessing", tone: "warn",
    path: [...HEAD, "risk", "escal", "policy", "ask"],
    notes: { intent: "No amount was written, so none is invented. missing: amount.", hard: "Amount checks are SKIPPED (not silently passed).", risk: "Context is insufficient → low confidence (~34%).", escal: "Groq phrases: “You usually send Alex ₱5,000…” — the ₱5,000 comes from a SQL query.", ask: "Your answer creates a NEW action; every check runs again." } },
  { id: "suspicious", title: "₱75,000 to a brand-new account", text: "Send ₱75,000 to this new account.", outcome: "Held for review — no funds move", tone: "warn",
    path: [...HEAD, "risk", "policy", "review"],
    notes: { hard: "Technically possible: balance is enough, so rules PASS.", risk: "Anomaly 3.8/4, recipient risk 3.9/4.", policy: "MANUAL_REVIEW: new payee + 9× usual amount + unfamiliar phone/location.", review: "Risk evidence can hold an action for a human but never hard-blocks it." } },
  { id: "insufficient", title: "₱40,000 with only ₱20,000", text: "Send ₱40,000 to Alex.", outcome: "Blocked by a plain fact — 0 AI calls", tone: "bad",
    path: [...HEAD, "blocked"],
    notes: { hard: "FAIL Sufficient funds: ₱20,000 < ₱40,000.", blocked: "No Jev call, no Groq call, no transfer row. Cheap, certain checks come first." } },
  { id: "outage", title: "Risk provider is down", text: "Send ₱2,000 to Alex. (Jev times out)", outcome: "Fails closed → held for review", tone: "warn",
    path: [...HEAD, "risk", "policy", "review"],
    notes: { risk: "Timeout / 429 / 5xx. A required check can't be skipped for money.", policy: "JEV_UNAVAILABLE_FAIL_CLOSED → manual review.", review: "Read-only requests would have continued unaffected." } },
];

export interface NodeInfo { who: string; what: string; fails: string; code: string }
export const NODE_INFO: Record<string, NodeInfo> = {
  user: { who: "You", what: "You type a request in plain English in the web app. The browser never holds API keys and never talks to a model.", fails: "—", code: "apps/web/src/pages/Try.tsx" },
  api: { who: "Rust API (axum)", what: "Gives the request a request_id and trace_id, resolves who is asking into an ActorContext (user, permissions, device) and passes it to the agent. No decisions are made here.", fails: "Malformed JSON → a clean INVALID_REQUEST error with the trace id.", code: "crates/api/src/routes.rs" },
  idem: { who: "Persisted idempotency", what: "If the same Idempotency-Key + same request was already handled, the stored result is returned. A different request under the same key is a conflict. This is what makes lost-response retries safe.", fails: "A failed first attempt releases its claim so you can retry.", code: "crates/persistence/src/idempotency.rs" },
  intent: { who: "Models propose", what: "Groq (strict JSON schema) or the offline rule parser turns words into {intent, recipient, amount}. The output is then grounded: an amount you never wrote is discarded; missing fields are recomputed in code.", fails: "Groq down/invalid → the rule parser takes over.", code: "crates/groq/src/intent.rs" },
  typed: { who: "Policy decides", what: "The draft becomes a typed Rust enum: only ReadBalance, FreezeCard or TransferFunds exist. The recipient name is matched to an id by SQL in your own address book. A model can never name a tool to run.", fails: "Unknown request → UNKNOWN_ACTION.", code: "crates/domain/src/action.rs" },
  hard: { who: "Policy decides", what: "Plain checks of facts: authenticated, permitted, you own the account, recipient exists and isn't blocked, positive amount, enough funds, under the hard limit. They run BEFORE any AI and again inside the execution transaction.", fails: "Any failure → BLOCK. No AI is consulted.", code: "crates/policy/src/hard.rs" },
  blocked: { who: "Outcome", what: "Stopped by a fact. No Jev call, no Groq call, no transfer row. The Decision panel shows exactly which check failed.", fails: "—", code: "crates/policy/src/decide.rs" },
  risk: { who: "Models propose", what: "Only a minimised context is sent (no names, balances or account numbers). Jev answers four narrow questions with probabilities. Confidence is the weakest answer. Risk, not truth, and never permission.", fails: "Timeout/429/5xx → money movement fails closed to manual review.", code: "crates/jev/src/questions.rs" },
  escal: { who: "Models propose", what: "When confidence is low or the request is incomplete, Groq may write one clarification question. It can recommend only CONTINUE, ASK or REVIEW — there is no value meaning “execute”. Any number in its question must come from you or from a SQL query.", fails: "Groq down → a fixed deterministic question, or manual review.", code: "crates/groq/src/escalation.rs" },
  policy: { who: "Policy decides", what: "A short list of tested rules in order: hard block → required provider failed → incomplete → severe signals → low confidence → the tool's default. Output: Execute, RequireConfirmation, AskForClarification, ManualReview or Block, plus reasons.", fails: "It is pure code — it can't be down.", code: "crates/policy/src/decide.rs" },
  ask: { who: "Outcome", what: "You're asked for the missing detail. Your answer doesn't edit the old proposal — it creates a brand-new one with a new fingerprint and runs the whole pipeline again.", fails: "—", code: "crates/agent/src/pipeline.rs (clarify)" },
  review: { who: "Outcome", what: "Held for a human. No funds move. In this MVP it is a parking state; production would add a reviewer queue.", fails: "—", code: "crates/policy/src/decide.rs" },
  confirm: { who: "You decide", what: "A confirmation is minted bound to this exact action's fingerprint: single-use, expires in 5 minutes. If the action changes in any way, the approval is invalid.", fails: "Expired / replayed / tampered → CONFIRMATION_*; nothing runs.", code: "crates/persistence/src/confirmations.rs" },
  exec: { who: "Executors execute", what: "ONE database transaction: lock the action, check idempotency and the confirmation, lock the account row, re-run the hard rules on the fresh balance, write two ledger entries, update state, audit.", fails: "Any error → full rollback. Balance changed → BLOCKED, nothing moves.", code: "crates/persistence/src/executor.rs" },
  ledger: { who: "Database", what: "Append-only, double-entry-inspired: −₱2,000 on your account, +₱2,000 on settlement. Triggers forbid edits and reject any unbalanced transaction at COMMIT; a CHECK keeps balances non-negative.", fails: "Violations fail the commit.", code: "migrations/0001_init.sql" },
  trace: { who: "Audit", what: "Every step wrote an append-only audit event with timings and versions. The Trace Viewer rebuilds the story: what was asked, proposed, checked, assessed, approved, executed — and whether it happened once.", fails: "—", code: "crates/persistence/src/trace.rs" },
};

export const LANES: { id: Lane; label: string; sub: string; x: number; w: number; cls: string }[] = [
  { id: "models", label: "MODELS PROPOSE", sub: "language & risk evidence", x: 20, w: 360, cls: "fill-violet-500/[0.06] stroke-violet-400/30" },
  { id: "policy", label: "POLICY DECIDES", sub: "plain, tested code", x: 410, w: 360, cls: "fill-emerald-500/[0.06] stroke-emerald-400/30" },
  { id: "exec", label: "EXECUTORS EXECUTE", sub: "transactions & the ledger", x: 800, w: 360, cls: "fill-sky-500/[0.06] stroke-sky-400/30" },
];
