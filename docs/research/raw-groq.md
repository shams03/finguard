# Groq API research (fetched 2026-10-01 from console.groq.com/docs; WebFetch summaries, not raw HTML)

## Basics
- Base URL (OpenAI-compatible): https://api.groq.com/openai/v1  (src: /docs/api-reference)
- Auth: `Authorization: Bearer $GROQ_API_KEY`
- Chat: POST https://api.groq.com/openai/v1/chat/completions
- Responses API: POST https://api.groq.com/openai/v1/responses (endpoint listed in api-reference; request/response shape and structured-output support on it: UNVERIFIED)
- Models list: GET https://api.groq.com/openai/v1/models

## Structured Outputs (https://console.groq.com/docs/structured-outputs)
- Strict (`strict:true`, constrained decoding): ONLY `openai/gpt-oss-20b`, `openai/gpt-oss-120b`, `qwen/qwen3.8-27b`.
- Best-effort (`strict:false`): those three plus `openai/gpt-oss-safeguard-20b`. Best-effort may return HTTP 400 "Generated JSON does not match the expected schema".
- All other models: JSON Object Mode only (`response_format:{"type":"json_object"}`).
- Strict requirements: every property in `required`; `additionalProperties:false` on all objects; optional = union with null.
- Not supported: streaming and tool use together with Structured Outputs.

### Example request
```json
{
  "model": "openai/gpt-oss-20b",
  "messages": [
    {"role": "system", "content": "Extract product review information."},
    {"role": "user", "content": "Review text here"}
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "product_review",
      "strict": true,
      "schema": {
        "type": "object",
        "properties": {
          "product_name": {"type": "string"},
          "rating": {"type": "number"},
          "sentiment": {"type": "string", "enum": ["positive", "negative", "neutral"]},
          "key_features": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["product_name", "rating", "sentiment", "key_features"],
        "additionalProperties": false
      }
    }
  }
}
```
### Example output (message.content string parses to this)
```json
{
  "product_name": "UltraSound Headphones",
  "rating": 4.5,
  "sentiment": "positive",
  "key_features": ["amazing noise cancellation", "all-day battery life", "crisp sound quality"]
}
```
### Full response envelope (docs' generic chat example, model gpt-oss-20b; content truncated in docs)
```json
{
  "id": "chatcmpl-f51b2cd2-bef7-417e-964e-a08f0b513c22",
  "object": "chat.completion",
  "created": 1730241104,
  "model": "openai/gpt-oss-20b",
  "choices": [{"index": 0, "message": {"role": "assistant", "content": "..."}, "logprobs": null, "finish_reason": "stop"}],
  "usage": {
    "queue_time": 0.037493756, "prompt_tokens": 18, "prompt_time": 0.000680594,
    "completion_tokens": 556, "completion_time": 0.463333333,
    "total_tokens": 574, "total_time": 0.464013927
  },
  "system_fingerprint": "fp_179b0f92c9",
  "x_groq": {"id": "req_01jbd6g2qdfw2adyrt2az8hz4w"}
}
```
(A real structured response would have content = the JSON string above; the docs did not show one envelope with it: composite, not a captured live call.)

## Models (https://console.groq.com/docs/models)
Production:
| id | context | max output |
|---|---|---|
| llama-3.1-8b-instant | 131K | 131K |
| llama-3.3-70b-versatile | 131K | 32K |
| openai/gpt-oss-120b | 131K | 65K |
| openai/gpt-oss-20b | 131K | 65K |
| whisper-large-v3, whisper-large-v3-turbo | speech | |
Preview: Canopy Labs Orpheus, Llama Prompt Guard 2, MiniMax M2.7, Qwen models (exact ids incl. qwen/qwen3.8-27b status: UNVERIFIED on models page; it is named in structured-outputs and deprecations pages).

## Deprecations (https://console.groq.com/docs/deprecations)
- llama-3.1-8b-instant: shutdown 08/16/26 -> openai/gpt-oss-20b
- llama-3.3-70b-versatile: 08/16/26 -> openai/gpt-oss-120b or qwen/qwen3.6-27b
- llama-4-scout-17b-16e-instruct: 07/17/26; llama-4-maverick: 03/09/26; llama-guard-4-12b: 03/05/26; qwen/qwen3-32b: 07/17/26; kimi-k2-instruct-0905: 04/15/26
- qwen/qwen3.6-27b: 09/14/26 -> qwen/qwen3.8-27b
- groq/compound, groq/compound-mini: 09/21/26
- CONFLICT: today is 2026-10-01, past the 08/16/26 date for both remaining Llama models, yet the models page still lists them as production. Treat all Llama ids as deprecated/possibly shut down; test before relying. Groq's current Llama-family offering is effectively nil.

## Rate limits (https://console.groq.com/docs/rate-limits)
Free plan (as shown): GPT-OSS 20B/120B: 30 RPM, 1K RPD, 8K TPM, 200K TPD. Other free rows: Prompt Guard 22M 30/14.4K/15K/500K; Whisper v3 20 RPM, 2K RPD; Orpheus 10/100/1.2K/3.6K.
Developer-tier numbers, qwen/qwen3.8-27b limits, and llama limits: UNVERIFIED (not in fetched summary; see console limits page for the account).
Headers: `retry-after` (seconds, only on 429); `x-ratelimit-limit-requests` (daily), `x-ratelimit-limit-tokens` (per-minute), `x-ratelimit-remaining-requests`, `x-ratelimit-remaining-tokens`, `x-ratelimit-reset-requests`, `x-ratelimit-reset-tokens`.

## Errors (https://console.groq.com/docs/errors)
400, 401, 403, 404, 413, 422, 424 (remote MCP), 429, 498 (flex capacity), 499 (cancelled), 500, 502, 503 (server errors not charged). Format: `{"error":{"message":"...","type":"invalid_request_error"}}`.
Retry guidance: on 429 honor `retry-after`; exponential backoff for 500/502/503. Docs-specified backoff algorithm: UNVERIFIED.

## Recommendation
- Primary: `openai/gpt-oss-20b` -- production, strict json_schema supported, 131K ctx/65K out, free tier 30 RPM / 1K RPD / 8K TPM, cheapest/fastest (~1000 t/s), and the named replacement for llama-3.1-8b-instant. Note 8K TPM is tight; keep prompts small. It is a reasoning model: consider low reasoning effort and ample max_completion_tokens (reasoning param details UNVERIFIED).
- Fallback: `openai/gpt-oss-120b` (same strict support and same free limits, better quality). Second fallback: `qwen/qwen3.8-27b` (strict-capable, preview status UNVERIFIED).
- Avoid Llama ids: no strict mode (JSON mode only) and deprecated.
