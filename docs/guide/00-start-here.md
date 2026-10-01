# 00 · Start here

## What you will be able to do after this guide

* Explain, in two minutes and in plain English, why an LLM must never be wired directly to a database that moves money — and what to build instead.
* Trace any request through FinGuard and say, for every step, *which code is allowed to decide*, *which is only advising*, and *what happens if that step fails*.
* Read the Rust code in this repo with confidence even if you've never written Rust.
* Add a new action (e.g. "unfreeze card" or "pay bill") safely — and know which tests must fail first.
* Defend the design in an interview, including its honest limitations.

## How to learn from a codebase (a method that works)

1. **Run it first.** Seeing behaviour makes code legible. Chapter 05 gives you ten experiments.
2. **Follow one request** all the way down (chapter 02). Open the file named at each step.
3. **Break things on purpose.** For each safety mechanism, delete it and watch exactly one test fail. Chapters 06–09 end with a "what breaks if you remove it" list; chapter 16 turns those into exercises. If nothing fails when you delete a safety check, you've found a missing test.
4. **Read tests as documentation.** `crates/agent/tests/scenarios.rs` is the executable specification: each test name is a sentence about the system.
5. **Explain it out loud** (rubber-duck or a friend). Chapter 17 has the questions to practise.

## The map of the repository

```
finguard/
├── crates/
│   ├── domain/        ← vocabulary: ids, Money, actions, state machine, risk types, traits. NO I/O.
│   ├── policy/        ← rules: hard checks, thresholds, final decision. NO I/O.
│   ├── persistence/   ← Postgres: the executor (only code that moves money), audit, idempotency
│   ├── jev/           ← adapter to the Jev risk model + a scripted mock
│   ├── groq/          ← adapter to Groq LLM: intent parser, escalation analyzer + mock
│   ├── agent/         ← the orchestrator that wires it together; deterministic parser; JSON views
│   └── api/           ← HTTP (axum): thin; config; log redaction
├── apps/web/          ← React UI: Dashboard, Agent Console (+ Decision Explorer), Trace Viewer
├── migrations/        ← the SQL schema (plain PostgreSQL)
└── docs/              ← you are here
```

**A rule you can use to read the code:** arrows of knowledge point *down*. `domain` knows nothing about the others. `policy` knows only `domain`. Only `persistence` knows SQL. Only `jev`/`groq` know HTTP to vendors. `api` knows HTTP to browsers. If you ever see SQL in `policy`, or an HTTP call in `domain`, something is wrong.

## Conventions used in this guide

* 🧠 **Concept** — an idea you'll reuse everywhere.
* 🔧 **In the code** — where to look.
* ❓ **Why this way** — the decision and its alternatives.
* 💥 **What breaks without it** — the failure the mechanism prevents.
* ⚠️ **Honest limit** — what the sandbox does *not* solve.

## Prerequisites you can pick up on the way

* Git, a terminal, Docker basics (`docker compose up -d`).
* Reading JSON and SQL.
* What an HTTP request is (method, path, headers, body, status code).
Everything else is explained.
