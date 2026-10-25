# Configuration reference

GlyphPad is configured by one TOML file, `glyphpad.toml`. The window edits it for you (and keeps
your comments and formatting when it does), so you never *have* to open it; this page is for
when you want to, or want to know exactly what an option means.

- [Where the files are](#where-the-files-are)
- [The shape of the file](#the-shape-of-the-file)
- [`[settings]`](#settings)
- [`[update]`](#update)
- [`[[gesture]]`](#gesture)
- [Actions](#actions)
- [Conditions](#conditions)
- [Shortcuts](#shortcuts)
- [Files GlyphPad keeps](#files-glyphpad-keeps)
- [Command line](#command-line)

## Where the files are

1. `glyphpad.toml` **next to `glyphpad.exe`** → *portable mode*; every other file lives there too.
2. Otherwise the user configuration folder: `%APPDATA%\GlyphPad\config\` on Windows. The file is
   created from the example on first start.
3. `--config <file>` on the command line overrides both.

GlyphPad watches the folder: saving the file loads it within a moment (*hot reload*). A file
with an error is **not** loaded; the last valid configuration stays active and the error (line,
field, reason) is shown in the window and in the console. `glyphpad check-config [file]`
validates a file without running anything. Unknown keys are errors, so typos do not go unnoticed.

## The shape of the file

```toml
config_version = 1          # the file's format; a newer one than the program knows is refused

[settings]                  # how GlyphPad behaves
[update]                    # automatic updates
[[gesture]]                 # one table per gesture, in priority order
```

Everything is optional; a missing value takes its default. Distances are **millimeters**, times
are **milliseconds** (`_ms`), seconds (`_s`) or hours (`_h`), as the name says.

**Priority.** Gestures are listed in priority order. When a movement matches several, the *most
specific* one fires (for example one with a condition on the program beats one without); among
equals, the **first in the file** wins.

## `[settings]`

### Thresholds

Global, millimeters. They define what counts as a tap, a hold, a swipe… For why the defaults
are what they are, see the comments in `crates/core/src/config.rs`.

| Key | Default | Range | Meaning |
|---|---|---|---|
| `tap_max_duration_ms` | `250` | 1–5000 | Longest touch still a tap |
| `tap_max_movement_mm` | `4.0` | > 0 | How far a finger may drift during a tap |
| `multi_tap_gap_ms` | `300` | 1–5000 | Longest pause between the taps of a double/triple tap |
| `hold_duration_ms` | `600` | 1–30000 | How long fingers rest before a hold fires |
| `hold_max_movement_mm` | `4.0` | > 0 | How far they may drift while resting |
| `swipe_min_distance_mm` | `18.0` | > 0 | Shortest travel of a swipe |
| `swipe_min_speed_mm_s` | `60.0` | ≥ 0 | Slowest average speed of a swipe (slower is a drag) |
| `pinch_min_mm` | `8.0` | > 0 | Change of the fingers' spread for a pinch |
| `rotate_min_deg` | `25.0` | 1–180 | Turn around the fingers' middle for a rotation |
| `edge_zone_percent` | `6.0` | 1–30 | Depth of the border zone where an edge swipe starts, in % of the touchpad |

### Drawn symbols

| Key | Default | Meaning |
|---|---|---|
| `draw_activation` | `{ mode = "fingers", count = 3 }` | How a touch becomes a stroke, see below |
| `stroke_gap_ms` | `400` (1–5000) | Strokes closer in time than this form one symbol; the pause that ends it |
| `min_stroke_mm` | `10.0` | Shortest stroke; anything shorter is a tap |
| `default_confidence` | `0.80` (0–1) | Minimum score of a symbol when the gesture sets no `confidence` |
| `rotation_tolerance_deg` | `0.0` (0–180) | How far a symbol may be turned and still match; 0 = orientation matters |

A positive `rotation_tolerance_deg` costs time: the recognizer tries the trace at a few coarse
angles and refines the best one in 5° steps, which takes about 12 ms at 10° and 40 ms at 180°
instead of 3 ms with orientation fixed.

`draw_activation` is an inline table with a `mode`:

```toml
draw_activation = { mode = "fingers", count = 3 }                      # draw with 3 fingers (1–10)
draw_activation = { mode = "edge", edge = "left", zone_percent = 10 }  # one finger from an edge zone
draw_activation = { mode = "hold", ms = 400 }                          # rest one finger 400 ms, then draw
draw_activation = { mode = "toggle", timeout_ms = 5000 }               # after a `draw_symbol` action
```

`edge` is `left`, `right`, `top` or `bottom`; `zone_percent` is 1–50; `ms` is 100–5000;
`timeout_ms` is 500–60000. The one-finger modes take `freeze_cursor = true` (experimental) to
pin the pointer while drawing. `count = 1` is accepted but warned about (every pointer movement
would be a stroke).

### Repeating the last gesture

| Key | Default | Meaning |
|---|---|---|
| `repeat_same_app_only` | `true` | `repeat_last` does nothing when another window is in front than where the gesture ran |
| `repeat_timeout_s` | none | Seconds after which `repeat_last` does nothing; unset = no limit |

### Window, tray and look

| Key | Default | Meaning |
|---|---|---|
| `close_to_tray` | `true` | The window's ✕ hides it instead of quitting |
| `start_minimized` | `false` | Start in the tray without the window |
| `autostart` | `false` | Start with Windows (a value under `HKCU\...\Run`; no administrator rights) |
| `theme` | `"system"` | `system`, `light` or `dark` |
| `language` | `"system"` | `system`, `english` or `italian` (`system` follows Windows, English if unsupported) |
| `history_size` | `20` (1–1000) | Recognitions kept in Review → History |
| `save_history` | `false` | Also append them to `history.jsonl` |
| `statistics` | `true` | Count gesture uses in `stats.json` (never leaves the computer) |
| `backup_count` | `10` (0–100) | Earlier versions of the file kept in `backups/` |

### Feedback overlay

```toml
overlay = { enabled = true, position = "bottom_right", duration_ms = 1500, opacity = 0.85, width_px = 240 }
```

`position` is `top_left`, `top_right`, `bottom_left` or `bottom_right`; `duration_ms` 100–30000;
`opacity` 0.05–1; `width_px` 80–2000 (the height follows the touchpad's proportions).

### Pausing

| Key | Default | Meaning |
|---|---|---|
| `pause_when_fullscreen` | `true` | Gestures pause while a full-screen program is in front |
| `pause_in_apps` | `[]` | …and while one of these executables is (`["game.exe"]`) |

### Profiles

| Key | Default | Meaning |
|---|---|---|
| `active_profile` | `"default"` | The profile in use. A gesture with a `profile` works only while it is active |
| `profiles` | `[]` | Profiles to list even if no gesture uses them yet |
| `profile_rules` | `[]` | Switch by itself while a program is in front (first match wins) |

```toml
active_profile = "default"
profiles = ["work", "media"]
profile_rules = [
    { app = ["code.exe", "devenv.exe"], profile = "work" },
    { app = "vlc.exe", profile = "media" },
]
```

A rule's switch is temporary and is never written to your file; `active_profile` is your own
choice and what applies when no rule matches.

### Cheat sheet

| Key | Default | Meaning |
|---|---|---|
| `cheat_sheet_s` | `8` (1–120) | Seconds the list of gestures stays on screen after `show_cheat_sheet` |

## `[update]`

### Signed releases

`SHA256SUMS` protects against a corrupt download, not against a release replaced by whoever got
into the GitHub account. For that the sums are signed with [minisign](https://jedisct1.github.io/minisign/),
and the executable carries the public key, so an update cannot be pointed at another key.

Making the keys (once, on a machine of your own — never in the repository):

```sh
minisign -G -p glyphpad.pub -s glyphpad.key
```

Then, on the GitHub repository:

| Kind | Name | Value |
|---|---|---|
| Variable | `MINISIGN_PUBKEY` | the contents of `glyphpad.pub` (the `RW…` line) |
| Secret | `MINISIGN_SECRET_KEY` | the contents of `glyphpad.key` |
| Secret | `MINISIGN_PASSWORD` | the password chosen when the key was made |

The build compiles `MINISIGN_PUBKEY` in (`GLYPHPAD_MINISIGN_PUBKEY`), and the release workflow
signs the sums in `dist/` with the secret. Without `MINISIGN_SECRET_KEY` the release is published
unsigned, with a warning in the workflow, and builds made without a key say in the log that they
cannot check signatures.


| Key | Default | Meaning |
|---|---|---|
| `mode` | `"notify"` | `off` (only when you ask), `notify` (a banner offers it), `auto` (installs and restarts) |
| `channel` | `"stable"` | `stable` or `prerelease` |
| `check_interval_h` | `24` | Hours between checks |

## `[[gesture]]`

One table per gesture:

```toml
[[gesture]]
name = "lock"                 # required, unique
type = "symbol"               # required: tap hold swipe edge_swipe pinch rotate click symbol sequence
symbol = "L"
action = { type = "lock_screen" }   # required
```

### Fields by type

| `type` | Fields (required in **bold**) |
|---|---|
| `tap` | **`fingers`** 1–10, `taps` 1–3 (default 1), `modifier_fingers` |
| `hold` | **`fingers`** |
| `swipe` | **`fingers`**, **`direction`**, `modifier_fingers` |
| `zone` | **`fingers`**, **`region`**, `taps` | A tap inside a rectangle of the pad, in millimetres: `region = { x = [0, 25], y = [0, 20] }` is a 25 × 20 mm corner at the top left |
| `edge_swipe` | **`fingers`**, **`edge`**, **`direction`** |
| `pinch` | **`fingers`** ≥ 2, **`zoom`** (`in` = fingers closer, `out` = apart) |
| `rotate` | **`fingers`** ≥ 2, **`rotation`** (`clockwise`, `counterclockwise`) |
| `click` | **`fingers`**, `direction` (makes it click-and-drag) |
| `symbol` | **`symbol`** a name or a list (`["q", "X"]`), `confidence` 0–1 |
| `sequence` | **`steps`** 2–4 tables each with a `type` and its fields, `within_ms` (default 800, 100–5000) |

`within_ms` is the longest pause between the end of one step and the *start* of the next, not the
time the next step takes. Steps drawn with the drawing finger count (a three-finger swipe, say)
are recognized `stroke_gap_ms` after the fingers lift; that delay does not count, but a step drawn
closer than `stroke_gap_ms` to the previous one still belongs to the same symbol, so a drawing
that matches no configured symbol is judged stroke by stroke, which is what makes two quick
swipes a sequence.

A step is over when its touch is: a `hold` is recognized `hold_duration_ms` into the touch, but its
window starts when the fingers lift, so how long they rest on afterwards does not matter.

A step may set its own window: `within_ms` inside a step is the pause allowed after *that* step,
overriding the sequence's — handy when a quick tap has to be followed at once while a drawn step
may take longer.

```toml
[[gesture]]
name = "tap-then-down"
type = "sequence"
within_ms = 1500
steps = [
    { type = "tap", fingers = 3, within_ms = 300 },   # the swipe has to follow at once
    { type = "swipe", fingers = 3, direction = "down" },
]
action = { type = "maximize" }
```

**Per-gesture thresholds.** `min_distance_mm` (swipe, edge_swipe), `min_speed_mm_s` (swipe),
`min_change_mm` (pinch) and `min_deg` (rotate) make one gesture stricter — or looser — than the
global setting, so two gestures on the same movement can ask for different lengths. The engine
reports the loosest of them and each gesture then checks its own.

When a sequence does not fire, the log says which of the two it was: `sequence "name" not
completed: no next step began within its within_ms` (too slow — raise `within_ms` or do it faster)
or `... the next gesture was not its next step` (the pad saw something else: check the steps with
`glyphpad replay <capture> --config <file> --all`, which lists every gesture the engine recognizes
regardless of what is configured).

A gesture that matches nothing also says what the configuration has with that many fingers
(`hold fingers=3 matched no gesture; with 3 fingers the configuration has: tap with 3 fingers`),
because the common mistake is a gesture of the wrong kind: fingers resting longer than
`tap_max_duration_ms` (250 ms) are a **hold**, and a tap has to be quicker.


A `zone` gesture turns part of the touchpad into a button: the tap has to land inside the
rectangle, and the corner zones are the easiest to hit without looking. Millimetres, not
fractions, so a new touchpad does not move it:

```toml
[[gesture]]
name = "corner-lock"
type = "zone"
fingers = 1
region = { x = [0.0, 25.0], y = [0.0, 20.0] }   # top left
action = { type = "lock_screen" }
```

`direction` is `up`, `down`, `left`, `right`, `up_left`, `up_right`, `down_left` or `down_right`.
`modifier_fingers` (1–3) makes the gesture a **chord**: that many other fingers must rest on the
touchpad while the others tap or swipe. Fields that do not belong to the type are errors.

### Fields for every type

| Field | Default | Meaning |
|---|---|---|
| `enabled` | `true` | `false` keeps the gesture in the file but inactive |
| `profile` | none | Works only while this profile is active |
| `when` | none | Conditions, see [Conditions](#conditions) |
| `repeatable` | automatic | Whether `repeat_last` may repeat it; by default everything except `lock_screen`, `toggle_enabled`, `show_window`, `repeat_last`, `draw_symbol`, the profile actions and `show_cheat_sheet` |

### Examples

```toml
[[gesture]]
name = "terminal"
type = "tap"
fingers = 4
action = { type = "run", command = "wt.exe" }

[[gesture]]
name = "next-tab-in-browser"
type = "swipe"
fingers = 3
direction = "right"
when = { app = ["chrome.exe", "firefox.exe"] }
action = { type = "keys", combo = "Ctrl+Tab" }

[[gesture]]
name = "undo"                 # two fingers swipe left while one rests
type = "swipe"
fingers = 2
modifier_fingers = 1
direction = "left"
action = { type = "keys", combo = "Ctrl+Z" }

[[gesture]]
name = "tap-then-up"          # a sequence
type = "sequence"
steps = [{ type = "tap", fingers = 3 }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 800
action = { type = "maximize" }

[[gesture]]
name = "close-app"
type = "symbol"
symbol = ["q", "X"]
confidence = 0.85
action = { type = "close_window", force = false }
```

## Actions

`action = { type = "...", ... }`. All of these also work as steps of a `chain`, except those
marked *app*.

| `type` | Fields | Notes |
|---|---|---|
| `keys` | `combo` *or* `sequence` | `combo = "Ctrl+Shift+T"`; `sequence = ["Ctrl+C", "Alt+Tab"]` presses each in turn |
| `lock_screen` | | Locks the workstation |
| `close_window` | `force` (default `false`) | Closes the foreground window; `force = true` ends its process |
| `minimize` | | Foreground window |
| `maximize` | | Maximizes, or restores if already maximized |
| `run` | **`command`**, `args` (list), `hidden`, `cwd`, `shell`, `env`, `wait`, `timeout_ms` | `hidden` runs a console tool without a console window; `shell = true` goes through the command interpreter; `env` adds variables; `wait = true` waits up to `timeout_ms` (default 30000) |
| `focus_or_launch` | **`app`**, **`command`**, `args` | Brings the first window of `app` (e.g. `code.exe`) to the front, or starts `command` when it is not running |
| `scroll` | **`amount`** | Turns the wheel: one step per notch, positive scrolls down |
| `mouse_click` | `button` (default `left`), `double` | Clicks where the pointer is: `left`, `right` or `middle` |
| `drag` | **`dx`**, **`dy`** | Presses the left button, moves the pointer by those pixels and releases it |
| `media` | **`key`** | `play_pause`, `next`, `previous`, `stop`, `volume_up`, `volume_down`, `mute` |
| `chain` | **`actions`** (a list) | Runs them in order; no `chain` inside a `chain` |
| `wait` | **`ms`** (≤ 60000) | A pause, for use inside a `chain` |
| `notify` | **`message`**, `title` | Shown in GlyphPad's overlay |
| `open` | **`target`** | A file, folder or web address, opened with its default program |
| `type_text` | **`text`** (1–2000 characters) | Types the text; `\n` presses Enter |
| `copy` | **`text`** | Puts the text on the clipboard, replacing what was there |
| `paste_text` | **`text`** | Puts the text on the clipboard and sends `Ctrl+V` |
| `window` | **`op`** | `snap_left`, `snap_right`, `next_monitor`, `toggle_always_on_top` |
| `virtual_desktop` | **`op`** | `previous`, `next`, `new` |
| `screenshot` | `mode` (default `region`) | `region` (snipping overlay) or `screen` (whole screen to Pictures\Screenshots) |
| `toggle_enabled` | | *app* — pauses or resumes every gesture |
| `repeat_last` | | *app* — see below |
| `show_window` | | *app* — opens the window |
| `draw_symbol` | | *app* — arms the next one-finger symbol (draw mode `toggle`) |
| `switch_profile` | **`profile`** | *app* — makes it the active profile (saved in the file) |
| `next_profile` | | *app* — the next profile in the list, wrapping around |
| `show_cheat_sheet` | | *app* — the list of gestures that work now |

**`repeat_last`** runs again the action of the last recognized gesture, directly. It never
records itself as the last one; gestures can opt out with `repeatable = false`; and by default it
does nothing if the window in front is not the one where the gesture ran (`repeat_same_app_only`).

Actions aimed at a window running as administrator do nothing unless GlyphPad runs as
administrator too.

`run` starts the program detached unless `wait = true`. With `shell = true` the command goes
through the system's command interpreter (`cmd /C` on Windows), so pipes, redirection, `&&` and
`%VARIABLES%` work; `args` are appended to that line, quoted when they contain a space:

```toml
action = { type = "run", command = "git pull && npm test", shell = true, wait = true, timeout_ms = 120000, cwd = "C:\\dev\\app" }
action = { type = "run", command = "powershell", args = ["-NoProfile", "-File", "C:\\scripts\\night.ps1"], hidden = true, env = { NIGHT = "1" } }
```

A program that exits with an error is logged and the next action of a `chain` runs anyway: the
action set has no condition on an exit code.

### Placeholders

In the texts of `run` (command, arguments, working folder, environment values), `open`, `notify`
and `type_text`, these are replaced when the action runs:

| Placeholder | Value |
|---|---|
| `{{app}}` | Executable name of the window in front, e.g. `chrome.exe` |
| `{{title}}` | Its window title |
| `{{profile}}` | The active profile |
| `{{date}}` | Local date, `YYYY-MM-DD` |
| `{{time}}` | Local time, `HH:MM` |
| `{{clipboard}}` | The clipboard's text (read only when the action asks for it) |

Anything else between braces is left alone. The clipboard is read when the gesture runs, so an
action that copies and one that pastes cannot be chained to the same text; `copy` and `paste_text`
do that.

```toml
action = { type = "run", command = "code", args = ["C:/notes/{{date}}.md"], shell = false }
```

## Conditions

```toml
when = { app = ["chrome.exe", "firefox.exe"], title = "^Inbox" }
```

- `app`: an executable name or a list; compared without regard to case.
- `title`: a regular expression ([Rust `regex` syntax](https://docs.rs/regex/latest/regex/#syntax))
  matched against the foreground window's title (anywhere in it; use `^`/`$` to anchor).

Both must match when both are given. Together with `profile` they are the only conditions.

## Shortcuts

`Ctrl`, `Alt`, `Shift` and `Win` are the modifiers; separate with `+`: `Ctrl+Shift+T`,
`Alt+F4`, `Win+D`. Keys: letters `A`–`Z`, digits `0`–`9`, `F1`–`F24`, `Tab`, `Enter`, `Escape`,
`Space`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`,
`Left`, `Right`, `PrintScreen`, `Menu`, `Minus`, `Plus`, `Comma`, `Period`, `Semicolon`,
`Slash`, `Backquote`, `BracketLeft`, `BracketRight`, `Backslash`, `Quote`, and the media keys
(`MediaPlayPause`, `MediaNext`, `MediaPrevious`, `MediaStop`, `VolumeUp`, `VolumeDown`,
`VolumeMute`). Common aliases work too (`Esc`, `Del`, `PgUp`, `PgDn`, `Return`, `Control`, `Super`). A lone modifier (`Win`) taps that key. `Win+L` and `Ctrl+Alt+Delete` cannot be
sent by software: `check-config` warns, and the *lock_screen* action does the first.

## Files GlyphPad keeps

All next to `glyphpad.toml`:

| File | What |
|---|---|
| `glyphpad.toml` | The configuration. Written atomically (temp file + rename), comments and formatting preserved |
| `templates.json` | Your recorded symbol examples, with the time of every point (so replays are faithful) |
| `backups/glyphpad-YYYYMMDD-HHMMSS-NNN.toml` | Earlier versions of the configuration |
| `history.jsonl` | The recognition history, if `save_history` is on |
| `stats.json` | Usage counters, if `statistics` is on |
| `glyphpad.log` (+ `glyphpad.log.1`) | The log, in portable mode only; rotated at 1 MB |
| `update-state.json` | When updates were last checked, the ETag, the ignored version |

Next to `glyphpad.exe`, only while updating: `glyphpad.old.exe` (the previous version, removed on
the next start) and a `glyphpad-update` folder (removed when done).

## Command line

See the [user guide](USER_GUIDE.md#17-the-command-line) for the table. In short:
`run` (default), `check-config`, `record`, `debug`, `dump-descriptor`, `capture`, `replay`,
`update [--check]`, `--version`; global options `--config <file>` and `--toplevel-window`.
