# Failure modes

For each failure: what happens, and whether the system **retries**, **fails closed**, **asks for clarification**, **returns the existing result**, or **requires manual review**.

| Failure | Behaviour | Class |
|---|---|---|
| **Groq unavailable** (timeout / 429 / 5xx) during *intent parsing* | One bounded retry for transient errors; then the deterministic parser handles the text; the failure is recorded in `groq_evaluations` and the audit log. Unparseable text → `UNKNOWN_ACTION`. | retries → falls back |
| **Groq unavailable during escalation** | Policy fails safe: incomplete action → clarification with a **deterministic** question (using only backend history); otherwise → manual review. Never auto-continues. | clarifies / manual review |
| **Groq returns malformed or schema-violating output** | One repair attempt; then typed `InvalidStructuredOutput`; same fallbacks as above. Output containing `EXECUTE_*`-style values fails deserialisation. | retries once → fail safe |
| **Groq rejects strict schema** | Degrades once to JSON mode; server-side validation still applies. | retries (degraded) |
| **Jev unavailable** (timeout / 429 / 5xx / invalid response) | Bounded retry for transient errors. If it still fails: read-only actions continue (Jev not required); **money movement → `MANUAL_REVIEW`** — it never silently bypasses risk policy. Failure recorded in `jev_evaluations` + `jev_failed` event. | fails closed |
| **Jev 401 / 4xx** | Not retried (deterministic failures). Same fail-closed result. | fails closed |
| **Jev rate limit exhausted** (JevStation: 10 req/min and a daily cap per key — observed live) | `RateLimited` is returned immediately when `Retry-After` is long; money movement fails closed to manual review, reads unaffected. Use the mock provider for repeated demos. | fails closed |
| **Jev low confidence** | Not a failure: escalation for extra handling (clarification); the second model never approves. | clarifies |
| **PostgreSQL unavailable** | Requests error with `INTERNAL_ERROR` (no details leaked); `/health` reports `degraded` (503). No partial state: every mutation is one transaction. | fails closed |
| **Response lost after commit** | Client retries with the same `Idempotency-Key`: the stored result is returned (`replayed: true`); nothing executes twice. Without a key, a second confirm gets `CONFIRMATION_INVALID` (single-use) and the client can `GET /actions/:id` to see the outcome. | returns existing result |
| **Duplicate client retry / double click** | Same key → stored result. Different payload under the same key → `IDEMPOTENCY_CONFLICT`. Concurrent duplicates serialise on the action row lock. | returns existing / conflict |
| **Provider timeout** | Explicit per-attempt timeout + total deadline; ≤1 retry; then typed `Timeout`. | retries once → fail safe |
| **Provider rate limit (429)** | Honour `Retry-After` when short (≤3 s) and retry once; otherwise surface `RateLimited` immediately (don't park a user request). Fail-safe as above. | retries / fail safe |
| **Malformed model output (Jev)** | Validation (types, ranges, distribution sums) → `InvalidResponse`; fail closed. | fails closed |
| **Confirmation expired** | `CONFIRMATION_EXPIRED` (410); action marked `EXPIRED` (committed); nothing executed. User must start over. | fails closed |
| **Confirmation replayed / wrong action / tampered payload** | `CONFIRMATION_INVALID`; a tampered action is marked `BLOCKED`. | fails closed |
| **Balance changed between proposal and confirm** | Revalidated inside the transaction under row lock → `BLOCKED` (`INSUFFICIENT_FUNDS`), committed with audit, no funds move. | fails closed |
| **Two transfers race for the same funds** | Second waits on the account row lock, then fails revalidation. | fails closed |
| **Client disconnects mid-request** | The pipeline is not cancelled by the client going away mid-transaction in a way that leaves partial state: DB work is transactional, so it either commits whole or rolls back. A command that was parsed but whose client disconnected leaves a harmless proposal (`AWAITING_*`) that can be fetched or expires; with an idempotency key, retrying returns it. | consistent state |
| **Server crash during a request** | Before commit: transaction rolled back by Postgres; no funds moved; an `IN_PROGRESS` command idempotency claim may remain (the same key then reports "still being processed" — operators can clear stale claims; see limitations). After commit: state is durable; a retry with the key returns the result. The post-commit `transaction_committed` audit marker may be missing, but in-transaction events (`ledger_entries_written`, `execution_completed`) are durable. | durable / rolled back |
| **Unbalanced or tampered ledger write** | Rejected by the deferred balance trigger at COMMIT / append-only triggers. | fails closed |

## Known gaps (documented, not hidden)
* Stale `IN_PROGRESS` idempotency claims after a crash need a janitor (not implemented).
* No circuit breaker or provider health caching; a degraded provider costs its timeout per request.
* Expired actions are marked lazily (on confirm attempt), not by a background sweeper.
