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
- `glyphpad debug` prints live touch frames in the terminal.
- The Windows release binary has no console window; CLI subcommands attach to the parent console.

[Unreleased]: https://github.com/wildzer0/glyphpad/commits/main
