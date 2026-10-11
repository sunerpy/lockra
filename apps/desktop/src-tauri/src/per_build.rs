//! A release build on macOS keeps each secret in a keychain item it created itself, one account per
//! build (docs/security.md, "Keychain items across updates").
//!
//! The login keychain gives every item a partition list. Code signed with a certificate that has no
//! Apple Team ID, as Lockra's self-signed release certificate, gets the partition `cdhash:<that
//! build>`, so reading an item another build created shows the system's dialog, and 「始终允许」
//! lets in only the build it was pressed for: every update asked again (Voltip found and measured
//! this on 2026-09-30; its fix is the model here). So each build stores its items under
//! `<user>.signed.<build>`, and an in-app update hands the values to the staged new build before it
//! installs ([`crate::handoff`]): the new build creates its own items and never reads an older one.
//! Without a hand-over (a dmg installed by hand, or the update from a build without this) the
//! newest older copy is read once, which asks once, and moved over; the item 0.7 and earlier made
//! (service `dev.lockra.desktop`, account the vault id) is the oldest copy.
//!
//! The store's logic is the same on every platform, so the tests here run anywhere; the login
//! keychain itself ([`crate::macos_keychain`]) only exists on macOS.

use std::collections::HashMap;

use lockra_core::ports::{KeychainStatus, PortError, SecretStore};
use parking_lot::Mutex;
use zeroize::Zeroizing;

/// The suffix of the accounts a signed build's items use: `<user>.signed.<build>`.
pub const SIGNED_ACCOUNT: &str = "signed";

/// What a keychain store knows: each entry's value, or `None` for an entry known to be absent (a
/// hand-over carries that too, so an older copy is not brought back).
pub type Entries = Vec<(String, Option<Zeroizing<String>>)>;

/// Whether a read may show the system's keychain dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The user may be asked (an older build's item, read once).
    Allowed,
    /// Never ask: an item that would need the dialog reads as [`Read::WouldAsk`].
    Never,
}

/// What reading one item found.
#[derive(Debug)]
pub enum Read {
    /// The value.
    Found(Zeroizing<String>),
    /// No such item.
    Missing,
    /// Another build created the item: reading it needs the dialog.
    WouldAsk,
}

/// One item as a listing shows it (never with its value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// The item's service.
    pub service: String,
    /// The item's account.
    pub account: String,
    /// Creation time as text that sorts in time order (the keychain's
    /// `2026-09-30 13:57:47 +0000`): larger is newer.
    pub created: String,
}

/// The keychain operations [`PerBuildStore`] needs.
pub trait Keychain: Send + Sync {
    /// The items of `service`, without their values (listing never asks).
    fn items(&self, service: &str) -> Result<Vec<Stored>, PortError>;
    /// The items whose account is `account`, without their values (listing never asks).
    fn items_of(&self, account: &str) -> Result<Vec<Stored>, PortError>;
    /// Read one item.
    fn read(&self, service: &str, account: &str, ask: Ask) -> Result<Read, PortError>;
    /// Create or overwrite an item of this build's.
    fn write(&self, service: &str, account: &str, value: &str) -> Result<(), PortError>;
    /// Remove an item without reading it (never asks); a missing one is not an error.
    fn remove(&self, service: &str, account: &str) -> Result<(), PortError>;
}

/// Secrets in items this build created, under `<user>.signed.<build>`; see the module docs.
pub struct PerBuildStore<K> {
    keychain: K,
    service: String,
    user: String,
    build: String,
    /// Each entry's state as this process last read, wrote or removed it.
    known: Mutex<HashMap<String, Option<Zeroizing<String>>>>,
}

impl<K> std::fmt::Debug for PerBuildStore<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerBuildStore").field("service", &self.service).field("build", &self.build).finish_non_exhaustive()
    }
}

impl<K: Keychain> PerBuildStore<K> {
    /// A store on `keychain` for `service` (`dev.lockra.desktop`) and `user`, as build `build`.
    pub fn new(keychain: K, service: impl Into<String>, user: impl Into<String>, build: impl Into<String>) -> Self {
        Self { keychain, service: service.into(), user: user.into(), build: build.into(), known: Mutex::new(HashMap::new()) }
    }

    /// The account of this build's items (`<user>.signed.<build>`).
    pub fn account(&self) -> String {
        self.own_account()
    }

    /// The keychain this store uses.
    pub fn keychain(&self) -> &K {
        &self.keychain
    }

    /// What the next build should be handed: every entry this process read, wrote or removed, and
    /// every other item of this build's, read quietly (never asking). An item of this build's that
    /// cannot be listed or read so is an error: handing over without it would leave the next build
    /// to ask for it.
    pub fn handoff_entries(&self) -> Result<Entries, PortError> {
        let mut all: HashMap<String, Option<Zeroizing<String>>> = self.known.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for item in self.keychain.items_of(&self.own_account())? {
            let Some(entry) = item.service.strip_prefix(&self.prefix()) else { continue };
            if all.contains_key(entry) {
                continue;
            }
            match self.keychain.read(&item.service, &item.account, Ask::Never)? {
                Read::Found(value) => {
                    all.insert(entry.to_owned(), Some(value));
                }
                // Removed since it was listed.
                Read::Missing => {}
                Read::WouldAsk => return Err(PortError(format!("the keychain item {entry} of this build cannot be read without asking"))),
            }
        }
        let mut entries: Entries = all.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }

    /// Store an update's hand-over in this build's own items, leaving the running build's copies
    /// alone. Every value is read back before success. On failure the items that were this build's
    /// before are put back (best effort), so the caller can refuse the hand-over whole.
    pub fn persist_handoff(&self, entries: &Entries) -> Result<(), PortError> {
        let mut before = Vec::with_capacity(entries.len());
        for (entry, _) in entries {
            before.push((entry.clone(), self.read_own(entry)?));
        }
        let apply = || -> Result<(), PortError> {
            for (entry, wanted) in entries {
                let service = self.item_service(entry);
                match wanted {
                    Some(value) => {
                        self.keychain.write(&service, &self.own_account(), value)?;
                        if self.read_own(entry)?.as_deref().map(String::as_str) != Some(value.as_str()) {
                            return Err(PortError(format!("the handed-over keychain item {entry} did not read back")));
                        }
                    }
                    None => {
                        self.keychain.remove(&service, &self.own_account())?;
                        // The running build's word that the entry is gone: older copies go too, or
                        // the installed build would ask for one and bring it back.
                        self.remove_others_strict(entry)?;
                        if self.read_own(entry)?.is_some() {
                            return Err(PortError(format!("the handed-over keychain item {entry} was not removed")));
                        }
                    }
                }
            }
            Ok(())
        };
        if let Err(error) = apply() {
            for (entry, original) in before {
                let service = self.item_service(&entry);
                let restored = match original {
                    Some(value) => self.keychain.write(&service, &self.own_account(), &value),
                    None => self.keychain.remove(&service, &self.own_account()),
                };
                if let Err(rollback) = restored {
                    tracing::warn!(entry, error = %rollback, "a handed-over keychain item could not be rolled back");
                }
            }
            return Err(error);
        }
        let mut known = self.known.lock();
        for (entry, value) in entries {
            known.insert(entry.clone(), value.clone());
        }
        Ok(())
    }

    fn prefix(&self) -> String {
        format!("{}/", self.service)
    }

    fn item_service(&self, entry: &str) -> String {
        format!("{}{entry}", self.prefix())
    }

    fn own_account(&self) -> String {
        format!("{}.{SIGNED_ACCOUNT}.{}", self.user, self.build)
    }

    fn read_own(&self, entry: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        match self.keychain.read(&self.item_service(entry), &self.own_account(), Ask::Never)? {
            Read::Found(value) => Ok(Some(value)),
            Read::Missing => Ok(None),
            // An item under this build's own account that this build did not create.
            Read::WouldAsk => Err(PortError(format!("the keychain item {entry} of this build needs the user's permission"))),
        }
    }

    /// Every other copy of `entry`: other builds' items, newest first, then the item of 0.7 and
    /// earlier (service `dev.lockra.desktop`, account the vault id).
    fn others(&self, entry: &str) -> Vec<(String, String)> {
        match self.listed_others(entry) {
            Ok(mut copies) => {
                copies.push(self.legacy(entry));
                copies
            }
            Err(error) => {
                tracing::warn!(%error, entry, "keychain items could not be listed; looking at the older item only");
                vec![self.legacy(entry)]
            }
        }
    }

    fn legacy(&self, entry: &str) -> (String, String) {
        (self.service.clone(), entry.to_owned())
    }

    fn listed_others(&self, entry: &str) -> Result<Vec<(String, String)>, PortError> {
        let service = self.item_service(entry);
        let builds = format!("{}.{SIGNED_ACCOUNT}.", self.user);
        let own = self.own_account();
        let mut items: Vec<Stored> =
            self.keychain.items(&service)?.into_iter().filter(|item| item.account.starts_with(&builds) && item.account != own).collect();
        items.sort_by(|a, b| b.created.cmp(&a.created));
        Ok(items.into_iter().map(|item| (item.service, item.account)).collect())
    }

    /// Remove every other copy of `entry`, never asking; one that cannot go stays, unused.
    fn remove_others(&self, entry: &str) {
        for (service, account) in self.others(entry) {
            if let Err(error) = self.keychain.remove(&service, &account) {
                tracing::warn!(%error, entry, "an older keychain item could not be removed; it stays, unused");
            }
        }
    }

    fn remove_others_strict(&self, entry: &str) -> Result<(), PortError> {
        for (service, account) in self.listed_others(entry)? {
            self.keychain.remove(&service, &account)?;
        }
        let (service, account) = self.legacy(entry);
        self.keychain.remove(&service, &account)
    }

    fn load(&self, entry: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        if let Some(value) = self.read_own(entry)? {
            // A staged update leaves the running build's copy in place until it is installed:
            // whichever build reads next owns the value and retires the other copies.
            self.remove_others(entry);
            return Ok(Some(value));
        }
        for (service, account) in self.others(entry) {
            let value = match self.keychain.read(&service, &account, Ask::Allowed)? {
                Read::Found(value) => value,
                Read::Missing => continue,
                Read::WouldAsk => return Err(PortError(format!("the keychain item {entry} needs the user's permission"))),
            };
            // Moved: written, read back, and only then the older copies removed.
            self.keychain.write(&self.item_service(entry), &self.own_account(), &value)?;
            if self.read_own(entry)?.as_deref().map(String::as_str) == Some(value.as_str()) {
                tracing::info!(entry, "keychain item moved into this build's own");
                self.remove_others(entry);
            } else {
                tracing::warn!(entry, "the moved keychain item did not read back; the older one stays");
            }
            return Ok(Some(value));
        }
        Ok(None)
    }
}

impl<K: Keychain> SecretStore for PerBuildStore<K> {
    fn status(&self) -> KeychainStatus {
        KeychainStatus::Available
    }

    fn get(&self, entry: &str) -> Result<Option<Zeroizing<String>>, PortError> {
        if let Some(known) = self.known.lock().get(entry) {
            return Ok(known.clone());
        }
        let value = self.load(entry)?;
        self.known.lock().insert(entry.to_owned(), value.clone());
        Ok(value)
    }

    fn set(&self, entry: &str, secret: &str) -> Result<(), PortError> {
        self.keychain.write(&self.item_service(entry), &self.own_account(), secret)?;
        self.known.lock().insert(entry.to_owned(), Some(Zeroizing::new(secret.to_owned())));
        self.remove_others(entry);
        Ok(())
    }

    /// Every copy goes, older builds' too, or the entry would come back from one of them.
    fn delete(&self, entry: &str) -> Result<(), PortError> {
        self.keychain.remove(&self.item_service(entry), &self.own_account())?;
        for (service, account) in self.others(entry) {
            self.keychain.remove(&service, &account)?;
        }
        self.known.lock().insert(entry.to_owned(), None);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use super::*;

    const SVC: &str = "dev.lockra.desktop";
    const VAULT: &str = "6f1c2e3a-0000-4000-8000-000000000001";

    struct Item {
        value: String,
        /// The builds in the item's partition list: its creator, and any the user always allowed.
        partition: BTreeSet<String>,
        created: String,
    }

    /// A login keychain as macOS keeps it: an item another build created is read only by asking
    /// (recorded in `asked`; the user answers 「允许」, which adds nothing to the partition);
    /// listing and removing never ask.
    #[derive(Clone, Default)]
    pub(crate) struct Login {
        items: Arc<Mutex<BTreeMap<(String, String), Item>>>,
        clock: Arc<Mutex<u64>>,
        asked: Arc<Mutex<Vec<String>>>,
    }

    impl Login {
        pub(crate) fn as_build(&self, build: &str) -> Fake {
            Fake { login: self.clone(), build: build.to_owned(), fail_service: None, fail_list: false }
        }
        fn accounts(&self, service: &str) -> Vec<String> {
            self.items.lock().keys().filter(|(s, _)| s == service).map(|(_, a)| a.clone()).collect()
        }
        fn asked(&self) -> Vec<String> {
            self.asked.lock().clone()
        }
    }

    pub(crate) struct Fake {
        login: Login,
        build: String,
        /// Fail the writes to this service.
        fail_service: Option<String>,
        fail_list: bool,
    }

    impl Keychain for Fake {
        fn items(&self, service: &str) -> Result<Vec<Stored>, PortError> {
            if self.fail_list {
                return Err(PortError("listing failed".into()));
            }
            let items = self.login.items.lock();
            Ok(items
                .iter()
                .filter(|((s, _), _)| s == service)
                .map(|((s, a), item)| Stored { service: s.clone(), account: a.clone(), created: item.created.clone() })
                .collect())
        }
        fn items_of(&self, account: &str) -> Result<Vec<Stored>, PortError> {
            if self.fail_list {
                return Err(PortError("listing failed".into()));
            }
            let items = self.login.items.lock();
            Ok(items
                .iter()
                .filter(|((_, a), _)| a == account)
                .map(|((s, a), item)| Stored { service: s.clone(), account: a.clone(), created: item.created.clone() })
                .collect())
        }
        fn read(&self, service: &str, account: &str, ask: Ask) -> Result<Read, PortError> {
            let items = self.login.items.lock();
            let Some(item) = items.get(&(service.to_owned(), account.to_owned())) else { return Ok(Read::Missing) };
            if item.partition.contains(&self.build) {
                return Ok(Read::Found(Zeroizing::new(item.value.clone())));
            }
            match ask {
                Ask::Never => Ok(Read::WouldAsk),
                Ask::Allowed => {
                    self.login.asked.lock().push(format!("{service} {account}"));
                    Ok(Read::Found(Zeroizing::new(item.value.clone())))
                }
            }
        }
        fn write(&self, service: &str, account: &str, value: &str) -> Result<(), PortError> {
            if self.fail_service.as_deref() == Some(service) {
                return Err(PortError("the keychain is locked".into()));
            }
            let mut items = self.login.items.lock();
            let key = (service.to_owned(), account.to_owned());
            if let Some(item) = items.get_mut(&key) {
                if !item.partition.contains(&self.build) {
                    return Err(PortError("another build's item".into()));
                }
                value.clone_into(&mut item.value);
                return Ok(());
            }
            let mut clock = self.login.clock.lock();
            *clock += 1;
            items.insert(key, Item { value: value.to_owned(), partition: BTreeSet::from([self.build.clone()]), created: format!("{:020}", *clock) });
            Ok(())
        }
        fn remove(&self, service: &str, account: &str) -> Result<(), PortError> {
            self.login.items.lock().remove(&(service.to_owned(), account.to_owned()));
            Ok(())
        }
    }

    fn store(login: &Login, build: &str) -> PerBuildStore<Fake> {
        PerBuildStore::new(login.as_build(build), SVC, "mac", build)
    }

    fn get(store: &PerBuildStore<Fake>, entry: &str) -> Option<String> {
        store.get(entry).unwrap().map(|v| v.to_string())
    }

    fn own(build: &str) -> String {
        format!("mac.signed.{build}")
    }

    fn entry_service() -> String {
        format!("{SVC}/{VAULT}")
    }

    /// Regression (user report 2026-10-04: opening Lockra after an update on macOS asked for the
    /// keychain): the build an update installs reads nothing another build created. The staged
    /// build stores what the old one handed over in items of its own, and the installed build reads
    /// them without asking, then retires the old build's copy.
    #[test]
    fn regression_an_update_reads_the_keychain_without_asking() {
        let login = Login::default();
        let old = store(&login, "aaaa");
        old.set(VAULT, "device key").unwrap();
        let handed = old.handoff_entries().unwrap();
        assert_eq!(handed.len(), 1);

        // The staged new build, before installation: its own item, the old one still there.
        let staged = store(&login, "bbbb");
        staged.persist_handoff(&handed).unwrap();
        assert_eq!(login.accounts(&entry_service()), [own("aaaa"), own("bbbb")]);

        // The installed new build.
        let new = store(&login, "bbbb");
        assert_eq!(get(&new, VAULT).as_deref(), Some("device key"));
        assert_eq!(login.asked(), Vec::<String>::new(), "nothing asked");
        assert_eq!(login.accounts(&entry_service()), [own("bbbb")], "the old build's item is gone");
    }

    #[test]
    fn the_hand_over_holds_items_this_process_never_read() {
        let login = Login::default();
        store(&login, "aaaa").set(VAULT, "device key").unwrap();
        // A later start of the same build that unlocked with the password: nothing read yet.
        let entries = store(&login, "aaaa").handoff_entries().unwrap();
        assert_eq!(entries.iter().map(|(e, v)| (e.as_str(), v.as_deref().map(String::as_str))).collect::<Vec<_>>(), [(VAULT, Some("device key"))]);
        assert_eq!(login.asked(), Vec::<String>::new());
    }

    #[test]
    fn without_a_hand_over_the_item_of_0_7_is_read_once_and_moved() {
        let login = Login::default();
        // 0.7 and earlier: the keyring layout, made by an ad-hoc build.
        login.as_build("adhoc").write(SVC, VAULT, "device key").unwrap();
        let new = store(&login, "bbbb");
        assert_eq!(get(&new, VAULT).as_deref(), Some("device key"));
        assert_eq!(login.asked(), [format!("{SVC} {VAULT}")], "asked once");
        assert_eq!(login.accounts(SVC), Vec::<String>::new(), "the 0.7 item is gone");
        // The next start reads its own item.
        assert_eq!(get(&store(&login, "bbbb"), VAULT).as_deref(), Some("device key"));
        assert_eq!(login.asked().len(), 1);
    }

    #[test]
    fn without_a_hand_over_the_newest_other_build_is_read() {
        let login = Login::default();
        store(&login, "aaaa").set(VAULT, "old").unwrap();
        // Two builds without a hand-over between them: the later item wins.
        login.as_build("bbbb").write(&entry_service(), &own("bbbb"), "newer").unwrap();
        let c = store(&login, "cccc");
        assert_eq!(get(&c, VAULT).as_deref(), Some("newer"));
        assert_eq!(login.accounts(&entry_service()), [own("cccc")]);
    }

    #[test]
    fn a_deletion_is_handed_over_and_no_copy_comes_back() {
        let login = Login::default();
        login.as_build("adhoc").write(SVC, VAULT, "stale").unwrap();
        let old = store(&login, "aaaa");
        old.set(VAULT, "key").unwrap();
        old.delete(VAULT).unwrap();
        let handed = old.handoff_entries().unwrap();
        assert_eq!(handed.iter().map(|(e, v)| (e.as_str(), v.is_none())).collect::<Vec<_>>(), [(VAULT, true)]);
        store(&login, "bbbb").persist_handoff(&handed).unwrap();
        assert_eq!(get(&store(&login, "bbbb"), VAULT), None);
        assert_eq!(login.asked(), Vec::<String>::new());
        assert_eq!(login.accounts(SVC), Vec::<String>::new());
    }

    /// Regression (user report 2026-10-05): what the hand-over cannot collect must stop it, not
    /// leave the entry out. A hand-over without it installs a version that asks for the keychain.
    #[test]
    fn regression_a_hand_over_that_cannot_list_this_build_s_items_fails() {
        let login = Login::default();
        store(&login, "aaaa").set(VAULT, "device key").unwrap();
        let listing_fails = PerBuildStore::new(Fake { fail_list: true, ..login.as_build("aaaa") }, SVC, "mac", "aaaa");
        assert!(listing_fails.handoff_entries().is_err());
    }

    #[test]
    fn an_item_of_this_build_s_that_cannot_be_read_quietly_fails_the_hand_over() {
        let login = Login::default();
        // Under this build's account, but made by another build: reading it would ask.
        login.as_build("intruder").write(&entry_service(), &own("aaaa"), "planted").unwrap();
        assert!(store(&login, "aaaa").handoff_entries().is_err());
        assert_eq!(login.asked(), Vec::<String>::new());
    }

    #[test]
    fn what_this_process_read_is_handed_over_even_when_the_item_no_longer_reads() {
        let login = Login::default();
        let s = store(&login, "aaaa");
        s.set(VAULT, "device key").unwrap();
        // The item can no longer be read quietly (as with a locked keychain): the value this
        // process holds goes over.
        login.as_build("aaaa").remove(&entry_service(), &own("aaaa")).unwrap();
        login.as_build("intruder").write(&entry_service(), &own("aaaa"), "planted").unwrap();
        let entries = s.handoff_entries().unwrap();
        assert_eq!(entries.iter().map(|(e, v)| (e.as_str(), v.as_deref().map(String::as_str))).collect::<Vec<_>>(), [(VAULT, Some("device key"))]);
    }

    #[test]
    fn a_hand_over_that_cannot_be_stored_whole_is_rolled_back() {
        let login = Login::default();
        let staged = PerBuildStore::new(Fake { fail_service: Some(format!("{SVC}/second")), ..login.as_build("bbbb") }, SVC, "mac", "bbbb");
        login.as_build("bbbb").write(&format!("{SVC}/first"), &own("bbbb"), "before").unwrap();
        let entries: Entries = vec![("first".into(), Some(Zeroizing::new("one".into()))), ("second".into(), Some(Zeroizing::new("two".into())))];
        // The first entry is stored, the second is not: the first is put back as it was.
        assert!(staged.persist_handoff(&entries).is_err());
        let reader = store(&login, "bbbb");
        assert_eq!(get(&reader, "first").as_deref(), Some("before"));
        assert_eq!(get(&reader, "second"), None);
    }

    #[test]
    fn an_item_of_this_build_s_account_that_another_build_made_is_an_error_not_a_dialog() {
        let login = Login::default();
        login.as_build("intruder").write(&entry_service(), &own("bbbb"), "planted").unwrap();
        assert!(store(&login, "bbbb").get(VAULT).is_err());
        assert_eq!(login.asked(), Vec::<String>::new());
    }

    #[test]
    fn listing_that_fails_still_finds_the_0_7_item() {
        let login = Login::default();
        login.as_build("adhoc").write(SVC, VAULT, "device key").unwrap();
        let new = PerBuildStore::new(Fake { fail_list: true, ..login.as_build("bbbb") }, SVC, "mac", "bbbb");
        assert_eq!(get(&new, VAULT).as_deref(), Some("device key"));
    }

    #[test]
    fn set_then_get_stays_in_this_process_and_retires_older_copies() {
        let login = Login::default();
        login.as_build("adhoc").write(SVC, VAULT, "stale").unwrap();
        let s = store(&login, "aaaa");
        s.set(VAULT, "fresh").unwrap();
        assert_eq!(get(&s, VAULT).as_deref(), Some("fresh"));
        assert_eq!(login.accounts(SVC), Vec::<String>::new());
        assert_eq!(s.status(), KeychainStatus::Available);
        assert!(format!("{s:?}").contains("aaaa"));
    }
}
