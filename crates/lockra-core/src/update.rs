//! The in-app update's bookkeeping: when the automatic check runs, how often download progress is
//! reported, and which code a failed step shows. The steps themselves are `Core::update_check` and
//! `Core::update_install` (session.rs); the network and the installer are the [`Updater`] port.
//!
//! [`Updater`]: crate::ports::Updater

use std::time::Duration;

use tokio::time::Instant;

use crate::error::ErrorCode;
use crate::ports::UpdateFailure;
use crate::ui::UpdateStatus;

/// How long after start the automatic check runs (the window and the vault settle first).
pub const STARTUP_CHECK_DELAY: Duration = Duration::from_secs(10);
/// Between two automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// After an automatic check that failed (offline at start, say).
pub const CHECK_RETRY: Duration = Duration::from_secs(60 * 60);
/// The fewest bytes between two download progress reports: each one is a whole state for the
/// webview.
pub const PROGRESS_MIN_STEP: u64 = 256 * 1024;

/// What a run does once it knows about a newer release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateRun {
    /// Ask only: the user's "check", or the automatic check.
    Check {
        /// Started by the timer, not by the user.
        automatic: bool,
    },
    /// Ask, download, install and restart: the user's "install".
    Install,
}

/// The update part of the core's state.
#[derive(Debug)]
pub(crate) struct UpdateState {
    /// What Settings › About shows.
    pub status: UpdateStatus,
    /// A run is in flight.
    pub busy: bool,
    /// When the automatic check runs next.
    pub next_check: Option<Instant>,
    /// The version the automatic check last announced: each one is announced once.
    pub announced: Option<String>,
}

impl UpdateState {
    /// The state at start; `automatic` schedules the first check.
    pub fn new(automatic: bool) -> Self {
        Self { status: UpdateStatus::Idle, busy: false, next_check: automatic.then(|| Instant::now() + STARTUP_CHECK_DELAY), announced: None }
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
    async fn the_first_automatic_check_waits_for_the_start() {
        let off = UpdateState::new(false);
        assert_eq!((off.status, off.busy, off.next_check, off.announced), (UpdateStatus::Idle, false, None, None));
        let on = UpdateState::new(true);
        assert_eq!(on.next_check, Some(Instant::now() + STARTUP_CHECK_DELAY));
    }
}
