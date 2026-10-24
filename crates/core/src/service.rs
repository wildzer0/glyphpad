//! State shared between the engine thread and the user interface, and the
//! commands the interface sends. The engine runs on its own thread and
//! keeps working while the window is hidden (SPEC §10.1).

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::config::{Config, GestureDef};
use crate::engine::{GestureTrace, Observation, Polyline};
use crate::model::{Contact, DeviceId, DeviceInfo};
use crate::symbols::{Candidate, Sample, TemplateFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineMode {
    /// Recognize and run actions.
    #[default]
    Normal,
    /// Recognize without running actions (SPEC §13 test mode).
    Test,
    /// Every drawing becomes a sample of a new symbol (SPEC §10.3).
    RecordSymbol,
    /// Every tap, hold or swipe becomes a demonstration (SPEC §10.3).
    RecordDemonstration,
}

impl EngineMode {
    pub fn is_recording(self) -> bool {
        matches!(
            self,
            EngineMode::RecordSymbol | EngineMode::RecordDemonstration
        )
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    SetMode(EngineMode),
    ToggleEnabled,
    /// Re-read the configuration and templates from disk.
    Reload,
    /// Write a new gesture and, for a symbol, its samples, in one go.
    SaveGesture {
        def: GestureDef,
        symbol_samples: Option<(String, Vec<Sample>)>,
    },
    /// Add a sample to a symbol's templates (e.g. from the history).
    AddTemplate {
        symbol: String,
        sample: Sample,
    },
    /// Delete one of the user's samples of a symbol.
    DeleteTemplate {
        symbol: String,
        index: usize,
    },
    /// Use or hide a symbol's built-in samples.
    HideBuiltins {
        symbol: String,
        hidden: bool,
    },
    /// Set a configuration value such as `["settings", "close_to_tray"]`.
    SetSetting {
        path: Vec<String>,
        value: toml::Value,
    },
    /// Run an action now ("Run now" in the editors).
    RunAction(Action),
    /// Drop a recorded sample or demonstration.
    DiscardRecorded(usize),
    ClearRecorded,
}

/// One recognition, as shown in the history (SPEC §10.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Local wall-clock time, `HH:MM:SS`.
    pub time: String,
    /// What was recognized, e.g. `symbol L score=0.91`.
    pub recognized: String,
    pub symbol: Option<String>,
    pub score: Option<f32>,
    pub gesture: Option<String>,
    pub action: Option<String>,
    pub app: Option<String>,
    /// False in test mode, when paused, or when no gesture matched.
    pub executed: bool,
    pub trace: Arc<GestureTrace>,
    pub width_mm: f32,
    pub height_mm: f32,
}

/// Something recorded by the gesture wizard.
#[derive(Debug, Clone)]
pub enum Recorded {
    Sample(Sample),
    Demonstration {
        observation: Observation,
        trace: Arc<GestureTrace>,
    },
}

/// The engine's state as the interface sees it.
#[derive(Debug, Clone, Default)]
pub struct Shared {
    pub config: Arc<Config>,
    pub config_path: PathBuf,
    /// The user's templates file.
    pub templates: TemplateFile,
    /// Templates in use: built-in plus the user's.
    pub effective: Arc<BTreeMap<String, Vec<Sample>>>,
    pub devices: Vec<DeviceInfo>,
    /// Device of the last frame, and its contacts.
    pub active_device: Option<DeviceId>,
    pub contacts: Vec<Contact>,
    /// Symbol being drawn, in millimeters.
    pub drawing: Vec<Polyline>,
    /// Candidates of the last drawing.
    pub last_candidates: Vec<Candidate>,
    pub enabled: bool,
    pub mode: EngineMode,
    /// Newest last.
    pub history: VecDeque<HistoryEntry>,
    pub recorded: Vec<Recorded>,
    /// Last noteworthy event for a status line (saved, errors, …).
    pub status: Option<String>,
    /// Warnings about the configuration and the environment.
    pub warnings: Vec<String>,
}

impl Shared {
    pub fn device(&self, id: DeviceId) -> Option<&DeviceInfo> {
        self.devices.iter().find(|d| d.id == id)
    }

    /// Size of the touchpad in use, or of the first one.
    pub fn pad_size_mm(&self) -> (f32, f32) {
        self.active_device
            .and_then(|id| self.device(id))
            .or(self.devices.first())
            .map_or((100.0, 70.0), |d| (d.width_mm, d.height_mm))
    }
}
