//! Bitwarden's JSON export (docs/formats.md §11): `{"encrypted": false, "folders"|"collections",
//! "items"}`. An encrypted export (`"encrypted": true`, by the account's key or a password of its
//! own) is listed as one item the import cannot take yet. Each item reads on its own: one of a
//! shape this version does not know is left out, not the file.

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::item::RejectReason;
use crate::login::{App, Found, Login, UrlMatch, non_blank, totp};

#[derive(Deserialize)]
struct Probe {
    encrypted: bool,
    #[serde(default)]
    items: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    data: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct Export<'a> {
    #[serde(default)]
    folders: Vec<Grouping>,
    #[serde(default)]
    collections: Vec<Grouping>,
    #[serde(default, borrow)]
    items: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct Grouping {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ItemJson {
    #[serde(rename = "type")]
    kind: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    favorite: Option<bool>,
    #[serde(default)]
    reprompt: Option<u32>,
    #[serde(default, rename = "folderId")]
    folder_id: Option<String>,
    #[serde(default, rename = "collectionIds")]
    collection_ids: Option<Vec<String>>,
    #[serde(default)]
    fields: Option<Vec<FieldJson>>,
    #[serde(default)]
    login: Option<LoginJson>,
}

#[derive(Deserialize)]
struct FieldJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default, rename = "type")]
    kind: u32,
}

#[derive(Deserialize)]
struct LoginJson {
    #[serde(default)]
    uris: Option<Vec<UriJson>>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    totp: Option<String>,
}

#[derive(Deserialize)]
struct UriJson {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default, rename = "match")]
    matching: Option<u32>,
}

/// A file that looks like Bitwarden's export and does not read as one.
#[derive(Debug, thiserror::Error)]
#[error("the Bitwarden export does not read")]
pub struct BitwardenError;

/// Whether `bytes` are Bitwarden's JSON export: an object with a boolean `encrypted` beside its
/// `items` (or, encrypted by a password of its own, its `data`).
pub fn recognize(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    if text.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return false;
    }
    serde_json::from_slice::<Probe>(text).is_ok_and(|probe| probe.items.is_some() || (probe.encrypted && probe.data.is_some()))
}

/// The logins of a Bitwarden export, and the items it leaves out (secure notes, cards,
/// identities, SSH keys, an encrypted export as a whole).
pub fn read(bytes: &[u8]) -> Result<Vec<Found>, BitwardenError> {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let probe: Probe = serde_json::from_slice(text).map_err(|_| BitwardenError)?;
    if probe.encrypted {
        return Ok(vec![Found::left("", RejectReason::EncryptedExport)]);
    }
    let export: Export<'_> = serde_json::from_slice(text).map_err(|_| BitwardenError)?;
    let name_of = |groupings: &[Grouping], id: &str| groupings.iter().find(|g| g.id == id).and_then(|g| g.name.as_deref()).and_then(non_blank);
    Ok(export
        .items
        .iter()
        .map(|raw| {
            let Ok(item) = serde_json::from_str::<ItemJson>(raw.get()) else {
                return Found::left(&label_of(raw), RejectReason::OtherItem);
            };
            let name = item.name.clone().unwrap_or_default();
            let reason = match item.kind {
                1 => None,
                2 => Some(RejectReason::NoteItem),
                3 => Some(RejectReason::CardItem),
                // Identity, driver's licence, passport.
                4 | 7 | 8 => Some(RejectReason::IdentityItem),
                // SSH key, bank account, and what later versions add.
                _ => Some(RejectReason::OtherItem),
            };
            if let Some(reason) = reason {
                return Found::left(&name, reason);
            }
            let mut login = Login::new(App::Bitwarden);
            login.title = name.trim().to_owned();
            login.notes = item.notes.unwrap_or_default().into();
            login.favorite = item.favorite.unwrap_or(false);
            login.reprompt = item.reprompt.unwrap_or(0) != 0;
            login.folder = match (&item.folder_id, &item.collection_ids) {
                (Some(folder), _) => name_of(&export.folders, folder),
                (None, Some(collections)) => collections.iter().find_map(|id| name_of(&export.collections, id)),
                (None, None) => None,
            };
            if let Some(details) = item.login {
                for uri in details.uris.unwrap_or_default() {
                    login.add_url(&uri.uri.unwrap_or_default(), uri.matching.and_then(url_match));
                }
                login.username = details.username.unwrap_or_default();
                login.password = details.password.unwrap_or_default().into();
                login.totp = totp(&details.totp.unwrap_or_default(), &login.title, &login.username);
            }
            for field in item.fields.unwrap_or_default() {
                let label = field.name.unwrap_or_default();
                match field.kind {
                    // Text, then hidden, then a check box (`true` or `false`); a linked field
                    // stands for the user name or the password and holds nothing of its own.
                    0 | 2 => login.add_field(&label, &field.value.unwrap_or_default(), false),
                    1 => login.add_field(&label, &field.value.unwrap_or_default(), true),
                    _ => {}
                }
            }
            Found::Login(Box::new(login))
        })
        .collect())
}

/// Bitwarden's `UriMatchStrategy`: Domain 0, Host 1, StartsWith 2, Exact 3, RegularExpression 4,
/// Never 5; `null` is the account's default, the domain.
fn url_match(value: u32) -> Option<UrlMatch> {
    Some(match value {
        0 => UrlMatch::Domain,
        1 => UrlMatch::Host,
        2 => UrlMatch::StartsWith,
        3 => UrlMatch::Exact,
        4 => UrlMatch::Regex,
        5 => UrlMatch::Never,
        _ => return None,
    })
}

/// The `name` of an item that does not read otherwise.
fn label_of(raw: &RawValue) -> String {
    #[derive(Deserialize)]
    struct Named {
        #[serde(default)]
        name: Option<String>,
    }
    serde_json::from_str::<Named>(raw.get()).ok().and_then(|n| n.name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::login::{LoginField, LoginUrl};

    const EXPORT: &[u8] = include_bytes!("../tests/fixtures/bitwarden.json");
    const ORGANIZATION: &[u8] = include_bytes!("../tests/fixtures/bitwarden-org.json");
    const ENCRYPTED: &[u8] = include_bytes!("../tests/fixtures/bitwarden-encrypted.json");

    fn login(found: &Found) -> &Login {
        match found {
            Found::Login(login) => login,
            Found::Left { .. } => panic!("not a login: {found:?}"),
        }
    }

    #[test]
    fn exports_are_recognised_encrypted_or_not() {
        assert!(recognize(EXPORT) && recognize(ORGANIZATION) && recognize(ENCRYPTED));
        assert!(recognize(b"\xef\xbb\xbf {\"encrypted\": false, \"items\": []}"));
        for other in [&b"{\"encrypted\": false}"[..], b"{\"items\": []}", b"[1, 2]", b"{\"encrypted\": \"no\", \"items\": []}", b"{", b"otpauth://x", b""] {
            assert!(!recognize(other), "{}", String::from_utf8_lossy(other));
        }
    }

    #[test]
    fn a_login_keeps_its_fields_rules_folder_and_code() {
        let found = read(EXPORT).unwrap();
        assert_eq!(found.len(), 9);
        let github = login(&found[0]);
        assert_eq!((github.app, github.title.as_str(), github.folder.as_deref()), (App::Bitwarden, "GitHub", Some("Work")));
        assert_eq!((github.favorite, github.reprompt), (true, true));
        assert_eq!((github.username.as_str(), github.password.as_str()), ("octocat", "bw-json-pass-1"));
        assert_eq!(github.notes.as_str(), "Recovery codes are in the safe");
        let rules: Vec<Option<UrlMatch>> = github.urls.iter().map(|u| u.matching).collect();
        assert_eq!(
            rules,
            [
                None,
                Some(UrlMatch::Host),
                Some(UrlMatch::StartsWith),
                Some(UrlMatch::Exact),
                Some(UrlMatch::Regex),
                Some(UrlMatch::Never),
                Some(UrlMatch::Domain)
            ]
        );
        assert_eq!(github.urls[4], LoginUrl { url: "^https://(www\\.)?github\\.com/".into(), matching: Some(UrlMatch::Regex) });
        assert_eq!(
            github.fields,
            [
                LoginField { label: "PIN".into(), value: "4242".to_owned().into(), hidden: true },
                LoginField { label: "Customer no.".into(), value: "C-77".to_owned().into(), hidden: false },
                LoginField { label: "Remember me".into(), value: "true".to_owned().into(), hidden: false },
            ]
        );
        assert!(matches!(&github.totp, Some(Item::Account(code)) if code.issuer == "GitHub" && code.account == "octocat"));
        // The second account's bare secret is the same as the first's.
        let second = login(&found[1]);
        let (Some(Item::Account(a)), Some(Item::Account(b))) = (&github.totp, &second.totp) else { panic!() };
        assert_eq!(a.secret, b.secret);
        assert_eq!((second.folder.as_deref(), second.reprompt, second.notes.as_str()), (Some("Home"), false, ""));
        let router = login(&found[2]);
        assert_eq!((router.urls.len(), router.folder.as_deref(), router.totp.is_none()), (0, None, true));
    }

    #[test]
    fn other_kinds_are_left_out_with_their_reason() {
        let found = read(EXPORT).unwrap();
        let left: Vec<(&str, RejectReason)> = found[3..]
            .iter()
            .map(|f| match f {
                Found::Left { label, reason } => (label.as_str(), *reason),
                Found::Login(_) => panic!(),
            })
            .collect();
        assert_eq!(
            left,
            [
                ("Wi-Fi", RejectReason::NoteItem),
                ("Visa", RejectReason::CardItem),
                ("Me", RejectReason::IdentityItem),
                ("Deploy key", RejectReason::OtherItem),
                ("Passport", RejectReason::IdentityItem),
                ("Damaged", RejectReason::OtherItem),
            ]
        );
    }

    #[test]
    fn an_organization_names_its_collection_and_an_encrypted_export_is_one_item_left_out() {
        let found = read(ORGANIZATION).unwrap();
        assert_eq!(login(&found[0]).folder.as_deref(), Some("Engineering"));
        assert_eq!(read(ENCRYPTED).unwrap(), [Found::Left { label: String::new(), reason: RejectReason::EncryptedExport }]);
        assert!(read(b"{\"encrypted\": false, \"items\": 7}").is_err());
        assert!(read(b"not json").is_err());
        assert_eq!(url_match(9), None);
    }
}
