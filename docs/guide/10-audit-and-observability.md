# 10 · Audit, tracing, observability

Three different things that people blur together:

| | Question it answers | Audience | Lifetime | Where |
|---|---|---|---|---|
| **Audit log** | "What exactly happened to this action, and why?" | auditors, support, you in 6 months | forever (append-only) | `audit_events`, `*_evaluations` tables |
| **Tracing/logs** | "What is the system doing right now / what went wrong in this request?" | on-call engineers | days–weeks | `tracing` → stdout |
| **Metrics** | "Is the system healthy in aggregate?" | dashboards/alerts | rolling | *(not implemented; see ch. 15)* |

## 10.1 The audit log

`audit_events(seq BIGSERIAL, event_id, trace_id, action_id NULL, event_type, occurred_at DEFAULT clock_timestamp(), metadata JSONB)`

* **Append-only**: a trigger rejects UPDATE/DELETE. History can't be quietly rewritten by a bug or an operator with app credentials.
* **Totally ordered** by `seq`; **real timestamps** via `clock_timestamp()`.
* **Two ids**: `trace_id` correlates everything that came from one user command, including events *before* the action row exists (request received, intent parsed — `action_id` is NULL); `action_id` correlates everything about one proposal. The Trace Viewer queries `WHERE trace_id = $1 OR action_id = $2`.
* **Safe metadata only.** Each call site passes an explicit small JSON object: counts, codes, durations, versions, decisions. Not raw prompts, not provider payloads, not secrets. User text is stored only as a scrubbed ≤280-char summary on the action.

Event vocabulary (constants in `persistence/src/audit.rs::ev`):

```
request_received → intent_parsing_started → intent_parsed | intent_parse_failed
→ action_created → authorization_passed|failed → hard_policy_passed|blocked
→ jev_skipped | jev_started → jev_completed | jev_failed
→ confidence_escalation_triggered → groq_escalation_started → …_completed|…_failed
→ policy_decision → confirmation_requested | clarification_requested | manual_review_required
→ confirmation_received → execution_started → account_row_locked → balance_revalidated
→ ledger_entries_written → execution_completed | execution_blocked → transaction_committed
(+ confirmation_rejected / confirmation_expired / action_cancelled / idempotent_replay / clarification_answered)
```

### Model audit records
`jev_evaluations`: action, trace, **question-set version**, provider, status (`OK`/`TIMEOUT`/…), **model returned**, **client-measured latency**, questions, answers, normalised assessment, overall confidence, tokens, credits.
`groq_evaluations`: purpose (`INTENT_PARSING`/`ESCALATION_ANALYSIS`), schema version, status, model, latency, **structured result only**, tokens. We store *results*, not prompts: prompts can contain sensitive text and are reproducible from the versioned template + stored facts.

### Versions
`policy_v1`, `risk_questions_v1`, `intent_schema_v1`, `escalation_schema_v1` (`domain/versions.rs`) appear in actions, evaluations, traces. Reproducibility: "what exactly decided this?" has an answer.

## 10.2 The Trace Viewer backend (`persistence/src/trace.rs`)

`GET /api/v1/actions/:id/trace` builds a `TraceView` purely from stored history:

* **Header**: trace id, action id, status, type, started, completed, total duration, policy version, versions.
* **Stages** (the "decision summary" strip): intent → action → hard policy → Jev → Groq → final policy → confirmation → execution, each `passed | warning | blocked | skipped | executed | pending`.
* **Events**: label, wall-clock time, `offset_ms` from the first event, `duration_ms` (from metadata), category (for colour).
* **Latency breakdown**: intent parsing, hard policy, Jev, Groq escalation (from `duration_ms` metadata) and **DB execution = `execution_completed` − `execution_started`** (from DB timestamps).
* **Usage**: Groq tokens, Jev credits/tokens — *as returned by providers*; no prices are invented.
* **Ledger**: entries of the resulting ledger transaction.
* **Answers to the "security value" questions**: what did the user ask, what was proposed, what did hard policy check, was Jev invoked, what did it return, was Groq escalated, why did policy choose this, did the user confirm, did execution happen, only once, what ledger transaction.

The UI (`apps/web/src/components/TraceViewer.tsx`) renders these; clicking an event shows its safe metadata JSON.

## 10.3 Tracing & logs

`tracing` spans nest: `http{request_id, trace_id, method, path}` → `command{trace_id}` → `action{action_id}` (or `confirm{action_id}`). The middleware adds `x-request-id` and `x-trace-id` response headers, and error bodies include `trace_id` so a user report ("trace_id tr_…") leads straight to logs *and* the audit trail.

Logging rules:
* **Never log secrets**: keys live in `Secret<T>`; `Config`'s `Debug` prints the DB URL with the password masked (`postgres://finguard:***@…`).
* **Redacting writer** (`api/src/logging.rs`): every formatted line passes through `redact_known` — known secret values (registered at startup) + patterns (Bearer tokens, `Authorization:` headers, token-like strings, DB URLs).
* **Avoid raw user content**: events carry counts and codes, not text.
* Production switches to **JSON logs** (`APP_ENV=production`) for log pipelines.

⚠️ Redaction by pattern is a safety net, not a licence to log secrets. The primary defence is not putting them in log statements in the first place.

## 10.4 What to add next (honest)

* Metrics: request latency histograms, provider error rates, escalation rate, manual-review rate, idempotent-replay rate; alerts on `jev_failed` spikes.
* OpenTelemetry export so spans from the web tier, API and providers join one distributed trace.
* A janitor for stale `IN_PROGRESS` idempotency claims and for expiring `AWAITING_*` actions.
* Tamper-evidence for the audit table (hash chain or WORM storage) — triggers stop casual edits, not a DBA.

## 10.5 Exercises

1. Add an audit event `risk_context_built` containing only the *names of fields* sent to Jev (not values). Where would you call it? Which test verifies the allow-list?
2. In `trace.rs` add a "queue time" metric (time between `request_received` and `intent_parsing_started`). What would make it non-zero?
3. Write a SQL query listing all actions where Jev was invoked but the final decision was not `REQUIRE_CONFIRMATION` or `MANUAL_REVIEW`.

Next: [11 · Testing strategy](11-testing.md)
