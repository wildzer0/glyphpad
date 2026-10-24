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
