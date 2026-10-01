# ADR 4: Supabase only as production Postgres

Status: accepted (MVP)

## Context
Supabase offers many APIs (auth, realtime, PostgREST). Using them couples business logic to a vendor.

## Decision
Supabase is just hosted Postgres reached through `DATABASE_URL`. Direct or session-pooler connection for the server, direct for migrations, `sslmode=require`. No Supabase SDK, RLS or APIs.

## Consequences
Deployment needs zero code change. We forgo Supabase auth/RLS; ActorContext is our own.
