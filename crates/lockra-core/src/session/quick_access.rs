//! The quick search (plan §四, "全局热键"): the command palette brought up from anywhere, by a global
//! shortcut, the tray's 「快捷搜索」 or `lockra-desktop --palette`. The shell owns the window and the
//! shortcut's registration; the core keeps the choice (`settings.json`) and what the shell reported
//! about it, and decides whether the palette opens: only while unlocked, so nothing of the vault
//! comes up over another app while it is locked.

use super::{Core, PhaseState, State};
use crate::error::ErrorCode;
use crate::ui::{Notice, Platform, QuickAccessView};

impl Core {
    /// This build offers the quick search here: password management on, the desktop app.
    pub fn quick_access_offered(&self) -> bool {
        self.shared.config.features.passwords && self.shared.config.platform != Platform::Android
    }

    /// The quick search was asked for: while unlocked the webview opens the command palette
    /// ([`Notice::OpenPalette`]). Whether it was asked to; locked, or without a vault, the window
    /// shows the unlock or welcome screen alone.
    pub fn quick_access_open(&self) -> bool {
        if !self.quick_access_offered() || !matches!(self.lock().phase, PhaseState::Unlocked(_)) {
            return false;
        }
        self.notice(Notice::OpenPalette);
        true
    }

    /// The shell registered the quick search's `shortcut`, or could not (another app has it, it is
    /// not a shortcut, the desktop gives none to apps): Settings › General says so. A report about a
    /// shortcut no longer chosen changes nothing, and one that changes nothing publishes nothing
    /// (the shell reports as it follows the state).
    pub fn quick_access_registered(&self, shortcut: &str, registered: bool) {
        if !self.quick_access_offered() {
            return;
        }
        let changed = {
            let mut st = self.lock();
            if st.settings.quick_access_shortcut.as_deref() != Some(shortcut) {
                return;
            }
            let error = (!registered).then_some(ErrorCode::ShortcutUnavailable);
            std::mem::replace(&mut st.quick_access_error, error) != error
        };
        if changed {
            self.changed();
        }
    }

    pub(super) fn quick_access_view(&self, st: &State) -> QuickAccessView {
        if !self.quick_access_offered() {
            return QuickAccessView { shortcut: None, error: None };
        }
        QuickAccessView { shortcut: st.settings.quick_access_shortcut.clone(), error: st.quick_access_error }
    }
}
