import { useEffect, useState } from "react";
import { api } from "./api";
import { Console } from "./components/Console";
import { Dashboard } from "./components/Dashboard";
import type { DemoCommand } from "./components/Dashboard";
import { TraceViewer } from "./components/TraceViewer";
import { Pill } from "./components/ui";
import type { HealthResp } from "./types";

type Tab = "dashboard" | "console" | "trace";

export default function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [refresh, setRefresh] = useState(0);
  const [pending, setPending] = useState<DemoCommand | null>(null);
  const [traceId, setTraceId] = useState<string | null>(null);
  const [health, setHealth] = useState<HealthResp | null>(null);

  useEffect(() => { api.health().then(setHealth).catch(() => setHealth(null)); }, [refresh]);

  const bump = () => setRefresh((n) => n + 1);
  const tabs: [Tab, string][] = [["dashboard", "Dashboard"], ["console", "Agent Console"], ["trace", "Trace Viewer"]];
  const providers = (health?.providers ?? {}) as Record<string, string | null>;

  return (
    <div className="mx-auto flex min-h-full max-w-[1400px] flex-col gap-4 px-4 py-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">FinGuard <span className="font-normal text-slate-500">policy-controlled execution runtime for AI financial agents</span></h1>
          <p className="text-xs text-slate-500">Models propose. Policy decides. Executors execute. · synthetic sandbox, not a bank</p>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
          {health ? (<>
            <Pill tone={health.status === "ok" ? "ok" : "bad"}>API {health.status}</Pill>
            <Pill tone="mute">parser: {providers.intent_parser}</Pill>
            <Pill tone="mute">risk: {providers.decision_provider}</Pill>
            <Pill tone="mute">escalation: {providers.escalation_analyzer}</Pill>
          </>) : <Pill tone="bad">backend unreachable</Pill>}
        </div>
      </header>

      <nav className="flex gap-1 border-b border-slate-800">
        {tabs.map(([k, label]) => (
          <button key={k} onClick={() => setTab(k)} className={`-mb-px border-b-2 px-4 py-2 text-sm transition ${tab === k ? "border-emerald-400 text-slate-100" : "border-transparent text-slate-400 hover:text-slate-200"}`}>{label}</button>
        ))}
      </nav>

      <main className="pb-16">
        {tab === "dashboard" && <Dashboard refreshKey={refresh} onChanged={bump} onRun={(c) => { setPending(c); setTab("console"); }} />}
        <div hidden={tab !== "console"}>
          <Console pending={pending} onPendingDone={() => setPending(null)} onChanged={bump}
            onOpenTrace={(id) => { setTraceId(id); setTab("trace"); }} />
        </div>
        {tab === "trace" && <TraceViewer actionId={traceId} onSelect={setTraceId} />}
      </main>
    </div>
  );
}
