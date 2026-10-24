//! Platform-independent keys and shortcuts such as `Ctrl+Shift+T`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    // Modifiers
    Ctrl,
    Shift,
    Alt,
    Win,
    /// `A`–`Z`
    Letter(char),
    /// `0`–`9`
    Digit(u8),
    /// `F1`–`F24`
    F(u8),
    Tab,
    Enter,
    Escape,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    PrintScreen,
    Menu,
    Minus,
    Plus,
    Comma,
    Period,
    Semicolon,
    Slash,
    Backslash,
    Quote,
    Backquote,
    BracketLeft,
    BracketRight,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    MediaStop,
    VolumeUp,
    VolumeDown,
    VolumeMute,
}

impl Key {
    pub fn is_modifier(self) -> bool {
        matches!(self, Key::Ctrl | Key::Shift | Key::Alt | Key::Win)
    }
}

const NAMED: &[(&str, Key)] = &[
    ("ctrl", Key::Ctrl),
    ("control", Key::Ctrl),
    ("shift", Key::Shift),
    ("alt", Key::Alt),
    ("win", Key::Win),
    ("super", Key::Win),
    ("meta", Key::Win),
    ("tab", Key::Tab),
    ("enter", Key::Enter),
    ("return", Key::Enter),
    ("esc", Key::Escape),
    ("escape", Key::Escape),
    ("space", Key::Space),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("del", Key::Delete),
    ("insert", Key::Insert),
    ("ins", Key::Insert),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pgup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("pgdn", Key::PageDown),
    ("up", Key::Up),
    ("down", Key::Down),
    ("left", Key::Left),
    ("right", Key::Right),
    ("printscreen", Key::PrintScreen),
    ("prtsc", Key::PrintScreen),
    ("menu", Key::Menu),
    ("minus", Key::Minus),
    ("-", Key::Minus),
    ("plus", Key::Plus),
    ("=", Key::Plus),
    ("comma", Key::Comma),
    (",", Key::Comma),
    ("period", Key::Period),
    (".", Key::Period),
    ("semicolon", Key::Semicolon),
    (";", Key::Semicolon),
    ("slash", Key::Slash),
    ("/", Key::Slash),
    ("backslash", Key::Backslash),
    ("\\", Key::Backslash),
    ("quote", Key::Quote),
    ("'", Key::Quote),
    ("backquote", Key::Backquote),
    ("`", Key::Backquote),
    ("[", Key::BracketLeft),
    ("bracketleft", Key::BracketLeft),
    ("]", Key::BracketRight),
    ("bracketright", Key::BracketRight),
    ("playpause", Key::MediaPlayPause),
    ("mediaplaypause", Key::MediaPlayPause),
    ("medianext", Key::MediaNext),
    ("mediaprevious", Key::MediaPrevious),
    ("mediastop", Key::MediaStop),
    ("volumeup", Key::VolumeUp),
    ("volumedown", Key::VolumeDown),
    ("volumemute", Key::VolumeMute),
];

impl FromStr for Key {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let lower = s.trim().to_ascii_lowercase();
        if let Some((_, k)) = NAMED.iter().find(|(n, _)| *n == lower) {
            return Ok(*k);
        }
        let mut chars = lower.chars();
        match (chars.next(), chars.next()) {
            (Some(c @ 'a'..='z'), None) => return Ok(Key::Letter(c.to_ascii_uppercase())),
            (Some(c @ '0'..='9'), None) => return Ok(Key::Digit(c as u8 - b'0')),
            _ => {}
        }
        if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
            && (1..=24).contains(&n)
        {
            return Ok(Key::F(n));
        }
        Err(format!("unknown key `{}`", s.trim()))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Letter(c) => write!(f, "{c}"),
            Key::Digit(d) => write!(f, "{d}"),
            Key::F(n) => write!(f, "F{n}"),
            other => {
                let name = format!("{other:?}");
                write!(f, "{name}")
            }
        }
    }
}

/// A shortcut: modifiers held while one key is pressed, e.g. `Ctrl+Shift+T`.
/// A lone modifier (`Win`) is allowed and taps that key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Key>,
    pub key: Key,
}

impl FromStr for KeyCombo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        // `Ctrl++` means Ctrl and the plus key.
        let normalized = s.replace("++", "+plus");
        let parts: Vec<&str> = normalized.split('+').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(format!("`{s}` is not a valid shortcut"));
        }
        let mut keys = parts
            .iter()
            .map(|p| p.parse::<Key>())
            .collect::<Result<Vec<_>, _>>()?;
        let key = keys.pop().ok_or_else(|| "empty shortcut".to_string())?;
        if let Some(k) = keys.iter().find(|k| !k.is_modifier()) {
            return Err(format!(
                "`{k}` in `{s}` is not a modifier; only the last key may be a normal key"
            ));
        }
        let mut modifiers = Vec::new();
        for k in keys {
            if !modifiers.contains(&k) {
                modifiers.push(k);
            }
        }
        Ok(Self { modifiers, key })
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.modifiers {
            write!(f, "{m}+")?;
        }
        write!(f, "{}", self.key)
    }
}

impl Serialize for KeyCombo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyCombo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combos() {
        let c: KeyCombo = "Ctrl+Shift+T".parse().unwrap();
        assert_eq!(c.modifiers, vec![Key::Ctrl, Key::Shift]);
        assert_eq!(c.key, Key::Letter('T'));
        assert_eq!(c.to_string(), "Ctrl+Shift+T");

        let c: KeyCombo = "alt + f4".parse().unwrap();
        assert_eq!((c.modifiers, c.key), (vec![Key::Alt], Key::F(4)));

        let c: KeyCombo = "Win".parse().unwrap();
        assert!(c.modifiers.is_empty() && c.key == Key::Win);

        let c: KeyCombo = "Ctrl++".parse().unwrap();
        assert_eq!(c.key, Key::Plus);

        assert_eq!("ctrl+tab".parse::<KeyCombo>().unwrap().key, Key::Tab);
        assert_eq!("VolumeUp".parse::<KeyCombo>().unwrap().key, Key::VolumeUp);
    }

    #[test]
    fn display_round_trips() {
        for (name, _) in NAMED {
            let key: Key = name.parse().unwrap();
            assert_eq!(key.to_string().parse::<Key>().unwrap(), key, "{name}");
        }
    }

    #[test]
    fn rejects_bad_combos() {
        assert!("Ctrl+".parse::<KeyCombo>().is_err());
        assert!("Ctrl+Bogus".parse::<KeyCombo>().is_err());
        assert!("A+B".parse::<KeyCombo>().is_err());
        assert!("F25".parse::<KeyCombo>().is_err());
        assert!("".parse::<KeyCombo>().is_err());
    }
}
