//! Diagnostics probes for Wargaming game endpoints: a FULL HTTPS request
//! (DNS + TCP + TLS + HTTP), measured through the current network path —
//! so with the engine running the probe shows the real bypass effect.
//!
//! A bare TCP-connect probe is not enough: the provider drops the first
//! connection / TLS stage, which TCP alone cannot see.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Below this total latency a target is fully healthy.
const OPERATIONAL_MS: u128 = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeStatus {
    Operational,
    Degraded,
    Blocked,
    Checking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeMetric {
    pub target: String,
    pub status: ProbeStatus,
    pub latency_ms: Option<u64>,
    pub dns_ms: Option<u64>,
    pub ip_address: Option<String>,
    pub message: String,
    pub last_checked_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub id: String,
    pub label: String,
    pub metric: ProbeMetric,
}

/// Probe suite for the WoT / WoT Blitz server infrastructure.
pub struct WoTServerProbe;

impl WoTServerProbe {
    /// Fixed probe targets: `(id, label, base_url)` — real HTTPS endpoints
    /// the game and its shop webview actually talk to. All DNS-verified
    /// via public resolvers on 2026-11-13.
    const TARGETS: &'static [(&'static str, &'static str, &'static str)] = &[
        ("blitz-eu", "WoT Blitz · EU", "https://eu.wotblitz.com/"),
        ("blitz-na", "WoT Blitz · NA", "https://na.wotblitz.com/"),
        ("blitz-asia", "WoT Blitz · Asia", "https://asia.wotblitz.com/"),
        (
            "wot-eu-login",
            "WoT PC · EU login",
            "https://login.p1.worldoftanks.eu/",
        ),
        ("wg-services", "Wargaming services", "https://eu.wargaming.net/"),
    ];

    /// Probe every target concurrently.
    pub async fn probe_all() -> Vec<ProbeResult> {
        let mut handles = Vec::new();
        for t in Self::TARGETS {
            handles.push(tokio::spawn(Self::probe_one(t.0, t.1, t.2)));
        }
        let mut results = Vec::new();
        for h in handles {
            match h.await {
                Ok(r) => results.push(r),
                Err(e) => results.push(ProbeResult {
                    id: "unknown".into(),
                    label: "probe task".into(),
                    metric: ProbeMetric {
                        target: String::new(),
                        status: ProbeStatus::Blocked,
                        latency_ms: None,
                        dns_ms: None,
                        ip_address: None,
                        message: format!("Ошибка пробы: {e}"),
                        last_checked_epoch: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    },
                }),
            }
        }
        results
    }

    async fn probe_one(
        id: &'static str,
        label: &'static str,
        url: &'static str,
    ) -> ProbeResult {
        let now_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let total = Instant::now();

        // DNS phase (for the latency/ip display; reqwest re-resolves itself).
        let host = url
            .split("//")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        let dns_started = Instant::now();
        let ip_address = tokio::net::lookup_host((host, 443u16))
            .await
            .ok()
            .and_then(|mut addrs| addrs.next().map(|a| a.ip().to_string()));
        let dns_ms = Some(dns_started.elapsed().as_millis() as u64);

        let client = match reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(Duration::from_secs(3))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return ProbeResult {
                    id: id.into(),
                    label: label.into(),
                    metric: ProbeMetric {
                        target: url.into(),
                        status: ProbeStatus::Blocked,
                        latency_ms: None,
                        dns_ms,
                        ip_address,
                        message: format!("Ошибка клиента: {e}"),
                        last_checked_epoch: now_epoch,
                    },
                }
            }
        };

        // Up to 3 attempts: the provider drops the FIRST connection on some
        // paths and passes retries — a single attempt would show a false
        // "недоступен" even with the bypass working.
        let mut last_err = None;
        let mut elapsed = 0u64;
        let mut code = 0u16;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let attempt_started = Instant::now();
            match client.get(url).send().await {
                Ok(r) => {
                    code = r.status().as_u16();
                    elapsed = attempt_started.elapsed().as_millis() as u64;
                    last_err = None;
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }

        match last_err {
            None => {
                // Any HTTP response means DNS + TCP + TLS all succeeded —
                // the path (with or without the bypass) works.
                let status = if elapsed as u128 <= OPERATIONAL_MS {
                    ProbeStatus::Operational
                } else {
                    ProbeStatus::Degraded
                };
                ProbeResult {
                    id: id.into(),
                    label: label.into(),
                    metric: ProbeMetric {
                        target: url.into(),
                        status,
                        latency_ms: Some(elapsed),
                        dns_ms,
                        ip_address,
                        message: format!("HTTPS ответ: {code}"),
                        last_checked_epoch: now_epoch,
                    },
                }
            }
            Some(e) => {
                let msg = if e.is_timeout() {
                    "Таймаут: соединение блокируется на этом пути"
                } else if e.is_connect() {
                    "Соединение не установлено (блокировка на TCP/TLS)"
                } else {
                    "Ошибка соединения"
                };
                ProbeResult {
                    id: id.into(),
                    label: label.into(),
                    metric: ProbeMetric {
                        target: url.into(),
                        status: ProbeStatus::Blocked,
                        latency_ms: None,
                        dns_ms,
                        ip_address,
                        message: msg.into(),
                        last_checked_epoch: now_epoch,
                    },
                }
            }
        }
    }
}
