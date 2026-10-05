import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { DemoControls, ModePills } from "./components/DemoControls";
import { Icon, ToastProvider, useLocalStorage } from "./components/ui";
import { How } from "./pages/How";
import { Overview } from "./pages/Overview";
import { Trace } from "./pages/Trace";
import { Try } from "./pages/Try";
import type { DemoCommand } from "./pages/Try";
import { useRoute } from "./lib/route";
import type { TourId } from "./lib/scenarios";
import type { HealthResp } from "./types";

const NAV: { to: string; page: string; label: string; icon: string }[] = [
  { to: "#/", page: "home", label: "Overview", icon: "list" },
  { to: "#/try", page: "try", label: "Try it", icon: "play" },
  { to: "#/how", page: "how", label: "How it works", icon: "flow" },
  { to: "#/trace", page: "trace", label: "Trace viewer", icon: "clock" },
];

export default function App() {
  const [route, go] = useRoute();
  const [refresh, setRefresh] = useState(0);
  const [pending, setPending] = useState<DemoCommand | null>(null);
  const [health, setHealth] = useState<HealthResp | null>(null);
  const [done, setDone] = useLocalStorage<Record<string, boolean>>("fg_tour_v1", {});
  const bump = useCallback(() => setRefresh((n) => n + 1), []);
  const mark = useCallback((id: TourId) => setDone({ ...done, [id]: true }), [done, setDone]);

  useEffect(() => { api.health().then(setHealth).catch(() => setHealth(null)); }, [refresh]);
  const providers = (health?.providers ?? {}) as Record<string, string | null>;
  const clearPending = useCallback(() => setPending(null), []);

  const run = (c: DemoCommand) => { setPending(c); go("#/try"); };
  const openTrace = (id: string) => { mark("trace"); go(`#/trace/${id}`); };

  return (
    <ToastProvider>
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded-lg focus:bg-slate-100 focus:px-3 focus:py-2 focus:text-slate-900">Skip to content</a>
      <div className="mx-auto flex min-h-screen max-w-[1400px] flex-col gap-5 px-4 py-4 sm:px-6">
        <header className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3">
          <a href="#/" className="flex items-center gap-3">
            <span className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-400 text-slate-950"><Icon name="shield" className="h-6 w-6" stroke={2.4} /></span>
            <span><span className="block text-lg font-semibold leading-tight tracking-tight text-slate-50">FinGuard</span><span className="block text-xs text-slate-400">Models propose · Policy decides · Executors execute</span></span>
          </a>
          <div className="flex flex-wrap items-center gap-2">
            {health ? <ModePills providers={providers} /> : <span className="rounded-full bg-rose-500/15 px-2.5 py-0.5 text-xs text-rose-300 ring-1 ring-inset ring-rose-500/30">backend unreachable</span>}
            {health && <DemoControls providers={providers} onChanged={bump} />}
          </div>
        </header>

        <nav aria-label="Main" className="flex gap-1 overflow-x-auto border-b border-line">
          {NAV.map((n) => {
            const active = route.page === n.page;
            return (
              <a key={n.to} href={n.to} aria-current={active ? "page" : undefined}
                className={`-mb-px flex items-center gap-2 whitespace-nowrap border-b-2 px-4 py-2.5 text-sm transition ${active ? "border-emerald-400 font-medium text-slate-50" : "border-transparent text-slate-400 hover:text-slate-100"}`}>
                <Icon name={n.icon} className="h-4 w-4" />{n.label}
              </a>
            );
          })}
        </nav>

        <main id="main" className="flex-1 pb-16">
          {route.page === "home" && <Overview refreshKey={refresh} done={done} onRun={run} onGo={go} onChanged={bump} />}
          <div hidden={route.page !== "try"}>
            <Try pending={pending} onPendingDone={clearPending} onChanged={bump} onTrace={openTrace} onOutcome={mark} />
          </div>
          {route.page === "how" && <How onGo={go} />}
          {route.page === "trace" && <Trace actionId={route.id ?? null} onSelect={(id) => go(`#/trace/${id}`)} onBack={() => go("#/try")} />}
        </main>

        <footer className="border-t border-line pt-4 text-xs text-slate-500">Synthetic sandbox — not a bank, not real money, no fraud-detection claims. Thresholds are uncalibrated demo values.</footer>
      </div>
    </ToastProvider>
  );
}
