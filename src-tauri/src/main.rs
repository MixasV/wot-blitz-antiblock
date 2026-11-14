//! WoT Blitz Connector — minimal DPI-bypass client for World of Tanks Blitz
//! and World of Tanks (Wargaming).
//!
//! Architecture:
//! - `engine`   — Supervisor (winws2 lifecycle, log ring buffer) + WoT server probes.
//! - `platform` — OS-specific helpers (binary resolution, process spawn, WinDivert cleanup).
//!
//! The frontend has exactly two actions: "Подключиться" (start bypass + probe
//! the Wargaming server list) and "Остановить" (stop). The window close button
//! and the "Закрыть" button both fully exit the process.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod engine;
mod platform;

use std::sync::Arc;

use tauri::Manager;

#[tokio::main]
async fn main() {
    // Single instance: a second copy must never spawn a second engine — two
    // WinDivert engines steal each other's packets and break the bypass.
    unsafe {
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
        let _ = CreateMutexW(None, false, w!("Local\\wotblitz-connector-single-instance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            MessageBoxW(
                None,
                w!("WoT Blitz Connector уже запущен.\nЗавершите его перед повторным запуском (два движка работать не могут)."),
                w!("Уже запущено"),
                MB_OK | MB_ICONWARNING,
            );
            return;
        }
    }

    // Auto-UAC (same UX as the parent project): a non-elevated start
    // immediately relaunches itself elevated, because WinDivert requires
    // administrator rights. If the user declines the prompt, we keep running
    // non-elevated and the UI shows the warning banner. The marker argument
    // prevents an elevation loop if elevation fails to stick.
    let no_auto = std::env::args().any(|a| a == "--no-auto-elevate");
    if !no_auto && !platform::windows::is_elevated() {
        if platform::windows::relaunch_elevated(&["--no-auto-elevate".to_string()]).is_ok() {
            return; // elevated instance took over
        }
    }

    // No tray, no autostart, no background service: this is a single-shot
    // diagnostic client. The window close button exits the process outright
    // (see on_window_event below), and the "Закрыть" button calls exit_app.
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(
            commands::AppState::new()
                .unwrap_or_else(|e| panic!("failed to initialize app state: {e}")),
        )
        .setup(|app| {
            // Frontend event sink: state changes + log batches → webview events.
            let state: tauri::State<commands::AppState> = app.state();
            state
                .supervisor
                .set_sink(Arc::new(commands::TauriSink::new(app.handle().clone())));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Full process exit — no tray, no background keep-alive.
                // Alt+F4 / system close land here; the UI close button calls
                // exit_app directly.
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    commands::graceful_exit(&app).await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::connect,
            commands::stop,
            commands::get_probe_results,
            commands::get_logs,
            commands::clear_logs,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::list_backgrounds,
            commands::get_elevation,
            commands::relaunch_elevated,
            commands::kill_stray_processes,
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running WoT Blitz Connector");
}
