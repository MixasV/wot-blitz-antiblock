//! Engine supervisor: winws2 lifecycle, log ring buffer, state events.
//!
//! Simplified port of the parent zapret2-gui supervisor: a single fixed
//! desync strategy tuned for Wargaming traffic (TLS fake/split on the
//! hostlist), no strategy catalog, no transports, no svc IPC, no file
//! logging. One engine child, bound to a Job Object, logs collected in a
//! 1000-entry ring and pushed to the frontend in small batches.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::platform::settings::AppSettings;
use crate::platform::windows::{
    cleanup_windivert_driver, is_elevated, kill_stray_winws2_processes, list_winws_pids,
    resolve_binaries, spawn_winws2, stop_winws2_gracefully, ProcessError,
};
use crate::platform::JobObject;

const RING_CAPACITY: usize = 1_000;
const EXIT_WATCH_INTERVAL: Duration = Duration::from_millis(500);
const LOG_FILE_MAX: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupervisorState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: String, // "info" | "warn" | "error" | "stdout" | "stderr"
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupervisorStatus {
    pub state: SupervisorState,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub binary_path: Option<String>,
    pub last_error: Option<String>,
}

/// Events pushed to the frontend (mirrors the parent project's EventSink).
pub trait EventSink: Send + Sync {
    fn emit_state_changed(&self, state: SupervisorState);
    fn emit_log_batch(&self, batch: &[LogEntry]);
}

struct NullSink;
impl EventSink for NullSink {
    fn emit_state_changed(&self, _state: SupervisorState) {}
    fn emit_log_batch(&self, _batch: &[LogEntry]) {}
}

pub struct Supervisor {
    /// Serializes start/stop so overlapping calls cannot interleave.
    lifecycle: tokio::sync::Mutex<()>,
    state: parking_lot::Mutex<SupervisorState>,
    active_pid: parking_lot::Mutex<Option<u32>>,
    active_binary: parking_lot::Mutex<Option<String>>,
    started_at: parking_lot::Mutex<Option<Instant>>,
    last_error: parking_lot::Mutex<Option<String>>,
    /// Engine child; tokio mutex because stop() awaits while holding it.
    active_child: tokio::sync::Mutex<Option<tokio::process::Child>>,
    job_object: Arc<JobObject>,
    ring: parking_lot::Mutex<VecDeque<LogEntry>>,
    sink: parking_lot::RwLock<Arc<dyn EventSink>>,
    /// Persistent engine log for post-mortem diagnostics (the UI ring is
    /// volatile and useless when the user reports a problem afterwards).
    log_file: parking_lot::Mutex<Option<std::fs::File>>,
}

/// Open (and size-cap) the persistent engine log file.
fn open_log_file() -> Option<std::fs::File> {
    let dir = if let Some(home) = dirs_home() {
        let d = home.join("AppData").join("Local").join("wotblitz-connector");
        let _ = std::fs::create_dir_all(&d);
        d
    } else {
        let d = std::env::current_exe().ok()?.parent()?.join("configs");
        let _ = std::fs::create_dir_all(&d);
        d
    };
    let path = dir.join("engine.log");
    // Rotate once: big log -> .old, start fresh.
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > LOG_FILE_MAX {
            let _ = std::fs::rename(&path, dir.join("engine.log.old"));
        }
    }
    std::fs::OpenOptions::new().create(true).append(true).open(&path).ok()
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from)
}

impl Supervisor {
    pub fn new() -> Result<Self, ProcessError> {
        Ok(Self {
            lifecycle: tokio::sync::Mutex::new(()),
            state: parking_lot::Mutex::new(SupervisorState::Stopped),
            active_pid: parking_lot::Mutex::new(None),
            active_binary: parking_lot::Mutex::new(None),
            started_at: parking_lot::Mutex::new(None),
            last_error: parking_lot::Mutex::new(None),
            active_child: tokio::sync::Mutex::new(None),
            job_object: Arc::new(
                JobObject::create()
                    .map_err(|e| ProcessError::JobError(e.to_string()))?,
            ),
            ring: parking_lot::Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
            sink: parking_lot::RwLock::new(Arc::new(NullSink)),
            log_file: parking_lot::Mutex::new(open_log_file()),
        })
    }

    /// Install the frontend event sink (called once at app setup).
    pub fn set_sink(&self, sink: Arc<dyn EventSink>) {
        *self.sink.write() = sink;
    }

    pub fn log(&self, level: &str, message: &str) {
        let entry = LogEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            level: level.to_string(),
            message: message.to_string(),
        };
        {
            let mut ring = self.ring.lock();
            if ring.len() >= RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(entry.clone());
        }
        // Persist for post-mortem diagnostics.
        {
            let mut guard = self.log_file.lock();
            if let Some(f) = guard.as_mut() {
                use std::io::Write;
                let ts = entry.timestamp;
                let millis = ts % 1000;
                let secs = ts / 1000;
                let _ = writeln!(
                    f,
                    "[{secs:010}.{millis:03}] [{level}] {message}"
                );
            }
        }
        self.sink.read().emit_log_batch(std::slice::from_ref(&entry));
    }

    pub fn get_status(&self) -> SupervisorStatus {
        let state = *self.state.lock();
        let uptime_secs = self
            .started_at
            .lock()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        SupervisorStatus {
            state,
            pid: *self.active_pid.lock(),
            uptime_secs,
            binary_path: self.active_binary.lock().clone(),
            last_error: self.last_error.lock().clone(),
        }
    }

    pub fn get_all_logs(&self) -> Vec<LogEntry> {
        self.ring.lock().iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        self.ring.lock().clear();
    }

    /// Start the engine with the current settings. Fails closed when not
    /// elevated (the frontend shows the admin gate on `ADMIN_REQUIRED`).
    pub async fn start(self: &Arc<Self>, settings: &AppSettings) -> Result<(), String> {
        let _guard = self.lifecycle.lock().await;
        let current = self.get_status().state;
        if matches!(
            current,
            SupervisorState::Starting | SupervisorState::Running | SupervisorState::Stopping
        ) {
            return Err(format!(
                "движок уже в состоянии {current:?}; дождитесь завершения операции"
            ));
        }

        self.set_state(SupervisorState::Starting);
        self.log("info", "Запуск движка обхода…");
        match self.start_locked(settings).await {
            Ok(()) => {
                self.set_state(SupervisorState::Running);
                self.log("info", "Движок запущен, обход активен");
                Ok(())
            }
            Err(message) => {
                *self.last_error.lock() = Some(message.clone());
                self.set_state(SupervisorState::Error);
                self.log("error", &format!("Не удалось запустить движок: {message}"));
                Err(message)
            }
        }
    }

    async fn start_locked(self: &Arc<Self>, settings: &AppSettings) -> Result<(), String> {
        if !is_elevated() {
            return Err(
                "ADMIN_REQUIRED: движку нужен запуск от имени администратора (WinDivert)"
                    .to_string(),
            );
        }

        // 1. Binaries.
        let binaries = resolve_binaries().map_err(|e| e.to_string())?;
        self.log(
            "info",
            &format!("Бинарники движка: {}", binaries.bin_dir.display()),
        );

        // 2. Coexistence check: another engine (the original zapret2-gui,
        //    Flowseal, …) must not be silently killed — two WinDivert
        //    engines steal each other's packets. Report and stop instead.
        let strays = list_winws_pids(None);
        if !strays.is_empty() {
            return Err(format!(
                "Обнаружен уже запущенный движок (winws.exe, PID: {}). Два DPI-движка \
                 конфликтуют за WinDivert. Если это оригинальный Zapret2 или другой \
                 обходчик — остановите его и повторите подключение.",
                strays.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
            ));
        }

        // 3. Hostlist from settings.
        let lists_dir = binaries
            .lists_dir
            .clone()
            .ok_or_else(|| "каталог lists не найден рядом с бинарниками".to_string())?;
        let hostlist = write_hostlist(&lists_dir, &settings.server_domains)
            .map_err(|e| format!("не удалось записать hostlist: {e}"))?;
        self.log(
            "info",
            &format!(
                "Hostlist: {} доменов → {}",
                settings.server_domains.len(),
                hostlist.display()
            ),
        );

        // 4. Command line: single fixed WoT strategy (TLS fake,split2).
        let args = build_arguments(settings, &binaries, &hostlist);
        self.log(
            "info",
            &format!("Командная строка: winws.exe {}", args.join(" ")),
        );

        // 5. Spawn + job binding. On a wedged WinDivert service left by a
        // previous crash, retry once after cleaning the driver state (safe:
        // we verified above that no other engine is running).
        let mut proc = match spawn_winws2(&binaries, &args, self.job_object.clone()) {
            Ok(p) => p,
            Err(first_err) => {
                self.log(
                    "warn",
                    &format!("Повторный запуск после сбоя первого: {first_err}"),
                );
                crate::platform::windows::cleanup_windivert_driver().await;
                spawn_winws2(&binaries, &args, self.job_object.clone())
                    .map_err(|e| format!("запуск winws.exe не удался: {e}"))?
            }
        };
        let stdout = proc.stdout.take();
        let stderr = proc.stderr.take();
        *self.active_pid.lock() = Some(proc.pid);
        *self.active_binary.lock() = Some(binaries.engine_exe.display().to_string());
        *self.started_at.lock() = Some(Instant::now());

        // 6. Output readers → log ring, then the exit watcher.
        if let Some(s) = stdout {
            tokio::spawn(self.clone().read_stream(s, "stdout"));
        }
        if let Some(s) = stderr {
            tokio::spawn(self.clone().read_stream(s, "stderr"));
        }
        *self.active_child.lock().await = Some(proc.child);
        tokio::spawn(self.clone().watch_exit());

        Ok(())
    }

    /// Stream child stdout/stderr lines into the log ring.
    async fn read_stream(
        self: Arc<Self>,
        stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        level: &'static str,
    ) {
        let mut lines = BufReader::new(stream).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => self.log(level, line.trim_end()),
                Ok(None) => break,
                Err(e) => {
                    self.log("warn", &format!("Ошибка чтения вывода движка: {e}"));
                    break;
                }
            }
        }
    }

    /// Watcher: polls the child every 500 ms; on unexpected exit resets state.
    async fn watch_exit(self: Arc<Self>) {
        loop {
            tokio::time::sleep(EXIT_WATCH_INTERVAL).await;
            let mut guard = self.active_child.lock().await;
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(code)) => {
                        guard.take();
                        drop(guard);
                        let message = match code.code() {
                            Some(10) => Some(
                                "WinDivert конфликтует с другой DPI-утилитой или антивирусом \
                                 (код выхода 10)"
                                    .to_string(),
                            ),
                            Some(0) => None,
                            Some(c) => {
                                Some(format!("Движок неожиданно завершился (код выхода {c})"))
                            }
                            None => Some("Движок неожиданно завершился".to_string()),
                        };
                        if let Some(m) = &message {
                            *self.last_error.lock() = Some(m.clone());
                            self.log("error", m);
                        }
                        self.reset_runtime_state();
                        // Only downgrade Running → Stopped; if stop() owns the
                        // transition (Stopping) it will finish on its own.
                        let mut state = self.state.lock();
                        if *state == SupervisorState::Running {
                            *state = SupervisorState::Stopped;
                            drop(state);
                            self.sink.read().emit_state_changed(SupervisorState::Stopped);
                        }
                        break;
                    }
                    Ok(None) => {} // still running
                    Err(e) => {
                        self.log("warn", &format!("Ошибка ожидания процесса: {e}"));
                    }
                },
                None => break, // child removed: watcher's job is done
            }
        }
    }

    /// Stop the engine and clean the WinDivert driver state.
    pub async fn stop(&self) -> Result<(), String> {
        let _guard = self.lifecycle.lock().await;
        let current = self.get_status().state;
        if current == SupervisorState::Stopped {
            return Ok(());
        }

        self.set_state(SupervisorState::Stopping);
        self.log("info", "Остановка движка…");

        let mut guard = self.active_child.lock().await;
        if let Some(child) = guard.as_mut() {
            stop_winws2_gracefully(child, &self.job_object)
                .await
                .map_err(|e| e.to_string())?;
        }
        guard.take();
        drop(guard);

        // VERIFY the kill: the engine must not survive the stop. If it
        // somehow did (AV interference, cygwin quirk) — finish it now.
        let survivors = list_winws_pids(None);
        if !survivors.is_empty() {
            self.log(
                "warn",
                &format!(
                    "Движок не завершился штатно (PID: {}) — завершаю принудительно",
                    survivors.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
                ),
            );
            kill_stray_winws2_processes(None);
        }

        // Deliberately NOT stopping the WinDivert service here: it is shared
        // system-wide, and stopping it breaks any other DPI tool that may be
        // running (the driver itself is inert once our handles are closed).

        self.reset_runtime_state();
        *self.last_error.lock() = None;
        self.set_state(SupervisorState::Stopped);
        self.log("info", "Движок остановлен");
        Ok(())
    }

    fn reset_runtime_state(&self) {
        *self.active_pid.lock() = None;
        *self.active_binary.lock() = None;
        *self.started_at.lock() = None;
    }

    fn set_state(&self, state: SupervisorState) {
        *self.state.lock() = state;
        self.sink.read().emit_state_changed(state);
    }
}

/// Build the winws.exe command line.
///
/// The parent project's working flowseal-alt line (Full mode), VERBATIM —
/// every section and desync format, with NO extra hostlists of our own.
/// Lesson learned: adding wot.txt/cloudflare/misc hostlists put fakedsplit
/// desync on paths the original leaves untouched (e.g. the shop CDN
/// wgcdn.co -> 84.38.4.62 is NOT in ipset-all.txt), which only added delay.
/// File arguments are relative to the engine's working directory (`bin_dir`).
fn build_arguments(
    settings: &AppSettings,
    binaries: &crate::platform::windows::ZapretBinaries,
    hostlist: &std::path::Path,
) -> Vec<String> {
    let to_arg = |p: &std::path::Path| -> String {
        match p.strip_prefix(&binaries.bin_dir) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => p.to_string_lossy().replace('\\', "/"),
        }
    };
    let general = "--hostlist=lists/list-general.txt".to_string();
    let general_user = "--hostlist=lists/list-general-user.txt".to_string();

    let mut args: Vec<String> = vec![
        "--debug=0".into(),
        "--wf-tcp=80,443,2053,2083,2087,2096,8443".into(),
        "--wf-udp=443,19294-19344,50000-65535".into(),
        // 1) UDP: QUIC by Wargaming + Cloudflare/DoH + misc domains.
        //    (NO --new before it: like the original, this first section is
        //    the default low-priority profile 0.)
        "--filter-udp=443".into(),
        general.clone(),
        general_user.clone(),
        "--hostlist-exclude=lists/list-exclude.txt".into(),
        "--hostlist-exclude=lists/list-exclude-user.txt".into(),
        "--ipset-exclude=lists/ipset-exclude.txt".into(),
        "--ipset-exclude=lists/ipset-exclude-user.txt".into(),
        "--dpi-desync=fake".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fake-quic=quic_initial_www_google_com.bin".into(),
        // 1b) UDP 19294-19344,50000-65535, ANY protocol (verbatim from the
        // original). CRITICAL for the game: this port range overlaps the
        // Blitz game/shop UDP ports (12000-29999), and any-protocol fakes
        // desync the game's own UDP protocol — no hostlist can match it.
        // Despite the "Discord" blob names, this section is what makes the
        // in-game shop work.
        "--new".into(),
        "--filter-udp=19294-19344,50000-65535".into(),
        "--dpi-desync=fake".into(),
        "--dpi-desync-any-protocol=1".into(),
        "--dpi-desync-cutoff=n4".into(),
        "--dpi-desync-fake-discord=ACTIVE_DISCORD_UDP.bin".into(),
        "--dpi-desync-fake-stun=ACTIVE_DISCORD_UDP.bin".into(),
        "--dpi-desync-fake-unknown-udp=ACTIVE_DISCORD_UDP.bin".into(),
        "--dpi-desync-repeats=6".into(),
        // 1c) Discord media TCP (multisplit) — verbatim.
        "--new".into(),
        "--filter-tcp=443,2053,2083,2087,2096,8443".into(),
        "--hostlist-domains=discord.media,gateway.discord.gg,discord.com,discordapp.com,discordapp.net,discordcdn.com,discord.gg,dis.gd,discord.co,discord.app,tenor.com,tenor.co,giphy.com".into(),
        "--dpi-desync=fake,multisplit".into(),
        "--dpi-desync-split-pos=1".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fooling=ts".into(),
        "--dpi-desync-fake-tls=tls_clienthello_www_google_com.bin".into(),
        // 2) TCP: Cloudflare/DoH + Wargaming companion domains.
        "--new".into(),
        "--filter-tcp=80,443".into(),
        general.clone(),
        general_user.clone(),
        "--hostlist-exclude=lists/list-exclude.txt".into(),
        "--hostlist-exclude=lists/list-exclude-user.txt".into(),
        "--ipset-exclude=lists/ipset-exclude.txt".into(),
        "--ipset-exclude=lists/ipset-exclude-user.txt".into(),
        "--dpi-desync=fake,fakedsplit".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fooling=ts".into(),
        "--dpi-desync-fakedsplit-pattern=0x00".into(),
        "--dpi-desync-fake-tls=stun.bin".into(),
        "--dpi-desync-fake-tls=tls_clienthello_www_google_com.bin".into(),
        "--dpi-desync-fake-http=tls_clienthello_max_ru.bin".into(),
        // 3) UDP: QUIC by game/commerce IP ranges.
        "--new".into(),
        "--filter-udp=443".into(),
        "--ipset=lists/ipset-all.txt".into(),
        "--hostlist-exclude=lists/list-exclude.txt".into(),
        "--hostlist-exclude=lists/list-exclude-user.txt".into(),
        "--ipset-exclude=lists/ipset-exclude.txt".into(),
        "--ipset-exclude=lists/ipset-exclude-user.txt".into(),
        "--dpi-desync=fake".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fake-quic=quic_initial_www_google_com.bin".into(),
        // 3b) TCP: Google / YouTube (fakedsplit) — returned per user request.
        "--new".into(),
        "--filter-tcp=443".into(),
        "--hostlist=lists/list-google.txt".into(),
        "--ip-id=zero".into(),
        "--dpi-desync=fake,fakedsplit".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fooling=ts".into(),
        "--dpi-desync-fakedsplit-pattern=0x00".into(),
        "--dpi-desync-fake-tls=tls_clienthello_www_google_com.bin".into(),
        // 4) TCP: game & commerce servers by IP.
        "--new".into(),
        "--filter-tcp=80,443,8443".into(),
        "--ipset=lists/ipset-all.txt".into(),
        "--hostlist-exclude=lists/list-exclude.txt".into(),
        "--hostlist-exclude=lists/list-exclude-user.txt".into(),
        "--ipset-exclude=lists/ipset-exclude.txt".into(),
        "--ipset-exclude=lists/ipset-exclude-user.txt".into(),
        "--dpi-desync=fake,fakedsplit".into(),
        "--dpi-desync-repeats=6".into(),
        "--dpi-desync-fooling=ts".into(),
        "--dpi-desync-fakedsplit-pattern=0x00".into(),
        "--dpi-desync-fake-tls=stun.bin".into(),
        "--dpi-desync-fake-tls=tls_clienthello_www_google_com.bin".into(),
        "--dpi-desync-fake-http=tls_clienthello_max_ru.bin".into(),
    ];

    if settings.auto_hostlist_enabled {
        if let Some(parent) = hostlist.parent() {
            let auto_path = parent.join("wot-auto.txt");
            if !auto_path.is_file() {
                let _ = std::fs::File::create(&auto_path);
            }
            // Insert right after the general user list (as in the parent).
            if let Some(pos) = args
                .iter()
                .rposition(|a: &String| a == "--hostlist=lists/list-general-user.txt")
            {
                args.insert(pos + 1, format!("--hostlist-auto={}", to_arg(&auto_path)));
            }
        }
    }
    args
}

/// Write the hostlist file from settings domains (atomic replace).
fn write_hostlist(
    lists_dir: &std::path::Path,
    domains: &[String],
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(lists_dir)?;
    let path = lists_dir.join("wot.txt");
    let mut content = String::from(
        "# WoT Blitz Connector hostlist — generated from app settings.\r\n\
         # Edit domains in the application, not in this file.\r\n",
    );
    for d in domains {
        content.push_str(d);
        content.push_str("\r\n");
    }
    crate::platform::settings::atomic_write(&path, content.as_bytes())?;
    Ok(path)
}
