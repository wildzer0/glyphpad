//! The platform's live touch source.

use anyhow::Result;
use crossbeam_channel::Receiver;
use glyphpad_core::DeviceEvent;

/// Starts the live touch source on its own thread.
#[cfg(windows)]
pub fn start(toplevel_window: bool) -> Result<Receiver<DeviceEvent>> {
    use glyphpad_core::TouchSource;
    use glyphpad_platform::windows::{RawInputSource, WindowKind};

    let kind = if toplevel_window {
        WindowKind::HiddenTopLevel
    } else {
        WindowKind::MessageOnly
    };
    let source = RawInputSource::new(kind);
    if source.devices().is_empty() {
        tracing::warn!("no Precision Touchpad found yet; waiting for one to be connected");
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::Builder::new()
        .name("touch-source".into())
        .spawn(move || {
            if let Err(e) = source.run(tx) {
                tracing::error!("touch source failed: {e}");
            }
        })?;
    Ok(rx)
}

#[cfg(not(windows))]
pub fn start(_toplevel_window: bool) -> Result<Receiver<DeviceEvent>> {
    anyhow::bail!("live touch input is only implemented on Windows so far (Linux arrives in M7)")
}

#[cfg(windows)]
pub fn dump_descriptors() -> Result<String> {
    Ok(glyphpad_platform::windows::dump_descriptors())
}

#[cfg(not(windows))]
pub fn dump_descriptors() -> Result<String> {
    anyhow::bail!("dump-descriptor is only implemented on Windows so far (Linux arrives in M7)")
}
