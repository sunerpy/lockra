//! 1Password's 1PUX export (docs/formats.md §11): a zip whose `export.data` is JSON, accounts of
//! vaults of items. Logins (category `001`) and passwords (`005`) are read; the other categories
//! are listed as left out. The zip is read within limits: at most [`MAX_ZIP_ENTRIES`] entries,
//! `export.data` expanded to at most [`MAX_EXPANDED_BYTES`].

use std::io::{Cursor, Read};

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::item::RejectReason;
use crate::login::{App, Found, Login, MAX_EXPANDED_BYTES, non_blank, totp};

/// The most entries a zip of an import may hold.
pub const MAX_ZIP_ENTRIES: usize = 10_000;

/// The name of the entry with the items.
const EXPORT_DATA: &str = "export.data";

/// Why a zip did not read as a 1PUX export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OnePuxError {
    /// A zip without `export.data`: not 1Password's.
    #[error("not a 1PUX export")]
    NotOnePux,
    /// More entries than [`MAX_ZIP_ENTRIES`], or `export.data` expanding past the limit.
    #[error("the zip is too large")]
    TooLarge,
    /// A damaged zip, or `export.data` that is not 1PUX's JSON.
    #[error("the zip does not read")]
    Damaged,
}

#[derive(Deserialize)]
struct ExportData<'a> {
    #[serde(default, borrow)]
    accounts: Vec<Account<'a>>,
}

#[derive(Deserialize)]
struct Account<'a> {
    #[serde(default, borrow)]
    vaults: Vec<Vault<'a>>,
}

#[derive(Deserialize)]
struct Vault<'a> {
    #[serde(default)]
    attrs: Option<VaultAttrs>,
    #[serde(default, borrow)]
    items: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct VaultAttrs {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ItemJson {
    #[serde(default, rename = "categoryUuid")]
    category: String,
    #[serde(default, rename = "favIndex")]
    fav_index: Option<i64>,
    #[serde(default)]
    details: Option<Details>,
    #[serde(default)]
    overview: Option<Overview>,
}

#[derive(Deserialize, Default)]
struct Details {
    #[serde(default, rename = "loginFields")]
    login_fields: Option<Vec<LoginFieldJson>>,
    #[serde(default, rename = "notesPlain")]
    notes: Option<String>,
    #[serde(default)]
    sections: Option<Vec<Option<Section>>>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
struct LoginFieldJson {
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "fieldType")]
    field_type: Option<String>,
    #[serde(default)]
    designation: Option<String>,
}

#[derive(Deserialize)]
struct Section {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    fields: Option<Vec<SectionField>>,
}

#[derive(Deserialize)]
struct SectionField {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    value: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct Overview {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    urls: Option<Vec<UrlJson>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct UrlJson {
    #[serde(default)]
    url: Option<String>,
}

/// The logins of a 1PUX export, and the items it leaves out.
pub fn read(bytes: &[u8]) -> Result<Vec<Found>, OnePuxError> {
    read_within(bytes, MAX_ZIP_ENTRIES, MAX_EXPANDED_BYTES)
}

/// [`read`], with the limits given.
pub(crate) fn read_within(bytes: &[u8], max_entries: usize, max_expanded: usize) -> Result<Vec<Found>, OnePuxError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| OnePuxError::Damaged)?;
    if archive.len() > max_entries {
        return Err(OnePuxError::TooLarge);
    }
    let data = {
        let entry = match archive.by_name(EXPORT_DATA) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => return Err(OnePuxError::NotOnePux),
            Err(_) => return Err(OnePuxError::Damaged),
        };
        let mut data = Vec::new();
        // The entry's declared size is not trusted: reading stops one byte past the limit.
        let limit = u64::try_from(max_expanded).unwrap_or(u64::MAX).saturating_add(1);
        entry.take(limit).read_to_end(&mut data).map_err(|_| OnePuxError::Damaged)?;
        if data.len() > max_expanded {
            return Err(OnePuxError::TooLarge);
        }
        zeroize::Zeroizing::new(data)
    };
    let export: ExportData<'_> = serde_json::from_slice(&data).map_err(|_| OnePuxError::Damaged)?;
    let vaults: Vec<&Vault<'_>> = export.accounts.iter().flat_map(|a| a.vaults.iter()).collect();
    // A vault names the group only where there are several; tags stay tags.
    let named = vaults.len() > 1;
    let mut found = Vec::new();
    for vault in vaults {
        let group = vault.attrs.as_ref().and_then(|a| a.name.as_deref()).and_then(non_blank).filter(|_| named);
        for raw in &vault.items {
            found.push(read_item(raw, group.as_deref()));
        }
    }
    Ok(found)
}

fn read_item(raw: &RawValue, group: Option<&str>) -> Found {
    let Ok(item) = serde_json::from_str::<ItemJson>(raw.get()) else {
        return Found::left(&title_of(raw), RejectReason::OtherItem);
    };
    let overview = item.overview.unwrap_or_default();
    let title = overview.title.clone().unwrap_or_default();
    let reason = match item.category.as_str() {
        "001" | "005" => None,
        // Credit card, bank account.
        "002" | "101" => Some(RejectReason::CardItem),
        "003" => Some(RejectReason::NoteItem),
        // Identity, driver's licence, outdoor licence, membership, passport, rewards, SSN.
        "004" | "103" | "104" | "105" | "106" | "107" | "108" => Some(RejectReason::IdentityItem),
        _ => Some(RejectReason::OtherItem),
    };
    if let Some(reason) = reason {
        return Found::left(&title, reason);
    }
    let details = item.details.unwrap_or_default();
    let mut login = Login::new(App::OnePassword);
    login.title = title.trim().to_owned();
    login.folder = group.map(str::to_owned);
    login.favorite = item.fav_index.is_some_and(|i| i > 0);
    login.tags = overview.tags.unwrap_or_default().iter().filter_map(|t| non_blank(t)).collect();
    login.tags.dedup();
    match overview.urls.filter(|urls| !urls.is_empty()) {
        Some(urls) => urls.iter().for_each(|u| login.add_url(u.url.as_deref().unwrap_or_default(), None)),
        None => login.add_url(overview.url.as_deref().unwrap_or_default(), None),
    }
    login.notes = details.notes.unwrap_or_default().into();
    login.password = details.password.unwrap_or_default().into();
    for field in details.login_fields.unwrap_or_default() {
        let value = field.value.unwrap_or_default();
        match field.designation.as_deref() {
            Some("username") if login.username.is_empty() => login.username = value,
            Some("password") if login.password.is_empty() => login.password = value.into(),
            _ => {
                let label = field.name.unwrap_or_default();
                match field.field_type.as_deref() {
                    Some("P") => login.add_field(&label, &value, true),
                    // A check box: its value is set when ticked.
                    Some("C") => login.add_field(&label, if value.is_empty() { "false" } else { "true" }, false),
                    _ => login.add_field(&label, &value, false),
                }
            }
        }
    }
    let mut code: Option<String> = None;
    for section in details.sections.unwrap_or_default().into_iter().flatten() {
        let section_title = section.title.unwrap_or_default();
        for field in section.fields.unwrap_or_default() {
            let title = field.title.as_deref().and_then(non_blank).unwrap_or_else(|| section_title.trim().to_owned());
            let Some((kind, value)) = field.value.iter().next() else { continue };
            let text = match (kind.as_str(), value) {
                ("email", serde_json::Value::Object(email)) => email.get("email_address").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned(),
                ("date", serde_json::Value::Number(seconds)) => seconds.as_i64().map(date_of).unwrap_or_default(),
                (_, serde_json::Value::String(text)) => text.clone(),
                (_, serde_json::Value::Number(number)) => number.to_string(),
                _ => continue,
            };
            if text.is_empty() {
                continue;
            }
            let is_code = kind == "totp" || field.id.as_deref().is_some_and(|id| id.starts_with("TOTP_"));
            match kind.as_str() {
                _ if is_code && code.is_none() => code = Some(text),
                // Section fields named so stand for the login's own where it has none.
                "string" if title == "username" && login.username.is_empty() => login.username = text,
                "concealed" if title == "password" && login.password.is_empty() => login.password = text.into(),
                // A second code is a secret too: it stays hidden.
                _ if is_code => login.add_field(&title, &text, true),
                "concealed" => login.add_field(&title, &text, true),
                _ => login.add_field(&title, &text, false),
            }
        }
    }
    login.totp = code.and_then(|code| totp(&code, &login.title, &login.username));
    Found::Login(Box::new(login))
}

/// `2026-01-01`: the day of a 1Password date field (Unix seconds, UTC).
fn date_of(seconds: i64) -> String {
    // Howard Hinnant's days-to-civil, for the proleptic Gregorian calendar.
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The title of an item that does not read otherwise.
fn title_of(raw: &RawValue) -> String {
    #[derive(Deserialize)]
    struct Titled {
        #[serde(default)]
        overview: Option<Overview>,
    }
    serde_json::from_str::<Titled>(raw.get()).ok().and_then(|t| t.overview).and_then(|o| o.title).unwrap_or_default()
}

#[cfg(test)]
mod tests;
