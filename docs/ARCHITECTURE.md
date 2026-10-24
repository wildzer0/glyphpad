# Architecture

GlyphPad is a Cargo workspace. The binding specification is [`SPEC.md`](SPEC.md);
this document describes how the code is actually organized and records known
limitations as they are discovered.

## Crates

| Directory | Package | Role |
|---|---|---|
| `crates/core` | `glyphpad-core` | Normalized data model (`TouchFrame`, `Contact`, `DeviceInfo`), gesture engine, recognizer, config. No OS dependencies. |
| `crates/platform` | `glyphpad-platform` | `TouchSource` / `ForegroundInfo` traits and the cfg-gated Windows (Raw Input + HID) and Linux (evdev) backends. |
| `crates/actions` | `glyphpad-actions` | `ActionExecutor` implementations per platform. |
| `crates/gui` | `glyphpad-gui` | egui/eframe window. |
| `crates/updater` | `glyphpad-updater` | Update check, download, verification and executable replacement. Owns the only reference to the GitHub repository (`GITHUB_REPO`). |
| `crates/app` | `glyphpad` | The `glyphpad` binary: CLI, tray, main loop, hot reload. |
| `xtask` | `xtask` | Project automation (`cargo xtask release`, `changelog`, `check-tag`). Not shipped. |

Packages carry a `glyphpad-` prefix because a crate named `core` would clash with
Rust's built-in `core` crate.

Dependency direction: `app` → `gui`, `updater`, `actions`, `platform` → `core`.
`core` depends on nothing platform-specific.

## Conventions enforced by the build

- `unsafe_code = "deny"` workspace-wide; platform FFI modules opt in locally with
  `#[allow(unsafe_code)]`.
- The Windows MSVC build statically links the CRT (`.cargo/config.toml`).
- Release profile: fat LTO, one codegen unit, stripped symbols, `panic = "unwind"`
  so a panicking backend thread can be caught and restarted.

## Touch input pipeline (Windows)

```
WM_INPUT ─▶ RAWHID (dwCount × dwSizeHid) ─▶ TouchpadLayout::parse ─▶ RawReport
        ─▶ FrameAssembler (hybrid mode) ─▶ ScanClock ─▶ Geometry ─▶ TouchFrame ─▶ channel
```

- `platform::windows::source`: a hidden window (message-only by default,
  `--toplevel-window` for a hidden top-level one) registered for usage page
  0x0D / usage 0x05 with `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`. Each
  `WM_INPUT` may hold several reports; `WM_INPUT_DEVICE_CHANGE` handles hot-plug.
- `platform::windows::hid`: values are located through the preparsed data by
  usage page, usage and link collection (one collection per finger), never by
  byte offset.
- `platform::ptp`: OS-independent and unit-tested on every platform — HID unit
  conversion, hybrid-mode frame assembly, Scan Time to monotonic microseconds.
- `core::capture` / `core::replay`: JSON Lines captures (header line, then one
  `DeviceEvent` per line) and their deterministic playback as a `TouchSource`.

The `TouchSource` trait lives in `glyphpad-core` rather than `glyphpad-platform`
because the core's own `ReplaySource` implements it.

## Gesture engine

`core::engine` turns frames into *observations* (what the fingers did);
`engine::resolve` maps an observation to the first matching configured
gesture whose profile and `when` conditions apply; the application runs its
action on a separate thread.

- `SessionTracker`: a touch session lasts from the first finger down to the
  last one up. It records the most fingers down at once, how far any finger
  drifted, whether the button was clicked and the *peak phase* (the stretch
  with the most fingers down, before the first lift).
- Recognizers, each an explicit state machine: `TapRecognizer` (taps and
  multi-taps), `HoldRecognizer` (fires while the fingers are down),
  `SwipeRecognizer` (centroid travel of the peak phase, judged on lift).
- `Spec` derives from the configuration what the recognizers must look for:
  the most taps configured per finger count (only then does a tap wait for a
  possible next one), which finger counts hold, which swipe in 8 directions.
- `Engine::tick` and `next_deadline` drive timeouts. The live loop blocks on
  the event channel until the next deadline, so it uses no CPU while idle;
  `Engine::process_all` does the same deterministically for captures.

Disambiguation: a session with a physical click produces nothing (click +
gesture comes in M5); a hold consumes its session; taps and swipes exclude
each other by their thresholds; among matching gestures the first in the
file wins.

## Known limitations

- **Late palm detection.** The reference touchpad reports a palm as a
  confident finger for 30–70 ms before flagging it. A session in which a
  finger turns into a palm produces no gesture and lasts until the palm lifts.
- **Native Windows gestures.** Three- and four-finger gestures configured in
  Windows' touchpad settings run as well as GlyphPad's; turn off the ones you
  replace.
- **Elevated windows (UIPI).** Without administrator rights, keys sent with
  `SendInput` and window actions do not reach applications running elevated.
- **`notify` before M4.** Notifications need the tray icon; until then they
  are only logged.
- **Release `run` before M4.** The release binary has no console and no tray
  yet, so `glyphpad run` started from Explorer runs invisibly; stop it from
  Task Manager. Use the console for now.

- **Maximum contacts.** Read from the Contact Count Maximum feature report when
  the device can be opened for it; otherwise derived from descriptor logical
  maxima or the number of finger slots, which may underestimate it.
  `dump-descriptor` prints which source was used.
- **Physical size.** Taken from the X/Y physical range and units. Descriptors
  without units (non-compliant with PTP) fall back to an assumed 100 mm width.
- **Console output of the release binary.** It is a GUI-subsystem program, so
  the shell does not wait for it and the prompt can appear before its output.
- _Still to be assessed: draw mode and cursor interference (SPEC §5)._
