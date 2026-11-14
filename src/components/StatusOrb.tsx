import { Loader2, ShieldAlert, ShieldX, ShieldCheck, StopCircle } from "lucide-react";
import type { SupervisorState } from "../types";

const STATE_META: Record<
  SupervisorState,
  { label: string; color: string; glow: string; pulse: boolean }
> = {
  stopped: {
    label: "Отключено",
    color: "text-neutral-400",
    glow: "shadow-[0_0_40px_rgba(120,130,140,0.15)]",
    pulse: false,
  },
  starting: {
    label: "Подключение…",
    color: "text-amber-400",
    glow: "shadow-[0_0_50px_rgba(245,158,11,0.35)]",
    pulse: true,
  },
  running: {
    label: "Подключено",
    color: "text-emerald-400",
    glow: "shadow-[0_0_55px_rgba(34,197,94,0.4)]",
    pulse: true,
  },
  stopping: {
    label: "Отключение…",
    color: "text-amber-400",
    glow: "shadow-[0_0_40px_rgba(245,158,11,0.25)]",
    pulse: false,
  },
  error: {
    label: "Ошибка",
    color: "text-red-400",
    glow: "shadow-[0_0_50px_rgba(220,38,38,0.35)]",
    pulse: false,
  },
};

function StateIcon({ state }: { state: SupervisorState }) {
  switch (state) {
    case "running":
      return <ShieldCheck size={56} strokeWidth={1.5} />;
    case "starting":
    case "stopping":
      return <Loader2 size={56} strokeWidth={1.5} className="animate-spin" />;
    case "error":
      return <ShieldX size={56} strokeWidth={1.5} />;
    case "stopped":
    default:
      return <StopCircle size={56} strokeWidth={1.5} className="opacity-80" />;
  }
}

function formatUptime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}:${String(m % 60).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** The big engine status indicator. */
export default function StatusOrb({
  state,
  uptimeSecs,
  pid,
}: {
  state: SupervisorState;
  uptimeSecs: number;
  pid: number | null;
}) {
  const meta = STATE_META[state];
  return (
    <div className="flex flex-col items-center gap-3">
      <div
        className={`orb h-28 w-28 border-2 border-current ${meta.color} ${meta.glow} ${meta.pulse ? "orb-pulse" : ""}`}
      >
        <StateIcon state={state} />
      </div>
      <div className={`text-xl font-bold ${meta.color}`}>{meta.label}</div>
      {state === "running" && (
        <div className="flex items-center gap-2 text-xs text-neutral-400">
          <span>{formatUptime(uptimeSecs)}</span>
          {pid != null && <span>· PID {pid}</span>}
        </div>
      )}
    </div>
  );
}

export { STATE_META };
