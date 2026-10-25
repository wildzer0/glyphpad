# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/wildzer0/glyphpad/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/wildzer0/glyphpad/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/wildzer0/glyphpad/releases/tag/v1.0.0
