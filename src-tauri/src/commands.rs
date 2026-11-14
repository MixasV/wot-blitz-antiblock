//! Tauri IPC commands — the frontend's whole API surface.
//!
//! The stripped client exposes exactly: status, connect/stop, probes, logs,
//! settings (incl. background image), elevation helpers, and exit.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::engine::probes::{ProbeResult, WoTServerProbe};
use crate::engine::supervisor::{EventSink, LogEntry, Supervisor, SupervisorStatus};
use crate::platform::settings::{
    load_settings, save_settings, AppSettings, BACKGROUND_IDS,
};
use crate::platform::windows::{is_elevated, kill_stray_winws2_processes};

/// Managed application state.
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        let supervisor =
            Supervisor::new().map_err(|e| format!("init supervisor failed: {e}"))?;
        Ok(Self {
            supervisor: Arc::new(supervisor),
        })
    }
}

/// Forwards supervisor events to the webview as Tauri events.
pub struct TauriSink {
    app: tauri::AppHandle,
}

impl TauriSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriSink {
    fn emit_state_changed(&self, state: crate::engine::supervisor::SupervisorState) {
        let _ = self.app.emit("state-changed", state);
    }

    fn emit_log_batch(&self, batch: &[LogEntry]) {
        let _ = self.app.emit("log-batch", batch);
    }
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub is_elevated: bool,
    pub supervisor: SupervisorStatus,
}

fn status_response(state: &tauri::State<'_, AppState>) -> StatusResponse {
    StatusResponse {
        is_elevated: is_elevated(),
        supervisor: state.supervisor.get_status(),
    }
}

#[tauri::command]
pub fn get_status(state: tauri::State<'_, AppState>) -> StatusResponse {
    status_response(&state)
}

/// Start the bypass engine with the current settings.
#[tauri::command]
pub async fn connect(
    state: tauri::State<'_, AppState>,
) -> Result<StatusResponse, String> {
    let settings = load_settings();
    state
        .supervisor
        .start(&settings)
        .await
        .map_err(|e| e.to_string())?;
    Ok(status_response(&state))
}

/// Stop the bypass engine.
#[tauri::command]
pub async fn stop(state: tauri::State<'_, AppState>) -> Result<StatusResponse, String> {
    state.supervisor.stop().await?;
    Ok(status_response(&state))
}

/// Run the Wargaming endpoint probes now (DNS + TCP).
#[tauri::command]
pub async fn get_probe_results() -> Result<Vec<ProbeResult>, String> {
    Ok(WoTServerProbe::probe_all().await)
}

#[tauri::command]
pub fn get_logs(state: tauri::State<'_, AppState>) -> Vec<LogEntry> {
    state.supervisor.get_all_logs()
}

#[tauri::command]
pub fn clear_logs(state: tauri::State<'_, AppState>) {
    state.supervisor.clear_logs();
}

#[tauri::command]
pub fn get_app_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

#[tauri::command]
pub fn save_app_settings(settings: AppSettings) -> Result<(), String> {
    save_settings(settings)
}

/// Background image ids bundled with the app (`/bg/<id>.jpg`).
#[tauri::command]
pub fn list_backgrounds() -> Vec<String> {
    BACKGROUND_IDS.iter().map(|s| s.to_string()).collect()
}

#[tauri::command]
pub fn get_elevation() -> bool {
    is_elevated()
}

/// Kill leftover engine processes from a previous unclean exit.
#[tauri::command]
pub fn kill_stray_processes() -> usize {
    kill_stray_winws2_processes(None)
}

/// Relaunch the app elevated (UAC). The current instance exits on success.
/// Current arguments (e.g. `--bin-dir`) are forwarded; the marker prevents
/// an auto-elevation loop in the new instance.
#[tauri::command]
pub async fn relaunch_elevated(app: tauri::AppHandle) -> Result<(), String> {
    crate::platform::windows::relaunch_elevated(&["--no-auto-elevate".to_string()])?;
    app.exit(0);
    Ok(())
}

/// Stop the engine (bounded), clean the driver, then exit the process.
#[tauri::command]
pub async fn exit_app(app: tauri::AppHandle) -> Result<(), String> {
    graceful_exit(&app).await;
    Ok(())
}

/// Shared shutdown path for the exit command and the window close button.
///
/// Deliberately minimal: stop OUR engine (bounded) and exit. We do not kill
/// foreign engine processes and do not stop the shared WinDivert service —
/// both would break other DPI tools the user may be running.
pub async fn graceful_exit(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let supervisor = state.supervisor.clone();
        // Bounded: never hang the exit on a wedged engine.
        let _ = tokio::time::timeout(Duration::from_secs(6), supervisor.stop()).await;
    }
    app.exit(0);
}
