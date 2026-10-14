//! Logins through the core before their commands exist: restores of backups that hold them, the
//! feature switch, and two devices syncing them (the payload is changed with `Core::edit_data`).

use lockra_sync::{Hlc, Record, SyncState};
use zeroize::Zeroizing;

use super::*;
use crate::StorageConfig;
use crate::item::{Purpose, SiteRule, SiteRuleKind, VaultItem, random_write_id};
use crate::ui::JoinSource;
use crate::{SYNC_DEBOUNCE, fakes::FakeTransport};

const USERNAME: Uuid = Uuid::from_u128(0x1);
const PASSWORD: Uuid = Uuid::from_u128(0x2);

/// A login made on `data`'s clock at `now_ms`, with `password`.
fn login(data: &mut crate::VaultData, title: &str, password: &str, now_ms: u64) -> VaultItem {
    let (stamp, mwid) = (data.tick(now_ms), random_write_id());
    let mut item = VaultItem::login(Uuid::new_v4(), now_ms, stamp, mwid);
    item.title = title.into();
    let (stamp, wid) = (data.tick(now_ms), random_write_id());
    item.write_field(USERNAME, Purpose::Username, stamp, wid, now_ms, |f| f.value = Zeroizing::new("octocat".into()));
    set_password(data, &mut item, password, now_ms);
    item
}

fn set_password(data: &mut crate::VaultData, item: &mut VaultItem, password: &str, now_ms: u64) {
    let (stamp, wid) = (data.tick(now_ms), random_write_id());
    item.write_field(PASSWORD, Purpose::Password, stamp, wid, now_ms, |f| {
        f.value = Zeroizing::new(password.into());
        f.hidden = true;
    });
}

fn add_login(h: &Harness, title: &str, password: &str) -> Uuid {
    let mut id = Uuid::nil();
    h.core.edit_data(|data| {
        let item = login(data, title, password, T0);
        id = item.id;
        data.items.push(item);
    });
    id
}

fn passwords(h: &Harness) -> Vec<String> {
    let mut passwords: Vec<String> = h.core.data().items.iter().map(|i| i.field(PASSWORD).map(|f| f.value.to_string()).unwrap_or_default()).collect();
    passwords.sort();
    passwords
}

/// A desktop that set up a space on a relay and a phone that joined it, both synced once.
pub(super) async fn two_devices() -> (Harness, Harness) {
    let transport = Arc::new(FakeTransport::default());
    let storage = StorageConfig::Relay { url: "https://lockra-relay.onethinker.top".into() };
    let desktop = harness_on(FakeUpdater::default(), Arc::clone(&transport) as Arc<dyn SyncTransport>);
    desktop.core.create_vault(pw(MASTER)).await.unwrap();
    let created = desktop.core.sync_create(storage.clone(), pw(MASTER), "Desktop".into()).await.unwrap();
    settle().await;
    let phone = harness_on(FakeUpdater::default(), Arc::clone(&transport) as Arc<dyn SyncTransport>);
    let join = JoinSource::Manual { storage, sync_key: pw(&created.sync_key) };
    phone.core.sync_join(join, Some(pw(MASTER)), "Phone".into(), None, None).await.unwrap();
    settle().await;
    (desktop, phone)
}

fn never_save(data: &mut crate::VaultData, domain: &str) -> SiteRule {
    SiteRule {
        id: SiteRule::single_id(SiteRuleKind::NeverSave, domain),
        kind: SiteRuleKind::NeverSave.into(),
        domains: vec![domain.into()],
        stamp: data.tick(T0),
        extra: Default::default(),
    }
}

#[tokio::test(start_paused = true)]
async fn the_state_says_which_features_this_build_offers() {
    let h = Harness::unlocked().await;
    assert!(h.core.state().features.passwords, "on in the tests");
    assert_eq!(crate::ui::Features::default().passwords, cfg!(debug_assertions), "a release build turns it on when it ships");
}

#[tokio::test(start_paused = true)]
async fn a_merged_restore_of_a_backup_with_logins_is_refused_and_stays_open_for_a_replace() {
    let source = Harness::unlocked().await;
    source.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    add_login(&source, "GitHub", "in the backup");
    let file = source.dir.path().join("with-logins.lockrabackup");
    source.core.backup_to(file.clone(), None).await.unwrap();

    let h = Harness::unlocked().await;
    h.core.add_uri(&otpauth("Mine", "m", "MFRGGZDF")).unwrap();
    h.core.restore_open(file.clone()).await.unwrap();
    // Not half restored: the codes would arrive and the logins not, and say nothing.
    assert_eq!(code_err(h.core.restore_commit(pw(MASTER), RestoreMode::Merge).await), ErrorCode::FeatureUnavailable);
    let state = h.core.state();
    assert!(state.restore.is_some(), "still open, to restore by replacing");
    assert!(state.import.is_none(), "nothing went to the preview");
    assert!(h.core.data().items.is_empty());
    // Replacing takes everything, the logins with the codes.
    h.core.restore_commit(pw(MASTER), RestoreMode::Replace).await.unwrap();
    assert_eq!(h.core.state().entries.iter().map(|e| e.issuer.as_str()).collect::<Vec<_>>(), ["GitHub"]);
    assert_eq!(passwords(&h), ["in the backup"]);
    // A backup's deletions of logins count as logins: refused too.
    let deleted = Harness::unlocked().await;
    let id = add_login(&deleted, "Gone", "x");
    deleted.core.edit_data(|data| {
        data.items.retain(|i| i.id != id);
        let stamp = data.tick(T0 + 1);
        data.item_tombstones.push(lockra_sync::Tombstone { id, stamp, counter: None });
    });
    let only_deletion = deleted.dir.path().join("deletion.lockrabackup");
    deleted.core.backup_to(only_deletion.clone(), None).await.unwrap();
    h.core.restore_open(only_deletion).await.unwrap();
    assert_eq!(code_err(h.core.restore_commit(pw(MASTER), RestoreMode::Merge).await), ErrorCode::FeatureUnavailable);
    // Into a new installation the backup becomes the vault, logins and all.
    let fresh = harness();
    fresh.core.restore_open(file).await.unwrap();
    fresh.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    assert_eq!(passwords(&fresh), ["in the backup"]);
}

#[tokio::test(start_paused = true)]
async fn a_replacing_restore_of_a_backup_without_logins_leaves_them_alone() {
    let source = Harness::unlocked().await;
    source.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    let file = source.dir.path().join("codes-only.lockrabackup");
    source.core.backup_to(file.clone(), None).await.unwrap();

    let h = Harness::unlocked().await;
    add_login(&h, "Bank", "kept");
    h.core.edit_data(|data| {
        let rule = never_save(data, "bank.example");
        data.site_rules.push(rule);
    });
    h.core.restore_open(file).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Replace).await.unwrap();
    assert_eq!(h.core.state().entries.iter().map(|e| e.issuer.as_str()).collect::<Vec<_>>(), ["A"], "the codes are the backup's");
    assert_eq!(passwords(&h), ["kept"], "a backup of format 2 holds no logins: this vault's stay");
    assert_eq!(h.core.data().site_rules.len(), 1);
    assert!(h.core.data().item_tombstones.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_merged_restore_takes_the_backups_site_rules_quietly() {
    let source = Harness::unlocked().await;
    source.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    source.core.edit_data(|data| {
        let rule = never_save(data, "example.com");
        data.site_rules.push(rule);
    });
    let file = source.dir.path().join("rules.lockrabackup");
    source.core.backup_to(file.clone(), None).await.unwrap();

    let h = Harness::unlocked().await;
    h.core.edit_data(|data| {
        let rule = never_save(data, "other.example");
        data.site_rules.push(rule);
    });
    h.core.restore_open(file).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    assert!(h.core.state().import.is_some(), "the codes go to the preview");
    let mut domains: Vec<String> = h.core.data().site_rules.iter().flat_map(|r| r.domains.clone()).collect();
    domains.sort();
    assert_eq!(domains, ["example.com", "other.example"], "the rules join, nothing asked");
    // Written: they are there after a restart.
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    let st = restarted.state();
    assert_eq!(st.phase, Phase::Unlocked);
}

#[tokio::test(start_paused = true)]
async fn logins_sync_part_by_part_and_say_what_came() {
    let (mut desktop, mut phone) = two_devices().await;
    let _ = (desktop.notices(), phone.notices());
    let id = add_login(&desktop, "GitHub", "first");
    advance(SYNC_DEBOUNCE).await;
    phone.core.sync_now().unwrap();
    settle().await;
    assert_eq!(passwords(&phone), ["first"]);
    let notices: Vec<Notice> = phone.notices().into_iter().filter(|n| matches!(n, Notice::SyncBrought { .. })).collect();
    assert_eq!(notices, [Notice::SyncBrought { added: 0, updated: 0, removed: 0, logins: 1, devices: vec!["Desktop".into()] }]);

    // The notes change on the desktop while the password changes on the phone.
    desktop.core.edit_data(|data| {
        let now = T0 + 10_000;
        let (stamp, mwid) = (data.tick(now), random_write_id());
        let item = data.items.iter_mut().find(|i| i.id == id).unwrap();
        item.notes = Zeroizing::new("recovery codes in the safe".into());
        item.meta_written(stamp, mwid, now);
    });
    phone.core.edit_data(|data| {
        let mut item = data.items.iter().find(|i| i.id == id).unwrap().clone();
        set_password(data, &mut item, "second", T0 + 11_000);
        data.items.retain(|i| i.id != id);
        data.items.push(item);
    });
    advance(SYNC_DEBOUNCE).await;
    settle().await;
    desktop.core.sync_now().unwrap();
    settle().await;
    phone.core.sync_now().unwrap();
    settle().await;
    for h in [&desktop, &phone] {
        let data = h.core.data();
        let item = data.items.iter().find(|i| i.id == id).unwrap();
        assert_eq!(item.field(PASSWORD).unwrap().value.as_str(), "second", "the password stays");
        assert_eq!(item.notes.as_str(), "recovery codes in the safe", "and the notes");
        assert!(!item.conflict());
    }
    assert_eq!(desktop.core.data().items, phone.core.data().items);
    let _ = desktop.notices();
}

#[tokio::test(start_paused = true)]
async fn a_device_updated_in_place_reads_the_snapshots_it_had_already_seen() {
    let (desktop, phone) = two_devices().await;
    add_login(&desktop, "GitHub", "from the desktop");
    advance(SYNC_DEBOUNCE).await;
    settle().await;
    phone.core.sync_now().unwrap();
    settle().await;
    // As 0.9 left it: the desktop's snapshot taken in at its etag, its logins ignored, its state
    // saved without a reader. The desktop writes nothing new.
    phone.core.edit_data(|data| {
        data.items.clear();
        let sync = data.sync_mut().unwrap();
        let mut state: serde_json::Value = serde_json::to_value(&sync.state).unwrap();
        state.as_object_mut().unwrap().remove("reader");
        sync.state = serde_json::from_value::<SyncState>(state).unwrap();
        assert_eq!(sync.state.reader, 0);
        assert!(sync.state.seen.values().all(|seen| seen.etag.is_some()));
    });
    phone.core.sync_now().unwrap();
    settle().await;
    assert_eq!(passwords(&phone), ["from the desktop"], "read once more, the logins arrive");
    assert_eq!(phone.core.data().sync().unwrap().state.reader, 2);
}

#[tokio::test(start_paused = true)]
async fn a_login_deleted_on_one_device_goes_on_the_other() {
    let (desktop, phone) = two_devices().await;
    let id = add_login(&desktop, "GitHub", "first");
    advance(SYNC_DEBOUNCE).await;
    settle().await;
    phone.core.sync_now().unwrap();
    settle().await;
    phone.core.edit_data(|data| {
        let stamp = data.tick(T0 + 20_000);
        let item = data.items.iter().find(|i| i.id == id).unwrap();
        assert!(Record::stamp(item) < stamp);
        data.items.retain(|i| i.id != id);
        data.item_tombstones.push(lockra_sync::Tombstone { id, stamp, counter: None });
    });
    advance(SYNC_DEBOUNCE).await;
    settle().await;
    desktop.core.sync_now().unwrap();
    settle().await;
    assert!(desktop.core.data().items.is_empty());
    assert!(desktop.core.data().item_tombstones.iter().any(|t| t.id == id && t.stamp > Hlc::default()));
}
