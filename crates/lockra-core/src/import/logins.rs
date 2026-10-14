//! Logins in the import preview (docs/formats.md §11): how one an export or a backup holds
//! compares with the vault's, and what committing it writes. An export's login becomes a vault
//! item of new writes; a backup's keeps its parts' write ids and gets new stamps.

use lockra_autofill::WebUrl;
use lockra_transfer::{Login, UrlMatch};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::{VaultData, clean_name};
use crate::item::{MAX_FIELDS, MAX_NOTES_BYTES, MAX_URLS, Matching, Protection, Purpose, UrlRule, VaultItem, random_write_id};
use crate::lenient::Lenient;
use crate::logins::{MAX_TAG_CHARS, MAX_TAGS, MAX_URL_CHARS, MAX_VALUE_BYTES, clean_group, holds_later_values};
use crate::ui::ConflictReason;

/// The longest custom field label an import keeps (characters), as an edit does.
const MAX_LABEL_CHARS: usize = 100;

/// A login as an import found it.
pub(super) enum FoundLogin {
    /// In another app's export; its code is a candidate of its own.
    Export(Box<Login>),
    /// In a Lockra backup, as the backup keeps it.
    Backup(Box<VaultItem>),
}

/// Where a login is for, as two logins are compared: the host of its first URL that is a web
/// address, else its first URL as written, else its title (a KeePass entry without a URL).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Place {
    Host(String),
    Url(String),
    Title(String),
}

fn place_of<'a>(urls: impl Iterator<Item = &'a str> + Clone, title: &str) -> Place {
    if let Some(host) = urls.clone().find_map(|u| WebUrl::parse_saved(u).map(|w| w.host().to_owned())) {
        return Place::Host(host);
    }
    match urls.map(str::trim).find(|u| !u.is_empty()) {
        Some(url) => Place::Url(url.to_lowercase()),
        None => Place::Title(title.trim().to_lowercase()),
    }
}

/// The host of the first URL that is a web address; empty when none is.
fn host_of<'a>(mut urls: impl Iterator<Item = &'a str>) -> String {
    urls.find_map(|u| WebUrl::parse_saved(u).map(|w| w.host().to_owned())).unwrap_or_default()
}

impl FoundLogin {
    pub(super) fn title(&self) -> String {
        match self {
            Self::Export(login) => clean_name(&login.title),
            Self::Backup(item) => item.title.clone(),
        }
    }

    pub(super) fn username(&self) -> String {
        match self {
            Self::Export(login) => clean_name(&login.username),
            Self::Backup(item) => username_of(item),
        }
    }

    fn password(&self) -> &str {
        match self {
            Self::Export(login) => &login.password,
            Self::Backup(item) => password_of(item),
        }
    }

    pub(super) fn place(&self) -> Place {
        match self {
            Self::Export(login) => place_of(login.urls.iter().map(|u| u.url.as_str()), &login.title),
            Self::Backup(item) => item_place(item),
        }
    }

    pub(super) fn host(&self) -> String {
        match self {
            Self::Export(login) => host_of(login.urls.iter().map(|u| u.url.as_str())),
            Self::Backup(item) => host_of(item.urls.iter().map(|u| u.url.as_str())),
        }
    }

    pub(super) fn backup(&self) -> Option<&VaultItem> {
        match self {
            Self::Export(_) => None,
            Self::Backup(item) => Some(item),
        }
    }

    /// The same place, user name and password as `other`.
    pub(super) fn same_credentials(&self, other: &Self) -> bool {
        self.place() == other.place() && self.username() == other.username() && self.password() == other.password()
    }

    /// The login of `items` of the same place and user name as this one.
    pub(super) fn same_site<'a>(&self, items: &'a [VaultItem]) -> Option<&'a VaultItem> {
        let (place, username) = (self.place(), self.username());
        items.iter().find(|item| item_place(item) == place && username_of(item) == username)
    }

    /// The login of `items` of the same place, user name and password as this one.
    pub(super) fn same_login<'a>(&self, items: &'a [VaultItem]) -> Option<&'a VaultItem> {
        let (place, username, password) = (self.place(), self.username(), self.password());
        items.iter().find(|item| item_place(item) == place && username_of(item) == username && password_of(item) == password)
    }

    /// Whether `item`, of the same site, holds this one's password.
    pub(super) fn same_password(&self, item: &VaultItem) -> bool {
        password_of(item) == self.password()
    }
}

fn item_place(item: &VaultItem) -> Place {
    place_of(item.urls.iter().map(|u| u.url.as_str()), &item.title)
}

fn username_of(item: &VaultItem) -> String {
    item.field_of(Purpose::Username).map(|f| f.value.trim().to_owned()).unwrap_or_default()
}

fn password_of(item: &VaultItem) -> &str {
    item.field_of(Purpose::Password).map_or("", |f| f.value.as_str())
}

/// An export's login past what an item holds: a password or a field's value over 4 KiB, notes
/// over 64 KiB, more fields than an item takes. URLs and tags past their number are dropped
/// instead.
pub(super) fn too_large(login: &Login) -> bool {
    let fields = login.fields.len() + usize::from(!login.username.trim().is_empty()) + usize::from(!login.password.is_empty());
    login.password.len() > MAX_VALUE_BYTES
        || login.fields.iter().any(|f| f.value.len() > MAX_VALUE_BYTES)
        || login.notes.len() > MAX_NOTES_BYTES
        || fields > MAX_FIELDS
}

/// The rule an export's own URL match becomes; none is the default, the domain.
fn matching_of(rule: Option<UrlMatch>) -> Matching {
    match rule {
        None | Some(UrlMatch::Domain) => Matching::Domain,
        Some(UrlMatch::Host) => Matching::Host,
        Some(UrlMatch::StartsWith) => Matching::StartsWith,
        Some(UrlMatch::Exact) => Matching::Exact,
        Some(UrlMatch::Regex) => Matching::Regex,
        Some(UrlMatch::Never) => Matching::Never,
    }
}

/// A new login of the vault for `found`, linked to the account `link`, made at `now_ms`: an
/// export's as new writes, a backup's with a new id and new stamps (as a backup's account gets in
/// a merge), so that no deletion here or in its sync space covers it.
pub(super) fn new_item(found: &FoundLogin, link: Option<Uuid>, data: &mut VaultData, now_ms: u64) -> VaultItem {
    match found {
        FoundLogin::Export(login) => {
            let mut item = VaultItem::login(Uuid::new_v4(), now_ms, data.tick(now_ms), random_write_id());
            item.title = clean_name(&login.title);
            item.urls = login
                .urls
                .iter()
                .filter(|u| !u.url.is_empty() && u.url.chars().count() <= MAX_URL_CHARS && !u.url.chars().any(char::is_control))
                .take(MAX_URLS)
                .map(|u| UrlRule { url: u.url.clone(), matching: Lenient::Known(matching_of(u.matching)), extra: Default::default() })
                .collect();
            item.notes = login.notes.clone();
            let mut tags: Vec<String> = Vec::new();
            for tag in login.tags.iter().map(|t| clean_name(t).chars().take(MAX_TAG_CHARS).collect::<String>()).filter(|t| !t.is_empty()) {
                if tags.len() < MAX_TAGS && !tags.iter().any(|k| k.to_lowercase() == tag.to_lowercase()) {
                    tags.push(tag);
                }
            }
            item.tags = tags;
            item.group = login.folder.as_deref().and_then(clean_group);
            item.favorite = login.favorite;
            item.protection = Lenient::Known(if login.reprompt { Protection::Confirm } else { Protection::Fill });
            item.source = Some(login.app.name().to_owned());
            item.totp_entry = link;
            let username = clean_name(&login.username);
            if !username.is_empty() {
                let (stamp, wid) = (data.tick(now_ms), random_write_id());
                item.write_field(Uuid::new_v4(), Purpose::Username, stamp, wid, now_ms, |f| f.value = Zeroizing::new(username));
            }
            if !login.password.is_empty() {
                let (stamp, wid) = (data.tick(now_ms), random_write_id());
                item.write_field(Uuid::new_v4(), Purpose::Password, stamp, wid, now_ms, |f| {
                    f.value = login.password.clone();
                    f.hidden = true;
                });
            }
            for field in &login.fields {
                let (stamp, wid) = (data.tick(now_ms), random_write_id());
                item.write_field(Uuid::new_v4(), Purpose::Custom, stamp, wid, now_ms, |f| {
                    f.label = clean_name(&field.label).chars().take(MAX_LABEL_CHARS).collect();
                    f.value = field.value.clone();
                    f.hidden = field.hidden;
                });
            }
            item
        }
        FoundLogin::Backup(backup) => {
            let mut item = (**backup).clone();
            item.id = Uuid::new_v4();
            item.totp_entry = link;
            item.stamp = data.tick(now_ms);
            item.mwid = random_write_id();
            for field in &mut item.fields {
                field.stamp = data.tick(now_ms);
            }
            item.updated_at_ms = now_ms;
            item.last_used_at_ms = None;
            item.normalize();
            item
        }
    }
}

/// The same login in all a user sees of it: the metadata and the fields not removed, the link
/// left out (it is compared as the import maps it), and the stamps, the write ids, the history
/// and the times too.
pub(super) fn same_content(a: &VaultItem, b: &VaultItem) -> bool {
    let fields = |item: &VaultItem| -> Vec<_> {
        item.fields
            .iter()
            .filter(|f| !f.removed)
            .map(|f| (f.id, f.purpose.clone(), f.label.clone(), f.value.clone(), f.hidden, f.page_field.clone(), f.extra.clone()))
            .collect()
    };
    (&a.kind, &a.title, &a.urls, &a.notes, &a.tags, &a.group, a.favorite, &a.color, &a.mark, &a.protection, &a.autofill, &a.extra)
        == (&b.kind, &b.title, &b.urls, &b.notes, &b.tags, &b.group, b.favorite, &b.color, &b.mark, &b.protection, &b.autofill, &b.extra)
        && fields(a) == fields(b)
}

/// Replace the vault's login `item_id` by `found` at `now_ms`, for the conflict of `reason`: a
/// password that differs is written anew (the one it replaces goes into the history); a code that
/// differs is linked anew (the account linked before stays in the code list); a backup's version
/// of the same login is taken back as a restore that replaces does. A link the import maps to no
/// account leaves the login's own. `false` when there is nothing to replace, or the login holds
/// what a later Lockra wrote.
pub(super) fn replace(data: &mut VaultData, item_id: Uuid, found: &FoundLogin, reason: ConflictReason, link: Option<Uuid>, now_ms: u64) -> bool {
    let Some(index) = data.items.iter().position(|i| i.id == item_id) else { return false };
    if holds_later_values(&data.items[index]) {
        return false;
    }
    let mut item = data.items[index].clone();
    match (reason, found) {
        (ConflictReason::Changed, FoundLogin::Backup(backup)) => {
            let mut restored = (**backup).clone();
            restored.totp_entry = link.or(item.totp_entry);
            item.take_back(&restored, now_ms, || data.tick(now_ms));
        }
        _ => {
            if reason == ConflictReason::Password {
                let field = item.field_of(Purpose::Password).map_or_else(Uuid::new_v4, |f| f.id);
                let password = Zeroizing::new(found.password().to_owned());
                let (stamp, wid) = (data.tick(now_ms), random_write_id());
                item.write_field(field, Purpose::Password, stamp, wid, now_ms, |f| {
                    f.value = password;
                    f.hidden = true;
                    f.removed = false;
                });
            }
            if let Some(link) = link.filter(|l| item.totp_entry != Some(*l)) {
                item.totp_entry = Some(link);
                let (stamp, mwid) = (data.tick(now_ms), random_write_id());
                item.meta_written(stamp, mwid, now_ms);
            }
        }
    }
    data.items[index] = item;
    true
}
