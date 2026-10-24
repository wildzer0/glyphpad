//! Platform-independent core of GlyphPad: normalized touch data model, gesture
//! engine, symbol recognizer and configuration.
//!
//! This crate must not depend on any OS API (SPEC §3).

pub mod action;
pub mod capture;
pub mod config;
pub mod config_edit;
pub mod engine;
pub mod infer;
pub mod keys;
pub mod model;
pub mod repeat;
pub mod replay;
pub mod source;
pub mod stats;
pub mod symbols;

pub use model::{Contact, DeviceEvent, DeviceId, DeviceInfo, MAX_CONTACTS, TouchFrame};
pub use source::{SourceError, TouchSource};
