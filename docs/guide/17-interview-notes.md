# 17 · Interview notes

## 17.1 The 60-second pitch

> "FinGuard is a control plane for AI financial agents. The LLM only interprets language; a separate Rust policy engine decides, and a transactional executor is the only code that can move money. Jev gives bounded risk assessments as probability distributions; low confidence triggers escalation for clarification, never a second model's approval. Every transfer needs a single-use confirmation bound to the exact action, execution is idempotent and revalidated under row locks, the ledger is append-only and balanced by the database, and every step lands in an audit log that powers a trace viewer."

## 17.2 Five strongest technical talking points

1. **Authority separation: models propose, policy decides, executors execute.** Closed schemas, typed enums, an allow-list registry; no field exists that lets a model waive confirmation or name a tool. *Proof:* injection tests; `RecommendedHandling` has no "execute".
2. **Money safety in one transaction.** Fingerprint-bound single-use confirmations, persisted idempotency, `FOR NO KEY UPDATE` on the contended row, revalidation under lock, deferred balanced-ledger trigger, non-negative CHECK. *Proof:* "two ₱8,000 on ₱10,000 → exactly one"; "8 × ₱3,000 → exactly 3".
3. **Confidence is escalation, not permission.** Weakest-link confidence from distributions; per-risk-class thresholds (documented as uncalibrated); escalation yields a clarification grounded in SQL history; the answer creates a *new* action and re-runs policy.
4. **Fail-closed degradation with typed errors.** Jev down → money goes to manual review, reads continue; Groq down → deterministic parser/fallback question; explicit timeouts, bounded retries, no retries on deterministic errors.
5. **Explainability by construction.** Append-only audit + versioned question sets/policy; the Trace Viewer answers "what did the user ask, what was proposed, what was checked, did Jev run, was it confident, why did policy choose that, did the user confirm, did it execute once, what ledger entry resulted?"

## 17.3 Questions you'll be asked (and good answers)

**Can an LLM move money directly?** No. The only writer is `executor.rs`; it requires a complete typed action, a valid single-use confirmation matching the action's fingerprint, and re-runs hard policy under a row lock.

**Can Jev move money? Can it waive confirmation?** No on both. Its output is evidence in `JevStatus::Assessed(...)`; the base decision for transfers is `RequireConfirmation` from the tool registry, independent of confidence.

**Can Groq override authorisation?** No. Its schema has three values (`CONTINUE_POLICY|ASK_USER|REVIEW`) and even `CONTINUE_POLICY` returns to the base decision. Hard policy ran before it was called.

**Why is low confidence "escalate", not "ask a better model"?** It signals ambiguity in the *situation*. The fixes are information (ask), oversight (review) or stopping. Two uncertain models agreeing isn't evidence, and it adds a new failure source.

**How do you prevent double execution?** Three independent layers: idempotency key + request fingerprint; single-use confirmation + state machine (`EXECUTED` is terminal); the action row lock serialising concurrent attempts.

**What if the DB commits but the HTTP response is lost?** Client retries with the same key; the stored result is returned (`replayed: true`). Without a key the confirmation is already used so a second confirm is rejected and `GET /actions/:id` shows the outcome.

**What if two transfers race?** `SELECT … FOR NO KEY UPDATE` on the customer account row, balance re-read after the lock, CHECK constraint as backstop. (Our first version used `IN (…) ORDER BY id FOR UPDATE` and the concurrency test exposed a real Postgres deadlock — a good story about why you test races.)

**How do you stop prompt injection?** You can't stop a model being *influenced*; you make influence harmless: closed output schemas, grounding to the user's text, deterministic policy, human confirmation for irreversible actions, tools allow-listed.

**What does "grounding" mean here?** Verifying model output against facts it doesn't control: the amount must appear in the user's text; a clarification may only cite numbers from the user's text or the backend's SQL-derived candidate.

**Why PostgreSQL and not event sourcing/Kafka?** One process, one customer, strict consistency needs → a relational DB with transactions is simplest and strongest; the audit table gives an event log without a broker (ADR 11).

**Why Rust?** Exhaustive `match` and newtypes make the policy layer hard to get subtly wrong; the compiler enforces crate boundaries (policy can't call SQL). Downsides: slower iteration, learning curve.

**How would you calibrate thresholds?** Labelled evaluation set, expected-cost minimisation with asymmetric costs, per-segment analysis, shadow-mode rollout, monitoring for drift, pin model versions.

**What's the weakest part of this design?** Be honest: the demo auth; confirmation without step-up auth; the "ledger" is simplified; thresholds uncalibrated; single hot-row contention; manual review is a stub; redaction is pattern-based; no circuit breaker/metrics yet.

**What did you learn building it?** (see below)

## 17.4 Things I'd say I learned (genuinely instructive)

* **Vendor surfaces change; contracts don't.** Jev had two HTTP surfaces (TypeSafe vs JevStation) with different envelopes; Groq's Llama ids were past shutdown while docs conflicted. Config + an adapter that normalises + dated research notes beat remembered APIs.
* **`now()` vs `clock_timestamp()`** — Postgres' `now()` is the *transaction start*; for an audit trail inside one transaction you want wall-clock time.
* **Deferred constraint triggers** make "this ledger transaction must balance" a database guarantee at COMMIT.
* **Measure before choosing thresholds.** My first guess (0.90) would have escalated every normal transfer; a 20-call live study showed healthy requests score ≈0.8 on the weakest-link metric. I also learned a model's hedging depends on question wording (0.83 → 0.98 'yes' after clarifying the criteria).
* **Advisory means advisory.** A rule that auto-blocked on 'extreme' model scores fired on the headline demo with the real model; AI evidence now only holds for review, only hard policy blocks.
* **Lock strength and order matter.** `ORDER BY … FOR UPDATE` doesn't guarantee lock order, and `FOR UPDATE` collides with FK-check locks; `FOR NO KEY UPDATE` on one contended row fixed it.
* **`TRUNCATE` bypasses row-level triggers** — convenient for demo reset, dangerous in production; hence the env guard.
* **Strict structured outputs guarantee shape, not truth** — you still need grounding.
* **Noul answers have no confidence** — so you must decide how to derive one, document it, and not pretend it's the vendor's number.
* **Test the negative**: "nothing moved", "Jev called 0 times".

## 17.5 Things NOT to claim
Production-readiness · a real banking ledger · fraud-detection accuracy · that high confidence means safe · that thresholds are validated.

## 17.6 Whiteboard exercise
Draw the pipeline from memory with: models (left), policy (middle), executor (right); mark every place a check is repeated (hard policy before AI and under lock; balance in Rust, in the lock, in a CHECK); mark the three places state is persisted for audit (audit_events, *_evaluations, ledger). Then answer: "Where would a new action type plug in, and what does the compiler force you to touch?"
