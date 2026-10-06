import { useEffect, useMemo, useRef, useState } from "react";
import { EDGES, FLOW_SCENARIOS, LANES, NODES, NODE_INFO } from "../lib/flow";
import type { FlowEdge, FlowNode } from "../lib/flow";
import { Button, Icon, Pill } from "./ui";

const byId = Object.fromEntries(NODES.map((n) => [n.id, n])) as Record<string, FlowNode>;

function anchor(a: FlowNode, b: FlowNode, dx = 0): { d: string; lx: number; ly: number } {
  const ac = { x: a.x + a.w / 2, y: a.y + a.h / 2 }, bc = { x: b.x + b.w / 2, y: b.y + b.h / 2 };
  // same row → horizontal
  if (Math.abs(a.y - b.y) < 20) {
    const right = bc.x > ac.x;
    const x1 = right ? a.x + a.w : a.x, x2 = right ? b.x : b.x + b.w;
    return { d: `M ${x1} ${ac.y} L ${x2} ${bc.y}`, lx: (x1 + x2) / 2, ly: ac.y - 9 };
  }
  // same column, going back up (answer loop) → bulge to the left
  if (b.y < a.y && Math.abs(a.x - b.x) < 20) {
    const x = a.x - 30;
    return { d: `M ${a.x} ${ac.y} C ${x} ${ac.y}, ${x} ${bc.y}, ${b.x} ${bc.y}`, lx: x - 4, ly: (ac.y + bc.y) / 2 };
  }
  // up and to the right → leave through the top, enter the target from the left
  if (b.y < a.y) {
    const x1 = a.x + a.w - 60;
    return { d: `M ${x1} ${a.y} C ${x1} ${bc.y}, ${x1 + 20} ${bc.y}, ${b.x} ${bc.y}`, lx: (x1 + b.x) / 2 + 8, ly: bc.y - 9 };
  }
  // down and to the right → leave through the bottom, enter the target from the left
  if (b.x >= a.x + a.w) {
    const x1 = ac.x, y1 = a.y + a.h;
    return { d: `M ${x1} ${y1} C ${x1} ${bc.y}, ${b.x - 60} ${bc.y}, ${b.x} ${bc.y}`, lx: (x1 + b.x) / 2, ly: bc.y - 9 };
  }
  const x1 = ac.x + dx, y1 = a.y + a.h, x2 = bc.x + dx, y2 = b.y;
  const d = Math.max(30, (y2 - y1) / 2);
  return { d: `M ${x1} ${y1} C ${x1} ${y1 + d}, ${x2} ${y2 - d}, ${x2} ${y2}`, lx: (x1 + x2) / 2, ly: (y1 + y2) / 2 };
}

const NODE_COLOR: Record<string, { fill: string; stroke: string; text: string }> = {
  entry: { fill: "#111a30", stroke: "#334166", text: "#e2e8f0" },
  models: { fill: "#1d1740", stroke: "#7c6ae6", text: "#ede9fe" },
  policy: { fill: "#0b2a26", stroke: "#2dd4a4", text: "#d1fae5" },
  exec: { fill: "#0c2236", stroke: "#38bdf8", text: "#e0f2fe" },
  stop: { fill: "#3a1220", stroke: "#fb7185", text: "#ffe4e6" },
  warn: { fill: "#3a2a0c", stroke: "#fbbf24", text: "#fef3c7" },
  ok: { fill: "#0b2a26", stroke: "#34d399", text: "#d1fae5" },
};
const nodeColor = (n: FlowNode) => NODE_COLOR[n.kind && n.kind !== "normal" ? n.kind : n.lane] ?? NODE_COLOR.entry;

type Phase = { type: "dwell" | "travel"; idx: number; start: number; end: number };

export function FlowChart() {
  const [sid, setSid] = useState(FLOW_SCENARIOS[0].id);
  const [playing, setPlaying] = useState(true);
  const [time, setTime] = useState(0);
  const [picked, setPicked] = useState<string | null>(null);
  const [speed, setSpeed] = useState(1);
  const scenario = FLOW_SCENARIOS.find((s) => s.id === sid)!;
  const pathRefs = useRef<Record<string, SVGPathElement | null>>({});
  const [lens, setLens] = useState<Record<string, number>>({});

  const geo = useMemo(() => Object.fromEntries(EDGES.map((e) => [`${e.from}>${e.to}`, anchor(byId[e.from], byId[e.to], e.dx)])), []);
  useEffect(() => {
    const l: Record<string, number> = {};
    for (const k of Object.keys(pathRefs.current)) { const el = pathRefs.current[k]; if (el) l[k] = el.getTotalLength(); }
    setLens(l);
  }, []);

  const pairs = useMemo(() => scenario.path.slice(0, -1).map((a, i) => `${a}>${scenario.path[i + 1]}`), [scenario]);
  const schedule = useMemo<Phase[]>(() => {
    const out: Phase[] = []; let t = 0;
    for (let i = 0; i < scenario.path.length; i++) {
      out.push({ type: "dwell", idx: i, start: t, end: t + 1500 }); t += 1500;
      if (i < pairs.length) { const dur = Math.max(450, ((lens[pairs[i]] ?? 200) / 330) * 1000); out.push({ type: "travel", idx: i, start: t, end: t + dur }); t += dur; }
    }
    return out;
  }, [scenario, pairs, lens]);
  const total = schedule[schedule.length - 1]?.end ?? 1;

  useEffect(() => { setTime(0); setPicked(null); }, [sid]);
  useEffect(() => {
    if (!playing) return;
    let raf = 0, last = performance.now();
    const tick = (now: number) => {
      setTime((t) => { const n = t + (now - last) * speed; return n >= total + 2500 ? 0 : n; });
      last = now; raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, total, speed]);

  const ph = schedule.find((p) => time >= p.start && time < p.end) ?? schedule[schedule.length - 1];
  const reachedIdx = ph.type === "dwell" ? ph.idx : ph.idx; // nodes up to idx are "visited"
  const activeNode = ph.type === "dwell" ? scenario.path[ph.idx] : scenario.path[ph.idx];
  const pathSet = new Set(scenario.path);
  const visited = new Set(scenario.path.slice(0, reachedIdx + (ph.type === "travel" ? 1 : 1)));

  // The dot rides along the arrows and fades out as it enters a box / back in as it leaves, so it never jumps or sits on the text.
  const edgePoint = (pairIdx: number, end: boolean) => {
    const el = pathRefs.current[pairs[pairIdx]], L = lens[pairs[pairIdx]];
    if (!el || !L) return null;
    const pt = el.getPointAtLength(end ? L : 0);
    return { x: pt.x, y: pt.y };
  };
  const ease = (f: number) => (f < 0.5 ? 2 * f * f : 1 - Math.pow(-2 * f + 2, 2) / 2);
  let packet: { x: number; y: number; o: number } | null = null;
  if (ph.type === "travel") {
    const el = pathRefs.current[pairs[ph.idx]], L = lens[pairs[ph.idx]];
    if (el && L) { const pt = el.getPointAtLength(ease((time - ph.start) / (ph.end - ph.start)) * L); packet = { x: pt.x, y: pt.y, o: 1 }; }
  } else {
    const entry = ph.idx > 0 ? edgePoint(ph.idx - 1, true) : null;
    const exit = ph.idx < pairs.length ? edgePoint(ph.idx, false) : null;
    const t = time - ph.start, dur = ph.end - ph.start;
    if (exit && t > dur - 350) packet = { ...exit, o: Math.min(1, (t - (dur - 350)) / 350) };
    else if (entry) packet = { ...entry, o: Math.max(0, 1 - t / 350) };
    else if (exit) packet = { ...exit, o: 1 };
  }

  const infoId = picked ?? activeNode;
  const info = NODE_INFO[infoId];
  const node = byId[infoId];
  const note = scenario.notes[infoId];
  const finished = time >= total;
  const edgeOn = (e: FlowEdge) => pairs.includes(`${e.from}>${e.to}`);
  const edgeDone = (e: FlowEdge) => { const i = pairs.indexOf(`${e.from}>${e.to}`); return i >= 0 && (i < ph.idx || (i === ph.idx && ph.type === "travel" && false) || finished); };

  return (
    <div>
      <div className="mb-4 flex flex-wrap gap-2" role="tablist" aria-label="Scenario">
        {FLOW_SCENARIOS.map((s) => (
          <button key={s.id} role="tab" aria-selected={s.id === sid} onClick={() => { setSid(s.id); setPlaying(true); }}
            className={`rounded-xl border px-3 py-2 text-left text-sm transition ${s.id === sid ? "border-emerald-400/70 bg-emerald-500/10 text-slate-50" : "border-line bg-panel/70 text-slate-300 hover:border-slate-500"}`}>
            <span className="block font-medium">{s.title}</span>
          </button>
        ))}
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-3 rounded-xl border border-line bg-slate-950/50 px-4 py-3">
        <span className="text-sm text-slate-400">You type:</span><span className="rounded-lg bg-slate-800 px-2.5 py-1 font-mono text-sm text-slate-100">{scenario.text}</span>
        <span className="text-slate-500"><Icon name="arrow" /></span>
        <Pill tone={scenario.tone}>{scenario.outcome}</Pill>
        <span className="ml-auto flex items-center gap-2">
          <Button size="sm" tone="ghost" icon={playing ? "pause" : "play"} onClick={() => setPlaying((p) => !p)}>{playing ? "Pause" : "Play"}</Button>
          <Button size="sm" tone="ghost" icon="refresh" onClick={() => { setTime(0); setPlaying(true); }}>Replay</Button>
          <label className="flex items-center gap-1 text-xs text-slate-400">Speed
            <select value={speed} onChange={(e) => setSpeed(Number(e.target.value))} className="rounded-md border border-slate-700 bg-slate-900 px-1.5 py-1 text-slate-200">
              <option value={0.6}>0.6×</option><option value={1}>1×</option><option value={1.6}>1.6×</option>
            </select>
          </label>
        </span>
      </div>

      <div className="space-y-4">
        <div className="overflow-x-auto rounded-2xl border border-line bg-ink p-2">
          <svg viewBox="-60 0 1330 920" className="min-w-[1000px]" role="img" aria-label="Flowchart of how a request travels through FinGuard">
            <defs><marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="currentColor" /></marker></defs>
            {LANES.map((l) => (
              <g key={l.id}>
                <rect x={l.x} y={110} width={l.w} height={780} rx={22} className={l.cls} strokeWidth={1} />
                <text x={l.x + 18} y={868} className="fill-slate-300" fontSize="12.5" fontWeight="700" letterSpacing="1.4">{l.label}</text>
                <text x={l.x + l.w - 18} y={868} textAnchor="end" className="fill-slate-500" fontSize="11.5">{l.sub}</text>
              </g>
            ))}
            {EDGES.map((e) => {
              const k = `${e.from}>${e.to}`, g = geo[k], on = edgeOn(e), done = edgeDone(e);
              return (
                <g key={k} className={on ? (done ? "text-emerald-300" : "text-sky-300") : "text-slate-600"} opacity={on ? 1 : 0.45}>
                  <path ref={(el) => { pathRefs.current[k] = el; }} d={g.d} fill="none" stroke="currentColor" strokeWidth={on ? 2.6 : 1.4} strokeDasharray={e.dashed || (on && !done) ? "6 6" : undefined} markerEnd="url(#arr)" className={on && !done ? "flow-edge-live" : ""} />
                </g>
              );
            })}
            {NODES.map((n) => {
              const c = nodeColor(n), onPath = pathSet.has(n.id), isActive = n.id === activeNode && !finished, isVisited = visited.has(n.id);
              return (
                <g key={n.id} onClick={() => { setPicked(n.id); setPlaying(false); }} className="cursor-pointer" opacity={onPath ? 1 : 0.38} role="button" tabIndex={0} aria-label={`${n.title}: ${n.sub}`}
                  onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { setPicked(n.id); setPlaying(false); } }}>
                  <rect x={n.x} y={n.y} width={n.w} height={n.h} rx={14} fill={c.fill} stroke={isActive ? "#fff" : c.stroke} strokeWidth={isActive ? 3 : isVisited ? 2.2 : 1.4} />
                  {n.n && <><circle cx={n.x + 22} cy={n.y + 24} r={11} fill={c.stroke} /><text x={n.x + 22} y={n.y + 28.5} textAnchor="middle" fontSize="12" fontWeight="700" fill="#07101f">{n.n}</text></>}
                  <text x={n.x + (n.n ? 42 : 16)} y={n.y + 29} fontSize="16" fontWeight="650" fill={c.text}>{n.title}</text>
                  <text x={n.x + 16} y={n.y + 55} fontSize="12.5" fill={c.text} opacity={0.78}>{n.sub.length > 52 ? n.sub.slice(0, 51) + "…" : n.sub}</text>
                  {isVisited && onPath && !isActive && <circle cx={n.x + n.w - 16} cy={n.y + 16} r={8} fill="#10b981" />}
                  {isVisited && onPath && !isActive && <path d={`M ${n.x + n.w - 20} ${n.y + 16} l 3 3 l 6 -6`} stroke="#04130e" strokeWidth={2} fill="none" strokeLinecap="round" />}
                </g>
              );
            })}
            {EDGES.filter((e) => e.label).map((e) => {
              const g = geo[`${e.from}>${e.to}`], on = edgeOn(e);
              return <text key={`l-${e.from}>${e.to}`} x={g.lx} y={g.ly} textAnchor="middle" fontSize="11" className={on ? "fill-slate-100" : "fill-slate-500"} paintOrder="stroke" stroke="#070b16" strokeWidth={5} strokeLinejoin="round" pointerEvents="none">{e.label}</text>;
            })}
            {packet && !finished && packet.o > 0.02 && <g opacity={packet.o}><circle cx={packet.x} cy={packet.y} r={13} fill="#38bdf8" opacity={0.25} /><circle cx={packet.x} cy={packet.y} r={7} fill="#7dd3fc" stroke="#fff" strokeWidth={2} /></g>}
          </svg>
        </div>

        <aside className="grid gap-4 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]" aria-live="polite">
          <div className="rounded-2xl border border-line bg-panel/80 p-5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold uppercase tracking-[0.12em] text-slate-400">{picked ? "You selected" : finished ? "Result" : "Now"}</span>
              <Pill tone={info.who.startsWith("Models") ? "info" : info.who.startsWith("Policy") ? "ok" : info.who.startsWith("Executors") ? "exec" : "mute"}>{info.who}</Pill>
            </div>
            <h3 className="mt-2 text-lg font-semibold text-slate-50">{node.n ? `${node.n} · ` : ""}{node.title}</h3>
            {note && <p className="mt-2 rounded-lg border border-emerald-500/30 bg-emerald-500/[0.08] px-3 py-2 text-sm text-emerald-100"><b>In this scenario:</b> {note}</p>}
            {!note && !pathSet.has(infoId) && <p className="mt-2 text-sm text-slate-400">This step isn't used in this scenario.</p>}
            <p className="mt-3 text-sm text-slate-300">{info.what}</p>
            <p className="mt-3 text-sm"><b className="text-slate-100">If it fails: </b><span className="text-slate-300">{info.fails}</span></p>
            <p className="mt-3 text-xs text-slate-500">Code: <span className="font-mono text-slate-400">{info.code}</span></p>
            {picked && <div className="mt-3"><Button size="sm" tone="ghost" onClick={() => { setPicked(null); setPlaying(true); }}>Resume the animation</Button></div>}
          </div>
          <div className="rounded-2xl border border-line bg-panel/80 p-5">
            <span className="text-xs font-semibold uppercase tracking-[0.12em] text-slate-400">This scenario, step by step</span>
            <ol className="mt-3 space-y-1.5">
              {scenario.path.map((id, i) => {
                const n = byId[id]; const cur = id === activeNode && !finished;
                return (
                  <li key={id}><button onClick={() => { setPicked(id); setPlaying(false); }} className={`flex w-full items-start gap-2 rounded-lg px-2 py-1 text-left text-sm transition hover:bg-slate-800/60 ${cur ? "bg-slate-800 text-slate-50" : visited.has(id) ? "text-slate-200" : "text-slate-500"}`}>
                    <span className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-xs ${visited.has(id) && !cur ? "bg-emerald-500/25 text-emerald-300" : cur ? "bg-sky-500/30 text-sky-200" : "bg-slate-800 text-slate-500"}`}>{visited.has(id) && !cur ? "✓" : i + 1}</span>
                    <span>{n.title}</span></button></li>
                );
              })}
            </ol>
            <p className="mt-3 text-xs text-slate-400">Click any box in the chart, or any step here, to read what happens there. Dimmed boxes aren't used in this scenario.</p>
          </div>
        </aside>
      </div>
    </div>
  );
}
