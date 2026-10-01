# 13 · Configuration and deployment

## 13.1 Twelve-factor configuration

All configuration comes from environment variables (`crates/api/src/config.rs`). `.env` is a **development convenience** loaded by `dotenvy`; production sets real env vars. `.env` is git-ignored; only `.env.example` (no secrets) is committed.

`Config::from_env` **fails fast** with a clear message: unknown enum values, thresholds outside 0..1, or `DECISION_PROVIDER=jev` without `JEVSTATION_API_KEY`. Better to refuse to start than to silently run on mocks in production.

Three switches pick real vs mock behaviour independently:

| Var | Values | Notes |
|---|---|---|
| `INTENT_PARSER` | `deterministic` \| `groq` | Groq failure falls back to deterministic |
| `DECISION_PROVIDER` | `mock` \| `jev` | mock supports failure injection endpoints |
| `ESCALATION_ANALYZER` | `mock` \| `groq` | |

## 13.2 Local development: Docker Postgres

`docker-compose.yml` → `postgres:18-alpine`, user/db `finguard`, volume mounted at `/var/lib/postgresql` (PG 18 images use a versioned `PGDATA` underneath), healthcheck so `docker compose up --wait` blocks until ready.

Local development **never** depends on Supabase or any cloud account.

## 13.3 Migrations

`migrations/0001_init.sql`, embedded with `sqlx::migrate!`. `cargo run -p finguard-api -- migrate` applies them and exits; serving also applies pending migrations (idempotent), and seeds demo data on first run when `APP_ENV != production`.

Plain SQL only — **no Supabase-specific features** — so the same file runs everywhere (ADR 4).

## 13.4 Production: Supabase as "just Postgres"

What the research found (2026-10-01; see `docs/research/integration-research.md`):

| Connection | Port | IP | Prepared statements | Use |
|---|---|---|---|---|
| Direct | 5432 | IPv6 (IPv4 via add-on) | yes | long-running server, migrations |
| Session pooler (Supavisor) | 5432 | IPv4 | yes | long-running server on IPv4-only hosts |
| Transaction pooler | 6543 | IPv4 | **no** | serverless / short-lived functions |

For this server (long-lived, uses explicit transactions & row locks):

```
DATABASE_URL=postgresql://postgres.<ref>:<password>@<host>:5432/postgres?sslmode=require   # direct or session pooler
DATABASE_MIGRATION_URL=postgresql://postgres:<password>@db.<ref>.supabase.co:5432/postgres?sslmode=require   # optional
DATABASE_MAX_CONNECTIONS=10
APP_ENV=production
```
* TLS: `sslmode=require` and the `tls-rustls` sqlx feature.
* Pool size: keep small; total connections across instances must stay under the project's limit.
* Transaction pooler only if you must: `DATABASE_STATEMENT_CACHE=0` disables sqlx's prepared-statement cache (exposed through `PgConnectOptions::statement_cache_capacity`). Verify with a smoke test — the exact API name is flagged UNVERIFIED in the research notes for sqlx 0.9.
* Create a **least-privilege role** for the API (no DDL/TRUNCATE) in production, and run migrations with a separate owner role via `DATABASE_MIGRATION_URL`.

No code change is required to switch; only `DATABASE_URL`.

## 13.5 Deploying the API

A single static-ish binary (`cargo build --release -p finguard-api`). Typical container: multi-stage Dockerfile (builder with `rust:slim`, runtime `debian:stable-slim` with CA certificates). Run behind a TLS-terminating proxy. Health check: `GET /health` (200 `ok` / 503 `degraded` when the DB is unreachable). Graceful shutdown on SIGINT.

Set `APP_ENV=production` so that: demo/reset/scenario/failure endpoints return 403; demo seed isn't auto-created; logs are JSON.

## 13.6 Deploying the web app

`cd apps/web && npm run build` → static files in `dist/`. Host on any static host/CDN and set `VITE_API_URL` at build time and `CORS_ORIGIN` on the API (or serve both under one origin with a reverse proxy and keep relative URLs).

## 13.7 Secrets management

Local: `.env`. Production: your platform's secret store (not files in the image). Rotate provider keys; if one leaks, rotate and review `audit_events` — they don't contain keys, so nothing needs scrubbing there.

Claude Code MCP for Jev (optional, **development only**):

```bash
export JEVSTATION_API_KEY=...   # never commit
claude mcp add --transport http jevstation https://jevstation.com/api/mcp \
  --header "Authorization: Bearer $JEVSTATION_API_KEY"
```
Tools: `jev_decide`, `jev_run_question_set`, `jev_list_question_sets`, `jev_route_model`, `jev_guard_tool_call`. Use them to explore question wording and see distributions; the app's runtime path stays direct HTTP.

## 13.8 Checklist before pointing at real data

- [ ] real identity provider → `ActorContext`
- [ ] least-privilege DB role; backups; PITR
- [ ] rate limiting per user/IP; WAF; CSP; CSRF where cookies are used
- [ ] step-up auth for money movement
- [ ] calibrated thresholds from labelled history
- [ ] metrics/alerts; on-call runbooks for each failure mode
- [ ] legal/compliance review (KYC/AML, data residency, vendor DPAs)

Next: [14 · Security thinking](14-security-thinking.md)
