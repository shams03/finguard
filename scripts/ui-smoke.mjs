// Drives the real UI in headless Chrome over the DevTools protocol (no extra deps; Node 22+).
// Usage: node scripts/ui-smoke.mjs [outDir]   (backend :8080 and `npm run dev` :5173 must be running, mock providers)
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const BASE = process.env.UI_URL ?? "http://localhost:5173/";
const out = process.argv[2] ?? join(tmpdir(), "finguard-ui");
mkdirSync(out, { recursive: true });
const port = 9300 + Math.floor(Math.random() * 500);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const chrome = spawn(CHROME, [`--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "fg-"))}`,
  "--headless=new", "--window-size=1400,1000", "--no-first-run", "--disable-gpu", "about:blank"], { stdio: "ignore" });
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
const js = async (expr) => { const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text)); return r.result?.result?.value; };
const shot = async (name) => { await sleep(900); const r = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true }); writeFileSync(join(out, `${name}.png`), Buffer.from(r.result.data, "base64")); console.log("screenshot", name); };
const clickText = (t, sel = "button") => js(`(() => { const els=[...document.querySelectorAll('${sel}')].filter(b=>b.offsetParent!==null&&b.textContent.toLowerCase().includes(${JSON.stringify(t.toLowerCase())})&&!b.disabled); const el=els[0]; if(!el) return false; el.click(); return true; })()`);
const bodyHas = (t) => js(`document.body.innerText.toLowerCase().includes(${JSON.stringify(t.toLowerCase())})`);
async function waitFor(text, ms = 9000) { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (await bodyHas(text)) return true; await sleep(150); } throw new Error(`timed out waiting for: ${text}`); }
async function must(ok, what) { if (!ok) throw new Error(`could not click: ${what}`); }
const goto = async (hash) => { await js(`location.hash=${JSON.stringify(hash)}`); await sleep(350); };
const typeInto = (sel, text) => js(`(() => { const el=document.querySelector(${JSON.stringify(sel)}); const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set; set.call(el,${JSON.stringify(text)}); el.dispatchEvent(new Event('input',{bubbles:true})); return true; })()`);
const api = (path) => fetch(`http://localhost:5173${path}`, { method: "POST" });

try {
  await connect();
  await send("Page.enable"); await send("Runtime.enable");
  await send("Page.navigate", { url: BASE }); await sleep(1200);
  await api("/api/v1/demo/reset"); await js("localStorage.clear()");
  await send("Page.reload"); await waitFor("Policy-controlled AI"); await shot("01-overview");

  // guided demo 1: balance (no AI)
  await must(await clickText("Ask something simple"), "tour step 1"); await waitFor("Your balance is"); await waitFor("Why this decision?"); await shot("02-balance");

  // guided demo 2: transfer -> approve -> retry -> trace
  await goto("#/"); await waitFor("1 of 5 done");
  await must(await clickText("Send money, safely"), "tour step 2"); await waitFor("Approve sending"); await waitFor("Single use"); await shot("03-approval");
  await clickText("Risk check", "button"); await sleep(300); await shot("03b-risk-open");
  await must(await clickText("Approve"), "Approve"); await waitFor("sent to Alex"); await shot("04-done");
  await must(await clickText("Simulate a lost response"), "retry"); await waitFor("Duplicate retry recognised");
  await must(await clickText("Open full trace"), "open trace"); await waitFor("Decision summary"); await waitFor("Ledger"); await shot("05-trace");

  // guided demo 3: ambiguous -> clarification -> new proposal
  await goto("#/"); await waitFor("Be vague on purpose");
  await must(await clickText("Be vague on purpose"), "tour step 3"); await waitFor("I need one more detail"); await waitFor("Why I asked you a question"); await shot("06-clarify");
  await must(await clickText("Yes, use"), "accept suggestion"); await waitFor("Approve sending ₱5,000"); await shot("07-after-clarify");

  // guided demo 4: suspicious
  await goto("#/"); await must(await clickText("Look suspicious"), "tour step 4"); await waitFor("Held for review"); await shot("08-review");

  // insufficient funds typed by hand
  await api("/api/v1/demo/scenarios/low_balance"); await goto("#/try");
  await must(await clickText("Reset demo"), "reset").catch(() => 0);
  await api("/api/v1/demo/scenarios/low_balance"); await send("Page.reload"); await goto("#/try"); await sleep(600);
  await typeInto("#cmd", "Send ₱40,000 to Alex."); await must(await clickText("Send"), "send"); await waitFor("Not enough funds"); await waitFor("0 AI calls"); await shot("09-insufficient");

  // unknown payee -> add them -> retry; unverified payee + big amount -> held
  await api("/api/v1/demo/reset"); await send("Page.reload"); await goto("#/try"); await sleep(600);
  await waitFor("Joe (unverified)");
  await typeInto("#cmd", "pay Maria 500"); await must(await clickText("Send"), "send"); await waitFor("Who can you send money to"); await shot("09b-unknown-payee");
  await typeInto("#newpayee", "Maria"); await must(await clickText("Add payee"), "add payee"); await waitFor("added as an unverified payee");
  await must(await clickText("Send"), "send"); await waitFor("Approve sending"); await shot("09c-added-payee-approval");
  await typeInto("#cmd", "Send ₱15,000 to Joe"); await must(await clickText("Send"), "send"); await waitFor("Held for review"); await shot("09d-joe-held");

  // how it works
  await goto("#/how"); await waitFor("Follow a request"); await sleep(1800);
  if (await bodyHas("Watch it in under two minutes")) throw new Error("the explainer video should be gone from the How page");
  await shot("10-how");
  await must(await clickText("₱40,000 with only ₱20,000", "[role=tab]"), "flow scenario"); await sleep(1200);
  await js(`document.querySelector('[aria-label^="Safety rules"]').dispatchEvent(new MouseEvent('click',{bubbles:true}))`); await waitFor("Facts only"); await shot("11-how-node");

  // trace list + tour complete
  await goto("#/trace"); await waitFor("Pick a request"); await shot("12-trace-list");
  await goto("#/"); await waitFor("5 of 5 done").catch(() => console.log("note: tour not fully complete (expected if a step was skipped)"));

  // mobile pass
  await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
  await goto("#/"); await sleep(500); await shot("13-mobile-overview");
  await goto("#/try"); await sleep(500); await shot("14-mobile-try");
  await goto("#/how"); await sleep(800); await shot("15-mobile-how");
  const overflow = await js("document.documentElement.scrollWidth > window.innerWidth + 2");
  console.log("mobile horizontal overflow:", overflow);
  console.log("UI SMOKE OK");
} catch (e) { console.error("UI SMOKE FAILED:", e.message); try { await shot("failure"); } catch {} process.exitCode = 1; }
finally { chrome.kill(); ws?.close(); }
