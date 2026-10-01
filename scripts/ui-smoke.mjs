// Drives the real UI in headless Chrome over the DevTools protocol (no extra deps; Node 22+).
// Usage: node scripts/ui-smoke.mjs [outDir]   (backend :8080 and `npm run dev` :5173 must be running)
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const URL_ = process.env.UI_URL ?? "http://localhost:5173/";
const out = process.argv[2] ?? join(tmpdir(), "finguard-ui");
mkdirSync(out, { recursive: true });
const port = 9300 + Math.floor(Math.random() * 500);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const chrome = spawn(CHROME, [`--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "fg-"))}`,
  "--headless=new", "--window-size=1400,1100", "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
let ws, id = 0; const pending = new Map();
async function connect() {
  for (let i = 0; i < 50; i++) {
    try {
      const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = tabs.find((t) => t.type === "page");
      if (page) { ws = new WebSocket(page.webSocketDebuggerUrl); await new Promise((r) => (ws.onopen = r)); break; }
    } catch { /* chrome still starting */ }
    await sleep(200);
  }
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
}
const send = (method, params = {}) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
const shot = async (name) => { const r = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true }); writeFileSync(join(out, `${name}.png`), Buffer.from(r.result.data, "base64")); console.log("screenshot", name); };
const clickText = (t, sel = "button") => js(`(() => { const el=[...document.querySelectorAll('${sel}')].find(b=>b.textContent.includes(${JSON.stringify(t)})&&!b.disabled); if(!el) return false; el.click(); return true; })()`);
const bodyHas = (t) => js(`document.body.innerText.toLowerCase().includes(${JSON.stringify(t.toLowerCase())})`);
async function waitFor(text, ms = 8000) { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (await bodyHas(text)) return true; await sleep(150); } throw new Error(`timed out waiting for: ${text}`); }

try {
  await connect();
  await send("Page.enable"); await send("Runtime.enable");
  await send("Page.navigate", { url: URL_ }); await sleep(1500);
  await fetch("http://localhost:5173/api/v1/demo/reset", { method: "POST" });
  await send("Page.reload"); await waitFor("Available balance"); await shot("1-dashboard");

  await clickText("Normal Transfer"); await waitFor("Confirm transfer"); await waitFor("Jev assessment"); await shot("2-normal-proposal");
  await clickText("Confirm"); await waitFor("Transfer executed"); await shot("3-executed");
  await clickText("Simulate lost response"); await waitFor("Duplicate retry detected"); console.log("idempotent replay shown");
  await clickText("Open trace"); await waitFor("Decision summary"); await waitFor("Ledger"); await shot("4-trace");

  await clickText("Dashboard"); await waitFor("Quick demos");
  await clickText("Ambiguous Transfer"); await waitFor("Clarification needed"); await waitFor("Confidence escalation triggered"); await shot("5-clarification");
  await clickText("Yes, use"); await waitFor("NEW action was created"); await waitFor("Confirm transfer"); await shot("6-after-clarification");

  await clickText("Dashboard"); await waitFor("Quick demos"); await clickText("Suspicious Transfer"); await waitFor("No funds moved"); await shot("7-manual-review");
  await clickText("Dashboard"); await waitFor("Quick demos"); await clickText("Insufficient Funds"); await waitFor("INSUFFICIENT_FUNDS"); await waitFor("AI assessment skipped"); await shot("8-insufficient");
  await clickText("Dashboard"); await waitFor("Quick demos"); await clickText("Reset demo");
  console.log("UI SMOKE OK");
} catch (e) { console.error("UI SMOKE FAILED:", e.message); try { await shot("failure"); } catch {} process.exitCode = 1; }
finally { chrome.kill(); ws?.close(); }
