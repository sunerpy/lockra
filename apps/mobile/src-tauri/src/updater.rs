//! Checking for a newer release on the phone (Settings › About › Check for updates): the release
//! manifest the desktop's updater reads (`latest.json` on GitHub), asked only when the user taps,
//! through lockra-remote's HTTPS client (on Android, the certificate authorities the system keeps).
//! The phone installs nothing itself: a newer release opens its page (`update_open_release`), and a
//! copy from Google Play updates through the store. The core refuses to download or install for
//! [`InstallMethod::Android`], and never checks on its own for it.

use lockra_core::ports::{Release, UpdateFailure, UpdateFuture, UpdateProgress, Updater};
use lockra_core::ui::{InstallMethod, UpdateStatus};
use serde::Deserialize;

/// The release manifest: the newest published release's (`plugins.updater.endpoints` of the
/// desktop's tauri.conf.json).
pub const MANIFEST_URL: &str = "https://github.com/sunerpy/lockra/releases/latest/download/latest.json";
/// The releases page; a release's own page is `…/tag/v<version>`.
pub const RELEASES_URL: &str = "https://github.com/sunerpy/lockra/releases";
/// The most of a manifest that is read: a few kilobytes with the release notes.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// The phone's update source.
pub struct PhoneUpdater {
    current: semver::Version,
    manifest: String,
}

impl PhoneUpdater {
    /// The source for this copy, `current` being its version.
    pub fn new(current: &str) -> Self {
        Self::with_manifest(current, MANIFEST_URL)
    }

    /// The source reading the manifest at `manifest` (the tests' server on this computer). A
    /// version that does not read (none in a build) counts as older than any release.
    pub fn with_manifest(current: &str, manifest: &str) -> Self {
        Self { current: semver::Version::parse(current).unwrap_or(semver::Version::new(0, 0, 0)), manifest: manifest.to_owned() }
    }
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
        Some(InstallMethod::Android)
    }

    fn check(&self) -> UpdateFuture<'_, Option<Release>> {
        Box::pin(async move {
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

/// The page of the release a check found, else the newest release's.
pub fn release_page(status: &UpdateStatus) -> String {
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

    #[test]
    fn the_page_is_the_release_found_or_the_newest() {
        let found = UpdateStatus::Available { version: "0.7.0".into(), notes: None, date: None, checked_at_ms: 1 };
        assert_eq!(release_page(&found), "https://github.com/sunerpy/lockra/releases/tag/v0.7.0");
        assert_eq!(release_page(&UpdateStatus::Idle), "https://github.com/sunerpy/lockra/releases/latest");
        assert_eq!(release_page(&UpdateStatus::UpToDate { checked_at_ms: 1 }), "https://github.com/sunerpy/lockra/releases/latest");
    }
}
