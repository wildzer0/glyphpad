//! End to end: a release in a local folder, downloaded, verified,
//! installed, started and, when the new version is broken, rolled back
//! (SPEC §12.4). The "new version" is the `fake_app` example.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use glyphpad_updater::install::{self, Handoff};
use glyphpad_updater::source::{Listing, LocalSource, UpdateSource};
use glyphpad_updater::{Channel, Error, Version, exe_name, newest, package, package_name, target};
use sha2::{Digest, Sha256};

fn fake_app() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap().join("examples");
    let name = if cfg!(windows) {
        "fake_app.exe"
    } else {
        "fake_app"
    };
    let path = dir.join(name);
    assert!(path.exists(), "{} is not built", path.display());
    path
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A release folder with one package holding `exe_bytes`, and the SHA256SUMS
/// of the package, or of something else when `tamper` is set.
fn publish(dir: &Path, version: &str, exe_bytes: &[u8], checksums: bool, tamper: bool) {
    let v = Version::parse(version).unwrap();
    let name = package_name(&v, target());
    let archive = dir.join(&name);
    if name.ends_with(".zip") {
        use std::io::Write;
        let file = std::fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(exe_name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(exe_bytes).unwrap();
        zip.finish().unwrap();
    } else {
        let file = std::fs::File::create(&archive).unwrap();
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(exe_bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, exe_name(), exe_bytes).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
    }
    let mut assets = vec![format!("\"{name}\"")];
    if checksums {
        let mut hash = hex(&Sha256::digest(std::fs::read(&archive).unwrap()));
        if tamper {
            hash = "0".repeat(64);
        }
        std::fs::write(dir.join("SHA256SUMS"), format!("{hash}  {name}\n")).unwrap();
        assets.push("\"SHA256SUMS\"".into());
    }
    std::fs::write(
        dir.join("releases.json"),
        format!(
            r#"[{{"tag":"v{version}","prerelease":false,"notes":"Fixes things.","assets":[{}]}}]"#,
            assets.join(",")
        ),
    )
    .unwrap();
}

struct World {
    _tmp: tempfile::TempDir,
    releases: PathBuf,
    app_dir: PathBuf,
    current: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let releases = tmp.path().join("releases");
    let app_dir = tmp.path().join("app");
    std::fs::create_dir_all(&releases).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    let current = app_dir.join(exe_name());
    std::fs::write(&current, b"MZ old version").unwrap();
    World {
        _tmp: tmp,
        releases,
        app_dir,
        current,
    }
}

/// Lists the folder, picks the update and downloads it to staging.
fn fetch(w: &World) -> Result<PathBuf, Error> {
    let source = LocalSource {
        dir: w.releases.clone(),
    };
    let Listing::Releases { releases, .. } = source.list(None)? else {
        panic!("a local source always lists")
    };
    let current = Version::parse("0.1.0").unwrap();
    let release = newest(&releases, &current, Channel::Stable)
        .expect("an update")
        .clone();
    let staging = install::prepare_staging(&w.current)?;
    package::download(&source, &release, target(), &staging)
}

fn start_args() -> Vec<OsString> {
    vec!["--post-update".into()]
}

fn skip() -> bool {
    target() == "unsupported"
}

#[test]
fn a_good_update_replaces_the_executable_and_the_new_one_reports() {
    if skip() {
        return;
    }
    let w = world();
    publish(
        &w.releases,
        "0.2.0",
        &std::fs::read(fake_app()).unwrap(),
        true,
        false,
    );
    std::fs::write(w.app_dir.join("behavior.txt"), "ok").unwrap();
    let staged = fetch(&w).unwrap();
    let installed = install::install(&staged, &w.current).unwrap();
    assert_eq!(std::fs::read(&installed.old).unwrap(), b"MZ old version");
    let mut released = false;
    let result = install::handoff(
        &installed,
        &start_args(),
        &install::marker_path(&w.current),
        Duration::from_secs(10),
        || released = true,
        || panic!("nothing to reclaim after a success"),
    );
    assert!(released);
    assert!(matches!(result, Handoff::Succeeded));
    // The old executable stays until the next start cleans it up.
    assert!(installed.old.exists());
}

#[test]
fn a_new_version_that_crashes_is_rolled_back() {
    if skip() {
        return;
    }
    let w = world();
    publish(
        &w.releases,
        "0.2.0",
        &std::fs::read(fake_app()).unwrap(),
        true,
        false,
    );
    std::fs::write(w.app_dir.join("behavior.txt"), "crash").unwrap();
    let staged = fetch(&w).unwrap();
    let installed = install::install(&staged, &w.current).unwrap();
    let mut reclaimed = false;
    let result = install::handoff(
        &installed,
        &start_args(),
        &install::marker_path(&w.current),
        Duration::from_secs(10),
        || {},
        || reclaimed = true,
    );
    let Handoff::RolledBack(why) = result else {
        panic!("expected a rollback")
    };
    assert!(why.contains("exited"), "{why}");
    assert!(reclaimed, "the lock is taken back");
    assert_eq!(std::fs::read(&w.current).unwrap(), b"MZ old version");
}

#[test]
fn a_new_version_that_never_reports_is_killed_and_rolled_back() {
    if skip() {
        return;
    }
    let w = world();
    publish(
        &w.releases,
        "0.2.0",
        &std::fs::read(fake_app()).unwrap(),
        true,
        false,
    );
    std::fs::write(w.app_dir.join("behavior.txt"), "hang").unwrap();
    let staged = fetch(&w).unwrap();
    let installed = install::install(&staged, &w.current).unwrap();
    let result = install::handoff(
        &installed,
        &start_args(),
        &install::marker_path(&w.current),
        Duration::from_secs(2),
        || {},
        || {},
    );
    let Handoff::RolledBack(why) = result else {
        panic!("expected a rollback")
    };
    assert!(why.contains("did not report"), "{why}");
    assert_eq!(std::fs::read(&w.current).unwrap(), b"MZ old version");
}

#[test]
fn a_package_with_a_wrong_checksum_is_rejected_and_nothing_changes() {
    if skip() {
        return;
    }
    let w = world();
    publish(
        &w.releases,
        "0.2.0",
        &std::fs::read(fake_app()).unwrap(),
        true,
        true,
    );
    let err = fetch(&w).unwrap_err();
    assert!(matches!(err, Error::ChecksumMismatch { .. }), "{err}");
    assert_eq!(std::fs::read(&w.current).unwrap(), b"MZ old version");
    assert!(
        std::fs::read_dir(install::staging_dir(&w.current))
            .unwrap()
            .next()
            .is_none(),
        "the rejected download is deleted"
    );
}

#[test]
fn a_release_without_checksums_is_refused() {
    if skip() {
        return;
    }
    let w = world();
    publish(
        &w.releases,
        "0.2.0",
        &std::fs::read(fake_app()).unwrap(),
        false,
        false,
    );
    assert!(matches!(fetch(&w), Err(Error::MissingChecksums(_))));
}

#[test]
fn a_package_that_holds_something_else_than_an_executable_is_refused() {
    if skip() {
        return;
    }
    let w = world();
    publish(&w.releases, "0.2.0", b"just some text", true, false);
    assert!(matches!(fetch(&w), Err(Error::Package(_))));
}
