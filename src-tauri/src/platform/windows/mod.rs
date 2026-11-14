//! Windows platform helpers: binary resolution, process spawn, WinDivert cleanup.

#![allow(unused_imports)]

pub mod job_object;
pub mod process;
pub mod settings;

pub use job_object::{JobObject, JobObjectError};
pub use process::{
    cleanup_windivert_driver, is_elevated, kill_stray_winws2_processes, list_winws_pids,
    relaunch_elevated, resolve_binaries, spawn_process, spawn_winws2,
    stop_winws2_gracefully, ProcessError, SupervisedProcess, ZapretBinaries,
};
pub use settings::{load_settings, save_settings, AppSettings};

/// Absolute path of a Windows system binary (resolved via SystemRoot, not PATH).
pub fn system32(binary: &str) -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    std::path::PathBuf::from(root).join("System32").join(binary)
}
