# ADR 8: Why money uses integer minor units

Status: accepted (MVP)

## Context
Floating point cannot represent 0.1 exactly; rounding errors in a ledger are unacceptable.

## Decision
`Money { amount_minor: i64, currency }` with checked arithmetic and explicit currency; PHP 2 000.00 = 200000. The DB stores BIGINT.

## Consequences
Formatting/parsing needs care; mixing currencies is a compile-time/runtime error rather than silent.
