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

## Known limitations

- **Maximum contacts.** Read from the Contact Count Maximum feature report when
  the device can be opened for it; otherwise derived from descriptor logical
  maxima or the number of finger slots, which may underestimate it.
  `dump-descriptor` prints which source was used.
- **Physical size.** Taken from the X/Y physical range and units. Descriptors
  without units (non-compliant with PTP) fall back to an assumed 100 mm width.
- **Console output of the release binary.** It is a GUI-subsystem program, so
  the shell does not wait for it and the prompt can appear before its output.
- _Still to be assessed: draw mode and cursor interference (SPEC §5), UIPI
  restrictions on elevated windows._
