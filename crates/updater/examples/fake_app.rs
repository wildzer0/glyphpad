//! A stand-in for GlyphPad in the updater's end-to-end tests: after an
//! update the "new version" behaves as `behavior.txt` next to it says.
//!
//! * `ok`: reports it is up (creates the marker) and exits after a few seconds;
//! * `crash`: exits at once with an error;
//! * `hang`: never reports.

use std::time::Duration;

fn main() {
    let exe = std::env::current_exe().expect("current exe");
    let dir = exe.parent().expect("parent");
    let behavior = std::fs::read_to_string(dir.join("behavior.txt")).unwrap_or_default();
    match behavior.trim() {
        "crash" => std::process::exit(3),
        "hang" => std::thread::sleep(Duration::from_secs(60)),
        _ => {
            std::fs::write(dir.join("glyphpad-update.ok"), b"ok").expect("marker");
            std::thread::sleep(Duration::from_secs(3));
        }
    }
}
