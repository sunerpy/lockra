//! The in-app update on the fake update source: what the state shows at each step, which requests
//! are refused, and when the automatic check runs.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::*;
use crate::fakes::FakeUpdater;
use crate::ports::UpdateFailure;
use crate::ui::{InstallMethod, UpdateStatus};
use crate::{CHECK_INTERVAL, CHECK_RETRY, PROGRESS_MIN_STEP, STARTUP_CHECK_DELAY};

fn status(h: &Harness) -> UpdateStatus {
    h.core.state().update.status
}

/// The update statuses the state events carried since the last call, without repeats.
fn statuses(h: &mut Harness) -> Vec<UpdateStatus> {
    let mut out: Vec<UpdateStatus> = Vec::new();
    while let Ok(event) = h.events.try_recv() {
        if let UiEvent::State { state } = event
            && out.last() != Some(&state.update.status)
        {
            out.push(state.update.status);
        }
    }
    out
}

/// Let a background run reach its last status (the fake yields between progress steps, more often
/// than `settle` does).
async fn finished(h: &Harness) {
    for _ in 0..500 {
        tokio::task::yield_now().await;
        if matches!(status(h), UpdateStatus::UpToDate { .. } | UpdateStatus::Available { .. } | UpdateStatus::Installing { .. } | UpdateStatus::Failed { .. }) {
            return;
        }
    }
    panic!("the update run did not finish: {:?}", status(h));
}

fn automatic(on: bool) -> Settings {
    Settings { auto_check_updates: on, ..Settings::default() }
}

#[tokio::test(start_paused = true)]
async fn a_copy_that_cannot_update_itself_refuses_and_never_checks() {
    let h = harness();
    let state = h.core.state();
    assert_eq!((state.update.method, state.update.status), (None, UpdateStatus::Idle));
    assert_eq!(code_err(h.core.update_check()), ErrorCode::UpdateUnavailable);
    assert_eq!(code_err(h.core.update_install()), ErrorCode::UpdateUnavailable);
    h.core.set_settings(automatic(true)).unwrap();
    advance(STARTUP_CHECK_DELAY + CHECK_INTERVAL).await;
    assert!(h.updater.calls().is_empty());
    assert_eq!(status(&h), UpdateStatus::Idle);
}

#[tokio::test(start_paused = true)]
async fn a_check_finds_nothing_newer_or_the_newer_release() {
    let mut h = harness_with(FakeUpdater::installed(InstallMethod::Deb));
    assert_eq!(h.core.state().update.method, Some(InstallMethod::Deb));
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(statuses(&mut h), [UpdateStatus::Checking, UpdateStatus::UpToDate { checked_at_ms: T0 }]);

    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    advance(Duration::from_secs(60)).await;
    h.core.update_check().unwrap();
    settle().await;
    let available = UpdateStatus::Available {
        version: "0.2.0".into(),
        notes: Some("## 0.2.0\n\n- What changed".into()),
        date: Some("2026-10-02T08:00:00Z".into()),
        checked_at_ms: T0 + 60_000,
    };
    assert_eq!(statuses(&mut h), [UpdateStatus::Checking, available]);
    assert_eq!(h.updater.calls(), ["check", "check"]);
    // A check the user asked for answers in Settings › About, without a toast.
    assert!(h.notices().is_empty());
}

#[tokio::test(start_paused = true)]
async fn one_run_at_a_time() {
    let updater = FakeUpdater::installed(InstallMethod::Appimage);
    updater.hold.store(true, Ordering::SeqCst);
    let h = harness_with(updater);
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(status(&h), UpdateStatus::Checking);
    assert_eq!(code_err(h.core.update_check()), ErrorCode::UpdateBusy);
    assert_eq!(code_err(h.core.update_install()), ErrorCode::UpdateBusy);

    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    h.updater.hold.store(false, Ordering::SeqCst);
    h.updater.resume();
    settle().await;
    assert!(matches!(status(&h), UpdateStatus::Available { .. }));
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(h.updater.calls(), ["check", "check"]);
}

#[tokio::test(start_paused = true)]
async fn install_downloads_with_progress_backs_up_and_installs() {
    let updater = FakeUpdater::installed(InstallMethod::Nsis);
    *updater.check.lock() = Ok(Some(FakeUpdater::release("0.3.0")));
    let total = 400 * PROGRESS_MIN_STEP;
    // Every hundredth for the first tenth, half-hundredths in between, then the rest.
    let mut steps: Vec<(u64, Option<u64>)> = (1..=20).map(|n| (n * total / 200, Some(total))).collect();
    steps.push((total, Some(total)));
    *updater.progress.lock() = steps;
    let mut h = harness_with(updater);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    // Automatic backups were just turned on: a backup is due within the debounce.
    let backups = h.dir.path().join("backups");
    fs::create_dir_all(&backups).unwrap();
    let auto_backup = AutoBackup { enabled: true, dir: Some(backups.display().to_string()), keep: 10 };
    h.core.set_settings(Settings { auto_backup, ..Settings::default() }).unwrap();
    statuses(&mut h);

    h.core.update_install().unwrap();
    finished(&h).await;
    settle().await;
    let seen = statuses(&mut h);
    assert_eq!(seen.first(), Some(&UpdateStatus::Checking));
    assert_eq!(seen.last(), Some(&UpdateStatus::Installing { version: "0.3.0".into() }));
    let received: Vec<u64> = seen
        .iter()
        .filter_map(|s| match s {
            UpdateStatus::Downloading { received, total: t, .. } => {
                assert!(t.is_none() || *t == Some(total));
                Some(*received)
            }
            _ => None,
        })
        .collect();
    let mut expected: Vec<u64> = vec![0];
    expected.extend((1..=10).map(|n| n * total / 100));
    expected.push(total);
    assert_eq!(received, expected, "a state per hundredth and at the end, not per chunk");
    assert_eq!(h.updater.calls(), ["check", "download", "install"]);
    assert_eq!(fs::read_dir(&backups).unwrap().count(), 1, "the pending backup was written before the install");
    assert_eq!(h.core.state().phase, Phase::Unlocked);
}

#[tokio::test(start_paused = true)]
async fn install_with_nothing_newer_ends_up_to_date() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::Rpm));
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h), UpdateStatus::UpToDate { checked_at_ms: T0 });
    assert_eq!(h.updater.calls(), ["check"]);
}

#[tokio::test(start_paused = true)]
async fn a_failed_step_shows_its_code_and_frees_the_updater() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::App));
    *h.updater.check.lock() = Err(UpdateFailure::Network);
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(status(&h), UpdateStatus::Failed { code: ErrorCode::UpdateNetwork, at_ms: T0 });

    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    *h.updater.download.lock() = Err(UpdateFailure::Signature);
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h), UpdateStatus::Failed { code: ErrorCode::UpdateSignature, at_ms: T0 });

    *h.updater.download.lock() = Ok(());
    *h.updater.install.lock() = Err(UpdateFailure::Cancelled);
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h), UpdateStatus::Failed { code: ErrorCode::UpdateCancelled, at_ms: T0 });
    assert_eq!(h.updater.calls(), ["check", "check", "download", "check", "download", "install"]);
}

#[tokio::test(start_paused = true)]
async fn automatic_checks_follow_the_setting_and_announce_each_release_once() {
    let mut h = harness_with(FakeUpdater::installed(InstallMethod::Deb));
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    // Off by default: nothing goes online, however long Lockra runs.
    advance(STARTUP_CHECK_DELAY + CHECK_INTERVAL).await;
    assert!(h.updater.calls().is_empty());

    h.core.set_settings(automatic(true)).unwrap();
    settle().await;
    assert_eq!(h.updater.calls(), ["check"], "turning it on checks at once");
    assert_eq!(h.notices(), [Notice::UpdateAvailable { version: "0.2.0".into() }]);

    advance(CHECK_INTERVAL).await;
    assert_eq!(h.updater.calls().len(), 2, "then once a day");
    assert!(h.notices().is_empty(), "the same release is announced once");
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.1")));
    advance(CHECK_INTERVAL).await;
    assert_eq!(h.notices(), [Notice::UpdateAvailable { version: "0.2.1".into() }]);

    h.core.set_settings(automatic(false)).unwrap();
    advance(CHECK_INTERVAL * 2).await;
    assert_eq!(h.updater.calls().len(), 3, "off again: no more checks");
}

#[tokio::test(start_paused = true)]
async fn after_a_restart_the_check_waits_and_a_failure_retries_within_the_hour() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::Appimage));
    h.core.set_settings(automatic(true)).unwrap();
    settle().await;

    let updater = Arc::new(FakeUpdater::installed(InstallMethod::Appimage));
    *updater.check.lock() = Err(UpdateFailure::Network);
    let core = start(h.dir.path(), Arc::clone(&h.keychain), Arc::clone(&h.clipboard), Arc::clone(&updater));
    advance(STARTUP_CHECK_DELAY - Duration::from_secs(1)).await;
    assert!(updater.calls().is_empty(), "not before the start delay");
    advance(Duration::from_secs(1)).await;
    assert_eq!(updater.calls(), ["check"]);
    assert!(matches!(core.state().update.status, UpdateStatus::Failed { code: ErrorCode::UpdateNetwork, .. }));
    advance(CHECK_RETRY).await;
    assert_eq!(updater.calls(), ["check", "check"], "offline at start: the next try is within the hour");
}

#[tokio::test(start_paused = true)]
async fn the_automatic_check_skips_a_turn_while_the_user_runs_one() {
    let updater = FakeUpdater::installed(InstallMethod::Deb);
    updater.hold.store(true, Ordering::SeqCst);
    let h = harness_with(updater);
    h.core.update_check().unwrap();
    settle().await;
    h.core.set_settings(automatic(true)).unwrap();
    settle().await;
    assert_eq!(h.updater.calls(), ["check"], "the user's check answers the question");

    h.updater.hold.store(false, Ordering::SeqCst);
    h.updater.resume();
    settle().await;
    advance(CHECK_INTERVAL).await;
    assert_eq!(h.updater.calls(), ["check", "check"], "the automatic one comes back a day later");
}
