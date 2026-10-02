//! Multi-device sync through the core: devices are cores sharing one fake storage.

use std::sync::atomic::AtomicBool;

use lockra_sync::{MemoryRemote, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use tokio::sync::Notify;

use super::*;
use crate::settings::Settings;
use crate::ui::{JoinSource, StorageView, SyncStatus};
use crate::{SYNC_DEBOUNCE, SYNC_INTERVAL, StorageConfig};

const STORAGE_SECRET: &str = FakeTransport::SECRET;

fn s3(secret: &str) -> StorageConfig {
    StorageConfig::S3 {
        endpoint: "https://s3.example.com".into(),
        region: "us-east-1".into(),
        bucket: "lockra".into(),
        prefix: "sync/".into(),
        access_key_id: "AKIDLOCKRA".into(),
        secret_access_key: pw(secret),
        path_style: false,
    }
}

fn webdav() -> StorageConfig {
    StorageConfig::Webdav { url: "https://dav.example.com/dav/".into(), prefix: String::new(), username: "me".into(), password: pw(STORAGE_SECRET) }
}

fn manual(storage: StorageConfig, sync_key: &str) -> JoinSource {
    JoinSource::Manual { storage, sync_key: pw(sync_key) }
}

/// A device on `transport`'s storage.
fn device(transport: &Arc<FakeTransport>) -> Harness {
    harness_on(FakeUpdater::default(), Arc::clone(transport) as Arc<dyn SyncTransport>)
}

/// A device with an account that set up a space at `storage`; the sync key.
async fn first_device(transport: &Arc<FakeTransport>, storage: StorageConfig) -> (Harness, String) {
    let h = device(transport);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let created = h.core.sync_create(storage, pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    (h, created.sync_key)
}

fn issuers(h: &Harness) -> Vec<String> {
    let mut issuers: Vec<String> = h.core.state().entries.iter().map(|e| e.issuer.clone()).collect();
    issuers.sort();
    issuers
}

fn space(h: &Harness) -> crate::ui::SyncSpaceView {
    h.core.state().sync.space.expect("sync is on")
}

fn device_names(h: &Harness) -> Vec<(String, bool)> {
    space(h).devices.into_iter().map(|d| (d.name, d.this_device)).collect()
}

#[tokio::test(start_paused = true)]
async fn a_space_created_on_one_device_is_joined_by_a_new_one_and_both_converge() {
    for storage in [s3(STORAGE_SECRET), webdav()] {
        let transport = Arc::new(FakeTransport::default());
        let (desktop, sync_key) = first_device(&transport, storage.clone()).await;
        assert!(sync_key.starts_with("LKS1-"));
        assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }), "{:?}", space(&desktop).status);
        // On the storage: the keyring and the desktop's snapshot, neither of them readable.
        let store = transport.store(&storage);
        assert_eq!(store.paths().len(), 2, "{:?}", store.paths());
        for path in store.paths() {
            let text = String::from_utf8_lossy(&store.object(&path).unwrap()).into_owned();
            for clear in ["GitHub", SECRET, "Desktop", MASTER] {
                assert!(!text.contains(clear), "{clear} in {path}");
            }
        }

        // A new device without a vault: the storage, the sync key and the master password make one.
        let phone = device(&transport);
        phone.core.sync_join(manual(storage.clone(), &sync_key), pw(MASTER), " Phone ".into()).await.unwrap();
        assert_eq!(phone.core.state().phase, Phase::Unlocked);
        settle().await;
        assert_eq!(issuers(&phone), ["GitHub"]);
        assert_eq!(device_names(&phone), [("Phone".to_owned(), true), ("Desktop".to_owned(), false)]);
        // Its vault is a vault like any other: the master password opens it.
        phone.core.lock_vault();
        phone.core.unlock(pw(MASTER)).await.unwrap();
        settle().await;

        // A rename and a new account on the phone reach the desktop, then a deletion goes back.
        let id = phone.core.state().entries[0].id;
        phone.core.update_entry(id, EntryPatch { issuer: Some("GitHub Enterprise".into()), ..EntryPatch::default() }).unwrap();
        phone.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
        advance(SYNC_DEBOUNCE).await;
        desktop.core.sync_now().unwrap();
        settle().await;
        assert_eq!(issuers(&desktop), ["GitHub Enterprise", "Mail"]);
        desktop.core.delete_entry(id).unwrap();
        advance(SYNC_DEBOUNCE).await;
        phone.core.sync_now().unwrap();
        settle().await;
        assert_eq!(issuers(&phone), ["Mail"]);
        assert_eq!(device_names(&desktop), [("Desktop".to_owned(), true), ("Phone".to_owned(), false)]);
    }
}

#[tokio::test(start_paused = true)]
async fn an_invitation_joins_an_unlocked_vault_and_its_accounts_join_the_space() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    assert_eq!(code_err(desktop.core.sync_invite(pw("wrong password")).await), ErrorCode::WrongPassword);
    let invite = desktop.core.sync_invite(pw(MASTER)).await.unwrap();
    assert!(invite.invite.starts_with("lockra-invite:1:") && invite.svg.contains("<svg"));
    assert_eq!(invite.sync_key, sync_key);

    let laptop = device(&transport);
    laptop.core.create_vault(pw("another password")).await.unwrap();
    laptop.core.add_uri(&otpauth("Bank", "card", "MZXW6YTBOI")).unwrap();
    laptop.core.sync_join(JoinSource::Invite { text: pw(&invite.invite) }, pw(MASTER), "Laptop".into()).await.unwrap();
    settle().await;
    assert_eq!(issuers(&laptop), ["Bank", "GitHub"]);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&desktop), ["Bank", "GitHub"]);
    assert_eq!(code_err(laptop.core.sync_join(JoinSource::Invite { text: pw(&invite.invite) }, pw(MASTER), "Laptop".into()).await), ErrorCode::SyncAlreadyOn);
    // The laptop keeps its own master password.
    laptop.core.lock_vault();
    laptop.core.unlock(pw("another password")).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn setting_up_and_joining_tell_every_failure_apart() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    let join = |source: JoinSource, password: &str| phone.core.sync_join(source, pw(password), "Phone".into());
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &sync_key), "wrong password").await), ErrorCode::SyncWrongCredentials);
    assert_eq!(phone.core.state().phase, Phase::NoVault, "no vault is made on a failed join");
    let other_key = lockra_sync::SyncKey::generate().unwrap().to_text();
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &other_key), MASTER).await), ErrorCode::SyncSpaceNotFound);
    let typo = format!("{}A", &sync_key[..sync_key.len() - 1]);
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &typo), MASTER).await), ErrorCode::SyncKeyInvalid);
    assert_eq!(code_err(join(manual(s3("wrong secret"), &sync_key), MASTER).await), ErrorCode::SyncDenied);
    let StorageConfig::S3 { region, bucket, prefix, access_key_id, secret_access_key, path_style, .. } = s3(STORAGE_SECRET) else { unreachable!() };
    let plain = StorageConfig::S3 { endpoint: "http://192.168.1.2:9000".into(), region, bucket, prefix, access_key_id, secret_access_key, path_style };
    assert_eq!(code_err(join(manual(plain, &sync_key), MASTER).await), ErrorCode::SyncInsecure);
    let no_user = StorageConfig::Webdav { url: "https://dav.example.com".into(), prefix: String::new(), username: " ".into(), password: pw(STORAGE_SECRET) };
    assert_eq!(code_err(join(manual(no_user, &sync_key), MASTER).await), ErrorCode::SyncConfigInvalid);
    assert_eq!(code_err(join(JoinSource::Invite { text: pw("otpauth://totp/x?secret=GEZDGNBV") }, MASTER).await), ErrorCode::SyncInviteInvalid);
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &sync_key), "short").await), ErrorCode::PasswordTooShort);

    // Setting up: once, with the master password, on an unlocked vault.
    assert_eq!(code_err(desktop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "x".into()).await), ErrorCode::SyncAlreadyOn);
    let laptop = device(&transport);
    assert_eq!(code_err(laptop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "x".into()).await), ErrorCode::NoVault);
    laptop.core.create_vault(pw(MASTER)).await.unwrap();
    assert_eq!(code_err(laptop.core.sync_create(s3(STORAGE_SECRET), pw("wrong password"), "x".into()).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(laptop.core.sync_create(s3("wrong secret"), pw(MASTER), "x".into()).await), ErrorCode::SyncDenied);
    assert!(laptop.core.state().sync.space.is_none());
    // Without a space, the space's commands say so.
    assert_eq!(code_err(laptop.core.sync_now()), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_invite(pw(MASTER)).await), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_disable()), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_rename_device("x")), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_remove_device("x").await), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_set_storage(s3(STORAGE_SECRET), pw(MASTER)).await), ErrorCode::SyncOff);
    // Locked, nothing goes.
    desktop.core.lock_vault();
    assert_eq!(code_err(desktop.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "x".into()).await), ErrorCode::Locked);
    assert_eq!(code_err(desktop.core.sync_now()), ErrorCode::Locked);
}

#[tokio::test(start_paused = true)]
async fn runs_follow_unlocking_changes_and_the_interval_and_stop_when_locked() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.set_settings(Settings { auto_lock_minutes: 0, ..desktop.core.state().settings }).unwrap();
    let store = transport.store(&s3(STORAGE_SECRET));
    let calls = || store.calls().len();
    let quiet = calls();
    advance(SYNC_INTERVAL - Duration::from_secs(1)).await;
    assert_eq!(calls(), quiet, "nothing before the interval");
    advance(Duration::from_secs(2)).await;
    assert!(calls() > quiet, "the periodic run");

    let before_change = calls();
    desktop.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    advance(SYNC_DEBOUNCE - Duration::from_millis(500)).await;
    assert_eq!(calls(), before_change, "the debounce");
    advance(Duration::from_secs(1)).await;
    assert!(calls() > before_change, "the change went out");

    desktop.core.lock_vault();
    assert!(desktop.core.state().sync.space.is_none(), "the space is in the vault");
    let locked = calls();
    advance(SYNC_INTERVAL * 3).await;
    assert_eq!(calls(), locked, "locked: no run");
    desktop.core.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    assert!(calls() > locked, "unlocking runs at once");
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
    assert!(space(&desktop).last_sync_ms.is_some());
}

#[tokio::test(start_paused = true)]
async fn a_failed_run_is_shown_and_the_next_one_recovers() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let store = transport.store(&s3(STORAGE_SECRET));
    store.fail_next(SyncError::Network("offline".into()));
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Failed { code: ErrorCode::SyncNetwork, .. }), "{:?}", space(&desktop).status);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
}

#[tokio::test(start_paused = true)]
async fn a_copied_vault_becomes_a_new_device_of_the_space() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.lock_vault();
    // The vault file, its local part included, copied to another computer.
    let other = device(&transport);
    fs::create_dir_all(other.data_dir()).unwrap();
    fs::copy(desktop.vault_path(), other.vault_path()).unwrap();
    let copy = other.restart();
    copy.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    copy.add_uri(&otpauth("Copy", "only", "MZXW6YTBOI")).unwrap();
    advance(SYNC_DEBOUNCE).await;

    // The desktop finds its name written by the copy: it becomes a new device and syncs on.
    desktop.core.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }), "{:?}", space(&desktop).status);
    assert_eq!(issuers(&desktop), ["Copy", "GitHub"]);
    assert_eq!(space(&desktop).devices.len(), 2, "the copy under the old name, the desktop under a new one");
    copy.sync_now().unwrap();
    settle().await;
    let copy_issuers: Vec<String> = copy.state().entries.iter().map(|e| e.issuer.clone()).collect();
    assert_eq!(copy_issuers.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_new_master_password_reaches_the_keyring_even_after_a_failed_run() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let store = transport.store(&s3(STORAGE_SECRET));
    store.fail_next(SyncError::Network("offline".into()));
    desktop.core.change_password(pw(MASTER), pw("a new password")).await.unwrap();
    assert!(space(&desktop).keyring_pending);
    settle().await;
    assert!(space(&desktop).keyring_pending, "the run failed: still to be written");
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(!space(&desktop).keyring_pending);

    let phone = device(&transport);
    assert_eq!(code_err(phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into()).await), ErrorCode::SyncWrongCredentials);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw("a new password"), "Phone".into()).await.unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub"]);
}

#[tokio::test(start_paused = true)]
async fn backups_and_the_webviews_state_carry_no_sync_secret() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let path = desktop.dir.path().join("mine.lockrabackup");
    desktop.core.backup_to(path.clone(), None).await.unwrap();
    let opened = Sealed::open_with_password(&fs::read(&path).unwrap(), MASTER.as_bytes()).unwrap();
    let backup = String::from_utf8(opened.payload.to_vec()).unwrap();
    for secret in [STORAGE_SECRET, &sync_key[5..19], "\"local\"", "AKIDLOCKRA"] {
        assert!(!backup.contains(secret), "{secret} in the backup: {backup}");
    }
    let state = serde_json::to_string(&desktop.core.state()).unwrap();
    for secret in [STORAGE_SECRET, &sync_key[5..19]] {
        assert!(!state.contains(secret), "{secret} in the state: {state}");
    }
    let view = space(&desktop);
    assert_eq!(
        view.storage,
        StorageView::S3 {
            endpoint: "https://s3.example.com".into(),
            region: "us-east-1".into(),
            bucket: "lockra".into(),
            prefix: "sync/".into(),
            access_key_id: "AKIDLOCKRA".into(),
            path_style: false
        }
    );
    // The vault file holds them, encrypted.
    let vault = fs::read(desktop.vault_path()).unwrap();
    assert!(!String::from_utf8_lossy(&vault).contains(STORAGE_SECRET));
}

#[tokio::test(start_paused = true)]
async fn devices_are_renamed_and_removed_and_sync_turns_off_here_only() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into()).await.unwrap();
    settle().await;
    phone.core.sync_rename_device("  Pixel 8\n").unwrap();
    settle().await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(device_names(&desktop), [("Desktop".to_owned(), true), ("Pixel 8".to_owned(), false)]);

    let devices = space(&desktop).devices;
    assert_eq!(code_err(desktop.core.sync_remove_device(&devices[0].tag).await), ErrorCode::Internal, "not this device");
    desktop.core.sync_remove_device(&devices[1].tag).await.unwrap();
    settle().await;
    assert_eq!(space(&desktop).devices.len(), 1);
    // The phone is still in use: its next change lists it again.
    phone.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    advance(SYNC_DEBOUNCE).await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(space(&desktop).devices.len(), 2);

    // Off on the phone: its accounts stay, the space goes on without it.
    phone.core.sync_disable().unwrap();
    assert!(phone.core.state().sync.space.is_none());
    assert_eq!(issuers(&phone), ["GitHub", "Mail"]);
    phone.core.add_uri(&otpauth("Local", "only", "MFRGGZDF")).unwrap();
    advance(SYNC_DEBOUNCE).await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&desktop), ["GitHub", "Mail"]);
}

#[tokio::test(start_paused = true)]
async fn restoring_a_backup_in_place_reaches_the_other_devices() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let path = desktop.dir.path().join("before.lockrabackup");
    desktop.core.backup_to(path.clone(), None).await.unwrap();
    desktop.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into()).await.unwrap();
    advance(SYNC_DEBOUNCE).await;
    phone.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub", "Mail"]);

    desktop.core.restore_open(path).await.unwrap();
    desktop.core.restore_commit(pw(MASTER), RestoreMode::Replace).await.unwrap();
    assert!(desktop.core.state().sync.space.is_some(), "the space stays");
    advance(SYNC_DEBOUNCE).await;
    phone.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub"]);
}

/// The fake storage, with writes that can be held: a run waits there while the test locks.
struct Gate {
    remote: MemoryRemote,
    hold: AtomicBool,
    reached: Notify,
    go: Notify,
}

impl RemoteStore for Gate {
    fn conditional_puts(&self) -> bool {
        self.remote.conditional_puts()
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        self.remote.list(dir)
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        self.remote.get(path)
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            if self.hold.load(Ordering::SeqCst) {
                self.reached.notify_one();
                self.go.notified().await;
            }
            self.remote.put(path, bytes, condition).await
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        self.remote.delete(path)
    }
}

struct GateTransport(Arc<Gate>);

impl SyncTransport for GateTransport {
    fn open(&self, _config: &StorageConfig) -> Result<Arc<dyn RemoteStore>, SyncError> {
        Ok(Arc::clone(&self.0) as Arc<dyn RemoteStore>)
    }
}

#[tokio::test(start_paused = true)]
async fn a_write_cut_off_by_locking_is_this_devices_own_on_the_next_run() {
    let gate = Arc::new(Gate { remote: MemoryRemote::new(true), hold: AtomicBool::new(false), reached: Notify::new(), go: Notify::new() });
    let desktop = harness_on(FakeUpdater::default(), Arc::new(GateTransport(Arc::clone(&gate))));
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    desktop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    // The next write reaches the storage only after the vault was locked: the run cannot record it.
    gate.hold.store(true, Ordering::SeqCst);
    desktop.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    advance(SYNC_DEBOUNCE).await;
    gate.reached.notified().await;
    desktop.core.lock_vault();
    gate.hold.store(false, Ordering::SeqCst);
    gate.go.notify_one();
    settle().await;

    desktop.core.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    let view = space(&desktop);
    assert!(matches!(view.status, SyncStatus::Synced { .. }), "{:?}", view.status);
    assert_eq!(view.devices.len(), 1, "the write is recognised: no clash, no new device");
}

#[tokio::test(start_paused = true)]
async fn the_storage_is_opened_once_for_its_settings() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let opened = || transport.opened.load(Ordering::SeqCst);
    assert_eq!(opened(), 1, "setting up opens it, the first run reuses it");
    for _ in 0..3 {
        desktop.core.sync_now().unwrap();
        settle().await;
    }
    assert_eq!(opened(), 1);
    // New settings for the same space: opened once more, then reused.
    let StorageConfig::S3 { region, bucket, prefix, access_key_id, secret_access_key, path_style, .. } = s3(STORAGE_SECRET) else { unreachable!() };
    let moved =
        StorageConfig::S3 { endpoint: "https://s3.example.com".into(), region, bucket, prefix, access_key_id, secret_access_key, path_style: !path_style };
    assert_eq!(code_err(desktop.core.sync_set_storage(moved.clone(), pw("wrong password")).await), ErrorCode::WrongPassword);
    desktop.core.sync_set_storage(moved, pw(MASTER)).await.unwrap();
    settle().await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(opened(), 2);
    // Settings that point where no space is are refused.
    let elsewhere =
        StorageConfig::Webdav { url: "https://dav.example.com/other/".into(), prefix: String::new(), username: "me".into(), password: pw(STORAGE_SECRET) };
    assert_eq!(code_err(desktop.core.sync_set_storage(elsewhere, pw(MASTER)).await), ErrorCode::SyncSpaceNotFound);
    // Locked and unlocked again: the same settings, the same storage.
    desktop.core.lock_vault();
    desktop.core.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
}

#[tokio::test(start_paused = true)]
async fn the_latest_master_password_keeps_the_keyring_when_two_devices_change_it_apart() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into()).await.unwrap();
    settle().await;
    let store = transport.store(&s3(STORAGE_SECRET));
    // The desktop changes its password offline: its keyring waits.
    store.fail_next(SyncError::Network("offline".into()));
    desktop.core.change_password(pw(MASTER), pw("desktop password")).await.unwrap();
    settle().await;
    assert!(space(&desktop).keyring_pending);
    // Later, the phone changes its own, online.
    advance(Duration::from_secs(10)).await;
    phone.core.change_password(pw(MASTER), pw("phone password")).await.unwrap();
    settle().await;
    assert!(!space(&phone).keyring_pending);
    // Back online, the desktop's older keyring does not go over the phone's.
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(!space(&desktop).keyring_pending);
    let laptop = device(&transport);
    assert_eq!(
        code_err(laptop.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw("desktop password"), "Laptop".into()).await),
        ErrorCode::SyncWrongCredentials
    );
    laptop.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw("phone password"), "Laptop".into()).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn an_hotp_counter_never_goes_back_when_two_devices_advance_it_apart() {
    let transport = Arc::new(FakeTransport::default());
    let desktop = device(&transport);
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    let id = desktop.core.add_uri("otpauth://hotp/Bank:card?secret=MZXW6YTBOI&counter=0&issuer=Bank").unwrap();
    let created = desktop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &created.sync_key), pw(MASTER), "Phone".into()).await.unwrap();
    settle().await;
    let counter = |h: &Harness| match h.core.state().entries[0].kind {
        OtpKind::Hotp { counter } => counter,
        OtpKind::Totp { .. } => unreachable!(),
    };
    // Apart: the desktop shows three more codes and syncs them; the phone, not synced since, shows
    // one more code later, from the counter it had.
    for _ in 0..3 {
        desktop.core.hotp_next(id).unwrap();
    }
    advance(SYNC_DEBOUNCE).await;
    assert_eq!(counter(&phone), 0);
    phone.core.hotp_next(id).unwrap();
    assert_eq!(counter(&phone), 1);
    for h in [&phone, &desktop] {
        h.core.sync_now().unwrap();
        settle().await;
    }
    assert_eq!((counter(&desktop), counter(&phone)), (3, 3), "the later change wins, but no code is shown twice");
}

#[tokio::test(start_paused = true)]
async fn new_storage_settings_must_lead_to_this_spaces_keyring() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let original = transport.store(&s3(STORAGE_SECRET));
    let keyring = original.paths().into_iter().find(|p| p.ends_with("keyring.lks")).unwrap();
    let relative = keyring.strip_prefix("sync/").unwrap().to_owned();
    // Moved to a WebDAV folder (no prefix): a damaged keyring there is refused, the real one taken.
    let moved = webdav();
    let there = transport.store(&moved);
    there.set_object(&relative, b"LKSKEYR1 not a keyring".to_vec());
    assert_eq!(code_err(desktop.core.sync_set_storage(moved.clone(), pw(MASTER)).await), ErrorCode::SyncDataCorrupted);
    there.set_object(&relative, original.object(&keyring).unwrap());
    desktop.core.sync_set_storage(moved, pw(MASTER)).await.unwrap();
    settle().await;
    assert!(matches!(space(&desktop).storage, StorageView::Webdav { .. }));
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
}
