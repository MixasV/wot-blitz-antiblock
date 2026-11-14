//! Application settings — minimal, JSON-backed, in the install directory.
//!
//! Defaults carry the Wargaming domain set verified via public DNS
//! (1.1.1.1 / 8.8.8.8) on 2026-11-13: `*.wotblitz.com`, `*.wargaming.net`,
//! `worldoftanks.*`, the `gcdn.co` CDN and voice services. Hostlist entries
//! are suffix-matched by winws2, so root domains cover all subdomains.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Background images bundled with the app (`public/bg`, served at `/bg`).
pub const BACKGROUND_IDS: &[&str] = &[
    "art-tank",
    "reforged",
    "battle-pass",
    "characters",
    "field-containers",
    "banner-clean",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Wargaming domains routed through the bypass (suffix-matched hostlist).
    pub server_domains: Vec<String>,
    /// TCP ports intercepted by WinDivert: 80/443 (TLS login+API+game),
    /// 5222/5223 (XMPP chat), per portforward.com Blitz requirements.
    pub intercept_ports_tcp: Vec<u16>,
    /// Whether engine-side auto-hostlist learning is enabled
    /// (`--hostlist-auto`, winws2 appends learned domains to `wot-auto.txt`).
    pub auto_hostlist_enabled: bool,
    /// Selected background image id from [`BACKGROUND_IDS`], `None` = plain theme.
    pub background_id: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            // Roots only: winws suffix-matches, so `wotblitz.com` covers
            // `eu.wotblitz.com`, `login0.wotblitz.com`, etc. All verified via
            // public DNS (1.1.1.1 / 8.8.8.8) on 2026-11-13; wgcdn.co and
            // wgcrowd.io have no root A record but live subdomains (CDN/shop
            // content: media-wbp.wgcdn.co -> 84.38.4.62, Wargaming IP space).
            server_domains: vec![
                "wotblitz.com".into(),
                "wotblitz.eu".into(),
                "wargaming.net".into(),
                "wargaming.com".into(),
                "worldoftanks.eu".into(),
                "worldoftanks.com".into(),
                "worldoftanks.asia".into(),
                "wgcdn.co".into(),
                "gcdn.co".into(),
                "wgcrowd.io".into(),
                "vivox.com".into(),
            ],
            intercept_ports_tcp: vec![80, 443, 5222, 5223],
            auto_hostlist_enabled: false,
            background_id: Some("art-tank".into()),
        }
    }
}

impl AppSettings {
    /// Normalize user input: lowercase domains, drop invalid entries and
    /// duplicates, enforce sane caps. Returns a clean copy.
    pub fn sanitized(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.server_domains.retain(|raw| {
            let d = raw.trim().to_lowercase();
            let valid = !d.is_empty()
                && d.len() <= 253
                && d.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
                && !d.starts_with('.')
                && !d.ends_with('.')
                && seen.insert(d.clone());
            valid
        });
        for d in self.server_domains.iter_mut() {
            *d = d.trim().to_lowercase();
        }
        if self.server_domains.len() > 500 {
            self.server_domains.truncate(500);
        }

        self.intercept_ports_tcp.retain(|p| (1..=65535).contains(p));
        self.intercept_ports_tcp.sort_unstable();
        self.intercept_ports_tcp.dedup();
        if self.intercept_ports_tcp.len() > 32 {
            self.intercept_ports_tcp.truncate(32);
        }
        if self.intercept_ports_tcp.is_empty() {
            self.intercept_ports_tcp = vec![80, 443, 5222, 5223];
        }

        if let Some(bg) = &self.background_id {
            if !BACKGROUND_IDS.contains(&bg.as_str()) {
                self.background_id = None;
            }
        }
        self
    }
}

fn settings_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let dir = parent.join("configs");
            let _ = fs::create_dir_all(&dir);
            return dir.join("app_settings.json");
        }
    }
    PathBuf::from("configs/app_settings.json")
}

/// Load settings from disk, falling back to defaults (missing or corrupt file).
pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            match serde_json::from_str::<AppSettings>(&content) {
                Ok(s) => return s.sanitized(),
                Err(_) => {
                    // Quarantine the corrupt file instead of silently overwriting it.
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = fs::rename(
                        &path,
                        path.with_extension(format!("json.corrupt-{stamp}")),
                    );
                }
            }
        }
    }
    AppSettings::default()
}

/// Persist settings atomically (tmp file + rename).
pub fn save_settings(s: AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&s.sanitized()).map_err(|e| e.to_string())?;
    atomic_write(&path, json.as_bytes()).map_err(|e| e.to_string())
}

/// Write-through atomic file replace (unique tmp file in the same directory).
pub fn atomic_write(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let id = std::process::id();
    let tmp = path.with_extension(format!("tmp-{}", id));
    let mut f = fs::File::create(&tmp)?;
    f.write_all(contents)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)?;
    Ok(())
}
