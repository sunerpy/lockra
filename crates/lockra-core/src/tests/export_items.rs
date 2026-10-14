//! Logins exported to a file (docs/formats.md §11): only where password management is on, only
//! for the user at this device and with a plain file's risk confirmed; a KeePass database or a
//! CSV that the import takes back, linked codes and all; saved for its owner only, and none of
//! it in what the webview hears.

use tokio::sync::broadcast::error::TryRecvError;

use super::*;
use crate::item::Purpose;
use crate::lenient::Lenient;
use crate::logins::{FieldChange, ItemDraft, SecretChange, UrlDraft};
use crate::{ItemsFile, ItemsFormat, Matching, Protection};

const LEAKY: &str = "hunter2-do-not-leak";
const KDBX_PASSWORD: &str = "a database password";

fn set(value: &str) -> SecretChange {
    SecretChange::Set { value: pw(value) }
}

/// A vault with three logins: GitHub with every part and its code linked, one in a group
/// without a code, and one known by its email address alone.
async fn vault_with_logins() -> (Harness, Vec<Uuid>) {
    let h = Harness::unlocked().await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let github = h
        .core
        .item_add(ItemDraft {
            title: "GitHub".into(),
            username: "octocat".into(),
            email: "octocat@example.com".into(),
            password: set(LEAKY),
            urls: vec![
                UrlDraft { url: "https://github.com/login".into(), matching: Matching::Domain },
                UrlDraft { url: "https://gist.github.com".into(), matching: Matching::Host },
            ],
            notes: pw("recovery codes in the safe"),
            tags: vec!["work".into(), "dev".into()],
            group: Some("Dev/Web".into()),
            favorite: true,
            protection: Protection::Confirm,
            totp_entry: Some(entry),
            fields: vec![
                FieldChange { label: Some("PIN".into()), value: set("4242"), hidden: Some(true), ..FieldChange::default() },
                FieldChange { label: Some("Customer no.".into()), value: set("C-77"), hidden: Some(false), ..FieldChange::default() },
            ],
            ..ItemDraft::default()
        })
        .unwrap();
    let wiki = h
        .core
        .item_add(ItemDraft { title: "Wiki".into(), username: "alex".into(), password: set("wiki-pass-2"), group: Some("Dev".into()), ..ItemDraft::default() })
        .unwrap();
    let mail =
        h.core.item_add(ItemDraft { title: "Mail".into(), email: "me@example.com".into(), password: set("mail-pass-3"), ..ItemDraft::default() }).unwrap();
    (h, vec![github, wiki, mail])
}

/// A new vault, `file` imported into it with the import's defaults (opened with `password`, for a
/// KeePass database).
async fn imported(file: &ItemsFile, password: Option<&str>) -> Harness {
    let h = Harness::unlocked().await;
    let path = h.dir.path().join(file.name());
    fs::write(&path, file.bytes()).unwrap();
    h.core.import_files(vec![path]).await.unwrap();
    if let Some(password) = password {
        h.core.import_backup_password(pw(password)).await.unwrap();
    }
    h.core.import_commit(&[]).unwrap();
    h
}

/// What a login of `h` holds, to compare: the custom fields sorted, the code as its URI.
#[derive(Debug, PartialEq)]
struct Parts {
    username: String,
    email: String,
    password: String,
    urls: Vec<String>,
    notes: String,
    tags: Vec<String>,
    group: Option<String>,
    fields: Vec<(String, String, bool)>,
    favorite: bool,
    confirm: bool,
    code: Option<String>,
}

fn parts(h: &Harness, title: &str) -> Parts {
    let data = h.core.data();
    let item = data.items.iter().find(|i| i.title == title).unwrap_or_else(|| panic!("no login {title}"));
    let value = |purpose| item.field_of(purpose).map(|f| f.value.to_string()).unwrap_or_default();
    let mut fields: Vec<(String, String, bool)> = item
        .fields
        .iter()
        .filter(|f| !f.removed && f.purpose == Lenient::Known(Purpose::Custom))
        .map(|f| (f.label.clone(), f.value.to_string(), f.hidden))
        .collect();
    fields.sort();
    Parts {
        username: value(Purpose::Username),
        email: value(Purpose::Email),
        password: value(Purpose::Password),
        urls: item.urls.iter().map(|u| u.url.clone()).collect(),
        notes: item.notes.to_string(),
        tags: item.tags.clone(),
        group: item.group.clone(),
        fields,
        favorite: item.favorite,
        confirm: item.protection == Lenient::Known(Protection::Confirm),
        code: item.totp_entry.and_then(|id| data.get(id)).map(|e| e.to_auth().to_uri().to_string()),
    }
}

fn source_of(h: &Harness, title: &str) -> Option<String> {
    h.core.data().items.iter().find(|i| i.title == title).and_then(|i| i.source.clone())
}

#[tokio::test(start_paused = true)]
async fn without_password_management_there_is_no_export_of_logins() {
    let h = harness_with_features(Features { passwords: false });
    h.core.create_vault(pw(MASTER)).await.unwrap();
    let export = h.core.export_items(&[Uuid::new_v4()], ItemsFormat::Csv, Some(pw(MASTER)), None, true, None).await;
    assert_eq!(code_err(export), ErrorCode::FeatureUnavailable);
}

#[tokio::test(start_paused = true)]
async fn an_export_of_logins_asks_who_is_there_and_that_a_plain_file_s_risk_was_read() {
    let (h, ids) = vault_with_logins().await;
    let ids = &ids;
    let csv = |password: Option<&str>, acknowledged: bool| h.core.export_items(ids, ItemsFormat::Csv, password.map(pw), None, acknowledged, None);
    // Nothing chosen, or nothing that is a login of this vault.
    assert_eq!(code_err(h.core.export_items(&[], ItemsFormat::Csv, Some(pw(MASTER)), None, true, None).await), ErrorCode::ExportNothing);
    assert_eq!(code_err(h.core.export_items(&[Uuid::new_v4()], ItemsFormat::Csv, Some(pw(MASTER)), None, true, None).await), ErrorCode::ExportNothing);
    // A plain file only once the user confirmed that it is one, which is asked before who is there.
    assert_eq!(code_err(csv(Some("wrong password"), false).await), ErrorCode::ExportAcknowledgeNeeded);
    assert_eq!(code_err(csv(Some("wrong password"), true).await), ErrorCode::WrongPassword);
    // Without the master password, the biometric check that unlocks this vault: none here.
    assert_eq!(code_err(csv(None, true).await), ErrorCode::BiometricUnavailable);
    assert_eq!(csv(Some(MASTER), true).await.unwrap().name(), "lockra-logins.csv");
    // A KeePass database takes a password of its own, as long as a vault's.
    let kdbx = |password: Option<&str>| h.core.export_items(ids, ItemsFormat::Kdbx, Some(pw(MASTER)), None, false, password.map(pw));
    assert_eq!(code_err(kdbx(None).await), ErrorCode::PasswordTooShort);
    assert_eq!(code_err(kdbx(Some("short")).await), ErrorCode::PasswordTooShort);
    assert_eq!(kdbx(Some(KDBX_PASSWORD)).await.unwrap().name(), "lockra-logins.kdbx");
}

#[tokio::test(start_paused = true)]
async fn without_the_master_password_the_biometric_check_that_unlocks_the_vault_proves_presence() {
    let (h, ids) = vault_with_logins().await;
    h.core.enable_device_biometric(None).await.unwrap();
    let ids = &ids;
    let export = |acknowledged: bool| h.core.export_items(ids, ItemsFormat::Csv, None, Some("export the logins".into()), acknowledged, None);
    h.biometrics.answer(Err(BiometricError::Cancelled));
    assert_eq!(code_err(export(true).await), ErrorCode::BiometricCancelled);
    h.biometrics.answer(Ok(()));
    assert!(export(true).await.is_ok());
    assert_eq!(h.biometrics.reasons().last().map(String::as_str), Some("export the logins"));
    // A plain file the user did not confirm asks for no check at all.
    let asked = h.biometrics.reasons().len();
    assert_eq!(code_err(export(false).await), ErrorCode::ExportAcknowledgeNeeded);
    assert_eq!(h.biometrics.reasons().len(), asked);
}

#[tokio::test(start_paused = true)]
async fn a_keepass_export_comes_back_through_the_import_with_its_codes() {
    let (a, ids) = vault_with_logins().await;
    let file = a.core.export_items(&ids, ItemsFormat::Kdbx, Some(pw(MASTER)), None, false, Some(pw(KDBX_PASSWORD))).await.unwrap();
    assert_eq!((file.name(), file.mime(), file.format()), ("lockra-logins.kdbx", "application/octet-stream", ItemsFormat::Kdbx));
    assert!(file.bytes().starts_with(&lockra_transfer::kdbx::MAGIC));
    let b = imported(&file, Some(KDBX_PASSWORD)).await;
    assert_eq!(b.core.data().items.len(), 3);
    assert_eq!(b.core.data().entries.len(), 1, "the linked code, and no other");
    // KeePass has no favourite and no protection level; the email address beside a user name is
    // a field of its name.
    let mut github = parts(&a, "GitHub");
    github.fields.push(("Email".into(), "octocat@example.com".into(), false));
    github.fields.sort();
    github.email.clear();
    (github.favorite, github.confirm) = (false, false);
    assert_eq!(parts(&b, "GitHub"), github);
    assert!(github.code.is_some(), "linked to the account its code made");
    assert_eq!(parts(&b, "Wiki"), parts(&a, "Wiki"));
    // Known by its email address alone: that is its user name there.
    let mut mail = parts(&a, "Mail");
    mail.username = std::mem::take(&mut mail.email);
    assert_eq!(parts(&b, "Mail"), mail);
    assert_eq!(source_of(&b, "GitHub").as_deref(), Some("kdbx"));
    // How each URL is compared is Lockra's own: back as the default.
    assert!(b.core.data().items.iter().flat_map(|i| &i.urls).all(|u| u.matching == Lenient::Known(Matching::Domain)));
}

#[tokio::test(start_paused = true)]
async fn a_csv_export_comes_back_through_the_import_but_for_what_csv_has_no_column_for() {
    let (a, ids) = vault_with_logins().await;
    let file = a.core.export_items(&ids, ItemsFormat::Csv, Some(pw(MASTER)), None, true, None).await.unwrap();
    assert_eq!((file.name(), file.mime(), file.format()), ("lockra-logins.csv", "text/csv", ItemsFormat::Csv));
    let b = imported(&file, None).await;
    assert_eq!(b.core.data().items.len(), 3);
    assert_eq!(b.core.data().entries.len(), 1);
    // Bitwarden's CSV has no tags and does not say which fields were hidden.
    let mut github = parts(&a, "GitHub");
    github.fields =
        vec![("Customer no.".into(), "C-77".into(), false), ("Email".into(), "octocat@example.com".into(), false), ("PIN".into(), "4242".into(), false)];
    github.email.clear();
    github.tags.clear();
    assert_eq!(parts(&b, "GitHub"), github, "the favourite and the re-prompt stay");
    assert_eq!(parts(&b, "Wiki"), parts(&a, "Wiki"));
    let mut mail = parts(&a, "Mail");
    mail.username = std::mem::take(&mut mail.email);
    assert_eq!(parts(&b, "Mail"), mail);
    assert_eq!(source_of(&b, "Wiki").as_deref(), Some("bitwarden"));
}

#[tokio::test(start_paused = true)]
async fn an_export_is_saved_for_its_owner_only_and_only_while_the_vault_is_open() {
    let (h, ids) = vault_with_logins().await;
    let ids = &ids;
    let export = || h.core.export_items(ids, ItemsFormat::Csv, Some(pw(MASTER)), None, true, None);
    let path = h.dir.path().join("logins.csv");
    assert_eq!(h.core.export_items_write(export().await.unwrap(), path.clone()).await.unwrap(), "logins.csv");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("folder,favorite,type,name,") && text.contains(LEAKY), "{text}");
    // A file that was there, readable by anyone, is the owner's alone before the logins go in.
    let old = h.dir.path().join("old.csv");
    fs::write(&old, "old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&old, fs::Permissions::from_mode(0o644)).unwrap();
    }
    h.core.export_items_write(export().await.unwrap(), old.clone()).await.unwrap();
    assert!(fs::read_to_string(&old).unwrap().contains(LEAKY));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in [&path, &old] {
            assert_eq!(fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600, "{}", file.display());
        }
    }
    // The vault locked while the save dialog was open: nothing is written.
    let file = export().await.unwrap();
    h.core.lock_vault();
    let late = h.dir.path().join("late.csv");
    assert_eq!(code_err(h.core.export_items_write(file, late.clone()).await), ErrorCode::Locked);
    assert!(!late.exists());
}

#[tokio::test(start_paused = true)]
async fn the_webview_hears_nothing_of_what_an_export_holds() {
    let (mut h, ids) = vault_with_logins().await;
    let _ = h.notices();
    for (format, kdbx_password, name) in [(ItemsFormat::Kdbx, Some(pw(KDBX_PASSWORD)), "out.kdbx"), (ItemsFormat::Csv, None, "out.csv")] {
        let file = h.core.export_items(&ids, format, Some(pw(MASTER)), None, true, kdbx_password).await.unwrap();
        assert_eq!(h.core.export_items_write(file, h.dir.path().join(name)).await.unwrap(), name, "its name, and nothing else");
    }
    let mut heard = serde_json::to_string(&h.core.state()).unwrap();
    loop {
        match h.events.try_recv() {
            Ok(event) => heard.push_str(&serde_json::to_string(&event).unwrap()),
            Err(TryRecvError::Lagged(_)) => {}
            Err(_) => break,
        }
    }
    for secret in [LEAKY, "recovery codes in the safe", "4242", "wiki-pass-2", "mail-pass-3", KDBX_PASSWORD, SECRET] {
        assert!(!heard.contains(secret), "{secret} reached the webview");
    }
}
