# 02 · One request, end to end

We'll follow **"Send ₱2,000 to Alex."** from the browser to the ledger, and then follow two variants (ambiguous, and insufficient funds). Open the named files as you read; each step says what's *allowed to decide*.

```mermaid
sequenceDiagram
    autonumber
    participant UI as Web UI
    participant API as api (axum)
    participant AG as agent pipeline
    participant PAR as IntentParser
    participant HP as policy::hard
    participant J as Jev (DecisionProvider)
    participant POL as policy::decide
    participant DB as PostgreSQL
    UI->>API: POST /api/v1/agent/commands {text} + Idempotency-Key
    API->>AG: handle_command(actor, text, key, trace)
    AG->>DB: claim idempotency key (IN_PROGRESS)
    AG->>PAR: parse(text)
    PAR-->>AG: ParsedIntent{TRANSFER, "alex", 200000}
    AG->>DB: resolve "alex" → recipient id (deterministic SQL)
    AG->>DB: INSERT action (status PROPOSED → VALIDATING)
    AG->>HP: evaluate(actor, draft, facts)
    HP-->>AG: PASS (checks listed)
    AG->>J: assess(minimised RiskContext)
    J-->>AG: RiskAssessment{distributions, confidence 0.93}
    AG->>POL: stage_one(...)
    POL-->>AG: RequireConfirmation
    AG->>DB: status → AWAITING_CONFIRMATION, create confirmation(fingerprint)
    AG-->>UI: proposal + confirmation_id   (NOTHING has moved)
    UI->>API: POST /actions/:id/confirm {confirmation_id} + Idempotency-Key
    API->>DB: ONE transaction: lock → revalidate → ledger → EXECUTED → audit
    API-->>UI: executed (or stored result if it's a retry)
```

## Step 1 — The browser sends text

`apps/web/src/api.ts` → `POST /api/v1/agent/commands` with `{"text": "Send ₱2,000 to Alex."}` and an `Idempotency-Key` header (a fresh UUID). The UI never sees API keys, and never talks to a model.

🔧 `crates/api/src/routes.rs::post_command` — this handler is deliberately thin: parse the request, call the agent, shape the response. **No decisions here.**

Every request gets a `request_id` and a `trace_id` in `request_context` (a middleware); both go on the tracing span and into response headers.

## Step 2 — Who is asking? (`ActorContext`)

`routes.rs::actor` loads the demo session into an `ActorContext { user_id, session_id, permissions, device_id }`. In a real system this comes from a verified login token. *Everything downstream takes an `ActorContext`*, so swapping in real auth later doesn't change the policy code.

## Step 3 — Idempotency claim

`agent/pipeline.rs::command_inner`: if the client sent a key, we try to insert `(user, "command", key, fingerprint)` as `IN_PROGRESS`. If the key already exists with the *same* fingerprint we return the stored action; with a *different* fingerprint → `IDEMPOTENCY_CONFLICT`. (Details in chapter 09.)

## Step 4 — Language → structured intent (models propose)

`IntentParser::parse` returns a `ParsedIntent`:

```json
{ "intent_type": "TRANSFER_FUNDS", "recipient_reference": "alex",
  "amount_minor": 200000, "currency": "PHP",
  "missing_fields": [], "ambiguity": false, "refers_to_usual_amount": false }
```

* Default parser: `agent/deterministic.rs` (rules; offline; free).
* With `INTENT_PARSER=groq`: `groq/intent.rs` calls the LLM with a **strict JSON schema** and then **grounds** the result — if the model returns an amount that isn't literally in the user's text, it is thrown away.

**Who decides here?** Nobody. This step only *interprets*.

## Step 5 — Structured intent → typed draft (allow-list)

`pipeline.rs::build_draft`:

* The recipient string `"alex"` becomes a `RecipientId` via `facts::resolve_recipient` — a SQL lookup in *this user's* address book. A model's string never becomes an id except through this function.
* The account is the actor's primary account, resolved server-side — the model never names an account.
* The result is an `ActionDraft::TransferFunds { from_account, recipient_id, amount }` — a **Rust enum**, not "tool name + JSON".

Unsupported text ("tell me a joke") → `UNKNOWN_ACTION`. A model can't conjure a new tool: the only tools are the three variants of `ActionKind` (see `domain/action.rs`, the `REGISTRY` of `ToolSpec`).

## Step 6 — The action row exists (state machine begins)

`actions::create` inserts a row with status `PROPOSED`, a **fingerprint** (SHA-256 over actor, kind, account, recipient, amount, currency) and a scrubbed copy of the text. An audit event `action_created` is written. Status goes `PROPOSED → VALIDATING` — checked by `ActionStatus::transition`.

## Step 7 — Hard policy (deterministic; before any AI)

`policy/hard.rs::evaluate` runs named checks and returns a report you can show in the UI:

```
PASS Authenticated actor      PASS Permission            PASS Account ownership
PASS Account active           PASS Recipient valid       PASS Recipient not blocked
PASS Positive amount          PASS Currency supported    PASS Sufficient funds
PASS Transfer under hard limit
```

If any check fails → status `BLOCKED`, policy decision `BLOCK`, **no Jev, no Groq call**, and the UI shows *"AI assessment skipped"*. (Variant B below.)

## Step 8 — Risk assessment (Jev advises)

Because transfers are `requires_jev` in the tool registry:

1. `facts::load_risk_facts` loads everything we know (including the customer's name and balance).
2. `RiskContext::from_facts` **minimises** it to an allow-list: scrubbed request, amount, ratio to usual maximum (`0.25×`), recipient age (240 days), device trusted, location familiar, velocity… *No names, no account numbers, no balance.*
3. `JevDecisionProvider::assess` sends **four atomic questions** in one request (`jev/questions.rs`): intent alignment, behavioural anomaly, recipient risk, context sufficiency.
4. The reply is normalised into a `RiskAssessment` (`jev/parse.rs`) with distributions and `overall_confidence` (the weakest link).

The result is stored in `jev_evaluations` and audited (`jev_started`, `jev_completed` with `duration_ms`).

## Step 9 — Final policy (deterministic)

`policy/decide.rs::stage_one` goes down a list of small rules — hard failure? Jev unavailable? incomplete action? severe signals? low confidence? — and otherwise returns the tool's base decision. For a transfer that is **`RequireConfirmation`**, *always*, even at 99% confidence.

Reasons are recorded (`CONFIRMATION_REQUIRED`, `CONFIDENCE_SUFFICIENT`) and rendered in the UI's "Final policy decision" card.

## Step 10 — A confirmation is minted

Status → `AWAITING_CONFIRMATION`; `confirmations::create` stores `(confirmation_id, action_id, user, action_fingerprint, expires_at = now + 5 min, used_at = NULL)`. The API responds with the typed proposal, e.g.

```json
{ "status": "AWAITING_CONFIRMATION", "policy_decision": "REQUIRE_CONFIRMATION",
  "proposed_action": { "type": "TRANSFER_FUNDS", "recipient": "Alex Rivera", "amount": "PHP 2000.00", ... },
  "confirmation": { "confirmation_id": "cnf_…", "expires_at": "…" } }
```

**No money has moved.** Balance is still ₱120,000. The UI shows an immutable summary and Confirm/Cancel.

## Step 11 — The user confirms

`POST /actions/:id/confirm` with the `confirmation_id` and a new `Idempotency-Key`. This calls `persistence/executor.rs::confirm_and_execute` — the heart of the system. In **one transaction**:

1. lock the action row (`FOR UPDATE`);
2. look up the idempotency key (a retry returns the stored result);
3. check the confirmation: right id, right user, unused, not expired;
4. **recompute the fingerprint** from the stored action and compare with the confirmation's;
5. mark the confirmation used; `AWAITING_CONFIRMATION → APPROVED → EXECUTING`;
6. lock the customer account row (`SELECT … FOR NO KEY UPDATE`);
7. **re-run hard policy** against the locked, fresh balance;
8. insert a ledger transaction with two entries (−₱2,000 customer, +₱2,000 settlement), update balances, add a history row;
9. `EXECUTING → EXECUTED`; write audit events; store the idempotency record;
10. `COMMIT`.

If step 7 fails (balance dropped in the meantime), the action becomes `BLOCKED` and nothing moves.

## Step 12 — The trace

`GET /actions/:id/trace` rebuilds the story from the append-only `audit_events` plus the Jev/Groq records and the ledger. The Trace Viewer draws a timeline: *request received → intent parsed → typed action created → authorization passed → hard policy passed → Jev started/completed (233 ms, confidence 93%) → policy decision → confirmation requested/received → execution started → account row locked → invariants revalidated → ledger entries written → executed → committed.*

---

## Variant A — "Send Alex the usual amount."

* Parser: `amount = null`, `missing_fields = ["amount"]`, `refers_to_usual_amount = true`. It did **not** invent a number.
* Draft is incomplete → can't be executed, can't get a fingerprint or confirmation.
* Hard policy: runs what it can; amount-dependent checks are **SKIPPED** (not silently passed).
* Jev: only the two questions that make sense are asked (intent alignment, context sufficiency). Context sufficiency comes back low → confidence ≈ 63% < 90% threshold.
* Policy: `Stage::Escalate(MissingInformation / LowDecisionConfidence)`.
* The backend (SQL!) computes a candidate: median of the last 5 transfers to Alex = ₱5,000, and hands it to the escalation analyzer as the **only number it may mention**.
* Groq writes: *"You usually send Alex ₱5,000. Would you like to send ₱5,000 this time?"* — validated: every number must be the user's own or the backend's candidate.
* Policy: `AskForClarification`. Status `AWAITING_CLARIFICATION`.
* The user clicks **Yes** → `POST /actions/:id/clarify`. The old action becomes `CANCELLED` ("superseded") and a **new** action is created with amount ₱5,000, fresh fingerprint, `parent_action_id` set. **The full pipeline runs again** → `AWAITING_CONFIRMATION`. The user still confirms.

The second model never approved anything; it wrote a question.

## Variant B — Insufficient funds (balance ₱20,000, request ₱40,000)

* Parse and draft as usual.
* Hard policy: `FAIL Sufficient funds` → `HARD_POLICY_BLOCK / INSUFFICIENT_FUNDS`.
* Status `BLOCKED`. Jev calls: **0**. Groq calls: **0**. No transfer row is created. The audit trail has `hard_policy_blocked` and `jev_skipped`.

**Lesson:** cheap, certain checks go first; AI is consulted only when it can change the outcome.

---

Next: [03 · Glossary](03-glossary.md)
