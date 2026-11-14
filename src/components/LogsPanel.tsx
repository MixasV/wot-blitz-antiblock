import { Terminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { LogEntry } from "../types";

const LEVEL_COLOR: Record<string, string> = {
  info: "text-sky-300",
  warn: "text-amber-300",
  error: "text-red-300",
  stdout: "text-neutral-400",
  stderr: "text-neutral-500",
};

/** Collapsible mini log console fed by the supervisor ring buffer. */
export default function LogsPanel({
  logs,
  onClear,
}: {
  logs: LogEntry[];
  onClear: () => void;
}) {
  const [open, setOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, open]);

  const visible = open ? logs.slice(-200) : logs.slice(-3);

  return (
    <div className="glass overflow-hidden">
      <button
        className="flex w-full items-center justify-between px-4 py-2.5 text-[13px] font-semibold tracking-wide text-neutral-300 transition-colors hover:bg-white/5"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="flex items-center gap-2">
          <Terminal size={14} className="text-amber-400" />
          ЖУРНАЛ ДВИЖКА
        </span>
        <span className="flex items-center gap-3">
          {open && (
            <span
              className="text-xs font-normal text-neutral-400 hover:text-amber-300"
              onClick={(e) => {
                e.stopPropagation();
                onClear();
              }}
            >
              Очистить
            </span>
          )}
          <span className="text-neutral-500">{open ? "▾" : "▸"}</span>
        </span>
      </button>
      <div
        ref={scrollRef}
        className={`log-console px-4 ${open ? "thin-scroll max-h-40 overflow-y-auto pb-3" : "pb-3"}`}
      >
        {visible.length === 0 && (
          <div className="py-1 text-neutral-600">Журнал пуст</div>
        )}
        {visible.map((entry, i) => (
          <div key={`${entry.timestamp}-${i}`} className="whitespace-pre-wrap break-all">
            <span className="text-neutral-600">
              {new Date(entry.timestamp).toLocaleTimeString("ru-RU")}{" "}
            </span>
            <span className={LEVEL_COLOR[entry.level] ?? "text-neutral-400"}>
              {entry.message}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
