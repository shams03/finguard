# 03 · Glossary

Plain-English definitions, each with where it shows up in this repo.

## AI / LLM terms

**LLM (large language model)** — a model that predicts text. Great at language, unreliable at guarantees. Here: Groq's `openai/gpt-oss-20b`.

**Prompt / system prompt** — instructions sent to an LLM. Our system prompts tell the model its limited role and to treat user text as *data*. (`groq/src/intent.rs::SYSTEM`)

**Prompt injection** — text that tries to override the model's instructions ("ignore previous rules"). You cannot fully prevent a model from being *influenced*; you design so influence doesn't matter (closed schemas, deterministic policy).

**Hallucination** — a confident, fabricated output (an invented amount). Mitigated by *grounding*: verify model output against facts you control.

**Structured outputs / JSON schema / strict mode** — asking the provider to return JSON matching a schema. *Strict* means the provider constrains decoding so the output must match (every property required, `additionalProperties: false`). Even then we validate again on our side.

**Temperature** — randomness knob; we use 0 for repeatability.

**Reasoning model / `reasoning_effort`** — some models "think" before answering; we ask for low effort to keep latency and tokens down.

**Token** — roughly a word-piece; providers bill and limit by tokens.

**Jev** — a model by TypeSafe AI that answers *bounded* questions (Noul / Choice / Score) with probability distributions and confidence. **JevStation** is a separate third-party service that resells/wraps access. **Noul** = a yes/no question answered as a probability. **Choice** = pick one of N options. **Score** = an ordered rubric (0 normal … 4 extreme).

**Probability distribution** — probabilities over options that sum to 1, e.g. `{Normal: 81%, Elevated: 14%, High: 4%, Extreme: 1%}`.

**Confidence** — how peaked/certain the distribution is. High confidence ≠ correct, and ≠ permission.

**Calibration** — whether "90% confident" really means right 90% of the time. Our thresholds are *uncalibrated demo values*.

**Escalation** — extra handling when the system is unsure. Here: ask a clarification question; never "let the other model decide".

**Fail-open vs fail-closed** — what the system does when a safety component is unavailable. Fail-open = proceed (dangerous). Fail-closed = hold/deny. We fail closed for money.

## Money & ledger terms

**Minor units** — the smallest unit of a currency (centavos). ₱2,000.00 = `200000`. Stored as integers, because floats can't represent 0.1 exactly.

**Ledger** — an append-only list of money movements; the source of truth.

**Double-entry** — every movement has equal and opposite entries (−₱2,000 customer, +₱2,000 settlement) so the total is always zero. Catches bugs where money appears from nowhere.

**Append-only** — rows are never updated or deleted; corrections are new rows (a *reversal*). Enforced by triggers.

**Materialised balance** — a stored balance column kept in sync with the ledger so reads are fast. We assert `balance == SUM(entries)` in tests.

**Settlement account** — the demo counter-party for transfers.

## Databases & concurrency

**Transaction (ACID)** — a group of statements that commit together or not at all: *Atomic, Consistent, Isolated, Durable*.

**Row lock / `SELECT … FOR UPDATE` / `FOR NO KEY UPDATE`** — "I'm about to change this row; make others wait." The mechanism that stops two transfers spending the same balance. `FOR NO KEY UPDATE` is the weaker variant for when you don't change the row's key; it doesn't conflict with foreign-key checks.

**Race condition** — correctness depends on timing between concurrent actors.

**Deadlock** — two transactions each waiting for the other's lock. Avoided by always taking locks in the same order, or by locking only one contended row. (Our first lock statement, `IN (…) ORDER BY id FOR UPDATE`, still deadlocked — see chapter 09.)

**Isolation level** — how much concurrent transactions can see of each other. We use PostgreSQL's default (READ COMMITTED) plus explicit row locks, which is simpler to reason about than SERIALIZABLE retries.

**Constraint (CHECK / UNIQUE / FK)** — rules enforced by the database itself; the last line of defence against bugs in app code.

**Deferred constraint trigger** — a check that runs at COMMIT, used to ensure each ledger transaction balances.

**Migration** — a versioned SQL script that evolves the schema (`migrations/`).

**Connection pool / pooler (Supavisor)** — reuses DB connections. Supabase's *transaction* pooler can't do prepared statements; direct and session modes can.

## Reliability

**Idempotency** — doing something twice has the same effect as once. Implemented with a client-supplied `Idempotency-Key` + a stored request fingerprint.

**Request fingerprint** — a cryptographic hash (SHA-256) of the fields that define "the same request".

**Replay attack** — resending a captured valid request.

**Confirmation (single-use, bound)** — approval tied to one action and its exact parameters; can't be reused or applied to a changed action.

**State machine** — an explicit set of states and legal transitions, instead of a pile of booleans.

**Timeout / retry / backoff / jitter** — never wait forever; retry only transient errors, with growing delays.

**Circuit breaker** — stop calling a failing dependency for a while (not implemented; listed as future work).

## Observability & security

**Audit log** — an append-only record of what happened and why.

**Trace / span / `trace_id`** — a correlation id that ties together everything for one request; spans nest.

**Redaction** — removing secrets/PII from logs and vendor payloads.

**Data minimisation** — sending vendors only what they need.

**Threat model** — a structured list of "who could attack what, how, and what stops them".

**Least privilege** — each component has only the permissions it needs.

**Defence in depth** — several independent layers; one failing isn't fatal (e.g. balance is checked in Rust, again under lock, and again by a CHECK constraint).

## Rust terms (more in chapter 04)

**Newtype** — a tiny wrapper type (`struct AccountId(Uuid)`) so different ids can't be mixed up.

**Enum / `match`** — a type that is *one of* several variants; `match` must handle all of them.

**`Result<T, E>` / `?`** — explicit success-or-error; `?` returns early on error.

**Trait** — an interface (`DecisionProvider`).

**`async`/`await`, Tokio** — cooperative concurrency for I/O.

**Ownership / borrowing** — Rust's rules that make memory-safety and data races compile-time errors.

**Crate / workspace** — a package / a set of packages built together.
