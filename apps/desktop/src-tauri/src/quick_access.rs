//! The quick search (plan §四, "全局热键"): a global shortcut, the tray's 「快捷搜索」 and
//! `lockra-desktop --palette` bring the window up with the command palette, from any app. Where
//! apps get no global shortcuts (Wayland) the user binds that command in the desktop's own keyboard
//! settings; a running Lockra hears it through the single-instance check.
//!
//! The core keeps the shortcut chosen (`settings.json`) and decides whether the palette opens: only
//! while unlocked. This module registers the shortcut with the system, from Rust only (the webview
//! has no permission for the plugin, which is added only once a shortcut is to be registered),
//! reports whether it could, and when the palette it opened closes after a copy, puts the window
//! back where it was.

use std::ffi::OsStr;

use lockra_core::{Core, CoreError};
use parking_lot::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager as _, Runtime, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, ShortcutState};

use crate::{MAIN_WINDOW, background, browser};

/// Started with it, Lockra brings its window up with the command palette (a running Lockra does,
/// through the single-instance check): the command to bind where apps get no global shortcuts.
pub const PALETTE: &str = "--palette";

/// What a start of `lockra-desktop` asks for, from its arguments (the program's own first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// The window, as any start.
    Window,
    /// The window with the command palette (`--palette`).
    Palette,
    /// Lockra for the browser extension (`--from-browser`): a running Lockra keeps its window as it
    /// is, and a new one stays in the tray while it runs in the background.
    Browser,
    /// Lockra at login (`--autostart`, src/autostart.rs): as for the browser.
    Login,
}

/// What `args` (the program's own first) ask of Lockra; the palette when both flags are given.
pub fn start_reason<S: AsRef<OsStr>>(args: &[S]) -> Start {
    let given = |flag: &str| args.iter().skip(1).any(|arg| arg.as_ref() == OsStr::new(flag));
    if given(PALETTE) {
        Start::Palette
    } else if given(browser::FROM_BROWSER) {
        Start::Browser
    } else if given(crate::autostart::AUTOSTART) {
        Start::Login
    } else {
        Start::Window
    }
}

/// Whether this desktop lets an app take a global shortcut. Windows and macOS do. On Linux only an
/// X11 session does: a Wayland compositor keeps the keys from apps, XWayland's included, so a
/// shortcut registered there would never fire; and without an X server there is nothing to
/// register with. Read from `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY` and `DISPLAY`.
pub fn global_shortcuts_possible(linux: bool, session_type: Option<&str>, wayland_display: Option<&str>, x_display: Option<&str>) -> bool {
    if !linux {
        return true;
    }
    let set = |value: Option<&str>| value.is_some_and(|v| !v.trim().is_empty());
    let wayland = match session_type.map(str::trim) {
        Some(kind) if kind.eq_ignore_ascii_case("wayland") => true,
        Some(kind) if kind.eq_ignore_ascii_case("x11") => false,
        _ => set(wayland_display),
    };
    !wayland && set(x_display)
}

/// How the window was before the quick search brought it up: what a copy from its palette puts
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Before {
    /// Hidden in the tray.
    Hidden,
    /// Minimized.
    Minimized,
}

/// What a summon remembers of the window: hidden or minimized, as it is now; already up, the
/// record of an earlier summon whose palette is still open (the shortcut pressed again), else
/// nothing to put back.
pub fn remember(visible: bool, minimized: bool, pending: Option<Before>) -> Option<Before> {
    if !visible {
        Some(Before::Hidden)
    } else if minimized {
        Some(Before::Minimized)
    } else {
        pending
    }
}

/// What the end of a quick search does with the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// Hide it again (on macOS the app hides, so the app the user came from has the keys again).
    Hide,
    /// Minimize it again.
    Minimize,
    /// Leave it where it is.
    Stay,
}

/// After a copy from the palette the window goes back where it was; closed without one (Esc,
/// another action), it stays where the user now is.
pub fn restore(before: Option<Before>, copied: bool) -> Restore {
    match (before, copied) {
        (Some(Before::Hidden), true) => Restore::Hide,
        (Some(Before::Minimized), true) => Restore::Minimize,
        _ => Restore::Stay,
    }
}

/// The shell's side of the quick search, in Tauri's state.
pub struct QuickAccess {
    /// This shell registers with the system, and this desktop lets it.
    system: bool,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The shortcut last asked for (`sync`), registered or not.
    asked: Option<String>,
    /// The shortcut registered with the system now.
    registered: Option<String>,
    /// The global-shortcut plugin is in.
    plugin: bool,
    /// How the window was before the quick search brought it up, while its palette is open.
    before: Option<Before>,
}

impl QuickAccess {
    /// The quick search's state; `system`: register with the system where this desktop allows it
    /// (off in the mock-runtime tests, which then behave as a desktop that gives apps none).
    pub fn new(system: bool) -> Self {
        let env = |name: &str| std::env::var(name).ok();
        let possible = global_shortcuts_possible(
            cfg!(all(unix, not(target_os = "macos"))),
            env("XDG_SESSION_TYPE").as_deref(),
            env("WAYLAND_DISPLAY").as_deref(),
            env("DISPLAY").as_deref(),
        );
        if system && !possible {
            tracing::info!("this desktop gives apps no global shortcuts: the quick search is `lockra-desktop --palette`");
        }
        Self { system: system && possible, inner: Mutex::new(Inner::default()) }
    }
}

/// The window came up some other way (the tray, the Dock, another start, an extension's request)
/// or the vault locked: a copy from a palette still open no longer sends it back.
pub fn forget<R: Runtime>(app: &AppHandle<R>) {
    if let Some(quick) = app.try_state::<QuickAccess>() {
        quick.inner.lock().before = None;
    }
}

/// Bring the window up and ask the core for the command palette, which it opens only while
/// unlocked; how the window was is kept for the end of this quick search.
pub fn summon<R: Runtime>(app: &AppHandle<R>) {
    let Some(core) = app.try_state::<Core>() else { return };
    let (visible, minimized) = match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => (window.is_visible().unwrap_or(true), window.is_minimized().unwrap_or(false)),
        None => (true, false),
    };
    let quick = app.try_state::<QuickAccess>();
    let pending = quick.as_ref().and_then(|q| q.inner.lock().before.take());
    background::show_main(app);
    // Locked, the window shows the unlock screen and no palette opens: nothing to put back.
    let opened = core.quick_access_open();
    if let Some(quick) = quick {
        quick.inner.lock().before = remember(visible, minimized, pending).filter(|_| opened);
    }
}

/// The palette the quick search opened is closed. After a copy from it (`copied`) the window goes
/// back where it was: hidden or minimized again if the quick search had brought it up. Answers
/// nothing.
#[tauri::command]
pub async fn quick_access_done<R: Runtime>(app: AppHandle<R>, quick: State<'_, QuickAccess>, copied: bool) -> Result<(), CoreError> {
    let before = quick.inner.lock().before.take();
    match restore(before, copied) {
        Restore::Hide => hide(&app),
        Restore::Minimize => {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.minimize();
            }
        }
        Restore::Stay => {}
    }
    Ok(())
}

fn hide<R: Runtime>(app: &AppHandle<R>) {
    // macOS keeps an app in front with its windows gone: hiding the app gives the keys back to the
    // app the user came from, and the Dock or the shortcut brings it up again.
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.hide();
    }
}

/// Follow the shortcut the core wants registered (`UiState.quick_access.shortcut`, at start and with
/// every state): the one registered goes, the new one comes, and the core hears whether the system
/// took it. The plugin wants its calls on the main thread; where this desktop gives apps no global
/// shortcuts every one is reported unavailable.
pub fn sync<R: Runtime>(app: &AppHandle<R>, wanted: Option<&str>) {
    let Some(quick) = app.try_state::<QuickAccess>() else { return };
    {
        let mut inner = quick.inner.lock();
        if inner.asked.as_deref() == wanted {
            return;
        }
        inner.asked = wanted.map(str::to_owned);
    }
    let handle = app.clone();
    let wanted = wanted.map(str::to_owned);
    if let Err(error) = app.run_on_main_thread(move || apply(&handle, wanted)) {
        tracing::warn!(%error, "the quick search's shortcut was not followed");
    }
}

fn apply<R: Runtime>(app: &AppHandle<R>, wanted: Option<String>) {
    let (Some(quick), Some(core)) = (app.try_state::<QuickAccess>(), app.try_state::<Core>()) else { return };
    let mut inner = quick.inner.lock();
    if inner.registered == wanted {
        return;
    }
    if let Some(old) = inner.registered.take()
        && let Err(error) = app.global_shortcut().unregister(old.as_str())
    {
        tracing::warn!(%error, "the quick search's old shortcut was not unregistered");
    }
    let Some(shortcut) = wanted else { return };
    let registered = quick.system && register(app, &mut inner, &shortcut);
    if registered {
        inner.registered = Some(shortcut.clone());
    }
    drop(inner);
    core.quick_access_registered(&shortcut, registered);
}

fn register<R: Runtime>(app: &AppHandle<R>, inner: &mut Inner, shortcut: &str) -> bool {
    if !inner.plugin {
        if let Err(error) = load_plugin(app) {
            tracing::warn!(%error, "the global shortcut plugin did not start");
            return false;
        }
        inner.plugin = true;
    }
    match app.global_shortcut().register(shortcut) {
        Ok(()) => true,
        Err(error) => {
            tracing::info!(%error, shortcut, "the quick search's shortcut was not registered");
            false
        }
    }
}

/// Add the global-shortcut plugin, whose one handler summons the quick search on a press. Done
/// once, on the main thread, the first time a shortcut is registered (and by the tests, which check
/// that its commands are not the webview's).
pub fn load_plugin<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    app.plugin(plugin())
}

fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            // On the main thread, and never waited for here: on X11 this handler runs on
            // global-hotkey's own thread, which a registration on the main thread waits for.
            let handle = app.clone();
            if let Err(error) = app.run_on_main_thread(move || summon(&handle)) {
                tracing::warn!(%error, "the quick search did not come up");
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_asks_for_the_palette_the_browser_or_the_window() {
        assert_eq!(start_reason(&["lockra-desktop"]), Start::Window);
        assert_eq!(start_reason(&["lockra-desktop", "--palette"]), Start::Palette);
        assert_eq!(start_reason(&["lockra-desktop", "--autostart"]), Start::Login);
        assert_eq!(start_reason(&["lockra-desktop", "--autostart", "--palette"]), Start::Palette);
        assert_eq!(start_reason(&["/usr/bin/lockra-desktop", "--from-browser"]), Start::Browser);
        assert_eq!(start_reason(&["lockra-desktop", "--from-browser", "--palette"]), Start::Palette);
        // The program's own path is not a flag, and nothing else is one either.
        assert_eq!(start_reason(&["--palette"]), Start::Window);
        assert_eq!(start_reason(&["lockra-desktop", "--palette=1", "palette"]), Start::Window);
        let none: [&str; 0] = [];
        assert_eq!(start_reason(&none), Start::Window);
        assert_eq!(start_reason(&[std::ffi::OsString::from("x"), std::ffi::OsString::from(PALETTE)]), Start::Palette);
    }

    #[test]
    fn only_an_x11_session_gives_a_linux_app_global_shortcuts() {
        // Windows and macOS: always.
        assert!(global_shortcuts_possible(false, None, None, None));
        assert!(global_shortcuts_possible(false, Some("wayland"), Some("wayland-0"), None));
        // An X11 session, or an X server with no session type (Xvfb, a tty): yes.
        assert!(global_shortcuts_possible(true, Some("x11"), None, Some(":0")));
        assert!(global_shortcuts_possible(true, Some("tty"), None, Some(":120")));
        assert!(global_shortcuts_possible(true, None, None, Some(":1")));
        assert!(global_shortcuts_possible(true, Some("X11"), Some("wayland-1"), Some(":0")), "a nested compositor in X11");
        // Wayland, XWayland's DISPLAY or not; no X server at all.
        assert!(!global_shortcuts_possible(true, Some("wayland"), Some("wayland-0"), Some(":0")));
        assert!(!global_shortcuts_possible(true, Some("Wayland"), None, Some(":0")));
        assert!(!global_shortcuts_possible(true, None, Some("wayland-0"), Some(":0")));
        assert!(!global_shortcuts_possible(true, Some("tty"), None, None));
        assert!(!global_shortcuts_possible(true, None, Some(" "), Some("")));
    }

    #[test]
    fn a_copy_puts_back_a_window_the_quick_search_brought_up() {
        // Down, it was brought up; up, it stays, unless an earlier summon's palette is still open.
        assert_eq!(remember(false, false, None), Some(Before::Hidden));
        assert_eq!(remember(false, true, None), Some(Before::Hidden));
        assert_eq!(remember(true, true, None), Some(Before::Minimized));
        assert_eq!(remember(true, false, None), None);
        assert_eq!(remember(true, false, Some(Before::Hidden)), Some(Before::Hidden), "pressed again");
        assert_eq!(remember(false, false, Some(Before::Minimized)), Some(Before::Hidden));
        // After a copy it goes back; closed without one it stays where the user is.
        assert_eq!(restore(Some(Before::Hidden), true), Restore::Hide);
        assert_eq!(restore(Some(Before::Minimized), true), Restore::Minimize);
        assert_eq!(restore(None, true), Restore::Stay);
        assert_eq!(restore(Some(Before::Hidden), false), Restore::Stay);
        assert_eq!(restore(Some(Before::Minimized), false), Restore::Stay);
    }
}
