//! Touch input sources: the cfg-gated Windows and Linux backends (SPEC §3.2,
//! §4, §7). The [`TouchSource`](glyphpad_core::TouchSource) trait itself
//! lives in the core.

pub mod ptp;

#[cfg(windows)]
pub mod windows;
