# 90-second demo script

Setup (before you start): `make dev-db && make backend` in one terminal, `make frontend` in another. Open <http://localhost:5173>. For the full effect use real providers (`DECISION_PROVIDER=jev`, `INTENT_PARSER=groq`, `ESCALATION_ANALYZER=groq`); the demo also works fully offline with mocks.

| Time | Say | Do | Point out |
|---|---|---|---|
| 0–10 s | "AI models propose actions. FinGuard controls execution — models propose, policy decides, executors execute." | Dashboard | Synthetic customer, trusted device, normal range, the 6 quick demos |
| 10–25 s | "A read-only request needs no AI at all." | **Check Balance** | Status `EXECUTED`; Decision Explorer: *Jev assessment — skipped, deterministic code is enough*; 0 AI calls |
| 25–45 s | "A normal transfer: the model proposes, Jev assesses, I confirm." | **Normal Transfer** → read *Proposed action* → *Jev assessment* (bars, confidence vs threshold) → **Confirm** | Language became a **typed action** before anything ran; probability distributions, "model assessment", not "fraud probability"; REQUIRE_CONFIRMATION even at high confidence; balance −₱2,000 |
| (optional) | "Response lost? Retry safely." | **Simulate lost response** | "Existing result returned" — no second transfer |
| 45–60 s | "Ambiguity is escalated, not guessed." | **Ambiguous Transfer** | *Confidence escalation triggered*; Groq clarification "You usually send Alex ₱5,000…" (amount from SQL history, not the model); click **Yes** → a **new** action, policy re-runs |
| 60–75 s | "Suspicious: technically possible, so hard rules pass — policy still stops it." | **Suspicious Transfer** | Hard policy PASS; anomaly high; reasons list; **ACTION HELD FOR REVIEW — No funds moved.** |
| (optional) | "Hard rules run before AI." | **Insufficient Funds** | `FAIL Sufficient funds`, "AI assessment skipped", Jev calls 0 |
| (optional) | "Provider down?" | Dashboard → **Jev: timeout** → Normal Transfer | Fails closed to manual review; balance reads still work. Reset with **Jev: none** |
| 75–90 s | "Everything is reconstructable." | **Open trace →** | Timeline with latencies, decision summary strip, latency breakdown (Groq vs policy vs DB), ledger entries (−₱2,000 / +₱2,000), "Did it happen only once? Yes" |

Reset between runs with **Reset demo** (or `make reset-demo`).

## Talking points for questions
* *Can an LLM move money?* No. Executor is the only writer; it needs a complete typed action, a single-use confirmation bound to its fingerprint, and re-validates under row locks.
* *What if Jev is wrong?* It can only add caution; it can never remove the confirmation or hard checks.
* *Why not let the stronger model decide on low confidence?* Low confidence means "unsure what is being asked" — we ask the human.
* *What about duplicates and races?* Persisted idempotency + row locks + CHECK constraint; tests prove "two ₱8,000 on ₱10,000 → one succeeds".
