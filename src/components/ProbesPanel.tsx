import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Loader2,
  ShieldCheck,
} from "lucide-react";
import type { ProbeResult } from "../types";

const STATUS_STYLE: Record<string, { dot: string; text: string }> = {
  operational: { dot: "text-emerald-400", text: "text-emerald-300" },
  degraded: { dot: "text-amber-400", text: "text-amber-300" },
  blocked: { dot: "text-red-400", text: "text-red-300" },
  checking: { dot: "text-neutral-400", text: "text-neutral-400" },
};

function StatusIcon({ status }: { status: string }) {
  switch (status) {
    case "operational":
      return <CheckCircle2 size={15} className="text-emerald-400" />;
    case "degraded":
      return <AlertTriangle size={15} className="text-amber-400" />;
    case "blocked":
      return <AlertTriangle size={15} className="text-red-400" />;
    default:
      return <Loader2 size={15} className="animate-spin text-neutral-400" />;
  }
}

/** Probe result rows: per-region Wargaming server reachability. */
export default function ProbesPanel({
  probes,
  onRefresh,
  refreshing,
  engineRunning,
}: {
  probes: ProbeResult[];
  onRefresh: () => void;
  refreshing: boolean;
  engineRunning: boolean;
}) {
  return (
    <div className="glass p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-[13px] font-semibold tracking-wide text-neutral-300">
          <Activity size={15} className="text-amber-400" />
          СЕРВЕРЫ WARGAMING
        </div>
        <button
          className="text-xs text-neutral-400 transition-colors hover:text-amber-300"
          onClick={onRefresh}
          disabled={refreshing}
        >
          {refreshing ? "Проверка…" : "Обновить"}
        </button>
      </div>
      <div className="flex flex-col gap-2">
        {probes.length === 0 && (
          <div className="py-3 text-center text-sm text-neutral-500">
            <Loader2 size={16} className="mx-auto mb-1 animate-spin" />
            Проверка серверов…
          </div>
        )}
        {probes.map((p) => {
          const style = STATUS_STYLE[p.metric.status] ?? STATUS_STYLE.checking;
          return (
            <div
              key={p.id}
              className="glass-row flex items-center justify-between px-3 py-2"
            >
              <div className="flex items-center gap-2.5">
                <StatusIcon status={p.metric.status} />
                <span className="text-[13px] text-neutral-100">{p.label}</span>
                {p.metric.ip_address && (
                  <span className="hidden text-[11px] text-neutral-500 sm:inline">
                    {p.metric.ip_address}
                  </span>
                )}
              </div>
              <span className={`text-[13px] font-semibold ${style.text}`}>
                {p.metric.latency_ms != null
                  ? `${p.metric.latency_ms} мс`
                  : p.metric.status === "blocked"
                    ? "недоступен"
                    : "…"}
              </span>
            </div>
          );
        })}
      </div>
      <div className="mt-3 flex items-start gap-1.5 text-[11px] leading-snug text-neutral-500">
        <ShieldCheck size={12} className="mt-0.5 shrink-0" />
        <span>
          {engineRunning
            ? "Движок активен: пробы идут через обход. Если сервер «недоступен» — включите обход ещё раз или подождите: провайдер сбрасывает первые соединения."
            : "Пробы идут напрямую (без обхода). Часть серверов может быть недоступна без него — это нормально."}
        </span>
      </div>
    </div>
  );
}
