use lockra_otp::{Digits, OtpKind, Period};

use super::*;
use crate::item::Item;
use crate::login::{LoginField, LoginUrl};

const CHROME: &[u8] = include_bytes!("../../tests/fixtures/chrome.csv");
const FIREFOX: &[u8] = include_bytes!("../../tests/fixtures/firefox.csv");
const SAFARI: &[u8] = include_bytes!("../../tests/fixtures/safari.csv");
const SAFARI_OLD: &[u8] = include_bytes!("../../tests/fixtures/safari-old.csv");
const BITWARDEN: &[u8] = include_bytes!("../../tests/fixtures/bitwarden.csv");
const ONEPASSWORD: &[u8] = include_bytes!("../../tests/fixtures/1password.csv");
const LASTPASS: &[u8] = include_bytes!("../../tests/fixtures/lastpass.csv");

fn logins(bytes: &[u8]) -> Vec<Found> {
    read(bytes, recognize(bytes).expect("a known header")).unwrap()
}

fn login(found: &Found) -> &Login {
    match found {
        Found::Login(login) => login,
        Found::Left { .. } => panic!("not a login: {found:?}"),
    }
}

fn urls(login: &Login) -> Vec<&str> {
    login.urls.iter().map(|u| u.url.as_str()).collect()
}

#[test]
fn every_export_is_recognised_by_its_header() {
    let cases: [(&[u8], CsvFormat); 7] = [
        (CHROME, CsvFormat::Chrome),
        (FIREFOX, CsvFormat::Firefox),
        (SAFARI, CsvFormat::Safari),
        (SAFARI_OLD, CsvFormat::Safari),
        (BITWARDEN, CsvFormat::Bitwarden),
        (ONEPASSWORD, CsvFormat::OnePassword),
        (LASTPASS, CsvFormat::LastPass),
    ];
    for (bytes, format) in cases {
        assert_eq!(recognize(bytes), Some(format), "{}", String::from_utf8_lossy(&bytes[..40]));
    }
    // A byte order mark and quoted names change nothing; Edge writes Chrome's columns.
    assert_eq!(recognize(b"\xef\xbb\xbf\"name\",\"url\",\"username\",\"password\"\n"), Some(CsvFormat::Chrome));
    // An organization's Bitwarden export names collections instead of a folder.
    assert_eq!(recognize(b"collections,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n"), Some(CsvFormat::Bitwarden));
    // A LastPass export from before it kept codes.
    assert_eq!(recognize(b"url,username,password,extra,name,grouping,fav\n"), Some(CsvFormat::LastPass));
    // A Safari export with the OTPAuth column but its URL column spelled `Url`: 1Password's.
    assert_eq!(recognize(b"Title,Url,Username,Password,OTPAuth,Notes\n"), Some(CsvFormat::OnePassword));
    // Not an export: an otpauth list, other CSV, nothing at all.
    assert_eq!(recognize(b"otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP\n"), None);
    assert_eq!(recognize(b"date,amount,payee\n2026-01-01,3,shop\n"), None);
    assert_eq!(recognize(b""), None);
    assert_eq!(recognize(b"\x00\x01\x02"), None);
}

#[test]
fn chrome_and_edge() {
    let found = logins(CHROME);
    assert_eq!(found.len(), 3);
    let github = login(&found[0]);
    assert_eq!((github.app, github.title.as_str(), github.username.as_str(), github.password.as_str()), (App::Chrome, "GitHub", "octocat", "chrome-pass-1"));
    assert_eq!(urls(github), ["https://github.com/login"]);
    assert_eq!(github.notes.as_str(), "First line, with a comma\r\nSecond line with \"quotes\"");
    assert_eq!(github.urls[0].matching, None, "Chrome has no rules of its own");
    // Without a name, the host names it; an Android app's address, its package.
    assert_eq!(login(&found[1]).title, "www.example.com");
    assert_eq!(login(&found[2]).title, "com.example.app");
    assert_eq!(urls(login(&found[2])), ["android://Kx9vQaBc==@com.example.app/"]);
    assert!(found.iter().all(|f| login(f).totp.is_none() && login(f).folder.is_none()));
}

#[test]
fn firefox_leaves_out_its_own_account() {
    let found = logins(FIREFOX);
    let titles: Vec<&str> = found.iter().map(|f| login(f).title.as_str()).collect();
    assert_eq!(titles, ["accounts.example.com", "intranet.example.com"]);
    let intranet = login(&found[1]);
    assert_eq!((intranet.username.as_str(), intranet.password.as_str()), ("bar", "firefox-pass-2"));
    assert_eq!(urls(intranet), ["https://intranet.example.com:8443"]);
    assert!(intranet.notes.is_empty());
}

#[test]
fn safari_brings_its_codes() {
    let found = logins(SAFARI);
    assert_eq!(found.len(), 2);
    let example = login(&found[0]);
    assert_eq!(example.title, "example.com (example_user)");
    assert_eq!(example.notes.as_str(), "Example note\nMore notes on new line");
    let Some(Item::Account(code)) = &example.totp else { panic!("{:?}", example.totp) };
    assert_eq!((code.issuer.as_str(), code.account.as_str()), ("Example", "example_user"));
    assert_eq!(code.secret.as_slice(), b"Hello!\xde\xad\xbe\xef");
    assert_eq!(login(&found[1]).totp, None, "an empty OTPAuth cell is no code");
    let old = logins(SAFARI_OLD);
    assert_eq!((login(&old[0]).app, login(&old[0]).password.as_str()), (App::Safari, "safari-pass-3"));
}

#[test]
fn bitwarden_csv() {
    let found = logins(BITWARDEN);
    assert_eq!(found.len(), 3);
    let github = login(&found[0]);
    assert_eq!((github.title.as_str(), github.folder.as_deref(), github.favorite, github.reprompt), ("GitHub", Some("Work"), true, true));
    assert_eq!(urls(github), ["https://github.com", "https://gist.github.com"]);
    assert_eq!((github.username.as_str(), github.password.as_str(), github.notes.as_str()), ("octocat", "bw-pass-1", "Work account"));
    assert_eq!(
        github.fields,
        [
            LoginField { label: "PIN".into(), value: "4242".to_owned().into(), hidden: false },
            LoginField { label: "Customer no.".into(), value: "C-77".to_owned().into(), hidden: false },
        ]
    );
    // A bare secret: TOTP with the default parameters, named after the login.
    let Some(Item::Account(code)) = &github.totp else { panic!() };
    assert_eq!(
        (code.issuer.as_str(), code.account.as_str(), code.digits, code.kind),
        ("GitHub", "octocat", Digits::SIX, OtpKind::Totp { period: Period::THIRTY })
    );
    let mail = login(&found[1]);
    assert_eq!((mail.folder.as_deref(), mail.favorite, mail.reprompt, mail.totp.is_none()), (None, false, false, true));
    assert_eq!(found[2], Found::Left { label: "Wi-Fi at home".into(), reason: RejectReason::NoteItem });
}

#[test]
fn onepassword_csv() {
    let found = logins(ONEPASSWORD);
    let dropbox = login(&found[0]);
    assert_eq!((dropbox.app, dropbox.favorite, dropbox.tags.clone()), (App::OnePassword, true, vec!["work".to_owned(), "cloud".to_owned()]));
    assert_eq!(dropbox.notes.as_str(), "Shared folder");
    assert!(matches!(&dropbox.totp, Some(Item::Account(code)) if code.issuer == "Dropbox"));
    let forum = login(&found[1]);
    assert_eq!((forum.title.as_str(), forum.favorite, forum.tags.len()), ("Old forum", false, 0));
}

#[test]
fn lastpass_csv() {
    let found = logins(LASTPASS);
    assert_eq!(found.len(), 4);
    let amazon = login(&found[0]);
    assert_eq!((amazon.title.as_str(), amazon.folder.as_deref(), amazon.favorite), ("Amazon", Some("Shopping/Online"), true));
    assert_eq!(amazon.notes.as_str(), "Gift cards in the drawer");
    assert!(matches!(&amazon.totp, Some(Item::Account(code)) if code.secret.as_slice() == b"foobar"));
    assert_eq!(found[1], Found::Left { label: "Visa".into(), reason: RejectReason::CardItem });
    assert_eq!(found[2], Found::Left { label: "Door".into(), reason: RejectReason::NoteItem });
    let router = login(&found[3]);
    assert_eq!((router.folder.as_deref(), router.urls.clone()), (None, vec![LoginUrl::plain("https://router.local")]));
}

#[test]
fn rows_of_any_length_and_blank_rows_are_taken_as_they_come() {
    let text = b"name,url,username,password\nShort,https://a.example\n\n , , , \nLong,https://b.example,b,pw,extra,cells\n";
    let found = read(text, CsvFormat::Chrome).unwrap();
    let rows: Vec<(&str, &str, &str)> = found.iter().map(login).map(|l| (l.title.as_str(), l.username.as_str(), l.password.as_str())).collect();
    assert_eq!(rows, [("Short", "", ""), ("Long", "b", "pw")]);
    assert!(read(b"", CsvFormat::Chrome).unwrap().is_empty());
}

#[test]
fn notes_of_lastpass_say_what_they_are() {
    assert_eq!(note_reason("NoteType:Bank Account\nBank:X"), RejectReason::CardItem);
    assert_eq!(note_reason("NoteType:Passport"), RejectReason::IdentityItem);
    assert_eq!(note_reason("NoteType:SSH Key"), RejectReason::OtherItem);
    assert_eq!(note_reason("just text"), RejectReason::NoteItem);
    assert_eq!(host_of("https://user:pw@[::1]:8080/x"), "[::1]");
    assert_eq!(host_of("example.com/login"), "example.com");
    assert!(truthy(" TRUE ") && truthy("yes") && truthy("1") && !truthy("0") && !truthy(""));
    assert_eq!(cells_of("a,\"b,c\""), ["a", "b,c"]);
}
