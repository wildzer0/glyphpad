//! Where releases come from: GitHub in production, a folder in tests
//! (SPEC §12.4).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::{Asset, Error, GITHUB_REPO, Release, parse_tag};

/// Largest package or checksum file accepted.
pub const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

pub enum Listing {
    /// The server says nothing changed since the ETag we sent.
    NotModified,
    Releases {
        releases: Vec<Release>,
        etag: Option<String>,
    },
}

pub trait UpdateSource: Send + Sync {
    /// Lists releases, newest first or not: the caller picks.
    fn list(&self, etag: Option<&str>) -> Result<Listing, Error>;
    /// Writes an asset's bytes to `out`.
    fn fetch(&self, asset: &Asset, out: &mut dyn Write) -> Result<(), Error>;
}

// ------------------------------------------------------------- GitHub

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The prefix every asset URL of this project's releases starts with.
fn trusted_prefix() -> String {
    format!(
        "https://github.com/{}/{}/releases/download/",
        GITHUB_REPO.0, GITHUB_REPO.1
    )
}

/// Parses GitHub's list of releases. Drafts, unparseable tags and assets
/// that do not live under this project's releases are dropped.
pub fn parse_github(json: &str) -> Result<Vec<Release>, Error> {
    let api: Vec<ApiRelease> =
        serde_json::from_str(json).map_err(|e| Error::Parse(e.to_string()))?;
    let prefix = trusted_prefix();
    Ok(api
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = parse_tag(&r.tag_name)?;
            Some(Release {
                version,
                tag: r.tag_name,
                prerelease: r.prerelease,
                notes: r.body.unwrap_or_default(),
                page_url: r.html_url,
                assets: r
                    .assets
                    .into_iter()
                    .filter(|a| a.browser_download_url.starts_with(&prefix))
                    .map(|a| Asset {
                        name: a.name,
                        url: a.browser_download_url,
                        size: a.size,
                    })
                    .collect(),
            })
        })
        .collect())
}

pub struct GithubSource {
    agent: ureq::Agent,
}

impl GithubSource {
    /// `user_agent` is `glyphpad/<version>` (SPEC §12.3).
    pub fn new(user_agent: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .timeout_global(Some(Duration::from_secs(120)))
            .timeout_connect(Some(Duration::from_secs(15)))
            // 304 and 403/429 are answers to read, not failures.
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

fn retry_after(resp: &ureq::http::Response<ureq::Body>) -> Duration {
    let header = |name: &str| -> Option<u64> {
        resp.headers().get(name)?.to_str().ok()?.trim().parse().ok()
    };
    if let Some(s) = header("retry-after") {
        return Duration::from_secs(s.max(60));
    }
    if let Some(reset) = header("x-ratelimit-reset") {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        return Duration::from_secs(reset.saturating_sub(now).max(60));
    }
    Duration::from_secs(3600)
}

impl UpdateSource for GithubSource {
    fn list(&self, etag: Option<&str>) -> Result<Listing, Error> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases?per_page=30",
            GITHUB_REPO.0, GITHUB_REPO.1
        );
        let mut req = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(etag) = etag {
            req = req.header("If-None-Match", etag);
        }
        let mut resp = req.call().map_err(|e| Error::Network(e.to_string()))?;
        match resp.status().as_u16() {
            304 => Ok(Listing::NotModified),
            200 => {
                let etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let body = resp
                    .body_mut()
                    .with_config()
                    .limit(8 * 1024 * 1024)
                    .read_to_string()
                    .map_err(|e| Error::Network(e.to_string()))?;
                Ok(Listing::Releases {
                    releases: parse_github(&body)?,
                    etag,
                })
            }
            403 | 429 => Err(Error::RateLimited {
                retry_after: retry_after(&resp),
            }),
            code => Err(Error::Http(code)),
        }
    }

    fn fetch(&self, asset: &Asset, out: &mut dyn Write) -> Result<(), Error> {
        // Assets are only ever fetched from this project's releases.
        if !asset.url.starts_with(&trusted_prefix()) {
            return Err(Error::UntrustedUrl(asset.url.clone()));
        }
        let mut resp = self
            .agent
            .get(&asset.url)
            .call()
            .map_err(|e| Error::Network(e.to_string()))?;
        if resp.status().as_u16() != 200 {
            return Err(Error::Http(resp.status().as_u16()));
        }
        let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
        copy_limited(&mut reader, out)
    }
}

fn copy_limited(reader: &mut dyn Read, out: &mut dyn Write) -> Result<(), Error> {
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| Error::Network(e.to_string()))?;
        if n == 0 {
            return Ok(());
        }
        total += n as u64;
        if total > MAX_DOWNLOAD {
            return Err(Error::Package(
                "the download is larger than expected".into(),
            ));
        }
        out.write_all(&buf[..n])?;
    }
}

// -------------------------------------------------------------- local

#[derive(Deserialize)]
struct LocalRelease {
    tag: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    notes: String,
    assets: Vec<String>,
}

/// A folder with a `releases.json` and the assets next to it, for tests and
/// for `--update-source` in debug builds.
///
/// ```json
/// [{ "tag": "v0.2.0", "prerelease": false, "notes": "…",
///    "assets": ["glyphpad-0.2.0-x86_64-pc-windows-msvc.zip", "SHA256SUMS"] }]
/// ```
pub struct LocalSource {
    pub dir: PathBuf,
}

impl UpdateSource for LocalSource {
    fn list(&self, _etag: Option<&str>) -> Result<Listing, Error> {
        let text = std::fs::read_to_string(self.dir.join("releases.json"))?;
        let local: Vec<LocalRelease> =
            serde_json::from_str(&text).map_err(|e| Error::Parse(e.to_string()))?;
        let releases = local
            .into_iter()
            .filter_map(|r| {
                let version = parse_tag(&r.tag)?;
                let assets = r
                    .assets
                    .into_iter()
                    .map(|name| Asset {
                        size: std::fs::metadata(self.dir.join(&name)).map_or(0, |m| m.len()),
                        url: name.clone(),
                        name,
                    })
                    .collect();
                Some(Release {
                    version,
                    page_url: format!("file://{}", self.dir.display()),
                    tag: r.tag,
                    prerelease: r.prerelease,
                    notes: r.notes,
                    assets,
                })
            })
            .collect();
        Ok(Listing::Releases {
            releases,
            etag: None,
        })
    }

    fn fetch(&self, asset: &Asset, out: &mut dyn Write) -> Result<(), Error> {
        // A plain file name: no way out of the folder. `Path::join` would
        // replace the folder for a rooted path or a drive-relative `C:name`.
        if asset.url.contains(['/', '\\', ':']) || asset.url.contains("..") {
            return Err(Error::UntrustedUrl(asset.url.clone()));
        }
        let mut file = std::fs::File::open(self.dir.join(&asset.url))?;
        copy_limited(&mut file, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"tag_name":"v0.2.0","draft":false,"prerelease":false,"body":"Notes","html_url":"https://github.com/wildzer0/glyphpad/releases/tag/v0.2.0",
       "assets":[
         {"name":"glyphpad-0.2.0-x86_64-pc-windows-msvc.zip","browser_download_url":"https://github.com/wildzer0/glyphpad/releases/download/v0.2.0/glyphpad-0.2.0-x86_64-pc-windows-msvc.zip","size":10},
         {"name":"evil.zip","browser_download_url":"https://example.com/evil.zip","size":1}]},
      {"tag_name":"v0.3.0","draft":true,"prerelease":false,"html_url":"https://github.com/x","assets":[]},
      {"tag_name":"not-a-version","draft":false,"prerelease":false,"html_url":"https://github.com/x","assets":[]}
    ]"#;

    #[test]
    fn github_listing_keeps_only_trusted_assets_of_real_releases() {
        let r = parse_github(SAMPLE).unwrap();
        assert_eq!(r.len(), 1, "drafts and non-versions are dropped");
        assert_eq!(r[0].assets.len(), 1, "foreign URLs are dropped");
        assert_eq!(r[0].notes, "Notes");
    }

    #[test]
    fn github_source_refuses_foreign_urls() {
        let source = GithubSource::new("glyphpad/test");
        let asset = Asset {
            name: "x".into(),
            url: "https://example.com/x".into(),
            size: 0,
        };
        let err = source.fetch(&asset, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, Error::UntrustedUrl(_)));
    }

    #[test]
    fn local_source_cannot_leave_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = LocalSource {
            dir: dir.path().to_path_buf(),
        };
        let asset = Asset {
            name: "x".into(),
            url: "../secret".into(),
            size: 0,
        };
        assert!(matches!(
            source.fetch(&asset, &mut Vec::new()),
            Err(Error::UntrustedUrl(_))
        ));
    }
}
