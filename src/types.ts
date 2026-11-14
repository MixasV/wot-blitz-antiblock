// Types mirrored from the Rust backend (src-tauri/src).

export type ProbeStatus = "operational" | "degraded" | "blocked" | "checking";

export interface ProbeMetric {
  target: string;
  status: ProbeStatus;
  latency_ms: number | null;
  dns_ms: number | null;
  ip_address: string | null;
  message: string;
  last_checked_epoch: number;
}

export interface ProbeResult {
  id: string;
  label: string;
  metric: ProbeMetric;
}

export type SupervisorState =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "error";

export interface LogEntry {
  timestamp: number;
  level: string;
  message: string;
}

export interface SupervisorStatus {
  state: SupervisorState;
  pid: number | null;
  uptime_secs: number;
  binary_path: string | null;
  last_error: string | null;
}

export interface StatusResponse {
  is_elevated: boolean;
  supervisor: SupervisorStatus;
}

export interface AppSettings {
  server_domains: string[];
  intercept_ports_tcp: number[];
  auto_hostlist_enabled: boolean;
  background_id: string | null;
}
