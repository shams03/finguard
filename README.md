# FinGuard

**A policy-controlled execution runtime for AI financial agents.**

> **Models propose. Policy decides. Executors execute.**

FinGuard shows how a generative-AI agent can understand a customer's request and *propose* a financial action, while a separate, deterministic control plane — written in Rust, backed by PostgreSQL — decides whether anything happens.

* The **LLM interprets** language (Groq).
* **Jev contributes bounded risk assessments** (probability distributions + confidence), never authority.
* **Rust policy remains authoritative**: hard invariants, thresholds, final decision.
* **Sensitive actions require confirmation** bound to the exact action, single-use, expiring.
* **Execution is idempotent and transactional**: row locks, revalidation, append-only ledger.
* **Every action is traceable** from an append-only audit log, with a Trace Viewer UI.

This is a portfolio sandbox with **synthetic data**. It is *not* a bank, not a production payments system, and makes no claims about fraud-detection accuracy. See [Known limitations](#known-limitations).

```
 USER ─► WEB (React) ─► RUST API ─► Intent parser (deterministic | Groq)
                                        │ typed ProposedAction (allow-listed tools only)
                                        ▼
                      Authorization ─► Hard policy ──blocked──► BLOCK  (0 AI calls)
                                        │ pass
                                        ▼
                    Risk context (minimised) ─► Jev (4 atomic questions)
                                        │
                      confidence ≥ threshold?──no──► Groq escalation / clarification
                                        ▼                      (advisory; user answers; NEW proposal)
                              Final policy (deterministic)
              ┌──────────┬──────────────┼───────────────┬─────────────┐
           EXECUTE   REQUIRE_CONFIRM  ASK_CLARIFY   MANUAL_REVIEW    BLOCK
          (read-only)       │
                      user confirms ─► Idempotent executor ─► ONE Postgres transaction
                                         lock → revalidate → ledger → action state → audit
```

## Quick start (no cloud account, no API keys needed)

Requirements: Rust (stable), Docker, Node 20+.

```bash
git clone <repo> && cd finguard
cp .env.example .env            # defaults run fully offline with mock AI providers
docker compose up -d --wait     # PostgreSQL 18 on :5432
cargo run -p finguard-api       # applies migrations, seeds demo data, serves :8080
# in another terminal
cd apps/web && npm install && npm run dev     # http://localhost:5173
```

Or with the Makefile: `make dev-db`, `make backend`, `make frontend`, `make test`.

Open <http://localhost:5173> → **Dashboard** → click **Normal Transfer**, then open the **Trace Viewer**.

### Using the real providers

Edit `.env` (never commit it):

```env
JEVSTATION_API_KEY=...          # JevStation (default) — or point JEVSTATION_BASE_URL at https://api.typesafe.ai/v1/systemone
DECISION_PROVIDER=jev
GROQ_API_KEY=...
INTENT_PARSER=groq
ESCALATION_ANALYZER=groq
```

All model calls originate from the Rust backend. The browser only ever talks to the backend.

## Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `APP_ENV` | `development` / `production` (production disables demo endpoints, JSON logs) | `development` |
| `BIND_ADDR`, `CORS_ORIGIN` | HTTP listener, allowed browser origin | `127.0.0.1:8080`, `http://localhost:5173` |
| `DATABASE_URL` | PostgreSQL URL (local Docker or Supabase) | local Docker |
| `DATABASE_MIGRATION_URL` | Optional *direct* URL used only for migrations | = `DATABASE_URL` |
| `DATABASE_MAX_CONNECTIONS` | Pool size | `10` |
| `DATABASE_STATEMENT_CACHE` | `0` only for Supabase *transaction* pooler | unset |
| `JEVSTATION_API_KEY`, `JEVSTATION_BASE_URL`, `JEV_MODEL`, `JEV_TIMEOUT_MS` | Jev | key empty |
| `JEV_DEFAULT_CONFIDENCE_THRESHOLD` + `JEV_THRESHOLD_{READ_ONLY,REVERSIBLE,MONEY_MOVEMENT,HIGH_IMPACT}` | confidence thresholds (demo values) | 0.85 / 0.70 / 0.80 / 0.90 / 0.95 |
| `GROQ_API_KEY`, `GROQ_BASE_URL`, `GROQ_MODEL`, `GROQ_TIMEOUT_MS`, `GROQ_STRICT_SCHEMA` | Groq | `openai/gpt-oss-20b` |
| `INTENT_PARSER` | `deterministic` \| `groq` | `deterministic` |
| `DECISION_PROVIDER` | `mock` \| `jev` | `mock` |
| `ESCALATION_ANALYZER` | `mock` \| `groq` | `mock` |
| `RUST_LOG` | log filter | `info` |

Secrets are wrapped in a `Secret<T>` type (Debug prints `[REDACTED]`), never sent to the browser, and scrubbed from logs by a redacting writer.

## Why these choices

* **Why Rust?** The part that must be right — money types, state machine, policy — gets exhaustive `match`, newtypes and no null surprises. A new `ProposedAction` variant won't compile until it has a risk class, a policy and an executor.
* **Why Jev?** For *bounded* judgements (is this unusual? does this match the request?) it returns probability distributions with a confidence value, which suits threshold-based escalation far better than a free-text "looks fine" from a chatbot. It stays advisory.
* **Why Groq?** Fast, cheap/free inference with strict JSON-schema output on `openai/gpt-oss-20b`. It is used only for language: parsing and writing a clarification question. The brief preferred a Llama model; the current Llama ids are past their announced shutdown date and lack strict structured output, so a supported model was chosen (see [`docs/research/integration-research.md`](docs/research/integration-research.md)). Change it with `GROQ_MODEL`.
* **Why PostgreSQL?** ACID transactions, `SELECT … FOR UPDATE`, CHECK constraints, deferred constraint triggers. Invariants are enforced twice: in Rust and again by the database.
* **No Kafka / Kubernetes / Redis / vector DBs.** One binary + Postgres; the audit table *is* the event log. See `docs/adr/`.

## How confidence escalation works

Each risk class has a threshold (`ReadOnly 0.70`, `Reversible 0.80`, `MoneyMovement 0.90`, `HighImpact 0.95`). Overall confidence is the **weakest link** of the answers Jev returned. Below the threshold the system **escalates for extra handling** — a Groq clarification analysis — it does **not** let the second model decide. The user answers, a **new** action is created, and policy runs again from scratch. Even at 99% confidence every transfer still needs explicit confirmation.

> These thresholds are demo parameters. In production they must be calibrated on representative historical evaluation data with asymmetric error costs.

## Demo scenarios

| Say | Result |
|---|---|
| "What's my available balance?" | Executed deterministically — 0 Jev calls, 0 Groq calls |
| "Freeze my card." | Needs confirmation; freezing twice is idempotent |
| "Send ₱2,000 to Alex." | Jev low risk → confirmation → one DB transaction + ledger entries |
| "Send Alex the usual amount." | Low confidence → Groq clarification ("You usually send Alex ₱5,000…") → new proposal |
| "Send ₱75,000 to this new account." (suspicious scenario) | Manual review — nothing moves |
| "Send ₱40,000 to Alex." (₱20,000 balance) | Hard block `INSUFFICIENT_FUNDS`, no AI called |
| Confirm → "Simulate lost response" | Same `Idempotency-Key` returns the stored result; no second transfer |
| Jev timeout / 429 / 5xx (Dashboard buttons) | Money movement fails closed to manual review; reads continue |

Full 90-second script: [`docs/DEMO.md`](docs/DEMO.md).

## Documentation

* **[`docs/GUIDE.md`](docs/GUIDE.md)** — the learning guide: from zero to one, then one to a hundred.
* [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) · [`docs/SECURITY.md`](docs/SECURITY.md) · [`docs/FAILURE_MODES.md`](docs/FAILURE_MODES.md) · [`docs/DEMO.md`](docs/DEMO.md)
* [`docs/adr/`](docs/adr/) — architecture decision records · [`docs/research/`](docs/research/) — integration research (dated)

## Tests

```bash
make dev-db && make test       # unit + DB integration tests (mock providers, offline)
make lint                      # rustfmt + clippy -D warnings
make test-live                 # OPTIONAL live provider smoke tests; skip cleanly without keys
cd apps/web && npm run build   # typecheck + production build
```

Highlights: money & state machine (domain); every hard rule and policy branch; idempotency (same key → one execution, different payload → conflict); confirmation (exact action, expiry, replay, tampered payload); **concurrency** (₱10,000 balance + two ₱8,000 transfers → exactly one succeeds); Jev failure matrix (timeout/429/5xx/invalid); Groq failure matrix (incl. invalid structured output); prompt-injection; append-only/balanced-ledger database triggers; redaction.

## Deployment database (Supabase)

Business logic never touches Supabase APIs; Supabase is just hosted Postgres.

1. Create a project; copy the **direct** connection string (or the **session pooler** string if your host is IPv4-only).
2. Set `DATABASE_URL=postgresql://postgres:<pw>@db.<ref>.supabase.co:5432/postgres?sslmode=require`.
3. Optionally set `DATABASE_MIGRATION_URL` to the direct string and `DATABASE_URL` to the session pooler.
4. Keep `DATABASE_MAX_CONNECTIONS` small (5–20). Only if you must use the *transaction* pooler (port 6543) set `DATABASE_STATEMENT_CACHE=0`.
5. Set `APP_ENV=production` (demo/reset endpoints are disabled; seed data is not auto-created).

No code change is required. Details and caveats: [`docs/research/integration-research.md`](docs/research/integration-research.md#3-database).

## Known limitations

* **Not production software.** Single synthetic customer; no real payments, cards or KYC. Authentication is a fixed demo session behind an `ActorContext`; production would use a real identity provider.
* The ledger is *double-entry-inspired*, not a real banking ledger (no chart of accounts, FX, holds/pending states, reconciliation, reversals).
* Confidence thresholds are uncalibrated demo values; Jev scores are "model assessments", **not fraud probabilities**; no accuracy claims are made.
* Manual review is a parking state; there is no analyst workflow.
* Provider contracts were verified from documentation on 2026-10-01; live behaviour should be re-checked with `make test-live`.
* Groq's free tier is rate limited (see research notes); the deterministic parser keeps simple commands working if Groq is down.

## Repository layout

```
crates/domain       ids, Money, actions, state machine, risk types, provider traits, redaction (pure)
crates/policy       hard rules, thresholds, final decision (pure)
crates/persistence  sqlx: executor (the only code that moves money), audit, idempotency, confirmations, trace
crates/jev          Jev adapter: questions, parser, HTTP client, mock
crates/groq         Groq adapter: strict-schema client, intent parser, escalation analyzer, mock
crates/agent        orchestrator pipeline + deterministic parser + JSON views
crates/api          axum HTTP layer, config, logging redaction
apps/web            React + TypeScript + Vite + Tailwind
migrations/         SQL schema (plain PostgreSQL)
docs/               guide, architecture, security, failure modes, demo, ADRs, research
```
