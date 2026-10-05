# FinGuard explainer video

A ~2 minute narrated explainer, rendered **deterministically** from code (no screen recording):

```
narration.json ──say (macOS TTS, per sentence)──► exact sentence timings ──► timeline
scene.html + scene.js  (every frame is a pure function of time)
   └── headless Chrome (CDP) captures frames in parallel ──► ffmpeg encodes H.264 + AAC
```

Output: `apps/web/public/finguard-explainer.mp4` (+ `finguard-explainer-poster.jpg`), played on the **How it works** page.

## Render

Requirements: macOS (`say`), Google Chrome, `ffmpeg`/`ffprobe`, Node 22+. No npm dependencies.

```bash
cd apps/video
npm run video                          # full render (~1 min on a laptop)
node render.mjs --stills 12,55,100     # PNG stills at those seconds → out/stills (fast QA)
WORKERS=6 node render.mjs              # more parallel Chrome instances
```

## Why this approach
Frame-by-frame rendering from a pure `renderFrame(t)` function is the same idea Remotion uses (a headless browser captures each frame, ffmpeg stitches them), without the dependency weight. Because visuals are functions of time rather than CSS animations they are **deterministic and seekable**, and because cues are keyed to the *measured* length of each narrated sentence, picture and voice cannot drift.

## Motion principles used
Staging (one idea per sentence), staggered entrances, ease-out for arrivals / ease-in-out for travel, overshoot ("back") for emphasis pops, anticipation (the cursor travels before the click), and a persistent progress rail so the viewer always knows where they are.

## Editing
* Change the words in `narration.json` — timing re-flows automatically (TTS clips are cached by text).
* Visual cues live in `scene.js`, one builder per scene; offsets are relative to sentence start (`S[i].a`).
* Captions are burned in (`captionFix` in `narration.json` fixes phonetic spellings such as "A I" → "AI").
