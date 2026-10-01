# 15 · From sandbox to production (the honest 1 → 100)

FinGuard is a *teaching-grade vertical slice*. This chapter lists what a real deployment needs, in the order you'd likely tackle it, and why each item matters. None of this is implemented; do not claim it is.

## Stage 1 — Make it correct under real conditions
* **Real authentication & sessions.** `ActorContext` already isolates the policy from identity; implement OIDC/JWT verification, device binding, token rotation. Add CSRF protection if using cookies.
* **Step-up authentication for money movement** (WebAuthn/passkeys or OTP) bound to the *confirmation*, so the human approval is cryptographically tied to the action fingerprint ("sign what you see").
* **Idempotency janitor** for stale `IN_PROGRESS` claims; **expiry sweeper** for `AWAITING_*` actions.
* **Least-privilege DB roles**, migrations under an owner role, no TRUNCATE/DDL for the app role.
* **Backups + point-in-time recovery**; restore drills.
* **Rate limiting & abuse controls** per user/device/IP; max pending actions; velocity limits as *hard* policy (not only as model context).

## Stage 2 — A real ledger
* Chart of accounts; customer, settlement, fee, suspense accounts.
* **Pending/holds/authorisations** vs settled; **reversals** as new entries; **reconciliation** against the external rail/bank statements; end-of-day balance proofs.
* Immutable, verifiable audit (hash-chained or WORM storage).
* Multi-currency/FX with currency exponents, rounding policies.
* Idempotent integration with an external payment rail (outbox pattern: write intent + ledger in one transaction, a worker sends to the rail, retries safely).

## Stage 3 — Operating AI responsibly
* **Calibrate thresholds** with labelled history: build an evaluation set, measure precision/recall of "review" decisions, assign costs (false-allow ≫ false-review) and pick thresholds that minimise expected cost; re-calibrate when models/versions change. Pin the Jev model version (`jev-1.13.0`) rather than `jev-latest` once tuned.
* **Shadow mode** for new policy versions: compute `policy_v2` decisions beside `policy_v1` and compare before switching. (We store enough to replay.)
* **Drift monitoring**: distribution of confidences, escalation rate, manual-review rate; alert on shifts.
* **Red-teaming**: a growing corpus of injection/jailbreak prompts as regression tests (we have a few).
* **Vendor risk**: DPAs, data residency, retention, zero-data-retention options, fallbacks (second vendor), SLAs and circuit breakers.
* **Evaluate the LLM parser** against a labelled intent set; track exact-match rates; keep the deterministic parser as a fallback.

## Stage 4 — Operability
* Metrics (RED: rate, errors, duration) per route and per provider; SLOs; alerts.
* OpenTelemetry tracing across web → API → providers.
* Runbooks for each failure mode in `FAILURE_MODES.md`; chaos tests (kill Postgres mid-confirm; add latency to Jev).
* Feature flags and staged rollout for policy changes.
* A human **review queue** for `MANUAL_REVIEW` with SLAs, approver identity, four-eyes for large amounts, and its own audit trail (today it is just a parking state).

## Stage 5 — Scale
* Today: one process, one primary Postgres; the hot row is the account. Transfers touching the *same* account serialise on its row lock, so per-account throughput is bounded (different accounts proceed in parallel). Real systems shard by account, batch postings, or move to specialised ledger databases (e.g. TigerBeetle) once measured volume demands it. Know the trade-off: simplicity and correctness first, scale when measured.
* Horizontal scale of the API is easy (stateless); the only shared state is Postgres.
* Event streaming (Kafka etc.) *only* when there are multiple independent consumers that cannot be served by the outbox + audit table. ADR 11 explains why we didn't start there.

## Stage 6 — Compliance & governance
KYC/AML, sanctions screening, transaction monitoring, SAR workflows, regulatory reporting, data-protection impact assessments, model-risk-management documentation, change control for policy and model versions. These are organisational as much as technical.

## What *not* to do when productionising
* Don't "just raise the threshold" because there are too many reviews — measure first.
* Don't let a model write the SQL, choose the account, or pick the tool.
* Don't weaken fail-closed to improve availability numbers without product sign-off.
* Don't claim fraud-detection accuracy without measured evidence on your data.

Next: [16 · Exercises](16-exercises.md)
