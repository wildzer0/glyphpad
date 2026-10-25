//! The updater thread: checks for releases now and then, tells the window
//! what it found, and installs and restarts on request or by itself in
//! `auto` mode (SPEC §12).
//!
//! The thread never touches the engine's state beyond pausing gestures
//! while the new version takes over. Releases are verified against
//! `SHA256SUMS` only: they are not signed yet.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use glyphpad_core::config::{UpdateChannel, UpdateMode};
use glyphpad_core::service::{AvailableUpdate, Command, Shared, UiLink, UpdateStatus};
use glyphpad_updater::install::{self, Handoff};
use glyphpad_updater::source::{GithubSource, Listing, LocalSource, UpdateSource};
use glyphpad_updater::state::State;
use glyphpad_updater::{Channel, Error, Release, Version, newest, package, target};

/// What the rest of the program asks the updater to do.
#[derive(Debug, Clone)]
pub enum Request {
    Check { manual: bool },
    Install,
    Ignore(String),
}

/// Waiting this long after start before the first check keeps start-up fast.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(8);
/// After a failed automatic check, try again in an hour rather than at once.
const RETRY_AFTER_ERROR: Duration = Duration::from_secs(3600);
/// The engine must be idle this long before an automatic installation.
const IDLE_POLL: Duration = Duration::from_secs(5);

pub struct Settings {
    pub shared: Arc<Mutex<Shared>>,
    pub ui: Arc<UiLink>,
    /// To pause gestures while the new version takes over.
    pub commands: Sender<Command>,
    pub state_path: PathBuf,
    pub source: Box<dyn UpdateSource>,
    pub version: Version,
    /// Gives up the single-instance lock before the new process starts.
    pub release_lock: Box<dyn Fn() + Send>,
    /// Takes it back after a rollback.
    pub reclaim_lock: Box<dyn Fn() + Send>,
}

/// The source releases come from: GitHub, or a folder in debug builds.
pub fn make_source(version: &str, local: Option<PathBuf>) -> Box<dyn UpdateSource> {
    match local {
        Some(dir) => Box::new(LocalSource { dir }),
        None => Box::new(GithubSource::new(&format!("glyphpad/{version}"))),
    }
}

pub fn channel(c: UpdateChannel) -> Channel {
    match c {
        UpdateChannel::Stable => Channel::Stable,
        UpdateChannel::Prerelease => Channel::Prerelease,
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn local_time(secs: u64) -> Option<String> {
    let ts = jiff::Timestamp::from_second(i64::try_from(secs).ok()?).ok()?;
    Some(
        ts.to_zoned(jiff::tz::TimeZone::system())
            .strftime("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

fn available(r: &Release) -> AvailableUpdate {
    AvailableUpdate {
        version: r.version.to_string(),
        notes: r.notes.clone(),
        url: r.page_url.clone(),
        prerelease: r.prerelease,
    }
}

/// Lists releases and picks the newest allowed one, remembering the answer
/// in `state`. Shared by the thread and the `update` subcommand.
pub fn find(
    source: &dyn UpdateSource,
    state: &mut State,
    cache: &mut Vec<Release>,
    current: &Version,
    channel: Channel,
) -> Result<Option<Release>, Error> {
    // Without the previous answer in memory an ETag would give us nothing.
    let etag = if cache.is_empty() {
        None
    } else {
        state.etag.as_deref()
    };
    match source.list(etag)? {
        Listing::NotModified => {}
        Listing::Releases { releases, etag } => {
            *cache = releases;
            state.etag = etag;
        }
    }
    state.last_check = Some(now_secs());
    Ok(newest(cache, current, channel).cloned())
}

/// Downloads and installs `release`, returning what to undo. The folder of
/// the executable must be writable.
pub fn download_and_install(
    source: &dyn UpdateSource,
    release: &Release,
) -> Result<install::Installed, Error> {
    let current = install::current_exe()?;
    let staging = install::prepare_staging(&current)?;
    let result = package::download(source, release, target(), &staging)
        .and_then(|staged| install::install(&staged, &current));
    let _ = std::fs::remove_dir_all(&staging);
    result
}

struct Updater {
    s: Settings,
    state: State,
    cache: Vec<Release>,
    /// The release offered now.
    offered: Option<Release>,
    /// After a rate limit, no request before this.
    blocked_until: Option<Instant>,
    args: Vec<OsString>,
}

pub fn spawn(rx: Receiver<Request>, settings: Settings) {
    let state = State::load(&settings.state_path);
    let updater = Updater {
        s: settings,
        state,
        cache: Vec::new(),
        offered: None,
        blocked_until: None,
        args: std::env::args_os().skip(1).collect(),
    };
    updater.publish_last_check();
    let spawned = std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || updater.run(rx));
    if let Err(e) = spawned {
        tracing::warn!("cannot start the updater: {e}");
    }
}

impl Updater {
    fn set(&self, status: UpdateStatus) {
        if let Ok(mut s) = self.s.shared.lock() {
            s.update = status;
        }
        self.s.ui.wake();
    }

    fn publish_last_check(&self) {
        let text = self.state.last_check.and_then(local_time);
        if let Ok(mut s) = self.s.shared.lock() {
            s.last_update_check = text;
        }
    }

    fn config(&self) -> (UpdateMode, UpdateChannel, u32) {
        self.s
            .shared
            .lock()
            .map(|s| {
                let u = &s.config.update;
                (u.mode, u.channel, u.check_interval_h)
            })
            .unwrap_or((UpdateMode::Off, UpdateChannel::Stable, 24))
    }

    fn save_state(&self) {
        if let Err(e) = self.state.save(&self.s.state_path) {
            tracing::warn!("cannot save the update state: {e}");
        }
    }

    fn run(mut self, rx: Receiver<Request>) {
        let mut next_auto = Instant::now() + FIRST_CHECK_DELAY;
        loop {
            match rx.recv_timeout(next_auto.saturating_duration_since(Instant::now())) {
                Ok(Request::Check { manual }) => {
                    self.check(manual, false);
                }
                Ok(Request::Install) => self.install(),
                Ok(Request::Ignore(version)) => {
                    self.state.ignored = Some(version);
                    self.save_state();
                    self.offered = None;
                    self.set(UpdateStatus::Idle);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    let (mode, _, interval) = self.config();
                    next_auto = Instant::now() + Duration::from_secs(60 * 60);
                    if mode != UpdateMode::Off {
                        if self.state.due(now_secs(), interval) {
                            if !self.check(false, mode == UpdateMode::Auto) {
                                next_auto = Instant::now() + RETRY_AFTER_ERROR;
                            }
                        } else {
                            // Not due yet: look again when it will be.
                            let last = self.state.last_check.unwrap_or(0);
                            let due_in =
                                (last + u64::from(interval) * 3600).saturating_sub(now_secs());
                            next_auto = Instant::now() + Duration::from_secs(due_in.max(60));
                        }
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Asks the source. A manual check reports "up to date" and errors; an
    /// automatic one stays quiet unless there is something to offer.
    fn check(&mut self, manual: bool, install_if_found: bool) -> bool {
        if let Some(until) = self.blocked_until
            && Instant::now() < until
            && !manual
        {
            return false;
        }
        let (_, channel_cfg, _) = self.config();
        if manual {
            self.set(UpdateStatus::Checking);
        }
        let found = find(
            self.s.source.as_ref(),
            &mut self.state,
            &mut self.cache,
            &self.s.version,
            channel(channel_cfg),
        );
        self.publish_last_check();
        match found {
            Ok(release) => {
                self.save_state();
                self.blocked_until = None;
                let ignored = release
                    .as_ref()
                    .is_some_and(|r| self.state.ignored.as_deref() == Some(&r.version.to_string()));
                match release {
                    Some(r) if manual || !ignored => {
                        tracing::info!("update available: {}", r.version);
                        let info = available(&r);
                        self.offered = Some(r);
                        self.set(UpdateStatus::Available(info));
                        if install_if_found {
                            self.wait_until_idle();
                            self.install();
                        }
                    }
                    _ => {
                        self.offered = None;
                        self.set(if manual {
                            UpdateStatus::UpToDate
                        } else {
                            UpdateStatus::Idle
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!("update check failed: {e}");
                if let Error::RateLimited { retry_after } = &e {
                    self.blocked_until = Some(Instant::now() + *retry_after);
                }
                // An automatic check that fails (offline, rate limit) is only
                // logged; the user sees errors of the checks they asked for.
                if manual {
                    self.set(UpdateStatus::Failed(e.to_string()));
                }
                return false;
            }
        }
        true
    }

    /// Never update in the middle of a gesture, a drawing or a recording
    /// (SPEC §12.2).
    fn wait_until_idle(&self) {
        loop {
            let busy = self
                .s
                .shared
                .lock()
                .map(|s| {
                    s.contacts.iter().any(|c| c.tip)
                        || !s.drawing.is_empty()
                        || s.mode.is_recording()
                })
                .unwrap_or(false);
            if !busy {
                return;
            }
            std::thread::sleep(IDLE_POLL);
        }
    }

    fn install(&mut self) {
        let Some(release) = self.offered.clone() else {
            return;
        };
        let version = release.version.to_string();
        let info = available(&release);
        self.set(UpdateStatus::Downloading(version.clone()));
        let installed = match download_and_install(self.s.source.as_ref(), &release) {
            Ok(i) => i,
            Err(Error::NotWritable(why)) => {
                tracing::warn!("cannot update in place: {why}");
                self.set(UpdateStatus::Manual(info, why));
                return;
            }
            Err(e) => {
                tracing::warn!("update failed: {e}");
                self.set(UpdateStatus::Failed(e.to_string()));
                return;
            }
        };
        self.set(UpdateStatus::Installing(version.clone()));
        // Both processes would act on the same touches for a moment.
        let was_enabled = self.s.shared.lock().map(|s| s.enabled).unwrap_or(true);
        if was_enabled {
            let _ = self.s.commands.send(Command::ToggleEnabled);
        }
        let mut args = self.args.clone();
        if !args.iter().any(|a| a == "--post-update") {
            args.push("--post-update".into());
        }
        let marker = install::marker_path(&installed.current);
        let release_lock = &self.s.release_lock;
        let reclaim_lock = &self.s.reclaim_lock;
        match install::handoff(
            &installed,
            &args,
            &marker,
            install::HANDOFF_TIMEOUT,
            release_lock,
            reclaim_lock,
        ) {
            Handoff::Succeeded => {
                tracing::info!("the new version {version} is running; closing");
                self.set(UpdateStatus::Restarting(version));
                self.s.ui.request_quit();
            }
            Handoff::RolledBack(why) => {
                tracing::error!("update to {version} rolled back: {why}");
                if was_enabled {
                    let _ = self.s.commands.send(Command::ToggleEnabled);
                }
                self.set(UpdateStatus::Failed(format!(
                    "Version {version} did not start, so the old one was restored: {why}"
                )));
            }
        }
    }
}
