// Renders the explainer: macOS TTS per sentence (exact timing) -> deterministic Chrome frames -> ffmpeg.
// Usage: node render.mjs                 full render to ../web/public/finguard-explainer.mp4
//        node render.mjs --stills 3,12,40  write PNG stills at those seconds to ./out/stills (fast QA)
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "out");
const PUBLIC = resolve(HERE, "../web/public");
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const FPS = 30, WORKERS = Number(process.env.WORKERS ?? 4);
const args = process.argv.slice(2);
const stillsArg = args.includes("--stills") ? args[args.indexOf("--stills") + 1] : null;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const run = (cmd, a, opts = {}) => { const r = spawnSync(cmd, a, { encoding: "utf8", ...opts }); if (r.status !== 0) throw new Error(`${cmd} ${a.slice(0, 4).join(" ")} failed:\n${r.stderr}`); return r.stdout; };

const nar = JSON.parse(readFileSync(join(HERE, "narration.json"), "utf8"));
mkdirSync(OUT, { recursive: true });

// ---------------------------------------------------------------- 1. narration + timeline
function dur(file) { return Number(run("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", file]).trim()); }
function buildTimeline() {
  const audioDir = join(OUT, "audio"); mkdirSync(audioDir, { recursive: true });
  const gap = 0.32, lead = 0.75, tail = 1.1;
  let cursor = 0, n = 0; const scenes = [], clips = [], captions = [];
  const cacheKey = JSON.stringify([nar.voice, nar.rate]);
  for (const sc of nar.scenes) {
    const start = cursor; let t = start + lead; const sents = [];
    sc.sentences.forEach((text, i) => {
      const f = join(audioDir, `s${String(n++).padStart(3, "0")}.wav`);
      const meta = f + ".json";
      let d;
      if (existsSync(f) && existsSync(meta) && JSON.parse(readFileSync(meta, "utf8")).k === cacheKey + text) d = dur(f);
      else {
        const aiff = f.replace(".wav", ".aiff");
        run("say", ["-v", nar.voice, "-r", String(nar.rate), "-o", aiff, text]);
        run("ffmpeg", ["-y", "-v", "error", "-i", aiff, "-ar", "44100", "-ac", "1", f]); rmSync(aiff);
        writeFileSync(meta, JSON.stringify({ k: cacheKey + text })); d = dur(f);
      }
      const extra = sc.id === "journey" ? 0.9 : sc.id === "branches" || sc.id === "guarantees" ? 0.5 : 0;
      sents.push({ text, start: t, end: t + d }); clips.push({ file: f, start: t });
      let cap = text; for (const [a, b] of Object.entries(nar.captionFix)) cap = cap.replaceAll(a, b);
      captions.push({ text: cap, start: t, end: t + d });
      t += d + gap + (i < sc.sentences.length - 1 ? extra : 0);
    });
    const end = sents[sents.length - 1].end + tail;
    scenes.push({ id: sc.id, chip: sc.chip, start, end, sents }); cursor = end;
  }
  return { total: cursor, scenes, captions, clips };
}

// ---------------------------------------------------------------- 2. chrome frames
async function worker(w, TL, frames, outDir, ext) {
  const port = 9600 + w + Math.floor(Math.random() * 300);
  const chrome = spawn(CHROME, [`--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "fgv-"))}`, "--headless=new", "--hide-scrollbars",
    "--force-device-scale-factor=1", "--window-size=1280,720", "--no-first-run", "--disable-gpu", "--allow-file-access-from-files", "about:blank"], { stdio: "ignore" });
  let ws, id = 0; const pend = new Map();
  for (let i = 0; i < 60; i++) { try { const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); const p = tabs.find((x) => x.type === "page"); if (p) { ws = new WebSocket(p.webSocketDebuggerUrl); await new Promise((r) => (ws.onopen = r)); break; } } catch { /* starting */ } await sleep(200); }
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pend.has(d.id)) { pend.get(d.id)(d); pend.delete(d.id); } };
  const send = (method, params = {}) => new Promise((res) => { const i = ++id; pend.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
  await send("Page.enable"); await send("Page.navigate", { url: "file://" + join(HERE, "scene.html") }); await sleep(700);
  const ev = await send("Runtime.evaluate", { expression: `setTimeline(${JSON.stringify(TL)}); true`, returnByValue: true });
  if (ev.result?.exceptionDetails) throw new Error("scene error: " + JSON.stringify(ev.result.exceptionDetails.exception?.description));
  for (const f of frames) {
    const t = f.t;
    const r = await send("Runtime.evaluate", { expression: `renderFrame(${t}); true`, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(`frame ${f.name} (t=${t}): ` + JSON.stringify(r.result.exceptionDetails.exception?.description));
    const shot = await send("Page.captureScreenshot", ext === "png" ? { format: "png" } : { format: "jpeg", quality: 92 });
    writeFileSync(join(outDir, `${f.name}.${ext}`), Buffer.from(shot.result.data, "base64"));
  }
  ws.close(); chrome.kill();
}

const TL = buildTimeline();
writeFileSync(join(OUT, "timeline.json"), JSON.stringify({ total: TL.total, scenes: TL.scenes.map((s) => ({ ...s, sents: s.sents.map((x) => ({ ...x, start: +x.start.toFixed(2), end: +x.end.toFixed(2) })) })) }, null, 1));
console.log(`timeline: ${TL.total.toFixed(1)}s, ${TL.scenes.length} scenes`);
TL.scenes.forEach((s) => console.log(`  ${s.id.padEnd(10)} ${s.start.toFixed(1).padStart(6)}–${s.end.toFixed(1).padEnd(6)} (${s.sents.map((x) => (x.end - x.start).toFixed(1)).join(", ")}s sentences)`));
const forScene = { total: TL.total, scenes: TL.scenes, captions: TL.captions };

if (stillsArg) {
  const dir = join(OUT, "stills"); rmSync(dir, { recursive: true, force: true }); mkdirSync(dir, { recursive: true });
  const times = stillsArg.split(",").map(Number);
  await worker(0, forScene, times.map((t) => ({ t, name: `t${String(t).replace(".", "_").padStart(6, "0")}` })), dir, "png");
  console.log("stills ->", dir); process.exit(0);
}

const framesDir = join(OUT, "frames"); rmSync(framesDir, { recursive: true, force: true }); mkdirSync(framesDir, { recursive: true });
const count = Math.ceil(TL.total * FPS);
const all = Array.from({ length: count }, (_, i) => ({ t: +(i / FPS).toFixed(4), name: String(i).padStart(5, "0") }));
const t0 = Date.now();
await Promise.all(Array.from({ length: WORKERS }, (_, w) => worker(w, forScene, all.filter((_, i) => i % WORKERS === w), framesDir, "jpg")));
console.log(`frames: ${count} in ${((Date.now() - t0) / 1000).toFixed(0)}s`);
if (readdirSync(framesDir).length !== count) throw new Error("missing frames");

// ---------------------------------------------------------------- 3. audio mix
const inputs = TL.clips.flatMap((c) => ["-i", c.file]);
const delays = TL.clips.map((c, i) => `[${i}:a]adelay=${Math.round(c.start * 1000)}|${Math.round(c.start * 1000)}[a${i}]`).join(";");
const mix = TL.clips.map((_, i) => `[a${i}]`).join("") + `amix=inputs=${TL.clips.length}:normalize=0:duration=longest,volume=1.1,alimiter=limit=0.89:level=0,apad=whole_dur=${TL.total.toFixed(2)}[voice]`;
// quiet warm pad under the voice (three sines, slow tremolo, faded) — purely atmospheric
const pad = `sine=f=110:d=${TL.total},sine=f=164.81:d=${TL.total},sine=f=220:d=${TL.total}`;
const padFilter = `[${TL.clips.length}:a][${TL.clips.length + 1}:a][${TL.clips.length + 2}:a]amix=inputs=3:normalize=0,tremolo=f=0.18:d=0.5,lowpass=f=900,volume=0.035,afade=t=in:d=2,afade=t=out:st=${(TL.total - 3).toFixed(2)}:d=3[pad]`;
run("ffmpeg", ["-y", "-v", "error", ...inputs, "-f", "lavfi", "-i", `sine=f=110:d=${TL.total}`, "-f", "lavfi", "-i", `sine=f=164.81:d=${TL.total}`, "-f", "lavfi", "-i", `sine=f=220:d=${TL.total}`,
  "-filter_complex", `${delays};${mix};${padFilter};[voice][pad]amix=inputs=2:normalize=0:duration=first[out]`, "-map", "[out]", "-ar", "44100", join(OUT, "audio.wav")]);

// ---------------------------------------------------------------- 4. encode
mkdirSync(PUBLIC, { recursive: true });
const mp4 = join(PUBLIC, "finguard-explainer.mp4");
run("ffmpeg", ["-y", "-v", "error", "-framerate", String(FPS), "-i", join(framesDir, "%05d.jpg"), "-i", join(OUT, "audio.wav"),
  "-c:v", "libx264", "-preset", "slow", "-crf", "21", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k", "-shortest", "-movflags", "+faststart", mp4]);
const outro = TL.scenes.find((s) => s.id === "outro");
const posterFrame = String(Math.floor((outro.sents[1].start + 1.6) * FPS)).padStart(5, "0");
copyFileSync(join(framesDir, `${posterFrame}.jpg`), join(PUBLIC, "finguard-explainer-poster.jpg"));
const info = run("ffprobe", ["-v", "error", "-show_entries", "format=duration,size", "-of", "csv=p=0", mp4]).trim();
console.log(`wrote ${mp4}  (${info})`);
