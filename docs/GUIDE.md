# FinGuard Learning Guide

*From "I've built a few CRUD apps" to "I can explain, defend and extend a safety-critical AI system."*

This guide is written for an engineer with roughly a year of experience. It assumes you can read code and use git, and that words like *transaction*, *enum* or *async* are familiar-ish but fuzzy. It does **not** assume you know Rust, payments, or LLM internals. Every concept is introduced when first needed, tied to real code in this repository, and followed by *why we chose it*, *what else we could have done*, and *what breaks if you remove it*.

## How the guide is organised

```
PART A — 0 → 1   "I understand what this is and can run and explain it"
  00  Start here                     how to learn from this repo
  01  The problem and the idea       why AI + money is dangerous; the one principle
  02  One request, end to end        a transfer traced through every layer
  03  Glossary                       every term, plain English
  04  Rust for this codebase         the 12 Rust ideas you need, shown in our code
  05  Run it and poke it             hands-on experiments that teach the invariants

PART B — 1 → 100 "I understand why each part exists and can change it safely"
  06  Domain modelling               ids, Money, typed actions, state machines
  07  Policy                         hard rules, thresholds, the final decision
  08  The AI layers                  Groq, Jev, grounding, minimisation, escalation
  09  Protecting money               confirmation, idempotency, transactions, concurrency, ledger
  10  Audit, tracing, observability  how every decision stays explainable
  11  Testing strategy               what each test proves, how to write the next one
  12  The frontend                   the UI as an explanation device
  13  Configuration and deployment   env vars, Docker, Supabase
  14  Security thinking              how to threat-model (with this repo as the worked example)
  15  From sandbox to production     the honest 1 → 100 roadmap
  16  Exercises                      30 graded tasks (with "what should fail" hints)
  17  Interview notes                talking points, hard questions, honest limits
```

Start with [00](guide/00-start-here.md). If you have only 30 minutes, read **01, 02, then run 05**.

| # | Chapter |
|---|---|
| 00 | [Start here](guide/00-start-here.md) |
| 01 | [The problem and the idea](guide/01-the-problem-and-the-idea.md) |
| 02 | [One request, end to end](guide/02-one-request-end-to-end.md) |
| 03 | [Glossary](guide/03-glossary.md) |
| 04 | [Rust for this codebase](guide/04-rust-for-this-codebase.md) |
| 05 | [Run it and poke it](guide/05-run-it-and-poke-it.md) |
| 06 | [Domain modelling](guide/06-domain-modelling.md) |
| 07 | [Policy](guide/07-policy.md) |
| 08 | [The AI layers](guide/08-ai-layers.md) |
| 09 | [Protecting money](guide/09-protecting-money.md) |
| 10 | [Audit, tracing, observability](guide/10-audit-and-observability.md) |
| 11 | [Testing strategy](guide/11-testing.md) |
| 12 | [The frontend](guide/12-frontend.md) |
| 13 | [Configuration and deployment](guide/13-config-and-deployment.md) |
| 14 | [Security thinking](guide/14-security-thinking.md) |
| 15 | [From sandbox to production](guide/15-to-production.md) |
| 16 | [Exercises](guide/16-exercises.md) |
| 17 | [Interview notes](guide/17-interview-notes.md) |

Related reference docs: [ARCHITECTURE](ARCHITECTURE.md) · [SECURITY](SECURITY.md) · [FAILURE_MODES](FAILURE_MODES.md) · [DEMO](DEMO.md) · [ADRs](adr/) · [research notes](research/integration-research.md).
