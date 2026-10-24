//! Reading and updating `[workspace.package]` in the root `Cargo.toml`,
//! preserving comments and formatting.

use anyhow::{Context, Result};
use semver::Version;
use toml_edit::DocumentMut;

fn workspace_package_str<'a>(doc: &'a DocumentMut, key: &str) -> Result<&'a str> {
    doc.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_str())
        .with_context(|| format!("Cargo.toml has no string `workspace.package.{key}`"))
}

pub fn workspace_version(text: &str) -> Result<Version> {
    let doc: DocumentMut = text.parse().context("Cargo.toml is not valid TOML")?;
    let raw = workspace_package_str(&doc, "version")?;
    Version::parse(raw).with_context(|| format!("`{raw}` is not a valid SemVer version"))
}

pub fn workspace_repository(text: &str) -> Result<String> {
    let doc: DocumentMut = text.parse().context("Cargo.toml is not valid TOML")?;
    Ok(workspace_package_str(&doc, "repository")?
        .trim_end_matches('/')
        .to_string())
}

pub fn set_workspace_version(text: &str, version: &Version) -> Result<String> {
    let mut doc: DocumentMut = text.parse().context("Cargo.toml is not valid TOML")?;
    workspace_package_str(&doc, "version")?;
    let item = &mut doc["workspace"]["package"]["version"];
    // Replace only the string, keeping the surrounding decor (comments, spacing).
    let decor = item.as_value().map(|v| v.decor().clone());
    *item = toml_edit::value(version.to_string());
    if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
        *value.decor_mut() = decor;
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"[workspace]
members = ["crates/*"]

[workspace.package]
# Single source of truth.
version = "0.0.1-alpha.1"  # trailing
repository = "https://github.com/o/r/"

[profile.release]
lto = "fat"
"#;

    #[test]
    fn reads_version_and_repository() {
        assert_eq!(
            workspace_version(MANIFEST).unwrap(),
            Version::parse("0.0.1-alpha.1").unwrap()
        );
        assert_eq!(
            workspace_repository(MANIFEST).unwrap(),
            "https://github.com/o/r"
        );
    }

    #[test]
    fn sets_version_preserving_formatting() {
        let out = set_workspace_version(MANIFEST, &Version::parse("1.2.3").unwrap()).unwrap();
        assert_eq!(out, MANIFEST.replace(r#""0.0.1-alpha.1""#, r#""1.2.3""#));
    }

    #[test]
    fn missing_version_is_an_error() {
        assert!(workspace_version("[workspace]\n").is_err());
        assert!(set_workspace_version("[workspace]\n", &Version::new(1, 0, 0)).is_err());
    }
}
