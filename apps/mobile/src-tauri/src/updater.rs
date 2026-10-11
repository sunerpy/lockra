//! Updates on the phone (Settings › About), by who installed this copy (after Voltip's design). A
//! copy from Google Play ([`InstallMethod::Play`]) is updated by Play: the row opens its Play
//! listing and nothing asks GitHub. Any other copy ([`InstallMethod::Android`], a GitHub APK or
//! adb) checks for a newer release when the user taps: the release manifest the desktop's updater
//! reads (`latest.json` on GitHub), through lockra-remote's HTTPS client (on Android, the
//! certificate authorities the system keeps); a newer release opens its page
//! (`update_open_release`). The phone installs nothing itself: the core refuses to download or
//! install for either, never checks on its own, and does not check at all for a Play copy.
//!
//! Who installed the app is asked of `UpdatePlugin.kt` once, after start and off the setup thread
//! (a plugin call waits for the main thread, which setup holds), as Voltip does: until it answers
//! the copy counts as a GitHub one, and once it names Google Play the core republishes its state
//! (`Core::refresh_update`). The environment cannot carry it from `MainActivity.kt`: an arm64 app
//! translated on an x86 device (the CI emulator, a Chromebook) has a libc of its own, which does
//! not see what the Java side set (PR #102).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lockra_core::Core;
use lockra_core::ports::{Release, UpdateFailure, UpdateFuture, UpdateProgress, Updater};
use lockra_core::ui::{InstallMethod, UpdateStatus};
use serde::Deserialize;
use tauri::{AppHandle, Runtime};

/// The release manifest: the newest published release's (`plugins.updater.endpoints` of the
/// desktop's tauri.conf.json).
pub const MANIFEST_URL: &str = "https://github.com/sunerpy/lockra/releases/latest/download/latest.json";
/// The releases page; a release's own page is `…/tag/v<version>`.
pub const RELEASES_URL: &str = "https://github.com/sunerpy/lockra/releases";
/// The most of a manifest that is read: a few kilobytes with the release notes.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// The installer that is Google Play.
pub const PLAY_STORE: &str = "com.android.vending";
/// This app's Google Play listing (`identifier` in tauri.conf.json), which the Play app opens.
pub const PLAY_LISTING: &str = "https://play.google.com/store/apps/details?id=dev.lockra.mobile";

/// The phone's update source.
pub struct PhoneUpdater {
    current: semver::Version,
    manifest: String,
    /// Google Play installed this copy and updates it; until the system says so, a GitHub copy.
    play: AtomicBool,
}

impl PhoneUpdater {
    /// The source for this copy, `current` being its version.
    pub fn new(current: &str) -> Self {
        Self::with_manifest(current, MANIFEST_URL)
    }

    /// The source reading the manifest at `manifest` (the tests' server on this computer). A
    /// version that does not read (none in a build) counts as older than any release.
    pub fn with_manifest(current: &str, manifest: &str) -> Self {
        Self { current: semver::Version::parse(current).unwrap_or(semver::Version::new(0, 0, 0)), manifest: manifest.to_owned(), play: AtomicBool::new(false) }
    }

    /// The package the system says installed this copy; `true` when that changes how it updates
    /// (Google Play's), for the state to go out again.
    pub fn note_installer(&self, installer: Option<&str>) -> bool {
        let play = installer == Some(PLAY_STORE);
        self.play.swap(play, Ordering::SeqCst) != play
    }

    fn play(&self) -> bool {
        self.play.load(Ordering::SeqCst)
    }
}

/// The Android plugin that names the installer (`UpdatePlugin.kt`).
#[cfg(target_os = "android")]
struct InstallSourcePlugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-update")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "UpdatePlugin")?;
                _app.manage(InstallSourcePlugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The installer package the system recorded; `None` when it recorded none, and off Android. It
/// waits for the activity's thread, so it is run off the async runtime and off the setup thread.
fn installer<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager as _;
        let plugin = app.try_state::<InstallSourcePlugin<R>>()?;
        match plugin.0.run_mobile_plugin::<serde_json::Value>("installSource", ()) {
            Ok(answer) => answer.get("installer").and_then(serde_json::Value::as_str).map(str::to_owned),
            Err(error) => {
                tracing::warn!(%error, "install source unknown; looking for releases on GitHub");
                None
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        None
    }
}

/// Ask the system who installed this copy, once, in the background; when it is Google Play, the
/// core republishes its state with the Play row.
pub fn read_installer<R: Runtime>(app: AppHandle<R>, updater: Arc<PhoneUpdater>, core: Core) {
    tauri::async_runtime::spawn(async move {
        let installer = tauri::async_runtime::spawn_blocking(move || installer(&app)).await.ok().flatten();
        tracing::info!(installer = installer.as_deref().unwrap_or("none"), "install source");
        if updater.note_installer(installer.as_deref()) {
            core.refresh_update();
        }
    });
}

/// What the phone reads of the manifest; the desktop's packages under `platforms` are left alone.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
}

/// The manifest's bytes, at most [`MAX_MANIFEST_BYTES`] of them.
async fn fetch(url: &str) -> Result<Vec<u8>, UpdateFailure> {
    let network = |error: &dyn std::fmt::Display| {
        tracing::warn!(%error, "update check: the release manifest did not come");
        UpdateFailure::Network
    };
    let client = lockra_remote::http_client().map_err(|e| network(&e))?;
    let mut response = client.get(url).send().await.map_err(|e| network(&e))?;
    if !response.status().is_success() {
        return Err(network(&response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| network(&e))? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_MANIFEST_BYTES {
            tracing::warn!("update check: the release manifest is larger than any");
            return Err(UpdateFailure::Invalid);
        }
    }
    Ok(body)
}

impl Updater for PhoneUpdater {
    fn method(&self) -> Option<InstallMethod> {
        Some(if self.play() { InstallMethod::Play } else { InstallMethod::Android })
    }

    fn check(&self) -> UpdateFuture<'_, Option<Release>> {
        Box::pin(async move {
            // Not asked for a Play copy (InstallMethod::checks); were it, nothing goes out.
            if self.play() {
                return Ok(None);
            }
            let body = fetch(&self.manifest).await?;
            let manifest: Manifest = serde_json::from_slice(&body).map_err(|_| UpdateFailure::Invalid)?;
            let version = semver::Version::parse(manifest.version.trim().trim_start_matches('v')).map_err(|_| UpdateFailure::Invalid)?;
            if version <= self.current {
                tracing::info!("no update available");
                return Ok(None);
            }
            tracing::info!(%version, "update available");
            Ok(Some(Release { version: version.to_string(), notes: manifest.notes.filter(|n| !n.trim().is_empty()), date: manifest.pub_date }))
        })
    }

    // The core asks for neither on the phone (InstallMethod::installs).
    fn download(&self, _progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async move { Err(UpdateFailure::Install) })
    }

    fn install(&self) -> UpdateFuture<'_, ()> {
        Box::pin(async move { Err(UpdateFailure::Install) })
    }
}

/// The page an update opens: a Play copy's Play listing; else the page of the release a check found,
/// or the newest release's.
pub fn update_page(method: Option<InstallMethod>, status: &UpdateStatus) -> String {
    if method == Some(InstallMethod::Play) {
        return PLAY_LISTING.to_owned();
    }
    match status {
        UpdateStatus::Available { version, .. } => format!("{RELEASES_URL}/tag/v{version}"),
        _ => format!("{RELEASES_URL}/latest"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest server on this computer: every request gets `status` and `body`.
    async fn serve(status: &'static str, body: String) -> String {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let body = body.clone();
                tokio::spawn(async move {
                    let mut request = vec![0u8; 8192];
                    let _ = socket.read(&mut request).await;
                    let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body.as_bytes()).await;
                });
            }
        });
        format!("http://127.0.0.1:{port}/latest.json")
    }

    const MANIFEST: &str = r###"{"version":"0.7.0","notes":"## 0.7.0\n\n- The phone","pub_date":"2026-10-04T08:00:00Z","platforms":{}}"###;

    #[tokio::test]
    async fn a_newer_release_is_found_and_the_same_or_an_older_one_is_not() {
        let url = serve("200 OK", MANIFEST.into()).await;
        let found = PhoneUpdater::with_manifest("0.6.0", &url).check().await.unwrap();
        let release = Release { version: "0.7.0".into(), notes: Some("## 0.7.0\n\n- The phone".into()), date: Some("2026-10-04T08:00:00Z".into()) };
        assert_eq!(found, Some(release));
        assert_eq!(PhoneUpdater::with_manifest("0.7.0", &url).check().await.unwrap(), None);
        assert_eq!(PhoneUpdater::with_manifest("0.8.0", &url).check().await.unwrap(), None);
        // A pre-release of the next version is older than the version itself.
        assert_eq!(PhoneUpdater::with_manifest("0.7.0-rc.1", &url).check().await.unwrap().map(|r| r.version), Some("0.7.0".into()));
    }

    #[tokio::test]
    async fn an_answer_that_does_not_read_or_does_not_come_says_which() {
        let check = |url: String| async move { PhoneUpdater::with_manifest("0.6.0", &url).check().await };
        assert_eq!(check(serve("200 OK", "not json".into()).await).await, Err(UpdateFailure::Invalid));
        assert_eq!(check(serve("200 OK", r#"{"version":"seven"}"#.into()).await).await, Err(UpdateFailure::Invalid));
        // More than any manifest is not read to its end.
        let huge = format!(r#"{{"version":"0.7.0","notes":"{}"}}"#, "x".repeat(2 * MAX_MANIFEST_BYTES));
        assert_eq!(check(serve("200 OK", huge).await).await, Err(UpdateFailure::Invalid));
        assert_eq!(check(serve("404 Not Found", String::new()).await).await, Err(UpdateFailure::Network));
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(check(format!("http://127.0.0.1:{closed}/latest.json")).await, Err(UpdateFailure::Network));
    }

    #[tokio::test]
    async fn the_phone_installs_nothing_itself() {
        let updater = PhoneUpdater::new("0.6.0");
        assert_eq!(updater.method(), Some(InstallMethod::Android));
        assert_eq!(updater.download(Box::new(|_, _| {})).await, Err(UpdateFailure::Install));
        assert_eq!(updater.install().await, Err(UpdateFailure::Install));
    }

    #[tokio::test]
    async fn a_copy_from_google_play_is_updated_by_play_and_any_other_by_its_github_release() {
        // A GitHub copy until the system names Google Play; only that change is news.
        let updater = PhoneUpdater::new("0.8.4");
        assert_eq!(updater.method(), Some(InstallMethod::Android));
        assert!(updater.note_installer(Some("com.android.vending")));
        assert_eq!(updater.method(), Some(InstallMethod::Play));
        assert!(!updater.note_installer(Some("com.android.vending")));
        // The browser or file manager that opened a GitHub APK, adb, nothing on record.
        for installer in [Some("com.android.chrome"), Some("com.google.android.packageinstaller"), Some(""), None] {
            let other = PhoneUpdater::new("0.8.4");
            assert!(!other.note_installer(installer), "{installer:?}");
            assert_eq!(other.method(), Some(InstallMethod::Android), "{installer:?}");
        }
        // Never asked (the core does not check for a Play copy), and even then nothing goes out.
        let url = serve("200 OK", MANIFEST.into()).await;
        let play = PhoneUpdater::with_manifest("0.6.0", &url);
        play.note_installer(Some(PLAY_STORE));
        assert_eq!(play.check().await, Ok(None));
    }

    #[test]
    fn the_page_is_the_play_listing_or_the_release_found_or_the_newest() {
        let found = UpdateStatus::Available { version: "0.7.0".into(), notes: None, date: None, checked_at_ms: 1 };
        let direct = Some(InstallMethod::Android);
        assert_eq!(update_page(direct, &found), "https://github.com/sunerpy/lockra/releases/tag/v0.7.0");
        assert_eq!(update_page(direct, &UpdateStatus::Idle), "https://github.com/sunerpy/lockra/releases/latest");
        assert_eq!(update_page(direct, &UpdateStatus::UpToDate { checked_at_ms: 1 }), "https://github.com/sunerpy/lockra/releases/latest");
        assert_eq!(update_page(Some(InstallMethod::Play), &UpdateStatus::Idle), "https://play.google.com/store/apps/details?id=dev.lockra.mobile");
    }

    #[test]
    fn the_play_listing_names_this_app() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(PLAY_LISTING, format!("https://play.google.com/store/apps/details?id={}", config["identifier"].as_str().unwrap()));
    }
}
