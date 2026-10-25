//! Gesture packs (SPEC §13): an archive of gestures and the symbol examples
//! they use, to back them up or share them. A `.glyphpad` file is a zip
//! with three entries: `manifest.json`, `gestures.toml` (the same
//! `[[gesture]]` tables as in the configuration) and `templates.json`.

use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};

use serde::{Deserialize, Serialize};

use crate::config::{self, GestureDef};
use crate::symbols::{Sample, TemplateFile};

pub const EXTENSION: &str = "glyphpad";
const FORMAT: u32 = 1;
/// A pack is a few kilobytes; anything bigger is not one.
const MAX_ENTRY: u64 = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("this is not a GlyphPad pack: {0}")]
    Invalid(String),
    #[error("the pack was made by a newer GlyphPad (format {0})")]
    Newer(u32),
    #[error("the gestures in the pack are not valid: {0}")]
    Gestures(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    name: String,
    glyphpad_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub name: String,
    pub gestures: Vec<GestureDef>,
    /// The examples of the symbols the gestures use.
    pub templates: BTreeMap<String, Vec<Sample>>,
}

#[derive(Serialize)]
struct GestureFile<'a> {
    gesture: &'a [GestureDef],
}

/// Symbols a gesture needs: its own, or those of its sequence's steps.
pub fn symbols_used(def: &GestureDef) -> Vec<String> {
    let mut out: Vec<String> = def.symbol.iter().flat_map(|s| s.to_vec()).collect();
    for step in def.steps.iter().flatten() {
        out.extend(step.symbol.iter().flat_map(|s| s.to_vec()));
    }
    out.sort();
    out.dedup();
    out
}

/// Writes a pack of `gestures` with the examples among `recorded` (the
/// user's own templates) that they use. Built-in symbols travel by name.
pub fn write(
    to: impl Write + Seek,
    name: &str,
    version: &str,
    gestures: &[GestureDef],
    recorded: &BTreeMap<String, Vec<Sample>>,
) -> Result<(), PackError> {
    let mut templates = TemplateFile::default();
    for symbol in gestures.iter().flat_map(symbols_used) {
        if let Some(samples) = recorded.get(&symbol) {
            templates.symbols.insert(symbol, samples.clone());
        }
    }
    let manifest = Manifest {
        format: FORMAT,
        name: name.to_string(),
        glyphpad_version: version.to_string(),
    };
    let toml = toml::to_string(&GestureFile { gesture: gestures })
        .map_err(|e| PackError::Gestures(e.to_string()))?;
    let mut zip = zip::ZipWriter::new(to);
    let options = zip::write::SimpleFileOptions::default();
    for (entry, bytes) in [
        (
            "manifest.json",
            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
        ),
        ("gestures.toml", toml),
        ("templates.json", templates.to_json()),
    ] {
        zip.start_file(entry, options)
            .map_err(|e| PackError::Invalid(e.to_string()))?;
        zip.write_all(bytes.as_bytes())?;
    }
    zip.finish()
        .map_err(|e| PackError::Invalid(e.to_string()))?;
    Ok(())
}

fn entry(zip: &mut zip::ZipArchive<impl Read + Seek>, name: &str) -> Result<String, PackError> {
    let file = zip
        .by_name(name)
        .map_err(|_| PackError::Invalid(format!("`{name}` is missing")))?;
    let mut text = String::new();
    file.take(MAX_ENTRY + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_ENTRY {
        return Err(PackError::Invalid(format!("`{name}` is too large")));
    }
    Ok(text)
}

/// Reads and validates a pack. Gestures go through the configuration's own
/// checks, so a damaged or hand-edited pack cannot bring in an invalid one.
pub fn read(from: impl Read + Seek) -> Result<Pack, PackError> {
    let mut zip = zip::ZipArchive::new(from).map_err(|e| PackError::Invalid(e.to_string()))?;
    let manifest: Manifest = serde_json::from_str(&entry(&mut zip, "manifest.json")?)
        .map_err(|e| PackError::Invalid(format!("manifest: {e}")))?;
    if manifest.format > FORMAT {
        return Err(PackError::Newer(manifest.format));
    }
    let gestures_text = entry(&mut zip, "gestures.toml")?;
    let loaded = config::parse(&gestures_text).map_err(|errors| {
        PackError::Gestures(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let templates = TemplateFile::parse(&entry(&mut zip, "templates.json")?)
        .map_err(PackError::Invalid)?
        .symbols;
    Ok(Pack {
        name: manifest.name,
        gestures: loaded.config.gestures.into_iter().map(|g| g.def).collect(),
        templates,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::symbols::TracePoint;

    fn gestures() -> Vec<GestureDef> {
        let text = r#"
[[gesture]]
name = "lock"
type = "symbol"
symbol = "L"
action = { type = "lock_screen" }

[[gesture]]
name = "terminal"
type = "tap"
fingers = 4
action = { type = "run", command = "wt.exe" }
"#;
        config::parse(text)
            .unwrap()
            .config
            .gestures
            .into_iter()
            .map(|g| g.def)
            .collect()
    }

    fn samples() -> BTreeMap<String, Vec<Sample>> {
        let p = |x, y| TracePoint {
            x_mm: x,
            y_mm: y,
            t_us: 0,
        };
        let sample = Sample {
            strokes: vec![vec![p(0.0, 0.0), p(0.0, 20.0), p(10.0, 20.0)]],
            width_mm: 100.0,
            height_mm: 70.0,
        };
        BTreeMap::from([
            ("L".to_string(), vec![sample.clone()]),
            ("unused".to_string(), vec![sample]),
        ])
    }

    #[test]
    fn a_pack_round_trips_with_only_the_symbols_it_uses() {
        let mut buffer = Cursor::new(Vec::new());
        write(&mut buffer, "My pack", "1.0.0", &gestures(), &samples()).unwrap();
        let pack = read(Cursor::new(buffer.into_inner())).unwrap();
        assert_eq!(pack.name, "My pack");
        assert_eq!(pack.gestures, gestures());
        assert_eq!(pack.templates.keys().collect::<Vec<_>>(), ["L"]);
        assert_eq!(pack.templates["L"], samples()["L"]);
    }

    #[test]
    fn things_that_are_not_packs_are_refused() {
        assert!(matches!(
            read(Cursor::new(b"not a zip".to_vec())),
            Err(PackError::Invalid(_))
        ));
        // A zip without the entries.
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            zip.start_file("other.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        assert!(matches!(
            read(Cursor::new(buffer.into_inner())),
            Err(PackError::Invalid(_))
        ));
    }

    #[test]
    fn invalid_gestures_and_newer_formats_are_refused() {
        let build = |manifest: &str, gestures: &str| {
            let mut buffer = Cursor::new(Vec::new());
            {
                let mut zip = zip::ZipWriter::new(&mut buffer);
                for (n, b) in [
                    ("manifest.json", manifest),
                    ("gestures.toml", gestures),
                    ("templates.json", "{\"format\":1,\"symbols\":{}}"),
                ] {
                    zip.start_file(n, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    zip.write_all(b.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            buffer.into_inner()
        };
        let ok_manifest = r#"{"format":1,"name":"x","glyphpad_version":"1"}"#;
        let bad = "[[gesture]]\nname = \"a\"\ntype = \"tap\"\n";
        assert!(matches!(
            read(Cursor::new(build(ok_manifest, bad))),
            Err(PackError::Gestures(_))
        ));
        let newer = r#"{"format":99,"name":"x","glyphpad_version":"9"}"#;
        assert!(matches!(
            read(Cursor::new(build(newer, ""))),
            Err(PackError::Newer(99))
        ));
    }
}
