# ADR 9: Why transfers always require confirmation

Status: accepted (MVP)

## Context
Money movement is irreversible in this MVP and model risk scores are not calibrated for it.

## Decision
Every TransferFunds returns RequireConfirmation (or something stricter) regardless of Jev confidence. Confirmation is bound to an action fingerprint, expires, and is single-use.

## Consequences
Slightly more friction, accepted on purpose.
