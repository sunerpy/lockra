//! The tray icon and running in the background (plan §四, "后台运行"; the tauri-desktop-engineering
//! skill's tray rules). The icon is there whenever Lockra runs on a desktop: the app icon's padlock
//! without its tile (icons/tray/, a template image on macOS), "Lockra" as its tooltip. A left click
//! brings the window up on Windows and macOS: the press alone, so the release and Windows' double
//! click bring it up no second time. Linux's AppIndicator sends no clicks and opens the menu, which
//! starts with 打开 Lockra. The menu, in the app's language: 打开 Lockra, 快捷搜索 (where the build
//! offers it), 锁定 · 设置…, 检查更新… (where this copy updates itself) · 退出 Lockra; 锁定, 设置… and
//! 检查更新… are off while the vault is locked, and the menu is rebuilt as any of that changes.
//!
//! Settings' `run_in_background` decides what closing the window does: on, the window hides and
//! Lockra stays in the tray, so the browser extension and the quick search can reach it; off, Lockra
//! quits, as it always has.

use lockra_core::Core;
use lockra_core::settings::LocaleSetting;
use lockra_core::ui::{Notice, Phase, UiState};
use parking_lot::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager as _, Runtime};

use crate::{MAIN_WINDOW, quick_access};

/// The tray icon's id.
pub const TRAY_ID: &str = "lockra";

/// The tray menu's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayWords {
    pub open: &'static str,
    pub search: &'static str,
    pub lock: &'static str,
    pub settings: &'static str,
    pub updates: &'static str,
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
        TrayWords { open: "打开 Lockra", search: "快捷搜索", lock: "锁定", settings: "设置…", updates: "检查更新…", quit: "退出 Lockra" }
    } else {
        TrayWords { open: "Open Lockra", search: "Quick search", lock: "Lock", settings: "Settings…", updates: "Check for updates…", quit: "Quit Lockra" }
    }
}

/// What a line of the menu, or a click on the icon, does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// Bring the window up.
    Open,
    /// The quick search: the window with the command palette.
    Search,
    /// Lock the vault.
    Lock,
    /// The window with Settings.
    Settings,
    /// The window with the update dialog, a check started when none runs and no release waits.
    Updates,
    /// Quit Lockra.
    Quit,
}

impl TrayAction {
    /// Every action, in the menu's order.
    pub const ALL: [Self; 6] = [Self::Open, Self::Search, Self::Lock, Self::Settings, Self::Updates, Self::Quit];

    /// Its menu item's id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Search => "search",
            Self::Lock => "lock",
            Self::Settings => "settings",
            Self::Updates => "updates",
            Self::Quit => "quit",
        }
    }

    /// The action of a menu item's id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }

    const fn label(self, words: &TrayWords) -> &'static str {
        match self {
            Self::Open => words.open,
            Self::Search => words.search,
            Self::Lock => words.lock,
            Self::Settings => words.settings,
            Self::Updates => words.updates,
            Self::Quit => words.quit,
        }
    }
}

/// What the menu shows: its words, and what the build and the vault allow now. A change rebuilds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayModel {
    pub words: TrayWords,
    /// The build offers the quick search.
    pub quick: bool,
    /// This copy updates itself.
    pub updates: bool,
    /// The vault is unlocked: 锁定, 设置… and 检查更新… have something to do.
    pub unlocked: bool,
}

impl TrayModel {
    /// The menu for `state` (the quick search offered or not: `quick`), in the language of its
    /// settings, the system's (`system`) when they follow the system.
    pub fn of(state: &UiState, quick: bool, system: Option<&str>) -> Self {
        Self { words: tray_words(state.settings.locale, system), quick, updates: state.update.method.is_some(), unlocked: state.phase == Phase::Unlocked }
    }
}

/// One line of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEntry {
    Item { action: TrayAction, label: &'static str, enabled: bool },
    Separator,
}

/// The menu's lines for `model`, top to bottom.
pub fn tray_entries(model: &TrayModel) -> Vec<TrayEntry> {
    let item = |action: TrayAction, enabled: bool| TrayEntry::Item { action, label: action.label(&model.words), enabled };
    let mut entries = vec![item(TrayAction::Open, true)];
    if model.quick {
        entries.push(item(TrayAction::Search, true));
    }
    entries.push(item(TrayAction::Lock, model.unlocked));
    entries.push(TrayEntry::Separator);
    entries.push(item(TrayAction::Settings, model.unlocked));
    if model.updates {
        entries.push(item(TrayAction::Updates, model.unlocked));
    }
    entries.push(TrayEntry::Separator);
    entries.push(item(TrayAction::Quit, true));
    entries
}

/// What an event on the icon does: the left button's press brings the window up. Tauri sends a
/// click's press and its release, and Windows the second press of a double click as `DoubleClick`,
/// so answering more than the press would bring the window up again. The right button opens the
/// menu by itself; Linux sends no events at all.
pub fn tray_icon_event_action(event: &TrayIconEvent) -> Option<TrayAction> {
    match event {
        TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, .. } => Some(TrayAction::Open),
        _ => None,
    }
}

/// Where the tray is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayHost {
    Windows,
    Macos,
    Linux,
}

impl TrayHost {
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

/// The colour padlock at the sizes a Windows notification area draws (16 px at 100 % scaling, 20 at
/// 125 %, 24 at 150 %, …), each rendered at its own size from icons/tray.svg so the system scales
/// nothing (scripts/build-icons.sh).
const COLOUR: [(u32, &[u8]); 8] = [
    (16, include_bytes!("../icons/tray/tray-16.png")),
    (20, include_bytes!("../icons/tray/tray-20.png")),
    (24, include_bytes!("../icons/tray/tray-24.png")),
    (28, include_bytes!("../icons/tray/tray-28.png")),
    (32, include_bytes!("../icons/tray/tray-32.png")),
    (40, include_bytes!("../icons/tray/tray-40.png")),
    (48, include_bytes!("../icons/tray/tray-48.png")),
    (64, include_bytes!("../icons/tray/tray-64.png")),
];
/// The macOS menu bar's template, 36 px for its 18 pt status items at 2x (tray-icon scales the
/// image to the bar's height).
const TEMPLATE: (u32, &[u8]) = (36, include_bytes!("../icons/tray/tray-template-36.png"));
/// What a Linux panel is given: it scales the image to its own height.
const LINUX_SIZE: u32 = 32;
/// The scale assumed when the display's could not be read.
const UNKNOWN_SCALE: f64 = 2.0;

/// The tray image for `host` on a display of `scale`: its PNG, its size in pixels, and whether it is
/// a template. Windows draws the small-icon size, 16 px times the scale: the smallest image at least
/// that big, the largest past them all.
pub fn tray_image(host: TrayHost, scale: f64) -> (&'static [u8], u32, bool) {
    match host {
        TrayHost::Macos => (TEMPLATE.1, TEMPLATE.0, true),
        TrayHost::Linux => colour(f64::from(LINUX_SIZE)),
        TrayHost::Windows => colour(16.0 * if scale.is_finite() && scale > 0.0 { scale } else { UNKNOWN_SCALE }),
    }
}

fn colour(wanted: f64) -> (&'static [u8], u32, bool) {
    let largest = COLOUR[COLOUR.len() - 1];
    // Half a pixel short counts: 1.25 × 16 is 20, and 1.24 × 16 rounds to it too.
    let (size, png) = COLOUR.iter().copied().find(|(size, _)| f64::from(*size) >= wanted - 0.5).unwrap_or(largest);
    (png, size, false)
}

/// Whether `notice` waits for the user in the window: a pairing to approve, a fill to confirm, an
/// unlock the extension asked for (boards 17–19). Lockra may be hidden in the tray or behind the
/// browser, so the window comes up with it.
pub fn brings_window_up(notice: &Notice) -> bool {
    matches!(notice, Notice::BrowserPairRequested { .. } | Notice::BrowserConfirmRequested { .. } | Notice::BrowserUnlockRequested { .. })
}

/// Bring the window up: shown, not minimized, focused. Brought up this way, it stays up after a copy
/// from a palette the quick search had opened (`quick_access::forget`).
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    quick_access::forget(app);
    // macOS: the app the quick search hid after a copy comes back first.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The menu shown, and what it was built from (`sync_tray`), in Tauri's state while there is a tray.
#[derive(Default)]
pub struct TrayState {
    shown: Mutex<Option<TrayModel>>,
}

/// The tray as `model` wants it: put up the first time, its menu rebuilt when `model` changes.
/// Comparing, building and setting the menu happen under one lock, so the menu set last is built
/// from the newest model (two states can be followed at once).
pub fn sync_tray<R: Runtime>(app: &AppHandle<R>, model: TrayModel) {
    let Some(state) = app.try_state::<TrayState>() else { return };
    let mut shown = state.shown.lock();
    if *shown == Some(model) {
        return;
    }
    let result = match app.tray_by_id(TRAY_ID) {
        Some(tray) => build_menu(app, &model).and_then(|menu| tray.set_menu(Some(menu))),
        None => install(app, &model),
    };
    match result {
        Ok(()) => *shown = Some(model),
        Err(error) => tracing::warn!(%error, "the tray icon could not be shown"),
    }
}

fn install<R: Runtime>(app: &AppHandle<R>, model: &TrayModel) -> tauri::Result<()> {
    let scale = app.primary_monitor().ok().flatten().map_or(UNKNOWN_SCALE, |monitor| monitor.scale_factor());
    let (png, size, template) = tray_image(TrayHost::current(), scale);
    let menu = build_menu(app, model)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(png)?)
        .icon_as_template(template)
        .tooltip("Lockra")
        .menu(&menu)
        // The left button brings the window up; the right one opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match TrayAction::from_id(event.id().as_ref()) {
            Some(action) => run(app, action),
            None => tracing::warn!(id = event.id().as_ref(), "tray menu event for no item"),
        })
        .on_tray_icon_event(|tray, event| {
            if let Some(action) = tray_icon_event_action(&event) {
                run(tray.app_handle(), action);
            }
        })
        .build(app)?;
    tracing::info!(size, template, "tray icon up");
    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, model: &TrayModel) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in tray_entries(model) {
        match entry {
            TrayEntry::Item { action, label, enabled } => menu.append(&MenuItem::with_id(app, action.id(), label, enabled, None::<&str>)?)?,
            TrayEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

fn run<R: Runtime>(app: &AppHandle<R>, action: TrayAction) {
    let core = app.try_state::<Core>();
    match action {
        TrayAction::Open => show_main(app),
        TrayAction::Search => quick_access::summon(app),
        TrayAction::Lock => {
            if let Some(core) = core {
                core.lock_vault();
            }
        }
        TrayAction::Settings => {
            show_main(app);
            if let Some(core) = core {
                core.open_settings();
            }
        }
        TrayAction::Updates => {
            show_main(app);
            if let Some(core) = core {
                core.open_updates();
            }
        }
        TrayAction::Quit => app.exit(0),
    }
}

#[cfg(test)]
mod tests {
    use tauri::tray::TrayIconId;
    use tauri::{PhysicalPosition, Rect};

    use super::*;

    #[test]
    fn the_tray_speaks_the_apps_language() {
        assert_eq!(tray_words(LocaleSetting::ZhCn, Some("en-US")).open, "打开 Lockra");
        assert_eq!(tray_words(LocaleSetting::En, Some("zh-CN")).lock, "Lock");
        assert_eq!(tray_words(LocaleSetting::System, Some("zh-Hans-CN")).quit, "退出 Lockra");
        assert_eq!(tray_words(LocaleSetting::System, Some("de-DE")).open, "Open Lockra");
        assert_eq!(tray_words(LocaleSetting::System, None).open, "Open Lockra");
        assert_eq!(tray_words(LocaleSetting::ZhCn, None).search, "快捷搜索");
        assert_eq!(tray_words(LocaleSetting::En, None).search, "Quick search");
        assert_eq!((tray_words(LocaleSetting::ZhCn, None).settings, tray_words(LocaleSetting::ZhCn, None).updates), ("设置…", "检查更新…"));
        assert_eq!((tray_words(LocaleSetting::En, None).settings, tray_words(LocaleSetting::En, None).updates), ("Settings…", "Check for updates…"));
    }

    fn lines(model: &TrayModel) -> Vec<(Option<TrayAction>, bool)> {
        tray_entries(model)
            .into_iter()
            .map(|entry| match entry {
                TrayEntry::Item { action, enabled, .. } => (Some(action), enabled),
                TrayEntry::Separator => (None, false),
            })
            .collect()
    }

    #[test]
    fn the_menu_offers_what_the_build_and_the_vault_allow() {
        let words = tray_words(LocaleSetting::ZhCn, None);
        let full = TrayModel { words, quick: true, updates: true, unlocked: true };
        use TrayAction::*;
        assert_eq!(
            lines(&full),
            [
                (Some(Open), true),
                (Some(Search), true),
                (Some(Lock), true),
                (None, false),
                (Some(Settings), true),
                (Some(Updates), true),
                (None, false),
                (Some(Quit), true)
            ]
        );
        assert!(tray_entries(&full).contains(&TrayEntry::Item { action: Updates, label: "检查更新…", enabled: true }));
        // Locked: what needs the vault is off; 打开, 快捷搜索 (the unlock screen) and 退出 stay.
        let locked = TrayModel { unlocked: false, ..full };
        assert_eq!(
            lines(&locked),
            [
                (Some(Open), true),
                (Some(Search), true),
                (Some(Lock), false),
                (None, false),
                (Some(Settings), false),
                (Some(Updates), false),
                (None, false),
                (Some(Quit), true)
            ]
        );
        // No quick search in this build, no update source for this copy.
        let plain = TrayModel { quick: false, updates: false, ..full };
        assert_eq!(lines(&plain), [(Some(Open), true), (Some(Lock), true), (None, false), (Some(Settings), true), (None, false), (Some(Quit), true)]);
        for action in TrayAction::ALL {
            assert_eq!(TrayAction::from_id(action.id()), Some(action));
        }
        assert_eq!(TrayAction::from_id("nothing"), None);
    }

    fn click(button: MouseButton, button_state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click { id: TrayIconId::new(TRAY_ID), position: PhysicalPosition::new(4.0, 2.0), rect: Rect::default(), button, button_state }
    }

    #[test]
    fn a_left_press_alone_brings_the_window_up() {
        let opens = |events: &[TrayIconEvent]| events.iter().filter_map(tray_icon_event_action).collect::<Vec<_>>();
        // A click is a press and a release: the window comes up once.
        let press = click(MouseButton::Left, MouseButtonState::Down);
        let release = click(MouseButton::Left, MouseButtonState::Up);
        assert_eq!(opens(&[press.clone(), release.clone()]), [TrayAction::Open]);
        // A double click on Windows: press, release, the second press as `DoubleClick`, release.
        // Its first click has brought the window up; answering a release would do it again.
        let double = TrayIconEvent::DoubleClick {
            id: TrayIconId::new(TRAY_ID),
            position: PhysicalPosition::new(4.0, 2.0),
            rect: Rect::default(),
            button: MouseButton::Left,
        };
        assert_eq!(opens(&[press, release.clone(), double, release]), [TrayAction::Open]);
        // The right button is the menu's, the middle one nothing's.
        for state in [MouseButtonState::Down, MouseButtonState::Up] {
            assert_eq!(tray_icon_event_action(&click(MouseButton::Right, state)), None);
            assert_eq!(tray_icon_event_action(&click(MouseButton::Middle, state)), None);
        }
        assert_eq!(
            tray_icon_event_action(&TrayIconEvent::Enter { id: TrayIconId::new(TRAY_ID), position: PhysicalPosition::new(4.0, 2.0), rect: Rect::default() }),
            None
        );
    }

    /// A PNG's width and height, from its header.
    fn png_size(png: &[u8]) -> (u32, u32) {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let word = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
        (word(16), word(20))
    }

    #[test]
    fn the_tray_image_is_drawn_at_the_size_the_system_shows_it() {
        for (scale, size) in [(1.0, 16), (1.25, 20), (1.5, 24), (1.75, 28), (2.0, 32), (2.5, 40), (3.0, 48), (3.5, 64), (4.0, 64), (6.0, 64)] {
            let (png, chosen, template) = tray_image(TrayHost::Windows, scale);
            assert_eq!((chosen, template), (size, false), "scale {scale}");
            assert_eq!(png_size(png), (size, size), "scale {scale}");
        }
        assert_eq!(tray_image(TrayHost::Windows, f64::NAN).1, 32, "a scale not read");
        assert_eq!(tray_image(TrayHost::Windows, 0.0).1, 32);
        let (png, size, template) = tray_image(TrayHost::Macos, 2.0);
        assert_eq!((png_size(png), size, template), ((36, 36), 36, true));
        assert_eq!(tray_image(TrayHost::Linux, 1.0).1, 32);
        // Every image has its own transparent corners: the tray behind it, no plate of its own.
        for (_, png) in COLOUR.iter().copied().chain([TEMPLATE]) {
            assert_eq!(png[25], 6, "RGBA");
        }
    }

    #[test]
    fn an_extension_waiting_for_the_user_brings_the_window_up() {
        let id = uuid::Uuid::nil();
        let browser = "chrome".to_owned();
        assert!(brings_window_up(&Notice::BrowserPairRequested { id, browser: browser.clone(), code: "123456".to_owned() }));
        assert!(brings_window_up(&Notice::BrowserConfirmRequested { id, browser: browser.clone(), title: "GitHub".to_owned() }));
        assert!(brings_window_up(&Notice::BrowserUnlockRequested { browser: browser.clone() }));
        // What only tells the user something stays a toast.
        assert!(!brings_window_up(&Notice::BrowserPaused { pairing: id, browser, label: String::new() }));
        assert!(!brings_window_up(&Notice::ItemSaved { item_id: id, title: "GitHub".to_owned(), updated: false }));
    }
}
