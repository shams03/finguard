# ADR 5: Why Jev is advisory

Status: accepted (MVP)

## Context
Jev returns calibrated-looking probabilities for bounded questions. It is a model, so it can be wrong or unavailable.

## Decision
Jev output feeds deterministic policy as *risk evidence*. It can raise caution (manual review, escalation) but can never grant authority or remove the confirmation requirement.

## Consequences
Worst case of a Jev error is an unnecessary review, never an unauthorised transfer.
