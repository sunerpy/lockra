//! The fingerprint on Android, through `BiometricPlugin.kt`: the check before the remembered key is
//! used (the core's `Biometrics` port) and the store of that key (`SecretStore`), encrypted by a
//! key of the phone's secure hardware (Android Keystore) that works for ten seconds after a passed
//! check and never again once a new fingerprint is enrolled. The core checks first and reads or
//! writes the key right after, so one prompt does both. Other builds of this crate (the host
//! tests) have neither and say so.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lockra_core::ports::{BiometricError, Biometrics, KeychainStatus, PortError, SecretStore};
use lockra_core::ui::BiometricKind;
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

/// Why there is no fingerprint on this build.
pub const BIOMETRIC_UNAVAILABLE: &str = "biometric: this build has no fingerprint";

/// The Android plugin (`BiometricPlugin.kt`).
#[cfg(target_os = "android")]
struct Plugin<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Registers the Android plugin; a no-op elsewhere.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("lockra-biometric")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("dev.lockra.mobile", "BiometricPlugin")?;
                _app.manage(Plugin(handle));
            }
            Ok(())
        })
        .build()
}

/// The fingerprint check and the key store, one plugin between them. Whether the store can be used
/// is learnt with the fingerprint's availability (the core asks for it at start and at every lock):
/// the state reads the store's status all the time, so it never makes a call of its own.
pub fn ports<R: Runtime>(app: AppHandle<R>) -> (PhoneBiometrics<R>, PhoneSecretStore<R>) {
    let usable = Arc::new(AtomicBool::new(false));
    (PhoneBiometrics { plugin: Bridge { app: app.clone() }, usable: Arc::clone(&usable) }, PhoneSecretStore { plugin: Bridge { app }, usable })
}

struct Bridge<R: Runtime> {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    app: AppHandle<R>,
}

impl<R: Runtime> Bridge<R> {
    /// Run a plugin command; it waits on the activity's thread, so the core calls it off the
    /// runtime (its ports run on the blocking pool), never from the setup.
    fn call(&self, command: &str, args: Value) -> Result<Value, PortError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager as _;
            let plugin = self.app.try_state::<Plugin<R>>().ok_or_else(|| PortError("biometric: plugin missing".into()))?;
            plugin.0.run_mobile_plugin(command, args).map_err(|e| PortError(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (command, args);
            Err(PortError(BIOMETRIC_UNAVAILABLE.into()))
        }
    }
}

/// The fingerprint check.
pub struct PhoneBiometrics<R: Runtime> {
    plugin: Bridge<R>,
    usable: Arc<AtomicBool>,
}

impl<R: Runtime> Biometrics for PhoneBiometrics<R> {
    fn availability(&self) -> Option<BiometricKind> {
        let strong = match self.plugin.call("status", serde_json::json!({})) {
            Ok(answer) => answer.get("strong").and_then(Value::as_bool).unwrap_or(false),
            Err(error) => {
                tracing::debug!(%error, "no fingerprint here");
                false
            }
        };
        // The store's key needs an enrolled strong biometric: the two come and go together.
        self.usable.store(strong, Ordering::SeqCst);
        strong.then_some(BiometricKind::Fingerprint)
    }

    fn verify(&self, reason: &str) -> Result<(), BiometricError> {
        let answer = self.plugin.call("verify", serde_json::json!({ "reason": reason })).map_err(|PortError(why)| BiometricError::Failed(why))?;
        verdict_of(&answer)
    }
}

/// The check's answer: `{ ok }`, `{ cancelled }`, `{ unavailable }` or `{ failed: why }`.
pub fn verdict_of(answer: &Value) -> Result<(), BiometricError> {
    let said = |name: &str| answer.get(name).and_then(Value::as_bool).unwrap_or(false);
    if said("ok") {
        Ok(())
    } else if said("cancelled") {
        Err(BiometricError::Cancelled)
    } else if said("unavailable") {
        Err(BiometricError::Unavailable)
    } else {
        Err(BiometricError::Failed(answer.get("failed").and_then(Value::as_str).unwrap_or("no answer").to_owned()))
    }
}

/// The remembered key's store, sealed by the Keystore key.
pub struct PhoneSecretStore<R: Runtime> {
    plugin: Bridge<R>,
    usable: Arc<AtomicBool>,
}

impl<R: Runtime> SecretStore for PhoneSecretStore<R> {
    fn status(&self) -> KeychainStatus {
        if self.usable.load(Ordering::SeqCst) { KeychainStatus::Available } else { KeychainStatus::Unavailable }
    }

    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        let answer = self.plugin.call("getSecret", serde_json::json!({ "account": account }))?;
        Ok(answer.get("secret").and_then(Value::as_str).map(|s| Zeroizing::new(s.to_owned())))
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), PortError> {
        self.plugin.call("setSecret", serde_json::json!({ "account": account, "secret": secret })).map(|_| ())
    }

    fn delete(&self, account: &str) -> Result<(), PortError> {
        self.plugin.call("deleteSecret", serde_json::json!({ "account": account })).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_s_answer_reads_as_the_core_s() {
        assert_eq!(verdict_of(&serde_json::json!({ "ok": true })), Ok(()));
        assert_eq!(verdict_of(&serde_json::json!({ "cancelled": true })), Err(BiometricError::Cancelled));
        assert_eq!(verdict_of(&serde_json::json!({ "unavailable": true })), Err(BiometricError::Unavailable));
        assert_eq!(verdict_of(&serde_json::json!({ "failed": "too many tries" })), Err(BiometricError::Failed("too many tries".into())));
        assert_eq!(verdict_of(&serde_json::json!({})), Err(BiometricError::Failed("no answer".into())));
    }

    #[test]
    fn without_a_fingerprint_neither_port_offers_anything() {
        let app = tauri::test::mock_app();
        let (biometrics, secrets) = ports(app.handle().clone());
        assert_eq!(biometrics.availability(), None);
        assert_eq!(secrets.status(), KeychainStatus::Unavailable);
        assert!(matches!(biometrics.verify("unlock"), Err(BiometricError::Failed(m)) if m == BIOMETRIC_UNAVAILABLE));
        assert!(matches!(secrets.get("vault"), Err(PortError(m)) if m == BIOMETRIC_UNAVAILABLE));
        assert!(secrets.set("vault", "key").is_err() && secrets.delete("vault").is_err());
    }
}
