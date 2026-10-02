//! Touch ID (macOS) and Windows Hello (Windows) before "remember on this device" unlocks. The
//! platform calls are robius-authentication's, so this crate keeps forbidding unsafe code. Without
//! Windows Hello, robius-authentication asks for the Windows account password instead; Lockra asks
//! `UserConsentVerifier` first and offers no check without Hello, so that prompt comes only when
//! Hello stops being available between its look and robius-authentication's own, an instant later
//! (docs/security.md). Elsewhere there is no check to offer.

use lockra_core::ports::{BiometricError, Biometrics};
use lockra_core::ui::BiometricKind;

/// The platform's check, as the core's port.
#[derive(Debug, Default)]
pub struct PlatformBiometrics {
    /// macOS cannot be asked without prompting: once a check found no Touch ID, this run of the app
    /// no longer offers it.
    #[cfg(target_os = "macos")]
    missing: std::sync::atomic::AtomicBool,
}

#[cfg(any(target_os = "macos", windows))]
impl Biometrics for PlatformBiometrics {
    fn availability(&self) -> Option<BiometricKind> {
        #[cfg(target_os = "macos")]
        {
            (!self.missing.load(std::sync::atomic::Ordering::SeqCst)).then_some(BiometricKind::TouchId)
        }
        #[cfg(windows)]
        {
            windows_hello_ready().then_some(BiometricKind::WindowsHello)
        }
    }

    fn verify(&self, reason: &str) -> Result<(), BiometricError> {
        use robius_authentication::{AndroidText, BiometricStrength, Context, PolicyBuilder, Text, WindowsText};

        // Without Hello, robius-authentication would ask for the account password: no check at all.
        #[cfg(windows)]
        if !windows_hello_ready() {
            return Err(BiometricError::Unavailable);
        }
        // macOS: the fingerprint alone (Lockra's own fallback is the master password). Windows: Hello
        // always allows its PIN, so a policy without it is refused.
        let policy = PolicyBuilder::new()
            .biometrics(Some(BiometricStrength::Strong))
            .password(cfg!(windows))
            .companion(false)
            .build()
            .ok_or(BiometricError::Unavailable)?;
        let text = Text {
            android: AndroidText { title: reason, subtitle: None, description: None },
            apple: reason,
            windows: WindowsText::new_truncated("Lockra", reason),
        };
        let (answer, answered) = std::sync::mpsc::channel();
        Context::new(())
            .authenticate(text, &policy, move |result| {
                let _ = answer.send(result);
            })
            .map_err(|error| self.failure(&error))?;
        match answered.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.failure(&error)),
            Err(_) => Err(BiometricError::Failed("the check gave no answer".into())),
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
impl PlatformBiometrics {
    fn failure(&self, error: &robius_authentication::Error) -> BiometricError {
        use robius_authentication::Error as E;
        match error {
            E::UserCanceled | E::UserFallback | E::SystemCanceled | E::AppCanceled => BiometricError::Cancelled,
            E::Unavailable
            | E::NotEnrolled
            | E::Exhausted
            | E::PasscodeNotSet
            | E::BiometryDisconnected
            | E::NotPaired
            | E::Busy
            | E::DisabledByPolicy
            | E::NotConfigured => {
                #[cfg(target_os = "macos")]
                if matches!(error, E::Unavailable | E::NotEnrolled | E::PasscodeNotSet) {
                    self.missing.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                BiometricError::Unavailable
            }
            other => BiometricError::Failed(format!("{other:?}")),
        }
    }
}

/// Windows Hello is set up for this user (a sensor or a PIN, not turned off by policy).
#[cfg(windows)]
fn windows_hello_ready() -> bool {
    use windows::Security::Credentials::UI::{UserConsentVerifier, UserConsentVerifierAvailability};
    UserConsentVerifier::CheckAvailabilityAsync().and_then(|asked| asked.get()).is_ok_and(|answer| answer == UserConsentVerifierAvailability::Available)
}

/// A check that passes at once, for a debug build without a sensor (`LOCKRA_DEV_BIOMETRIC`).
#[derive(Debug)]
pub struct StandIn(pub BiometricKind);

impl Biometrics for StandIn {
    fn availability(&self) -> Option<BiometricKind> {
        Some(self.0)
    }

    fn verify(&self, _reason: &str) -> Result<(), BiometricError> {
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
impl Biometrics for PlatformBiometrics {
    fn availability(&self) -> Option<BiometricKind> {
        None
    }

    fn verify(&self, _reason: &str) -> Result<(), BiometricError> {
        Err(BiometricError::Unavailable)
    }
}
