//! Running in the background (plan §四, "后台运行"): with Settings' `run_in_background` on,
//! closing the window hides it and Lockra stays in the tray, so the browser extension and the
//! quick search can reach it; the tray's menu opens the window, locks the vault and quits. Off,
//! closing the window quits, as it always has. The tray comes and goes with the setting.

use lockra_core::Core;
use lockra_core::settings::LocaleSetting;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager as _, Runtime};

use crate::MAIN_WINDOW;

/// The tray icon's id.
pub const TRAY_ID: &str = "lockra";

/// The tray menu's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayWords {
    pub open: &'static str,
    pub lock: &'static str,
    pub quit: &'static str,
}

/// The tray's words in the app's language: the setting's, else the system's (the webview's own
/// language is not the shell's to ask).
pub fn tray_words(locale: LocaleSetting, system: Option<&str>) -> TrayWords {
    let chinese = match locale {
        LocaleSetting::ZhCn => true,
        LocaleSetting::En => false,
        LocaleSetting::System => system.is_some_and(|l| l.to_ascii_lowercase().starts_with("zh")),
    };
    if chinese {
        TrayWords { open: "打开 Lockra", lock: "锁定", quit: "退出 Lockra" }
    } else {
        TrayWords { open: "Open Lockra", lock: "Lock", quit: "Quit Lockra" }
    }
}

/// Bring the window up: shown, not minimized, focused.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The tray as the setting wants it: there while Lockra runs in the background, gone otherwise.
pub fn sync_tray<R: Runtime>(app: &AppHandle<R>, on: bool, locale: LocaleSetting) {
    let present = app.tray_by_id(TRAY_ID).is_some();
    if on && !present {
        if let Err(error) = build_tray(app, tray_words(locale, sys_locale::get_locale().as_deref())) {
            tracing::warn!(%error, "the tray icon could not be shown");
        }
    } else if !on && present {
        let _ = app.remove_tray_by_id(TRAY_ID);
    }
}

fn build_tray<R: Runtime>(app: &AppHandle<R>, words: TrayWords) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", words.open, true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", words.lock, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", words.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &lock, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID).tooltip("Lockra").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "open" => show_main(app),
        "lock" => {
            if let Some(core) = app.try_state::<Core>() {
                core.lock_vault();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_speaks_the_apps_language() {
        assert_eq!(tray_words(LocaleSetting::ZhCn, Some("en-US")).open, "打开 Lockra");
        assert_eq!(tray_words(LocaleSetting::En, Some("zh-CN")).lock, "Lock");
        assert_eq!(tray_words(LocaleSetting::System, Some("zh-Hans-CN")).quit, "退出 Lockra");
        assert_eq!(tray_words(LocaleSetting::System, Some("de-DE")).open, "Open Lockra");
        assert_eq!(tray_words(LocaleSetting::System, None).open, "Open Lockra");
    }
}
