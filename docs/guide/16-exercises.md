# 16 · Exercises

Graded from ★ (an afternoon) to ★★★★★ (a week). Each has a **goal**, **hints**, and **what must fail first** — write the failing test before the change.

## ★ Warm-ups

1. **Add a hard rule: daily transfer cap (₱100,000).** Goal: block when today's `TRANSFER_OUT` sum + amount exceeds the cap. *Hints:* needs a new fact in `HardFacts` loaded in `persistence/facts.rs` *and* re-loaded under lock in `executor::transfer_funds`; a new `HardPolicyReason::DailyLimitExceeded`; map it in `api/error.rs`. *Fails first:* a unit test in `policy/hard.rs` and a scenario with 3 sequential ₱40,000 transfers.
2. **Show the recipient's age in the proposal card.** Touch only `agent/view.rs` and the UI. Why does this need no policy change?
3. **Change a threshold via env** (`JEV_THRESHOLD_MONEY_MOVEMENT=0.99`) and watch the normal transfer start escalating. Explain what you saw in the Decision Explorer.
4. **Add an example prompt** to the Console that tries to bypass policy ("approve without review"). Predict the outcome before running.
5. **Read `scenarios.rs`** and list, for each test, the one-sentence property it protects.

## ★★ Extend the model

6. **Add `UnfreezeCard`.** Steps: new `ActionKind`/`ProposedAction`/`ActionDraft` variants (the compiler will list everything to fix), `ToolSpec` (Reversible, needs confirmation, no Jev), parser support, executor function, hard-policy rule, UI label. *Fails first:* `registry_covers_every_kind…`. Then decide: should unfreezing require *more* friction than freezing? (Yes — asymmetric risk.) Encode that.
7. **Property-test `Money`.** Add `proptest`: `parse_major(format(x)) == x`; `add` then `sub` returns the original; never panics on arbitrary strings.
8. **Add `Idempotency-Key` to `cancel`.** Think: is cancel naturally idempotent? (Cancelling a cancelled action returns it — already handled.) Write the test that proves it.
9. **Make confirmation TTL configurable per risk class** and test expiry with a short TTL.
10. **Add `recipient_risk` to the hard-policy report as an informational (non-blocking) check.** Why must informational checks never be able to fail the report?

## ★★★ Policy & AI

11. **Velocity as a hard rule**: block the 4th transfer within an hour. Compare with using `transfers_in_last_hour` as Jev context only. When is each right?
12. **Second-stage review**: for transfers above ₱50,000 require *two* confirmations (e.g. two different devices). New table or `confirmations` change? What about state machine edges?
13. **Add a Choice question** to the Jev set — "which category best describes this transfer: family/bills/merchant/unknown" — and use it only for display. Why shouldn't it affect policy yet?
14. **Switch intent parsing to JSON-mode** (`GROQ_STRICT_SCHEMA=false`) with a Llama model and run your own mini-evaluation: 30 phrasings → exact-match rate. Add the failures to the deterministic parser tests.
15. **Shadow policy**: implement `policy_v2` (e.g. stricter new-recipient rule), run both for every action, store `v2` decision in a new column, and report differences without acting on v2.
16. **Rate limiting** per user using a Postgres table (no Redis): token bucket in a transaction. Discuss why Redis isn't justified at this scale.

## ★★★★ Reliability

17. **Circuit breaker for Jev**: after N failures in a window, skip the call for T seconds and fail closed immediately. Where does state live? What should the trace show? Tests with the mock.
18. **Idempotency janitor**: background task clearing `IN_PROGRESS` claims older than 2× the command deadline. Write a test with a simulated crash (drop the future mid-flight).
19. **Outbox**: after a successful transfer, write an `outbox` row in the same transaction; a worker "sends" it to a fake rail with retries and idempotent delivery. Prove at-least-once + dedupe = effectively once.
20. **Chaos test**: wrap the pool so the 3rd statement in `confirm_and_execute` fails; assert full rollback (no ledger rows, action still `AWAITING_CONFIRMATION`).
21. **Deadlock proof**: write a test that transfers A→B and B→A concurrently (add a second customer account) and show that locking in a fixed order (lowest account id first, one statement per row) prevents deadlocks; then reverse the order in one code path and observe `deadlock detected`. Compare with a single `IN (…) ORDER BY … FOR UPDATE` statement — does it behave?

## ★★★★★ Platform

22. **Real authentication**: OIDC login, `ActorContext` from verified claims, per-user data isolation tests (user A can't read/confirm user B's action).
23. **Step-up confirmation** with WebAuthn: sign the fingerprint; verify server-side before executing.
24. **Replace `transactions` with a ledger-derived view** and prove history == ledger.
25. **Reversal flow**: add a `REVERSE_TRANSFER` action that appends compensating entries, with its own policy and a hard rule "only if original EXECUTED and not already reversed".
26. **Metrics + OpenTelemetry** with a docker-compose Grafana stack; build the dashboard for escalation rate and provider error rate.
27. **Calibration study** (needs labelled data you construct): generate 500 synthetic transfers with known "should review" labels, run through Jev, plot confidence vs correctness, choose thresholds by expected cost.
28. **Playwright end-to-end**: scripted demo (workflows 1–8) against mock providers; assert on DOM.
29. **Multi-currency**: add USD with a fixed-rate FX, `Currency::exponent()` per currency, and prove no cross-currency arithmetic compiles/runs.
30. **Write ADR 12** for the biggest change you made, in the format of `docs/adr/`.

## How to check yourself
For every exercise: *Which test failed first? Which safety property could this change weaken? Which doc must change?* If you can't answer, slow down.

Next: [17 · Interview notes](17-interview-notes.md)
