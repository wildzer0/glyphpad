# GlyphPad user guide

This guide takes you from "I just downloaded it" to "my touchpad does what I want". It
assumes nothing: if you can use a mouse and a text box, you can use GlyphPad. Everything
you can do in the window can also be done by editing `glyphpad.toml`; the
[configuration reference](CONFIGURATION.md) lists every option.

*(Versione italiana: [Guida utente](GUIDA_UTENTE.md).)*

**Contents**

1. [How it works](#1-how-it-works)
2. [Installing and starting](#2-installing-and-starting)
3. [Set up Windows first](#3-set-up-windows-first)
4. [A tour of the window](#4-a-tour-of-the-window)
5. [Your first gesture](#5-your-first-gesture)
6. [Kinds of gesture](#6-kinds-of-gesture)
7. [Drawing symbols](#7-drawing-symbols)
8. [Actions](#8-actions)
9. [Where and when a gesture works](#9-where-and-when-a-gesture-works)
10. [Profiles](#10-profiles)
11. [Helpers: test mode, repeat, cheat sheet, overlay, pause](#11-helpers)
12. [Review: templates, history, statistics](#12-review-templates-history-statistics)
13. [Settings](#13-settings)
14. [Sharing gestures](#14-sharing-gestures)
15. [Saving, backups and editing the file](#15-saving-backups-and-editing-the-file)
16. [Updates](#16-updates)
17. [The command line](#17-the-command-line)
18. [Troubleshooting](#18-troubleshooting)
19. [Limits worth knowing](#19-limits-worth-knowing)

---

## 1. How it works

Your touchpad reports, many times a second, where each finger is. Windows turns that into
pointer movement, scrolling and its own gestures. GlyphPad **listens to the same reports**
(Windows calls this *Raw Input*) and watches for the gestures you defined. When one happens,
it runs the action you attached to it.

Two consequences are worth keeping in mind:

- GlyphPad cannot *stop* Windows from reacting to the same touch. A swipe with three fingers
  does what you configured **and** what Windows' own three-finger gesture does, unless you
  turn Windows' gesture off ([section 3](#3-set-up-windows-first)).
- A single finger moves the pointer. That is why symbols are drawn with several fingers by
  default, or in another way Windows ignores ([section 7](#7-drawing-symbols)).

GlyphPad needs a **Precision Touchpad**, the standard most laptops use since about 2015.
Run `glyphpad dump-descriptor` in a terminal to see the touchpads it finds (name, size,
how many fingers it reports).

## 2. Installing and starting

1. Download the `.zip` from the [Releases](https://github.com/wildzer0/glyphpad/releases)
   page and unzip it anywhere (your Documents folder, a USB stick…). It contains
   `glyphpad.exe`, an example configuration, the README and the license.
2. Double-click `glyphpad.exe`.

**SmartScreen.** The first time, Windows may show "Windows protected your PC", because the
program is not code-signed yet. Choose *More info*, then *Run anyway*. To be sure the file
is the one that was published, compare its SHA-256 with the `SHA256SUMS` file on the release
page (`certutil -hashfile glyphpad.exe SHA256` prints it).

**Where things live.** GlyphPad looks for `glyphpad.toml` next to `glyphpad.exe`
(*portable mode*: copy the folder to another PC and everything comes along). If there is
none, it uses `%APPDATA%\GlyphPad\config\` and creates the file there from the example on
first start. Next to the configuration it also keeps `templates.json` (your recorded
symbols), `stats.json` (usage counters), `update-state.json` and a `backups` folder. The bar at the
bottom of the window shows where the configuration file is.

**The tray.** GlyphPad lives in the notification area (the `^` next to the clock). Click its
icon to open the window; right-click for the menu: *Open GlyphPad*, *Gestures enabled*,
*Profile*, *Reload configuration*, *Open configuration folder*, *Check for updates*, *Quit*.
The icon turns gray while gestures are paused. Closing the window with ✕ hides it to the tray
and gestures keep working; turn that off in Settings if you prefer ✕ to quit.

**Only one copy runs.** Starting GlyphPad again just brings the existing window forward.

**Start with Windows:** Settings → *Start with Windows*. **Start hidden:** *Start in the tray*.

**Removing it:** turn *Start with Windows* off, quit from the tray, delete the folder (and
`%APPDATA%\GlyphPad` if you did not use portable mode).

## 3. Set up Windows first

Windows has its own gestures for three and four fingers. If both Windows and GlyphPad react
to the same movement, you get both. In **Settings → Bluetooth & devices → Touchpad** set the
gestures you want GlyphPad to use to *Nothing*:

- **Three-finger gestures** (swipes and taps): *Nothing* — needed for drawing symbols with
  three fingers, which is the default.
- **Four-finger gestures** (swipes and taps): *Nothing*, if you use four-finger gestures.
- **Tap to click** and two-finger scroll can stay as they are.

GlyphPad reads these settings and warns you at start when the number of fingers you draw
with is still taken by Windows.

## 4. A tour of the window

The **sidebar** is always there:

- **Gestures**, **Review**, **Settings**, **Live debug**: the four pages.
- **The status card** (bottom left) tells whether gestures are active, paused, being
  recorded or in test mode. Its switches pause every gesture (*Gestures*) and turn
  *Test mode* on or off. With more than one profile it also lets you pick the profile.

The **bar at the bottom** shows where the configuration file is, whether your changes are
saved, and the **undo / redo** buttons (also Ctrl+Z and Ctrl+Y). There is no Save button:
**changes are saved by themselves** a moment after you stop editing, as long as they are
valid. If something stops a save (a field that is not valid, the file changed elsewhere), a
message at the top of the page says so and nothing is lost.

A **banner** at the top appears when a new version is available.

### Gestures

The list on the left shows your gestures in **priority order**: when two could fire for the
same movement, the one *higher in the list* wins (a more specific gesture, such as one with
a condition, wins first). Drag the dotted handle to reorder. Each row has a switch to
enable or disable it, an icon for its kind, its name, a short description and what it does.
A warning triangle means it may clash with another gesture; select it to read why.

Above the list: a search box, an *All / On / Off* filter and a type filter. Below it: the
**active profile** and a button to manage profiles.

Select a gesture and the right side shows its editor in three parts:

- **When**: what you do on the touchpad.
- **Then**: what GlyphPad does.
- **Only when**: where it works (apps, window title, profile).

The `…` menu of a gesture duplicates or deletes it (deleting asks first, and Ctrl+Z brings it
back). **Record a gesture** starts the guided recorder; **Add manually** creates a gesture
you fill in yourself; **Share** exports and imports gestures
([section 14](#14-sharing-gestures)).

### Review, Settings, Live debug

See [section 12](#12-review-templates-history-statistics), [section 13](#13-settings) and
[section 18](#18-troubleshooting) (Live debug is the page to open when something does not
behave).

## 5. Your first gesture

### A drawn symbol

1. **Gestures → Record a gesture → A symbol.** Type a name for the symbol (for example `S`
   or `star`). Choose how many examples you will draw (5 is a good start). The card tells
   you *how* you will draw: with 3 fingers by default.
2. **Start recording.** Draw the symbol on the touchpad, then pause: the pause (400 ms by
   default) ends an example. A symbol with several strokes (like an X) is fine: strokes drawn
   close together count as one example. The live area shows your fingers and the stroke; the
   counter shows how many examples you have. While recording, no gesture runs, so nothing
   can fire by mistake. Not happy with one? Press the bin on its thumbnail and draw another.
3. **Check.** GlyphPad compares your examples with each other and with the symbols that
   already exist. A bar shows how much each example resembles the others; an example that
   is unlike the rest is flagged, so you can discard it. If the new symbol looks too much
   like an existing one, you see them side by side. Discard what is off, or *Record again*.
4. **Name and action.** Give the gesture a name and choose what it does ([section 8](#8-actions)).
   The same form as the editor, with the same checks. A gesture that is not valid cannot be
   saved.
5. **Try it.** *Try it in test mode* lets you draw the symbol a few times and see what
   GlyphPad recognizes and with what score, without running the action.

### A movement (tap, swipe, pinch…)

**Record a gesture → A movement.** Do the movement a few times (3 or so). GlyphPad works
out what it was: the kind, the number of fingers, the direction, the number of taps, or
whether other fingers were resting. You get the guess filled in (and can change it), plus a
warning if an existing gesture already fires for the same movement.

You can also add a gesture without recording: **Add manually**, then choose the kind with the
tiles in *When*.

## 6. Kinds of gesture

Choose the kind with the tiles at the top of *When*.

| Kind | What you do | Options |
|---|---|---|
| **Tap** | Touch briefly with N fingers | fingers (1–10), taps (single, double, triple), *chord* (see below) |
| **Hold** | Rest N fingers without moving | fingers |
| **Swipe** | Slide N fingers in one direction | fingers, direction (8), chord |
| **Edge swipe** | Start at a border of the touchpad and slide in | fingers, which edge, direction |
| **Pinch** | Two or more fingers move closer together or apart | fingers (2+), closer / apart |
| **Rotate** | Two or more fingers turn around their middle | fingers (2+), clockwise / counterclockwise |
| **Click** | Press the touchpad down, optionally and drag | fingers, optional drag direction |
| **Symbol** | Draw a letter or shape | which symbols (one or several), minimum score |
| **Sequence** | Two to four gestures one after the other, within a time | the steps, the time between them |

**Chord.** For tap and swipe you can require that *other* fingers rest on the touchpad
meanwhile: "one finger rests, two fingers swipe left". Set *With resting fingers* and how
many.

**Priority and waiting.** If you configured a double tap with three fingers, a *single* tap
with three fingers has to wait a moment to see whether a second one follows. If you do not
use double taps with that many fingers, single taps fire at once. A sequence's first step
waits likewise for the next step.

**Thresholds.** How long a tap may last, how far fingers may drift, how long a hold takes,
how far a swipe must travel, and so on are global and measured in **millimeters**, so they
mean the same on every touchpad. They live in *Settings → Advanced*. The defaults come from
real recordings; change them only if gestures fire too easily or not enough.

## 7. Drawing symbols

### Why symbols need a special way to be drawn

One finger on the touchpad moves the pointer, and a quick touch clicks. GlyphPad cannot stop
Windows from doing that. So a symbol is drawn in a way Windows ignores or that GlyphPad can
tell apart from pointing. Choose in **Settings → Drawing symbols → Draw with**:

| Mode | How you draw | Good | Watch out |
|---|---|---|---|
| **Several fingers** (default, 3) | Put 3 fingers down and draw: the stroke is the middle of the fingers | Pointer does not move; most reliable | Windows' own 3-finger gestures must be off |
| **From an edge** | Start with one finger in a zone along an edge (left by default, 10 % deep) | One finger | The pointer moves while you draw, unless *freeze the pointer* is on |
| **After a rest** | Rest one finger for 400 ms, then draw | One finger, anywhere | Same pointer issue |
| **After a gesture** | Do a gesture whose action is *Draw the next symbol with one finger*, then draw | One finger, no zone | Needs that extra gesture |

*Freeze the pointer while drawing* (experimental) pins the pointer in place once a stroke is
detected. Taps may still click. If GlyphPad ever crashes while frozen, the next start
releases it.

### Templates and recognition

GlyphPad recognizes a symbol by comparing what you drew with **templates**: examples of that
symbol. It uses the $Q point-cloud recognizer: it does not matter where you draw, how big
you draw, or in which order or direction you draw the strokes; the shape matters. **Rotation
matters too**, by default: a tilted L is not an L (*Settings → Rotation tolerance* allows a
tilt if you want it).

- Built-in templates: **L, q, X, C, V, Z, O**, arrow up and arrow down.
- Your own examples (from the recorder, from *Review → Record more examples*, or from the
  history) are **added** to the built-in ones. If a built-in set does not suit you, switch
  *Built-in examples* off for that symbol in Review.
- Several symbols can trigger one gesture (`q` or `X` closes the window).
- A drawing needs a **score** (0 to 1) above the gesture's *Minimum score* (0.80 by default;
  each gesture can set its own). Below it, nothing happens, and the overlay shows the best
  guess with its score; that tells you whether to record more examples or lower the score.
- A drawing that matches no symbol but is a simple tap or swipe is treated as a tap or
  swipe, so three-finger swipes still work next to three-finger drawing (they fire a moment
  later, after the pause that would end a symbol).

### Getting good recognition

- Record **5–8 examples**, varied the way you really draw (fast, slow, big, small).
- Watch the check step: discard an example flagged as unlike the others.
- If two symbols get mixed up (a `C` and an `O`), record more of the one that loses, or give
  the important one a higher minimum score.
- *Review → History → a drawing → Add it as an example* turns a drawing that failed into a
  new example, or adds it to a different symbol if it was recognized as the wrong one.
- *Live debug* shows the scores of the last drawing.

## 8. Actions

Choose the action in *Then*. **Try it** runs it immediately so you can check it.

| Action | What it does |
|---|---|
| **Press a shortcut** | Presses a combination like `Ctrl+Shift+T`, or several in a row. Press **Record**, then the keys. *Win+L* and *Ctrl+Alt+Del* cannot be simulated; the Windows key and a few system shortcuts are taken before GlyphPad sees them: type those instead of recording them. |
| **Lock the PC** | Locks the workstation (the proper API; pressing Win+L cannot work). |
| **Close the window in front** | Asks the foreground window to close, like its ✕. With *Terminate the program* it ends the process (unsaved work is lost). Never touches the desktop or the taskbar. |
| **Minimize / Maximize** | Of the foreground window; maximize toggles back to normal. |
| **Run a program** | Starts a program with arguments and a working folder; *Without a console window* hides the console of a command-line tool. |
| **Media key** | Play/pause, next, previous, stop, volume up/down, mute. |
| **Several actions in a row** | A *chain*: shortcuts, programs, pauses, messages… run in order, with *Pause* steps between. |
| **Show a message** | Shows a message in GlyphPad's overlay (useful to see that a gesture fired). |
| **Pause / resume gestures** | Toggles all gestures. This one works even while paused. |
| **Repeat the last gesture** | Runs again the action of the last gesture ([section 11](#11-helpers)). |
| **Show the GlyphPad window** | Opens the window. |
| **Draw the next symbol with one finger** | For the *After a gesture* draw mode. |
| **Open a file, folder or address** | Opens it with the program Windows associates with it: a document, a folder in Explorer, a web page in the browser. |
| **Type a text** | Types a text (up to 2000 characters) into the window that has the focus; a new line presses Enter. Any language and symbol works. |
| **Arrange the window in front** | Fills the left or right half of the screen, moves to the next monitor, or turns "always on top" on and off. |
| **Switch virtual desktop** | Previous, next, or create a new one (Windows' own shortcuts). |
| **Take a screenshot** | *Choose an area* opens Windows' snipping overlay; *whole screen* saves a picture in *Pictures → Screenshots*. |
| **Switch to a profile / next profile** | Changes the active profile ([section 10](#10-profiles)). |
| **Show the list of gestures** | The cheat sheet ([section 11](#11-helpers)). |

Windows does not let a normal program send keys to, or arrange, a window that runs **as
administrator**. Gestures aimed at such a window do nothing; run GlyphPad as administrator
too if you really need that.

## 9. Where and when a gesture works

In *Only when*:

- **Only in these apps**: pick from the programs that have a window open now, or type an
  executable name (`chrome.exe`). Empty means everywhere. A gesture for "Chrome or Firefox"
  can send `Ctrl+Tab`; elsewhere the same swipe can do something else.
- **Window title**: a regular expression matched against the title (`^Inbox`), for gestures
  that should work only in one document or site.
- **Profile**: the gesture works only while that profile is in use; *All profiles* means
  always.
- **Repeatable**: whether *Repeat the last gesture* may run it again (default: yes, except
  lock, pause and window actions).

## 10. Profiles

A profile is a named set of gestures. A gesture with no profile always works; a gesture with
a profile works only while that profile is the active one. Use them to have, say, a *work*
set and a *media* set.

- **Create, rename, delete**: *Gestures → Manage* next to the active profile. Deleting a
  profile makes its gestures work everywhere.
- **Switch**: in the sidebar card, from the tray (*Profile*), in Settings, or with a gesture
  whose action is *Switch to a profile* or *Switch to the next profile*. The choice is saved.
- **Switch by itself**: *Settings → Profiles → Switch by itself*: "while `code.exe` is in
  front use *work*". The first matching rule wins; with none matching, your own choice
  applies. This never rewrites your file.

## 11. Helpers

- **Test mode** (sidebar). Gestures are recognized and shown (in the overlay, in *History*
  and *Live debug*) but their actions do not run. Use it to try new gestures. A drawing that
  scores badly is logged with the best candidate.
- **Repeat the last gesture.** A gesture such as a double tap with three fingers runs the
  action of the last gesture again, directly. It never repeats itself, ignores gestures
  marked as not repeatable, and by default does nothing if a *different window* is in front
  now (so a repeated "close window" cannot close the wrong program). Settings let you add a
  time limit.
- **Cheat sheet.** Give a gesture (for example a hold with four fingers) the action *Show the
  list of gestures*. A panel appears in the middle of the screen with the gestures that work
  right now (in the active profile and the program in front), each with a small picture of
  the movement and what it does. It disappears by itself (8 seconds by default).
- **Feedback overlay.** A small panel in a corner of the screen shows your stroke while you
  draw and then the recognized gesture, or "not recognized" with the best guess. Messages from
  *Show a message* appear there too. Position, size, opacity and duration are in Settings.
- **Automatic pause.** Gestures step aside while a full-screen program (a game, a
  presentation) is in front, and while any program you list is. The sidebar says why, and the
  tray icon turns gray.
- **Conflicts.** GlyphPad warns when two gestures could fire for the same movement, when two
  symbols look alike, and when a hold could pre-empt a chord.

## 12. Review: templates, history, statistics

- **Templates.** Every symbol with all its examples, each drawn on a silhouette with your
  touchpad's proportions. **Play** replays the strokes with their real timing (speed ¼×–2×,
  loop, a slider to scrub). *Record more examples* adds examples; the bin deletes one of
  yours. *Built-in examples* switches the built-in ones off or on for a symbol.
- **History.** The latest recognized gestures (20 by default; save them to a file in
  Settings): time, what was recognized, the score, the action, the program in front. Select
  one to replay its trace on top of the template that matched it, so you can see *why* it
  matched or failed, and add it as a new example of that symbol or of another one.
- **Statistics.** How many times each gesture ran, how many drawings were not recognized and
  how many symbols were recognized with no gesture to run. Gestures *never used* stand out,
  and a high number of unrecognized drawings tells you to record more examples. Counted only
  on your computer; turn off in Settings; *Reset the counts* clears it.

## 13. Settings

All of `[settings]` and `[update]` is here, grouped by what you want to do; the technical
thresholds are under **Advanced** (closed at first). Next to a value that differs from its
default there is a ↺ button to restore it. The full list with defaults and ranges is in the
[configuration reference](CONFIGURATION.md#settings).

- **Appearance**: theme (*System* follows Windows, *Light*, *Dark*) and language (*System*,
  *English*, *Italiano*).
- **Window and tray**, **Drawing symbols**, **Profiles**, **Repeat the last gesture**,
  **Automatic pause**, **Feedback overlay**, **Cheat sheet**, **Updates**, **Backups**,
  **Advanced**.

## 14. Sharing gestures

**Gestures → Share**:

- **Export gestures…** Choose which gestures go in and a name, then *Save as…*. You get a
  `.glyphpad` file holding the gestures and the examples of the symbols they use.
- **Import gestures…** Pick a `.glyphpad` file. A preview lists its gestures. For each one
  whose name already exists you choose *Keep both* (the new one is renamed), *Replace mine* or
  *Skip*; a warning shows when an imported gesture fires for the same movement as one of yours.
  Symbol examples can be switched off one by one. Nothing changes until you press *Import*, and
  the import can be undone with Ctrl+Z.

A pack is a plain zip file (`manifest.json`, `gestures.toml`, `templates.json`); it contains no
program, only data, and its gestures are checked like any configuration. Still, look at the
actions of a pack from somebody else before using it: a gesture can run a program.

## 15. Saving, backups and editing the file

**Autosave.** Every change is saved about a second after you stop, when it is valid. The bar at
the bottom says *All changes saved*.

**Backups.** Before the file changes, a copy goes to the `backups` folder (the last 10 are
kept; changes made close together share one copy). *Settings → Backups → Restore…* brings one
back (the current file is backed up first).

**Editing `glyphpad.toml` by hand.** Use any text editor. GlyphPad notices the change and loads
it (*hot reload*). If the file has a mistake it keeps running with the **last valid**
configuration and tells you the line and the field; `glyphpad check-config` checks a file
without running anything. If you changed the file while the window had unsaved edits, a message
offers *Load the file*, *Keep my changes* or *Show differences*. Comments and formatting in your
file are preserved when the window saves.

## 16. Updates

GlyphPad can check GitHub for a newer version (*Settings → Updates*):

- **Check for updates**: *Never* (only when you press *Check now*), *Tell me* (default: a
  banner offers the version with its notes) or *Install automatically*.
- **Channel**: *Stable* or *Pre-releases*. **Check every**: 24 hours by default; the first
  check comes a few seconds after start.
- **Install and restart** downloads the package, checks it against the release's `SHA256SUMS`,
  replaces the executable and starts the new version. If the new version does not start within
  15 seconds, GlyphPad **goes back to the old one** and tells you. It never installs in the
  middle of a gesture, drawing or recording, and never goes back to an older version.
  *Ignore this version* hides a release you do not want.
- If the program's folder cannot be written (for example `Program Files`), GlyphPad cannot
  replace itself and gives you a link to download the release.

Releases are not signed yet, so the check protects against a damaged download but not against
a tampered GitHub account. Command line: `glyphpad update --check` / `glyphpad update`.

## 17. The command line

`glyphpad <command>`; with no command it runs. Useful ones:

| Command | What it does |
|---|---|
| `glyphpad` / `glyphpad run` | Starts GlyphPad |
| `glyphpad check-config [file]` | Validates a configuration and prints errors with line numbers |
| `glyphpad record <name> [--samples N] [--replace]` | Records examples of a symbol from the terminal |
| `glyphpad debug` | Opens the Live debug page in test mode and prints what is recognized, with scores |
| `glyphpad dump-descriptor` | Prints what GlyphPad knows about your touchpads |
| `glyphpad capture <file.jsonl> [--seconds N]` | Records the raw touches to a file (to report a problem or to test) |
| `glyphpad replay <file.jsonl> [--all] [--frames]` | Plays a capture back and prints what it recognizes |
| `glyphpad update [--check]` | Looks for a newer version, and installs it |
| `glyphpad --version` | Prints the version |

Global options: `--config <file>` to use another configuration, `--toplevel-window` as a
fallback if raw input does not arrive in the background. Add `RUST_LOG=debug` in the
environment for a detailed log on the console.

## 18. Troubleshooting

**Nothing happens when I make a gesture.**
Open *Live debug* and touch the touchpad. You should see your fingers. If not, GlyphPad does
not see your touchpad: run `glyphpad dump-descriptor`; a touchpad that is not a Precision
Touchpad is not supported. If the fingers show, check the status card (*Paused*? a
full-screen program in front? *Test mode*, which runs no actions?).

**It fires, but twice (my action and something else).**
Windows' own gesture is still on: see [section 3](#3-set-up-windows-first).

**My drawing is not recognized.**
Turn *Test mode* on, draw, and read the score in *Live debug* or the overlay. Below the gesture's
minimum score? Record more examples (varied), or lower *Minimum score* a little (0.75). Does it
look like another symbol? Check *Review → History* to see what it matched. Drawing with too
short strokes (under 10 mm) counts as a tap.

**The pointer jumps while I draw with one finger.**
Normal for the one-finger modes ([section 7](#7-drawing-symbols)): use several fingers, or try
*Freeze the pointer while drawing*.

**A gesture does nothing in one program only.**
That program may run as administrator, or draw its own window in a way shortcuts do not reach.
Try *Try it* in the editor with that program in front... or run GlyphPad as administrator.

**"Not saved: needs attention" at the bottom.**
A field is not valid (an empty name, a bad shortcut, a regular expression that does not compile).
The banner at the top says which; your edit is kept and saved as soon as it is valid.

**The tray icon is missing.**
It may be in the hidden icons (`^`). If Windows hides it, drag it to the visible area. Without a
tray, the window still works.

**SmartScreen or antivirus complains.**
The program is unsigned and reads the touchpad and sends keystrokes, which is exactly what some
security software looks for. Verify the download against `SHA256SUMS` and add an exception if you
trust it.

**I broke my configuration.**
Settings → Backups → *Restore…*, or delete `glyphpad.toml` and restart to get the example back.

**Reporting a problem.** Say your Windows version, GlyphPad's version, and what you did. If it is
about recognition, a capture (`glyphpad capture problem.jsonl`, then do the gesture) lets the
developer replay exactly what happened. Captures contain finger positions only, no keyboard data.

## 19. Limits worth knowing

- Windows only for now; Linux is planned.
- A click gesture cannot stop the click from reaching Windows.
- The touchpad reports a palm as a finger for a few milliseconds before flagging it; a session in
  which a finger turns out to be a palm produces no gesture.
- Gestures aimed at windows running as administrator do not work unless GlyphPad also runs as
  administrator.
- The overlay and cheat sheet are drawn with the classic Windows API and look slightly soft on
  high-DPI screens.
- Descriptions of recognized movements in *History* are in English whatever the language.
- More in [ARCHITECTURE.md](ARCHITECTURE.md#known-limitations).
