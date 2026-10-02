//! The in-app update on the fake update source: what the state shows at each step, which requests
//! are refused, what an install reuses, and what the automatic update does at start.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::*;
use crate::fakes::FakeUpdater;
use crate::ports::UpdateFailure;
use crate::settings::SettingsStore;
use crate::ui::{InstallMethod, UpdateStatus};
use crate::{PROGRESS_MIN_STEP, STARTUP_CHECK_DELAY, UPDATE_MARKER_FILE};

fn status(core: &Core) -> UpdateStatus {
    core.state().update.status
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

/// Let a background run reach a status `done` accepts (the fake yields between progress steps,
/// more often than `settle` does).
async fn reaches(core: &Core, done: impl Fn(&UpdateStatus) -> bool) {
    for _ in 0..500 {
        tokio::task::yield_now().await;
        if done(&status(core)) {
            return;
        }
    }
    panic!("the update run did not get there: {:?}", status(core));
}

/// A run that ends: nothing newer, a release found, downloaded, installing, or failed.
fn ended(status: &UpdateStatus) -> bool {
    matches!(
        status,
        UpdateStatus::UpToDate { .. }
            | UpdateStatus::Available { .. }
            | UpdateStatus::Ready { .. }
            | UpdateStatus::Installing { .. }
            | UpdateStatus::Failed { .. }
    )
}

fn installing(status: &UpdateStatus) -> bool {
    matches!(status, UpdateStatus::Installing { .. })
}

fn automatic(on: bool) -> Settings {
    Settings { auto_update: on, ..Settings::default() }
}

fn available(version: &str, checked_at_ms: u64) -> UpdateStatus {
    UpdateStatus::Available {
        version: version.into(),
        notes: Some(format!("## {version}\n\n- What changed")),
        date: Some("2026-10-02T08:00:00Z".into()),
        checked_at_ms,
    }
}

fn ready(version: &str) -> UpdateStatus {
    UpdateStatus::Ready { version: version.into() }
}

/// The version the automatic update remembers as downloaded.
fn remembered(h: &Harness) -> Option<String> {
    let bytes = fs::read(h.data_dir().join(UPDATE_MARKER_FILE)).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes).ok()?["version"].as_str().map(str::to_owned)
}

/// What an earlier run downloaded and left for the next start.
fn remember(h: &Harness, version: &str) {
    fs::create_dir_all(h.data_dir()).unwrap();
    fs::write(h.data_dir().join(UPDATE_MARKER_FILE), format!(r#"{{"version":"{version}"}}"#)).unwrap();
}

/// A new start on the harness's directories with automatic updates saved as on.
fn start_automatic(h: &Harness, updater: &Arc<FakeUpdater>) -> Core {
    SettingsStore::new(&h.dir.path().join("config")).save(&automatic(true)).unwrap();
    start(h.dir.path(), Arc::clone(&h.keychain), Arc::clone(&h.clipboard), Arc::clone(updater), Arc::clone(&h.sync))
}

fn release_out(method: InstallMethod, version: &str) -> Arc<FakeUpdater> {
    let updater = FakeUpdater::installed(method);
    *updater.check.lock() = Ok(Some(FakeUpdater::release(version)));
    Arc::new(updater)
}

#[tokio::test(start_paused = true)]
async fn a_copy_that_cannot_update_itself_refuses_and_never_goes_online() {
    let h = harness();
    let state = h.core.state();
    assert_eq!((state.update.method, state.update.status), (None, UpdateStatus::Idle));
    assert_eq!(code_err(h.core.update_check()), ErrorCode::UpdateUnavailable);
    assert_eq!(code_err(h.core.update_install()), ErrorCode::UpdateUnavailable);
    h.core.set_settings(automatic(true)).unwrap();
    let core = h.restart();
    advance(STARTUP_CHECK_DELAY * 10).await;
    assert!(h.updater.calls().is_empty());
    assert_eq!(status(&core), UpdateStatus::Idle);
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
    assert_eq!(statuses(&mut h), [UpdateStatus::Checking, available("0.2.0", T0 + 60_000)]);
    assert_eq!(h.updater.calls(), ["check", "check"], "a check downloads nothing");
    // The title bar and Settings show what a check found: there is no toast.
    assert!(h.notices().is_empty());
}

#[tokio::test(start_paused = true)]
async fn one_run_at_a_time() {
    let updater = FakeUpdater::installed(InstallMethod::Appimage);
    updater.hold.store(true, Ordering::SeqCst);
    let h = harness_with(updater);
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(status(&h.core), UpdateStatus::Checking);
    assert_eq!(code_err(h.core.update_check()), ErrorCode::UpdateBusy);
    assert_eq!(code_err(h.core.update_install()), ErrorCode::UpdateBusy);

    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    h.updater.hold.store(false, Ordering::SeqCst);
    h.updater.resume();
    settle().await;
    assert!(matches!(status(&h.core), UpdateStatus::Available { .. }));
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
    reaches(&h.core, installing).await;
    settle().await;
    let seen = statuses(&mut h);
    assert_eq!(seen[..2], [UpdateStatus::Checking, available("0.3.0", T0)]);
    assert_eq!(seen[seen.len() - 2..], [ready("0.3.0"), UpdateStatus::Installing { version: "0.3.0".into() }]);
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
    assert_eq!(remembered(&h), None, "installed: nothing is left for the next start");
}

#[tokio::test(start_paused = true)]
async fn installing_what_a_check_found_asks_and_downloads_once() {
    let mut h = harness_with(FakeUpdater::installed(InstallMethod::Msi));
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.3.0")));
    h.core.update_check().unwrap();
    reaches(&h.core, ended).await;
    // A second check forgets the first answer and asks again.
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.3.1")));
    h.core.update_check().unwrap();
    reaches(&h.core, |s| *s == available("0.3.1", T0)).await;
    statuses(&mut h);

    h.core.update_install().unwrap();
    reaches(&h.core, installing).await;
    assert_eq!(h.updater.calls(), ["check", "check", "download", "install"], "the install goes on from the check");
    let seen = statuses(&mut h);
    assert_eq!(seen.first(), Some(&UpdateStatus::Downloading { version: "0.3.1".into(), received: 0, total: None }));
    assert_eq!(seen.last(), Some(&UpdateStatus::Installing { version: "0.3.1".into() }));
}

#[tokio::test(start_paused = true)]
async fn install_with_nothing_newer_ends_up_to_date() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::Rpm));
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h.core), UpdateStatus::UpToDate { checked_at_ms: T0 });
    assert_eq!(h.updater.calls(), ["check"]);
}

#[tokio::test(start_paused = true)]
async fn a_failed_step_shows_its_code_and_frees_the_updater() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::App));
    *h.updater.check.lock() = Err(UpdateFailure::Network);
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(status(&h.core), UpdateStatus::Failed { code: ErrorCode::UpdateNetwork, at_ms: T0 });

    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    *h.updater.download.lock() = Err(UpdateFailure::Signature);
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h.core), UpdateStatus::Failed { code: ErrorCode::UpdateSignature, at_ms: T0 });

    // A failed run keeps nothing: the next one asks again.
    *h.updater.download.lock() = Ok(());
    *h.updater.install.lock() = Err(UpdateFailure::Cancelled);
    h.core.update_install().unwrap();
    settle().await;
    assert_eq!(status(&h.core), UpdateStatus::Failed { code: ErrorCode::UpdateCancelled, at_ms: T0 });
    assert_eq!(h.updater.calls(), ["check", "check", "download", "check", "download", "install"]);
}

#[tokio::test(start_paused = true)]
async fn automatic_updates_are_off_by_default() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::Deb));
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    advance(STARTUP_CHECK_DELAY * 100).await;
    assert!(h.updater.calls().is_empty(), "nothing goes online, however long Lockra runs");
    assert_eq!(status(&h.core), UpdateStatus::Idle);
}

#[tokio::test(start_paused = true)]
async fn at_start_the_automatic_update_downloads_and_waits_for_the_restart() {
    let h = harness();
    let updater = release_out(InstallMethod::Deb, "0.2.0");
    let core = start_automatic(&h, &updater);
    advance(STARTUP_CHECK_DELAY - Duration::from_secs(1)).await;
    assert!(updater.calls().is_empty(), "not before the start delay");
    advance(Duration::from_secs(1)).await;
    reaches(&core, ended).await;
    assert_eq!(updater.calls(), ["check", "download"], "downloaded and verified, not installed");
    assert_eq!(status(&core), ready("0.2.0"));
    assert_eq!(remembered(&h), Some("0.2.0".into()));

    // 「重启并更新」 installs the package already downloaded.
    core.update_install().unwrap();
    reaches(&core, installing).await;
    assert_eq!(updater.calls(), ["check", "download", "install"]);
    assert_eq!(remembered(&h), None);
}

#[tokio::test(start_paused = true)]
async fn a_switch_saved_before_0_3_2_neither_checks_nor_installs_at_start() {
    // 0.2.0 (checks only) → 0.3.0 saved `auto_update: true` without a schema → this version.
    let h = harness();
    fs::create_dir_all(h.dir.path().join("config")).unwrap();
    fs::write(h.dir.path().join("config/settings.json"), r#"{"auto_update":true}"#).unwrap();
    remember(&h, "0.2.0");
    let updater = release_out(InstallMethod::Deb, "0.2.0");
    let core = start(h.dir.path(), Arc::clone(&h.keychain), Arc::clone(&h.clipboard), Arc::clone(&updater), Arc::clone(&h.sync));
    assert!(!core.state().settings.auto_update);
    advance(STARTUP_CHECK_DELAY * 10).await;
    assert!(updater.calls().is_empty(), "nothing goes online until the switch is turned on again");
}

#[tokio::test(start_paused = true)]
async fn the_next_start_installs_the_version_it_downloaded_before() {
    // The user quit instead of restarting: the same release is installed at the next start.
    let h = harness();
    remember(&h, "0.2.0");
    let updater = release_out(InstallMethod::Appimage, "0.2.0");
    let core = start_automatic(&h, &updater);
    advance(STARTUP_CHECK_DELAY).await;
    reaches(&core, installing).await;
    assert_eq!(updater.calls(), ["check", "download", "install"]);
    assert_eq!(remembered(&h), None);
}

#[tokio::test(start_paused = true)]
async fn a_release_newer_than_the_remembered_one_waits_for_the_user_again() {
    let h = harness();
    remember(&h, "0.2.0");
    let updater = release_out(InstallMethod::Appimage, "0.2.1");
    let core = start_automatic(&h, &updater);
    advance(STARTUP_CHECK_DELAY).await;
    reaches(&core, ended).await;
    assert_eq!(updater.calls(), ["check", "download"], "a version the user has not seen is never installed unattended");
    assert_eq!(status(&core), ready("0.2.1"));
    assert_eq!(remembered(&h), Some("0.2.1".into()));
}

#[tokio::test(start_paused = true)]
async fn turning_automatic_updates_on_downloads_now_and_installs_nothing() {
    let mut h = harness_with(FakeUpdater::installed(InstallMethod::Nsis));
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    // Even a version remembered from an earlier start waits: the switch only downloads.
    remember(&h, "0.2.0");
    h.core.set_settings(automatic(true)).unwrap();
    reaches(&h.core, ended).await;
    assert_eq!(h.updater.calls(), ["check", "download"]);
    assert_eq!(status(&h.core), ready("0.2.0"));

    // Off again: the downloaded update stays ready, and nothing else goes online.
    h.core.set_settings(automatic(false)).unwrap();
    advance(STARTUP_CHECK_DELAY * 100).await;
    assert_eq!(h.updater.calls().len(), 2);
    assert_eq!(status(&h.core), ready("0.2.0"));
    assert!(h.notices().is_empty(), "no toast: the title bar shows a waiting update");
}

#[tokio::test(start_paused = true)]
async fn the_automatic_update_skips_its_turn_while_the_user_runs_one() {
    let h = harness();
    let updater = FakeUpdater::installed(InstallMethod::Deb);
    updater.hold.store(true, Ordering::SeqCst);
    let updater = Arc::new(updater);
    let core = start_automatic(&h, &updater);
    core.update_check().unwrap();
    settle().await;
    advance(STARTUP_CHECK_DELAY).await;
    assert_eq!(updater.calls(), ["check"], "the user's check answers the question");

    updater.hold.store(false, Ordering::SeqCst);
    updater.resume();
    reaches(&core, ended).await;
    advance(STARTUP_CHECK_DELAY * 100).await;
    assert_eq!(updater.calls(), ["check"], "and the automatic one does not come back");
}

#[tokio::test(start_paused = true)]
async fn nothing_newer_forgets_the_remembered_version() {
    let h = harness_with(FakeUpdater::installed(InstallMethod::Rpm));
    remember(&h, "0.2.0");
    h.core.update_check().unwrap();
    reaches(&h.core, ended).await;
    assert_eq!(status(&h.core), UpdateStatus::UpToDate { checked_at_ms: T0 });
    assert_eq!(remembered(&h), None);
}
