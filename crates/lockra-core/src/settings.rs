//! `settings.json`: appearance, security timers, sorting and the automatic backup. Plain JSON in
//! the config directory; it holds no secret. Every field has a default and a value that fails to
//! parse falls back to it, so a hand-edited or older file never stops the app from starting.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The four palettes of the design system (DESIGN.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeId {
    /// 明亮 · 白瓷.
    #[default]
    Light,
    /// 暗黑 · 夜灯.
    Dark,
    /// 暖纸 · 手稿.
    Warm,
    /// 石墨 · 仪表.
    Graphite,
}

/// The accent choices of Settings › Appearance; `default` keeps the theme's own accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentId {
    /// The theme's own accent.
    #[default]
    Default,
    /// Blue.
    Blue,
    /// Green.
    Green,
    /// Yellow.
    Yellow,
    /// Pink.
    Pink,
    /// Orange.
    Orange,
    /// Purple.
    Purple,
    /// The theme's ink colour.
    Ink,
}

/// Row height and padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// 32 px rows.
    #[default]
    Default,
    /// 28 px rows.
    Compact,
}

/// The interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocaleSetting {
    /// Chinese when the webview's language is Chinese, English otherwise.
    #[default]
    System,
    /// 简体中文.
    ZhCn,
    /// English.
    En,
}

/// The order of the code list; favourites always come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// Issuer, then account, case-insensitive.
    #[default]
    Name,
    /// Newest first.
    Added,
    /// Most recently copied first.
    Recent,
}

/// Where automatic backups go and how many are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoBackup {
    /// On or off.
    pub enabled: bool,
    /// The folder; may sit inside a cloud-synced directory. A string, not a path: the webview
    /// shows it, and a folder whose name is not valid Unicode is refused when it is chosen.
    pub dir: Option<String>,
    /// How many automatic backups to keep, 1 to 100.
    pub keep: u32,
}

impl Default for AutoBackup {
    fn default() -> Self {
        Self { enabled: false, dir: None, keep: 10 }
    }
}

/// Every user setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Palette.
    pub theme: ThemeId,
    /// Follow the system's light / dark preference (light ↔ dark only).
    pub follow_system_theme: bool,
    /// Accent colour.
    pub accent: AccentId,
    /// Row density.
    pub density: Density,
    /// Base font size, 12 to 18 px.
    pub font_size_px: u8,
    /// Turn animations and transitions off.
    pub reduce_motion: bool,
    /// Interface language.
    pub locale: LocaleSetting,
    /// Lock after this many idle minutes; 0 never locks by itself.
    pub auto_lock_minutes: u32,
    /// Clear a copied code after this many seconds, if it is still on the clipboard; 0 never.
    pub clipboard_clear_seconds: u32,
    /// Show codes as dots until hovered or focused.
    pub hide_codes: bool,
    /// Code list order.
    pub sort: SortOrder,
    /// Show the code list in sections, one per group, each of which folds.
    pub group_codes: bool,
    /// Automatic backups.
    pub auto_backup: AutoBackup,
    /// Update automatically: 10 seconds after start look for a newer release and download it in the
    /// background; it installs when the user restarts Lockra for it, or at the next start. Off:
    /// Lockra goes online only when the user checks. 0.2.0's `auto_check_updates` only checked, so
    /// it is not read: automatic downloads need this switch turned on.
    pub auto_update: bool,
}

/// Font size bounds of Settings › Appearance.
pub const FONT_SIZE_MIN: u8 = 12;
/// Font size bounds of Settings › Appearance.
pub const FONT_SIZE_MAX: u8 = 18;
/// The auto-lock choices offered in Settings › Security, in minutes (0 = never).
pub const AUTO_LOCK_CHOICES: [u32; 8] = [0, 1, 2, 5, 10, 15, 30, 60];
/// The clipboard choices offered in Settings › Security, in seconds (0 = never).
pub const CLIPBOARD_CHOICES: [u32; 6] = [0, 10, 20, 30, 60, 90];
/// Bounds of the automatic backup count.
pub const KEEP_MAX: u32 = 100;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeId::default(),
            follow_system_theme: true,
            accent: AccentId::default(),
            density: Density::default(),
            font_size_px: 14,
            reduce_motion: false,
            locale: LocaleSetting::default(),
            auto_lock_minutes: 5,
            clipboard_clear_seconds: 30,
            hide_codes: false,
            sort: SortOrder::default(),
            group_codes: true,
            auto_backup: AutoBackup::default(),
            auto_update: false,
        }
    }
}

impl Settings {
    /// The settings with every number pulled into its allowed range.
    pub fn normalized(mut self) -> Self {
        self.font_size_px = self.font_size_px.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
        if !AUTO_LOCK_CHOICES.contains(&self.auto_lock_minutes) {
            self.auto_lock_minutes = Settings::default().auto_lock_minutes;
        }
        if !CLIPBOARD_CHOICES.contains(&self.clipboard_clear_seconds) {
            self.clipboard_clear_seconds = Settings::default().clipboard_clear_seconds;
        }
        self.auto_backup.keep = self.auto_backup.keep.clamp(1, KEEP_MAX);
        if self.auto_backup.dir.as_ref().is_some_and(|d| d.trim().is_empty()) {
            self.auto_backup.dir = None;
        }
        self
    }
}

/// The version `settings.json` is written at: 2 from 0.3.2, since `auto_update` means downloading.
/// A file without it (0.2.0 to 0.3.1) cannot say the user agreed to that: its `auto_update` is read
/// as off, until the switch is turned on again.
pub const SETTINGS_SCHEMA: u64 = 2;

/// Loads and saves `settings.json`.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// The store for `config_dir/settings.json`.
    pub fn new(config_dir: &Path) -> Self {
        Self { path: config_dir.join("settings.json") }
    }

    /// The saved settings; defaults when the file is missing or unreadable. A file from before
    /// [`SETTINGS_SCHEMA`] has automatic updates off.
    pub fn load(&self) -> Settings {
        let Ok(bytes) = fs::read(&self.path) else { return Settings::default() };
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => {
                let schema = value.get("schema").and_then(serde_json::Value::as_u64).unwrap_or(0);
                let mut settings = lenient(value).normalized();
                if schema < SETTINGS_SCHEMA && settings.auto_update {
                    tracing::info!("automatic updates were saved before 0.3.2 (possibly carried over from 0.2.0's checks): off until turned on again");
                    settings.auto_update = false;
                }
                settings
            }
            Err(error) => {
                tracing::warn!(%error, "settings.json is not JSON; using defaults");
                Settings::default()
            }
        }
    }

    /// Write the settings (atomically, like the vault), with [`SETTINGS_SCHEMA`] first.
    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        #[derive(Serialize)]
        struct File<'a> {
            schema: u64,
            #[serde(flatten)]
            settings: &'a Settings,
        }
        let bytes = serde_json::to_vec_pretty(&File { schema: SETTINGS_SCHEMA, settings }).map_err(std::io::Error::other)?;
        lockra_vault::write_atomic(&self.path, &bytes)
    }
}

/// Settings from a JSON object field by field: a field that does not parse keeps its default
/// instead of discarding the whole file.
fn lenient(value: serde_json::Value) -> Settings {
    let defaults = serde_json::to_value(Settings::default()).unwrap_or_default();
    let (Some(defaults), Some(given)) = (defaults.as_object(), value.as_object()) else { return Settings::default() };
    let mut merged = defaults.clone();
    for (key, default) in defaults {
        if let Some(candidate) = given.get(key) {
            let mut trial = merged.clone();
            trial.insert(key.clone(), candidate.clone());
            if serde_json::from_value::<Settings>(serde_json::Value::Object(trial.clone())).is_ok() {
                merged = trial;
            } else {
                merged.insert(key.clone(), default.clone());
            }
        }
    }
    serde_json::from_value(serde_json::Value::Object(merged)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SettingsStore::new(dir.path()).load(), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(&dir.path().join("nested"));
        let settings = Settings {
            theme: ThemeId::Graphite,
            accent: AccentId::Purple,
            auto_lock_minutes: 15,
            auto_backup: AutoBackup { enabled: true, dir: Some("/backups".into()), keep: 3 },
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load(), settings);
    }

    #[test]
    fn a_bad_field_keeps_its_default_and_the_rest_survives() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), r#"{"theme":"dark","accent":"neon","font_size_px":99,"auto_lock_minutes":7,"future":1}"#).unwrap();
        let loaded = SettingsStore::new(dir.path()).load();
        assert_eq!(loaded.theme, ThemeId::Dark);
        assert_eq!(loaded.accent, AccentId::Default);
        assert_eq!(loaded.font_size_px, FONT_SIZE_MAX);
        assert_eq!(loaded.auto_lock_minutes, 5);
        fs::write(dir.path().join("settings.json"), b"not json").unwrap();
        assert_eq!(SettingsStore::new(dir.path()).load(), Settings::default());
    }

    #[test]
    fn normalization_clamps_and_cleans() {
        let s = Settings {
            font_size_px: 3,
            clipboard_clear_seconds: 45,
            auto_backup: AutoBackup { enabled: true, dir: Some("  ".into()), keep: 0 },
            ..Settings::default()
        }
        .normalized();
        assert_eq!((s.font_size_px, s.clipboard_clear_seconds, s.auto_backup.keep, s.auto_backup.dir), (FONT_SIZE_MIN, 30, 1, None));
    }

    #[test]
    fn wire_names_are_stable() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["theme"], "light");
        assert_eq!(json["locale"], "system");
        assert_eq!(serde_json::to_value(LocaleSetting::ZhCn).unwrap(), "zh-cn");
        assert_eq!(json["auto_backup"]["keep"], 10);
        assert_eq!(json["auto_update"], false);
        assert!(json.get("auto_check_updates").is_none(), "written under its new name only");
    }

    #[test]
    fn automatic_updates_stay_off_until_turned_on() {
        let dir = tempfile::tempdir().unwrap();
        // A settings file from before the setting existed.
        fs::write(dir.path().join("settings.json"), r#"{"theme":"dark","auto_lock_minutes":15}"#).unwrap();
        let store = SettingsStore::new(dir.path());
        assert!(!store.load().auto_update);
        store.save(&Settings { auto_update: true, ..Settings::default() }).unwrap();
        assert!(store.load().auto_update);
        // 0.2.0's `auto_check_updates` only ever checked: its yes is not a yes to downloading
        // and installing, so it is not carried over.
        fs::write(dir.path().join("settings.json"), r#"{"auto_check_updates":true}"#).unwrap();
        assert!(!store.load().auto_update);
    }

    #[test]
    fn automatic_updates_saved_before_0_3_2_are_off_until_turned_on_again() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        // 0.3.0 carried 0.2.0's check-only switch over and saved it under the new name, without a
        // schema: such a file cannot say the user agreed to downloads.
        fs::write(dir.path().join("settings.json"), r#"{"theme":"dark","auto_update":true}"#).unwrap();
        let loaded = store.load();
        assert!(!loaded.auto_update);
        assert_eq!(loaded.theme, ThemeId::Dark, "everything else is read as it was");
        // Turned on again, the file says so with its schema, and it stays on.
        store.save(&Settings { auto_update: true, ..loaded }).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("settings.json")).unwrap()).unwrap();
        assert_eq!(written["schema"], SETTINGS_SCHEMA);
        assert!(store.load().auto_update);
        // A schema from before the change counts as none.
        fs::write(dir.path().join("settings.json"), r#"{"schema":1,"auto_update":true}"#).unwrap();
        assert!(!store.load().auto_update);
    }
}
