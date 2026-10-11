//! The in-app update through tauri-plugin-updater: the core's [`Updater`] port (docs/security.md,
//! "Updates").
//!
//! The plugin reads `plugins.updater` from tauri.conf.json: the release manifest's address and the
//! minisign public key. It asks the manifest, picks the package for the way this copy was installed
//! (`{os}-{arch}-{installer}`, then `{os}-{arch}`), downloads it and checks its signature, and the
//! version the signature was made for (`requireSignedVersion`), before anything is installed.
//! Nothing here runs unless the core asks. Failures are logged with their whole cause chain and
//! reach the core as an [`UpdateFailure`]: the webview only ever sees a code.

use std::time::Duration;

use lockra_core::ports::{Release, UpdateFailure, UpdateFuture, UpdateProgress, Updater};
use lockra_core::ui::InstallMethod;
use parking_lot::Mutex;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Error as PluginError, Update, UpdaterExt as _};

/// How long opening a connection (TCP and TLS) to the update host may take, for the manifest and
/// the package alike. The plugin sets none, which leaves a check hanging for as long as the
/// operating system lets a connection attempt run.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long one read may stall before the step fails.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How this copy was installed: the bundle type the bundler patched into the executable, or on
/// macOS the `.app` it runs from. `None` for an executable run from the build tree, which has
/// nothing to update and must not replace itself.
pub fn install_method() -> Option<InstallMethod> {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().ok()?;
        exe.ancestors().any(|dir| dir.extension().is_some_and(|ext| ext == "app")).then_some(InstallMethod::App)
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::utils::config::BundleType;
        use tauri::utils::platform::bundle_type;
        match bundle_type()? {
            BundleType::Deb => Some(InstallMethod::Deb),
            BundleType::Rpm => Some(InstallMethod::Rpm),
            BundleType::AppImage => Some(InstallMethod::Appimage),
            BundleType::Nsis => Some(InstallMethod::Nsis),
            BundleType::Msi => Some(InstallMethod::Msi),
            _ => None,
        }
    }
}

/// How the Windows installer of an update runs (`plugins.updater.windows.installMode`), by how
/// this copy was installed. The NSIS installer installs for the current user without
/// administrator rights, so it runs silently (`/S /UPDATE /R`): no installer window comes between
/// the update dialog and the new version, which the installer starts itself. Its passive mode shows
/// its own progress page instead. The MSI installs for the whole computer and needs the
/// administrator prompt, which silent mode cannot show: it stays passive, as does anything else.
pub fn windows_install_mode(bundle: Option<tauri::utils::config::BundleType>) -> &'static str {
    match bundle {
        Some(tauri::utils::config::BundleType::Nsis) => "quiet",
        _ => "passive",
    }
}

/// Put `mode` in the updater plugin's configuration before the app runs: the plugin takes its
/// install mode from there and offers no way to change it later. Nothing to do without an updater
/// block.
pub fn set_windows_install_mode(plugins: &mut tauri::utils::config::PluginConfig, mode: &str) {
    let Some(serde_json::Value::Object(updater)) = plugins.0.get_mut("updater") else { return };
    let windows = updater.entry("windows").or_insert_with(|| serde_json::json!({}));
    if let serde_json::Value::Object(windows) = windows {
        windows.insert("installMode".to_owned(), serde_json::Value::from(mode));
    }
}

/// The update port on tauri-plugin-updater: the release `check` found and the package `download`
/// verified stay here until `install`.
pub struct PluginUpdater<R: Runtime> {
    app: AppHandle<R>,
    method: Option<InstallMethod>,
    found: Mutex<Option<Update>>,
    package: Mutex<Option<Vec<u8>>>,
}

impl<R: Runtime> PluginUpdater<R> {
    /// The port for `app`, whose context carries `plugins.updater`; `method` is how this copy was
    /// installed ([`install_method`]).
    pub fn new(app: AppHandle<R>, method: Option<InstallMethod>) -> Self {
        Self { app, method, found: Mutex::new(None), package: Mutex::new(None) }
    }
}

impl<R: Runtime> Updater for PluginUpdater<R> {
    fn method(&self) -> Option<InstallMethod> {
        self.method
    }

    fn check(&self) -> UpdateFuture<'_, Option<Release>> {
        Box::pin(async move {
            // The client settings travel with the `Update` the check returns, so they hold for the
            // package download too.
            let updater = self
                .app
                .updater_builder()
                .configure_client(|client| client.connect_timeout(CONNECT_TIMEOUT).read_timeout(READ_TIMEOUT))
                .build()
                .map_err(|e| failure("set up", &e))?;
            let found = updater.check().await.map_err(|e| failure("check", &e))?;
            let release = found.as_ref().map(|update| Release {
                version: update.version.clone(),
                notes: update.body.clone(),
                date: update.raw_json.get("pub_date").and_then(serde_json::Value::as_str).map(str::to_owned),
            });
            match &release {
                Some(release) => tracing::info!(version = %release.version, "update available"),
                None => tracing::info!("no update available"),
            }
            *self.package.lock() = None;
            *self.found.lock() = found;
            Ok(release)
        })
    }

    fn download(&self, mut progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            let update = self.found.lock().clone().ok_or(UpdateFailure::Invalid)?;
            let mut received = 0u64;
            let bytes = update
                .download(
                    |chunk, total| {
                        received = received.saturating_add(u64::try_from(chunk).unwrap_or(u64::MAX));
                        progress(received, total);
                    },
                    || {},
                )
                .await
                .map_err(|e| failure("download", &e))?;
            tracing::info!(version = %update.version, bytes = bytes.len(), "update downloaded and its signature verified");
            *self.package.lock() = Some(bytes);
            Ok(())
        })
    }

    fn install(&self) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            let update = self.found.lock().clone().ok_or(UpdateFailure::Install)?;
            let package = self.package.lock().take().ok_or(UpdateFailure::Install)?;
            // Installing writes files and may wait for an administrator prompt (pkexec for a .deb or
            // an .rpm): off the async runtime. On Windows the installer takes over and the process
            // exits inside `install`.
            let installed = tauri::async_runtime::spawn_blocking(move || prepare_then_install(package, hand_over, |package| update.install(package)))
                .await
                .map_err(|_| UpdateFailure::Install)?;
            match installed {
                Ok(()) => {}
                Err(Installing::Handoff(reason)) => {
                    tracing::error!(%reason, "the keychain hand-over failed; the update was not installed");
                    return Err(UpdateFailure::Keychain);
                }
                Err(Installing::Install(error)) => return Err(failure("install", &error)),
            }
            tracing::info!("update installed; restarting");
            self.app.request_restart();
            Ok(())
        })
    }
}

/// Why [`prepare_then_install`] installed nothing, or failed installing.
#[derive(Debug)]
enum Installing<E> {
    /// The hand-over before installation failed: nothing was installed.
    Handoff(String),
    /// The installation itself failed.
    Install(E),
}

/// Install `package` only once `hand_over` has succeeded: the staged new build holds this build's
/// keychain entries in items of its own (macOS; docs/security.md, "Keychain items across
/// updates"). A failed hand-over installs nothing, rather than a version that would ask for the
/// keychain on its first start.
fn prepare_then_install<E>(
    package: Vec<u8>,
    hand_over: impl FnOnce(&[u8]) -> Result<(), String>,
    install: impl FnOnce(Vec<u8>) -> Result<(), E>,
) -> Result<(), Installing<E>> {
    hand_over(&package).map_err(Installing::Handoff)?;
    install(package).map_err(Installing::Install)
}

/// macOS: hand this build's keychain entries to the staged new build (keychain_handoff.rs).
#[cfg(target_os = "macos")]
fn hand_over(package: &[u8]) -> Result<(), String> {
    match crate::keychain_handoff::prepare_update(package)? {
        0 => tracing::info!("nothing in the keychain to hand over"),
        entries => tracing::info!(entries, "the staged update stored the keychain hand-over"),
    }
    Ok(())
}

/// Elsewhere the keychain does not tie an item to the build that made it: nothing to hand over.
#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps, reason = "the macOS hand-over can fail")]
fn hand_over(_package: &[u8]) -> Result<(), String> {
    Ok(())
}

/// Log a failed step with every cause, and classify it.
fn failure(step: &str, error: &PluginError) -> UpdateFailure {
    tracing::warn!(step, error = %describe(error), "update step failed");
    classify(error)
}

/// `error` followed by its causes: the first line alone rarely says why a request failed.
pub fn describe(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

/// Which kind of failure the plugin's error is.
pub fn classify(error: &PluginError) -> UpdateFailure {
    use PluginError as E;
    match error {
        E::Reqwest(e) if e.is_decode() => UpdateFailure::Invalid,
        E::Reqwest(_) | E::Network(_) | E::Http(_) | E::InvalidHeaderValue(_) | E::InvalidHeaderName(_) => UpdateFailure::Network,
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) | E::SignedVersionMismatch { .. } | E::MissingSignedVersion => UpdateFailure::Signature,
        E::AuthenticationFailed => UpdateFailure::Cancelled,
        E::Io(_)
        | E::FailedToDetermineExtractPath
        | E::TempDirNotOnSameMountPoint
        | E::BinaryNotFoundInArchive
        | E::TempDirNotFound
        | E::DebInstallFailed
        | E::PackageInstallFailed
        | E::InvalidUpdaterFormat
        | E::Tauri(_) => UpdateFailure::Install,
        // No manifest (any status but success: `ReleaseNotFound`), no package for this computer, a
        // manifest that does not parse, or a configuration the plugin refuses.
        _ => UpdateFailure::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_classified_by_what_the_user_can_do() {
        assert_eq!(classify(&PluginError::Network("timed out".into())), UpdateFailure::Network);
        assert_eq!(classify(&PluginError::ReleaseNotFound), UpdateFailure::Invalid);
        assert_eq!(classify(&PluginError::TargetsNotFound(vec!["linux-x86_64-deb".into(), "linux-x86_64".into()])), UpdateFailure::Invalid);
        assert_eq!(classify(&PluginError::SignatureUtf8("x".into())), UpdateFailure::Signature);
        assert_eq!(classify(&PluginError::SignedVersionMismatch { signed: "0.1.0".into(), announced: "0.2.0".into() }), UpdateFailure::Signature);
        assert_eq!(classify(&PluginError::MissingSignedVersion), UpdateFailure::Signature);
        assert_eq!(classify(&PluginError::AuthenticationFailed), UpdateFailure::Cancelled);
        assert_eq!(classify(&PluginError::InvalidUpdaterFormat), UpdateFailure::Install);
        assert_eq!(classify(&PluginError::Io(std::io::Error::other("read-only"))), UpdateFailure::Install);
        assert_eq!(classify(&PluginError::EmptyEndpoints), UpdateFailure::Invalid);
    }

    /// An error with an optional cause, standing in for reqwest's and hyper's chain.
    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);

    impl std::fmt::Display for Layer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Layer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|cause| cause as &(dyn std::error::Error + 'static))
        }
    }

    #[test]
    fn the_log_keeps_every_cause() {
        let chain = Layer("error sending request", Some(Box::new(Layer("client error (Connect)", Some(Box::new(Layer("dns error", None)))))));
        assert_eq!(describe(&chain), "error sending request: client error (Connect): dns error");
        let repeated = Layer("operation timed out: deadline", Some(Box::new(Layer("deadline", Some(Box::new(Layer("", None)))))));
        assert_eq!(describe(&repeated), "operation timed out: deadline");
    }

    /// Regression (user report 2026-10-05: macOS asked for the keychain after an update): a
    /// hand-over that fails installs nothing, so the running version keeps working and its
    /// keychain items, and no new version starts without them.
    #[test]
    fn regression_a_failed_hand_over_installs_nothing() {
        let installs = std::cell::Cell::new(0);
        let outcome = prepare_then_install(
            vec![7],
            |_| Err("the staged build exited with exit status: 1".to_owned()),
            |_| {
                installs.set(installs.get() + 1);
                Ok::<(), PluginError>(())
            },
        );
        assert!(matches!(outcome, Err(Installing::Handoff(ref reason)) if reason.contains("exit status: 1")));
        assert_eq!(installs.get(), 0);
    }

    #[test]
    fn the_package_installs_once_the_hand_over_is_done() {
        let order = std::cell::RefCell::new(Vec::new());
        let outcome = prepare_then_install(
            vec![7],
            |package| {
                assert_eq!(package, [7]);
                order.borrow_mut().push("hand over");
                Ok(())
            },
            |package| {
                assert_eq!(package, [7]);
                order.borrow_mut().push("install");
                Ok::<(), PluginError>(())
            },
        );
        assert!(outcome.is_ok());
        assert_eq!(*order.borrow(), ["hand over", "install"]);
        // An install that fails after the hand-over is the plugin's failure.
        let failed = prepare_then_install(vec![7], |_| Ok(()), |_| Err(PluginError::InvalidUpdaterFormat));
        assert!(matches!(failed, Err(Installing::Install(PluginError::InvalidUpdaterFormat))));
    }

    /// Regression (user report 2026-10-06: after an in-app update on Windows the installer's own
    /// window came up before Lockra opened again): the NSIS installer of an update runs silently;
    /// the MSI keeps its passive mode, which can show the administrator prompt it needs.
    #[test]
    fn regression_a_windows_update_installs_without_a_window_where_it_can() {
        use tauri::utils::config::BundleType;
        assert_eq!(windows_install_mode(Some(BundleType::Nsis)), "quiet");
        assert_eq!(windows_install_mode(Some(BundleType::Msi)), "passive");
        assert_eq!(windows_install_mode(None), "passive");
    }

    #[test]
    fn the_install_mode_reaches_the_plugin_s_own_configuration() {
        let mut plugins = tauri::utils::config::PluginConfig(
            [(
                "updater".to_owned(),
                serde_json::json!({"pubkey": "key", "endpoints": ["https://example.com/latest.json"], "windows": {"installMode": "passive"}}),
            )]
            .into(),
        );
        set_windows_install_mode(&mut plugins, "quiet");
        let config: tauri_plugin_updater::Config = serde_json::from_value(plugins.0["updater"].clone()).unwrap();
        assert_eq!(config.windows.map(|w| w.install_mode.to_string()).as_deref(), Some("quiet"));
        assert_eq!(config.pubkey, "key");
        // A block without `windows` gets one; no updater block, nothing to set.
        let mut bare = tauri::utils::config::PluginConfig([("updater".to_owned(), serde_json::json!({"pubkey": "key", "endpoints": []}))].into());
        set_windows_install_mode(&mut bare, "quiet");
        assert_eq!(bare.0["updater"]["windows"]["installMode"], "quiet");
        let mut none = tauri::utils::config::PluginConfig::default();
        set_windows_install_mode(&mut none, "quiet");
        assert!(none.0.is_empty());
    }

    #[test]
    fn a_build_run_from_the_tree_cannot_update_itself() {
        // `cargo test` runs an executable no bundler touched.
        assert_eq!(install_method(), None);
    }
}
