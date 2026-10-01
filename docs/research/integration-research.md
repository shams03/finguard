# Integration research (Phase 0)

**Research date:** 2026-10-01. Raw notes with exact JSON and URLs live next to this file:
[`raw-jev.md`](raw-jev.md), [`raw-groq.md`](raw-groq.md), [`raw-stack.md`](raw-stack.md).
No secrets are recorded anywhere in this repository.

> Several details below were gathered from page summaries, not from live API calls
> (we had no provider keys during research). Anything marked **UNVERIFIED** must be
> re-checked on first live call; the live integration tests (`make test-live`) exist for that.

## 1. Jev

### Who is who
* **Jev** is the model, made by **TypeSafe AI**. Official docs: <https://docs.typesafe.ai>. Official API: `POST https://api.typesafe.ai/v1/systemone`, `Authorization: Bearer <key>`.
* **JevStation** (<https://jevstation.com>) is an **independent third-party** workspace/reseller that fronts the same model. It exposes `POST https://jevstation.com/api/v1/systemone` with a different response envelope, credit billing, and its own MCP server. Its own About page says it is *not* TypeSafe.

### Chosen integration
Direct HTTP (`reqwest` + `serde`) from the Rust backend. **No official Rust SDK exists** (official SDKs: Python, JS/TS; docs say "call the HTTP API directly from any language").

Because the brief provisions `JEVSTATION_API_KEY`, the default target is JevStation, but the adapter (`crates/jev`) is written to accept **both** response shapes:

| | JevStation | TypeSafe official |
|---|---|---|
| URL | `https://jevstation.com/api/v1/systemone` | `https://api.typesafe.ai/v1/systemone` |
| Envelope | `{code, message, data:{...}}` | bare `{model, answers, usage}` |
| Extra fields | `data.credits.{charged,remaining}`, `data.latency_ms` | none (only `usage.input_tokens/output_tokens`) |
| `model` request field | ignored (pinned) | required, use a versioned id e.g. `jev-1.13.0` |
| Question cap | 12 questions, 24 000 chars of state | 64k-token context (32k for state + longest question) |
| Errors | 400 401 402 404 413 429 502 503 | 401 422 429 529 |

Switch with `JEVSTATION_BASE_URL`. The adapter detects the envelope by the presence of `data`.

### Verified request / response contract
* Request: `{"state": <string|object|array>, "model": "...", "questions": {"<id>": {...}}}`.
* Question types: `noul` (`instructions`, optional `criteria{true,false}`), `choice` (`criteria` = option→description map), `score` (`criteria` = ordered array, 2–10 levels).
* Answers: `noul` → `{type, noul: 0..1}` (**no confidence field**); `choice` → `{type, choice, probabilities, confidence}`; `score` → `{type, score, legend, probabilities, confidence}` (`score` is probability-weighted and can be fractional).
* `confidence` is derived from the probability distribution (Choice formula in docs: `clamp((N·peak − 1)/(N − 1), 0, 1)`; Score formula **UNVERIFIED**).
* Rate limiting: HTTP 429 with `Retry-After`; retry 429/5xx/529 with exponential backoff. Official SDK defaults: 10 s timeout/attempt, 2 retries, backoff 0.5 s→5 s.
* Streaming: none. Latency: not in the official response; JevStation returns `latency_ms`. We **always measure latency client-side** and prefer our measurement.

### Observed live behaviour (2026-10-01)
See [`calibration.md`](calibration.md). Highlights: JevStation limits are 10 requests/minute and a daily cap per key; a batched 4-question call takes ≈1.4–4.2 s and costs 1 credit; the response envelope and `credits`/`latency_ms` fields match the documentation; Noul answers carry no confidence (derived by us).

### MCP (development aid only — not a runtime dependency)
No official TypeSafe MCP server was found. JevStation's server:

```bash
claude mcp add --transport http jevstation https://jevstation.com/api/mcp \
  --header "Authorization: Bearer $JEVSTATION_API_KEY"
```

Tools: `jev_decide`, `jev_run_question_set`, `jev_list_question_sets`, `jev_route_model`, `jev_guard_tool_call` (returns execute/confirm/block). Useful for exploring question wording from Claude Code. Runtime code never uses it. Exact output shape of `jev_guard_tool_call` is **UNVERIFIED**.

### Decision
* Use `Noul` for yes/no semantic checks (intent alignment, context sufficiency) and `Score` for ordered rubrics (behavioural anomaly, recipient risk). All four are batched in **one** request (cheaper and faster than separate calls — official fan-out guidance).
* Because `Noul` carries no confidence, our `overall_confidence` is computed in Rust from the probability distributions we *do* have (see `docs/GUIDE.md` §confidence) — Jev's number is never treated as an authorization signal.

## 2. Groq

* Base URL `https://api.groq.com/openai/v1`, `Authorization: Bearer $GROQ_API_KEY`, `POST /chat/completions` (OpenAI compatible). Responses API exists; **not used** (chat completions is enough and best documented).
* `usage` returns `prompt_tokens`, `completion_tokens`, `total_tokens`, plus `queue_time`, `prompt_time`, `completion_time`, `total_time`.
* **Strict structured outputs** (`response_format: {type:"json_schema", json_schema:{name, strict:true, schema}}`) are supported **only** on `openai/gpt-oss-20b`, `openai/gpt-oss-120b`, `qwen/qwen3.8-27b`. Llama ids only get JSON-object mode. Strict mode requires every property in `required` and `additionalProperties:false`.
* Llama status: `llama-3.1-8b-instant` and `llama-3.3-70b-versatile` appear on the deprecations page with a shutdown date of 2026-08-16 (already past) while the models page still lists them as production — the docs conflict, so we treat them as **deprecated**.
* Free-tier limits for GPT-OSS 20B/120B: 30 RPM, 1 000 RPD, 8 K TPM, 200 K TPD (so keep prompts small). 429 returns `retry-after` + `x-ratelimit-*` headers.
* Errors: 400/401/403/404/413/422/424/429/498/499/500/502/503; body `{"error":{"message","type"}}`.

### Chosen model: `GROQ_MODEL=openai/gpt-oss-20b`
Why (this deviates from "prefer Llama", as the brief explicitly allows):
1. Current production model, not deprecated.
2. Supports **strict** JSON-schema output, so the model *cannot* return fields outside the schema.
3. Free tier, and Groq names it the successor of `llama-3.1-8b-instant`.
Fallback `openai/gpt-oss-120b` (same strict support). Server-side validation + one repair retry is still applied (defence in depth), and a JSON-mode path exists for non-strict models if someone sets `GROQ_MODEL` to a Llama id (`GROQ_STRICT_SCHEMA=false`).

## 3. Database

* **Local:** Docker `postgres:18-alpine`. PG 18 images store data under a versioned `PGDATA`, so the volume is mounted at `/var/lib/postgresql` (not `.../data`).
* **Production:** Supabase Postgres via the same SQL. Guidance:

| Option | Port | IPv | Prepared stmts | Use for |
|---|---|---|---|---|
| Direct | 5432 | IPv6 (IPv4 via paid add-on) | yes | long-running server, **migrations** |
| Session pooler (Supavisor) | 5432 | IPv4 | yes | long-running server on IPv4-only hosts |
| Transaction pooler | 6543 | IPv4 | **no** | serverless only |

* Recommendation: a long-running Rust server uses **direct** or **session pooler**; migrations use **direct**. `sslmode=require` and SQLx `tls-rustls`.
* If someone *does* point at the transaction pooler: disable SQLx's statement cache (`statement_cache_capacity = 0`; exact 0.9 API **UNVERIFIED**, we expose `DATABASE_STATEMENT_CACHE=0`). Note that `SELECT … FOR UPDATE` transactions work through the transaction pooler, but we still advise session/direct.
* Env vars added: `DATABASE_URL`, `DATABASE_MIGRATION_URL` (optional; falls back to `DATABASE_URL`), `DATABASE_MAX_CONNECTIONS` (default 10), `DATABASE_STATEMENT_CACHE` (optional).
* Pool sizing: small per-instance pool (5–20); keep total under Supabase's limit.
* No Supabase APIs/SDKs are used anywhere in business logic.

## 4. Rust & frontend stack (crates.io / npm on 2026-10-01)

| Item | Version | Notes |
|---|---|---|
| Rust stable | 1.98.1 | edition 2024 |
| axum | 0.8 | routes use `/{id}` syntax |
| tokio | 1 | |
| sqlx | 0.9 | features `runtime-tokio tls-rustls postgres uuid time migrate macros`; queries need `&'static str` (use `sqlx::query` with literals) |
| reqwest | 0.13 | `default-features=false`, features `rustls json` |
| serde / serde_json | 1 | |
| tower / tower-http | 0.5 / 0.7 | `trace cors request-id` |
| tracing / tracing-subscriber | 0.1 / 0.3 | `env-filter json` |
| uuid, thiserror, time, sha2 | 1 / 2 / 0.3 / 0.11 | |
| async-trait | 0.1 | needed for `Arc<dyn Trait>` |
| React / Vite / Tailwind | 19 / 8 / 4 | Tailwind v4 via `@tailwindcss/vite`, no config file |

Where we pin different versions because an API turned out to differ, the actual `Cargo.toml` is authoritative.

## 5. Architectural decisions that came out of research
1. Direct HTTP to Jev; dual-envelope adapter; client-side latency measurement.
2. One batched Jev request per assessment with 4 atomic questions; app-level `risk_questions_v1`.
3. `openai/gpt-oss-20b` + strict schema for Groq; JSON-mode fallback with strict server-side validation.
4. Docker PG 18 locally; Supabase direct/session pooler in prod; separate migration URL.
5. `#[async_trait]` + `Arc<dyn …>` for provider traits so tests can inject mocks.
