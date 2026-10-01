# Live calibration notes (Jev, 2026-10-01)

**Purpose:** choose the demo confidence thresholds from observed behaviour instead of guessing, and record what we learned. **This is not a validation.** n is tiny (≤5 per scenario), one model version (`jev-1.13.0`), synthetic contexts written by us.

Reproduce (spends ~1 credit per call; paced to the provider's rate limit):

```bash
set -a; source .env; set +a
CAL_N=5 cargo test -p finguard-jev --test calibrate -- --ignored --nocapture
CAL_ONLY=normal CAL_N=4 …        # run one scenario
```

## What "confidence" means here
`overall_confidence` = the minimum, over the answers present, of
* Score answers: Jev's own `confidence`;
* Noul answers (no vendor confidence): `|2p − 1|`. This equals Jev's documented Choice formula `(N·peak − 1)/(N − 1)` for N = 2, so the scale is consistent.

On this scale a *healthy* request can't score near 1: an answer with a 0.90 peak over 5 levels is only `(5·0.9−1)/4 = 0.875`, and "93% yes" is `0.86`.

## Results (weakest-link confidence)

| Scenario | n | overall confidence | notes |
|---|---|---|---|
| Normal ₱2,000 → known recipient | 5 | **0.79 – 0.85** | anomaly confidence 0.76–0.85 is the limiting term |
| Normal ₱5,000 → known recipient | 5 | **0.86 – 0.88** | |
| Ambiguous ("usual amount", no amount) | 3 | **0.26 – 0.34** | `context_sufficient` ≈ 0.06 yes |
| Suspicious ₱75,000 → new payee, new device | 3–5 | **0.52 – 0.64** | anomaly and recipient scores ≈ 3.8/4 (one sample returned 1.0 — model variance) |

## Finding 1 — the first guess was wrong
The initial demo thresholds (money movement 0.90) were unreachable: **every** normal transfer would have escalated. Wording mattered too: with my first `intent_alignment` question the model hedged at ≈0.83 "yes" (overall 0.64–0.74); after clarifying that wording differences and "known recipient" vs a named person are not mismatches, it answered ≈0.98 "yes".

**New defaults:** ReadOnly 0.50 · Reversible 0.60 · **MoneyMovement 0.70** · HighImpact 0.80 · default 0.65. The money-movement threshold sits between "normal" (≥0.79) and "suspicious/ambiguous" (≤0.64). The margin is thin on purpose-built contexts and **would need real labelled data** before any real use.

## Finding 2 — AI evidence must not hard-block
With live Jev the "suspicious" demo scored both dimensions ≈3.8/4. An earlier rule ("both ≥ 3.5 → Block") therefore *blocked* it rather than holding it for review. Policy now: risk evidence (model or deterministic) can only produce `MANUAL_REVIEW`; only hard policy blocks.

## Finding 3 — provider limits (JevStation, observed headers)
* `x-ratelimit-limit: 10` per **minute** per key (`x-ratelimit-scope: key-minute`), and a **daily** cap (`x-ratelimit-remaining-day` was 49 after ~60 calls).
* Credits are returned in the body (`credits.charged/remaining`) and in `x-credits-remaining`.
* Latency (client-measured, one batched 4-question call): ≈1.4 – 4.2 s.
* A 429 inside the cap window is expected during bursts. Our client honours a short `Retry-After`, otherwise returns `RateLimited` immediately and **policy fails closed (manual review)** — verified live.
* Operational consequence: for repeated demos use `DECISION_PROVIDER=mock`, or pace live runs; each transfer proposal costs 1 credit (4 questions batched).

## Finding 4 — Groq live (`openai/gpt-oss-20b`)
* Strict `json_schema` accepted; `reasoning_effort: "low"` accepted. Latency ≈ 0.45 – 0.95 s; ≈ 500 input / 90–165 output tokens per call.
* Intent parsing: "usual amount" → `amount_minor: null`, `missing_fields: [amount]` (not invented). Injection text still parses as an ordinary transfer.
* Escalation: the first prompt let the model ignore the backend's history-derived `candidate_amount` and ask a generic question. After telling it to confirm that exact value, 3/3 samples produced *"You usually send Alex ₱5,000.00. Do you want to send ₱5,000.00 this time?"* (and the grounding check passes).
