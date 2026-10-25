//! The window's languages (SPEC §13): English, which the code is written
//! in, and Italian. Texts in the code are English literals; showing one in
//! Italian is a lookup in a table, and a text without an entry is shown in
//! English rather than not at all.
//!
//! Only literals are translated: the widgets take `&'static str` for what
//! they show, so a name the user typed (a gesture, a profile) can never be
//! mistaken for a text of the program.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use glyphpad_core::config::Language;

mod it;

static ITALIAN: AtomicBool = AtomicBool::new(false);

static TABLE: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| it::TABLE.iter().copied().collect());

/// Whether a system locale such as `it-IT` or `it_IT.UTF-8` is Italian.
fn is_italian_locale(locale: &str) -> bool {
    locale
        .get(..2)
        .is_some_and(|l| l.eq_ignore_ascii_case("it"))
}

/// Applies the language setting; "system" follows the system's locale.
pub fn set(language: Language) {
    let italian = match language {
        Language::English => false,
        Language::Italian => true,
        Language::System => sys_locale::get_locale().is_some_and(|l| is_italian_locale(&l)),
    };
    ITALIAN.store(italian, Ordering::Relaxed);
}

pub fn italian() -> bool {
    ITALIAN.load(Ordering::Relaxed)
}

/// `text` in the current language.
pub fn tr(text: &'static str) -> &'static str {
    if italian() {
        TABLE.get(text).copied().unwrap_or(text)
    } else {
        text
    }
}

/// A translated template with `{}` replaced, in order, by `args`.
pub fn tf(template: &'static str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = tr(template);
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        out.push_str(args.next().copied().unwrap_or(""));
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// A plural: `one` for 1, `many` otherwise, both translated.
pub fn plural(n: usize, one: &'static str, many: &'static str) -> String {
    tf(if n == 1 { one } else { many }, &[&n.to_string()])
}

/// A sentence for what a trigger asks the user to do.
pub fn describe_trigger(t: &glyphpad_core::trigger::Trigger) -> String {
    if !italian() {
        return t.describe();
    }
    use glyphpad_core::config::Edge;
    use glyphpad_core::trigger::{Trigger, Turn, Zoom};
    let dita = |n: u8| {
        if n == 1 {
            "1 dito".to_string()
        } else {
            format!("{n} dita")
        }
    };
    let direction = |d: glyphpad_core::config::Direction| {
        use glyphpad_core::config::Direction as D;
        match d {
            D::Up => "in alto",
            D::Down => "in basso",
            D::Left => "a sinistra",
            D::Right => "a destra",
            D::UpLeft => "in alto a sinistra",
            D::UpRight => "in alto a destra",
            D::DownLeft => "in basso a sinistra",
            D::DownRight => "in basso a destra",
        }
    };
    let edge = |e: Edge| match e {
        Edge::Left => "sinistro",
        Edge::Right => "destro",
        Edge::Top => "superiore",
        Edge::Bottom => "inferiore",
    };
    let resting = |m: u8| {
        if m > 0 {
            format!(
                ", mentre {} {}",
                dita(m),
                if m == 1 {
                    "resta appoggiato"
                } else {
                    "restano appoggiate"
                }
            )
        } else {
            String::new()
        }
    };
    match t {
        Trigger::Zone {
            fingers,
            taps,
            regions,
            click,
        } => {
            let what = if *taps == 1 {
                "tocco".to_string()
            } else {
                format!("{taps}× tocco")
            };
            let first = regions.first().map_or_else(
                || "da nessuna parte".to_string(),
                |r| format!("{}–{} × {}–{} mm", r.x[0], r.x[1], r.y[0], r.y[1]),
            );
            let what = if *click {
                format!("{what} o click")
            } else {
                what
            };
            if regions.len() > 1 {
                format!(
                    "{what} con {} in {first} e altre {}",
                    dita(*fingers),
                    regions.len() - 1
                )
            } else {
                format!("{what} con {} in {first}", dita(*fingers))
            }
        }
        Trigger::Tap {
            fingers,
            taps,
            modifiers,
        } => {
            let what = match taps {
                1 => "tocco",
                2 => "doppio tocco",
                _ => "triplo tocco",
            };
            format!("{what} con {}{}", dita(*fingers), resting(*modifiers))
        }
        Trigger::Hold { fingers } => format!("pressione prolungata con {}", dita(*fingers)),
        Trigger::Swipe {
            fingers,
            direction: d,
            modifiers,
            ..
        } => {
            format!(
                "scorri {} con {}{}",
                direction(*d),
                dita(*fingers),
                resting(*modifiers)
            )
        }
        Trigger::EdgeSwipe {
            edge: e,
            fingers,
            direction: d,
            ..
        } => format!(
            "scorri {} dal bordo {} con {}",
            direction(*d),
            edge(*e),
            dita(*fingers)
        ),
        Trigger::Pinch { fingers, zoom, .. } => format!(
            "{} con {}",
            if *zoom == Zoom::In {
                "pizzica per stringere"
            } else {
                "pizzica per allargare"
            },
            dita(*fingers)
        ),
        Trigger::Rotate { fingers, turn, .. } => format!(
            "ruota {} con {}",
            if *turn == Turn::Clockwise {
                "in senso orario"
            } else {
                "in senso antiorario"
            },
            dita(*fingers)
        ),
        Trigger::Click {
            fingers,
            direction: d,
        } => match d {
            Some(d) => format!("clic e trascina {} con {}", direction(*d), dita(*fingers)),
            None => format!("clic con {}", dita(*fingers)),
        },
        Trigger::Symbol { names, .. } => format!("disegna {}", names.join(" o ")),
    }
}

/// Runs `f` in `language`, one test at a time: the language is global, and
/// the machine running the tests may well be set to a different one.
#[cfg(test)]
pub fn with_language<R>(language: Language, f: impl FnOnce() -> R) -> R {
    static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = LANGUAGE.lock().unwrap_or_else(|e| e.into_inner());
    set(language);
    let result = f();
    set(Language::English);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_are_recognized() {
        assert!(is_italian_locale("it-IT"));
        assert!(is_italian_locale("it_IT.UTF-8"));
        assert!(is_italian_locale("IT"));
        assert!(!is_italian_locale("en-US"));
        assert!(!is_italian_locale("i"));
        assert!(!is_italian_locale(""));
    }

    #[test]
    fn templates_fill_in_order_and_plurals_follow_the_count() {
        with_language(Language::English, || {
            assert_eq!(tf("{} of {} done", &["2", "5"]), "2 of 5 done");
            assert_eq!(plural(1, "{} gesture", "{} gestures"), "1 gesture");
            assert_eq!(plural(3, "{} gesture", "{} gestures"), "3 gestures");
        });
    }

    #[test]
    fn the_table_has_no_duplicate_keys_and_no_empty_entries() {
        let mut seen = std::collections::HashSet::new();
        for (en, it) in it::TABLE {
            assert!(seen.insert(*en), "duplicate key: {en}");
            assert!(!it.is_empty(), "empty translation of {en}");
            // Placeholders must survive translation.
            assert_eq!(en.matches("{}").count(), it.matches("{}").count(), "{en}");
        }
    }

    #[test]
    fn italian_descriptions_cover_every_kind() {
        use glyphpad_core::trigger::TriggerDef;
        let texts = [
            "type = \"tap\"\nfingers = 3\ntaps = 2",
            "type = \"hold\"\nfingers = 1",
            "type = \"swipe\"\nfingers = 2\ndirection = \"up\"",
            "type = \"edge_swipe\"\nfingers = 1\nedge = \"left\"\ndirection = \"right\"",
            "type = \"pinch\"\nfingers = 2\nzoom = \"out\"",
            "type = \"rotate\"\nfingers = 2\nrotation = \"clockwise\"",
            "type = \"click\"\nfingers = 1",
            "type = \"symbol\"\nsymbol = [\"L\", \"q\"]",
        ];
        with_language(Language::Italian, || {
            for text in texts {
                let t = toml::from_str::<TriggerDef>(text).unwrap().build().unwrap();
                let d = describe_trigger(&t);
                assert!(!d.is_empty() && d != t.describe(), "{text}: {d}");
            }
        });
    }
}
