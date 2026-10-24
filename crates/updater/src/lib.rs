//! Update check, download, verification and executable replacement from the
//! project's GitHub Releases (SPEC §12).

/// Owner and name of the GitHub repository releases are fetched from.
///
/// Fixed at compile time and deliberately not configurable, so the updater
/// cannot be pointed at another repository (SPEC §12.1). This is the only
/// place in the code that names the repository.
pub const GITHUB_REPO: (&str, &str) = ("wildzer0", "glyphpad");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_is_set() {
        assert!(!GITHUB_REPO.0.is_empty() && !GITHUB_REPO.1.is_empty());
    }
}
