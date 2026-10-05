//! Multi-device sync through the core: devices are cores sharing one fake storage.

use lockra_sync::{MemoryRemote, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SyncError};
use tokio::sync::Notify;

use super::*;
use crate::settings::Settings;
use crate::ui::{JoinSource, StorageView, SyncStatus};
use crate::{
    SYNC_DEBOUNCE, SYNC_FOCUS_MIN, SYNC_FOLDER_SETTLE, SYNC_INTERVAL, SYNC_INTERVAL_FOLDER, SYNC_INTERVAL_FOLDER_FOREGROUND, SYNC_INTERVAL_FOREGROUND,
    StorageConfig,
};

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
        // On the storage: the desktop's snapshot alone, its keyring inside, nothing of it readable.
        let store = transport.store(&storage);
        assert_eq!(store.paths().len(), 1, "{:?}", store.paths());
        for path in store.paths() {
            let text = String::from_utf8_lossy(&store.object(&path).unwrap()).into_owned();
            for clear in ["GitHub", SECRET, "Desktop", MASTER] {
                assert!(!text.contains(clear), "{clear} in {path}");
            }
        }

        // A new device without a vault: the storage, the sync key and the master password make one.
        let phone = device(&transport);
        phone.core.sync_join(manual(storage.clone(), &sync_key), pw(MASTER), " Phone ".into(), None).await.unwrap();
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
    assert_eq!(code_err(desktop.core.sync_invite(Some(pw("wrong password")), None).await), ErrorCode::WrongPassword);
    let invite = desktop.core.sync_invite(Some(pw(MASTER)), None).await.unwrap();
    assert!(invite.invite.starts_with("lockra-invite:1:") && invite.svg.contains("<svg"));
    assert_eq!(invite.sync_key, sync_key);

    let laptop = device(&transport);
    laptop.core.create_vault(pw("another password")).await.unwrap();
    laptop.core.add_uri(&otpauth("Bank", "card", "MZXW6YTBOI")).unwrap();
    let invited = || JoinSource::Invite { text: pw(&invite.invite), code: None, storage: None };
    // This vault's own master password is checked; alone, it opens nothing of the space.
    assert_eq!(code_err(laptop.core.sync_join(invited(), pw(MASTER), "Laptop".into(), None).await), ErrorCode::WrongPassword);
    // Its own password is right but opens nothing in the space: the join asks for the space's.
    assert_eq!(code_err(laptop.core.sync_join(invited(), pw("another password"), "Laptop".into(), None).await), ErrorCode::SyncSpacePasswordNeeded);
    // A wrong space password, once asked for, is wrong.
    assert_eq!(
        code_err(laptop.core.sync_join(invited(), pw("another password"), "Laptop".into(), Some(pw("not the space's"))).await),
        ErrorCode::SyncWrongCredentials
    );
    laptop.core.sync_join(invited(), pw("another password"), "Laptop".into(), Some(pw(MASTER))).await.unwrap();
    settle().await;
    assert_eq!(issuers(&laptop), ["Bank", "GitHub"]);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&desktop), ["Bank", "GitHub"]);
    assert_eq!(code_err(laptop.core.sync_join(invited(), pw("another password"), "Laptop".into(), None).await), ErrorCode::SyncAlreadyOn);
    // The laptop keeps its own master password, and its keyring in the space is under it: that
    // password opens the space as well now.
    laptop.core.lock_vault();
    laptop.core.unlock(pw("another password")).await.unwrap();
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw("another password"), "Phone".into(), None).await.unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["Bank", "GitHub"]);
}

#[tokio::test(start_paused = true)]
async fn a_shared_invitation_joins_with_its_code_and_the_biometric_check_can_show_it() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    // Without a password, only the biometric check that unlocks this vault proves presence.
    assert_eq!(code_err(desktop.core.sync_invite(None, None).await), ErrorCode::BiometricUnavailable);
    desktop.core.enable_device_biometric(None).await.unwrap();
    let invite = desktop.core.sync_invite(None, Some("show the invitation".into())).await.unwrap();
    assert_eq!(desktop.biometrics.reasons(), ["unlock Lockra".to_owned(), "show the invitation".to_owned()]);
    assert!(invite.shared_text.starts_with("lockra-invite:2:") && !invite.shared_text.contains(STORAGE_SECRET));
    assert_eq!(invite.code.len(), 11);
    assert_eq!(invite.sync_key, sync_key);
    desktop.biometrics.answer(Err(crate::ports::BiometricError::Cancelled));
    assert_eq!(code_err(desktop.core.sync_invite(None, None).await), ErrorCode::BiometricCancelled);

    let sealed = |code: Option<&str>| JoinSource::Invite { text: pw(&invite.shared_text), code: code.map(pw), storage: None };
    let phone = device(&transport);
    for wrong in [None, Some("ABCDE-FGHJK")] {
        assert_eq!(code_err(phone.core.sync_join(sealed(wrong), pw(MASTER), "Phone".into(), None).await), ErrorCode::SyncInviteCodeWrong);
    }
    assert_eq!(phone.core.state().phase, Phase::NoVault);
    phone.core.sync_join(sealed(Some(&invite.code.to_lowercase())), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub"]);
}

#[tokio::test(start_paused = true)]
async fn the_device_that_made_the_space_is_reminded_of_the_key_until_it_is_saved() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    assert!(!space(&desktop).key_saved);
    // The key file: the interface's words with the key in its one slot, after presence is proved.
    let template = "Lockra sync key: {{sync_key}}";
    assert_eq!(code_err(desktop.core.sync_key_file(Some(pw(MASTER)), None, "no slot").await), ErrorCode::Internal);
    assert_eq!(code_err(desktop.core.sync_key_file(Some(pw(MASTER)), None, "{{sync_key}} {{sync_key}}").await), ErrorCode::Internal);
    assert_eq!(code_err(desktop.core.sync_key_file(Some(pw(MASTER)), None, &"x".repeat(9000)).await), ErrorCode::Internal);
    assert_eq!(code_err(desktop.core.sync_key_file(Some(pw("wrong password")), None, template).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(desktop.core.sync_key_file(None, None, template).await), ErrorCode::BiometricUnavailable);
    let text = desktop.core.sync_key_file(Some(pw(MASTER)), None, template).await.unwrap();
    assert_eq!(text.as_str(), format!("Lockra sync key: {sync_key}"));
    assert!(!space(&desktop).key_saved, "making the text saves nothing");
    // Saved to a file (the desktop's save dialog chose it): written, private, and the reminder goes.
    let file = desktop.dir.path().join("lockra-sync-key.txt");
    desktop.core.sync_key_save(Some(pw(MASTER)), None, template, file.clone()).await.unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("Lockra sync key: {sync_key}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert!(space(&desktop).key_saved);
    // It stays saved across a lock, and saying so again changes nothing.
    desktop.core.lock_vault();
    desktop.core.unlock(pw(MASTER)).await.unwrap();
    assert!(space(&desktop).key_saved);
    desktop.core.sync_key_acknowledge().unwrap();

    // A device that joined has nothing to save: the reminder is the maker's.
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    assert!(space(&phone).key_saved);
    // The maker can also just say it wrote the key down.
    let (laptop, _) = first_device(&Arc::new(FakeTransport::default()), webdav()).await;
    assert!(!space(&laptop).key_saved);
    laptop.core.sync_key_acknowledge().unwrap();
    assert!(space(&laptop).key_saved);
    // Without a space, there is no key to save.
    let empty = device(&transport);
    empty.core.create_vault(pw(MASTER)).await.unwrap();
    assert_eq!(code_err(empty.core.sync_key_acknowledge()), ErrorCode::SyncOff);
    assert_eq!(code_err(empty.core.sync_key_file(Some(pw(MASTER)), None, template).await), ErrorCode::SyncOff);
}

#[tokio::test(start_paused = true)]
async fn setting_up_and_joining_tell_every_failure_apart() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    let join = |source: JoinSource, password: &str| phone.core.sync_join(source, pw(password), "Phone".into(), None);
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &sync_key), "wrong password").await), ErrorCode::SyncWrongCredentials);
    assert_eq!(phone.core.state().phase, Phase::NoVault, "no vault is made on a failed join");
    let other_key = lockra_sync::SyncKey::generate().unwrap().to_text();
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &other_key), MASTER).await), ErrorCode::SyncSpaceNotFound);
    // The last character mistyped, for certain: a random key may end in the letter put there.
    let (head, last) = sync_key.split_at(sync_key.len() - 1);
    let typo = format!("{head}{}", if last == "A" { "B" } else { "A" });
    assert_eq!(code_err(join(manual(s3(STORAGE_SECRET), &typo), MASTER).await), ErrorCode::SyncKeyInvalid);
    assert_eq!(code_err(join(manual(s3("wrong secret"), &sync_key), MASTER).await), ErrorCode::SyncDenied);
    let StorageConfig::S3 { region, bucket, prefix, access_key_id, secret_access_key, path_style, .. } = s3(STORAGE_SECRET) else { unreachable!() };
    let plain = StorageConfig::S3 { endpoint: "http://192.168.1.2:9000".into(), region, bucket, prefix, access_key_id, secret_access_key, path_style };
    assert_eq!(code_err(join(manual(plain, &sync_key), MASTER).await), ErrorCode::SyncInsecure);
    let no_user = StorageConfig::Webdav { url: "https://dav.example.com".into(), prefix: String::new(), username: " ".into(), password: pw(STORAGE_SECRET) };
    assert_eq!(code_err(join(manual(no_user, &sync_key), MASTER).await), ErrorCode::SyncConfigInvalid);
    assert_eq!(
        code_err(join(JoinSource::Invite { text: pw("otpauth://totp/x?secret=GEZDGNBV"), code: None, storage: None }, MASTER).await),
        ErrorCode::SyncInviteInvalid
    );
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
    assert_eq!(code_err(laptop.core.sync_invite(Some(pw(MASTER)), None).await), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_disable()), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_rename_device("x")), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_remove_device("x").await), ErrorCode::SyncOff);
    assert_eq!(code_err(laptop.core.sync_set_storage(s3(STORAGE_SECRET), pw(MASTER)).await), ErrorCode::SyncOff);
    // Locked, nothing goes.
    desktop.core.lock_vault();
    assert_eq!(code_err(desktop.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "x".into(), None).await), ErrorCode::Locked);
    assert_eq!(code_err(desktop.core.sync_now()), ErrorCode::Locked);
}

#[tokio::test(start_paused = true)]
async fn runs_follow_unlocking_changes_and_the_interval_and_stop_when_locked() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.set_settings(Settings { auto_lock_minutes: 0, ..desktop.core.state().settings }).unwrap();
    let store = transport.store(&s3(STORAGE_SECRET));
    let calls = || store.calls().len();
    // Behind other windows, from a run that ended there.
    desktop.core.set_foreground(false);
    desktop.core.sync_now().unwrap();
    settle().await;
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
async fn in_front_runs_come_every_minute_and_coming_back_runs_soon() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.set_settings(Settings { auto_lock_minutes: 0, ..desktop.core.state().settings }).unwrap();
    let store = transport.store(&s3(STORAGE_SECRET));
    let calls = || store.calls().len();
    // In front (where the app starts): a run a minute after the last.
    let quiet = calls();
    advance(SYNC_INTERVAL_FOREGROUND - Duration::from_secs(1)).await;
    assert_eq!(calls(), quiet, "nothing before the minute");
    advance(Duration::from_secs(2)).await;
    assert!(calls() > quiet, "the run a minute later");

    // Behind: the run already planned, then five minutes.
    desktop.core.set_foreground(false);
    let behind = calls();
    advance(SYNC_INTERVAL_FOREGROUND).await;
    assert!(calls() > behind, "the planned run");
    let after = calls();
    advance(SYNC_INTERVAL - Duration::from_secs(1)).await;
    assert_eq!(calls(), after, "behind: five minutes");

    // Back in front, the last run long enough ago: a run at once.
    desktop.core.set_foreground(true);
    settle().await;
    assert!(calls() > after, "coming back runs");
    // Back again just after: the next run comes half a minute after the last, not at once.
    let back = calls();
    desktop.core.set_foreground(false);
    desktop.core.set_foreground(true);
    settle().await;
    assert_eq!(calls(), back, "not again at once");
    advance(SYNC_FOCUS_MIN - Duration::from_secs(1)).await;
    assert_eq!(calls(), back);
    advance(Duration::from_secs(2)).await;
    assert!(calls() > back, "half a minute after the last run");
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
}

#[tokio::test(start_paused = true)]
async fn in_front_a_failed_run_still_waits_five_minutes() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.set_settings(Settings { auto_lock_minutes: 0, ..desktop.core.state().settings }).unwrap();
    let store = transport.store(&s3(STORAGE_SECRET));
    let calls = || store.calls().len();
    store.fail_next(SyncError::Network("offline".into()));
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Failed { code: ErrorCode::SyncNetwork, .. }));
    let failed = calls();
    advance(SYNC_INTERVAL - Duration::from_secs(1)).await;
    assert_eq!(calls(), failed, "a failure waits the long interval");
    advance(Duration::from_secs(2)).await;
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }), "{:?}", space(&desktop).status);
    // Then, in front again, every minute.
    let recovered = calls();
    advance(SYNC_INTERVAL_FOREGROUND + Duration::from_secs(1)).await;
    assert!(calls() > recovered);
}

/// The sync notices among a device's notices since the last look.
fn brought(h: &mut Harness) -> Vec<Notice> {
    h.notices().into_iter().filter(|n| matches!(n, Notice::SyncBrought { .. })).collect()
}

#[tokio::test(start_paused = true)]
async fn what_a_run_brings_is_told_with_the_devices_it_came_from() {
    let transport = Arc::new(FakeTransport::default());
    let (mut desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    desktop.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    desktop.core.add_uri(&otpauth("Bank", "me", "MFRGGZDF")).unwrap();
    advance(SYNC_DEBOUNCE).await;
    assert_eq!(brought(&mut desktop), [], "its own changes are not news");

    // Joining brings the space's accounts.
    let mut phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    assert_eq!(brought(&mut phone), [Notice::SyncBrought { added: 3, updated: 0, removed: 0, devices: vec!["Desktop".into()] }]);

    // A rename, a deletion and a new account on the phone, told on the desktop.
    let id_of = |h: &Harness, issuer: &str| h.core.state().entries.iter().find(|e| e.issuer == issuer).unwrap().id;
    let github = id_of(&phone, "GitHub");
    phone.core.update_entry(github, EntryPatch { issuer: Some("GitHub Enterprise".into()), ..EntryPatch::default() }).unwrap();
    phone.core.delete_entry(id_of(&phone, "Bank")).unwrap();
    phone.core.add_uri(&otpauth("Wiki", "me", "GEZDGNBVGY3TQOJQ")).unwrap();
    advance(SYNC_DEBOUNCE).await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&desktop), ["GitHub Enterprise", "Mail", "Wiki"]);
    assert_eq!(brought(&mut desktop), [Notice::SyncBrought { added: 1, updated: 1, removed: 1, devices: vec!["Phone".into()] }]);
    // A run that brings nothing tells nothing.
    desktop.core.sync_now().unwrap();
    settle().await;
    assert_eq!(brought(&mut desktop), []);

    // Taken in, then the run failed writing: still told, without the devices.
    phone.core.add_uri(&otpauth("Shop", "me", "MFRGGZDFMZTWQ2LK")).unwrap();
    advance(SYNC_DEBOUNCE).await;
    transport.store(&s3(STORAGE_SECRET)).fail_next_put(SyncError::Network("offline".into()), false);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Failed { .. }), "{:?}", space(&desktop).status);
    assert_eq!(brought(&mut desktop), [Notice::SyncBrought { added: 1, updated: 0, removed: 0, devices: vec![] }]);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
    assert_eq!(brought(&mut desktop), [], "told once");
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
async fn a_new_master_password_reaches_the_space_even_after_a_failed_run() {
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
    assert_eq!(code_err(phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await), ErrorCode::SyncWrongCredentials);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw("a new password"), "Phone".into(), None).await.unwrap();
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
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
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
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
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

/// The fake storage, with calls that can be held: a run waits there while the test acts.
struct Gate {
    remote: MemoryRemote,
    /// Calls that begin with this (`put …`, `list sync/`) wait until released.
    hold: std::sync::Mutex<Option<String>>,
    reached: Notify,
    go: Notify,
}

impl Gate {
    fn new(conditional: bool) -> Arc<Self> {
        Arc::new(Self { remote: MemoryRemote::new(conditional), hold: std::sync::Mutex::new(None), reached: Notify::new(), go: Notify::new() })
    }

    fn hold(&self, call: &str) {
        *self.hold.lock().unwrap() = Some(call.to_owned());
    }

    fn release(&self) {
        *self.hold.lock().unwrap() = None;
        self.go.notify_one();
    }

    async fn pass(&self, call: String) {
        let held = self.hold.lock().unwrap().as_ref().is_some_and(|start| call.starts_with(start.as_str()));
        if held {
            self.reached.notify_one();
            self.go.notified().await;
        }
    }
}

impl RemoteStore for Gate {
    fn conditional_puts(&self) -> bool {
        self.remote.conditional_puts()
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            self.pass(format!("list {dir}")).await;
            self.remote.list(dir).await
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        self.remote.get(path)
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            self.pass(format!("put {path}")).await;
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
    let gate = Gate::new(true);
    let desktop = harness_on(FakeUpdater::default(), Arc::new(GateTransport(Arc::clone(&gate))));
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    desktop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    // The next write reaches the storage only after the vault was locked: the run cannot record it.
    gate.hold("put ");
    desktop.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    advance(SYNC_DEBOUNCE).await;
    gate.reached.notified().await;
    desktop.core.lock_vault();
    gate.release();
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
async fn every_devices_master_password_opens_the_space_and_a_replaced_one_no_longer_does() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    let store = transport.store(&s3(STORAGE_SECRET));
    // The desktop changes its password offline: its keyring waits.
    store.fail_next(SyncError::Network("offline".into()));
    desktop.core.change_password(pw(MASTER), pw("desktop password")).await.unwrap();
    settle().await;
    assert!(space(&desktop).keyring_pending);
    // The phone changes its own, online; then the desktop is back.
    phone.core.change_password(pw(MASTER), pw("phone password")).await.unwrap();
    settle().await;
    assert!(!space(&phone).keyring_pending);
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(!space(&desktop).keyring_pending);
    // Each device's master password opens the space; the one both replaced opens nothing.
    let join = |password: &'static str| {
        let laptop = device(&transport);
        let sync_key = sync_key.clone();
        async move { laptop.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(password), "Laptop".into(), None).await.map_err(|e| e.code) }
    };
    assert_eq!(join(MASTER).await, Err(ErrorCode::SyncWrongCredentials));
    assert_eq!(join("desktop password").await, Ok(()));
    assert_eq!(join("phone password").await, Ok(()));
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
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &created.sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
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
async fn new_storage_settings_must_hold_this_spaces_snapshots() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let original = transport.store(&s3(STORAGE_SECRET));
    let snapshot = original.paths().pop().unwrap();
    let relative = snapshot.strip_prefix("sync/").unwrap().to_owned();
    // Moved to a WebDAV folder (no prefix): a damaged snapshot there is refused, the real one taken.
    let moved = webdav();
    let there = transport.store(&moved);
    let mut damaged = original.object(&snapshot).unwrap();
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    there.set_object(&relative, damaged);
    assert_eq!(code_err(desktop.core.sync_set_storage(moved.clone(), pw(MASTER)).await), ErrorCode::SyncDataCorrupted);
    there.set_object(&relative, original.object(&snapshot).unwrap());
    desktop.core.sync_set_storage(moved, pw(MASTER)).await.unwrap();
    settle().await;
    assert!(matches!(space(&desktop).storage, StorageView::Webdav { .. }));
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }));
}

#[tokio::test(start_paused = true)]
async fn two_devices_writing_at_once_lose_nothing_on_a_storage_without_conditions() {
    // WebDAV's way: every write goes through, whatever is there.
    let gate = Gate::new(false);
    let transport: Arc<dyn SyncTransport> = Arc::new(GateTransport(Arc::clone(&gate)));
    let desktop = harness_on(FakeUpdater::default(), Arc::clone(&transport));
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    let created = desktop.core.sync_create(webdav(), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    let desktop_snapshot = gate.remote.paths().pop().unwrap();
    let phone = harness_on(FakeUpdater::default(), Arc::clone(&transport));
    phone.core.sync_join(manual(webdav(), &created.sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;

    // The desktop's run is held at its write while the phone runs from start to end: both read the
    // same space, both write, a new account and a new master password each.
    gate.hold(&format!("put {desktop_snapshot}"));
    desktop.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    desktop.core.change_password(pw(MASTER), pw("desktop password")).await.unwrap();
    gate.reached.notified().await;
    phone.core.add_uri(&otpauth("Bank", "card", "MZXW6YTBOI")).unwrap();
    phone.core.change_password(pw(MASTER), pw("phone password")).await.unwrap();
    advance(SYNC_DEBOUNCE).await;
    assert!(!space(&phone).keyring_pending, "the phone's run went through");
    gate.release();
    settle().await;
    for h in [&desktop, &phone, &desktop] {
        h.core.sync_now().unwrap();
        settle().await;
    }
    assert_eq!(issuers(&desktop), ["Bank", "Mail"]);
    assert_eq!(issuers(&phone), ["Bank", "Mail"]);
    // Both keyrings are there: each new master password opens the space.
    for password in ["desktop password", "phone password"] {
        let laptop = harness_on(FakeUpdater::default(), Arc::clone(&transport));
        laptop.core.sync_join(manual(webdav(), &created.sync_key), pw(password), "Laptop".into(), None).await.unwrap();
        laptop.core.sync_disable().unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_stops_before_writing_where_the_space_no_longer_is() {
    let gate = Gate::new(true);
    let desktop = harness_on(FakeUpdater::default(), Arc::new(GateTransport(Arc::clone(&gate))));
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    desktop.core.sync_create(s3(STORAGE_SECRET), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    // The space is moved (its folder, then the settings) while a run is out reading the old place.
    let snapshot = gate.remote.paths().pop().unwrap();
    gate.remote.set_object(snapshot.strip_prefix("sync/").unwrap(), gate.remote.object(&snapshot).unwrap());
    gate.hold("list sync/");
    desktop.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    advance(SYNC_DEBOUNCE).await;
    gate.reached.notified().await;
    desktop.core.sync_set_storage(webdav(), pw(MASTER)).await.unwrap();
    let calls = gate.remote.calls().len();
    gate.release();
    settle().await;
    let after: Vec<String> = gate.remote.calls()[calls..].to_vec();
    assert!(!after.iter().any(|c| c.starts_with("put sync/")), "nothing written to the old place: {after:?}");
    assert!(after.iter().any(|c| c.starts_with("put lockra-sync-v1/")), "the next run writes to the new one: {after:?}");
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }), "{:?}", space(&desktop).status);
}

#[tokio::test(start_paused = true)]
async fn a_group_set_on_several_accounts_reaches_the_other_devices() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    let ids: Vec<_> = desktop.core.state().entries.iter().map(|e| e.id).collect();
    desktop.core.set_entries_group(&ids, "Work").unwrap();
    advance(SYNC_DEBOUNCE).await;
    phone.core.sync_now().unwrap();
    settle().await;
    assert!(phone.core.state().entries.iter().all(|e| e.group.as_deref() == Some("Work")));
}

#[tokio::test(start_paused = true)]
async fn an_accounts_colour_and_mark_reach_the_other_devices() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, sync_key) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let phone = device(&transport);
    phone.core.sync_join(manual(s3(STORAGE_SECRET), &sync_key), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    let id = desktop.core.state().entries[0].id;
    let patch = EntryPatch { color: Some(crate::AccountColor::Teal), mark: Some("GH".into()), ..EntryPatch::default() };
    desktop.core.update_entry(id, patch).unwrap();
    advance(SYNC_DEBOUNCE).await;
    phone.core.sync_now().unwrap();
    settle().await;
    let seen = phone.core.state().entries[0].clone();
    assert_eq!((seen.color, seen.mark.as_deref()), (crate::AccountColor::Teal, Some("GH")));
}

/// A folder a cloud drive keeps in sync, made under `h`'s directory, named as the core keeps a
/// chosen one (every link resolved: macOS keeps temporary folders under a link).
fn drive_folder(h: &Harness, name: &str) -> std::path::PathBuf {
    let folder = h.dir.path().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    kept(&folder)
}

/// `dir` as the core keeps a chosen folder: every link resolved, Windows' `\\?\` left out.
fn kept(dir: &std::path::Path) -> std::path::PathBuf {
    crate::session::plain_path(std::fs::canonicalize(dir).unwrap())
}

/// What the interface sends for "the folder chosen": no path.
fn chosen_folder() -> StorageConfig {
    StorageConfig::Folder { path: std::path::PathBuf::new() }
}

/// A device with an account that set up a space in the folder `folder`, chosen through the
/// dialog; the sync key.
async fn folder_device(transport: &Arc<FakeTransport>, folder: &std::path::Path) -> (Harness, String) {
    let h = device(transport);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    h.core.sync_choose_folder(folder).unwrap();
    let created = h.core.sync_create(chosen_folder(), pw(MASTER), "Windows".into()).await.unwrap();
    settle().await;
    (h, created.sync_key)
}

#[tokio::test(start_paused = true)]
async fn a_space_goes_in_the_folder_the_dialog_chose_and_never_where_the_interface_says() {
    let transport = Arc::new(FakeTransport::default());
    let h = device(&transport);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    // Nothing chosen yet.
    assert_eq!(code_err(h.core.sync_create(chosen_folder(), pw(MASTER), "Windows".into()).await), ErrorCode::SyncFolderNotChosen);
    // A path the interface names is never taken, chosen folder or not.
    let folder = drive_folder(&h, "OneDrive/Lockra");
    let named = StorageConfig::Folder { path: folder.clone() };
    assert_eq!(code_err(h.core.sync_create(named.clone(), pw(MASTER), "Windows".into()).await), ErrorCode::Internal);
    // Only a folder that is there, named in full.
    assert_eq!(code_err(h.core.sync_choose_folder(std::path::Path::new("OneDrive/Lockra"))), ErrorCode::SyncConfigInvalid);
    assert_eq!(code_err(h.core.sync_choose_folder(&h.dir.path().join("missing"))), ErrorCode::SyncFolderMissing);
    h.core.sync_choose_folder(&folder).unwrap();
    assert_eq!(code_err(h.core.sync_create(named, pw(MASTER), "Windows".into()).await), ErrorCode::Internal);
    h.core.sync_create(chosen_folder(), pw(MASTER), "Windows".into()).await.unwrap();
    settle().await;
    assert_eq!(space(&h).storage, StorageView::Folder { path: folder.display().to_string() });
    assert!(matches!(space(&h).status, SyncStatus::Synced { .. }), "{:?}", space(&h).status);
    assert_eq!(transport.store(&StorageConfig::Folder { path: folder }).paths().len(), 1);
}

/// A folder chosen through a link (`~/Dropbox` pointing at another disk) is kept as the folder it
/// leads to: a link put in its place later is no longer the folder.
#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn a_folder_chosen_through_a_link_is_kept_as_the_folder_it_leads_to() {
    let transport = Arc::new(FakeTransport::default());
    let h = device(&transport);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    let disk = drive_folder(&h, "disk/Dropbox");
    let link = h.dir.path().join("Dropbox");
    std::os::unix::fs::symlink(&disk, &link).unwrap();
    h.core.sync_choose_folder(&link).unwrap();
    h.core.sync_create(chosen_folder(), pw(MASTER), "Linux".into()).await.unwrap();
    settle().await;
    assert_eq!(space(&h).storage, StorageView::Folder { path: kept(&disk).display().to_string() });
}

#[tokio::test(start_paused = true)]
async fn a_folder_space_invites_with_its_key_alone_and_each_device_reaches_it_its_own_way() {
    let transport = Arc::new(FakeTransport::default());
    let shared = tempfile::tempdir().unwrap();
    let folder = shared.path().join("Jianguoyun");
    std::fs::create_dir(&folder).unwrap();
    let folder = kept(&folder);
    let (windows, sync_key) = folder_device(&transport, &folder).await;
    let invite = windows.core.sync_invite(Some(pw(MASTER)), None).await.unwrap();
    assert!(!invite.includes_storage);
    assert_eq!(lockra_sync::Invite::from_text(&invite.invite).unwrap().storage, None, "no path and no storage in the text");
    assert_eq!(invite.sync_key, sync_key);

    // A phone given the invitation alone is asked how it reaches the space, before any password
    // is checked; then it joins over the drive's WebDAV, which shows the same folder.
    let phone = device(&transport);
    let invited = |storage: Option<StorageConfig>| JoinSource::Invite { text: pw(&invite.invite), code: None, storage };
    assert_eq!(code_err(phone.core.sync_join(invited(None), pw("any password"), "Phone".into(), None).await), ErrorCode::SyncInviteNeedsStorage);
    assert_eq!(phone.core.state().phase, Phase::NoVault);
    transport.share(&webdav(), &StorageConfig::Folder { path: folder.clone() });
    phone.core.sync_join(invited(Some(webdav())), pw(MASTER), "Phone".into(), None).await.unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub"]);

    // Another computer with the same drive chooses its own copy of the folder.
    let mac = device(&transport);
    mac.core.create_vault(pw(MASTER)).await.unwrap();
    mac.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    mac.core.sync_choose_folder(&folder).unwrap();
    mac.core.sync_join(invited(Some(chosen_folder())), pw(MASTER), "MacBook".into(), None).await.unwrap();
    settle().await;
    assert_eq!(issuers(&mac), ["GitHub", "Mail"]);
    phone.core.sync_now().unwrap();
    settle().await;
    assert_eq!(issuers(&phone), ["GitHub", "Mail"]);

    // A space on a storage of its own still invites with it.
    let (desktop, _) = first_device(&transport, s3(STORAGE_SECRET)).await;
    let full = desktop.core.sync_invite(Some(pw(MASTER)), None).await.unwrap();
    assert!(full.includes_storage);
    assert_eq!(lockra_sync::Invite::from_text(&full.invite).unwrap().storage, Some(s3(STORAGE_SECRET)));
}

#[tokio::test(start_paused = true)]
async fn a_folder_is_looked_at_every_fifteen_seconds_in_front_and_every_minute_behind() {
    let transport = Arc::new(FakeTransport::default());
    let parent = tempfile::tempdir().unwrap();
    let (windows, _) = folder_device(&transport, parent.path()).await;
    windows.core.set_settings(Settings { auto_lock_minutes: 0, ..windows.core.state().settings }).unwrap();
    let store = transport.store(&StorageConfig::Folder { path: kept(parent.path()) });
    let calls = || store.calls().len();
    let quiet = calls();
    advance(SYNC_INTERVAL_FOLDER_FOREGROUND - Duration::from_secs(1)).await;
    assert_eq!(calls(), quiet, "nothing before fifteen seconds");
    advance(Duration::from_secs(2)).await;
    assert!(calls() > quiet, "in front: every fifteen seconds");
    windows.core.set_foreground(false);
    advance(SYNC_INTERVAL_FOLDER_FOREGROUND).await;
    let behind = calls();
    advance(SYNC_INTERVAL_FOLDER - Duration::from_secs(1)).await;
    assert_eq!(calls(), behind, "behind: a minute");
    advance(Duration::from_secs(2)).await;
    assert!(calls() > behind);
    // Back in front (the run that brings comes half a minute after the last), a folder gone
    // missing is told as such, and looked at again a minute later, in front too.
    windows.core.set_foreground(true);
    advance(SYNC_FOCUS_MIN + Duration::from_secs(1)).await;
    store.fail_next(SyncError::FolderMissing);
    windows.core.sync_now().unwrap();
    settle().await;
    assert!(matches!(space(&windows).status, SyncStatus::Failed { code: ErrorCode::SyncFolderMissing, .. }), "{:?}", space(&windows).status);
    let failed = calls();
    advance(SYNC_INTERVAL_FOLDER - Duration::from_secs(1)).await;
    assert_eq!(calls(), failed, "a failed run waits a minute, in front too");
    advance(Duration::from_secs(2)).await;
    assert!(matches!(space(&windows).status, SyncStatus::Synced { .. }), "{:?}", space(&windows).status);
}

#[tokio::test(start_paused = true)]
async fn a_space_moves_into_the_drives_folder_that_holds_it() {
    let transport = Arc::new(FakeTransport::default());
    let (desktop, _) = first_device(&transport, webdav()).await;
    let folder = drive_folder(&desktop, "Nextcloud");
    // The folder holds nothing of the space yet: refused.
    desktop.core.sync_choose_folder(&folder).unwrap();
    assert_eq!(code_err(desktop.core.sync_set_storage(chosen_folder(), pw(MASTER)).await), ErrorCode::SyncSpaceNotFound);
    // The drive's client brought it down: taken.
    transport.share(&StorageConfig::Folder { path: folder.clone() }, &webdav());
    desktop.core.sync_set_storage(chosen_folder(), pw(MASTER)).await.unwrap();
    settle().await;
    assert_eq!(space(&desktop).storage, StorageView::Folder { path: folder.display().to_string() });
    assert!(matches!(space(&desktop).status, SyncStatus::Synced { .. }), "{:?}", space(&desktop).status);
}

/// The `list` calls a storage has had: one per run.
fn runs_of(store: &MemoryRemote) -> usize {
    store.calls().iter().filter(|call| call.starts_with("list")).count()
}

#[tokio::test(start_paused = true)]
async fn a_change_the_drive_brings_into_the_folder_runs_a_sync_within_a_second() {
    let transport = Arc::new(FakeTransport::default());
    let parent = tempfile::tempdir().unwrap();
    let folder = StorageConfig::Folder { path: kept(parent.path()) };
    let (windows, _) = folder_device(&transport, parent.path()).await;
    windows.core.set_settings(Settings { auto_lock_minutes: 0, ..windows.core.state().settings }).unwrap();
    windows.core.set_foreground(false);
    advance(SYNC_INTERVAL_FOLDER_FOREGROUND).await;
    let store = transport.store(&folder);
    let quiet = runs_of(&store);
    // The drive writes a few files within a second: one run follows, a second after the first.
    for _ in 0..3 {
        transport.touch(&folder);
        advance(SYNC_FOLDER_SETTLE / 4).await;
    }
    assert_eq!(runs_of(&store), quiet, "not at once");
    advance(SYNC_FOLDER_SETTLE).await;
    assert_eq!(runs_of(&store), quiet + 1, "one run for the burst");
    // Nothing more until the next look, a minute on.
    advance(SYNC_INTERVAL_FOLDER - SYNC_FOLDER_SETTLE * 2).await;
    assert_eq!(runs_of(&store), quiet + 1);
}

#[tokio::test(start_paused = true)]
async fn the_folder_is_watched_only_while_its_space_is_open_here() {
    let transport = Arc::new(FakeTransport::default());
    let parent = tempfile::tempdir().unwrap();
    let folder = StorageConfig::Folder { path: kept(parent.path()) };
    let (windows, _) = folder_device(&transport, parent.path()).await;
    windows.core.set_settings(Settings { auto_lock_minutes: 0, ..windows.core.state().settings }).unwrap();
    assert_eq!(transport.watching(&folder), 1, "watched once the space is set up");
    windows.core.lock_vault();
    assert_eq!(transport.watching(&folder), 0, "locked: not watched");
    let store = transport.store(&folder);
    let locked = runs_of(&store);
    transport.touch(&folder);
    advance(SYNC_FOLDER_SETTLE * 2).await;
    assert_eq!(runs_of(&store), locked, "locked: nothing runs");
    windows.core.unlock(pw(MASTER)).await.unwrap();
    settle().await;
    assert_eq!(transport.watching(&folder), 1, "watched again once unlocked");
    // Moved to a storage of its own: that one tells nothing, the folder is no longer watched.
    transport.share(&webdav(), &folder);
    windows.core.sync_set_storage(webdav(), pw(MASTER)).await.unwrap();
    settle().await;
    assert_eq!(transport.watching(&folder), 0);
    // Back into the folder, then sync turned off here.
    windows.core.sync_choose_folder(parent.path()).unwrap();
    windows.core.sync_set_storage(chosen_folder(), pw(MASTER)).await.unwrap();
    settle().await;
    assert_eq!(transport.watching(&folder), 1);
    windows.core.sync_disable().unwrap();
    assert_eq!(transport.watching(&folder), 0);
}
