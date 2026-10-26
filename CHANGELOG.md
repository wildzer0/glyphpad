# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.7.2] - 2026-10-26

### Added
- `[settings] symbol_margin` (default 0.02, in Settings → Drawing): how much better the best
  symbol must score than every other one for a drawing to count. At 0 the best score always wins.

### Fixed
- A silhouette keeps the proportions of a touchpad (the ratio is clamped to 0.5–1.2), so a sample
  whose extents are those of a stroke no longer draws an enormous pad in the wizard.
- The conflict notice in the gestures list is one line with a "Details" button, instead of a
  banner as tall as the list.

## [1.7.1] - 2026-10-26

### Fixed
- The examples in the recording wizard stop growing wider and wider: each card measured the page
  inside itself, and the wizard sizes to its content, so once a card was wider than the page (the
  side-by-side thumbnail of a confusing symbol) it grew again on the next frame.
- A drawing that looks like two symbols is no longer fired as the better-scoring one: the best has
  to beat every other symbol by a margin, otherwise it is reported as not recognized with both
  names in the log. It is what made a drawn `b` run the gesture of a recorded `n`.
- The tour's bubble is legible again: the dim was painted on the same layer as the bubble, and
  within one layer the two are composed in no particular order — so the dim could land on top of
  the text. The dim and the outline now go one layer below.

## [1.7.0] - 2026-10-26

### Added
- A guided tour of the window: an overlay that dims the page, outlines one element at a time and
  says what it is for — the list and its precedence, the recorder, the editor, Review, Settings and
  the tray — step by step, skippable, and openable again from the sidebar.
- The tour is offered on the first run only: `onboarding.json` beside the configuration records
  that it was seen, whether it was finished or skipped, so an update never brings it back.
- Gestures are off while the tour runs and come back on as they were when it ends, so the
  touchpad is for the tour alone. New `SetEnabled` command for that: it sets the state instead of
  toggling it.
- The tour teaches by doing: a step hands over to the recording wizard — a few examples, a name,
  an action, and then the gesture tried for real — and the tour picks up again when it closes.
- The tour can be opened again by the user: "Show the tour" in Settings → Window and tray, and in
  the tray menu.

### Fixed
- The $Q distance no longer skips a start it should have tried: the lower bound was computed for
  an ordered matching while the distance always takes the nearest free point, so the bound could
  come out above the distance it bounded and drop the best start — a recognition could then miss
  the symbol it should have matched. The early abandoning stays (it is what makes it cheap), and
  the property test against the exhaustive reference passes again.
- `start_minimized` is respected after an update: the old process no longer asks the new one to
  show its window while handing over, and a show request that arrives in the first frames is
  ignored when the window started in the tray. Before, the window appeared at every start after an
  update.

## [1.6.4] - 2026-10-26

### Fixed
- A gesture saved by the recording wizard shows in the list at once: the page takes in what the
  file gained before drawing, instead of waiting for a change the editor had already missed. It
  used to appear only after closing and reopening the window.
- While recording, the drawing follows the configured activation: with
  `draw_activation = { mode = "fingers", count = 3 }` a single finger no longer becomes a sample
  (and no longer fires a gesture). Only the `toggle` mode is armed for recording, since it has no
  other way in.

## [1.6.3] - 2026-10-25

### Fixed
- A gesture saved while the window had edits in flight stayed invisible: the editor latched into a
  conflict it only left when resolved by hand, and from then on ignored the file. The file is now
  the base the draft is rendered against and the source of the gestures it gained, the draft keeps
  its edits, and the next save writes both — no latch, no invisible gestures.

## [1.6.2] - 2026-10-25

### Fixed
- A gesture saved (or added by another program) while the window had edits of its own is no longer
  swallowed by the conflict: the gestures the file gained are taken into the draft, so they appear
  in the list, and the edit that was in flight is kept. The "recorded gesture does not appear"
  reports could not be reproduced from a file that was correct, so the case that hid it is closed.

### Added
- The log file is written beside the configuration in every mode, not only portable: a windowed
  program has no terminal to log into, and a report can now come with its log.

## [1.6.1] - 2026-10-25

### Changed
- A new icon and logo: a rounded tile with a gesture trail sweeping across it. It is drawn in
  code, so the window and the tray get it at any size; the executable's build script compiles the
  same drawing into a Windows resource (seven sizes, 16 to 256) so the file carries it too; and
  `docs/images/logo.svg` (icon and name) is what the README shows.
- The "these changes are not saved yet" banner is gone. The status line says when a change is not
  saved and clicking it opens the detail with the ways out (try again, load the file, keep the
  draft), so nothing appears above the page while one types — which is also what made a field lose
  the focus whenever a half-typed value made the draft invalid.

### Fixed
- The `focus_or_launch` fields are one per row and as wide as the column: side by side they were
  clipped by the gesture editor, and a clipped field cannot be clicked into.
- A gesture saved through the wizard is read back into the window even when its symbol examples
  could not be written: the reload was skipped on that error, leaving the gesture in the file but
  not in the list.

## [1.6.0] - 2026-10-25

### Added
- A drawn symbol can be a chord: `modifier_fingers = 1` draws it with the drawing fingers while
  another rests, so the same shape can mean two things.

## [1.5.0] - 2026-10-25

### Added
- The statistics say which symbol the unrecognized drawings came closest to, and how often, which
  is what points at a symbol that is drawn badly or whose minimum score is too high.
- A single gesture can be exported from its own menu, which writes the same kind of pack as
  "Share" (it holds that gesture and the examples of its symbols).
- Review can merge two examples of a symbol into one (the two halves of an X recorded apart) and
  divide one example into its strokes.
- `scroll_hold` scrolls continuously after a hold: the gesture fires once and the wheel turns as
  the fingers move, a notch every few millimetres, until they lift.
- `repeat` runs an action several times, `only_if` runs one only when the window in front matches
  (evaluated when it runs, not when the gesture fired), and `scroll` can be smooth (`smooth_ms`).
- The gestures page shows every zone on one map of the pad, the overlapping ones in red and a
  filter for the profile: clicking a zone opens the gesture that uses it, which is what makes a
  tangle of corner zones understandable.
- A `zone` gesture can fire on the physical click as well as a tap (`click = true`), which is
  what one wants for a corner button: press it rather than tap it.
- A `zone` gesture can have several regions (`regions = [...]`, up to 8) — one gesture for "any
  corner" — and the window has an "All four corners" button to make them.

## [1.4.1] - 2026-10-25

### Changed
- The gestures page no longer re-checks lookalike symbols on every edit: on a real configuration
  that check measured over 200 ms (a debug build) and ran shortly after each change, which is
  what made typing a name or dragging the zone map stutter. It now depends on the symbols in use
  and is cached; the rest of the conflict check costs microseconds.
- A draft is validated only once it has been left alone, not on every keystroke or every frame of
  a drag, and the "not saved yet" banner no longer appears and disappears while a name is half
  typed.

## [1.4.0] - 2026-10-25

### Added
- `[settings] touchpad` chooses which touchpad to use, by part of its name: the other pads'
  frames are then ignored, and the zone map is drawn with the chosen pad's size. Settings →
  Touchpad lists the pads that have reported, with their sizes. Empty (the default) keeps
  listening to them all, as before.
- A zone is placed on a map of the real touchpad: click to put it, drag its corner to resize it,
  or pick one of the four corners; the numbers in millimetres stay underneath.
- Two zones that overlap are reported as a conflict, in the gesture list's conflict banner.
- With the overlay on, a zone gesture is announced by its name as soon as the fingers land on it,
  and the message names the zone instead of saying "tap".

### Fixed
- A gesture recorded through the wizard is selected and scrolled into view: it was saved and
  worked, but the list did not show it.
- Renaming a gesture no longer stutters, and the name field keeps the focus when it is emptied
  and typed into again.

## [1.3.0] - 2026-10-25

### Added
- Importing a pack warns when a gesture starts a program and will not import it until the box is
  ticked: a pack can come from anywhere, and a `run` action can do anything.
- Releases can be signed with minisign: the workflow signs `SHA256SUMS`
  (`SHA256SUMS.minisig`), the public key is compiled into the executable, and an update whose
  signature does not verify is refused and logged. A build without a key says so.
- Mouse actions: `scroll` (wheel notches), `mouse_click` (left, right, middle, once or twice) and
  `drag` (press, move by a few pixels, release).
- A new gesture kind, `zone`: a tap inside a rectangle of the touchpad, in millimetres
  (`region = { x = [0, 25], y = [0, 20] }`). The corners become buttons you can hit without
  looking.
- A gesture can set its own thresholds — `min_distance_mm` and `min_speed_mm_s` for swipes,
  `min_change_mm` for pinches, `min_deg` for rotations — stricter or looser than the settings, so
  two gestures on the same movement can ask for different lengths. The engine recognizes the
  loosest of them and each gesture checks its own.
- Each step of a sequence can set its own `within_ms`, the pause allowed after that step.
- `focus_or_launch`: brings the first window of a program to the front, or starts it when it is
  not running — what "open my editor" means, without a script.
- `copy` puts a text on the clipboard; `paste_text` puts it there and sends `Ctrl+V`, which is
  faster and more faithful than typing it character by character.
- Placeholders in the texts of `run`, `open`, `notify` and `type_text`: `{{app}}`, `{{title}}`,
  `{{profile}}`, `{{date}}`, `{{time}}` and `{{clipboard}}` (`{{clipboard}}` through
  `GlyphPad`'s own clipboard read, so nothing is stored or sent anywhere).
- `run` can go through the command interpreter (`shell = true`, so pipes, `>`, `&&` and
  variables work), take environment variables (`env`), and wait for the program to finish
  (`wait = true`, up to `timeout_ms`).
- The window edits a `run` action's arguments one per line, so an argument with a space is
  possible, and has fields for the new options.

## [1.2.1] - 2026-10-25

### Fixed
- The history no longer records a second entry, without a gesture, for a drawing whose own stroke
  ran one: a sequence's closing swipe is drawn with the drawing finger count, so it was first
  reported as an unrecognized drawing and then as the gesture that completed the sequence. The
  "not recognized" message and the unrecognized counter were affected the same way.

## [1.2.0] - 2026-10-25

### Fixed
- A sequence that starts with a `hold` no longer fails when the fingers rest on after the hold
  fires: its `within_ms` counts from the lift, when the step's touch ends, instead of from the
  moment the hold was recognized.

### Added
- A sequence that does not complete now says why in the log ("no next step began within its
  `within_ms`", or "the next gesture was not its next step") and names the sequence, instead of
  leaving its steps to fire on their own as if nothing had happened. `glyphpad replay` prints the
  same line, which is the quickest way to tell a sequence done too slowly from one that is not
  configured as expected.
- A gesture that matches nothing says what the configuration does have with that many fingers
  ("hold fingers=3 matched no gesture; with 3 fingers the configuration has: tap with 3 fingers"):
  a touch held 1.7 s is a hold, not the tap a sequence's first step asked for, and "no gesture
  configured" hid the difference.

## [1.1.0] - 2026-10-25

### Added
- Portable mode keeps a rotated log (`glyphpad.log`, one previous copy) next to the configuration.
- The window says so when the gesture engine stops after an internal error, instead of leaving a
  program that silently does nothing.

### Changed
- **Sequences of drawn steps work at any pace.** A step drawn with the drawing finger count (a
  three-finger swipe, say) less than `stroke_gap_ms` after the previous one used to merge into a
  single drawing that matched no symbol, so *nothing at all* happened; two quick three-finger
  swipes now run the sequence. A drawing that matches no symbol is judged one stroke at a time.
- A tap waiting for a possible second tap is confirmed as soon as the next touch cannot be one,
  instead of when that touch ends: a tap followed by pointer movement no longer runs its action
  seconds later, and sequences that start with a tap no longer extend past `within_ms`.
- Every recognition carries the times of its own movement, so `within_ms` means the same thing
  whether or not a multi-tap gesture is configured, and each candidate sequence keeps its own
  window.
- Rotation tolerance is searched coarse-to-fine: about 26 ms at 45° and 40 ms at 180° (was 47 and
  152 ms), inside the 50 ms budget.
- `glyphpad capture` keeps a running summary instead of every frame, so a long recording no longer
  grows in memory or rescans itself every second.
- Conflict detection in the window waits for the draft to settle and hashes only what it looks at,
  instead of formatting the whole draft on every frame.
- A pack import writes a symbol's examples once, and a new gesture's configuration is written
  before its examples, so a failed save leaves no orphan templates.
- Hiding the window while the recording wizard runs ends the recording, instead of leaving the
  gestures paused for good.
- Renaming or deleting a profile updates the profiles rules that use it.
- The update readiness marker is written once the window's event loop is alive: a new version that
  cannot create its window is rolled back instead of replacing a working one.
- The HID parser reuses its buffers instead of allocating several vectors per report.
- A trace playback repaints 30 times a second instead of every frame.

### Fixed
- `glyphpad capture` and `glyphpad replay` panicked, or printed an absurd duration, on a capture
  whose timestamps go backwards; the summary is now per device and saturating.
- A Precision Touchpad that reports the end of a touch only as Contact Count 0 left the engine
  with a touch that never ended, so nothing was recognized on such a device.
- `Ctrl+Plus` sent a bare `=`; it now sends Shift with the plus key.
- A settings error in `glyphpad.toml` now names its line as well as the field.
- A shortcut field follows a shortcut changed by undo, an import or a replaced action.
- The single-instance mutex is created after clearing the last error, so a stale error could not
  make the first instance exit as if another one were running.
- `write_atomically` flushes the temporary file to disk before renaming it.
- The local update source (debug builds, tests) refuses drive-relative asset names.
- A chord's tap duration is measured from the fingers touching down, not from the frame it was
  noticed.
- The line diff of the conflict dialog can no longer overflow on a very long file.

## [1.0.2] - 2026-10-25

### Added
- `docs/DESIGN.md`: in-depth design and architecture guide with diagrams (crates, threads, pipeline, engine, sequences, configuration, GUI, updates, release), invariants and recipes for common changes.

### Fixed
- Sequences: `within_ms` is now the pause between the end of one step and the *start* of the next, not the time up to the end of the next step. A calm pace (about 0.6 s between a tap and a swipe) used to break the sequence silently. A step that began in time is waited for until it is recognized.
- Fingers no longer stay "stuck" in the Debug and recording views after lifting: the frame that lifts the last finger could be skipped by the 16 ms refresh limit.
- The feedback overlay no longer stays open on "Drawing…" after a touch that triggers nothing (an unconfigured tap or swipe, a held sequence step): it closes when drawing ends, with a timeout as safety net.
- A plain gesture that is the first step of a sequence now waits `within_ms` before firing instead of `within_ms` + `stroke_gap_ms`.

## [1.0.1] - 2026-10-25

### Fixed
- Sequences whose later step is a swipe made with the drawing finger count (e.g. a 3-finger tap, then a 3-finger swipe up) now work at a normal pace: the `within_ms` window is counted to the end of the movement, not to when it was recognized (a stroke is only recognized `stroke_gap_ms` after the fingers lift).
- Starting a just-installed version on Linux no longer fails with "text file busy" (it is tried again).

## [1.0.0] - 2026-10-25

### Added
- Cargo workspace with the `core`, `platform`, `actions`, `gui`, `updater` and `app` crates.
- Minimal `glyphpad` binary that prints its version (`--version`).
- `cargo xtask release <version>` to check, bump the version, roll the changelog, commit and tag.
- CI workflow (fmt, clippy, tests on Windows and Linux) and tag-triggered release workflow
  producing `glyphpad-<version>-<target>` packages and `SHA256SUMS`.
- Normalized touch model (`TouchFrame`, `Contact`, `DeviceInfo`) with coordinates in millimeters.
- Windows Precision Touchpad backend over Raw Input and the HID parser: background input,
  hybrid reporting mode, multiple reports per message, hot-plug, Scan Time based timestamps.
- `glyphpad capture <file>` records touch sessions as JSON Lines; `glyphpad replay <file>`
  plays them back deterministically and prints session statistics.
- `glyphpad dump-descriptor` prints the HID capabilities of every Precision Touchpad.
- The Windows release binary has no console window; CLI subcommands attach to the parent console.
- Gesture engine with tap and multi-tap, hold and swipe (4 or 8 directions) recognizers,
  each an explicit state machine, and a documented disambiguation policy.
- TOML configuration with validation errors that name line, gesture and field; per-app
  (`when.app`, `when.title`) and profile conditions; the default configuration is created
  on first start, or read from `glyphpad.toml` next to the executable (portable mode).
- Windows actions: `keys`, `lock_screen`, `close_window` (with `force`), `minimize`,
  `maximize`, `run`, `media`, `chain`/`wait` and `toggle_enabled`.
- `glyphpad run` executes gestures; `glyphpad check-config` validates a configuration;
  `debug` and `replay` print recognized gestures without executing them.
- Regression tests on touch sessions recorded on real hardware (`tests/fixtures/`).
- Drawn symbols with the $Q point-cloud recognizer: unistroke and multistroke (strokes
  closer than `stroke_gap_ms` form one symbol), orientation-sensitive with an optional
  rotation tolerance, per-gesture confidence, and the best candidate logged when nothing
  fires. Built-in templates for L, q, X, C, V, Z, O, arrow-up and arrow-down.
- Draw mode strategies: `fingers` (default, three fingers), `edge`, `hold` and `toggle`
  (with the new `draw_symbol` action); experimental `freeze_cursor` for one-finger modes.
  A stroke that matches no symbol falls back to a tap or swipe.
- `glyphpad record <name>` records examples of a symbol into `templates.json`, with a
  consistency and lookalike check.
- Warning at start-up when Windows' own gestures for the drawing finger count are on.
- Window (egui/eframe, glow backend) with the Gestures, Review gestures, Settings and Live
  debug tabs: animated playback of every template and of the history, adding a drawing
  from the history as a template, deleting templates, a settings editor that keeps the
  file's comments.
- Gesture wizard: record a symbol or demonstrate a tap, hold or swipe, review and discard
  samples, quality and lookalike check, name and action, save, then try it in test mode.
- Tray icon with Open, enable/disable, reload, configuration folder and quit;
  `close_to_tray` and `start_minimized`; a second start shows the running instance.
- The engine runs on its own thread and keeps working with the window hidden.
- `repeat_last` with per-gesture `repeatable`, `repeat_timeout_s` and `repeat_same_app_only`.
- Feedback overlay showing the stroke and the recognized gesture; `notify` messages appear
  there. Test mode recognizes without running actions.
- `glyphpad debug` opens the live debug tab in test mode.
- New gesture types: pinch, rotate, edge swipe, click (and click-and-drag), chords (a tap
  or swipe while other fingers rest, `modifier_fingers`) and sequences (`steps` performed
  within `within_ms`). An edge swipe matches edge-swipe gestures before plain swipes.
- Configuration editor in the window: gesture list with search, filters, quick enabling,
  drag and drop priority, duplicate and delete; a form for every type; action editor
  with shortcut capture, program picker, chains and "Run now"; per-app conditions picked
  from open windows; profiles; undo/redo; validation as you type; saving that keeps the
  file's comments and formatting.
- Hot reload of `glyphpad.toml` and `templates.json`; an invalid file keeps the last valid
  configuration. Edits from the window are refused, with a choice to reload, overwrite or
  see the differences, when the file changed on disk meanwhile.
- Backups of the last `backup_count` configurations in `backups/`, restorable from Settings.
- Conflict detection: gestures set off by the same movement, lookalike symbols, holds that
  would pre-empt chords.
- Automatic pause while a listed or full-screen application is in front; the tray icon
  shows it. Optional start with Windows (`autostart`).

- Automatic updates from this project's GitHub Releases (`[update]`): `mode` off, notify
  (default) or auto, `channel` stable or prerelease, `check_interval_h`. A check runs a few
  seconds after start and then every interval, using the ETag and backing off when GitHub's
  rate limit is reached. The window shows a banner with the release notes and *Install and
  restart* / *Ignore this version*; Settings → Updates and the tray have *Check now*.
- The download is verified against the release's `SHA256SUMS`, then the executable is
  swapped (`glyphpad.old.exe`), the new version is started and, if it does not report
  within 15 s or exits, it is stopped and the old one restored. Never installs during a
  gesture, a drawing or a recording; never downgrades; a folder that cannot be written
  gets a message with the release link instead of a silent failure.
- `glyphpad update [--check]`. Debug builds also accept a hidden `--update-source <dir>`.
- Releases are not signed yet: `SHA256SUMS` protects against corrupt downloads, not
  against a compromised GitHub account.

- More actions: `open` (file, folder or address), `type_text`, `window` (snap left/right, next
  monitor, always on top), `virtual_desktop`, `screenshot`, `switch_profile`, `next_profile` and
  `show_cheat_sheet`.
- Profiles that switch by themselves (`profile_rules`), a profile switcher in the sidebar, the tray
  menu and Settings, and the actions above to switch by gesture.
- Cheat sheet: a list of the active gestures with a picture each, shown for a few seconds.
- Local usage statistics (`stats.json`, off with `statistics = false`): runs per gesture,
  unrecognized drawings, symbols with no gesture; Review → Statistics.
- Gesture packs: *Share → Export / Import* a `.glyphpad` file with gestures and the symbol examples
  they use, with a preview and a choice per gesture when names clash.
- Italian and English interface (`language`, also in Settings → Appearance), following the system
  by default.
- User guide (English and Italian), configuration reference and screenshots in `docs/`.

### Changed
- The window was rebuilt on a design system: light and dark themes (`theme = "system" |
  "light" | "dark"` in `[settings]`, also in Settings → Appearance), Phosphor icons, a
  sidebar with the engine state and the Gestures / Test mode switches, master-detail
  Gestures page with a visual picker for the type and the direction, and previews that
  lay out in columns and scale with the window.
- Configuration changes from the window are saved automatically a moment after the last
  edit, and only when valid; the bottom bar shows "All changes saved", and problems (invalid
  draft, file changed on disk, write failure) appear as a message above the page. There is
  no Save button. Saves made close together share one backup.

### Fixed
- Symbols shown as boxes in the window (system fonts are now used on Windows).
- Template thumbnails stuck in a corner or cut off; the history panel growing during replay.
- The drawing preview showed the first finger before the others landed.
- `repeat_last` repeated into another window of the same application.
- "Show window" only flashed the taskbar button when the window was behind others.
- The wizard's quality check was recomputed every frame, slowing the window down.

[Unreleased]: https://github.com/wildzer0/glyphpad/compare/v1.7.2...HEAD
[1.7.2]: https://github.com/wildzer0/glyphpad/compare/v1.7.1...v1.7.2
[1.7.1]: https://github.com/wildzer0/glyphpad/compare/v1.7.0...v1.7.1
[1.7.0]: https://github.com/wildzer0/glyphpad/compare/v1.6.4...v1.7.0
[1.6.4]: https://github.com/wildzer0/glyphpad/compare/v1.6.3...v1.6.4
[1.6.3]: https://github.com/wildzer0/glyphpad/compare/v1.6.2...v1.6.3
[1.6.2]: https://github.com/wildzer0/glyphpad/compare/v1.6.1...v1.6.2
[1.6.1]: https://github.com/wildzer0/glyphpad/compare/v1.6.0...v1.6.1
[1.6.0]: https://github.com/wildzer0/glyphpad/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/wildzer0/glyphpad/compare/v1.4.1...v1.5.0
[1.4.1]: https://github.com/wildzer0/glyphpad/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/wildzer0/glyphpad/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/wildzer0/glyphpad/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/wildzer0/glyphpad/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/wildzer0/glyphpad/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/wildzer0/glyphpad/compare/v1.0.2...v1.1.0
[1.0.2]: https://github.com/wildzer0/glyphpad/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/wildzer0/glyphpad/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/wildzer0/glyphpad/releases/tag/v1.0.0
