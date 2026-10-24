//! Project automation, run as `cargo xtask <command>` (SPEC §11.1).

mod changelog;
mod manifest;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use semver::Version;

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check, bump the version, roll the changelog, then commit and tag (no push).
    Release {
        /// New version, without the leading `v` (e.g. 0.3.0-beta.1).
        version: Version,
        /// Run every check and show what would change, without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the CHANGELOG.md section of a version (used as release notes).
    Changelog { version: String },
    /// Fail unless the tag (e.g. v0.3.0) matches the workspace version.
    CheckTag { tag: String },
}

fn main() -> Result<()> {
    let root = workspace_root();
    match Cli::parse().command {
        Cmd::Release { version, dry_run } => release(&root, &version, dry_run),
        Cmd::Changelog { version } => {
            let text = read(&root.join("CHANGELOG.md"))?;
            let body = changelog::section(&text, &version)
                .with_context(|| format!("CHANGELOG.md has no section for {version}"))?;
            println!("{body}");
            Ok(())
        }
        Cmd::CheckTag { tag } => {
            let version = manifest::workspace_version(&read(&root.join("Cargo.toml"))?)?;
            check_tag(&tag, &version)?;
            println!("tag {tag} matches workspace version {version}");
            Ok(())
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

fn tag_name(version: &Version) -> String {
    format!("v{version}")
}

fn check_tag(tag: &str, version: &Version) -> Result<()> {
    ensure!(
        tag == tag_name(version),
        "tag `{tag}` does not match workspace version {version} (expected `{}`)",
        tag_name(version)
    );
    Ok(())
}

/// The new version must be greater than the current one. It may equal it only
/// while its tag does not exist yet, so the very first release can ship the
/// version the workspace was created with.
fn validate_target(current: &Version, new: &Version, tag_exists: bool) -> Result<()> {
    ensure!(!tag_exists, "tag {} already exists", tag_name(new));
    ensure!(
        new >= current,
        "{new} is lower than the current version {current}: downgrades are not allowed"
    );
    Ok(())
}

fn release(root: &Path, version: &Version, dry_run: bool) -> Result<()> {
    let cargo_toml_path = root.join("Cargo.toml");
    let changelog_path = root.join("CHANGELOG.md");
    let cargo_toml = read(&cargo_toml_path)?;
    let current = manifest::workspace_version(&cargo_toml)?;
    let tag = tag_name(version);

    let branch = git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    ensure!(
        branch == "main",
        "releases are made from `main`, not `{branch}`"
    );
    let dirty = git_output(root, &["status", "--porcelain"])?;
    ensure!(dirty.is_empty(), "the working tree is not clean:\n{dirty}");
    let tag_exists = Command::new("git")
        .args(["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")])
        .current_dir(root)
        .output()
        .context("cannot run git")?
        .status
        .success();
    validate_target(&current, version, tag_exists)?;

    // Compute every change up front so a malformed file aborts before the slow checks.
    let date = jiff::Zoned::now().date().to_string();
    let repo_url = manifest::workspace_repository(&cargo_toml)?;
    let new_changelog = changelog::release(
        &read(&changelog_path)?,
        &version.to_string(),
        &date,
        &repo_url,
    )?;
    let new_cargo_toml = manifest::set_workspace_version(&cargo_toml, version)?;

    cargo(root, &["fmt", "--all", "--check"])?;
    cargo(
        root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    cargo(root, &["test", "--workspace"])?;

    if dry_run {
        println!("\n[dry run] would release {current} -> {version} ({date}) with notes:\n");
        println!(
            "{}",
            changelog::section(&new_changelog, &version.to_string()).unwrap_or_default()
        );
        println!("\n[dry run] would commit `chore(release): {tag}` and create tag {tag}");
        return Ok(());
    }

    std::fs::write(&cargo_toml_path, new_cargo_toml).context("cannot write Cargo.toml")?;
    std::fs::write(&changelog_path, new_changelog).context("cannot write CHANGELOG.md")?;
    // Refresh the workspace members' versions in Cargo.lock.
    cargo(root, &["update", "--workspace"])?;

    git(root, &["add", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"])?;
    git(root, &["commit", "-m", &format!("chore(release): {tag}")])?;
    git(root, &["tag", "-a", &tag, "-m", &format!("GlyphPad {tag}")])?;

    println!("\nCreated commit and tag {tag}. Nothing was pushed. To publish:");
    println!("  git push origin main");
    println!("  git push origin {tag}");
    Ok(())
}

fn cargo(root: &Path, args: &[&str]) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .current_dir(root)
        .status()
        .context("cannot run cargo")?;
    if !status.success() {
        bail!("`cargo {}` failed", args.join(" "));
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .context("cannot run git")?;
    if !status.success() {
        bail!("`git {}` failed", args.join(" "));
    }
    Ok(())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn tag_must_match_version() {
        assert!(check_tag("v0.0.1-alpha.1", &v("0.0.1-alpha.1")).is_ok());
        assert!(check_tag("0.0.1-alpha.1", &v("0.0.1-alpha.1")).is_err());
        assert!(check_tag("v0.0.2", &v("0.0.1")).is_err());
    }

    #[test]
    fn target_version_rules() {
        assert!(validate_target(&v("0.1.0"), &v("0.2.0"), false).is_ok());
        assert!(validate_target(&v("0.1.0"), &v("0.1.0"), false).is_ok());
        assert!(validate_target(&v("0.1.0"), &v("0.1.0"), true).is_err());
        assert!(validate_target(&v("0.2.0"), &v("0.1.0"), false).is_err());
        assert!(validate_target(&v("0.1.0"), &v("0.1.0-rc.1"), false).is_err());
    }
}
