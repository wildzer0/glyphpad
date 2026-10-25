//! Changes to `glyphpad.toml` made from the GUI, applied with `toml_edit`
//! so the user's comments and formatting survive (SPEC §10.2).

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::config::{self, GestureDef};

fn to_edit_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::from(s.as_str()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::from(*b),
        toml::Value::Datetime(d) => Value::from(d.to_string()),
        toml::Value::Array(a) => {
            let mut out = Array::new();
            for x in a {
                out.push(to_edit_value(x));
            }
            Value::Array(out)
        }
        toml::Value::Table(t) => {
            let mut out = InlineTable::new();
            for (k, x) in t {
                out.insert(k, to_edit_value(x));
            }
            Value::InlineTable(out)
        }
    }
}

fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>().map_err(|e| e.to_string())
}

/// Checks that the edited text is still a valid configuration.
fn validated(doc: DocumentMut) -> Result<String, String> {
    let text = doc.to_string();
    config::parse(&text).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(text)
}

/// Appends a `[[gesture]]` table, with `action` and `when` as inline tables
/// like in hand-written files.
pub fn append_gesture(text: &str, def: &GestureDef) -> Result<String, String> {
    let mut doc = parse(text)?;
    let fields = toml::Table::try_from(def).map_err(|e| e.to_string())?;
    let mut table = Table::new();
    for (k, v) in &fields {
        table.insert(k, Item::Value(to_edit_value(v)));
    }
    match doc.get_mut("gesture") {
        None => {
            let mut array = ArrayOfTables::new();
            array.push(table);
            doc.insert("gesture", Item::ArrayOfTables(array));
        }
        Some(Item::ArrayOfTables(array)) => array.push(table),
        Some(_) => return Err("`gesture` in the file is not a list of [[gesture]] tables".into()),
    }
    validated(doc)
}

/// Sets a value such as `settings.close_to_tray` or
/// `settings.overlay.enabled`, creating tables as needed.
pub fn set_value<V: serde::Serialize>(
    text: &str,
    path: &[&str],
    value: V,
) -> Result<String, String> {
    let (key, tables) = path.split_last().ok_or("empty path")?;
    let value = toml::Value::try_from(value).map_err(|e| e.to_string())?;
    let mut doc = parse(text)?;
    let mut table: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for name in tables {
        if table.get(name).is_none() {
            table.insert(name, Item::Table(Table::new()));
        }
        table = table
            .get_mut(name)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| format!("`{name}` is not a table"))?;
    }
    match table.get_mut(key) {
        // Keep the decoration (spacing, trailing comment) of an existing value.
        Some(Item::Value(old)) => {
            let decor = old.decor().clone();
            *old = to_edit_value(&value);
            *old.decor_mut() = decor;
        }
        _ => {
            table.insert(key, Item::Value(to_edit_value(&value)));
        }
    }
    validated(doc)
}

/// Removes a value such as `settings.repeat_timeout_s`, so its default applies.
pub fn remove_value(text: &str, path: &[&str]) -> Result<String, String> {
    let (key, tables) = path.split_last().ok_or("empty path")?;
    let mut doc = parse(text)?;
    let mut table: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for name in tables {
        match table.get_mut(name).and_then(Item::as_table_like_mut) {
            Some(t) => table = t,
            None => return validated(doc),
        }
    }
    table.remove(key);
    validated(doc)
}

/// A gesture of the editor's list: its definition and, when it comes from
/// the file, its position there (so its comments and formatting survive).
#[derive(Debug, Clone, PartialEq)]
pub struct EditedGesture {
    pub origin: Option<usize>,
    pub def: GestureDef,
}

/// The value as TOML data, ignoring formatting.
fn semantic(v: &Value) -> Option<toml::Value> {
    let mut t: toml::Table = toml::from_str(&format!("x = {}", v.to_string().trim())).ok()?;
    t.remove("x")
}

/// Rewrites the `[[gesture]]` list in the given order. Gestures taken from
/// the file keep their table, with its comments and the formatting of every
/// value that did not change; new gestures are added as inline-table style
/// entries like hand-written ones.
pub fn write_gestures(text: &str, gestures: &[EditedGesture]) -> Result<String, String> {
    let mut doc = parse(text)?;
    let old: Vec<Table> = match doc.get("gesture") {
        None => Vec::new(),
        Some(Item::ArrayOfTables(a)) => a.iter().cloned().collect(),
        Some(_) => return Err("`gesture` in the file is not a list of [[gesture]] tables".into()),
    };
    let mut array = ArrayOfTables::new();
    for g in gestures {
        let fields = toml::Table::try_from(&g.def).map_err(|e| e.to_string())?;
        let mut table = g
            .origin
            .and_then(|i| old.get(i).cloned())
            .unwrap_or_default();
        let stale: Vec<String> = table
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| !fields.contains_key(k))
            .collect();
        for k in stale {
            table.remove(&k);
        }
        for (k, v) in &fields {
            let new = to_edit_value(v);
            match table.get_mut(k) {
                Some(Item::Value(old)) => {
                    if semantic(old) != Some(v.clone()) {
                        let decor = old.decor().clone();
                        *old = new;
                        *old.decor_mut() = decor;
                    }
                }
                _ => {
                    table.insert(k, Item::Value(new));
                }
            }
        }
        // Tables render in their recorded position; clearing it renders
        // them in list order.
        table.set_position(None);
        array.push(table);
    }
    if gestures.is_empty() {
        doc.remove("gesture");
    } else {
        doc.insert("gesture", Item::ArrayOfTables(array));
    }
    validated(doc)
}

/// A change to one value of the configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    Set(Vec<String>, toml::Value),
    Remove(Vec<String>),
}

/// Leaves of a serialized table as paths; `draw_activation` is one value
/// because its keys depend on its mode.
fn flatten(prefix: &[String], v: &toml::Value, out: &mut Vec<(Vec<String>, toml::Value)>) {
    match v {
        toml::Value::Table(t) if prefix.last().is_none_or(|k| k != "draw_activation") => {
            for (k, x) in t {
                let mut p = prefix.to_vec();
                p.push(k.clone());
                flatten(&p, x, out);
            }
        }
        other => out.push((prefix.to_vec(), other.clone())),
    }
}

fn leaves(section: &str, v: &impl serde::Serialize) -> Vec<(Vec<String>, toml::Value)> {
    let mut out = Vec::new();
    if let Ok(v) = toml::Value::try_from(v) {
        flatten(&[section.to_string()], &v, &mut out);
    }
    out
}

/// The edits that turn `base` settings into `draft`, value by value, so
/// unchanged values keep their formatting and comments.
pub fn settings_edits(
    base: &(crate::config::Settings, crate::config::UpdateSettings),
    draft: &(crate::config::Settings, crate::config::UpdateSettings),
) -> Vec<Edit> {
    let mut out = Vec::new();
    for (b, d) in [
        (leaves("settings", &base.0), leaves("settings", &draft.0)),
        (leaves("update", &base.1), leaves("update", &draft.1)),
    ] {
        for (path, value) in &d {
            if !b.iter().any(|(p, v)| p == path && v == value) {
                out.push(Edit::Set(path.clone(), value.clone()));
            }
        }
        for (path, _) in &b {
            if !d.iter().any(|(p, _)| p == path) {
                out.push(Edit::Remove(path.clone()));
            }
        }
    }
    out
}

pub fn apply_edits(text: &str, edits: &[Edit]) -> Result<String, String> {
    let mut text = text.to_string();
    for e in edits {
        text = match e {
            Edit::Set(path, value) => {
                let keys: Vec<&str> = path.iter().map(String::as_str).collect();
                set_value(&text, &keys, value)?
            }
            Edit::Remove(path) => {
                let keys: Vec<&str> = path.iter().map(String::as_str).collect();
                remove_value(&text, &keys)?
            }
        };
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::config::{GestureKind, OneOrMany, When};

    const FILE: &str = r#"# My gestures
[settings]
close_to_tray = true   # hide instead of quitting

# Lock with an L
[[gesture]]
name = "lock"
type = "symbol"
symbol = "L"
action = { type = "lock_screen" }
"#;

    fn def() -> GestureDef {
        GestureDef {
            name: "copy".into(),
            kind: GestureKind::Swipe,
            fingers: Some(4),
            taps: None,
            modifier_fingers: None,
            direction: Some(crate::config::Direction::Down),
            edge: None,
            zoom: None,
            rotation: None,
            symbol: None,
            confidence: None,
            min_distance_mm: None,
            min_speed_mm_s: None,
            min_change_mm: None,
            min_deg: None,
            steps: None,
            within_ms: None,
            action: Action::Chain {
                actions: vec![
                    Action::Keys {
                        combo: Some("Ctrl+C".parse().unwrap()),
                        sequence: vec![],
                    },
                    Action::Wait { ms: 100 },
                ],
            },
            enabled: true,
            profile: None,
            when: Some(When {
                app: Some(OneOrMany::Many(vec!["notepad.exe".into()])),
                title: None,
            }),
            repeatable: None,
        }
    }

    #[test]
    fn appends_a_gesture_keeping_comments() {
        let out = append_gesture(FILE, &def()).unwrap();
        assert!(out.starts_with(FILE), "{out}");
        let added = &out[FILE.len()..];
        assert!(added.contains(
            "[[gesture]]\nname = \"copy\"\ntype = \"swipe\"\nfingers = 4\ndirection = \"down\"\n"
        ));
        assert!(added.contains(r#"action = { type = "chain", actions = [{ type = "keys", combo = "Ctrl+C" }, { type = "wait", ms = 100 }] }"#), "{added}");
        assert!(added.contains(r#"when = { app = ["notepad.exe"] }"#));
        let parsed = config::parse(&out).unwrap().config;
        assert_eq!(parsed.gestures.len(), 2);
        assert_eq!(parsed.gestures[1].def, def());
    }

    const TWO: &str = r#"[settings]
close_to_tray = true

# First: lock
[[gesture]]
name = "lock"
type = "symbol"
symbol = "L"   # my L
action = { type = "lock_screen" }

# Second: show desktop
[[gesture]]
name = "desk"
type = "tap"
fingers = 4
action = { type = "keys", combo = "Win+D" }
"#;

    fn edited(text: &str) -> Vec<EditedGesture> {
        config::parse(text)
            .unwrap()
            .config
            .gestures
            .into_iter()
            .enumerate()
            .map(|(i, g)| EditedGesture {
                origin: Some(i),
                def: g.def,
            })
            .collect()
    }

    #[test]
    fn rewriting_unchanged_gestures_keeps_the_file() {
        assert_eq!(write_gestures(TWO, &edited(TWO)).unwrap(), TWO);
    }

    #[test]
    fn reorders_edits_and_deletes_keeping_comments() {
        let mut list = edited(TWO);
        list.swap(0, 1);
        list[0].def.fingers = Some(5);
        list[1].def.enabled = false;
        let out = write_gestures(TWO, &list).unwrap();
        let desk = out.find("name = \"desk\"").unwrap();
        let lock = out.find("name = \"lock\"").unwrap();
        assert!(desk < lock, "{out}");
        assert!(
            out.contains("# Second: show desktop\n[[gesture]]\nname = \"desk\""),
            "{out}"
        );
        assert!(out.contains("fingers = 5\n"), "{out}");
        assert!(out.contains("symbol = \"L\"   # my L"), "{out}");
        assert!(out.contains("enabled = false"), "{out}");
        let parsed = config::parse(&out).unwrap().config;
        assert_eq!(parsed.gestures[0].def.name, "desk");
        assert!(!parsed.gestures[1].def.enabled);

        // Delete the first, add a new one.
        let mut list = edited(&out);
        list.remove(0);
        list.push(EditedGesture {
            origin: None,
            def: def(),
        });
        let out = write_gestures(&out, &list).unwrap();
        let names: Vec<String> = config::parse(&out)
            .unwrap()
            .config
            .gestures
            .into_iter()
            .map(|g| g.def.name)
            .collect();
        assert_eq!(names, ["lock", "copy"]);
        assert!(!out.contains("desk"));
        assert!(out.contains("close_to_tray = true"));
    }

    #[test]
    fn settings_edits_change_only_what_changed() {
        use crate::config::{Settings, UpdateSettings};
        let base = (Settings::default(), UpdateSettings::default());
        let mut draft = base.clone();
        draft.0.close_to_tray = false;
        draft.0.overlay.opacity = 0.5;
        let edits = settings_edits(&base, &draft);
        assert_eq!(edits.len(), 2, "{edits:?}");
        let out = apply_edits(FILE, &edits).unwrap();
        assert!(
            out.contains("close_to_tray = false   # hide instead of quitting"),
            "{out}"
        );
        assert_eq!(
            config::parse(&out).unwrap().config.settings.overlay.opacity,
            0.5
        );
    }

    #[test]
    fn appends_to_a_file_without_gestures() {
        let out = append_gesture("", &def()).unwrap();
        assert_eq!(config::parse(&out).unwrap().config.gestures.len(), 1);
    }

    #[test]
    fn sets_values_keeping_comments() {
        let out = set_value(FILE, &["settings", "close_to_tray"], false).unwrap();
        assert!(
            out.contains("close_to_tray = false   # hide instead of quitting"),
            "{out}"
        );
        let out = set_value(&out, &["settings", "overlay", "opacity"], 0.5).unwrap();
        let s = config::parse(&out).unwrap().config.settings;
        assert!(!s.close_to_tray);
        assert_eq!(s.overlay.opacity, 0.5);
        // Invalid values are refused.
        assert!(set_value(FILE, &["settings", "history_size"], 0).is_err());
        let out = set_value(FILE, &["settings", "repeat_timeout_s"], 30).unwrap();
        let out = remove_value(&out, &["settings", "repeat_timeout_s"]).unwrap();
        assert_eq!(
            config::parse(&out)
                .unwrap()
                .config
                .settings
                .repeat_timeout_s,
            None
        );
        assert!(out.contains("# hide instead of quitting"));
    }
}
