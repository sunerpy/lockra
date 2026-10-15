//! The quick search (plan §四, "全局热键"): its shortcut is a setting, the shell says whether it could
//! take it, and asking for the command palette reaches the webview only while the vault is unlocked.

use super::*;
use crate::ui::QuickAccessView;

const SHORTCUT: &str = "CommandOrControl+Shift+Space";

fn choose(h: &Harness, shortcut: Option<&str>) {
    h.core.set_settings(Settings { quick_access_shortcut: shortcut.map(str::to_owned), ..h.core.state().settings }).unwrap();
}

/// The states published since the last call.
fn published(events: &mut broadcast::Receiver<UiEvent>) -> Vec<crate::ui::UiState> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            UiEvent::State { state } => Some(*state),
            UiEvent::Notice { .. } => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn the_palette_opens_from_anywhere_only_while_unlocked() {
    let mut h = harness();
    assert!(h.core.quick_access_offered());
    // No vault yet: nothing to search.
    assert!(!h.core.quick_access_open());
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.notices();
    assert!(h.core.quick_access_open());
    assert_eq!(h.notices(), [Notice::OpenPalette]);
    // Locked: the window shows the unlock screen and nothing else; the vault's names never come
    // up over another app.
    h.core.lock_vault();
    h.notices();
    assert!(!h.core.quick_access_open());
    assert!(h.notices().is_empty());
    h.core.unlock(pw(MASTER)).await.unwrap();
    assert!(h.core.quick_access_open());
}

#[tokio::test(start_paused = true)]
async fn the_shortcut_is_a_setting_and_the_shell_says_whether_it_could_take_it() {
    let h = Harness::unlocked().await;
    assert_eq!(h.core.state().quick_access, QuickAccessView { shortcut: None, error: None }, "off until one is chosen");
    choose(&h, Some(SHORTCUT));
    assert_eq!(h.core.state().quick_access, QuickAccessView { shortcut: Some(SHORTCUT.into()), error: None });
    // Another app has it, or the desktop gives none to apps: Settings › General says so at once.
    let mut events = h.core.subscribe();
    h.core.quick_access_registered(SHORTCUT, false);
    let states = published(&mut events);
    assert_eq!(states.last().map(|s| s.quick_access.error), Some(Some(ErrorCode::ShortcutUnavailable)));
    // The same report again publishes nothing (the shell watches the state it changes).
    h.core.quick_access_registered(SHORTCUT, false);
    assert!(published(&mut events).is_empty());
    // A report about a shortcut no longer chosen changes nothing; a new choice starts clean.
    h.core.quick_access_registered("Alt+K", true);
    assert_eq!(h.core.state().quick_access.error, Some(ErrorCode::ShortcutUnavailable));
    choose(&h, Some("Alt+K"));
    assert_eq!(h.core.state().quick_access, QuickAccessView { shortcut: Some("Alt+K".into()), error: None });
    h.core.quick_access_registered(SHORTCUT, false);
    assert_eq!(h.core.state().quick_access.error, None, "a late report on the old shortcut");
    h.core.quick_access_registered("Alt+K", false);
    h.core.quick_access_registered("Alt+K", true);
    assert_eq!(h.core.state().quick_access.error, None, "registered after all");
    // settings.json keeps the shortcut; what the shell reported stays in memory.
    h.core.quick_access_registered("Alt+K", false);
    let restarted = h.restart();
    assert_eq!(restarted.state().quick_access, QuickAccessView { shortcut: Some("Alt+K".into()), error: None });
    // Off again: nothing to register, nothing to report.
    choose(&h, None);
    assert_eq!(h.core.state().quick_access, QuickAccessView { shortcut: None, error: None });
    h.core.quick_access_registered("Alt+K", false);
    assert_eq!(h.core.state().quick_access.error, None);
    // A blank shortcut is none.
    choose(&h, Some("  "));
    assert_eq!(h.core.state().settings.quick_access_shortcut, None);
}

#[tokio::test(start_paused = true)]
async fn a_build_without_password_management_and_the_phone_offer_no_quick_search() {
    let mut off = harness_with_features(Features { passwords: false });
    off.core.create_vault(pw(MASTER)).await.unwrap();
    off.notices();
    // The setting stays as written (by a build that offers it), and nothing uses it.
    choose(&off, Some(SHORTCUT));
    assert_eq!(off.core.state().settings.quick_access_shortcut.as_deref(), Some(SHORTCUT));
    assert!(!off.core.quick_access_offered());
    assert_eq!(off.core.state().quick_access, QuickAccessView { shortcut: None, error: None });
    off.core.quick_access_registered(SHORTCUT, false);
    assert_eq!(off.core.state().quick_access.error, None);
    assert!(!off.core.quick_access_open());
    assert!(off.notices().is_empty());

    let dir = tempfile::tempdir().unwrap();
    let config = CoreConfig {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        app_version: "0.1.0-test".into(),
        kdf: KdfCost::FAST_INSECURE,
        platform: Platform::Android,
        features: Features { passwords: true },
    };
    let ports = Ports {
        secrets: Arc::new(FakeKeychain::default()),
        clipboard: Arc::new(FakeClipboard::default()),
        clock: Arc::new(FakeClock::new(T0)),
        updater: Arc::new(FakeUpdater::default()),
        sync: Arc::new(FakeTransport::default()),
        biometrics: Arc::new(FakeBiometrics::default()),
    };
    let phone = Core::start(config, ports);
    phone.create_vault(pw(MASTER)).await.unwrap();
    let mut events = phone.subscribe();
    phone.set_settings(Settings { quick_access_shortcut: Some(SHORTCUT.into()), ..phone.state().settings }).unwrap();
    assert!(!phone.quick_access_offered());
    assert_eq!(phone.state().quick_access, QuickAccessView { shortcut: None, error: None });
    assert!(!phone.quick_access_open());
    assert!(!std::iter::from_fn(|| events.try_recv().ok()).any(|e| matches!(e, UiEvent::Notice { notice: Notice::OpenPalette })));
}
