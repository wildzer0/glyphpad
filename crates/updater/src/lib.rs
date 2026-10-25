//! Update check, download, verification and executable replacement from the
//! project's GitHub Releases (SPEC §12).
//!
//! The work is split so each part can be tested alone: [`source`] finds
//! releases (GitHub, or a local folder for tests), [`package`] downloads and
//! verifies one, [`install`] swaps the executable and can undo it, and
//! [`state`] remembers what was checked.

pub mod install;
pub mod package;
pub mod source;
pub mod state;

use std::time::Duration;

pub use semver::Version;

/// Owner and name of the GitHub repository releases are fetched from.
///
/// Fixed at compile time and deliberately not configurable, so the updater
/// cannot be pointed at another repository (SPEC §12.1). This is the only
/// place in the code that names the repository.
pub const GITHUB_REPO: (&str, &str) = ("wildzer0", "glyphpad");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot reach the update server: {0}")]
    Network(String),
    #[error("GitHub's rate limit was reached; try again in {} minutes", .retry_after.as_secs().div_ceil(60))]
    RateLimited { retry_after: Duration },
    #[error("the update server answered with HTTP {0}")]
    Http(u16),
    #[error("unexpected answer from the update server: {0}")]
    Parse(String),
    #[error("release {0} has no package for this system ({1})")]
    NoPackage(String, String),
    #[error("release {0} has no SHA256SUMS file, so the download cannot be verified")]
    MissingChecksums(String),
    #[error("SHA256SUMS does not list {0}")]
    NotListed(String),
    #[error(
        "the download is corrupt or was tampered with (expected SHA-256 {expected}, got {actual})"
    )]
    ChecksumMismatch { expected: String, actual: String },
    #[error("the package is not valid: {0}")]
    Package(String),
    #[error("refusing to download from {0}: not a release of this project")]
    UntrustedUrl(String),
    #[error("cannot write next to the executable ({0}); update manually")]
    NotWritable(String),
    #[error("cannot install the update: {0}")]
    Install(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// Where to download it; its meaning depends on the source.
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub prerelease: bool,
    /// Release notes, as written (Markdown).
    pub notes: String,
    /// The release's page, for manual updates.
    pub page_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Prerelease,
}

/// Parses a tag such as `v0.3.0-beta.1`.
pub fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// The newest release that is strictly newer than `current` and allowed by
/// `channel`. Never an older or equal version: no downgrades (SPEC §12.1).
pub fn newest<'a>(
    releases: &'a [Release],
    current: &Version,
    channel: Channel,
) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|r| r.version > *current)
        .filter(|r| channel == Channel::Prerelease || (!r.prerelease && r.version.pre.is_empty()))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// The Rust target this executable was built for, as release packages are named.
pub fn target() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "unsupported"
    }
}

/// `glyphpad-<version>-<target>.zip` (Windows) or `.tar.gz` (SPEC §11.3).
pub fn package_name(version: &Version, target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("glyphpad-{version}-{target}.{ext}")
}

/// File name of the executable inside a package.
pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "glyphpad.exe"
    } else {
        "glyphpad"
    }
}

/// Name of the file listing the SHA-256 of every asset of a release.
pub const CHECKSUMS: &str = "SHA256SUMS";

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            version: parse_tag(tag).unwrap(),
            tag: tag.into(),
            prerelease,
            notes: String::new(),
            page_url: String::new(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn repo_is_set() {
        assert!(!GITHUB_REPO.0.is_empty() && !GITHUB_REPO.1.is_empty());
    }

    #[test]
    fn picks_the_newest_allowed_release_and_never_downgrades() {
        let all = [
            release("v0.1.0", false),
            release("v0.2.0", false),
            release("v0.3.0-beta.1", true),
            release("v0.2.5", false),
        ];
        let cur = |s: &str| Version::parse(s).unwrap();
        let tag = |r: Option<&Release>| r.map(|r| r.tag.clone());
        assert_eq!(
            tag(newest(&all, &cur("0.1.0"), Channel::Stable)),
            Some("v0.2.5".into())
        );
        assert_eq!(
            tag(newest(&all, &cur("0.1.0"), Channel::Prerelease)),
            Some("v0.3.0-beta.1".into())
        );
        // Already current or ahead: nothing, even if older releases exist.
        assert_eq!(tag(newest(&all, &cur("0.2.5"), Channel::Stable)), None);
        assert_eq!(tag(newest(&all, &cur("1.0.0"), Channel::Prerelease)), None);
        // A pre-release of the next version is newer than the current alpha.
        assert_eq!(
            tag(newest(&all, &cur("0.0.1-alpha.1"), Channel::Stable)),
            Some("v0.2.5".into())
        );
        // A pre-release is older than its release.
        let all = [release("v0.3.0", false), release("v0.3.0-rc.1", true)];
        assert_eq!(
            tag(newest(&all, &cur("0.3.0-rc.1"), Channel::Prerelease)),
            Some("v0.3.0".into())
        );
    }

    #[test]
    fn package_names_follow_the_release_convention() {
        let v = Version::parse("0.2.0").unwrap();
        assert_eq!(
            package_name(&v, "x86_64-pc-windows-msvc"),
            "glyphpad-0.2.0-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            package_name(&v, "x86_64-unknown-linux-gnu"),
            "glyphpad-0.2.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn tags_parse_with_or_without_v() {
        assert!(parse_tag("v1.2.3").is_some());
        assert!(parse_tag("1.2.3-rc.1").is_some());
        assert!(parse_tag("nightly").is_none());
    }
}
