//! The touch source abstraction (SPEC §3.2).
//!
//! The trait lives in the core, not in `glyphpad-platform`, because the core's
//! own [`ReplaySource`](crate::replay::ReplaySource) implements it and the
//! core cannot depend on the platform crate.

use crossbeam_channel::Sender;

use crate::model::{DeviceEvent, DeviceInfo};

pub type SourceError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub trait TouchSource {
    /// Touch devices currently available to this source.
    fn devices(&self) -> Vec<DeviceInfo>;

    /// Emits events until the source ends or the receiver is dropped.
    ///
    /// Blocks the calling thread; run it on a dedicated one.
    fn run(self, tx: Sender<DeviceEvent>) -> Result<(), SourceError>;
}
