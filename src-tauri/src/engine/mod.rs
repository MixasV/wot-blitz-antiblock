//! Engine subsystem: supervisor lifecycle + diagnostics probes.

#![allow(unused_imports)]

pub mod probes;
pub mod supervisor;

pub use probes::{ProbeMetric, ProbeResult, ProbeStatus, WoTServerProbe};
pub use supervisor::{LogEntry, Supervisor, SupervisorState, SupervisorStatus};
