//! The native-messaging manifests (plan §四): the file that tells a browser which program its
//! extension may start as the host `dev.lockra.browser`, and which extensions may. Written when the
//! user turns the integration on, written again at each start while it is on (the program may have
//! moved with an update), removed when it is turned off.
//!
//! | Browser  | Linux                                   | macOS (`~/Library/Application Support/…`) | Windows (HKCU, the default value the manifest's path) |
//! |----------|-----------------------------------------|-------------------------------------------|--------------------------------------------------------|
//! | Chrome   | `~/.config/google-chrome/NativeMessagingHosts/` | `Google/Chrome/NativeMessagingHosts/` | `Software\Google\Chrome\NativeMessagingHosts\…`  |
//! | Chromium | `~/.config/chromium/NativeMessagingHosts/` | `Chromium/NativeMessagingHosts/`       | `Software\Chromium\NativeMessagingHosts\…`             |
//! | Edge     | `~/.config/microsoft-edge/NativeMessagingHosts/` | `Microsoft Edge/NativeMessagingHosts/` | `Software\Microsoft\Edge\NativeMessagingHosts\…` |
//! | Brave    | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` | `BraveSoftware/Brave-Browser/NativeMessagingHosts/` | Chrome's key (Brave reads it) |
//! | Firefox  | `~/.mozilla/native-messaging-hosts/`    | `Mozilla/NativeMessagingHosts/`           | `Software\Mozilla\NativeMessagingHosts\…`              |
//!
//! On Windows the files themselves go to `%LOCALAPPDATA%\Lockra\browser\`.

use std::io;
use std::path::{Path, PathBuf};

use lockra_browser::{HOST_NAME, ids};
use serde::Serialize;

/// A debug build may write the Chromium manifests into a folder of its own (a browser started
/// with `--user-data-dir` reads `<that>/NativeMessagingHosts/`): the browser smoke test's.
pub const DEV_MANIFEST_DIR_ENV: &str = "LOCKRA_DEV_BROWSER_MANIFEST_DIR";

/// The browsers Lockra writes a manifest for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Browser {
    Chrome,
    Chromium,
    Edge,
    Brave,
    Firefox,
}

impl Browser {
    /// Every one, in the order Settings › 浏览器 lists them.
    pub const ALL: [Self; 5] = [Self::Chrome, Self::Edge, Self::Firefox, Self::Chromium, Self::Brave];

    /// Its name on the wire and in settings.
    pub fn id(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Edge => "edge",
            Self::Brave => "brave",
            Self::Firefox => "firefox",
        }
    }

    /// The browser called `id`.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.id() == id)
    }

    fn firefox(self) -> bool {
        self == Self::Firefox
    }
}

/// A browser's manifest as JSON.
#[derive(Debug, Serialize)]
pub struct Manifest {
    name: &'static str,
    description: &'static str,
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_origins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_extensions: Option<Vec<&'static str>>,
}

/// The manifest for `browser`, pointing at `program`: the Chromium ones allow the stores' ids
/// (and, in a debug build, the development one), Firefox's the add-on's id.
pub fn manifest(browser: Browser, program: &Path, debug: bool) -> Manifest {
    let (allowed_origins, allowed_extensions) = if browser.firefox() { (None, Some(vec![ids::FIREFOX])) } else { (Some(ids::chromium_origins(debug)), None) };
    Manifest { name: HOST_NAME, description: "Lockra", path: program.to_string_lossy().into_owned(), kind: "stdio", allowed_origins, allowed_extensions }
}

/// The system the paths are for (the tests check every one on any machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Linux,
    MacOs,
    Windows,
}

impl System {
    /// This machine's.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The folders a manifest's location starts from.
#[derive(Debug, Clone)]
pub struct Places {
    pub home: PathBuf,
    /// `%LOCALAPPDATA%` on Windows.
    pub local_data: PathBuf,
    /// `%APPDATA%` on Windows.
    pub roaming_data: PathBuf,
    /// A debug build's own folder for the Chromium manifests ([`DEV_MANIFEST_DIR_ENV`]).
    pub dev_dir: Option<PathBuf>,
}

impl Places {
    /// This user's.
    pub fn current(debug: bool) -> Option<Self> {
        let dev_dir = std::env::var_os(DEV_MANIFEST_DIR_ENV).filter(|_| debug).filter(|v| !v.is_empty()).map(PathBuf::from);
        Some(Self { home: dirs::home_dir()?, local_data: dirs::data_local_dir()?, roaming_data: dirs::data_dir()?, dev_dir })
    }
}

/// Where `browser` reads its manifest from (Linux and macOS), or where Lockra keeps the file the
/// registry points at (Windows).
pub fn manifest_path(system: System, browser: Browser, places: &Places) -> PathBuf {
    let file = format!("{HOST_NAME}.json");
    if let Some(dev) = places.dev_dir.as_ref().filter(|_| !browser.firefox()) {
        return dev.join("NativeMessagingHosts").join(file);
    }
    let support = places.home.join("Library/Application Support");
    let dir = match (system, browser) {
        (System::Linux, Browser::Chrome) => places.home.join(".config/google-chrome/NativeMessagingHosts"),
        (System::Linux, Browser::Chromium) => places.home.join(".config/chromium/NativeMessagingHosts"),
        (System::Linux, Browser::Edge) => places.home.join(".config/microsoft-edge/NativeMessagingHosts"),
        (System::Linux, Browser::Brave) => places.home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
        (System::Linux, Browser::Firefox) => places.home.join(".mozilla/native-messaging-hosts"),
        (System::MacOs, Browser::Chrome) => support.join("Google/Chrome/NativeMessagingHosts"),
        (System::MacOs, Browser::Chromium) => support.join("Chromium/NativeMessagingHosts"),
        (System::MacOs, Browser::Edge) => support.join("Microsoft Edge/NativeMessagingHosts"),
        (System::MacOs, Browser::Brave) => support.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
        (System::MacOs, Browser::Firefox) => support.join("Mozilla/NativeMessagingHosts"),
        // One file for the Chromium browsers, one for Firefox: their keys point at them.
        (System::Windows, Browser::Firefox) => return places.local_data.join("Lockra/browser").join(format!("{HOST_NAME}.firefox.json")),
        (System::Windows, _) => return places.local_data.join("Lockra/browser").join(format!("{HOST_NAME}.chromium.json")),
    };
    dir.join(file)
}

/// The registry key (under HKCU) whose default value is the manifest's path, on Windows.
pub fn registry_key(browser: Browser) -> String {
    let vendor = match browser {
        // Brave reads Chrome's key.
        Browser::Chrome | Browser::Brave => r"Software\Google\Chrome",
        Browser::Chromium => r"Software\Chromium",
        Browser::Edge => r"Software\Microsoft\Edge",
        Browser::Firefox => r"Software\Mozilla",
    };
    format!(r"{vendor}\NativeMessagingHosts\{HOST_NAME}")
}

/// The folder a browser keeps its profiles in: there when it has been used on this account.
pub fn profile_dir(system: System, browser: Browser, places: &Places) -> PathBuf {
    let support = places.home.join("Library/Application Support");
    match (system, browser) {
        (System::Linux, Browser::Chrome) => places.home.join(".config/google-chrome"),
        (System::Linux, Browser::Chromium) => places.home.join(".config/chromium"),
        (System::Linux, Browser::Edge) => places.home.join(".config/microsoft-edge"),
        (System::Linux, Browser::Brave) => places.home.join(".config/BraveSoftware/Brave-Browser"),
        (System::Linux, Browser::Firefox) => places.home.join(".mozilla"),
        (System::MacOs, Browser::Chrome) => support.join("Google/Chrome"),
        (System::MacOs, Browser::Chromium) => support.join("Chromium"),
        (System::MacOs, Browser::Edge) => support.join("Microsoft Edge"),
        (System::MacOs, Browser::Brave) => support.join("BraveSoftware/Brave-Browser"),
        (System::MacOs, Browser::Firefox) => support.join("Firefox"),
        (System::Windows, Browser::Chrome) => places.local_data.join(r"Google\Chrome\User Data"),
        (System::Windows, Browser::Chromium) => places.local_data.join(r"Chromium\User Data"),
        (System::Windows, Browser::Edge) => places.local_data.join(r"Microsoft\Edge\User Data"),
        (System::Windows, Browser::Brave) => places.local_data.join(r"BraveSoftware\Brave-Browser\User Data"),
        (System::Windows, Browser::Firefox) => places.roaming_data.join(r"Mozilla\Firefox"),
    }
}

/// What Settings › 浏览器 shows of a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrowserStatus {
    pub browser: Browser,
    /// Its profiles are on this account.
    pub installed: bool,
    /// Lockra's manifest is in place for it.
    pub manifest: bool,
}

/// Writing and removing the manifests of one system.
pub struct Manifests {
    pub system: System,
    pub places: Places,
    /// The program the manifests point at.
    pub program: PathBuf,
    pub debug: bool,
    /// The registry's root for the keys (`Software` by default; the tests use their own).
    #[cfg(windows)]
    pub registry_root: Option<String>,
}

impl Manifests {
    /// This machine's.
    pub fn current() -> io::Result<Self> {
        let debug = cfg!(debug_assertions);
        let places = Places::current(debug).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
        Ok(Self {
            system: System::current(),
            places,
            program: super::program()?,
            debug,
            #[cfg(windows)]
            registry_root: None,
        })
    }

    fn path(&self, browser: Browser) -> PathBuf {
        manifest_path(self.system, browser, &self.places)
    }

    /// Write `browser`'s manifest (and its registry key on Windows).
    pub fn write(&self, browser: Browser) -> io::Result<()> {
        let path = self.path(browser);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&manifest(browser, &self.program, self.debug)).map_err(io::Error::other)?;
        std::fs::write(&path, json)?;
        #[cfg(windows)]
        self.set_key(browser, Some(&path))?;
        Ok(())
    }

    /// Remove `browser`'s manifest (and its key); one not there is fine.
    pub fn remove(&self, browser: Browser) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.set_key(browser, None)?;
            // The file is shared by the Chromium browsers: it goes with the last key.
            if Browser::ALL.iter().any(|&other| other != browser && self.path(other) == self.path(browser) && self.key_set(other)) {
                return Ok(());
            }
        }
        match std::fs::remove_file(self.path(browser)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// Whether `browser`'s manifest is in place.
    pub fn written(&self, browser: Browser) -> bool {
        #[cfg(windows)]
        if !self.key_set(browser) {
            return false;
        }
        self.path(browser).is_file()
    }

    /// Every browser's status.
    pub fn statuses(&self) -> Vec<BrowserStatus> {
        Browser::ALL
            .into_iter()
            .map(|browser| BrowserStatus { browser, installed: profile_dir(self.system, browser, &self.places).is_dir(), manifest: self.written(browser) })
            .collect()
    }

    /// Write again the manifests that are in place: the program may have moved.
    pub fn refresh(&self) -> io::Result<()> {
        for browser in Browser::ALL {
            if self.written(browser) {
                self.write(browser)?;
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn key(&self, browser: Browser) -> String {
        let key = registry_key(browser);
        match &self.registry_root {
            Some(root) => key.replacen("Software", root, 1),
            None => key,
        }
    }

    #[cfg(windows)]
    fn set_key(&self, browser: Browser, manifest: Option<&Path>) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let user = RegKey::predef(HKEY_CURRENT_USER);
        match manifest {
            Some(path) => {
                let (key, _) = user.create_subkey(self.key(browser))?;
                key.set_value("", &path.to_string_lossy().into_owned())
            }
            None => match user.delete_subkey(self.key(browser)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            },
        }
    }

    #[cfg(windows)]
    fn key_set(&self, browser: Browser) -> bool {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(self.key(browser)).and_then(|key| key.get_value::<String, _>("")).is_ok()
    }
}

#[cfg(test)]
mod tests;
