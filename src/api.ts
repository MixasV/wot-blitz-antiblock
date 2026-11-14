import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings,
  LogEntry,
  ProbeResult,
  StatusResponse,
  SupervisorState,
} from "./types";

export const api = {
  getStatus: () => invoke<StatusResponse>("get_status"),
  connect: () => invoke<StatusResponse>("connect"),
  stop: () => invoke<StatusResponse>("stop"),
  getProbeResults: () => invoke<ProbeResult[]>("get_probe_results"),
  getLogs: () => invoke<LogEntry[]>("get_logs"),
  clearLogs: () => invoke<void>("clear_logs"),
  getSettings: () => invoke<AppSettings>("get_app_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<void>("save_app_settings", { settings }),
  listBackgrounds: () => invoke<string[]>("list_backgrounds"),
  getElevation: () => invoke<boolean>("get_elevation"),
  relaunchElevated: () => invoke<void>("relaunch_elevated"),
  killStrayProcesses: () => invoke<number>("kill_stray_processes"),
  exitApp: () => invoke<void>("exit_app"),
};

export const onStateChanged = (cb: (state: SupervisorState) => void) =>
  listen<SupervisorState>("state-changed", (e) => cb(e.payload));

export const onLogBatch = (cb: (batch: LogEntry[]) => void) =>
  listen<LogEntry[]>("log-batch", (e) => cb(e.payload));
