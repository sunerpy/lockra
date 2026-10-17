//! Leaving the app locks the vault, from the activity's own pause as Tauri reports it
//! (`WindowEvent::Suspended`, the activity's `onPause`): the webview's `visibilitychange` (App.tsx)
//! locks too, but a CI run of 2026-10-17 showed it missing once, the vault still open after Home.
//! A screen of the phone's own that Lockra opens over itself (the camera, the file pickers) pauses
//! the activity as well without the app being left: while one is up, the vault stays open. Leaving
//! from the camera's page locks in scanner.rs; from a picker, which runs in another app, the
//! auto-lock time does.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tauri::Runtime;

/// The screens of the phone's own that are up over the app.
#[derive(Debug, Clone, Default)]
pub struct PhoneScreens(Arc<AtomicUsize>);

impl PhoneScreens {
    /// One is up while the guard lives.
    pub fn over(&self) -> Over {
        self.0.fetch_add(1, Ordering::SeqCst);
        Over(self.0.clone())
    }

    /// Whether one is up.
    pub fn open(&self) -> bool {
        self.0.load(Ordering::SeqCst) > 0
    }
}

/// A screen of the phone's own, up until dropped.
#[must_use = "the screen is up only while the guard lives"]
pub struct Over(Arc<AtomicUsize>);

impl Drop for Over {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The guard for a screen of the phone's that `app` opens, when the shell keeps the count (the
/// host tests' bare apps do not).
pub fn over<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<Over> {
    use tauri::Manager as _;
    app.try_state::<PhoneScreens>().map(|screens| screens.over())
}

/// The activity paused: whether that leaves the app, which locks the vault.
pub fn leaves(screens: &PhoneScreens) -> bool {
    !screens.open()
}

/// The window's events: on a pause that leaves the app, the vault locks, off the UI thread (a
/// backup still waiting goes out first, `Core::lock_vault`).
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    #[cfg(mobile)]
    if matches!(event, tauri::WindowEvent::Suspended) {
        use lockra_core::Core;
        use tauri::Manager as _;
        let (Some(core), Some(screens)) = (window.try_state::<Core>(), window.try_state::<PhoneScreens>()) else { return };
        if leaves(&screens) {
            let core: Core = core.inner().clone();
            tauri::async_runtime::spawn_blocking(move || core.lock_vault());
        }
    }
    #[cfg(not(mobile))]
    let _ = (window, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_leaves_the_app_unless_a_screen_of_its_own_is_up() {
        let screens = PhoneScreens::default();
        assert!(leaves(&screens), "Home, another app, the screen off");
        let camera = screens.over();
        assert!(!leaves(&screens), "the camera's page over the app");
        let picker = screens.clone().over();
        drop(camera);
        assert!(!leaves(&screens), "one still up");
        drop(picker);
        assert!(leaves(&screens), "both gone: a pause leaves again");
    }

    #[test]
    fn the_guard_comes_from_the_shell_s_count_only() {
        let bare = tauri::test::mock_app();
        assert!(over(bare.handle()).is_none(), "a bare app keeps no count");
        let app = tauri::test::mock_app();
        use tauri::Manager as _;
        app.manage(PhoneScreens::default());
        let guard = over(app.handle());
        assert!(guard.is_some() && app.state::<PhoneScreens>().open());
        drop(guard);
        assert!(!app.state::<PhoneScreens>().open());
    }
}
