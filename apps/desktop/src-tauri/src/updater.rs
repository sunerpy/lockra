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
            tauri::async_runtime::spawn_blocking(move || {
                // macOS: the staged new build takes this build's keychain entries first, so it
                // starts without asking for them (keychain_handoff.rs). Without the hand-over the
                // update still installs; the new build then asks once.
                #[cfg(target_os = "macos")]
                match crate::keychain_handoff::prepare_update(&package) {
                    Ok(0) => {}
                    Ok(entries) => tracing::info!(entries, "the staged update stored the keychain hand-over"),
                    Err(error) => tracing::warn!(%error, "no keychain hand-over before the update; the new version asks for the keychain once"),
                }
                update.install(package)
            })
            .await
            .map_err(|_| UpdateFailure::Install)?
            .map_err(|e| failure("install", &e))?;
            tracing::info!("update installed; restarting");
            self.app.request_restart();
            Ok(())
        })
    }
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

    #[test]
    fn a_build_run_from_the_tree_cannot_update_itself() {
        // `cargo test` runs an executable no bundler touched.
        assert_eq!(install_method(), None);
    }
}
