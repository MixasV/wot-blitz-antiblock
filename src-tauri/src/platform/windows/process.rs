//! Windows process helpers: binary resolution, engine spawn/stop,
//! WinDivert driver cleanup, stray-process kill, elevation checks.
//!
//! Trimmed port of the parent zapret2-gui `process.rs` — no strategies,
//! no `winws.exe`/split variants, no service binaries.

use std::os::windows::io::RawHandle;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;
use tokio::process::{Child, Command};

use super::job_object::JobObject;

pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Process names owned by the engine (killed as strays on cleanup).
pub const ENGINE_PROCESS_NAMES: &[&str] = &["winws.exe", "winws2.exe"];

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("engine binaries not found (searched: {searched})")]
    BinariesNotFound { searched: String },
    #[error("failed to spawn engine: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("child process has no process id")]
    MissingProcessId,
    #[error("child process has no raw handle")]
    MissingRawHandle,
    #[error("job object error: {0}")]
    JobError(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ZapretBinaries {
    pub bin_dir: PathBuf,
    /// The classic zapret `winws.exe` — its CLI dialect (`--dpi-desync=...`)
    /// matches the fixed strategy; `winws2.exe` (zapret2 Lua dialect) does not.
    pub engine_exe: PathBuf,
    pub windivert_dll: PathBuf,
    pub windivert_sys: PathBuf,
    pub lists_dir: Option<PathBuf>,
}

/// Explicit bin-dir override from the launcher: `--bin-dir <path>` or
/// `--bin-dir=<path>`. Command-line arguments survive UAC elevation, while
/// process environment variables do not — the elevated relaunch relies on it.
pub fn bin_dir_override() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--bin-dir" {
            if let Some(value) = args.next() {
                return Some(PathBuf::from(value));
            }
        } else if let Some(value) = arg.strip_prefix("--bin-dir=") {
            return Some(PathBuf::from(value));
        }
    }
    None
}

/// Resolve the engine binaries. Anchored at the running exe (portable layout:
/// `bin/` next to the app), with a `--bin-dir` argument override (used by the
/// launcher through UAC) and a `WOTBLITZ_BIN_DIR` env override for dev runs.
pub fn resolve_binaries() -> Result<ZapretBinaries, ProcessError> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = bin_dir_override() {
        candidates.push(dir);
    }
    if let Ok(over) = std::env::var("WOTBLITZ_BIN_DIR") {
        if !over.trim().is_empty() {
            candidates.push(PathBuf::from(over.trim()));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("bin"));
            candidates.push(dir.join("bin/windows-x86_64"));
            candidates.push(dir.to_path_buf());
        }
    }
    // Dev fallbacks (cargo run / tauri dev from the project directory).
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("bin"));
        candidates.push(cwd.to_path_buf());
    }

    let mut searched = Vec::new();
    for dir in candidates {
        searched.push(dir.display().to_string());
        let complete = ["winws.exe", "WinDivert.dll", "WinDivert64.sys"]
            .iter()
            .all(|f| dir.join(f).is_file());
        if complete {
            let lists_dir = [
                Some(dir.join("lists")),
                dir.parent().map(|p| p.join("lists")),
            ]
            .into_iter()
            .flatten()
            .find(|p| p.is_dir());
            return Ok(ZapretBinaries {
                engine_exe: dir.join("winws.exe"),
                windivert_dll: dir.join("WinDivert.dll"),
                windivert_sys: dir.join("WinDivert64.sys"),
                bin_dir: dir,
                lists_dir,
            });
        }
    }
    Err(ProcessError::BinariesNotFound {
        searched: searched.join(", "),
    })
}

/// Whether the current process token is elevated (administrator).
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = Default::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// Relaunch this app elevated via the UAC prompt ("runas").
/// `extra_args` are appended to the current command line (e.g. the
/// `--no-auto-elevate` marker); arguments survive UAC elevation while
/// environment variables do not.
pub fn relaunch_elevated(extra_args: &[String]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let file: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    // Re-quote args containing spaces so they arrive as single arguments.
    let mut parts: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a })
        .collect();
    parts.extend(extra_args.iter().cloned());
    let params = parts.join(" ");
    let params16: Vec<u16> = if params.is_empty() {
        Vec::new()
    } else {
        params.encode_utf16().chain(Some(0)).collect()
    };
    let ret = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            if params16.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(params16.as_ptr())
            },
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    if ret.0 as usize > 32 {
        Ok(())
    } else {
        Err("UAC request was declined or failed".into())
    }
}

/// A spawned engine child already assigned to the supervisor's job object.
/// The job object itself is owned by the supervisor, not stored here.
pub struct SupervisedProcess {
    pub child: Child,
    pub pid: u32,
    pub stdout: Option<tokio::process::ChildStdout>,
    pub stderr: Option<tokio::process::ChildStderr>,
}

/// Spawn a hidden, job-bound child process with piped stdout/stderr.
pub fn spawn_process(
    exe_path: &Path,
    bin_dir: &Path,
    args: &[String],
    job: Arc<JobObject>,
) -> Result<SupervisedProcess, ProcessError> {
    let mut cmd = Command::new(exe_path);
    cmd.args(args)
        .current_dir(bin_dir)
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        // Kill the child even if the Supervisor's stop path is aborted
        // (e.g. the app is terminated mid-start).
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(ProcessError::SpawnFailed)?;
    let pid = child.id().ok_or(ProcessError::MissingProcessId)?;
    let raw: RawHandle = child.raw_handle().ok_or(ProcessError::MissingRawHandle)?;
    job.assign_raw_handle(raw)
        .map_err(|e| ProcessError::JobError(e.to_string()))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    Ok(SupervisedProcess {
        child,
        pid,
        stdout,
        stderr,
    })
}

/// Thin wrapper spawning the engine binary.
pub fn spawn_winws2(
    binaries: &ZapretBinaries,
    args: &[String],
    job: Arc<JobObject>,
) -> Result<SupervisedProcess, ProcessError> {
    spawn_process(&binaries.engine_exe, &binaries.bin_dir, args, job)
}

/// Stop the engine: kill, bounded wait, job-object terminate as a safety net.
///
/// `CREATE_NO_WINDOW` means the child has no console, so no CTRL_BREAK_EVENT
/// path exists — a direct kill is the graceful-enough option here.
pub async fn stop_winws2_gracefully(
    child: &mut Child,
    job: &JobObject,
) -> Result<(), ProcessError> {
    let _ = child.kill().await;
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    let _ = job.terminate(1);
    Ok(())
}

/// Stop (not delete) leftover WinDivert driver services so a new engine
/// instance can bind the driver even if a previous one died uncleanly.
pub async fn cleanup_windivert_driver() {
    for name in ["WinDivert", "WinDivert14", "WinDivert64"] {
        let _ = Command::new(super::system32("sc.exe"))
            .args(["stop", name])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .await;
    }
}

/// List PIDs of running engine processes (snapshot + image-name re-check
/// against the opened handle to avoid PID-reuse races).
///
/// Used to DETECT foreign engines (the original zapret2-gui, Flowseal, etc.)
/// instead of blindly killing them: two WinDivert engines steal each other's
/// packets, so coexistence is impossible and must be reported, not hidden.
pub fn list_winws_pids(exclude_pid: Option<u32>) -> Vec<u32> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut pids = Vec::new();
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return pids,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let pid = entry.th32ProcessID;
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                );
                let is_target = ENGINE_PROCESS_NAMES
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(&name));
                if is_target && Some(pid) != exclude_pid {
                    if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        let exe = ENGINE_PROCESS_NAMES
                            .iter()
                            .find(|n| process_image_matches(handle, n))
                            .map(|n| n.to_string());
                        if exe.is_some() {
                            pids.push(pid);
                        }
                        let _ = CloseHandle(handle);
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        pids
    }
}

/// Terminate every stray engine process (snapshot + image-name re-check
/// against the opened handle to avoid PID-reuse races). Returns kill count.
pub fn kill_stray_winws2_processes(exclude_pid: Option<u32>) -> usize {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    let mut killed = 0usize;
    for pid in list_winws_pids(exclude_pid) {
        unsafe {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                if TerminateProcess(handle, 1).is_ok() {
                    killed += 1;
                }
                let _ = CloseHandle(handle);
            }
        }
    }
    killed
}

/// Verify that an opened process handle really refers to `exe_name`.
unsafe fn process_image_matches(handle: windows::Win32::Foundation::HANDLE, exe_name: &str) -> bool {
    use windows::core::PWSTR;
    use windows::Win32::System::Threading::{
        QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    };

    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    if QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_FORMAT(0),
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    )
    .is_err()
    {
        return false;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/'])
        .next()
        .map(|n| n.eq_ignore_ascii_case(exe_name))
        .unwrap_or(false)
}
