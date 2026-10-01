# ADR 11: Why no Kafka (or other infra)

Status: accepted (MVP)

## Context
This MVP is one customer, one process. Distributed infra would hide the interesting logic behind plumbing.

## Decision
Single binary + Postgres. The append-only `audit_events` table is the event log and powers the Trace Viewer.

## Consequences
Does not horizontally scale as an event platform; documented as future work, not hidden.
