import {
  Check,
  ExternalLink,
  Globe,
  Image,
  RotateCcw,
  Send,
  X,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { api } from "../api";
import type { AppSettings } from "../types";

const FULL_VERSION_URL =
  "https://github.com/MixasV/zapret2-gui/tree/fix/graceful-exit-and-bridge-cooldowns";

export { FULL_VERSION_URL };

const BG_LABELS: Record<string, string> = {
  "art-tank": "Art Tank",
  reforged: "Reforged",
  "battle-pass": "Battle Pass",
  characters: "Characters",
  "field-containers": "Field Containers",
  "banner-clean": "Banner",
};

/** Settings slide-over: background gallery, domain list, engine options. */
export default function SettingsPanel({
  settings,
  backgrounds,
  onClose,
  onSaved,
}: {
  settings: AppSettings;
  backgrounds: string[];
  onClose: () => void;
  onSaved: (s: AppSettings) => void;
}) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const persist = async (next: AppSettings) => {
    setSaving(true);
    setError(null);
    try {
      await api.saveSettings(next);
      setDraft(next);
      onSaved(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  // Background choice applies instantly (pure visual preference).
  const pickBackground = (id: string | null) => {
    void persist({ ...draft, background_id: id });
  };

  return (
    <div className="slide-over fixed inset-0 z-40 flex justify-end bg-black/50">
      <div className="glass thin-scroll m-3 w-[400px] max-w-[calc(100vw-24px)] overflow-y-auto p-4">
        {/* Header */}
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-sm font-bold tracking-wide text-neutral-100">
            НАСТРОЙКИ
          </h2>
          <button
            className="rounded-md p-1 text-neutral-400 transition-colors hover:bg-white/10 hover:text-white"
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </div>

        {error && (
          <div className="mb-3 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-300">
            {error}
          </div>
        )}

        {/* Background */}
        <Section title="Фоновое изображение (WoT Blitz)" icon={<Image size={13} />}>
          <div className="grid grid-cols-3 gap-2">
            <Thumb
              active={draft.background_id == null}
              onClick={() => pickBackground(null)}
              label="Нет"
            />
            {backgrounds.map((id) => (
              <Thumb
                key={id}
                id={id}
                label={BG_LABELS[id] ?? id}
                active={draft.background_id === id}
                onClick={() => pickBackground(id)}
              />
            ))}
          </div>
          <p className="mt-2 text-[11px] leading-snug text-neutral-500">
            Официальный арт World of Tanks Blitz © Wargaming.net. Изображения
            загружены с официального сайта игры и используются локально.
          </p>
        </Section>

        {/* Domains — read-only info (editor removed per user request) */}
        <Section title="Домены обхода" icon={<Globe size={13} />}>
          <p className="text-[11px] leading-relaxed text-neutral-400">
            Домены задаются файлами в <span className="font-mono">bin/lists/</span>:
            <span className="font-mono text-neutral-300"> wot.txt</span> (Wargaming),
            <span className="font-mono text-neutral-300"> list-cloudflare.txt</span>,
            <span className="font-mono text-neutral-300"> list-general.txt</span>,
            <span className="font-mono text-neutral-300"> list-google.txt</span>,
            <span className="font-mono text-neutral-300"> list-misc.txt</span>.
            Игровые подсети — <span className="font-mono">lists/ipset-all.txt</span>.
          </p>
        </Section>

        {/* Auto hostlist */}
        <Section title="Движок" icon={<RotateCcw size={13} />}>
          <Toggle
            label="Авто-хостлист (--hostlist-auto)"
            hint="Движок сам дописывает похожие домены в wot-auto.txt"
            checked={draft.auto_hostlist_enabled}
            onChange={(v) => {
              const next = { ...draft, auto_hostlist_enabled: v };
              setDraft(next);
              void persist(next);
            }}
          />
          <div className="mt-2 text-[11px] text-neutral-500">
            TCP-порты перехвата: {draft.intercept_ports_tcp.join(", ")}
          </div>
        </Section>

        {/* About */}
        <div className="mt-4 border-t border-white/5 pt-3">
          <p className="text-[13px] leading-relaxed text-neutral-200">
            Хотите так же смотреть YouTube, сидеть в Discord и других сайтах
            без VPN? Используйте бесплатно полную версию Zapret2:
          </p>
          <button
            className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs font-medium text-amber-300 transition-colors hover:bg-amber-500/20"
            onClick={() => void openUrl(FULL_VERSION_URL)}
          >
            <ExternalLink size={13} />
            Открыть полную версию Zapret2
          </button>
          <p className="mt-2 break-all text-[11px] text-neutral-500">
            {FULL_VERSION_URL}
          </p>

          <div className="mt-3 flex flex-col gap-1.5">
            <button
              className="glass-row flex items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-white/10"
              onClick={() => void openUrl("https://mixas.pro")}
            >
              <Globe size={14} className="shrink-0 text-sky-400" />
              <span className="text-xs text-neutral-200">mixas.pro</span>
              <span className="ml-auto text-[11px] text-neutral-500">сайт автора</span>
            </button>
            <button
              className="glass-row flex items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-white/10"
              onClick={() => void openUrl("https://mixaseth.t.me")}
            >
              <Send size={14} className="shrink-0 text-sky-400" />
              <span className="text-xs text-neutral-200">mixaseth.t.me</span>
              <span className="ml-auto text-[11px] text-neutral-500">телеграм автора</span>
            </button>
          </div>
        </div>

        <div className="mt-4 border-t border-white/5 pt-3 text-[11px] leading-relaxed text-neutral-400">
          WoT Blitz Connector 1.0.0 · неофициальная урезанная утилита для игры
          на серверах Wargaming. Не связана с Wargaming.net. Движок: winws
          (zapret) + WinDivert.
        </div>
      </div>
    </div>
  );
}

function Section({
  title,
  icon,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-4">
      <div className="mb-2 flex items-center gap-1.5 text-[13px] font-semibold tracking-wide text-neutral-300">
        <span className="text-amber-400">{icon}</span>
        {title.toUpperCase()}
      </div>
      {children}
    </div>
  );
}

function Thumb({
  id,
  label,
  active,
  onClick,
}: {
  id?: string;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`relative aspect-video overflow-hidden rounded-lg border-2 transition-all ${
        active
          ? "border-amber-400 shadow-[0_0_16px_rgba(245,158,11,0.4)]"
          : "border-transparent hover:border-white/30"
      }`}
    >
      {id ? (
        <img src={`/bg/${id}.jpg`} alt={label} className="h-full w-full object-cover" />
      ) : (
        <div className="flex h-full w-full items-center justify-center bg-neutral-800 text-[10px] text-neutral-400">
          Нет
        </div>
      )}
      {active && (
        <div className="absolute right-1 top-1 rounded-full bg-amber-400 p-0.5 text-black">
          <Check size={10} />
        </div>
      )}
      <div className="absolute inset-x-0 bottom-0 bg-black/60 px-1 py-0.5 text-center text-[9px] text-white">
        {label}
      </div>
    </button>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className="glass-row flex w-full items-center justify-between px-2.5 py-2 text-left transition-colors hover:bg-white/8"
      onClick={() => onChange(!checked)}
    >
      <span>
        <span className="block text-xs text-neutral-200">{label}</span>
        {hint && <span className="block text-[11px] text-neutral-500">{hint}</span>}
      </span>
      <span
        className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
          checked ? "bg-emerald-500" : "bg-neutral-600"
        }`}
      >
        <span
          className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${
            checked ? "left-[18px]" : "left-0.5"
          }`}
        />
      </span>
    </button>
  );
}
