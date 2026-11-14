import { Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";

/** Frameless-window title bar: drag region + minimize/close. */
export default function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-end"
    >
      <button
        className="flex h-9 w-10 items-center justify-center text-neutral-400 transition-colors hover:bg-white/10 hover:text-white"
        onClick={() => void getCurrentWindow().minimize()}
        title="Свернуть"
      >
        <Minus size={14} />
      </button>
      <button
        className="flex h-9 w-10 items-center justify-center text-neutral-400 transition-colors hover:bg-red-600 hover:text-white"
        onClick={() => void api.exitApp()}
        title="Закрыть (полный выход)"
      >
        <X size={14} />
      </button>
    </div>
  );
}
