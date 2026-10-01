# ADR 6: Why Groq cannot authorize

Status: accepted (MVP)

## Context
Groq (an LLM) is good at language and bad at being an authority; it is also exposed to prompt injection.

## Decision
Groq parses language into a proposed action and, on low confidence, writes a clarification. Its output schema is an enum `CONTINUE_POLICY | ASK_USER | REVIEW`; there is no value that means execute/override. Any other value fails validation.

## Consequences
Even a fully compromised Groq response cannot move money or skip a policy gate.
