# 07 · Policy

`crates/policy` is pure functions: facts in, decision out. No clock, no network, no database. That is what makes it testable, replayable and reviewable.

## 7.1 Hard policy (`hard.rs`)

*Hard* = facts and invariants a model has no say in. It runs **before any AI** and **again inside the execution transaction**.

Checks (each is a named line the UI shows as PASS / FAIL / SKIP):

| Check | Fails with |
|---|---|
| Authenticated actor | `NOT_AUTHENTICATED` |
| Action allow-listed | (by construction) |
| Permission | `MISSING_PERMISSION` |
| Account / card ownership | `ACCOUNT_NOT_OWNED` / `CARD_NOT_OWNED` |
| Account active | `ACCOUNT_INACTIVE` |
| Recipient valid / not blocked | `RECIPIENT_NOT_FOUND` / `RECIPIENT_BLOCKED` |
| Positive amount | `INVALID_AMOUNT` |
| Currency supported | `UNSUPPORTED_CURRENCY` |
| Sufficient funds | `INSUFFICIENT_FUNDS` |
| Under absolute limit (₱500,000) | `TRANSFER_LIMIT_EXCEEDED` |
| Card state valid (freeze is idempotent) | — |

Result: `HardPolicyResult::Pass | Block(reason)` — the **first** failing check decides the reason. Checks that cannot be evaluated yet (amount unknown) are **`Skipped`, never silently passed**: a skipped check on an incomplete draft is fine because an incomplete draft can't execute; at execution time the full set must pass.

🧠 **Facts are loaded outside, rules are evaluated inside.** `evaluate(actor, draft, &HardFacts, &HardLimits)` receives already-loaded facts. In the proposal phase the agent loads them with plain reads; in the executor they're loaded *under row lock*. Same rules, different freshness. That's how "revalidate at execution time" costs zero duplicated logic.

💥 *Without execution-time revalidation:* the classic TOCTOU bug (time-of-check/time-of-use): balance checked at proposal, spent elsewhere, then the transfer overdraws. Test: `balance_is_revalidated_at_execution_not_trusted_from_earlier`.

## 7.2 Thresholds (`thresholds.rs`)

```
ReadOnly 0.70   Reversible 0.80   MoneyMovement 0.90   HighImpact 0.95   (default 0.85)
```
Configurable by env. **Demo values, not validated.** In production they must be calibrated on representative historical evaluation data with *asymmetric error costs*: wrongly allowing a fraudulent transfer is much more costly than wrongly escalating a legitimate one, so the threshold sits where expected cost is minimal, not at 50%.

## 7.3 The final decision (`decide.rs`)

Decomposed into small rule functions evaluated in order — *not* a giant `if`:

```rust
let rules = [rule_hard, rule_provider_failure, rule_incomplete, rule_severe, rule_confidence];
```
Each returns `Option<Stage>`; the first `Some` wins; otherwise `base_decision(tool)`.

| Order | Rule | Outcome |
|---|---|---|
| 1 | hard policy failed | `Block(HardPolicy(reason))` |
| 2 | Jev required but failed (any `DecisionProviderError`) | `ManualReview` — **fail closed** |
| 3 | draft incomplete | escalate (`MissingInformation`) → clarification |
| 4 | severe signals | `ManualReview`; `Block(SevereRisk)` if *both* Jev scores ≥ 3.5/4 |
| 5 | overall confidence < threshold, or `context_sufficient` < 0.5, or `intent_alignment` < 0.5 | escalate |
| — | otherwise | confirmation (transfers, freezes) or execute (read-only) |

**Severe signals** (rule 4) combine model evidence and model-free evidence:
* Jev: anomaly score ≥ 3 or recipient-risk score ≥ 3.
* *Deterministic*: recipient is new (< 30 days) **and** amount ≥ 5× the user's usual maximum **and** (device untrusted **or** location unfamiliar). So even if Jev were down or wrong, the "₱75,000 to a brand-new payee from a new device in a new country" case is still held. (Test: `deterministic_signals_hold_a_transfer_even_if_jev_says_fine`.)

**Caution is one-directional.** Model evidence can raise caution (review/block/escalate) but can never remove a requirement: the confirmation rule for transfers is in the tool registry, not in the assessment.

### After escalation: `finalize_after_escalation`

Inputs: the stage's reasons, the (optional) `EscalationAnalysis`, and a deterministic fallback question.

* Analysis says clarify / `ASK_USER` → `AskForClarification{question}` (its own question if valid; else the fallback).
* `REVIEW` → `ManualReview`.
* `CONTINUE_POLICY` → the base decision (a transfer is *still* `RequireConfirmation`).
* **Incomplete action ⇒ always clarify**, whatever the model recommends (`model_cannot_talk_policy_out_of_clarifying_an_incomplete_action`).
* Analyzer failed → incomplete ⇒ clarify with the fallback; complete ⇒ `ManualReview`.

### Why escalate instead of "ask a smarter model"?

Because *low confidence* is information about **the situation** (ambiguous, missing context, conflicting signals), not about which model is better. The right responses are: gather information (ask the user), involve a human (review), or stop. A second model is just another opinion with its own errors, and "two uncertain models agree" is not evidence. This repo's rule: *a second model may help phrase a question; it never approves.*

## 7.4 Reasons are first-class

`PolicyOutcome { decision, reasons: Vec<PolicyReason{code, detail}>, escalation }`. Reasons are stored (`actions.decision_reasons`), audited, and shown ("recipient is new", "amount significantly exceeds the typical transfer"). A decision you can't explain is a liability.

## 7.5 Versioning

`POLICY_VERSION = "policy_v1"` is written to each action and trace. If you change a rule, bump it; then "what would policy_v2 have done to last month's actions?" becomes a replay job, because `HardFacts` and assessments are stored.

## 7.6 Tests to read

`crates/policy/src/hard.rs` (tests): normal pass, insufficient funds, invalid amounts, exact balance, absolute limit, unauthorised account, no actor / no permission, blocked/unknown recipient, incomplete draft skips, frozen card idempotent.
`crates/policy/src/decide.rs` (tests): read-only executes w/o Jev; freeze needs confirmation; **transfer needs confirmation even at 99%**; hard block wins regardless of a glowing assessment; below/above threshold; per-class thresholds; Jev unavailable (fails closed for money, not for reads); high anomaly → review with reasons; deterministic signals; extreme → block; known vs new recipient; missing amount escalates; escalation can clarify/review but never execute; provider failure fails safe.

## 7.7 Mutation exercises

| Change | Test that should fail |
|---|---|
| Make `base_decision` return `Execute` for transfers when confidence is high | `money_movement_always_requires_confirmation_even_at_99_percent` |
| Treat `JevStatus::Unavailable` as "no signal" | `jev_unavailable_on_money_movement_fails_closed` |
| Let `ContinuePolicy` skip the missing-field clarification | `model_cannot_talk_policy_out_of_clarifying_an_incomplete_action` |
| Swap rule order so confidence runs before hard | `hard_block_wins_and_never_needs_ai` |

Next: [08 · The AI layers](08-ai-layers.md)
