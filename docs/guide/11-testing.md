# 11 · Testing strategy

A good test suite for a system like this answers: *"If someone changes the code, which safety property would silently break?"* We organise tests by the property they protect, not by file.

## 11.1 The pyramid here

```
              ┌─────────────────────────┐
              │  live provider smoke    │  few · slow · optional · need keys (make test-live)
              ├─────────────────────────┤
              │  scenarios (DB + mocks) │  ~30 · seconds · full pipeline against real Postgres
              ├─────────────────────────┤
              │  adapter HTTP tests     │  local scripted server: retries, 429, 5xx, timeout
              ├─────────────────────────┤
              │  unit tests (pure)      │  ~100 · milliseconds · domain, policy, parsers, redaction
              └─────────────────────────┘
```

* **Unit (pure)** — `domain`, `policy`: no I/O, run in microseconds. Every policy branch and hard rule.
* **Adapter tests** — the Jev client and Groq backend against a tiny in-process HTTP server (`tests/common/mod.rs`) so we can script `429 + Retry-After`, `503`, timeouts, garbage, without any vendor.
* **Scenario tests** (`crates/agent/tests/scenarios.rs`) — the full pipeline against a real PostgreSQL. `#[sqlx::test]` creates a throw-away database per test and applies the migrations, so tests are isolated and parallel. Providers are mocks → offline, free, deterministic.
* **Live smoke** (`crates/jev/tests/live.rs`, `#[ignore]`) — a handful of real calls; skip with a clear message without keys; never run on every `cargo test`; never fail CI because secrets are absent.

**Why a real Postgres, not mocks of the DB?** The properties we care about — row locks, constraint triggers, unique indexes, transaction rollback — *are* database behaviour. Mocking them would test the mock.

## 11.2 Property → test map

| Property | Tests |
|---|---|
| Money is exact | `money::tests::*` (parse, overflow, no float drift, serde shape) |
| Illegal state transitions fail | `state::tests::*` |
| Unknown tools rejected; every tool has a spec | `action::tests::*` |
| Fingerprint binds fields | `fingerprint_is_deterministic_and_binds_every_field` |
| Hard policy: insufficient funds, invalid amount, unauthorised, blocked recipient, limit | `policy::hard::tests::*` |
| Confidence below/above threshold; per-class | `policy::decide::tests::*` |
| **Money movement always needs confirmation** | `money_movement_always_requires_confirmation_even_at_99_percent`, `w3_*` |
| Jev unavailable fails closed (money) but not reads | `jev_unavailable_*`, `w8_*` |
| Escalation can't execute / can't skip clarification | `escalation_can_clarify_or_review_but_never_execute`, `model_cannot_talk_policy_out_of_*` |
| Minimisation removes PII | `minimisation_drops_pii_and_balances` |
| Redaction | `redact::tests::*`, `logging::tests::*` |
| Jev parsing (both envelopes, bad shapes) | `jev::parse::tests::*` |
| Jev HTTP behaviour | `jev/tests/http.rs` |
| Groq grounding & schema | `groq::intent::tests::*`, `groq::escalation::tests::*` |
| Workflows 1–8 | `w1_`…`w8_` in `scenarios.rs` |
| Idempotency | `w7_*`, `idempotency_same_key_different_request_conflicts`, `failed_command_releases_its_idempotency_key` |
| Confirmation | single-use, expiry, tamper, binding, cancelled |
| **Concurrency** | `concurrent_transfers_cannot_overspend`, `many_concurrent_*`, `same_action_confirmed_concurrently_executes_once` |
| DB defences | `financial_history_is_append_only_and_balanced` |
| Prompt injection | `injection_may_parse_but_policy_still_requires_confirmation` + `groq::intent` injection test |
| LLM failure fallbacks | `llm_parser_failure_falls_back_*`, `groq_escalation_failures_fail_safe_*` |

## 11.3 How to write the next test

1. **State the property in a sentence** ("a confirmation can't be used on a different action").
2. **Arrange** with the harness: `setup(pool, DemoSetup)` seeds data and wires mocks (`crates/agent/tests/common/mod.rs`).
3. **Act** through the public API (`h.cmd(...)`, `h.confirm(...)`).
4. **Assert on the database** as well as the return value ("balance unchanged", "0 ledger transactions", "Jev called 0 times"). Negative assertions ("nothing moved") are the valuable ones in finance.
5. **Mutation-check**: temporarily break the code and confirm the test fails. A test you've never seen fail is a guess.

## 11.4 Testing concurrency without flakiness

* Create the contended resources *before* spawning (two confirmable actions).
* `tokio::spawn` both confirms; `await` both; assert the **invariant** (exactly one succeeded; balance; ledger consistent), not an order.
* Because the final state is deterministic under correct locking, the test is not flaky when the code is right — and fails (often) when it's wrong. Run it in a loop locally: `for i in $(seq 30); do cargo test -p finguard-agent concurrent_transfers || break; done`.

## 11.5 What isn't tested (yet)

* Real vendor behaviour (needs keys; `make test-live`).
* Load/soak tests; network partitions; Postgres failover.
* Browser/UI tests (the UI is type-checked and built; adding Playwright is exercise 28).
* Property-based tests for `Money` parsing/formatting (add `proptest`; exercise 7).

Next: [12 · The frontend](12-frontend.md)
