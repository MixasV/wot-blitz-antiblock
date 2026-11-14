import { Settings, ShieldAlert, Zap } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, onLogBatch, onStateChanged } from "./api";
import LogsPanel from "./components/LogsPanel";
import ProbesPanel from "./components/ProbesPanel";
import SettingsPanel, { FULL_VERSION_URL } from "./components/SettingsPanel";
import StatusOrb, { STATE_META } from "./components/StatusOrb";
import TitleBar from "./components/TitleBar";
import type { AppSettings, LogEntry, ProbeResult, SupervisorState } from "./types";

const MAX_VIEW_LOGS = 400;

export default function App() {
  const [state, setState] = useState<SupervisorState>("stopped");
  const [isElevated, setIsElevated] = useState(true);
  const [uptimeSecs, setUptimeSecs] = useState(0);
  const [pid, setPid] = useState<number | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [probes, setProbes] = useState<ProbeResult[]>([]);
  const [probing, setProbing] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [backgrounds, setBackgrounds] = useState<string[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const refreshProbes = useCallback(async () => {
    setProbing(true);
    try {
      setProbes(await api.getProbeResults());
    } catch {
      // probe errors are non-fatal; keep the previous snapshot
    } finally {
      setProbing(false);
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await api.getStatus();
      setState(s.supervisor.state);
      setPid(s.supervisor.pid);
      setUptimeSecs(s.supervisor.uptime_secs);
      setLastError(s.supervisor.last_error);
      setIsElevated(s.is_elevated);
    } catch {
      // backend unreachable: leave the last known state on screen
    }
  }, []);

  // Initial load: status, settings, backgrounds, initial logs, first probes.
  useEffect(() => {
    void refreshStatus();
    void refreshProbes();
    void api.getSettings().then(setSettings).catch(() => {});
    void api.listBackgrounds().then(setBackgrounds).catch(() => {});
    void api.getLogs().then(setLogs).catch(() => {});
  }, [refreshStatus, refreshProbes]);

  // Backend events.
  useEffect(() => {
    const unsubs: Promise<() => void>[] = [
      onStateChanged((s) => {
        setState(s);
        // Probes are meaningful right after a state transition: they show
        // whether the target servers respond with the bypass active.
        void refreshProbes();
      }),
      onLogBatch((batch) => {
        setLogs((prev) => {
          const next = [...prev, ...batch];
          return next.length > MAX_VIEW_LOGS
            ? next.slice(next.length - MAX_VIEW_LOGS)
            : next;
        });
      }),
    ];
    return () => {
      unsubs.forEach((p) => void p.then((unlisten) => unlisten()));
    };
  }, [refreshProbes]);

  // Heartbeat (covers missed events) + uptime ticker.
  useEffect(() => {
    const beat = setInterval(() => void refreshStatus(), 2500);
    return () => clearInterval(beat);
  }, [refreshStatus]);

  useEffect(() => {
    if (state !== "running") return;
    const tick = setInterval(() => setUptimeSecs((v) => v + 1), 1000);
    return () => clearInterval(tick);
  }, [state]);

  const busy = state === "starting" || state === "stopping";
  const running = state === "running";

  const handleConnect = async () => {
    setActionError(null);
    try {
      const s = await api.connect();
      setState(s.supervisor.state);
      setIsElevated(s.is_elevated);
      void refreshProbes();
    } catch (e) {
      const msg = String(e);
      setActionError(msg.replace(/^ADMIN_REQUIRED:\s*/i, "").trim() || msg);
      void refreshStatus();
    }
  };

  const handleStop = async () => {
    setActionError(null);
    try {
      const s = await api.stop();
      setState(s.supervisor.state);
    } catch (e) {
      setActionError(String(e));
      void refreshStatus();
    }
  };

  const bgUrl =
    settings?.background_id != null ? `/bg/${settings.background_id}.jpg` : null;

  return (
    <div className="relative h-full">
      {/* Background image + darkening overlay */}
      {bgUrl && <div className="app-bg" style={{ backgroundImage: `url(${bgUrl})` }} />}
      <div className="app-bg-overlay" />

      <div className="relative z-10 flex h-full flex-col">
        <TitleBar />

        <main className="thin-scroll mx-auto flex w-full max-w-[470px] flex-1 flex-col gap-4 overflow-y-auto px-5 pb-5">
          {/* Header */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-amber-500 to-red-600 text-base font-black text-white shadow-lg">
                W
              </div>
              <div>
                <div className="text-base font-bold text-white">
                  WoT Blitz Connector
                </div>
                <div className="text-xs text-neutral-400">
                  DPI-обход для серверов Wargaming
                </div>
              </div>
            </div>
            <button
              className="rounded-xl p-2.5 text-neutral-300 transition-colors hover:bg-white/10 hover:text-amber-300"
              onClick={() => setSettingsOpen(true)}
              title="Настройки"
            >
              <Settings size={20} />
            </button>
          </div>

          {/* Elevation warning */}
          {!isElevated && (
            <div className="glass flex items-center gap-2.5 border border-amber-500/30 px-4 py-3 text-[13px] text-amber-300">
              <ShieldAlert size={17} className="shrink-0" />
              <span>Запущено без прав администратора — движку нужен UAC.</span>
              <button
                className="ml-auto shrink-0 rounded-lg bg-amber-500 px-3 py-1.5 text-xs font-semibold text-black hover:bg-amber-400"
                onClick={() => void api.relaunchElevated()}
              >
                Перезапустить
              </button>
            </div>
          )}

          {/* Status orb + controls */}
          <div className="glass flex flex-col items-center gap-4 px-5 py-6">
            <StatusOrb state={state} uptimeSecs={uptimeSecs} pid={pid} />
            {(lastError || actionError) && !running && (
              <div className="w-full rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-2.5 text-center text-[13px] leading-snug text-red-300">
                {actionError ?? lastError}
              </div>
            )}
            <div className="flex w-full gap-3">
              {running ? (
                <button
                  className="btn btn-stop flex-1 rounded-xl py-3.5 text-base text-white"
                  onClick={() => void handleStop()}
                  disabled={busy}
                >
                  {busy ? "Отключение…" : "Остановить"}
                </button>
              ) : (
                <button
                  className="btn btn-connect flex-1 rounded-xl py-3.5 text-base text-white"
                  onClick={() => void handleConnect()}
                  disabled={busy}
                >
                  {state === "starting" ? "Подключение…" : "Подключиться"}
                </button>
              )}
            </div>
            <div className="text-xs text-neutral-500">
              Стратегия Zapret2 · домены Wargaming · десинк TCP + игровых UDP-портов
            </div>
          </div>

          {/* Probes */}
          <ProbesPanel
            probes={probes}
            onRefresh={() => void refreshProbes()}
            refreshing={probing}
            engineRunning={running}
          />

          {/* Logs */}
          <LogsPanel
            logs={logs}
            onClear={() => {
              void api.clearLogs();
              setLogs([]);
            }}
          />

          {/* Full-version promo — visible right on the main screen */}
          <button
            className="glass-row group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:border-amber-500/40 hover:bg-amber-500/10"
            onClick={() => void openUrl(FULL_VERSION_URL)}
          >
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-amber-500 to-red-600 text-white">
              <Zap size={17} />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-[13px] font-semibold text-neutral-100">
                Хотите так же смотреть YouTube, сидеть в Discord и других
                сайтах без VPN?
              </span>
              <span className="block text-xs text-neutral-400">
                Используйте бесплатно полную версию Zapret2
              </span>
            </span>
            <span className="shrink-0 text-xs font-semibold text-amber-400 group-hover:text-amber-300">
              Открыть →
            </span>
          </button>
        </main>
      </div>

      {/* Settings slide-over */}
      {settingsOpen && settings && (
        <SettingsPanel
          settings={settings}
          backgrounds={backgrounds}
          onClose={() => setSettingsOpen(false)}
          onSaved={(s) => setSettings(s)}
        />
      )}
    </div>
  );
}
