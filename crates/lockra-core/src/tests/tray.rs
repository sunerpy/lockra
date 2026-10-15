//! The tray's 「设置…」 and 「检查更新…」 (docs/architecture.md, "The tray"): the window comes up and,
//! while unlocked, the webview opens Settings or the update dialog. 检查更新… asks for a newer release
//! only when no run is in flight and no release waits: a download is never started over from the
//! tray (the tauri-desktop-engineering skill, "The in-app update experience").

use super::*;
use crate::fakes::FakeUpdater;
use crate::ui::{InstallMethod, UpdateStatus};

#[tokio::test(start_paused = true)]
async fn settings_and_the_update_dialog_open_from_the_tray_only_while_unlocked() {
    let mut h = harness_with(FakeUpdater::installed(InstallMethod::Deb));
    // No vault yet: the welcome screen alone.
    assert!(!h.core.open_settings());
    assert!(!h.core.open_updates());
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.notices();
    assert!(h.core.open_settings());
    assert_eq!(h.notices(), [Notice::OpenSettings]);
    // Locked: the unlock screen alone, and no release is asked for.
    h.core.lock_vault();
    h.notices();
    assert!(!h.core.open_settings());
    assert!(!h.core.open_updates());
    assert!(h.notices().is_empty());
    settle().await;
    assert!(h.updater.calls().is_empty(), "{:?}", h.updater.calls());
}

#[tokio::test(start_paused = true)]
async fn check_for_updates_asks_again_only_when_nothing_runs_or_waits() {
    let updater = FakeUpdater::installed(InstallMethod::Deb);
    *updater.check.lock() = Ok(None);
    let mut h = harness_with(updater);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.notices();
    // Nothing asked yet: the dialog opens and a check starts; up to date, the next one asks again.
    assert!(h.core.open_updates());
    assert_eq!(h.notices(), [Notice::OpenUpdates]);
    settle().await;
    assert!(matches!(h.core.state().update.status, UpdateStatus::UpToDate { .. }));
    assert!(h.core.open_updates());
    settle().await;
    assert_eq!(h.updater.calls(), ["check", "check"]);
    // A release found waits in the dialog: nothing is asked again.
    *h.updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    h.core.update_check().unwrap();
    settle().await;
    assert!(matches!(h.core.state().update.status, UpdateStatus::Available { .. }));
    assert!(h.core.open_updates());
    settle().await;
    assert_eq!(h.updater.calls(), ["check", "check", "check"]);
    // A run in flight is not started twice.
    h.updater.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    *h.updater.check.lock() = Ok(None);
    h.core.update_check().unwrap();
    settle().await;
    assert_eq!(h.core.state().update.status, UpdateStatus::Checking);
    h.notices();
    assert!(h.core.open_updates());
    assert_eq!(h.notices(), [Notice::OpenUpdates], "the dialog opens on the run");
    h.updater.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    h.updater.resume();
    settle().await;
    assert_eq!(h.updater.calls(), ["check", "check", "check", "check"]);
}

#[tokio::test(start_paused = true)]
async fn a_copy_that_cannot_update_itself_asks_nothing() {
    let mut h = harness_with(FakeUpdater::default());
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.notices();
    assert!(h.core.open_updates(), "the dialog says why");
    assert_eq!(h.notices(), [Notice::OpenUpdates]);
    settle().await;
    assert!(h.updater.calls().is_empty());
    assert_eq!(h.core.state().update.status, UpdateStatus::Idle);
}
