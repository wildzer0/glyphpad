//! Pure transformations of a *Keep a Changelog* formatted `CHANGELOG.md`.

use anyhow::{Context, Result, bail};

const UNRELEASED: &str = "## [Unreleased]";

/// A link reference definition such as `[Unreleased]: https://…`.
fn is_link_def(line: &str) -> bool {
    line.starts_with('[') && line.contains("]: ")
}

fn is_version_heading(line: &str) -> bool {
    line.starts_with("## [")
}

/// Version named by a `## [x.y.z] - date` heading.
fn heading_version(line: &str) -> Option<&str> {
    line.strip_prefix("## [")?.split(']').next()
}

/// Index one past the last line of the section starting at `start`.
fn section_end(lines: &[&str], start: usize) -> usize {
    lines[start + 1..]
        .iter()
        .position(|l| is_version_heading(l) || is_link_def(l))
        .map_or(lines.len(), |i| start + 1 + i)
}

/// Returns the body of the section for `version`, trimmed, for use as release notes.
pub fn section(text: &str, version: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| is_version_heading(l) && heading_version(l) == Some(version))?;
    let end = section_end(&lines, start);
    Some(lines[start + 1..end].join("\n").trim().to_string())
}

/// Moves the `Unreleased` section under a new `version` heading dated `date`,
/// leaves an empty `Unreleased` section above it and updates the comparison
/// links at the bottom of the file.
pub fn release(text: &str, version: &str, date: &str, repo_url: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_end() == UNRELEASED)
        .context("CHANGELOG.md has no `## [Unreleased]` section")?;
    let end = section_end(&lines, start);
    if lines[start + 1..end].iter().all(|l| l.trim().is_empty()) {
        bail!("the `Unreleased` section of CHANGELOG.md is empty: nothing to release");
    }
    if lines
        .iter()
        .any(|l| is_version_heading(l) && heading_version(l) == Some(version))
    {
        bail!("CHANGELOG.md already has a section for {version}");
    }
    let previous = lines[end..]
        .iter()
        .find(|l| is_version_heading(l))
        .and_then(|l| heading_version(l));

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 4);
    out.extend(lines[..=start].iter().map(|l| l.to_string()));
    out.push(String::new());
    out.push(format!("## [{version}] - {date}"));
    out.extend(lines[start + 1..].iter().map(|l| l.to_string()));

    let new_links = [
        format!("[Unreleased]: {repo_url}/compare/v{version}...HEAD"),
        match previous {
            Some(prev) => format!("[{version}]: {repo_url}/compare/v{prev}...v{version}"),
            None => format!("[{version}]: {repo_url}/releases/tag/v{version}"),
        },
    ];
    match out.iter().position(|l| l.starts_with("[Unreleased]: ")) {
        Some(i) => {
            out.splice(i..=i, new_links);
        }
        None => {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend(new_links);
        }
    }

    let mut result = out.join("\n");
    result.push('\n');
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "https://github.com/o/r";

    const FIRST: &str = "\
# Changelog

## [Unreleased]

### Added
- Workspace skeleton.

[Unreleased]: https://github.com/o/r/commits/main
";

    #[test]
    fn first_release_links_to_tag() {
        let out = release(FIRST, "0.0.1-alpha.1", "2026-10-24", REPO).unwrap();
        assert_eq!(
            out,
            "\
# Changelog

## [Unreleased]

## [0.0.1-alpha.1] - 2026-10-24

### Added
- Workspace skeleton.

[Unreleased]: https://github.com/o/r/compare/v0.0.1-alpha.1...HEAD
[0.0.1-alpha.1]: https://github.com/o/r/releases/tag/v0.0.1-alpha.1
"
        );
    }

    #[test]
    fn later_release_compares_with_previous() {
        let first = release(FIRST, "0.1.0", "2026-10-24", REPO).unwrap();
        let next = first.replace(
            "## [Unreleased]\n",
            "## [Unreleased]\n\n### Fixed\n- A bug.\n",
        );
        let out = release(&next, "0.2.0", "2026-11-01", REPO).unwrap();
        assert!(out.contains("## [0.2.0] - 2026-11-01\n\n### Fixed\n- A bug.\n\n## [0.1.0]"));
        assert!(out.contains("[Unreleased]: https://github.com/o/r/compare/v0.2.0...HEAD\n"));
        assert!(out.contains("[0.2.0]: https://github.com/o/r/compare/v0.1.0...v0.2.0\n"));
        assert!(out.contains("[0.1.0]: https://github.com/o/r/releases/tag/v0.1.0\n"));
        assert_eq!(out.matches("[Unreleased]: ").count(), 1);
    }

    #[test]
    fn appends_links_when_missing() {
        let text = "# Changelog\n\n## [Unreleased]\n- Thing.\n";
        let out = release(text, "1.0.0", "2026-01-01", REPO).unwrap();
        assert!(out.ends_with(
            "- Thing.\n\n[Unreleased]: https://github.com/o/r/compare/v1.0.0...HEAD\n\
             [1.0.0]: https://github.com/o/r/releases/tag/v1.0.0\n"
        ));
    }

    #[test]
    fn rejects_empty_unreleased() {
        let text = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n- Old.\n";
        assert!(release(text, "0.2.0", "2026-02-01", REPO).is_err());
    }

    #[test]
    fn rejects_missing_unreleased() {
        assert!(release("# Changelog\n", "0.2.0", "2026-02-01", REPO).is_err());
    }

    #[test]
    fn rejects_duplicate_version() {
        let text = "## [Unreleased]\n- New.\n\n## [0.1.0] - 2026-01-01\n- Old.\n";
        assert!(release(text, "0.1.0", "2026-02-01", REPO).is_err());
    }

    #[test]
    fn extracts_section_body() {
        let out = release(FIRST, "0.0.1-alpha.1", "2026-10-24", REPO).unwrap();
        assert_eq!(
            section(&out, "0.0.1-alpha.1").unwrap(),
            "### Added\n- Workspace skeleton."
        );
        assert_eq!(section(&out, "Unreleased").unwrap(), "");
        assert!(section(&out, "9.9.9").is_none());
    }
}
