# Raw research: Jev / JevStation / TypeSafe API (verified 2026-10-01)

Everything below was fetched directly from the live pages on 2026-10-01. Anything not found is marked UNVERIFIED.

## 0. Who is who (critical)

- **Jev** is made by **TypeSafe AI**. Official docs: https://docs.typesafe.ai (index: https://docs.typesafe.ai/llms.txt; any page as markdown by appending `.md`). Official API: `https://api.typesafe.ai`. Official SDK repos: github.com/typesafe-ai/typesafe-sdk-python, github.com/typesafe-ai/typesafe-sdk-js, github.com/typesafe-ai/skills.
- **JevStation** (https://jevstation.com) is an INDEPENDENT third-party reseller/workspace, not TypeSafe. Its About page (https://jevstation.com/about): "Jev is made by TypeSafe AI. JevStation is not: it is a separate product that uses TypeSafe's API as a customer." Its endpoint is `POST https://jevstation.com/api/v1/systemone` with a different envelope, credit billing and a pinned model. Its MCP server is JevStation's, not TypeSafe's.
- The JevStation docs page itself says (https://jevstation.com/docs): "If you would rather talk to the model directly, without JevStation metering, TypeSafe's own API is the supported path."
- TypeSafe direct access is behind a waitlist (https://jevstation.com/get-jev, claim by JevStation: "Join TypeSafe's waitlist... no self-serve plans"). UNVERIFIED on TypeSafe's own site whether the waitlist is still active (console.typesafe.ai/keys is linked from the official quickstart).

## 1. Endpoints and auth

### TypeSafe official (source: https://docs.typesafe.ai/api.md, https://docs.typesafe.ai/introduction/quickstart.md)
```http
POST https://api.typesafe.ai/v1/systemone
Authorization: Bearer <API_KEY>
Content-Type: application/json
```
- Env var used by official SDKs: `TYPESAFE_API_KEY`; also `TYPESAFE_BASE_URL`, `TYPESAFE_DEFAULT_MODEL`, `TYPESAFE_LOG_LEVEL` (https://docs.typesafe.ai/sdk/python/api/constants.md). `DEFAULT_BASE_URL = 'https://api.typesafe.ai'`, `DEFAULT_MODEL = 'jev-latest'`.
- Keys from https://console.typesafe.ai/keys. Playground: https://console.typesafe.ai/playground.
- Model list: `GET https://api.typesafe.ai/v1/models` (Bearer auth) returns `{ "models": [ {name, description, release_date} ] }` (https://docs.typesafe.ai/models.md).

### JevStation (source: https://jevstation.com/docs, https://jevstation.com/get-jev, https://jevstation.com/skills/jev-api/SKILL.md)
```
POST https://jevstation.com/api/v1/systemone
Authorization: Bearer sk_...
```
- Env var convention: `JEVSTATION_API_KEY`. Keys: https://jevstation.com/settings/apikeys.
- `GET https://jevstation.com/api/v1/systemone` with same header: free; "returns your plan, remaining credits and rate limits". Response shape UNVERIFIED.

## 2. Request schema (TypeSafe official, https://docs.typesafe.ai/api.md)

Top-level fields:
- `state` : string | object | array, required.
- `model` : string, required. Use `"jev-latest"`.
- `questions` : map<string, Question>, required. Key chosen by you; "The key is not sent to the underlying model and is not used in inference."

Question types (all share `type`, `instructions`; `instructions` may be string | object | array):

Noul:
```json
{ "type": "noul", "instructions": "Does this convey urgency?",
  "criteria": { "true": "Explicitly time-sensitive", "false": "No urgency expressed" } }
```
(`criteria` optional; keys `true`/`false`.)

Choice (`criteria` required, map option -> string|object|array|null; "maximum of 255 options per Choice"):
```json
{ "type": "choice", "instructions": "Which team should handle this?",
  "criteria": { "billing": "Payments, invoicing, refunds", "technical": "Bugs, outages, integrations", "sales": "Pricing, upgrades, new accounts" } }
```

Score (`criteria` required, ordered array; "at least two levels; the API accepts up to 10"):
```json
{ "type": "score", "instructions": "How frustrated is the customer?",
  "criteria": ["Calm", "Frustrated", "Very angry"] }
```

Structured instructions example (from docs):
```json
"instructions": {
  "potential_duplicate": { "name": "John Smith", "location": "Oakland, California", "last_employer": "Google" },
  "question": "Is the resume for the same person as `potential_duplicate`?"
}
```

### Full official example request (https://docs.typesafe.ai/introduction/quickstart.md)
```json
{
  "state": "Hi, I've been trying to connect my Stripe account for 3 days and the integration keeps failing. I'm losing sales. Please help ASAP.",
  "model": "jev-latest",
  "questions": {
    "department": {
      "type": "choice",
      "instructions": "Which team should handle this",
      "criteria": {
        "billing": "Payment or subscription issues",
        "technical": "Bugs or integration problems",
        "sales": "Pricing or account questions"
      }
    },
    "frustration": {
      "type": "score",
      "instructions": "How frustrated the customer appears",
      "criteria": [
        "Calm, just stating facts",
        "Frustrated but civil",
        "Very angry, strong language"
      ]
    },
    "is_urgent": {
      "type": "noul",
      "instructions": "The message conveys urgency or time-sensitivity"
    }
  }
}
```

## 3. Response schema (TypeSafe official)

Top-level: `model` (string; versioned ID that answered, e.g. `jev-1.13.0`), `answers` (map keyed by your ids), `usage` {`input_tokens`, `output_tokens`}. No latency or credit fields in the official response.

- Noul answer: `{ "type": "noul", "noul": <0..1> }` (no confidence, no probabilities).
- Choice answer: `type`, `choice` (highest-probability option), `probabilities` (map option -> float, sums to 1), `confidence` (0..1).
- Score answer: `type`, `score` (probability-weighted, can land between levels), `legend` (map level-index string -> description), `probabilities` (map level-index string -> float), `confidence`.

### Full official example response (quickstart)
```json
{
  "model": "jev-1.13.0",
  "answers": {
    "department": {
      "type": "choice",
      "choice": "technical",
      "confidence": 0.78,
      "probabilities": {
        "technical": 0.85,
        "sales": 0.0,
        "billing": 0.15
      }
    },
    "frustration": {
      "type": "score",
      "score": 1.0,
      "confidence": 1.0,
      "legend": {
        "0": "Calm, just stating facts",
        "1": "Frustrated but civil",
        "2": "Very angry, strong language"
      },
      "probabilities": {
        "0": 0.0,
        "1": 1.0,
        "2": 0.0
      }
    },
    "is_urgent": {
      "type": "noul",
      "noul": 1.0
    }
  },
  "usage": {
    "input_tokens": 392,
    "output_tokens": 65
  }
}
```

Individual API-page examples:
```json
{ "model": "jev-1.13.0", "answers": { "is_urgent": { "type": "noul", "noul": 0.95 } }, "usage": { "input_tokens": 296, "output_tokens": 20 } }
```
```json
{ "model": "jev-1.13.0", "answers": { "department": { "type": "choice", "choice": "billing",
  "probabilities": { "billing": 0.88, "technical": 0.12, "sales": 0.0 }, "confidence": 0.81 } },
  "usage": { "input_tokens": 318, "output_tokens": 34 } }
```
```json
{ "model": "jev-1.13.0", "answers": { "frustration": { "type": "score", "score": 1.05,
  "legend": { "0": "Calm", "1": "Frustrated", "2": "Very angry" },
  "probabilities": { "0": 0.0, "1": 0.95, "2": 0.05 }, "confidence": 0.92 } },
  "usage": { "input_tokens": 304, "output_tokens": 18 } }
```

## 4. Confidence and probabilities
- Source: https://docs.typesafe.ai/api.md, https://docs.typesafe.ai/confidence.md.
- Choice/Score carry `confidence` in [0,1], "derived from the answer's probability distribution". The docs' Choice explorer computes `confidence = clamp((N * peak - 1) / (N - 1), 0, 1)` where N = number of options and peak = max probability (from the page's interactive component source). Score formula: UNVERIFIED (not read).
- Noul has no confidence; near 0.5 = similar yes/no probability.
- Docs example gating: `if confidence < 0.5: route to human`; above that, higher threshold (e.g. > 0.9) for destructive actions; "The correct threshold values depend on your domain... Start with conservative thresholds, test with your own data".
- JevStation suggests three bands (high = act, medium = confirm, low = do not act) with no numeric cutoffs.

## 5. Models (https://docs.typesafe.ai/models.md)
- Current model: `jev-1.13.0`. Price: $42 per Btok / $0.042 per Mtok input; output tokens free.
- Rate limits stated on that page: "100K tokens per second / 40 requests per second". (JevStation's get-jev page separately quotes "1,200 requests a minute and 250,000 tokens a second" as TypeSafe's limits: that conflicts; treat as UNVERIFIED/stale.) Docs warn: "Rate limits are adjusting dynamically... can change without notice". Higher limits: sales@typesafe.ai.
- Context: 64k tokens per request; 32k tokens for `state` plus the longest question. Text only (string, JSON object, array of text values). English best.
- Aliases: `jev-latest` -> `jev-1.13.0` (default in SDKs); `jev-preview` -> `jev-1.13.0` (currently same). Versioned IDs like `jev-1.13.0` accepted by `model`. Docs advise pinning the versioned ID if thresholds are tuned.
- Max questions per request on TypeSafe official API: UNVERIFIED (docs say adding questions is cheap; only the 64k/32k token budgets are stated). Per-Choice option cap 255; Score levels cap 10.

## 6. Errors, status codes, rate limiting (TypeSafe official, api.md)
| Status | Meaning |
| 401 Unauthorized | Missing or invalid API key |
| 422 Unprocessable Entity | Body failed validation; "The body details the offending field" |
| 429 Too Many Requests | Rate limit exceeded; back off and retry |
| 529 Overloaded | Temporarily overloaded; retry after short delay |

- Exact JSON error body schema: UNVERIFIED (docs say only "a JSON body describing what went wrong").
- Guidance: retry 429/529 with exponential backoff. Models page: SDKs "honor the `retry-after` header when the response carries one" (also `retry-after-ms` per Python RetryPolicy).
- SDK default retry policy (JS RetryPolicy / Python): maxRetries 2, initial backoff 500 ms, max backoff 5000 ms, jitter 0.25, retried statuses 408, 429, 500-599, honors Retry-After up to 60000 ms, retries connection and timeout errors.
- Timeouts: official SDK default timeout is 10 s per attempt (JS `timeout` default 10000 ms; Python `DEFAULT_TIMEOUT = 10.0`, "for each HTTP operation"). The Python RetryPolicy signature shows `timeout: float | None = 30.0` (inconsistent in docs; UNVERIFIED which wins). No official "recommended timeout" statement found; 10 s is the SDK default.
- Streaming: not offered (LiteLLM doc https://docs.litellm.ai/docs/pass_through/typesafe; JevStation: "Jev returns answers, not a token stream").
- Latency/usage/credits: official response only has `usage.input_tokens/output_tokens`. No latency or credit field. No official latency number found in docs fetched: UNVERIFIED.

## 7. JevStation API variant (https://jevstation.com/docs, /get-jev, /skills/jev-api/SKILL.md)

Request: same `state` + `questions` (or `question_set_id` instead of questions, not both). `model` is ignored ("JevStation pins the model at the deployment level").
Limits (SKILL.md): up to 12 questions per call; `instructions` up to 600 characters; state up to 24,000 characters; choice 2-12 options; score 2-10 levels (JevStation tighter than TypeSafe's 255 options).
Billing: 1 credit if state <= 8,000 chars and <= 5 questions; 3 credits above. Failed evaluations refunded. New account: 200 free credits (30 days). Packs: $9.90/5,000, $15.90/12,000.

Response envelope (docs say: `code` is 0 on success, -1 on error; `message` is `ok` or error text; result under `data`):
```json
{
  "code": 0,
  "message": "ok",
  "data": {
    "model": "jev-1.13.0",
    "answers": {
      "category": {
        "type": "choice",
        "choice": "integrations",
        "probabilities": { "billing": 0.2, "integrations": 0.7, "bug": 0.08, "account": 0.02 },
        "confidence": 0.6
      },
      "churn_risk": { "type": "noul", "noul": 0.8 }
    },
    "usage": { "input_tokens": 180, "output_tokens": 0 },
    "credits": { "charged": 1, "remaining": 199 },
    "latency_ms": 400
  }
}
```
(Docs: "The values below show the shape only".) Extra fields vs official: `code`, `message`, `data` wrapper, `credits.{charged,remaining}`, `latency_ms`.

Example request (get-jev):
```json
{
  "state": "I have been trying to connect my Stripe account for 3 days and it keeps failing with invalid redirect. I am losing sales every hour.",
  "questions": {
    "category": { "type": "choice", "instructions": "Which team should handle this support ticket?",
      "criteria": { "billing": "Payments, invoices, refunds or plan changes", "integrations": "Connecting or syncing a third-party service", "bug": "The product behaves incorrectly", "account": "Login, access or account settings" } },
    "churn_risk": { "type": "noul", "instructions": "Is the customer at risk of churning?" }
  }
}
```
Errors (real HTTP status, SKILL.md): 401 missing/invalid key; 400 malformed body or questions; 413 state too large; 402 not enough credits; 404 unknown `question_set_id`; 429 rate limit (honour `retry-after`); 502 evaluation failed (credits refunded); 503 at capacity. JevStation docs: "On 429 or 529, retry with exponential backoff." Python example in docs uses `timeout=30`. Error body shape: envelope with `code: -1` and `message` (stated); full detail UNVERIFIED.
JevStation sample latency in docs example: `latency_ms: 400` (illustrative, "shape only").

## 8. MCP server
There is NO official TypeSafe-hosted MCP server found in TypeSafe docs (official route for agents is the Agent skill: `claude plugin marketplace add typesafe-ai/skills` then `claude plugin install typesafe@typesafe-ai`, per https://docs.typesafe.ai/agent-skill.md). The MCP server below is JevStation's (third-party), https://jevstation.com/integrations/jev-mcp (updated 2026-09-28).

- Endpoint: `https://jevstation.com/api/mcp`. Streamable HTTP, stateless, POST only (GET/DELETE return 405), JSON replies, no SSE, no session id. Methods: `initialize`, `ping`, `tools/list`, `tools/call` (batches up to 20). No resources/prompts/sampling. Auth: `Authorization: Bearer <key>`; OAuth not supported. Protocol versions 2025-06-18, 2025-03-26, 2024-11-05.
- Claude Code setup (verbatim):
```bash
claude mcp add --transport http jevstation https://jevstation.com/api/mcp \
  --header "Authorization: Bearer $JEVSTATION_API_KEY"
```
  User scope: `claude mcp add --transport http --scope user jevstation ...`. Project `.mcp.json`:
```json
{
  "mcpServers": {
    "jevstation": {
      "type": "http",
      "url": "https://jevstation.com/api/mcp",
      "headers": { "Authorization": "Bearer ${JEVSTATION_API_KEY}" }
    }
  }
}
```
- Tools (five; exact):
  - `jev_decide` inputs `state`, `questions` (up to 12) - 1 or 3 credits
  - `jev_run_question_set` inputs `state`, `question_set_id` - 1 or 3
  - `jev_list_question_sets` no inputs - free
  - `jev_route_model` input `prompt` - small/medium/frontier + whether code is needed - 1 (3 if long)
  - `jev_guard_tool_call` inputs `user_request`, `tool_name`, optional `tool_args` object - returns `execute`, `confirm` or `block` with probabilities - 1 (3 if long)
- Question-set support: yes, via `jev_run_question_set` / `jev_list_question_sets` (sets saved in JevStation playground).
- Result: returned as JSON text and `structuredContent`; both contain `questions`, `model`, `answers`, `usage`, `credits` (charged, remaining), `latency_ms`. Tool errors come back as results with `isError: true` (not-enough-credits, rate-limit, bad input, refunded failure); unknown tool is a JSON-RPC error; bad key is HTTP 401.
- Guard internals (https://jevstation.com/tools/tool-call-guard): fixed questions `matches_intent` (Noul), `risk` (Score, Harmless -> Low -> Moderate -> High -> Destructive), `decision` (Choice: execute / confirm / block). Exact MCP output JSON for guard: UNVERIFIED.
- The tool list `jev_guard_tool_call, jev_decide, jev_route_model, jev_route_task, jev_check_research, jev_review_completion` seen in search results belongs to unofficial community repos (e.g. tijo95/jev-mcp), NOT the JevStation server.
- JevStation Agent Skills install: `curl -fsSL https://jevstation.com/skills/install.sh | sh`.

## 9. Rust SDK
- Official SDKs: Python (`pip install typesafe-sdk`, Python >= 3.10) and JavaScript/TypeScript (`npm install @typesafe-ai/sdk`, Node >= 20) only (https://docs.typesafe.ai/sdk.md). No official Rust SDK listed; TypeSafe's GitHub org has only Python/JS SDKs.
- Docs state: "You can also call the HTTP API directly from any language." (https://docs.typesafe.ai/sdk.md). So direct HTTP (reqwest + serde) is the documented path for Rust.
- Community Rust crates exist (not official; not vetted): e.g. docs.rs/jev-sdk, typesafeai-sdk, typesafeai-sdk-community, github.com/openjevai/typesafe-sdk-rs, github.com/abeldzan/jev-rs.

## 10. Other verified facts
- LangChain integrations: `langchain-typesafe` (Python), `@langchain/typesafe` (JS).
- Each question evaluated independently and in parallel; batch related questions in one call (official fan-out pattern; cookbook claim: 13 questions batched is "12.2x cheaper and 10.0x faster").
- Data handling: Jev not trained on customer requests; ZDR for enterprise (https://docs.typesafe.ai/legal.md).
- Known weaknesses page: https://docs.typesafe.ai/model-jaggedness/jev-1.13.md (not summarized here).

## Source URLs
https://docs.typesafe.ai/api.md ; /models.md ; /introduction/quickstart.md ; /sdk.md ; /agent-skill.md ; /confidence.md ; /primitives/{choice,score,noul}.md ; /sdk/python/api/constants.md ; /sdk/python/api/retries.md ; /sdk/javascript/api/interfaces/{TypeSafeClientConfig,RetryPolicy}.md ; https://jevstation.com/{docs,get-jev,about,llms.txt,integrations/jev-mcp,integrations/jev-claude-code,tools/tool-call-guard,skills/jev-api/SKILL.md} ; https://docs.litellm.ai/docs/pass_through/typesafe
