# 12 · The frontend

The UI is not a bank app; it is an **explanation device** for the pipeline. Priorities: clarity, decision transparency, technical credibility, workflow visibility. Stack: React 19 + TypeScript + Vite + Tailwind CSS v4; no Redux (local state is enough).

## 12.1 Principles

1. **The browser holds no secrets and no authority.** It calls the Rust API only. Confirmation is a request the backend re-validates — never a direct DB write.
2. **Show the transformation.** Language → typed action, *before* anything runs ("PROPOSED ACTION" card).
3. **Render the backend's decisions; never recompute them.** All statuses, reasons, checks, thresholds come from the API. The UI has no business logic that could drift from policy.
4. **Make failure visible and boring.** Blocked, held-for-review and fail-closed states are first-class screens with plain language ("No funds moved.").

## 12.2 Pages (hash-routed: `#/`, `#/try`, `#/how`, `#/trace/<id>`)

| Page | File | Purpose |
|---|---|---|
| Overview | `pages/Overview.tsx` | what this is in one screen; a 5-step **guided demo** with saved progress; account summary |
| Try it | `pages/Try.tsx` | ask in plain English; a plain-language **outcome card** (approval ticket with countdown, clarification, held for review, blocked, done) + a **Why this decision?** panel |
| How it works | `pages/How.tsx` | the narrated explainer video, an interactive animated **flowchart**, guarantees, failure table |
| Trace viewer | `pages/Trace.tsx` | header, stage strip, clickable timeline, latency, ledger, audit Q&A |

### UX decisions (and the audit that drove them)
An honest audit of the first UI found: no onboarding; a wall of cards after every request; a dead spinner during 1–4 s live-provider calls; unexplained jargon; developer controls mixed with the product; no "what happens next"; 10–11 px low-contrast text. Fixes:

1. **Progressive disclosure.** `Explorer.tsx` shows a six-step journey stepper first; each section is a collapsible with a one-line summary ("11 of 11 checks passed", "Looks normal · confidence 82% (needs 70%)") and opens on demand. Failures open automatically.
2. **Say what happened and what's next.** `lib/stages.ts::headline()` turns a status into a plain sentence plus a "Next:" line; `ActionCard.tsx` renders it with the right call to action.
3. **Make waiting visible.** While the server works, `Try.tsx` animates the pipeline stepper (honest "working…" progress), then replaces it with the real result.
4. **Explain jargon in place.** `<Term k="jev">` shows an accessible tooltip from `lib/glossary.ts`.
5. **Categorical, not just numeric.** `Gauge` shows confidence against the required threshold with a marker and a plain verdict; colours carry meaning (green passed, amber needs attention, red stopped, sky needs you).
6. **Quarantine dev tools.** Outage simulation and reset live in a **Demo tools** popover; header pills say "live" or "offline demo".
7. **Accessibility basics.** skip link, `aria-live` results and toasts, visible focus rings, ≥12 px text, `prefers-reduced-motion` respected, keyboard-operable flowchart nodes.

## 12.3 Data flow

```
App ── hash route · refresh counter · pending demo · guided-demo progress (localStorage)
 ├─ Overview  ── api.account/cards/recipients/transactions ; starts demos
 ├─ Try       ── api.command → ActionView ; confirm / cancel / clarify ; keeps last confirm {id, key}
 │              ├─ ActionCard(view)   plain-language outcome + actions
 │              └─ Explorer(view)     stepper + collapsible details (pure rendering of ActionView.explorer)
 ├─ How       ── FlowChart (lib/flow.ts is the data: nodes, edges, scenarios, per-step explanations)
 └─ Trace     ── api.trace(actionId) → TraceView
```

`src/types.ts` mirrors the Rust view structs; `src/api.ts` is a ~60-line fetch wrapper turning `{error:{code,message,trace_id}}` into an `ApiError`.

### The flowchart (`components/FlowChart.tsx`)
An SVG diagram drawn from data in `lib/flow.ts`. Each scenario is just an ordered list of node ids; the animation builds a schedule (dwell at each node, travel along each edge using `getPointAtLength` with ease-in-out), highlights the path, dims unused boxes, and shows who decides at the current step, what happens if it fails, and the code file. To add a step, add a node + edges + `NODE_INFO`.

### The video (`apps/video`)
Rendered from code, not recorded: every frame is a pure function of time, cues are keyed to the measured length of each narrated sentence, headless Chrome captures frames in parallel and ffmpeg encodes them. See `apps/video/README.md`.

## 12.4 Idempotency in the UI

Every state-changing call gets a fresh `Idempotency-Key` (`crypto.randomUUID()`). The Console remembers `{actionId, confirmationId, key}` of the last confirm so **"Simulate lost response → client retries"** can resend the *identical* request and show `replayed: true` — a live demonstration of the guarantee.

## 12.5 Safe-by-default rendering

React escapes text by default; we never use `dangerouslySetInnerHTML`. Metadata JSON in the Trace Viewer is rendered as text. The server also strips secrets before they reach audit metadata, so even a UI bug can't reveal what the API never sent.

## 12.6 Dev proxy

`vite.config.ts` proxies `/api` and `/health` to `127.0.0.1:8080`, so in development there's no CORS and no API base URL; for a separate deployment set `VITE_API_URL` and `CORS_ORIGIN`.

## 12.7 Exercises
0. Add a seventh scenario to the flowchart (e.g. "approval expired") — only `lib/flow.ts` should change.
1. Add a "policy simulation" panel: given an action, show which hard checks *would* fail for a different balance (hint: needs a new read-only endpoint — and a test proving it can't mutate).
2. Colour-blind accessibility: add icons next to PASS/FAIL/SKIP.
3. Add a keyboard shortcut for Confirm — then discuss why you probably shouldn't.

Next: [13 · Configuration and deployment](13-config-and-deployment.md)
