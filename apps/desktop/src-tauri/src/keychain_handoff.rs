//! macOS release builds: their keychain store, and the hand-over an in-app update makes
//! (docs/security.md, "Keychain items across updates").
//!
//! A build signed with the release certificate ([`RELEASE_REQUIREMENT`]) keeps its secrets in
//! keychain items it created ([`PerBuildStore`]), because macOS asks before one build reads an item
//! another build made. Before the updater replaces the running bundle, this build expands the
//! verified update next to it, starts the staged executable with [`HANDOFF_ARG`] and hands over
//! what its store holds ([`crate::handoff`]); the staged build stores and reads back items of its
//! own before acknowledging. Each side first checks that the other process satisfies the release
//! requirement, so the secrets only go from one release to the next. Without the acknowledgement
//! the updater installs nothing (`updater::prepare_then_install`). A build that is not signed that
//! way (local and CI builds are ad hoc) keeps the keyring store and hands nothing over.
//!
//! Everything here is security-framework's safe calls: the requirement is written out from the
//! pinned certificate instead of asked of the system, and the build's name comes from its own code
//! signature ([`crate::code_identity`]).

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use security_framework::os::macos::code_signing::{Flags, GuestAttributes, SecCode, SecRequirement};

use crate::handoff::{self, HANDOFF_ARG, PeerCheck};
use crate::macos_keychain::LoginKeychain;
use crate::per_build::PerBuildStore;

pub use crate::code_identity::RELEASE_REQUIREMENT;

/// The account prefix of the release builds' items: `lockra.signed.<build>`.
const ACCOUNT_USER: &str = "lockra";
/// How long the staged build waits for the hand-over, and this build for its acknowledgement.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The signing a hand-over trusts, and the keychain service its items use. The app has one,
/// [`Release::production`]; the CI harness (`examples/keychain_harness.rs`) makes its own with a
/// certificate of the run.
pub struct Release {
    requirement: SecRequirement,
    service: String,
    /// Never show the keychain's dialog, even for an older build's item (the CI harness: nobody
    /// answers it there).
    quiet: bool,
}

/// The requirement the releases are recognised by and hand over to: [`RELEASE_REQUIREMENT`]. A
/// debug build compiled with `LOCKRA_DEV_RELEASE_REQUIREMENT` takes that one instead, so CI can
/// sign the real app with a certificate of its run and check the hand-over end to end
/// (`.github/scripts/check-keychain-preinstall.sh`). A release build never reads it.
fn trusted_requirement() -> &'static str {
    #[cfg(debug_assertions)]
    if let Some(requirement) = option_env!("LOCKRA_DEV_RELEASE_REQUIREMENT") {
        return requirement;
    }
    RELEASE_REQUIREMENT
}

impl Release {
    /// Lockra's releases: [`RELEASE_REQUIREMENT`] and the service `dev.lockra.desktop`.
    pub fn production() -> Option<Self> {
        Self::new(trusted_requirement(), crate::KEYCHAIN_SERVICE, false)
    }

    /// The signing `requirement` (code requirement language) and the items' `service`.
    pub fn new(requirement: &str, service: &str, quiet: bool) -> Option<Self> {
        Some(Self { requirement: requirement.parse().ok()?, service: service.to_owned(), quiet })
    }

    /// Whether this process is signed so.
    pub fn signed(&self) -> bool {
        SecCode::for_self(Flags::NONE).and_then(|code| code.check_validity(Flags::NONE, &self.requirement)).is_ok()
    }

    /// This build's store: `None` when this process is not signed so, or its login keychain or
    /// code signature cannot be read.
    pub fn store(&self) -> Option<PerBuildStore<LoginKeychain>> {
        if !self.signed() {
            return None;
        }
        let build = crate::code_identity::build_id_of(&std::env::current_exe().ok()?)?;
        match LoginKeychain::open(self.quiet) {
            Ok(keychain) => Some(PerBuildStore::new(keychain, self.service.clone(), ACCOUNT_USER, build)),
            Err(error) => {
                tracing::warn!(%error, "the login keychain could not be opened");
                None
            }
        }
    }

    /// The staged build's side: take the hand-over from the parent, store it in this build's
    /// items and acknowledge. The number of entries stored.
    pub fn take_handoff(&self) -> Result<usize, String> {
        if !self.signed() {
            return Err("this build is not signed for the hand-over".into());
        }
        let pending = handoff::take_pending_from_stdin(&Signed(&self.requirement), TIMEOUT).map_err(|e| e.to_string())?;
        let store = self.store().ok_or("this build has no keychain store of its own")?;
        store.persist_handoff(pending.entries()).map_err(|e| e.0)?;
        let count = pending.entries().len();
        pending.acknowledge().map_err(|e| e.to_string())?;
        Ok(count)
    }

    /// The running build's side: hand what `store` holds to the build `executable`, which stores
    /// it before it exits successfully. The number of entries handed over; `Ok(0)` for none.
    pub fn hand_over_to(&self, store: &PerBuildStore<LoginKeychain>, executable: &Path) -> Result<usize, String> {
        if !self.signed() {
            return Err("this build is not signed for the hand-over".into());
        }
        let entries = store.handoff_entries().map_err(|e| e.0)?;
        if entries.is_empty() {
            return Ok(0);
        }
        let mut child = handoff::start_preinstall(executable, [""; 0], &entries, &Signed(&self.requirement), TIMEOUT).map_err(|e| e.to_string())?;
        let status = child.wait().map_err(|e| format!("the staged build could not finish: {e}"))?;
        if !status.success() {
            return Err(format!("the staged build exited with {status}"));
        }
        Ok(entries.len())
    }
}

/// Trusts a process that satisfies the requirement.
struct Signed<'a>(&'a SecRequirement);

impl PeerCheck for Signed<'_> {
    fn trusted(&self, pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else { return false };
        let mut attributes = GuestAttributes::new();
        attributes.set_pid(pid);
        SecCode::copy_guest_with_attribues(None, &attributes, Flags::NONE).and_then(|code| code.check_validity(Flags::NONE, self.0)).is_ok()
    }
}

static STORE: OnceLock<Option<Arc<PerBuildStore<LoginKeychain>>>> = OnceLock::new();

/// The keychain store of this release build, made once: `None` for a build that is not signed
/// with the release certificate.
pub fn release_store() -> Option<Arc<PerBuildStore<LoginKeychain>>> {
    STORE
        .get_or_init(|| {
            let store = Release::production()?.store()?;
            tracing::info!("release build: keychain items of this build's own");
            Some(Arc::new(store))
        })
        .clone()
}

/// `true` when this process was started to take an update's hand-over.
pub fn asked_for_handoff() -> bool {
    std::env::args_os().nth(1).is_some_and(|arg| arg == HANDOFF_ARG)
}

/// Debug builds only: `--lockra-keychain-probe <entry>` reads one entry from this build's store
/// without ever asking and exits, printing `found <the first 8 bytes of its SHA-256, in hex>`,
/// `none` or why it could not (never the value). How the end-to-end check of the hand-over sees
/// that the installed build reads its item quietly (`.github/scripts/check-keychain-preinstall.sh`).
#[cfg(debug_assertions)]
pub const PROBE_ARG: &str = "--lockra-keychain-probe";

/// The entry [`PROBE_ARG`] names, when this process was started with it.
#[cfg(debug_assertions)]
pub fn asked_for_probe() -> Option<String> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some(PROBE_ARG) { args.next() } else { None }
}

/// Run [`PROBE_ARG`]: the exit status, 0 when the entry was read or is known to be absent.
#[cfg(debug_assertions)]
pub fn probe(entry: &str) -> i32 {
    use lockra_core::ports::SecretStore as _;
    use sha2::{Digest as _, Sha256};
    let Some(store) = Release::new(trusted_requirement(), crate::KEYCHAIN_SERVICE, true).and_then(|release| release.store()) else {
        println!("no store: this build is not signed with the requirement it trusts");
        return 2;
    };
    match store.get(entry) {
        Ok(Some(value)) => {
            let digest = Sha256::digest(value.as_bytes());
            println!("found {}", digest[..8].iter().map(|b| format!("{b:02x}")).collect::<String>());
            0
        }
        Ok(None) => {
            println!("none");
            0
        }
        Err(error) => {
            println!("failed: {}", error.0);
            1
        }
    }
}

/// The staged build's side, run instead of the app when [`asked_for_handoff`]: the exit status,
/// 0 once the entries are stored and acknowledged.
pub fn take_handoff() -> i32 {
    match Release::production().ok_or_else(|| "the release requirement does not parse".to_owned()).and_then(|release| release.take_handoff()) {
        Ok(entries) => {
            tracing::info!(entries, "staged build stored the keychain hand-over");
            0
        }
        Err(error) => {
            tracing::error!(%error, "staged build refused the keychain hand-over");
            1
        }
    }
}

/// Before an update installs: expand the verified package `package` (the `.app.tar.gz` the updater
/// checked) next to the running build and hand this build's keychain entries to the staged
/// executable, which stores them in items of its own. Nothing at the installed path is touched.
/// `Ok(0)` when there is nothing to hand over (not a release build, or no entries); an error
/// leaves the running build as it was.
pub fn prepare_update(package: &[u8]) -> Result<usize, String> {
    let Some(store) = release_store() else { return Ok(0) };
    let release = Release::production().ok_or("the release requirement does not parse")?;
    let stage = tempfile::Builder::new().prefix("lockra-staged-update").tempdir().map_err(|e| e.to_string())?;
    let name = std::env::current_exe().ok().and_then(|exe| exe.file_name().map(ToOwned::to_owned)).ok_or("this build's executable has no name")?;
    let executable = handoff::stage_update(package, stage.path(), &name)?;
    release.hand_over_to(&store, &executable)
}
