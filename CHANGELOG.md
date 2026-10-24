# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/wildzer0/glyphpad/commits/main
