# Raw stack research (as of 2026-10-01)

Sources: crates.io API (versions/features), npm registry, static.rust-lang.org channel manifest, Docker Hub API, supabase.com docs, postgresql.org, sqlx CHANGELOG, reqwest CHANGELOG, tower-http CHANGELOG, axum CHANGELOG, tailwindcss.com docs.

## A. Supabase Postgres connections
Source: https://supabase.com/docs/guides/database/connecting-to-postgres ; https://supabase.com/docs/guides/database/connection-management

| Mode | Port | IPv4 | IPv6 | Use |
|---|---|---|---|---|
| Direct | 5432 | only with paid IPv4 add-on | yes (default) | persistent backends (VMs, long-running containers); REQUIRED for migrations, pg_dump, backups, replication |
| Session pooler (Supavisor) | 5432 | yes | no | IPv4-only networks that lack the add-on; full prepared-statement support |
| Transaction pooler (Supavisor) | 6543 | yes | no | serverless/edge, many short-lived connections |

- Transaction pooler: no prepared statements, no query pipelining; session state, cursors, temp tables, advisory-lock/session settings don't persist across transactions. Docs say disable prepared statements in the driver and set SSL to require.
- Recommendation: long-running Rust server -> direct connection if host has IPv6, otherwise session pooler (5432, pooler host, user `postgres.<project-ref>`). Migrations -> direct (or session pooler if IPv4-only); never the transaction pooler (migrate takes advisory locks, uses multi-statement/prepared).
- SQLx on transaction pooler (port 6543): set `PgConnectOptions::statement_cache_capacity(0)` (API from sqlx docs knowledge, UNVERIFIED against 0.9 docs page this session) and avoid relying on the macros' prepared flow; use `.persistent(false)` on queries as an alternative (UNVERIFIED for 0.9). Not needed for direct/session.
- TLS: use `sslmode=require` (`?sslmode=require` in URL; `verify-full` with Supabase CA is stricter, optional). SQLx needs a tls feature (`tls-rustls`) or connection fails/falls back.
- Pool sizing (connection-management doc): don't raise pool past ~40% of DB max connections if heavily using PostgREST; otherwise up to ~80%, leaving rest for Auth/Storage etc. For one Rust server instance: `max_connections` ~5-20, set acquire_timeout; total across instances must stay under limit. Monitor `pg_stat_activity` for idle-in-transaction. Pooler-side client/pool limits depend on plan (UNVERIFIED exact numbers).

## B. Versions (TODAY) and features

| Crate / tool | Latest stable | Features to enable |
|---|---|---|
| Rust stable | 1.98.1 (2026-09-01) | edition 2024 |
| axum | 0.8.9 (MSRV 1.80) | default (http1,json,query,tokio,tracing,matched-path,original-uri,form,macros); add `macros` if used |
| tokio | 1.53.1 | `rt-multi-thread`, `macros`, `net`, `signal`, `time` (or `full`) |
| sqlx | 0.9.0 (MSRV 1.94) | `runtime-tokio`, `tls-rustls`, `postgres`, `uuid`, `time` (or `chrono`), `migrate`, `macros`, `json` (default has any,macros,migrate,json; use default-features=false to trim) |
| reqwest | 0.13.5 | default-features=false, `rustls`, `json` (`query`/`form` now opt-in) |
| serde | 1.0.229 | `derive` |
| serde_json | 1.0.151 | default |
| tower | 0.5.3 | `util`, `timeout`, `limit` as needed |
| tower-http | 0.7.1 | `trace`, `cors`, `request-id`, `propagate-header`, `timeout`, `compression-gzip` as needed |
| tracing | 0.1.44 | default |
| tracing-subscriber | 0.3.23 | `env-filter`, `json`, (fmt default) |
| uuid | 1.26.1 | `v4`, `v7`, `serde` |
| thiserror | 2.0.21 | default |
| time | 0.3.55 | `serde`, `formatting`, `parsing`, `macros` (pick time OR chrono; sqlx `time` feature) |
| chrono (alt) | 0.4.45 | `serde` |
| sha2 | 0.11.0 | default (0.11 new RustCrypto generation; API differences vs 0.10 possible, UNVERIFIED) |
| async-trait | 0.1.92 | only if you need dyn async traits |
| testcontainers | 0.28.0 | + testcontainers-modules 0.15.0 feature `postgres` (optional) |
| sqlx::test | part of sqlx (`macros`+`migrate`) | needs DATABASE_URL; auto-creates per-test DB, runs migrations |

Breaking changes since 2024:
- axum 0.8: routes `/{id}` and `/{*rest}` (old `/:id` panics); `#[async_trait]` removed from FromRequest/Handler (use native async fn); `Option<Path/Query>` stricter; `axum::serve(listener, app)`. https://github.com/tokio-rs/axum/blob/main/axum/CHANGELOG.md
- sqlx 0.9 (https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md): all `query*()` take `impl SqlSafeStr` (only `&'static str` or `AssertSqlSafe(..)`, so `format!` queries must be wrapped); combined features like `runtime-tokio-rustls` deleted -> use `runtime-tokio` + `tls-rustls`; `tls-rustls` = ring provider (also `tls-rustls-aws-lc-rs`, `tls-rustls-ring-webpki`, `-native-roots`); Migrate trait changed; PgConnectOptions::options auto-escaped; Query nullability inference changes; MSRV 1.94; repo moved to github.com/transact-rs.
- reqwest 0.13: rustls default TLS with aws-lc; `rustls-tls` renamed `rustls`; `query` and `form` features opt-in; uses rustls-platform-verifier. https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md
- tower-http 0.6: tower 0.5; 0.7: compression rejects `identity;q=0` with 406, removed no-op `tokio`/`async-compression` features, FollowRedirect forwards extensions. https://github.com/tower-rs/tower-http/blob/master/tower-http/CHANGELOG.md
- thiserror 2.x: needs `{field}` formatting rules/no-direct-dep-on-v1; sha2 0.11, tracing-subscriber unchanged API (UNVERIFIED details).
- async-trait: native `async fn` in traits is stable but NOT dyn-compatible. Recommendation: use native async fn for generic/static dispatch; for `Arc<dyn Trait>` use `#[async_trait]` or return `Pin<Box<dyn Future + Send>>` manually (recommendation from language knowledge, UNVERIFIED in a doc fetch this session).
- DB tests: prefer `#[sqlx::test]` (fastest, isolated DBs) against a docker postgres; use testcontainers 0.28 when you want self-contained tests without external DB.

## C. PostgreSQL / frontend
- PostgreSQL 18 is the newest major (18.6 current minor, supported to 2030-11-14); 17.11, 16.15, 15.19, 14.24 (EOL 2026-11-12). https://www.postgresql.org/support/versioning/ . Docker tags exist: `postgres:18-alpine` (also `18-alpine3.24`). Check Supabase project's major version (Supabase runs its own, commonly 15/17; UNVERIFIED) and match local dev to it.
- npm latest: vite 8.3.1, react 19.3.0, tailwindcss 4.3.3, @tailwindcss/vite 4.3.3, @vitejs/plugin-react 6.1.1, typescript 7.0.2.
- Tailwind v4 + Vite (https://tailwindcss.com/docs/installation/using-vite): `npm i tailwindcss @tailwindcss/vite`; in vite.config.ts `plugins: [react(), tailwindcss()]`; CSS: `@import "tailwindcss";`. No tailwind.config.js or PostCSS required.
