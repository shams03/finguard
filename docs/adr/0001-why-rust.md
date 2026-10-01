# ADR 1: Why Rust

Status: accepted (MVP)

## Context
The control plane is the part that must be correct: money types, state machines, policy. Bugs here are expensive; a model's mistakes must not become runtime surprises.

## Decision
Single Rust backend (axum + tokio + sqlx). Newtypes, exhaustive enums and `match` make illegal states unrepresentable and force every action type to be handled by policy.

## Consequences
Slower iteration than a scripting language and a steeper learning curve. In exchange, the compiler enforces that a new ProposedAction variant cannot ship without a risk class, a policy rule and an executor.
