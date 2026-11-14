//! Platform abstraction. Windows-only for now.

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::*;