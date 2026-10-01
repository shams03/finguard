# 08 · The AI layers

Three model-facing components, one rule for all of them: **everything a model returns is untrusted input that must be parsed into a closed type, validated, grounded against facts we control, and can only add caution.**

## 8.1 Intent parsing

### Deterministic parser (`agent/src/deterministic.rs`)
Rule-based: verbs `send|transfer|pay|give|wire`, keywords `balance`, `freeze … card`; amounts via `domain::amounts_mentioned` (handles `₱2,000`, `75000`, `1,250.50`, `2k`); "usual/normally/regular" sets `refers_to_usual_amount` and **leaves the amount empty**. Offline, deterministic, free → used by every test, and as a fallback when the LLM parser errors.

### Groq parser (`groq/src/intent.rs`)
1. **System prompt** states the role ("you only interpret"), that the message is data, the supported intents, and *never invent an amount or recipient*.
2. **Strict JSON schema** (`intent_schema_v1`): `intent_type` enum, nullable `recipient_reference`/`amount_minor`/`currency`, `missing_fields`, `ambiguity`, `refers_to_usual_amount`; all required; `additionalProperties:false`.
3. **Validation + grounding** (`validate_and_ground`), pure and tested:
   * parse with `deny_unknown_fields` (extra keys → error);
   * `amount_minor` must equal an amount that **literally appears in the user's text** (via `amounts_mentioned`) or it is discarded (+`ambiguity`);
   * `recipient_reference` must share a word with the user's text or it is discarded;
   * `missing_fields` is **recomputed** from what survived — the model's own list is ignored.
4. One **repair retry** for malformed JSON; then typed `InvalidStructuredOutput`.
5. Provider failures (timeout, 429, 5xx, 401) are *not* looped on; the agent falls back to the deterministic parser, recording the failure.

🧠 **Grounding** = checking model output against something the model doesn't control. It turns "the model says 12,000" into "12,000 is in the text, or it isn't".

❓ *Why not trust strict mode alone?* Strict mode guarantees *shape*, not *truth*. A perfectly shaped `{"amount_minor": 1200000}` can still be fabricated. Grounding addresses truth; strict mode addresses parsing.

💥 *Without grounding:* test `llm_parser_cannot_invent_an_amount` — "send Alex the usual amount" + a model that returns ₱99,000 must still end in a clarification, not a confirmable ₱99,000.

### Which Groq model, and why
Researched 2026-10-01 (`docs/research/integration-research.md`):
* Strict `json_schema` is supported on `openai/gpt-oss-20b`, `openai/gpt-oss-120b`, and a Qwen model; other models (incl. Llama) only get JSON mode.
* The current Llama production ids appear on Groq's deprecations page with a shutdown date already past, while the models page still lists them — contradictory docs, so treat as deprecated.
* → default `GROQ_MODEL=openai/gpt-oss-20b` (free tier: 30 RPM / 8K TPM, so keep prompts small). `GROQ_STRICT_SCHEMA=false` selects JSON mode + validation for non-strict models. If the provider rejects strict mode the client degrades once automatically.

*Lesson:* model catalogues change monthly. Put the model in config, put the contract in code (schema + validation), and write down when you checked.

## 8.2 Risk assessment with Jev

### What Jev is
Jev answers *bounded* questions over a text/JSON `state`:
* **Noul** — a yes/no question → a probability in [0,1].
* **Choice** — pick one of N options → distribution + confidence.
* **Score** — an ordered rubric (2–10 levels) → distribution, probability-weighted score, confidence.
Questions are independent and evaluated in parallel; batching several into one call is cheaper and faster than separate calls.

### The question set `risk_questions_v1` (`jev/src/questions.rs`)
| id | type | asks |
|---|---|---|
| `intent_alignment` | Noul | does the proposed action clearly match what the user asked? |
| `behavioral_anomaly` | Score 0–4 | how unusual is this for this customer (from supplied fields only)? |
| `recipient_risk` | Score 0–4 | how unusual/risky is the recipient context? |
| `context_sufficient` | Noul | is there enough information to safely interpret the action? |

Each says *use only the supplied fields* so the model isn't invited to guess. **Only questions supported by actual state are asked:** an incomplete draft gets just the two Noul questions.

❓ *Why atomic questions rather than one "is this fraud?"* — a single giant question mixes concerns, returns one number nobody can interpret, and can't be monitored per-dimension. Atomic questions give per-aspect distributions and let policy reason about them ("anomaly high" vs "context insufficient" need different handling).

### Minimisation (`domain/risk.rs`)
`RiskFacts` (names, account number, balance…) → `RiskContext::from_facts` → allow-list. Notice what Jev gets: *"recipient_known: true, recipient_age_days: 240, amount_to_typical_ratio: 0.25, device_trusted: true…"* — never "Alex Rivera", never the balance. The user's request text passes through `scrub_text` (masks emails, ≥9-digit runs like card numbers, token-like strings, control chars; caps length).

💥 Test `minimisation_drops_pii_and_balances`.

### Transport (`jev/src/client.rs`)
* `reqwest` + `serde`, no SDK (none exists for Rust; docs say call the HTTP API directly).
* **Per-attempt timeout (10 s default)**, connect timeout 5 s, **total deadline**, **≤1 bounded retry** for timeout/429/5xx with backoff; honours `Retry-After` only if short; **never retries 401 / 4xx**.
* Maps status → typed error: 401/403 `Unauthorized`; 429 `RateLimited{retry_after_ms}`; 402 (credits) / 5xx / 529 `ProviderUnavailable`; other 4xx `InvalidResponse`.
* Errors never include URLs/headers (a test asserts the key can't appear in Display/Debug).
* HTTP-level tests use a tiny scripted local server (`crates/jev/tests/http.rs`): retry-then-succeed, bounded retries, `Retry-After`, no retry on 401/422, timeout, garbage bodies.

### Two vendors, one adapter (`jev/src/parse.rs`)
Research found two surfaces: TypeSafe's official `POST https://api.typesafe.ai/v1/systemone` (bare `{model, answers, usage}`) and JevStation's `POST https://jevstation.com/api/v1/systemone` (envelope `{code, message, data:{… credits, latency_ms}}`). The adapter detects the envelope and normalises both. `JEVSTATION_BASE_URL` switches vendor.

### Normalisation and confidence
* Score answers give Jev's own `confidence`. **Noul answers have no confidence field**, so we derive `|2p − 1|` — how decisively the model says yes or no (0 at p=0.5, 1 at 0 or 1).
* `overall_confidence` = **minimum** over answers present ("weakest link"). Conservative: one unsure dimension makes the whole assessment unsure.
* Validation rejects distributions that don't sum to ≈1, out-of-range scores, missing levels, wrong answer types, missing answers.
* **Latency is measured by us** (around the HTTP call), not read from the vendor.

⚠️ **This is not a fraud probability.** It's "how a model rated these four questions, and how decisively". The UI and docs say *model assessment / probability distribution / confidence*, never *fraud score*.

### The mock (`jev/src/mock.rs`)
A scripted `MockDecisionProvider` used by tests and the offline demo. Without overrides it derives a plausible profile from the context (incomplete → low confidence 0.63; new recipient + far above typical or unfamiliar device → high risk; else low risk). It can be told to fail (`Timeout`, `RateLimited`, `ServerError`, `InvalidResponse`, `Unauthorized`) and counts calls — tests assert **zero** calls for hard-blocked requests.

## 8.3 Escalation (`groq/src/escalation.rs`)

Triggered by policy, not by the model. Input (`EscalationInput`): scrubbed request, proposed action, missing fields, a text digest of the Jev assessment, the recipient's first name, and — only if the backend found evidence — `candidate_amount` + `candidate_evidence`.

Output schema (`escalation_schema_v1`):
```json
{ "intent_supported": true, "ambiguity_detected": true, "missing_information": ["amount"],
  "clarification_needed": true,
  "clarification_question": "You usually send Alex ₱5,000. Do you want to send ₱5,000 this time?",
  "recommended_handling": "ASK_USER" }      // CONTINUE_POLICY | ASK_USER | REVIEW
```
Guarantees:
* `recommended_handling` has **no value meaning "execute"/"override policy"**. `EXECUTE_TRANSFER`, `OVERRIDE_AUTHORIZATION`, `IGNORE_POLICY` fail deserialisation (tests in both `domain` and `groq`).
* **Grounded questions.** `question_is_grounded` extracts every number in the question and requires it to appear in the user's request or equal the backend's `candidate_amount`. Else the question is dropped and `fallback_question` (deterministic) is used.
* The suggestion's source is SQL: `facts::usual_amount` = the **median of the last ≤5 transfers to that recipient** (needs ≥3 samples). A model cannot make up "what you usually send".

### What a clarification does
`POST /actions/:id/clarify` creates a **new action** (see chapter 09 for why), cancelling the old as superseded, then runs the *entire* pipeline again.

## 8.4 Failure behaviour of the AI layers (summary)

| Component fails | Result |
|---|---|
| Groq intent parse | fall back to deterministic parser; failure logged in `groq_evaluations` + `intent_parse_failed` |
| Jev (transfer) | manual review (fail closed) |
| Jev (read-only) | irrelevant — not called |
| Groq escalation | clarify with deterministic question, or manual review |
| Anything returns garbage | typed error, same as above — never a guess |

## 8.5 Mutation exercises

| Change | Test that should fail |
|---|---|
| Skip grounding in `validate_and_ground` | `hallucinated_amount_is_discarded`, `llm_parser_cannot_invent_an_amount` |
| Add `Execute` to `RecommendedHandling` | `handling_cannot_express_execution` (+ groq schema test) |
| Send `customer_name` in `RiskContext` | `minimisation_drops_pii_and_balances` |
| Retry on 401 in Jev client | `unauthorized_is_not_retried` |
| Treat Jev timeout as "assume low risk" | `w8_jev_failures_fail_closed_for_money_but_not_reads` |

Next: [09 · Protecting money](09-protecting-money.md)
