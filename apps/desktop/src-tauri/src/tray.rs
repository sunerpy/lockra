//! The system tray, on Windows and macOS: with Settings' "keep running in the background" on,
//! Lockra keeps an icon there, closing the window hides it (a LAN hub goes on taking its devices'
//! changes), and the icon or its menu brings the window back, locks the vault or quits. The webview
//! gives the menu's words, as it holds the app's language. Linux has no tray here: AppIndicator
//! needs a library many systems lack and reports no clicks, so closing the window quits there (and
//! on any system other than Windows and macOS).

use lockra_core::Core;
use serde::Deserialize;
use tauri::{AppHandle, Manager as _, Runtime};

/// The tray menu's words, in the app's language.
#[derive(Debug, Clone, Deserialize)]
pub struct TrayLabels {
    pub open: String,
    pub lock: String,
    pub quit: String,
    pub tooltip: String,
}

/// What a click on the icon or a menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Lock,
    Quit,
    Ignore,
}

/// The mouse buttons of a click on the icon, as the tray reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Other,
}

/// Which half of a click: the tray reports the press and the release apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Down,
    Up,
}

/// The tray icon's id.
pub const TRAY_ID: &str = "lockra";
const OPEN: &str = "tray-open";
const LOCK: &str = "tray-lock";
const QUIT: &str = "tray-quit";

/// A menu item's action.
pub fn menu_action(id: &str) -> TrayAction {
    match id {
        OPEN => TrayAction::Open,
        LOCK => TrayAction::Lock,
        QUIT => TrayAction::Quit,
        _ => TrayAction::Ignore,
    }
}

/// A click on the icon: the left button's press opens the window, once per click (the release,
/// a double click's second press and the other buttons do nothing; the right one has the menu).
pub fn click_action(button: Button, press: Press) -> TrayAction {
    match (button, press) {
        (Button::Left, Press::Down) => TrayAction::Open,
        _ => TrayAction::Ignore,
    }
}

/// Bring the main window to the front.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Do what the tray asked.
pub fn act<R: Runtime>(app: &AppHandle<R>, action: TrayAction) {
    match action {
        TrayAction::Open => show_main(app),
        TrayAction::Lock => {
            if let Some(core) = app.try_state::<Core>() {
                core.lock_vault();
            }
        }
        TrayAction::Quit => app.exit(0),
        TrayAction::Ignore => {}
    }
}

/// Show the tray with `labels`, or take it away (`None`); `false` where this build has no tray.
#[cfg(any(target_os = "macos", windows))]
pub fn set<R: Runtime>(app: &AppHandle<R>, labels: Option<TrayLabels>) -> tauri::Result<bool> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let Some(labels) = labels else {
        app.remove_tray_by_id(TRAY_ID);
        return Ok(true);
    };
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, OPEN, &labels.open, true, None::<&str>)?,
            &MenuItem::with_id(app, LOCK, &labels.lock, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, &labels.quit, true, None::<&str>)?,
        ],
    )?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu))?;
        tray.set_tooltip(Some(&labels.tooltip))?;
        return Ok(true);
    }
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(&labels.tooltip)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| act(app, menu_action(event.id().as_ref())))
        .on_tray_icon_event(|tray, event| {
            let action = match event {
                TrayIconEvent::Click { button, button_state, .. } => click_action(
                    if button == MouseButton::Left { Button::Left } else { Button::Other },
                    if button_state == MouseButtonState::Down { Press::Down } else { Press::Up },
                ),
                _ => TrayAction::Ignore,
            };
            act(tray.app_handle(), action);
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(true)
}

/// Show the tray with `labels`, or take it away (`None`); `false` where this build has no tray.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn set<R: Runtime>(_app: &AppHandle<R>, _labels: Option<TrayLabels>) -> tauri::Result<bool> {
    Ok(false)
}

/// Whether closing the window only hides it: the tray is there to bring it back.
#[cfg(any(target_os = "macos", windows))]
pub fn holds<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

/// Whether closing the window only hides it: never, with no tray.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn holds<R: Runtime>(_app: &AppHandle<R>) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_menu_item_has_its_action_and_nothing_else_acts() {
        assert_eq!(menu_action(OPEN), TrayAction::Open);
        assert_eq!(menu_action(LOCK), TrayAction::Lock);
        assert_eq!(menu_action(QUIT), TrayAction::Quit);
        assert_eq!(menu_action("something else"), TrayAction::Ignore);
    }

    #[test]
    fn a_left_click_opens_the_window_once() {
        // A click comes as its press and its release: only one of them opens.
        let opened = [Press::Down, Press::Up].into_iter().filter(|press| click_action(Button::Left, *press) == TrayAction::Open).count();
        assert_eq!(opened, 1);
        assert_eq!(click_action(Button::Other, Press::Down), TrayAction::Ignore);
        assert_eq!(click_action(Button::Other, Press::Up), TrayAction::Ignore);
    }
}
