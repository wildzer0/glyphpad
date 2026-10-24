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

## Known limitations

_To be filled in from M1 on: draw mode and cursor interference (SPEC §5), maximum
number of contacts reported by the device, UIPI restrictions on elevated windows._
