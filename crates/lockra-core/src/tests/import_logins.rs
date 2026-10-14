//! Logins through the import (docs/formats.md §11): the exports of browsers, password managers
//! and KeePass in the preview and committed, a KeePass database waiting for its password and key
//! file, a backup's logins merged with their codes linked through the map, and all of it only
//! where password management is on.

use std::collections::BTreeSet;
use std::io::Write as _;

use keepass::config::{DatabaseConfig, KdfConfig};
use keepass::db::fields;
use keepass::{Database, DatabaseKey};

use super::*;
use crate::LoginOutcome;
use crate::item::Purpose;
use crate::ui::{AwaitingKind, AwaitingView, ConflictReason};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../lockra-transfer/tests/fixtures");
const KDBX_PASSWORD: &str = "keepass password";
const KEY_FILE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<KeyFile><Meta><Version>2.0</Version></Meta><Key><Data Hash="A65F0C2D">36057B1C 35037FD9 62257893 C0A22403 EE3F8FBB 504D9981 08B821CB 00D28F89</Data></Key></KeyFile>
"#;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(Path::new(FIXTURES).join(name)).unwrap()
}

/// 1Password's zip of the fixture's `export.data`.
fn onepux() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for name in ["export.attributes", "export.data"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(&fixture(&format!("1pux-{name}"))).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A KeePass database of two entries, one with a code, under `key` (a quick key derivation).
fn kdbx(key: DatabaseKey) -> Vec<u8> {
    let mut config = DatabaseConfig::default();
    config.kdf_config = KdfConfig::Aes { rounds: 8 };
    let mut db = Database::with_config(config);
    db.root_mut().add_entry().edit(|e| {
        e.set_unprotected(fields::TITLE, "Forum");
        e.set_unprotected(fields::USERNAME, "alex");
        e.set_protected(fields::PASSWORD, "kdbx-pass-1");
        e.set_unprotected(fields::URL, "https://forum.example.org/");
        e.set_protected(fields::OTP, "otpauth://totp/Forum:alex?secret=MFRGGZDF");
    });
    {
        let mut root = db.root_mut();
        let mut work = root.add_group();
        work.name = "Work".into();
        work.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "Wiki");
            e.set_unprotected(fields::USERNAME, "alex.w");
            e.set_protected(fields::PASSWORD, "kdbx-pass-2");
        });
    }
    let mut out = Vec::new();
    db.save(&mut out, key).unwrap();
    out
}

fn kdbx_key() -> DatabaseKey {
    DatabaseKey::new().with_password(KDBX_PASSWORD)
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

/// The logins of the preview: (title, file).
fn login_candidates(h: &Harness) -> Vec<(String, String)> {
    h.core
        .state()
        .import
        .map(|i| i.candidates)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let ImportSource::File { name } = c.source else { return None };
            c.login.map(|l| (l.title, name))
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn the_exports_of_browsers_and_password_managers_go_to_the_preview() {
    let mut h = Harness::unlocked().await;
    let dir = tempfile::tempdir().unwrap();
    let mut paths: Vec<PathBuf> = ["chrome.csv", "firefox.csv", "safari.csv", "bitwarden.csv", "1password.csv", "lastpass.csv", "bitwarden.json"]
        .iter()
        .map(|name| write(dir.path(), name, &fixture(name)))
        .collect();
    paths.push(write(dir.path(), "export.1pux", &onepux()));
    paths.push(write(dir.path(), "readme.zip", &{
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap().into_inner()
    }));
    h.notices();
    h.core.import_files(paths).await.unwrap();
    let per_file = |file: &str| login_candidates(&h).iter().filter(|(_, f)| f == file).count();
    assert_eq!(
        ["chrome.csv", "firefox.csv", "safari.csv", "bitwarden.csv", "1password.csv", "lastpass.csv", "bitwarden.json", "export.1pux"].map(per_file),
        [3, 2, 2, 2, 2, 2, 3, 3]
    );
    let preview = h.core.state().import.unwrap();
    // Safari's CSV holds otpauth links: read as CSV, its code a candidate after its login.
    let safari = preview.candidates.iter().position(|c| c.login.as_ref().is_some_and(|l| l.title == "example.com (example_user)")).unwrap();
    let code = &preview.candidates[safari + 1];
    assert_eq!((code.issuer.as_str(), code.origin, code.login.is_none()), ("Example", lockra_transfer::Origin::Uri, true));
    assert_eq!(preview.candidates[safari].login.as_ref().unwrap().totp, Some(code.id));
    // What the exports hold beside logins is listed and left out.
    let left: Vec<lockra_transfer::RejectReason> = preview
        .candidates
        .iter()
        .filter_map(|c| match c.status {
            CandidateStatus::Unsupported { reason } if c.login.is_none() && c.kind.is_none() => Some(reason),
            _ => None,
        })
        .collect();
    assert!(left.contains(&lockra_transfer::RejectReason::CardItem) && left.contains(&lockra_transfer::RejectReason::NoteItem), "{left:?}");
    assert_eq!(h.notices(), [Notice::FileUnrecognized { name: "readme.zip".into() }]);
    // Nothing secret in the preview.
    let state = serde_json::to_string(&h.core.state()).unwrap();
    for secret in ["chrome-pass-1", "bw-json-pass-1", "op-pux-pass-1", "lp-pass-1", "4242", "Gift cards"] {
        assert!(!state.contains(secret), "{secret} in the state");
    }
    let outcome = h.core.import_commit(&[]).unwrap();
    assert_eq!(outcome.logins.added, 19);
    let notices = h.notices();
    assert!(notices.iter().any(|n| matches!(n, Notice::Imported { logins: LoginOutcome { added: 19, .. }, .. })), "{notices:?}");
    let items = h.core.items_list().unwrap();
    // Bitwarden's JSON: its seven URLs with their rules, the re-prompt, the code linked.
    let github = items.iter().find(|i| i.title == "GitHub" && i.urls.len() == 7).unwrap();
    assert_eq!((github.source.as_deref(), github.protection), (Some("bitwarden"), crate::Protection::Confirm));
    assert_eq!(github.urls[1].matching, crate::Matching::Host);
    // Safari's example, Bitwarden's CSV and its JSON share one secret: one account, the first's.
    let linked = github.totp_entry.unwrap();
    assert!(h.core.state().entries.iter().any(|e| e.id == linked && e.issuer == "Example"));
    let safari = items.iter().find(|i| i.source.as_deref() == Some("safari") && i.title.starts_with("example.com")).unwrap();
    assert_eq!(safari.totp_entry, Some(linked));
    let sources: BTreeSet<Option<String>> = items.iter().map(|i| i.source.clone()).collect();
    let expected: BTreeSet<Option<String>> =
        ["chrome", "firefox", "safari", "bitwarden", "1password", "lastpass"].iter().map(|s| Some((*s).to_owned())).collect();
    assert_eq!(sources, expected);
}

#[tokio::test(start_paused = true)]
async fn without_password_management_the_exports_read_as_they_did() {
    let mut h = harness_with_features(Features { passwords: false });
    h.core.create_vault(pw(MASTER)).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        write(dir.path(), "safari.csv", &fixture("safari.csv")),
        write(dir.path(), "chrome.csv", &fixture("chrome.csv")),
        write(dir.path(), "export.1pux", &onepux()),
        write(dir.path(), "db.kdbx", &kdbx(kdbx_key())),
    ];
    h.notices();
    h.core.import_files(paths).await.unwrap();
    let preview = h.core.state().import.unwrap();
    // Safari's file is a text with links, as before: no line of it is an otpauth URI alone.
    assert!(preview.candidates.iter().all(|c| c.login.is_none() && matches!(c.status, CandidateStatus::Unsupported { .. })));
    assert!(preview.candidates.iter().all(|c| matches!(&c.source, ImportSource::File { name } if name == "safari.csv")));
    assert_eq!(preview.awaiting, None);
    let notices = h.notices();
    for name in ["chrome.csv", "export.1pux", "db.kdbx"] {
        assert!(notices.contains(&Notice::FileUnrecognized { name: name.into() }), "{name}: {notices:?}");
    }
    assert_eq!(code_err(h.core.import_awaits_key_file()), ErrorCode::FeatureUnavailable);
    assert_eq!(code_err(h.core.import_key_file(PickedFile { name: "k".into(), bytes: Zeroizing::new(b"key".to_vec()) }).await), ErrorCode::FeatureUnavailable);
    assert_eq!(code_err(h.core.import_key_file_clear()), ErrorCode::FeatureUnavailable);
}

#[tokio::test(start_paused = true)]
async fn a_keepass_database_waits_for_its_password_and_its_key_file() {
    let mut h = Harness::unlocked().await;
    let dir = tempfile::tempdir().unwrap();
    let db = write(dir.path(), "Passwords.kdbx", &kdbx(kdbx_key().with_keyfile(&mut KEY_FILE.as_bytes()).unwrap()));
    let key_file = write(dir.path(), "Passwords.keyx", KEY_FILE.as_bytes());
    let other_key = write(dir.path(), "photo.jpg", b"not the key file, but a file all the same");
    assert_eq!(code_err(h.core.import_awaits_key_file()), ErrorCode::NoImport);
    h.core.import_files(vec![db]).await.unwrap();
    let awaiting = |h: &Harness| h.core.state().import.unwrap().awaiting;
    assert_eq!(awaiting(&h), Some(AwaitingView { name: "Passwords.kdbx".into(), kind: AwaitingKind::Kdbx, key_file: None }));
    h.core.import_awaits_key_file().unwrap();
    // Without its key file the password alone is wrong.
    assert_eq!(code_err(h.core.import_backup_password(pw(KDBX_PASSWORD)).await), ErrorCode::WrongPassword);
    // A key file that does not read is refused at once; another file is a key, the wrong one.
    let broken = KEY_FILE.replace("A65F0C2D", "00000000");
    assert_eq!(
        code_err(h.core.import_key_file(PickedFile { name: "broken.keyx".into(), bytes: Zeroizing::new(broken.into_bytes()) }).await),
        ErrorCode::ImportKeyFileInvalid
    );
    assert_eq!(code_err(h.core.import_key_file(PickedFile { name: "empty".into(), bytes: Zeroizing::new(Vec::new()) }).await), ErrorCode::ImportKeyFileInvalid);
    assert_eq!(code_err(h.core.import_key_file_from(dir.path().join("missing.keyx")).await), ErrorCode::ImportKeyFileInvalid);
    assert_eq!(awaiting(&h).unwrap().key_file, None);
    h.core.import_key_file_from(other_key).await.unwrap();
    assert_eq!(awaiting(&h).unwrap().key_file.as_deref(), Some("photo.jpg"));
    assert_eq!(code_err(h.core.import_backup_password(pw(KDBX_PASSWORD)).await), ErrorCode::ImportKeyFileInvalid);
    h.core.import_key_file_clear().unwrap();
    assert_eq!(awaiting(&h).unwrap().key_file, None);
    h.core.import_key_file_from(key_file).await.unwrap();
    assert_eq!(code_err(h.core.import_backup_password(pw("not it")).await), ErrorCode::ImportKeyFileInvalid);
    h.core.import_backup_password(pw(KDBX_PASSWORD)).await.unwrap();
    let preview = h.core.state().import.unwrap();
    assert_eq!(preview.awaiting, None);
    let logins: Vec<String> = login_candidates(&h).into_iter().map(|(title, _)| title).collect();
    assert_eq!(logins.len(), 2, "{logins:?}");
    // Not secret, the key file's name; the file itself stays out of the state.
    assert!(!serde_json::to_string(&h.core.state()).unwrap().contains("36057B1C"));
    h.notices();
    let outcome = h.core.import_commit(&[]).unwrap();
    assert_eq!((outcome.added, outcome.logins.added), (1, 2));
    let forum = h.core.items_list().unwrap().into_iter().find(|i| i.title == "Forum").unwrap();
    assert_eq!((forum.source.as_deref(), forum.username.as_str()), (Some("kdbx"), "alex"));
    assert!(forum.totp_entry.is_some());
    assert_eq!(h.core.items_list().unwrap().iter().find(|i| i.title == "Wiki").unwrap().group.as_deref(), Some("Work"));
    // Nothing waits any more: the key file commands say so.
    assert_eq!(code_err(h.core.import_key_file_clear()), ErrorCode::NoImport);
}

#[tokio::test(start_paused = true)]
async fn a_keepass_database_lockra_will_not_open_is_said_at_once() {
    let mut h = Harness::unlocked().await;
    let dir = tempfile::tempdir().unwrap();
    // KDBX 3.1: the signature, version 3, and a header Lockra does not read.
    let old = write(dir.path(), "old.kdbx", &[&lockra_transfer::kdbx::MAGIC[..], b"\x01\x00\x03\x00\x02\x10\x00rest"].concat());
    // A database whose AES-KDF asks for 2^64 - 1 rounds: its header says so, before any work.
    let mut costly = kdbx(kdbx_key());
    let rounds = costly.windows(10).position(|w| w == [0x05, 1, 0, 0, 0, b'R', 8, 0, 0, 0]).unwrap() + 10;
    costly[rounds..rounds + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    let costly = write(dir.path(), "costly.kdbx", &costly);
    h.notices();
    assert_eq!(code_err(h.core.import_files(vec![old, costly]).await), ErrorCode::ImportEmpty);
    let notices = h.notices();
    assert!(notices.contains(&Notice::FileUnreadable { name: "old.kdbx".into() }), "{notices:?}");
    assert!(notices.contains(&Notice::FileUnreadable { name: "costly.kdbx".into() }), "{notices:?}");
}

/// A vault with an account and a login linked to it, backed up.
async fn backup_with_a_linked_login() -> (Harness, PathBuf, Uuid) {
    let source = Harness::unlocked().await;
    let entry = source.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let draft = crate::ItemDraft {
        title: "GitHub".into(),
        username: "octocat".into(),
        password: crate::SecretChange::Set { value: pw("from the backup") },
        urls: vec![crate::UrlDraft { url: "https://github.com/".into(), matching: crate::Matching::Domain }],
        totp_entry: Some(entry),
        ..crate::ItemDraft::default()
    };
    let item = source.core.item_add(draft).unwrap();
    let file = source.dir.path().join("with-logins.lockrabackup");
    source.core.backup_to(file.clone(), None).await.unwrap();
    (source, file, item)
}

#[tokio::test(start_paused = true)]
async fn a_merged_restore_links_each_login_to_its_code_s_new_account() {
    let (source, file, item) = backup_with_a_linked_login().await;
    let backup_entry = source.core.state().entries[0].id;
    let h = Harness::unlocked().await;
    h.core.restore_open(file.clone()).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    let preview = h.core.state().import.unwrap();
    assert_eq!(preview.candidates.len(), 2);
    assert_eq!(preview.candidates[1].login.as_ref().map(|l| (l.title.as_str(), l.totp)), Some(("GitHub", Some(preview.candidates[0].id))));
    assert_eq!(preview.candidates[1].status, CandidateStatus::New);
    let outcome = h.core.import_commit(&[]).unwrap();
    assert_eq!((outcome.added, outcome.logins), (1, LoginOutcome { added: 1, replaced: 0, skipped: 0 }));
    let entry = h.core.state().entries[0].id;
    let login = &h.core.items_list().unwrap()[0];
    assert_ne!((entry, login.id), (backup_entry, item), "new ids");
    assert_eq!(login.totp_entry, Some(entry), "the code's new account, not the backup's id");
    // Into a vault that holds the account already (another id, the same secret): linked to it.
    let other = Harness::unlocked().await;
    let own = other.core.add_uri(&otpauth("Renamed", "me", SECRET)).unwrap();
    other.core.restore_open(file).await.unwrap();
    other.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    assert_eq!(other.core.state().import.unwrap().candidates[0].status, CandidateStatus::Exists { entry_id: own });
    other.core.import_commit(&[]).unwrap();
    assert_eq!(other.core.items_list().unwrap()[0].totp_entry, Some(own));
    assert_eq!(other.core.state().entries.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_merged_restore_of_the_vault_s_own_logins_says_which_changed() {
    let (source, file, item) = backup_with_a_linked_login().await;
    // The same vault later: the password changed here after the backup.
    source
        .core
        .item_update(item, crate::ItemPatch { password: crate::SecretChange::Set { value: pw("changed here") }, ..crate::ItemPatch::default() })
        .unwrap();
    source.core.restore_open(file).await.unwrap();
    source.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    let preview = source.core.state().import.unwrap();
    assert_eq!(preview.candidates[1].status, CandidateStatus::LoginConflict { item_id: item, reason: ConflictReason::Changed });
    // Replacing (the default) takes the backup's version back; the password of here goes into
    // the history.
    assert_eq!(source.core.import_commit(&[]).unwrap().logins.replaced, 1);
    let data = source.core.data();
    let login = data.items.iter().find(|i| i.id == item).unwrap();
    assert_eq!(login.field_of(Purpose::Password).map(|f| f.value.as_str()), Some("from the backup"));
    assert!(login.history.iter().any(|p| p.value.as_str() == "changed here"));
    assert_eq!(data.items.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_backup_in_the_import_brings_its_logins_or_waits_without_password_management() {
    let (_source, file, _) = backup_with_a_linked_login().await;
    let h = Harness::unlocked().await;
    h.core.import_files(vec![file.clone()]).await.unwrap();
    h.core.import_backup_password(pw(MASTER)).await.unwrap();
    assert_eq!(login_candidates(&h).len(), 1);
    let off = harness_with_features(Features { passwords: false });
    off.core.create_vault(pw(MASTER)).await.unwrap();
    off.core.import_files(vec![file]).await.unwrap();
    assert_eq!(code_err(off.core.import_backup_password(pw(MASTER)).await), ErrorCode::FeatureUnavailable);
    let preview = off.core.state().import.unwrap();
    assert!(preview.awaiting.is_some() && preview.candidates.is_empty(), "not half imported: the backup still waits");
}

#[tokio::test(start_paused = true)]
async fn an_import_that_brings_logins_has_the_list_asked_for_again() {
    let h = Harness::unlocked().await;
    let dir = tempfile::tempdir().unwrap();
    h.core.import_files(vec![write(dir.path(), "chrome.csv", &fixture("chrome.csv"))]).await.unwrap();
    let before = h.core.state().items_rev;
    h.core.import_commit(&[]).unwrap();
    assert!(h.core.state().items_rev > before, "the webview fetches items_list again");
    // An import of codes alone changes no login: the list stays as it was.
    h.core.import_text(&otpauth("Only", "codes", "MFRGGZDF")).unwrap();
    let before = h.core.state().items_rev;
    h.core.import_commit(&[]).unwrap();
    assert_eq!(h.core.state().items_rev, before);
}
