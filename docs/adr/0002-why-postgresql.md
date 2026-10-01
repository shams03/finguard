# ADR 2: Why PostgreSQL

Status: accepted (MVP)

## Context
We need ACID transactions, row locks, unique constraints and CHECK constraints to protect money.

## Decision
PostgreSQL is the only durable store. Invariants are enforced twice: in Rust and again by the database (CHECK, UNIQUE, FK, append-only triggers).

## Consequences
Requires a running Postgres for integration tests; no in-memory shortcut for the ledger.
