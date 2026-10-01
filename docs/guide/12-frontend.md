# 12 · The frontend

The UI is not a bank app; it is an **explanation device** for the pipeline. Priorities: clarity, decision transparency, technical credibility, workflow visibility. Stack: React 19 + TypeScript + Vite + Tailwind CSS v4; no Redux (local state is enough).

## 12.1 Principles

1. **The browser holds no secrets and no authority.** It calls the Rust API only. Confirmation is a request the backend re-validates — never a direct DB write.
2. **Show the transformation.** Language → typed action, *before* anything runs ("PROPOSED ACTION" card).
3. **Render the backend's decisions; never recompute them.** All statuses, reasons, checks, thresholds come from the API. The UI has no business logic that could drift from policy.
4. **Make failure visible and boring.** Blocked, held-for-review and fail-closed states are first-class screens with plain language ("No funds moved.").

## 12.2 Screens

| Screen | File | Shows |
|---|---|---|
| Dashboard | `components/Dashboard.tsx` | balance, card state, device trust, normal range, recent transactions, recipients; six quick-demo buttons; failure-injection and reset controls |
| Agent Console | `components/Console.tsx` | input + examples; Outcome card (confirm / clarify / review / blocked / executed); history |
| Decision Explorer | `components/DecisionExplorer.tsx` | Proposed action → Hard policy → Jev bars → (escalation) → Final decision + reasons |
| Trace Viewer | `components/TraceViewer.tsx` | header, stage strip, clickable timeline, event details, latency bars, usage, ledger, audit Q&A |

## 12.3 Data flow

```
App ── tab state, refresh counter, pending demo command, selected trace id
 ├─ Dashboard  ── api.account/cards/recipients/transactions; onRun(demo) ─► sets pending + switches tab
 ├─ Console    ── api.command → ActionView ; api.confirm / cancel / clarify ; keeps last confirm {id, key}
 │               └─ DecisionExplorer(view)    (pure rendering of ActionView.explorer)
 └─ TraceViewer ── api.trace(actionId) → TraceView
```

`src/types.ts` mirrors the Rust view structs; `src/api.ts` is a 60-line fetch wrapper that turns the backend's `{error:{code,message,trace_id}}` into an `ApiError`.

## 12.4 Idempotency in the UI

Every state-changing call gets a fresh `Idempotency-Key` (`crypto.randomUUID()`). The Console remembers `{actionId, confirmationId, key}` of the last confirm so **"Simulate lost response → client retries"** can resend the *identical* request and show `replayed: true` — a live demonstration of the guarantee.

## 12.5 Safe-by-default rendering

React escapes text by default; we never use `dangerouslySetInnerHTML`. Metadata JSON in the Trace Viewer is rendered as text. The server also strips secrets before they reach audit metadata, so even a UI bug can't reveal what the API never sent.

## 12.6 Dev proxy

`vite.config.ts` proxies `/api` and `/health` to `127.0.0.1:8080`, so in development there's no CORS and no API base URL; for a separate deployment set `VITE_API_URL` and `CORS_ORIGIN`.

## 12.7 Exercises
1. Add a "policy simulation" panel: given an action, show which hard checks *would* fail for a different balance (hint: needs a new read-only endpoint — and a test proving it can't mutate).
2. Colour-blind accessibility: add icons next to PASS/FAIL/SKIP.
3. Add a keyboard shortcut for Confirm — then discuss why you probably shouldn't.

Next: [13 · Configuration and deployment](13-config-and-deployment.md)
