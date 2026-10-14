//! The writer against Lockra's own reader and the keepass crate's: every part of a login comes
//! back, in its folder and with its code; the key is derived as asked.

use std::collections::BTreeSet;

use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use lockra_otp::uri;
use zeroize::Zeroizing;

use super::*;
use crate::item::Item;
use crate::kdbx::{self, Cipher, KdbxError, Kdf};
use crate::login::{App, Found, Login, LoginField, LoginUrl};

const PASSWORD: &str = "export password";
/// Argon2's least: the tests' quick key derivation.
const QUICK: Argon2Cost = Argon2Cost { memory_kib: 8, iterations: 1, lanes: 1 };
const GITHUB_CODE: &str = "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";

fn field(label: &str, value: &str, hidden: bool) -> LoginField {
    LoginField { label: label.into(), value: Zeroizing::new(value.into()), hidden }
}

fn login(title: &str, username: &str, password: &str) -> Login {
    let mut login = Login::new(App::KeePass);
    login.title = title.into();
    login.username = username.into();
    login.password = Zeroizing::new(password.into());
    login
}

/// Three logins: one at the root with every part, one in a nested folder, and one with more
/// URLs than ten and fields named as KeePass's own strings.
fn sample() -> Vec<Login> {
    let mut github = login("GitHub", "octocat", "pa\"ss, <word>");
    github.urls = vec![LoginUrl::plain("https://github.com/login"), LoginUrl::plain("https://gist.github.com")];
    github.notes = Zeroizing::new("Recovery codes\nin the safe & <here>".into());
    github.tags = vec!["dev".into(), "work".into()];
    github.fields = vec![field("PIN", "4242", true), field("Customer no.", "C-77", false)];
    github.totp = Some(Item::Account(uri::parse(GITHUB_CODE).unwrap()));
    let mut build = login("Build", "ci", "kp-pass-3");
    build.folder = Some("Work/Servers".into());
    let mut many = login("Many", "me", "kp-pass-4");
    many.folder = Some("Work".into());
    many.urls = (0..12).map(|i| LoginUrl::plain(&format!("https://site{i}.example"))).collect();
    many.fields = vec![
        field("Title", "not the title", false),
        field("otp", "not a code", false),
        field("KP2A_URL_3", "not a URL", false),
        field("TimeOtp-Secret", "not a secret", true),
        field("PIN", "1", true),
        field("PIN", "2", true),
        field("", "nameless", false),
    ];
    vec![github, build, many]
}

fn read_back(bytes: &[u8]) -> Vec<Login> {
    kdbx::read(bytes, PASSWORD, None)
        .unwrap()
        .into_iter()
        .map(|found| match found {
            Found::Login(login) => *login,
            Found::Left { .. } => panic!("left out: {found:?}"),
        })
        .collect()
}

fn sorted_fields(login: &Login) -> Vec<(String, String, bool)> {
    let mut fields: Vec<_> = login.fields.iter().map(|f| (f.label.clone(), f.value.to_string(), f.hidden)).collect();
    fields.sort();
    fields
}

fn owned(fields: &[(&str, &str, bool)]) -> Vec<(String, String, bool)> {
    fields.iter().map(|(label, value, hidden)| ((*label).to_owned(), (*value).to_owned(), *hidden)).collect()
}

#[test]
fn every_part_of_a_login_comes_back_in_its_folder() {
    let written = sample();
    let bytes = write(&written, PASSWORD, QUICK).unwrap();
    let read = read_back(&bytes);
    assert_eq!(read.len(), 3);
    let github = read.iter().find(|l| l.title == "GitHub").unwrap();
    assert_eq!((github.username.as_str(), github.password.as_str()), ("octocat", "pa\"ss, <word>"));
    assert_eq!(github.urls, written[0].urls);
    assert_eq!(github.notes.as_str(), "Recovery codes\nin the safe & <here>");
    assert_eq!(github.tags, ["dev", "work"]);
    assert_eq!(github.folder, None, "at the root");
    assert_eq!(sorted_fields(github), owned(&[("Customer no.", "C-77", false), ("PIN", "4242", true)]));
    assert_eq!(github.totp, written[0].totp);
    let build = read.iter().find(|l| l.title == "Build").unwrap();
    assert_eq!(build.folder.as_deref(), Some("Work/Servers"));
    assert_eq!(build.totp, None);
    let many = read.iter().find(|l| l.title == "Many").unwrap();
    assert_eq!(many.folder.as_deref(), Some("Work"));
    assert_eq!(many.urls, written[2].urls, "every URL, in its order");
    // A field named as one of KeePass's own strings, or as another field, takes a name of its own.
    assert_eq!(
        sorted_fields(many),
        owned(&[
            ("Field", "nameless", false),
            ("PIN", "1", true),
            ("PIN (2)", "2", true),
            ("Title (2)", "not the title", false),
            ("_KP2A_URL_3", "not a URL", false),
            ("_TimeOtp-Secret", "not a secret", true),
            ("otp (2)", "not a code", false),
        ])
    );
    assert_eq!(many.totp, None, "a field named otp is no code");
}

#[test]
fn the_key_is_derived_with_argon2id_at_the_cost_given() {
    let cost = Argon2Cost { memory_kib: 64, iterations: 2, lanes: 2 };
    let bytes = write(&sample(), PASSWORD, cost).unwrap();
    let header = kdbx::header(&bytes).unwrap();
    let Kdf::Argon2 { variant, memory_kib, iterations, lanes, .. } = header.kdf else { panic!("not Argon2") };
    assert_eq!((variant, memory_kib, iterations, lanes), (argon2::Variant::Argon2id, 64, 2, 2));
    assert_eq!((header.cipher, header.gzip), (Cipher::Aes256, true));
    // Its password opens it, and nothing else.
    assert_eq!(kdbx::read(&bytes, "another password", None).unwrap_err(), KdbxError::WrongKey);
}

#[test]
fn keepass_reads_it_too() {
    let bytes = write(&sample(), PASSWORD, QUICK).unwrap();
    let db = Database::open(&mut bytes.as_slice(), DatabaseKey::new().with_password(PASSWORD)).unwrap();
    assert_eq!(db.root().name, "Lockra");
    assert_eq!(db.meta.database_name.as_deref(), Some("Lockra"));
    let titles: BTreeSet<String> = db.iter_all_entries().filter_map(|e| e.get(fields::TITLE).map(str::to_owned)).collect();
    assert_eq!(titles, BTreeSet::from(["Build".to_owned(), "GitHub".to_owned(), "Many".to_owned()]));
    let github = db.iter_all_entries().find(|e| e.get(fields::TITLE) == Some("GitHub")).unwrap();
    assert_eq!(uri::parse(github.get(fields::OTP).unwrap()).unwrap(), uri::parse(GITHUB_CODE).unwrap(), "KeePassXC's otp string");
    assert_eq!(github.get("KP2A_URL"), Some("https://gist.github.com"));
    for (key, protected) in [(fields::PASSWORD, true), (fields::OTP, true), ("PIN", true), ("Customer no.", false), (fields::USERNAME, false)] {
        assert_eq!(github.fields[key].is_protected(), protected, "{key}");
    }
}
