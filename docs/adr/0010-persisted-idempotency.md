# ADR 10: Why persisted idempotency

Status: accepted (MVP)

## Context
Networks lose responses; clients retry. In-memory dedupe disappears on crash or when scaled out.

## Decision
`idempotency_records` has a UNIQUE key and stores a request fingerprint (SHA-256 over canonical fields) and the response. Same key + same fingerprint returns the stored result; same key + different fingerprint is IDEMPOTENCY_CONFLICT.

## Consequences
One extra insert per state-changing request; correct under crashes and retries.
