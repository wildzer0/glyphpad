//! The configuration draft shared by the Gestures and Settings pages
//! (SPEC §10.2): edits stay in the draft until saved, with undo/redo,
//! validation as you type, and a prompt when the file changes on disk.

use glyphpad_core::config::{self, Config, GestureDef, Settings, UpdateSettings};
use glyphpad_core::config_edit::{self, EditedGesture};
use glyphpad_core::service::{Command, Shared};

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub gestures: Vec<EditedGesture>,
    pub settings: Settings,
    pub update: UpdateSettings,
}

impl Draft {
    fn from_config(c: &Config) -> Self {
        Self {
            gestures: c
                .gestures
                .iter()
                .enumerate()
                .map(|(i, g)| EditedGesture {
                    origin: Some(i),
                    def: g.def.clone(),
                })
                .collect(),
            settings: c.settings.clone(),
            update: c.update.clone(),
        }
    }

    /// Profiles known to the draft: declared ones and those gestures use.
    pub fn profiles(&self) -> Vec<String> {
        let mut out = self.settings.profiles.clone();
        for g in &self.gestures {
            if let Some(p) = &g.def.profile
                && !out.contains(p)
            {
                out.push(p.clone());
            }
        }
        if !out.contains(&self.settings.active_profile) {
            out.insert(0, self.settings.active_profile.clone());
        }
        out
    }
}

/// What the editor does when the file changed on disk while it had
/// unsaved changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Reload,
    Overwrite,
    ShowDiff,
}

pub struct Editor {
    /// Revision and text of the file the draft started from.
    base_revision: u64,
    base_text: String,
    base: Draft,
    pub draft: Draft,
    undo: Vec<Draft>,
    redo: Vec<Draft>,
    /// Last state pushed onto the undo stack.
    committed: Draft,
    /// Validation of the current draft, recomputed only when it changes.
    checked: Option<(Draft, Result<String, String>)>,
    /// The file changed on disk while the draft had unsaved changes.
    pub external_change: bool,
    pub show_diff: bool,
    /// The text of a save on its way: the revision showing exactly this
    /// text is ours; any other one is a change made elsewhere.
    sent: Option<String>,
    /// Refusals counted when the save was sent.
    refusals: u64,
}

impl Editor {
    pub fn new(shared: &Shared) -> Self {
        let d = Draft::from_config(&shared.config);
        Self {
            base_revision: shared.revision,
            base_text: shared.config_text.to_string(),
            base: d.clone(),
            committed: d.clone(),
            draft: d,
            undo: Vec::new(),
            redo: Vec::new(),
            checked: None,
            external_change: false,
            show_diff: false,
            sent: None,
            refusals: shared.write_refusals,
        }
    }

    pub fn dirty(&self) -> bool {
        self.draft != self.base
    }

    /// Follows the file: a new revision replaces a clean draft, and is
    /// reported when the draft has unsaved changes.
    pub fn sync(&mut self, shared: &Shared) {
        if self.sent.is_some() && shared.write_refusals > self.refusals {
            // Our save was refused: the file changed first.
            self.sent = None;
            self.external_change = true;
            return;
        }
        if shared.revision == self.base_revision || self.external_change {
            return;
        }
        let ours = self.sent.as_deref() == Some(shared.config_text.as_str());
        if ours || !self.dirty() {
            let undo = std::mem::take(&mut self.undo);
            *self = Self::new(shared);
            if ours {
                // Undo history survives our own saves.
                self.undo = undo;
            }
        } else {
            self.sent = None;
            self.external_change = true;
        }
    }

    /// Records an undo step when the draft changed and the user is not in
    /// the middle of typing or dragging.
    pub fn track(&mut self, ctx: &eframe::egui::Context) {
        let busy = ctx.egui_wants_keyboard_input() || ctx.input(|i| i.pointer.any_down());
        if !busy && self.draft != self.committed {
            self.undo
                .push(std::mem::replace(&mut self.committed, self.draft.clone()));
            self.redo.clear();
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.draft != self.committed
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if self.draft != self.committed {
            // Unfinished typing counts as one step.
            self.redo
                .push(std::mem::replace(&mut self.draft, self.committed.clone()));
        } else if let Some(prev) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.draft, prev.clone()));
            self.committed = prev;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo
                .push(std::mem::replace(&mut self.committed, next.clone()));
            self.draft = next;
        }
    }

    pub fn discard(&mut self, shared: &Shared) {
        *self = Self::new(shared);
    }

    /// The file text the draft would produce, or why it is invalid.
    fn render(&self, from_text: &str) -> Result<String, String> {
        let text = config_edit::write_gestures(from_text, &self.draft.gestures)?;
        let edits = config_edit::settings_edits(
            &(self.base.settings.clone(), self.base.update.clone()),
            &(self.draft.settings.clone(), self.draft.update.clone()),
        );
        config_edit::apply_edits(&text, &edits)
    }

    /// Validation of the draft with the same checks as the file's.
    pub fn check(&mut self) -> Result<String, String> {
        if let Some((d, r)) = &self.checked
            && *d == self.draft
        {
            return r.clone();
        }
        let r = self.render(&self.base_text).and_then(|t| {
            config::parse(&t).map(|_| t).map_err(|e| {
                e.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        });
        self.checked = Some((self.draft.clone(), r.clone()));
        r
    }

    /// The validated configuration of the draft, for conflict checks.
    pub fn draft_config(&mut self) -> Option<Config> {
        self.check()
            .ok()
            .and_then(|t| config::parse(&t).ok())
            .map(|l| l.config)
    }

    pub fn save(&mut self, shared: &Shared) -> Option<Command> {
        let text = self.check().ok()?;
        self.sent = Some(text.clone());
        self.refusals = shared.write_refusals;
        // Overwriting after a conflict writes over the newer file.
        let base_revision = if self.external_change {
            shared.revision
        } else {
            self.base_revision
        };
        self.external_change = false;
        Some(Command::WriteConfig {
            text,
            base_revision,
        })
    }

    pub fn resolve_conflict(&mut self, choice: ConflictChoice, shared: &Shared) -> Option<Command> {
        match choice {
            ConflictChoice::Reload => {
                self.discard(shared);
                None
            }
            ConflictChoice::Overwrite => self.save(shared),
            ConflictChoice::ShowDiff => {
                self.show_diff = true;
                None
            }
        }
    }

    /// Line differences between the file on disk and the draft.
    pub fn diff(&mut self, shared: &Shared) -> Vec<(char, String)> {
        let ours = self.check().unwrap_or_default();
        line_diff(&shared.config_text, &ours)
    }

    pub fn add(&mut self, def: GestureDef) -> usize {
        self.draft
            .gestures
            .push(EditedGesture { origin: None, def });
        self.draft.gestures.len() - 1
    }

    /// A name not used by any gesture of the draft, based on `base`.
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.draft.gestures.iter().any(|g| g.def.name == n);
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|i| format!("{base}-{i}"))
            .find(|n| !taken(n))
            .unwrap_or_default()
    }
}

/// A plain LCS line diff: `' '` same, `'-'` only on disk, `'+'` only in the draft.
pub fn line_diff(a: &str, b: &str) -> Vec<(char, String)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((' ', a[i].to_string()));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(('+', b[j].to_string()));
            j += 1;
        } else {
            out.push(('-', a[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn shared(text: &str, revision: u64) -> Shared {
        Shared {
            config: Arc::new(config::parse(text).unwrap().config),
            config_text: Arc::new(text.to_string()),
            revision,
            ..Default::default()
        }
    }

    const FILE: &str = "[settings]\nclose_to_tray = true\n";

    fn edited(e: &mut Editor) {
        e.draft.settings.close_to_tray = false;
    }

    #[test]
    fn own_save_is_recognized_and_keeps_undo() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        edited(&mut e);
        e.undo.push(e.base.clone());
        let Some(Command::WriteConfig {
            text,
            base_revision,
        }) = e.save(&s1)
        else {
            panic!()
        };
        assert_eq!(base_revision, 1);
        // Until the file shows our text, the draft stays unsaved.
        e.sync(&s1);
        assert!(e.dirty());
        e.sync(&shared(&text, 2));
        assert!(!e.dirty() && !e.external_change);
        assert!(e.can_undo());
        assert!(!e.draft.settings.close_to_tray);
    }

    #[test]
    fn external_change_keeps_the_draft() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        edited(&mut e);
        e.sync(&shared("[settings]\nstart_minimized = true\n", 2));
        assert!(e.external_change);
        assert!(!e.draft.settings.close_to_tray, "the draft survives");
        // A clean editor simply follows the file.
        let mut clean = Editor::new(&s1);
        clean.sync(&shared("[settings]\nstart_minimized = true\n", 2));
        assert!(clean.draft.settings.start_minimized && !clean.external_change);
    }

    #[test]
    fn refused_save_becomes_a_conflict() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        edited(&mut e);
        e.save(&s1);
        let refused = Shared {
            write_refusals: 1,
            ..s1.clone()
        };
        e.sync(&refused);
        assert!(e.external_change && e.dirty());
        // Overwriting writes against the current revision.
        let newer = Shared {
            revision: 3,
            ..refused
        };
        let Some(Command::WriteConfig { base_revision, .. }) =
            e.resolve_conflict(ConflictChoice::Overwrite, &newer)
        else {
            panic!()
        };
        assert_eq!(base_revision, 3);
    }

    #[test]
    fn diff_marks_changes() {
        let d = line_diff("a\nb\nc\n", "a\nx\nc\nd\n");
        let got: Vec<(char, &str)> = d.iter().map(|(m, l)| (*m, l.as_str())).collect();
        assert_eq!(
            got,
            [(' ', "a"), ('+', "x"), ('-', "b"), (' ', "c"), ('+', "d")]
        );
    }
}
