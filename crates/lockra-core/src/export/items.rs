//! Logins exported to a file (docs/formats.md §11): a KeePass database under a password of its
//! own, or Bitwarden's plain CSV. The file is made in Rust, after the user proved to be there,
//! and saved by the shell; the webview hears its name only.

use std::fmt;

use lockra_transfer::kdbx::Argon2Cost;
use lockra_transfer::{App, Item, Login, LoginField, LoginUrl, csv_logins, kdbx};
use lockra_vault::KdfCost;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::entry::VaultData;
use crate::error::{CoreResult, ErrorCode};
use crate::item::{Protection, Purpose, VaultItem};
use crate::lenient::Lenient;

/// What logins export to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemsFormat {
    /// A KeePass database (KDBX 4), under a password of its own.
    Kdbx,
    /// Bitwarden's CSV: plain text, which every password manager imports.
    Csv,
}

/// A file of logins, made and not yet saved: its bytes stay in Rust.
pub struct ItemsFile {
    format: ItemsFormat,
    bytes: Zeroizing<Vec<u8>>,
}

impl ItemsFile {
    /// What it is.
    pub fn format(&self) -> ItemsFormat {
        self.format
    }

    /// The name the save dialog offers.
    pub fn name(&self) -> &'static str {
        match self.format {
            ItemsFormat::Kdbx => "lockra-logins.kdbx",
            ItemsFormat::Csv => "lockra-logins.csv",
        }
    }

    /// Its media type, for the phone's file picker.
    pub fn mime(&self) -> &'static str {
        match self.format {
            ItemsFormat::Kdbx => "application/octet-stream",
            ItemsFormat::Csv => "text/csv",
        }
    }

    /// What it holds.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for ItemsFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemsFile").field("format", &self.format).field("bytes", &format_args!("<{} bytes>", self.bytes.len())).finish()
    }
}

/// `item` as an export writes it, with the code of the account it is linked to in `data`: the
/// user name (the email address where there is none, else the email as a field of its name),
/// the password, every URL, the notes, the group, the favourite, the tags and the custom fields;
/// the confirm level as Bitwarden's re-prompt.
pub(crate) fn login_of(item: &VaultItem, data: &VaultData, app: App) -> Login {
    let mut login = Login::new(app);
    login.title.clone_from(&item.title);
    let value = |purpose: Purpose| item.field_of(purpose).map(|f| f.value.clone()).filter(|v| !v.is_empty());
    let email = value(Purpose::Email);
    match value(Purpose::Username) {
        Some(username) => {
            login.username = username.to_string();
            if let Some(email) = email {
                login.fields.push(LoginField { label: "Email".to_owned(), value: email, hidden: false });
            }
        }
        None => login.username = email.map(|e| e.to_string()).unwrap_or_default(),
    }
    login.password = value(Purpose::Password).unwrap_or_default();
    login.urls = item.urls.iter().map(|u| LoginUrl { url: u.url.clone(), matching: None }).collect();
    login.notes = item.notes.clone();
    login.folder.clone_from(&item.group);
    login.favorite = item.favorite;
    login.tags.clone_from(&item.tags);
    for field in item.fields.iter().filter(|f| !f.removed && f.purpose == Lenient::Known(Purpose::Custom)) {
        login.fields.push(LoginField { label: field.label.clone(), value: field.value.clone(), hidden: field.hidden });
    }
    login.reprompt = item.protection == Lenient::Known(Protection::Confirm);
    login.totp = item.totp_entry.and_then(|id| data.get(id)).map(|entry| Item::Account(entry.to_auth()));
    login
}

/// The file of `logins` in `format`: a KeePass database under `password`, its key derived at
/// `cost`, or Bitwarden's CSV. CPU work (Argon2): run off the runtime.
pub(crate) fn encode(logins: &[Login], format: ItemsFormat, password: &str, cost: KdfCost) -> CoreResult<ItemsFile> {
    let bytes = match format {
        ItemsFormat::Kdbx => {
            let cost = Argon2Cost { memory_kib: cost.memory_kib(), iterations: cost.passes(), lanes: cost.lanes() };
            kdbx::write(logins, password, cost).map_err(|_| ErrorCode::Internal)?
        }
        // The text's own bytes, not a copy of them.
        ItemsFormat::Csv => Zeroizing::new(std::mem::take(&mut *csv_logins::write_bitwarden(logins)).into_bytes()),
    };
    Ok(ItemsFile { format, bytes })
}
