//! The login keychain as [`Keychain`] (macOS), through security-framework's safe calls.
//!
//! Every call names the default keychain explicitly: lookups through the implicit search list find
//! nothing on GitHub's Macs, even for the item's own creator (Voltip, runs 36722230660 and
//! 36725475355). Listing (attributes only) and removing (by reference, never reading the value) do
//! not need the item's partition, so neither ever asks (same runs, both kinds of Mac). Reads try
//! quietly first, with the dialog turned off, so the dialog only comes for an item another build
//! made, and only where the caller allows it.

use parking_lot::Mutex;
use security_framework::base::Error as SecError;
use security_framework::item::{ItemClass, ItemSearchOptions, Limit, Reference, SearchResult};
use security_framework::os::macos::keychain::SecKeychain;
use zeroize::Zeroizing;

use crate::per_build::{Ask, Keychain, Read, Stored};
use lockra_core::ports::PortError;

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;
/// `errSecAuthFailed`: another build's item with the dialog turned off (the partition check).
const AUTH_FAILED: i32 = -25293;
/// `errSecInteractionNotAllowed`.
const INTERACTION_NOT_ALLOWED: i32 = -25308;

/// The user's default keychain (the login keychain).
pub struct LoginKeychain {
    keychain: SecKeychain,
    /// Never show the dialog, not even where the caller allows it (the CI harness).
    quiet: bool,
    /// One call at a time: turning the dialog off is process-wide.
    lock: Mutex<()>,
}

impl std::fmt::Debug for LoginKeychain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginKeychain").finish_non_exhaustive()
    }
}

impl LoginKeychain {
    /// The user's default keychain; `quiet` never shows the dialog.
    pub fn open(quiet: bool) -> Result<Self, PortError> {
        Ok(Self { keychain: SecKeychain::default().map_err(unavailable)?, quiet, lock: Mutex::new(()) })
    }

    fn search(&self) -> ItemSearchOptions {
        let mut options = ItemSearchOptions::new();
        options.class(ItemClass::generic_password()).keychains(std::slice::from_ref(&self.keychain)).limit(Limit::All);
        options
    }

    fn list(&self, options: &ItemSearchOptions) -> Result<Vec<Stored>, PortError> {
        let found = match options.search() {
            Ok(found) => found,
            Err(e) if e.code() == NOT_FOUND => return Ok(Vec::new()),
            Err(e) => return Err(unavailable(e)),
        };
        Ok(found
            .iter()
            .filter_map(SearchResult::simplify_dict)
            .filter_map(|attributes| {
                Some(Stored {
                    service: attributes.get("svce")?.clone(),
                    account: attributes.get("acct")?.clone(),
                    created: attributes.get("cdat").cloned().unwrap_or_default(),
                })
            })
            .collect())
    }
}

fn unavailable(e: SecError) -> PortError {
    PortError(e.to_string())
}

fn text(bytes: &[u8]) -> Result<Zeroizing<String>, PortError> {
    String::from_utf8(bytes.to_vec()).map(Zeroizing::new).map_err(|_| PortError("a keychain item is not text".into()))
}

impl Keychain for LoginKeychain {
    fn items(&self, service: &str) -> Result<Vec<Stored>, PortError> {
        let _one = self.lock.lock();
        let mut options = self.search();
        options.service(service).load_attributes(true);
        self.list(&options)
    }

    fn items_of(&self, account: &str) -> Result<Vec<Stored>, PortError> {
        let _one = self.lock.lock();
        let mut options = self.search();
        options.account(account).load_attributes(true);
        self.list(&options)
    }

    fn read(&self, service: &str, account: &str, ask: Ask) -> Result<Read, PortError> {
        let _one = self.lock.lock();
        // Quietly first: an item this build may read, or no item at all, never needs the dialog.
        let quiet = {
            let _quiet = SecKeychain::disable_user_interaction().map_err(unavailable)?;
            self.keychain.find_generic_password(service, account)
        };
        match quiet {
            Ok((password, _item)) => return Ok(Read::Found(text(&password)?)),
            Err(e) if e.code() == NOT_FOUND => return Ok(Read::Missing),
            Err(e) if matches!(e.code(), AUTH_FAILED | INTERACTION_NOT_ALLOWED) => {
                if ask == Ask::Never || self.quiet {
                    return Ok(Read::WouldAsk);
                }
            }
            Err(e) => return Err(unavailable(e)),
        }
        tracing::info!(service, "reading the keychain item an earlier build stored; macOS asks once");
        match self.keychain.find_generic_password(service, account) {
            Ok((password, _item)) => Ok(Read::Found(text(&password)?)),
            Err(e) if e.code() == NOT_FOUND => Ok(Read::Missing),
            Err(e) => Err(unavailable(e)),
        }
    }

    fn write(&self, service: &str, account: &str, value: &str) -> Result<(), PortError> {
        let _one = self.lock.lock();
        // An item of this build's never needs the dialog; one that would is not overwritten.
        let _quiet = SecKeychain::disable_user_interaction().map_err(unavailable)?;
        self.keychain.set_generic_password(service, account, value.as_bytes()).map_err(unavailable)
    }

    fn remove(&self, service: &str, account: &str) -> Result<(), PortError> {
        let _one = self.lock.lock();
        let _quiet = SecKeychain::disable_user_interaction().map_err(unavailable)?;
        let lookup = || {
            let mut options = self.search();
            options.service(service).account(account).load_refs(true);
            options.search()
        };
        let found = match lookup() {
            Ok(found) => found,
            Err(e) if e.code() == NOT_FOUND => return Ok(()),
            Err(e) => return Err(unavailable(e)),
        };
        for result in found {
            if let SearchResult::Ref(Reference::KeychainItem(item)) = result {
                item.delete();
            }
        }
        // `delete` reports nothing: look again.
        match lookup() {
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Ok(left) if left.is_empty() => Ok(()),
            Ok(_) => Err(PortError(format!("the keychain item {service} could not be removed"))),
            Err(e) => Err(unavailable(e)),
        }
    }
}
