# GlyphPad

A lightweight, portable engine for custom touchpad gestures, written in Rust.

GlyphPad reads raw multi-touch data from the touchpad (every finger's position and
state) and recognizes user-defined gestures — taps, swipes, pinches and even
**symbols drawn on the touchpad** — then runs a configurable action. Draw an **L**
to lock the PC, an **X** to close the foreground app, tap with four fingers to open
a terminal.

- Windows (Precision Touchpad) first, Linux (evdev) next.
- Single executable, no installer, no administrator rights.
- No telemetry: the only network access is the optional update check against
  this repository's GitHub Releases.

> **Status: pre-alpha.** The project skeleton is in place; the gesture engine is not
> implemented yet. See [`docs/SPEC.md`](docs/SPEC.md) for the full specification and
> the milestones.

## Download

Pre-built packages are published on the
[Releases](https://github.com/wildzer0/glyphpad/releases) page as
`glyphpad-<version>-<target>.zip` (Windows) and `.tar.gz` (Linux), together with a
`SHA256SUMS` file.

**Windows SmartScreen:** the executable is not code-signed yet, so on first launch
Windows may show a "Windows protected your PC" warning. Choose *More info* → *Run
anyway* if you trust the download (verify it against `SHA256SUMS` first).

## Building from source

Requires the latest stable Rust toolchain (`rustup`).

```sh
cargo build --release
```

On Linux the GUI and tray dependencies need GTK, xdo and AppIndicator headers, e.g.
on Debian/Ubuntu:

```sh
sudo apt-get install libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
```

## License

[MIT](LICENSE)
