//! The tray's 「设置…」 and 「检查更新…」 (docs/architecture.md, "The tray"): the shell brings the window
//! up and asks the core, which has the webview open Settings or the update dialog while unlocked
//! (locked, the window shows the unlock screen alone).

use super::{Core, PhaseState};
use crate::ui::{Notice, UpdateStatus};

impl Core {
    /// 「设置…」: Settings opens in the window. Whether it was asked to; locked, or without a vault,
    /// the window shows the unlock or welcome screen alone.
    pub fn open_settings(&self) -> bool {
        if !matches!(self.lock().phase, PhaseState::Unlocked(_)) {
            return false;
        }
        self.notice(Notice::OpenSettings);
        true
    }

    /// 「检查更新…」: the update dialog opens, after asking for a newer release when no run is in
    /// flight and no release waits (one found, downloaded or installing): a download is never
    /// started over from the tray. Whether it was asked to, as [`Core::open_settings`].
    pub fn open_updates(&self) -> bool {
        let asks = {
            let st = self.lock();
            if !matches!(st.phase, PhaseState::Unlocked(_)) {
                return false;
            }
            !st.update.busy && matches!(st.update.status, UpdateStatus::Idle | UpdateStatus::UpToDate { .. } | UpdateStatus::Failed { .. })
        };
        // A copy that cannot update itself asks nothing: the dialog says why.
        if asks && self.shared.ports.updater.method().is_some() {
            let _ = self.update_check();
        }
        self.notice(Notice::OpenUpdates);
        true
    }
}
