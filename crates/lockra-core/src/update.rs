//! The in-app update's bookkeeping, after Voltip's design: what a run does, what it leaves for the
//! next one, when the automatic update runs, how often download progress is reported, and which
//! code a failed step shows. The steps themselves are `Core::update_check` and
//! `Core::update_install` (session.rs); the network and the installer are the [`Updater`] port.
//!
//! **By hand.** A check asks the release manifest and shows what it found. Installing goes on from
//! there: the release a check found is not asked for again, and a package already downloaded is not
//! downloaded again; then the package is installed and Lockra restarts.
//!
//! **Automatically** (`Settings::auto_update`, off by default). [`STARTUP_CHECK_DELAY`] after start
//! the update asks the manifest and downloads a newer release in the background, to
//! [`UpdateStatus::Ready`]: nothing is installed behind the user's back, the title bar offers the
//! restart. The version that reached `Ready` is remembered in [`MARKER_FILE`]; when the user quits
//! instead, the next start finds the same version again and installs it at once. A version newer
//! than the remembered one goes through `Ready` again, so a release the user has not been shown is
//! never installed unattended. Turning the switch on checks and downloads at once, without
//! installing.
//!
//! [`Updater`]: crate::ports::Updater

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::error::ErrorCode;
use crate::ports::{Release, UpdateFailure};
use crate::ui::UpdateStatus;

/// How long after start the automatic update runs (the window and the vault settle first).
pub const STARTUP_CHECK_DELAY: Duration = Duration::from_secs(10);
/// The fewest bytes between two download progress reports: each one is a whole state for the
/// webview.
pub const PROGRESS_MIN_STEP: u64 = 256 * 1024;
/// The file in the data directory that remembers the version the automatic update downloaded.
pub const MARKER_FILE: &str = "update-ready.json";

/// What a run does once it knows about a newer release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateRun {
    /// Ask the manifest only, afresh (「检查更新」).
    Check,
    /// Download what is not downloaded yet, install it and restart (「立即更新」, 「重启并更新」).
    Install,
    /// The automatic update: download to `Ready`; install too when the release is the version
    /// remembered as `Ready` by an earlier start.
    Auto {
        /// The version [`MARKER_FILE`] held at start; `None` after the switch was turned on.
        install_version: Option<String>,
    },
}

impl UpdateRun {
    /// The package is downloaded once a newer release is known.
    pub fn downloads(&self) -> bool {
        !matches!(self, Self::Check)
    }

    /// `version` is installed once it is downloaded.
    pub fn installs(&self, version: &str) -> bool {
        match self {
            Self::Check => false,
            Self::Install => true,
            Self::Auto { install_version } => install_version.as_deref() == Some(version),
        }
    }
}

/// A newer release an earlier run found, and whether its package is downloaded (the [`Updater`]
/// port keeps the package).
///
/// [`Updater`]: crate::ports::Updater
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub release: Release,
    pub downloaded: bool,
}

/// The update part of the core's state.
#[derive(Debug)]
pub(crate) struct UpdateState {
    /// What the title bar and Settings show.
    pub status: UpdateStatus,
    /// A run is in flight.
    pub busy: bool,
    /// When the automatic update runs: [`STARTUP_CHECK_DELAY`] after start, once.
    pub auto_at: Option<Instant>,
    /// What the last run found and left for the next one.
    pub pending: Option<Pending>,
}

impl UpdateState {
    /// The state at start; `automatic` schedules the automatic update.
    pub fn new(automatic: bool) -> Self {
        Self { status: UpdateStatus::Idle, busy: false, auto_at: automatic.then(|| Instant::now() + STARTUP_CHECK_DELAY), pending: None }
    }
}

/// What [`MARKER_FILE`] holds.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Marker {
    version: String,
}

/// The version remembered in `data_dir`, if the marker is there and reads.
pub(crate) fn read_marker(data_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(data_dir.join(MARKER_FILE)).ok()?;
    serde_json::from_slice::<Marker>(&bytes).ok().map(|marker| marker.version)
}

/// Remember `version` as downloaded. A failure is logged: the marker only decides whether the next
/// start installs at once.
pub(crate) fn write_marker(data_dir: &Path, version: &str) {
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(data_dir.join(MARKER_FILE), serde_json::to_vec(&Marker { version: version.to_owned() })?)
    };
    if let Err(error) = write() {
        tracing::warn!(%error, "the update marker was not written");
    }
}

/// Forget the remembered version (nothing newer, or installed).
pub(crate) fn clear_marker(data_dir: &Path) {
    if let Err(error) = std::fs::remove_file(data_dir.join(MARKER_FILE))
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "the update marker was not removed");
    }
}

/// The code a failed step shows.
pub(crate) fn failure_code(failure: UpdateFailure) -> ErrorCode {
    match failure {
        UpdateFailure::Network => ErrorCode::UpdateNetwork,
        UpdateFailure::Invalid => ErrorCode::UpdateInvalid,
        UpdateFailure::Signature => ErrorCode::UpdateSignature,
        UpdateFailure::Install => ErrorCode::UpdateInstallFailed,
        UpdateFailure::Cancelled => ErrorCode::UpdateCancelled,
        UpdateFailure::Keychain => ErrorCode::UpdateKeychain,
    }
}

/// Which download progress is worth a state: every hundredth of the size, at least
/// [`PROGRESS_MIN_STEP`] apart, and the end.
#[derive(Debug, Default)]
pub(crate) struct ProgressGate {
    last: u64,
}

impl ProgressGate {
    /// `true` when `received` deserves a state.
    pub fn step(&mut self, received: u64, total: Option<u64>) -> bool {
        let step = total.map_or(PROGRESS_MIN_STEP, |t| (t / 100).max(PROGRESS_MIN_STEP));
        let due = received.saturating_sub(self.last) >= step || total.is_some_and(|t| received >= t);
        if due {
            self.last = received;
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_map_to_their_codes() {
        let cases = [
            (UpdateFailure::Network, ErrorCode::UpdateNetwork),
            (UpdateFailure::Invalid, ErrorCode::UpdateInvalid),
            (UpdateFailure::Signature, ErrorCode::UpdateSignature),
            (UpdateFailure::Install, ErrorCode::UpdateInstallFailed),
            (UpdateFailure::Cancelled, ErrorCode::UpdateCancelled),
            (UpdateFailure::Keychain, ErrorCode::UpdateKeychain),
        ];
        for (failure, code) in cases {
            assert_eq!(failure_code(failure), code);
        }
    }

    #[test]
    fn progress_is_reported_every_hundredth_and_at_the_end() {
        let total = 100 * PROGRESS_MIN_STEP * 2;
        let mut gate = ProgressGate::default();
        assert!(!gate.step(1, Some(total)));
        assert!(!gate.step(total / 100 - 1, Some(total)));
        assert!(gate.step(total / 100, Some(total)));
        assert!(!gate.step(total / 100 + 1, Some(total)));
        assert!(gate.step(total, Some(total)), "the end always counts");
        // A small package still steps by PROGRESS_MIN_STEP, and an unknown size by that step alone.
        let mut small = ProgressGate::default();
        assert!(!small.step(PROGRESS_MIN_STEP - 1, Some(10 * PROGRESS_MIN_STEP)));
        assert!(small.step(PROGRESS_MIN_STEP, Some(10 * PROGRESS_MIN_STEP)));
        let mut unknown = ProgressGate::default();
        assert!(!unknown.step(PROGRESS_MIN_STEP - 1, None));
        assert!(unknown.step(PROGRESS_MIN_STEP, None));
    }

    #[tokio::test(start_paused = true)]
    async fn the_automatic_update_waits_for_the_start() {
        let off = UpdateState::new(false);
        assert_eq!((off.status, off.busy, off.auto_at), (UpdateStatus::Idle, false, None));
        assert!(off.pending.is_none());
        let on = UpdateState::new(true);
        assert_eq!(on.auto_at, Some(Instant::now() + STARTUP_CHECK_DELAY));
    }

    #[test]
    fn runs_download_and_install_as_documented() {
        assert!(!UpdateRun::Check.downloads());
        assert!(!UpdateRun::Check.installs("0.3.0"));
        assert!(UpdateRun::Install.downloads());
        assert!(UpdateRun::Install.installs("0.3.0"));
        let fresh = UpdateRun::Auto { install_version: None };
        assert!(fresh.downloads());
        assert!(!fresh.installs("0.3.0"), "a version never shown as ready is only downloaded");
        let remembered = UpdateRun::Auto { install_version: Some("0.3.0".into()) };
        assert!(remembered.installs("0.3.0"), "the remembered version installs at the next start");
        assert!(!remembered.installs("0.3.1"), "a newer one goes through ready again");
    }

    #[test]
    fn the_marker_round_trips_and_a_damaged_one_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b");
        assert_eq!(read_marker(&nested), None);
        clear_marker(&nested);
        write_marker(&nested, "0.3.0");
        assert_eq!(read_marker(&nested), Some("0.3.0".into()));
        assert_eq!(std::fs::read_to_string(nested.join(MARKER_FILE)).unwrap(), r#"{"version":"0.3.0"}"#);
        clear_marker(&nested);
        assert_eq!(read_marker(&nested), None);
        std::fs::write(nested.join(MARKER_FILE), b"{").unwrap();
        assert_eq!(read_marker(&nested), None);
        // A data directory that cannot hold it: logged, nothing else.
        let file = dir.path().join("file");
        std::fs::write(&file, b"").unwrap();
        write_marker(&file, "0.3.0");
        assert_eq!(read_marker(&file), None);
    }
}
