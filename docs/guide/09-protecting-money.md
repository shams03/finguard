# 09 · Protecting money

This chapter is the core of "how does it not lose money?". Read it slowly; it's the part interviewers probe.

## 9.1 Confirmation: approval bound to an exact action

Table `confirmations(id, action_id UNIQUE, user_id, action_fingerprint, created_at, expires_at, used_at)`.

Rules (all enforced in `executor.rs`, all tested):

1. **Bound to one action.** Using confirmation A on action B → `CONFIRMATION_INVALID` (`confirmation_binds_to_its_own_action`).
2. **Bound to the exact parameters.** The fingerprint stored when the user was shown the proposal must equal the fingerprint recomputed *now* from the stored action. If the amount changed in between → the action is marked `BLOCKED` and nothing executes (`payload_changed_after_confirmation_is_rejected`).
3. **Single use.** `used_at` is set in the same transaction; a second attempt gets `CONFIRMATION_INVALID` (`confirmation_is_single_use_replay_is_rejected`).
4. **Expires.** 5 minutes (configurable). An expired confirmation marks the action `EXPIRED` and *commits that*, then returns `CONFIRMATION_EXPIRED` (410) (`expired_confirmation_cannot_execute`).
5. **No global `confirmed = true`.** There is nowhere to put such a flag.

❓ *Why not a signed token (JWT) instead of a DB row?* Single-use + revocation + "is this action still the same?" all need server state anyway; a row is simpler and auditable.

⚠️ Production: add step-up authentication for money movement (passkey/OTP) so a hijacked session can't just click Confirm.

## 9.2 Idempotency: retries are normal, duplicates are bugs

🧠 The network can lose the *response* to a request that succeeded. The client can't tell "it failed" from "it worked but I didn't hear". So clients retry — and the server must make retries safe.

Table `idempotency_records(user_id, scope, idempotency_key, request_fingerprint, action_id, status IN_PROGRESS|COMPLETED, response)`, `UNIQUE(user_id, scope, key)`.

Semantics:

| Situation | Result |
|---|---|
| New key | proceed; store the result |
| Same key + same fingerprint | return the **stored result**, `replayed: true`, nothing executes |
| Same key + different fingerprint | `IDEMPOTENCY_CONFLICT` |
| Same key, first request still running | `IDEMPOTENCY_CONFLICT` ("still being processed") |
| First attempt *failed* | the claim is released so the client can retry (`failed_command_releases_its_idempotency_key`) |

The **fingerprint** (SHA-256 over canonical fields) is what defines "the same request": for commands, actor + normalised text; for confirms, actor + action id + action fingerprint + confirmation id. Not "hash the JSON body" — key order and whitespace would produce false conflicts.

Two scopes, two patterns:
* **Commands** use *claim-then-complete*: insert an `IN_PROGRESS` row first (the UNIQUE constraint decides races), do the work (which includes slow network calls), then mark `COMPLETED`.
* **Confirms** insert the record *inside the execution transaction*, after locking the action row: either money and record commit together or neither does. No "money moved but key not recorded" window.

Independently of keys, the state machine and single-use confirmation stop a double execution even if a client sends no key.

## 9.3 The execution transaction (`persistence/src/executor.rs`)

Walk through `confirm_and_execute` and `transfer_funds` line by line. (The numbered list in chapter 02 step 11 is the summary; here are the *why*s.)

```
BEGIN
 1. SELECT * FROM actions WHERE id=$1 FOR UPDATE
```
Locks the action row. Concurrent confirms for the *same* action queue up here; the second one will see `EXECUTED` and be rejected/replayed. (`same_action_confirmed_concurrently_executes_once`: 5 concurrent → exactly 1 execution.)

```
 2. idempotency lookup → replay or conflict
 3. status must be AWAITING_CONFIRMATION
 4. SELECT * FROM confirmations WHERE action_id=$1 FOR UPDATE; checks (id, user, unused, unexpired)
 5. fingerprint recomputed == confirmation fingerprint == stored fingerprint
 6. UPDATE confirmations SET used_at=…; status → APPROVED → EXECUTING; audit events
 7. SELECT id FROM accounts WHERE kind='SETTLEMENT' …   (no lock)
 8. SELECT … FROM accounts WHERE id = $customer FOR NO KEY UPDATE
```
Step 8 is **the** lock. Any other transaction that wants the same customer row waits until this one commits or rolls back. The settlement balance is later changed with an atomic `UPDATE … SET balance_minor = balance_minor + $1`, so every transaction takes its row locks in the same order (customer, then settlement).

> **Lesson learned the hard way (the concurrency test caught it).** The first version locked both account rows with `WHERE id IN (…) ORDER BY id FOR UPDATE` believing `ORDER BY` fixed the lock order. Under 8 concurrent transfers Postgres reported `deadlock detected`. Two fixes mattered: (1) don't rely on `ORDER BY … FOR UPDATE` for lock order — Postgres doesn't guarantee it; take locks in separate statements in a fixed order or reduce to one contended row; (2) use the **weakest lock that is correct**: we never change the account's key, so `FOR NO KEY UPDATE` is right, and it doesn't conflict with the `FOR KEY SHARE` locks that foreign-key checks take when child rows (ledger entries, history) reference the account. `FOR UPDATE` is the strongest lock and collided with those. After the change, 20 consecutive runs of the concurrency tests showed zero deadlocks.

```
 9. recipient SELECT … FOR SHARE (can't be deleted/blocked mid-transfer)
10. hard policy re-evaluated with the LOCKED balance
```
This is the "revalidate" step. After the lock, the balance we read can't change until we finish. If `balance < amount` now → status `BLOCKED`, audit `execution_blocked`, **commit** (we want that record), no ledger writes.

```
11. INSERT INTO ledger_transactions …
12. INSERT INTO ledger_entries (customer, −amount), (settlement, +amount)
13. UPDATE accounts SET balance_minor = balance_minor + delta (both)
14. INSERT INTO transactions (user-facing history row)
15. status EXECUTING → EXECUTED; actions.result / ledger_transaction_id; audit events
16. INSERT INTO idempotency_records (if a key was sent)
COMMIT
17. (best-effort) audit 'transaction_committed'
```
If *anything* errors before `COMMIT`, the whole transaction rolls back (sqlx rolls back on drop). The `CHECK (balance_minor >= 0)` on customer accounts and the deferred "ledger balances" trigger mean that even a Rust bug in steps 10–13 can't leave the database in an invalid financial state — the COMMIT fails.

**Timestamps:** audit rows default to `clock_timestamp()` (actual wall time) not `now()` (transaction start). Otherwise every event inside one transaction would share a timestamp and the Trace Viewer couldn't show "account locked → balance revalidated → ledger written" in order with real offsets.

### Concurrency, concretely
Balance ₱10,000; two ₱8,000 confirms at the same time (`concurrent_transfers_cannot_overspend`):

```
T1: lock accounts ───────────────┐ revalidate: 10,000 ≥ 8,000 ✔ → write ledger → COMMIT
T2:      (waits on the row lock) └──────────────► lock acquired; read balance 2,000 → revalidate ✘ → BLOCKED
```
Outcome: exactly one `EXECUTED`, one `BLOCKED(INSUFFICIENT_FUNDS)`, balance ₱2,000. A second test fires 8 concurrent ₱3,000 transfers at ₱10,000 and asserts exactly 3 succeed.

❓ *Why `FOR UPDATE` and not SERIALIZABLE isolation?* SERIALIZABLE would also be correct but makes transactions fail with serialization errors that the application must catch and retry. `FOR UPDATE` expresses the actual intent ("I own this balance row until I commit") and is easier to reason about for a single hot row. Alternatives: optimistic version column (`UPDATE … WHERE version=$v`), or `UPDATE accounts SET balance = balance - $x WHERE id=$1 AND balance >= $x` as an atomic conditional update (also correct; we keep the explicit lock because we also want to run policy rules under it).

⚠️ Pooler note: row locks + transactions work through Supavisor *session* and *direct* modes; transaction-mode pooling also works for explicit transactions, but disables prepared statements (see chapter 13).

## 9.4 The ledger

Two tables: `ledger_transactions` (a movement) and `ledger_entries` (signed amounts per account). For a transfer: `customer −200000`, `settlement +200000`.

* **Append-only:** `BEFORE UPDATE OR DELETE` triggers raise an exception (also on `audit_events`, `jev_evaluations`, `groq_evaluations`). Corrections would be *new reversing entries* — not implemented in this MVP, but the schema doesn't block it.
* **Balanced:** a `CONSTRAINT TRIGGER … DEFERRABLE INITIALLY DEFERRED` sums entries per ledger transaction **at COMMIT** and rejects a non-zero sum. (Deferred is essential: within the transaction the entries are inserted one by one, so it's temporarily unbalanced.)
* **Materialised balance:** `accounts.balance_minor` is updated in the same transaction. The opening balance is itself a ledger transaction, so `balance == SUM(entries)` always holds (`reads::ledger_matches_balances`, asserted in tests).
* **Non-negative:** `CHECK (kind='SETTLEMENT' OR (owner IS NOT NULL AND balance_minor >= 0))`.

⚠️ *Honest limits:* not a real banking ledger — no chart of accounts, no pending/holds, no FX, no reconciliation with an external rail, no reversal flow, history table (`transactions`) is display data not derived from the ledger, and TRUNCATE (used by the demo reset) bypasses row triggers — which is why demo endpoints are disabled in production.

## 9.5 Why a clarification creates a *new* action

Imagine mutating the old proposal: "amount = ₱5,000", then letting the previous approvals carry over. Problems: stale confirmations, an audit trail that says one thing was proposed but another executed, and policy results (Jev's assessment!) that were computed for a *different* request. So the rule: **if the content of a proposal changes, it is a different proposal.** `clarify()`:

1. cancel the old action (`AWAITING_CLARIFICATION → CANCELLED`, audited "superseded");
2. create a new `ActionDraft` with the explicit amount, new `action_id`, new `trace_id`, `parent_action_id` = old;
3. run the full pipeline again (hard policy → Jev → policy → confirmation).

Test `w5_low_confidence_escalates_then_new_proposal` asserts different ids, parent link, old = `CANCELLED`, new = `AWAITING_CONFIRMATION`, and that the old one can't be clarified again.

## 9.6 Mutation exercises (run these!)

| Delete / change | Test that should fail |
|---|---|
| Remove `FOR UPDATE` from the account lock | `concurrent_transfers_cannot_overspend` (flaky-fail — run it 20×) — and the DB `CHECK` will still save you from a negative balance! Observe which defence triggers. |
| Skip the fingerprint comparison | `payload_changed_after_confirmation_is_rejected` |
| Don't mark the confirmation used | `confirmation_is_single_use_replay_is_rejected` |
| Do the idempotency lookup *after* the state gate | `w7_duplicate_retry_returns_existing_result` |
| Remove the ledger balanced trigger | `financial_history_is_append_only_and_balanced` |
| Use `now()` for audit timestamps | trace offsets collapse to 0 (visual) — write a test! |

Next: [10 · Audit, tracing, observability](10-audit-and-observability.md)
