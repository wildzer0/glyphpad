//! Downloading a release's package, checking it against `SHA256SUMS` and
//! extracting the executable (SPEC §12.2).
//!
//! `SHA256SUMS` protects against corrupt downloads, not against a
//! compromised GitHub account: `SHA256SUMS.minisig` is checked first when the
//! build has a minisign key (SPEC §12.3, [`crate::signature`]).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::signature;
use crate::source::{MAX_DOWNLOAD, UpdateSource};
use crate::{CHECKSUMS, CHECKSUMS_SIG, Error, Release, exe_name, package_name};

/// `<hash>  <name>` per line (also `<hash> *<name>`, as `sha256sum -b` writes).
pub fn parse_checksums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (name.to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// Writes to a file while hashing what passes.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Whether `head` starts like an executable of `target`'s system: catches an
/// archive that holds something else, or a corrupt extraction.
pub fn looks_like_executable(head: &[u8], target: &str) -> bool {
    if target.contains("windows") {
        head.starts_with(b"MZ")
    } else {
        head.starts_with(b"\x7fELF")
    }
}

fn extract_zip(archive: &Path, to: &Path) -> Result<(), Error> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Package(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::Package(e.to_string()))?;
        // Only the executable's file name matters; paths inside are ignored,
        // so an entry cannot write anywhere else.
        let full = entry.name().unwrap_or_default().to_string();
        let name = full.rsplit(['/', '\\']).next().unwrap_or_default();
        if entry.is_file() && name == exe_name() {
            let mut out = File::create(to)?;
            copy_capped(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(Error::Package(format!("the package has no {}", exe_name())))
}

fn extract_targz(archive: &Path, to: &Path) -> Result<(), Error> {
    let file = File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(|e| Error::Package(e.to_string()))? {
        let mut entry = entry.map_err(|e| Error::Package(e.to_string()))?;
        let is_exe = entry.header().entry_type().is_file()
            && entry
                .path()
                .ok()
                .and_then(|p| p.file_name().map(|n| n == exe_name()))
                .unwrap_or(false);
        if is_exe {
            let mut out = File::create(to)?;
            copy_capped(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(Error::Package(format!("the package has no {}", exe_name())))
}

fn copy_capped(from: &mut dyn Read, to: &mut dyn Write) -> Result<(), Error> {
    let copied = std::io::copy(&mut from.take(MAX_DOWNLOAD + 1), to)?;
    if copied > MAX_DOWNLOAD {
        return Err(Error::Package(
            "the executable is larger than expected".into(),
        ));
    }
    Ok(())
}

/// Downloads `release`'s package for `target` into `staging` (which must
/// exist and be next to the executable, on the same volume), verifies it and
/// returns the path of the extracted executable.
pub fn download(
    source: &dyn UpdateSource,
    release: &Release,
    target: &str,
    staging: &Path,
) -> Result<PathBuf, Error> {
    let name = package_name(&release.version, target);
    let package = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| Error::NoPackage(release.tag.clone(), target.to_string()))?;
    let sums_asset = release
        .assets
        .iter()
        .find(|a| a.name == CHECKSUMS)
        .ok_or_else(|| Error::MissingChecksums(release.tag.clone()))?;

    let mut sums = Vec::new();
    source.fetch(sums_asset, &mut sums)?;
    let sums_text = String::from_utf8_lossy(&sums).to_string();
    // The signature is checked before the package is even downloaded: a
    // release replaced through the GitHub account cannot hand out a binary
    // (SPEC §12.3).
    if signature::is_configured() {
        let sig_asset = release
            .assets
            .iter()
            .find(|a| a.name == CHECKSUMS_SIG)
            .ok_or_else(|| Error::MissingSignature(release.tag.clone()))?;
        let mut sig = Vec::new();
        source.fetch(sig_asset, &mut sig)?;
        signature::verify(&sums_text, &String::from_utf8_lossy(&sig))?;
    } else {
        tracing::warn!(
            "this build has no minisign key, so the release's signature cannot be checked"
        );
    }
    let sums = parse_checksums(&sums_text);
    let expected = sums
        .get(&name)
        .ok_or_else(|| Error::NotListed(name.clone()))?;

    let archive = staging.join(&name);
    let mut writer = HashingWriter {
        inner: File::create(&archive)?,
        hasher: Sha256::new(),
    };
    let fetched = source.fetch(package, &mut writer).and_then(|()| {
        writer.flush()?;
        Ok(())
    });
    let actual = hex(&writer.hasher.finalize());
    drop(writer.inner);
    if let Err(e) = fetched {
        let _ = std::fs::remove_file(&archive);
        return Err(e);
    }
    if actual != *expected {
        let _ = std::fs::remove_file(&archive);
        return Err(Error::ChecksumMismatch {
            expected: expected.clone(),
            actual,
        });
    }

    let staged = staging.join(format!("new-{}", exe_name()));
    let extracted = if name.ends_with(".zip") {
        extract_zip(&archive, &staged)
    } else {
        extract_targz(&archive, &staged)
    };
    let _ = std::fs::remove_file(&archive);
    extracted?;

    let mut head = [0u8; 4];
    let n = File::open(&staged)?.read(&mut head)?;
    if !looks_like_executable(&head[..n], target) {
        let _ = std::fs::remove_file(&staged);
        return Err(Error::Package(
            "the executable in the package is not valid".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(staged)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_lines_parse_in_both_formats() {
        let h = "a".repeat(64);
        let text = format!(
            "{h}  one.zip\n{h} *two.zip\nnot a line\n{}  short.zip\n",
            "b".repeat(10)
        );
        let m = parse_checksums(&text);
        assert_eq!(m.len(), 2);
        assert_eq!(m["one.zip"], h);
        assert!(m.contains_key("two.zip"));
    }

    #[test]
    fn executable_magic_depends_on_the_system() {
        assert!(looks_like_executable(
            b"MZ\x90\x00",
            "x86_64-pc-windows-msvc"
        ));
        assert!(!looks_like_executable(b"\x7fELF", "x86_64-pc-windows-msvc"));
        assert!(looks_like_executable(
            b"\x7fELF",
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!looks_like_executable(b"", "x86_64-unknown-linux-gnu"));
    }
}
