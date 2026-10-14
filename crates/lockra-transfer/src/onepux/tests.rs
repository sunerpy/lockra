use std::io::Write;

use zip::write::SimpleFileOptions;

use super::*;
use crate::item::Item;
use crate::login::LoginField;

const DATA: &[u8] = include_bytes!("../../tests/fixtures/1pux-export.data");
const ATTRIBUTES: &[u8] = include_bytes!("../../tests/fixtures/1pux-export.attributes");

/// A zip of `entries`, deflated as 1Password writes them.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn export() -> Vec<u8> {
    zip_of(&[("export.attributes", ATTRIBUTES), ("export.data", DATA), ("files/ATTACHMENT", b"\x89PNG")])
}

fn login(found: &Found) -> &Login {
    match found {
        Found::Login(login) => login,
        Found::Left { .. } => panic!("not a login: {found:?}"),
    }
}

fn field(label: &str, value: &str, hidden: bool) -> LoginField {
    LoginField { label: label.into(), value: value.to_owned().into(), hidden }
}

#[test]
fn logins_and_passwords_are_read_with_their_fields_and_codes() {
    let found = read(&export()).unwrap();
    assert_eq!(found.len(), 8);
    let dropbox = login(&found[0]);
    assert_eq!((dropbox.app, dropbox.title.as_str(), dropbox.favorite), (App::OnePassword, "Dropbox", true));
    assert_eq!((dropbox.username.as_str(), dropbox.password.as_str()), ("alex@example.com", "op-pux-pass-1"));
    let urls: Vec<&str> = dropbox.urls.iter().map(|u| u.url.as_str()).collect();
    assert_eq!(urls, ["https://www.dropbox.com/login", "https://dropbox.com"]);
    assert_eq!((dropbox.notes.as_str(), dropbox.tags.clone()), ("Shared with the family", vec!["cloud".to_owned(), "family".to_owned()]));
    // Two vaults: each names the group of its items.
    assert_eq!(dropbox.folder.as_deref(), Some("Personal"));
    assert_eq!(
        dropbox.fields,
        [
            field("remember", "false", false),
            field("keep", "true", false),
            field("pin", "1234", true),
            field("recovery key", "ABCD-EFGH", true),
            field("security question", "first pet", false),
            field("Security", "support@example.com", false),
            field("renewal", "2026-01-01", false),
        ]
    );
    assert!(matches!(&dropbox.totp, Some(Item::Account(code)) if code.issuer == "Dropbox" && code.secret.as_slice() == b"1234567890"));
    // A password item: the password alone; archived items come along.
    let garage = login(&found[1]);
    assert_eq!((garage.title.as_str(), garage.password.as_str(), garage.username.as_str(), garage.urls.len()), ("Garage door", "op-pux-pass-2", "", 0));
    // Section fields named `username` and `password` stand for the login's own; a bare secret.
    let streaming = login(&found[7]);
    assert_eq!((streaming.username.as_str(), streaming.password.as_str(), streaming.folder.as_deref()), ("family", "op-pux-pass-3", Some("Family")));
    assert!(streaming.fields.is_empty());
    assert!(matches!(&streaming.totp, Some(Item::Account(code)) if code.issuer == "Streaming" && code.account == "family"));
}

#[test]
fn other_categories_are_left_out() {
    let found = read(&export()).unwrap();
    let left: Vec<(&str, RejectReason)> = [2, 3, 4, 5, 6]
        .iter()
        .map(|&i| match &found[i] {
            Found::Left { label, reason } => (label.as_str(), *reason),
            Found::Login(_) => panic!("{i}"),
        })
        .collect();
    assert_eq!(
        left,
        [
            ("Visa", RejectReason::CardItem),
            ("Recipe", RejectReason::NoteItem),
            ("Alex", RejectReason::IdentityItem),
            ("Server key", RejectReason::OtherItem),
            ("Odd", RejectReason::OtherItem),
        ]
    );
}

#[test]
fn one_vault_names_no_group_and_a_second_code_stays_hidden() {
    let data = br#"{"accounts":[{"vaults":[{"attrs":{"name":"Personal"},"items":[{"categoryUuid":"001","favIndex":0,
        "details":{"sections":[{"title":"","fields":[
            {"title":"code","id":"TOTP_1","value":{"totp":"JBSWY3DPEHPK3PXP"}},
            {"title":"backup code","id":"TOTP_2","value":{"totp":"GEZDGNBVGY3TQOJQ"}}]}]},
        "overview":{"title":"Solo","url":"https://solo.example"}}]}]}]}"#;
    let found = read(&zip_of(&[("export.data", data)])).unwrap();
    let solo = login(&found[0]);
    assert_eq!((solo.folder.as_deref(), solo.favorite), (None, false));
    assert_eq!(solo.urls[0].url, "https://solo.example");
    assert_eq!(solo.fields, [field("backup code", "GEZDGNBVGY3TQOJQ", true)]);
    assert!(matches!(&solo.totp, Some(Item::Account(code)) if code.secret.as_slice() == b"Hello!\xde\xad\xbe\xef"));
}

#[test]
fn zips_that_are_not_1pux_or_too_large_are_refused() {
    assert_eq!(read(&zip_of(&[("readme.txt", b"hello")])), Err(OnePuxError::NotOnePux));
    assert_eq!(read(&zip_of(&[("export.data", b"not json")])), Err(OnePuxError::Damaged));
    assert_eq!(read(b"PK\x03\x04 damaged"), Err(OnePuxError::Damaged));
    let mut cut = export();
    cut.truncate(cut.len() / 2);
    assert_eq!(read(&cut), Err(OnePuxError::Damaged));
    // More entries than allowed, and an `export.data` that expands past the limit: a few
    // kilobytes that inflate to a megabyte stop at the limit's byte, whatever the zip declares.
    assert_eq!(read_within(&export(), 2, MAX_EXPANDED_BYTES), Err(OnePuxError::TooLarge));
    let bomb = zip_of(&[("export.data", &vec![b' '; 1 << 20])]);
    assert!(bomb.len() < 8 * 1024, "{}", bomb.len());
    assert_eq!(read_within(&bomb, MAX_ZIP_ENTRIES, 64 * 1024), Err(OnePuxError::TooLarge));
    assert!(read_within(&export(), 3, DATA.len()).is_ok(), "the limits themselves are allowed");
}

#[test]
fn a_zip_bomb_stops_at_the_import_s_limit() {
    // 64 MiB and one byte of spaces deflate to some 64 KB.
    let bomb = zip_of(&[("export.data", &vec![b' '; MAX_EXPANDED_BYTES + 1])]);
    assert!(bomb.len() < 256 * 1024, "{}", bomb.len());
    assert_eq!(read(&bomb), Err(OnePuxError::TooLarge));
    let many: Vec<(String, &[u8])> = (0..=MAX_ZIP_ENTRIES).map(|i| (format!("files/{i}"), &b""[..])).collect();
    let entries: Vec<(&str, &[u8])> = many.iter().map(|(name, bytes)| (name.as_str(), *bytes)).collect();
    assert_eq!(read(&zip_of(&entries)), Err(OnePuxError::TooLarge));
}

#[test]
fn dates_read_as_days() {
    assert_eq!(date_of(1_767_225_600), "2026-01-01");
    assert_eq!(date_of(0), "1970-01-01");
    assert_eq!(date_of(951_782_400), "2000-02-29");
    assert_eq!(date_of(-86_400), "1969-12-31");
}
