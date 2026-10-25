//! The configuration draft shared by the Gestures and Settings pages
//! (SPEC §10.2). Edits go to the draft; once the draft is valid and has
//! been left alone for a moment it is saved by itself, so there is no Save
//! button to forget. Undo/redo, validation as you type and the handling of
//! changes made to the file elsewhere live here too.
//!
//! Every gesture of the draft carries a stable `uid`. The file's position
//! of a gesture (which keeps its comments and formatting when rewritten) is
//! looked up through the uid of the *saved* state, so the draft can keep
//! changing, even reordering, while a save is on its way.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use glyphpad_core::config::{self, Config, GestureDef, Settings, UpdateSettings};
use glyphpad_core::config_edit::{self, EditedGesture};
use glyphpad_core::service::{Command, Shared};

/// How long the draft must stay untouched before it is saved: long enough
/// not to write after every keystroke, short enough to feel immediate.
const DEBOUNCE: Duration = Duration::from_millis(700);
/// A save the engine has not confirmed after this long is reported failed.
const SAVE_TIMEOUT: Duration = Duration::from_secs(4);

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

fn new_uid() -> u64 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

/// A gesture of the draft with its identity.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub uid: u64,
    pub def: GestureDef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub gestures: Vec<Slot>,
    pub settings: Settings,
    pub update: UpdateSettings,
}

impl Draft {
    /// The draft of a configuration, and where each of its gestures sits in
    /// the file.
    fn from_config(c: &Config) -> (Self, HashMap<u64, usize>) {
        let gestures: Vec<Slot> = c
            .gestures
            .iter()
            .map(|g| Slot {
                uid: new_uid(),
                def: g.def.clone(),
            })
            .collect();
        let origin = gestures
            .iter()
            .enumerate()
            .map(|(i, s)| (s.uid, i))
            .collect();
        (
            Self {
                gestures,
                settings: c.settings.clone(),
                update: c.update.clone(),
            },
            origin,
        )
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

/// Where the draft stands with respect to the file: what the header shows.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveState {
    /// The file holds the draft.
    Saved,
    /// Changed, waiting for a pause before saving.
    Waiting,
    Saving,
    /// The draft is not valid, so it is kept but not saved.
    Invalid(String),
    /// The file changed elsewhere; the user decides.
    Conflict,
    /// The engine could not write the file.
    Failed(String),
}

struct Sent {
    text: String,
    draft: Draft,
    at: Instant,
    /// Refusals counted when the save was sent.
    refusals: u64,
}

pub struct Editor {
    /// Revision and text of the file the saved state comes from.
    base_revision: u64,
    base_text: String,
    base: Draft,
    base_origin: HashMap<u64, usize>,
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
    sent: Option<Sent>,
    /// A draft the engine failed to write, and why: not retried until the
    /// draft changes.
    failed: Option<(Draft, String)>,
    /// The draft as of the last frame, to notice edits.
    observed: Draft,
    changed_at: Instant,
    state: SaveState,
}

impl Editor {
    pub fn new(shared: &Shared) -> Self {
        let (d, origin) = Draft::from_config(&shared.config);
        Self {
            base_revision: shared.revision,
            base_text: shared.config_text.to_string(),
            base: d.clone(),
            base_origin: origin,
            committed: d.clone(),
            observed: d.clone(),
            draft: d,
            undo: Vec::new(),
            redo: Vec::new(),
            checked: None,
            external_change: false,
            show_diff: false,
            sent: None,
            failed: None,
            changed_at: Instant::now(),
            state: SaveState::Saved,
        }
    }

    pub fn dirty(&self) -> bool {
        self.draft != self.base
    }

    pub fn state(&self) -> &SaveState {
        &self.state
    }

    /// Follows the file: a new revision replaces a clean draft, and is
    /// reported when the draft has unsaved changes.
    pub fn sync(&mut self, shared: &Shared) {
        if let Some(sent) = &self.sent
            && shared.write_refusals > sent.refusals
        {
            // Our save was refused: the file changed first.
            self.sent = None;
            self.external_change = true;
            return;
        }
        if shared.revision == self.base_revision || self.external_change {
            return;
        }
        if let Some(sent) = self.sent.take_if(|s| s.text == *shared.config_text) {
            // Our own save landed. The draft stays as it is, even if it
            // changed meanwhile: only the saved state moves.
            self.base_revision = shared.revision;
            self.base_text = sent.text;
            self.base_origin = sent
                .draft
                .gestures
                .iter()
                .enumerate()
                .map(|(i, s)| (s.uid, i))
                .collect();
            self.base = sent.draft;
            self.failed = None;
        } else if *shared.config_text == self.base_text {
            // Same text: touched, not changed.
            self.base_revision = shared.revision;
        } else if !self.dirty() {
            self.reset(shared);
        } else {
            self.sent = None;
            self.external_change = true;
        }
    }

    /// Frame by frame: follows the file, records undo steps and saves the
    /// draft once it has settled. Returns the command to send, if any.
    pub fn frame(&mut self, ctx: &eframe::egui::Context, shared: &Shared) -> Option<Command> {
        self.sync(shared);
        let typing_or_dragging =
            ctx.egui_wants_keyboard_input() || ctx.input(|i| i.pointer.any_down());
        self.track(typing_or_dragging);
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        let (cmd, wake) = self.autosave(shared, Instant::now(), pointer_down);
        if let Some(after) = wake {
            ctx.request_repaint_after(after);
        }
        cmd
    }

    /// Notices edits and decides whether to save now. Also returns when to
    /// look again, if something is pending.
    fn autosave(
        &mut self,
        shared: &Shared,
        now: Instant,
        pointer_down: bool,
    ) -> (Option<Command>, Option<Duration>) {
        if self.draft != self.observed {
            self.observed = self.draft.clone();
            self.changed_at = now;
        }
        if pointer_down && self.dirty() {
            // A slider or a drag in progress: wait for the release.
            self.changed_at = now;
        }
        if !self.dirty() {
            self.state = SaveState::Saved;
            return (None, None);
        }
        if self.external_change {
            self.state = SaveState::Conflict;
            return (None, None);
        }
        if let Err(e) = self.check() {
            self.state = SaveState::Invalid(e);
            return (None, None);
        }
        if let Some(sent) = self
            .sent
            .take_if(|s| now.duration_since(s.at) >= SAVE_TIMEOUT)
        {
            let why = shared
                .status
                .clone()
                .unwrap_or_else(|| "the file could not be written".into());
            self.failed = Some((sent.draft, why));
        }
        if let Some(sent) = &self.sent {
            self.state = SaveState::Saving;
            return (
                None,
                Some(SAVE_TIMEOUT.saturating_sub(now.duration_since(sent.at))),
            );
        }
        if let Some((d, why)) = &self.failed {
            if *d == self.draft {
                self.state = SaveState::Failed(why.clone());
                return (None, None);
            }
            self.failed = None;
        }
        let settled = now.duration_since(self.changed_at);
        if settled < DEBOUNCE {
            self.state = SaveState::Waiting;
            return (None, Some(DEBOUNCE - settled));
        }
        self.state = SaveState::Saving;
        (self.save(shared, now), Some(SAVE_TIMEOUT))
    }

    /// Tries again after a failed save.
    pub fn retry(&mut self) {
        self.failed = None;
        self.flush();
    }

    /// Asks for the next autosave to happen at once (before leaving for
    /// the wizard, which writes to the same file).
    pub fn flush(&mut self) {
        self.changed_at = Instant::now()
            .checked_sub(DEBOUNCE * 2)
            .unwrap_or_else(Instant::now);
    }

    /// Records an undo step when the draft changed and the user is not in
    /// the middle of typing or dragging.
    fn track(&mut self, busy: bool) {
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

    /// Drops the draft and starts over from the file.
    pub fn reset(&mut self, shared: &Shared) {
        *self = Self::new(shared);
    }

    /// The file text the draft would produce, or why it is invalid.
    fn render(&self) -> Result<String, String> {
        let edited: Vec<EditedGesture> = self
            .draft
            .gestures
            .iter()
            .map(|s| EditedGesture {
                origin: self.base_origin.get(&s.uid).copied(),
                def: s.def.clone(),
            })
            .collect();
        let text = config_edit::write_gestures(&self.base_text, &edited)?;
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
        let r = self.render().and_then(|t| {
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

    fn save(&mut self, shared: &Shared, now: Instant) -> Option<Command> {
        let text = self.check().ok()?;
        self.sent = Some(Sent {
            text: text.clone(),
            draft: self.draft.clone(),
            at: now,
            refusals: shared.write_refusals,
        });
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
                self.reset(shared);
                None
            }
            ConflictChoice::Overwrite => self.save(shared, Instant::now()),
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

    pub fn add(&mut self, def: GestureDef) -> u64 {
        let uid = new_uid();
        self.draft.gestures.push(Slot { uid, def });
        uid
    }

    pub fn index_of(&self, uid: u64) -> Option<usize> {
        self.draft.gestures.iter().position(|s| s.uid == uid)
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

    const FILE: &str = "[settings]\nclose_to_tray = true\n\n[[gesture]]\nname = \"a\"\ntype = \"tap\"\nfingers = 1\naction = { type = \"lock_screen\" }\n\n[[gesture]]\nname = \"b\"\ntype = \"tap\"\nfingers = 2\naction = { type = \"lock_screen\" }\n";

    fn later(secs: u64) -> Instant {
        Instant::now() + Duration::from_secs(secs)
    }

    /// Runs the autosave at `secs`, then again after the debounce has
    /// passed: the first call notices an edit, the second saves it.
    fn step(e: &mut Editor, s: &Shared, secs: u64) -> Option<Command> {
        let t = later(secs);
        let first = e.autosave(s, t, false).0;
        first.or(e.autosave(s, t + DEBOUNCE * 2, false).0)
    }

    #[test]
    fn saves_once_the_draft_has_settled() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        assert!(step(&mut e, &s1, 0).is_none());
        assert_eq!(e.state(), &SaveState::Saved);
        e.draft.settings.close_to_tray = false;
        // Just edited: waiting for a pause.
        assert!(e.autosave(&s1, Instant::now(), false).0.is_none());
        assert_eq!(e.state(), &SaveState::Waiting);
        // The pointer being down (a slider drag) keeps it waiting.
        assert!(e.autosave(&s1, later(5), true).0.is_none());
        let Some(Command::WriteConfig {
            text,
            base_revision,
        }) = step(&mut e, &s1, 10)
        else {
            panic!("no save")
        };
        assert_eq!(base_revision, 1);
        assert!(text.contains("close_to_tray = false"));
        assert_eq!(e.state(), &SaveState::Saving);
        // Nothing is sent twice while the save is on its way.
        assert!(step(&mut e, &s1, 11).is_none());
    }

    #[test]
    fn an_invalid_draft_is_kept_and_not_saved() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        e.draft.gestures[0].def.name = String::new();
        assert!(step(&mut e, &s1, 10).is_none());
        assert!(matches!(e.state(), SaveState::Invalid(_)));
        e.draft.gestures[0].def.name = "fixed".into();
        assert!(step(&mut e, &s1, 20).is_some());
    }

    #[test]
    fn own_save_is_recognized_and_edits_made_meanwhile_survive() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        e.draft.gestures[0].def.name = "first".into();
        e.undo.push(e.base.clone());
        let Some(Command::WriteConfig { text, .. }) = step(&mut e, &s1, 10) else {
            panic!()
        };
        // The user keeps editing, and even reorders, while it is written.
        e.draft.gestures[1].def.name = "second".into();
        e.draft.gestures.swap(0, 1);
        e.sync(&s1);
        assert!(e.dirty());
        let s2 = shared(&text, 2);
        e.sync(&s2);
        assert!(!e.external_change, "our own save is not a conflict");
        assert!(e.dirty(), "the later edits are still unsaved");
        assert_eq!(e.draft.gestures[0].def.name, "second");
        assert!(e.can_undo());
        // The next save rewrites in place: both gestures keep their tables.
        let Some(Command::WriteConfig {
            text: t2,
            base_revision,
        }) = step(&mut e, &s2, 20)
        else {
            panic!()
        };
        assert_eq!(base_revision, 2);
        let names: Vec<String> = config::parse(&t2)
            .unwrap()
            .config
            .gestures
            .iter()
            .map(|g| g.def.name.clone())
            .collect();
        assert_eq!(names, ["second", "first"]);
    }

    #[test]
    fn external_change_keeps_the_draft() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        e.draft.settings.close_to_tray = false;
        e.sync(&shared("[settings]\nstart_minimized = true\n", 2));
        assert!(e.external_change);
        assert!(!e.draft.settings.close_to_tray, "the draft survives");
        assert!(step(&mut e, &s1, 10).is_none(), "no saving over a conflict");
        assert_eq!(e.state(), &SaveState::Conflict);
        // A clean editor simply follows the file.
        let mut clean = Editor::new(&s1);
        clean.sync(&shared("[settings]\nstart_minimized = true\n", 2));
        assert!(clean.draft.settings.start_minimized && !clean.external_change);
    }

    #[test]
    fn refused_save_becomes_a_conflict() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        e.draft.settings.close_to_tray = false;
        step(&mut e, &s1, 10).unwrap();
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
    fn a_save_the_engine_never_confirms_is_reported_once() {
        let s1 = shared(FILE, 1);
        let mut e = Editor::new(&s1);
        e.draft.settings.close_to_tray = false;
        step(&mut e, &s1, 10).unwrap();
        let failing = Shared {
            status: Some("not saved: disk full".into()),
            ..s1.clone()
        };
        assert!(step(&mut e, &failing, 20).is_none());
        assert_eq!(e.state(), &SaveState::Failed("not saved: disk full".into()));
        // Not retried in a loop; an edit lifts the failure.
        assert!(step(&mut e, &failing, 40).is_none());
        e.draft.settings.start_minimized = true;
        assert!(step(&mut e, &failing, 60).is_some());
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
