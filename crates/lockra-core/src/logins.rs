//! Logins as the webview sees them and changes them: the draft of a new one, the patch of an
//! edit, the secret-free view of `items_list`, and the answers that show a secret. Secrets travel
//! one way only, from the webview to the core, except in [`ItemRevealed`] and the generator's
//! answers (docs/security.md).

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::{AccountColor, MAX_NAME_CHARS, clean_mark, clean_name};
use crate::error::{CoreResult, ErrorCode};
use crate::item::{
    AutofillPrefs, Field, MAX_FIELDS, MAX_NOTES_BYTES, MAX_URLS, Matching, PastValue, Protection, Purpose, SiteRule, SiteRuleKind, Tri, UrlRule, VaultItem,
};
use crate::lenient::Lenient;

/// The longest value a field holds, in bytes.
pub const MAX_VALUE_BYTES: usize = 4096;
/// The most tags a login has.
pub const MAX_TAGS: usize = 32;
/// The longest tag (characters).
pub const MAX_TAG_CHARS: usize = 64;
/// The longest URL or pattern kept (characters).
pub const MAX_URL_CHARS: usize = 2048;
/// The most domains a site rule names.
pub const MAX_RULE_DOMAINS: usize = 32;

/// What becomes of a secret in an edit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretChange {
    /// It stays as it is (for a new login: there is none).
    #[default]
    Keep,
    /// It becomes `value`.
    Set {
        /// The new value.
        value: Zeroizing<String>,
    },
    /// It is emptied.
    Clear,
    /// It becomes the password the generator made last, which the webview need not send back.
    Generated,
}

/// A saved URL as the webview sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UrlDraft {
    /// The URL, or the pattern of a [`Matching::Regex`] rule.
    pub url: String,
    /// How it is compared with the page.
    #[serde(default)]
    pub matching: Matching,
}

/// An item's own autofill preferences, as the webview sends them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutofillChoice {
    /// Fill it as the page loads.
    #[serde(default)]
    pub on_page_load: Tri,
    /// Submit the form once filled.
    #[serde(default)]
    pub auto_submit: Tri,
    /// Fill the linked code into a one-time-code field.
    #[serde(default)]
    pub fill_otp: Tri,
}

/// A custom field added, changed or removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldChange {
    /// The field; none for a new one.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Its name.
    #[serde(default)]
    pub label: Option<String>,
    /// Its value.
    #[serde(default)]
    pub value: SecretChange,
    /// Shown masked, its earlier values kept in the history.
    #[serde(default)]
    pub hidden: Option<bool>,
    /// The page's field it fills (`name` or `id`); `""` fills none.
    #[serde(default)]
    pub page_field: Option<String>,
    /// Take it out of the login.
    #[serde(default)]
    pub remove: bool,
}

/// A new login.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDraft {
    /// Its name.
    #[serde(default)]
    pub title: String,
    /// The user name.
    #[serde(default)]
    pub username: String,
    /// The email address.
    #[serde(default)]
    pub email: String,
    /// The password: typed (`set`), the generator's last one (`generated`), or none (`keep`).
    #[serde(default)]
    pub password: SecretChange,
    /// The sites it is for.
    #[serde(default)]
    pub urls: Vec<UrlDraft>,
    /// Notes.
    #[serde(default)]
    pub notes: Zeroizing<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Group.
    #[serde(default)]
    pub group: Option<String>,
    /// Pinned.
    #[serde(default)]
    pub favorite: bool,
    /// Its colour.
    #[serde(default)]
    pub color: AccountColor,
    /// What its avatar shows.
    #[serde(default)]
    pub mark: Option<String>,
    /// What the browser extension may do with it.
    #[serde(default)]
    pub protection: Protection,
    /// Its own autofill preferences.
    #[serde(default)]
    pub autofill: AutofillChoice,
    /// The account whose codes go with it.
    #[serde(default)]
    pub totp_entry: Option<Uuid>,
    /// Custom fields.
    #[serde(default)]
    pub fields: Vec<FieldChange>,
}

/// Settle a field's conflict: take one of the values its history holds, or keep the current one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settle {
    /// The field.
    pub field: Uuid,
    /// The value to take ([`PastView::id`]); none keeps the current one.
    #[serde(default)]
    pub take: Option<String>,
}

/// What an edit changes; what it leaves out stays as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPatch {
    /// New name.
    #[serde(default)]
    pub title: Option<String>,
    /// New user name; `""` clears it.
    #[serde(default)]
    pub username: Option<String>,
    /// New email address; `""` clears it.
    #[serde(default)]
    pub email: Option<String>,
    /// The password.
    #[serde(default)]
    pub password: SecretChange,
    /// The sites, all of them.
    #[serde(default)]
    pub urls: Option<Vec<UrlDraft>>,
    /// The notes.
    #[serde(default)]
    pub notes: SecretChange,
    /// The tags, all of them.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// New group; `""` takes it out of its group.
    #[serde(default)]
    pub group: Option<String>,
    /// Pin or unpin.
    #[serde(default)]
    pub favorite: Option<bool>,
    /// New colour.
    #[serde(default)]
    pub color: Option<AccountColor>,
    /// New mark; `""` goes back to the title's initial.
    #[serde(default)]
    pub mark: Option<String>,
    /// New protection.
    #[serde(default)]
    pub protection: Option<Protection>,
    /// New autofill preferences.
    #[serde(default)]
    pub autofill: Option<AutofillChoice>,
    /// The linked account; `null` unlinks it.
    #[serde(default, deserialize_with = "present")]
    pub totp_entry: Option<Option<Uuid>>,
    /// Custom fields added, changed or removed.
    #[serde(default)]
    pub fields: Vec<FieldChange>,
    /// Conflicts settled.
    #[serde(default)]
    pub settle: Vec<Settle>,
}

/// A field that is there, even as `null`: `Some(None)`; a field left out stays `None`.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// What `item_reveal` shows of a login.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevealParts {
    /// The password.
    #[serde(default)]
    pub password: bool,
    /// The notes.
    #[serde(default)]
    pub notes: bool,
    /// The earlier values.
    #[serde(default)]
    pub history: bool,
    /// These hidden fields' values.
    #[serde(default)]
    pub fields: Vec<Uuid>,
}

/// What `item_copy` puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CopyPart {
    /// The user name.
    Username,
    /// The email address.
    Email,
    /// The password.
    Password,
    /// The current code of the linked account.
    Totp,
    /// A custom field's value.
    Field {
        /// Which.
        id: Uuid,
    },
}

/// A saved URL, as the webview sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UrlView {
    /// The URL or pattern.
    pub url: String,
    /// How it is compared (absent: `domain`); one of a later kind shows as `never`, which it acts
    /// as.
    #[serde(skip_serializing_if = "is_default")]
    pub matching: Matching,
}

/// A custom field, as the webview sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldView {
    /// Stable id.
    pub id: Uuid,
    /// Its name.
    pub label: String,
    /// Masked: its value is shown only on `item_reveal`.
    pub hidden: bool,
    /// The value of a field that is not hidden; none for a hidden one.
    pub value: Option<String>,
    /// It holds something.
    pub has_value: bool,
    /// The page's field it fills.
    pub page_field: Option<String>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_login(kind: &str) -> bool {
    kind == "login"
}

/// A field in conflict: two of its changes were made apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictView {
    /// The field.
    pub field: Uuid,
    /// What it holds.
    pub purpose: Purpose,
    /// Its name, for a custom field.
    pub label: String,
}

/// A login as `items_list` shows it: no secret. A part at its default is left out of the JSON
/// (the webview's schema fills it in), which keeps 3000 logins under 1.5 MB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemView {
    /// Stable id.
    pub id: Uuid,
    /// `login` (absent), or the name a later Lockra gave its kind.
    #[serde(skip_serializing_if = "is_login")]
    pub kind: String,
    /// Its name.
    pub title: String,
    /// The user name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    /// The email address.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// It has a password.
    #[serde(skip_serializing_if = "is_default")]
    pub has_password: bool,
    /// The sites it is for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<UrlView>,
    /// It has notes (shown only on `item_reveal`).
    #[serde(skip_serializing_if = "is_default")]
    pub has_notes: bool,
    /// Tags.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Pinned.
    #[serde(skip_serializing_if = "is_default")]
    pub favorite: bool,
    /// Its colour; one of a later Lockra shows as `auto`.
    #[serde(skip_serializing_if = "is_default")]
    pub color: AccountColor,
    /// What its avatar shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
    /// What the browser extension may do with it; one of a later kind shows as `no_extension`.
    #[serde(skip_serializing_if = "is_default")]
    pub protection: Protection,
    /// Its own autofill preferences.
    #[serde(skip_serializing_if = "is_default")]
    pub autofill: AutofillChoice,
    /// The linked account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_entry: Option<Uuid>,
    /// Custom fields, removed ones left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldView>,
    /// Earlier values kept.
    #[serde(skip_serializing_if = "is_default")]
    pub history_count: u32,
    /// Fields in conflict.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ConflictView>,
    /// Where it came from, when imported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Unix milliseconds.
    pub created_at_ms: u64,
    /// Unix milliseconds.
    pub updated_at_ms: u64,
    /// Last copy or fill here, Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<u64>,
    /// Holds what a later Lockra wrote that this one cannot change without losing it: shown, not
    /// edited.
    #[serde(skip_serializing_if = "is_default")]
    pub read_only: bool,
}

/// An earlier value, as `item_reveal` shows the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PastView {
    /// What `item_update`'s `settle` names it by.
    pub id: String,
    /// The field it was in.
    pub field: Uuid,
    /// The value.
    pub value: String,
    /// When it was written, Unix milliseconds.
    pub set_at_ms: u64,
    /// Written apart from the field's current value: one side of a conflict.
    pub conflict: bool,
}

/// A hidden field's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevealedField {
    /// The field.
    pub id: Uuid,
    /// Its value.
    pub value: String,
}

/// The answer to `item_reveal`: the parts asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemRevealed {
    /// Which login.
    pub item_id: Uuid,
    /// The password, when asked for.
    pub password: Option<String>,
    /// The notes, when asked for.
    pub notes: Option<String>,
    /// Hidden fields' values, those asked for.
    pub fields: Vec<RevealedField>,
    /// The earlier values, newest first, when asked for.
    pub history: Option<Vec<PastView>>,
}

/// A site rule, as the webview sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteRuleView {
    /// Stable id.
    pub id: Uuid,
    /// What it says, or the name a later Lockra gave its kind (it says nothing here).
    pub kind: String,
    /// The domains.
    pub domains: Vec<String>,
}

impl SiteRuleView {
    pub(crate) fn of(rule: &SiteRule) -> Self {
        Self { id: rule.id, kind: lenient_name(&rule.kind), domains: rule.domains.clone() }
    }
}

/// The answer to `generator_generate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedView {
    /// The password.
    pub password: String,
    /// An estimate of its entropy, in bits.
    pub bits: u32,
    /// It was made to a site's rules, and meets them.
    pub rules_met: bool,
}

/// A password of the generator's history, as `generator_history_reveal` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedPastView {
    /// The password.
    pub password: String,
    /// When it was copied or used, Unix milliseconds.
    pub at_ms: u64,
    /// The site it was made for.
    pub site: Option<String>,
}

/// The answer to `password_score`: zxcvbn's score and what to say about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScoreView {
    /// 0 (guessed at once) to 4 (very strong).
    pub score: u8,
    /// Why it is weak, as a code the interface words.
    pub warning: Option<String>,
}

fn lenient_name<T: Serialize>(value: &Lenient<T>) -> String {
    match value {
        Lenient::Known(known) => serde_json::to_value(known).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
        Lenient::Unknown(text) => text.clone(),
    }
}

/// The value of the first field of `purpose` that is not removed.
fn value_of(item: &VaultItem, purpose: Purpose) -> Option<&Field> {
    item.field_of(purpose)
}

/// It holds a value of a kind a later Lockra wrote: changing it here would lose it.
pub(crate) fn holds_later_values(item: &VaultItem) -> bool {
    item.read_only()
        || !matches!(item.protection, Lenient::Known(_))
        || !matches!(item.color, Lenient::Known(_))
        || item.urls.iter().any(|u| !matches!(u.matching, Lenient::Known(_)) || !u.extra.is_empty())
        || [&item.autofill.on_page_load, &item.autofill.auto_submit, &item.autofill.fill_otp].iter().any(|t| !matches!(t, Lenient::Known(_)))
        || !item.autofill.extra.is_empty()
        || !item.extra.is_empty()
        || item.fields.iter().any(|f| !matches!(f.purpose, Lenient::Known(_)) || !f.extra.is_empty())
}

impl ItemView {
    /// The view of `item`: everything but its secrets.
    pub(crate) fn of(item: &VaultItem) -> Self {
        let custom = item.fields.iter().filter(|f| !f.removed && f.purpose == Lenient::Known(Purpose::Custom));
        let conflicts =
            item.conflicted_fields().map(|f| ConflictView { field: f.id, purpose: f.purpose.or(Purpose::Custom), label: f.label.clone() }).collect();
        Self {
            id: item.id,
            kind: lenient_name(&item.kind),
            title: item.title.clone(),
            username: value_of(item, Purpose::Username).map(|f| f.value.to_string()).unwrap_or_default(),
            email: value_of(item, Purpose::Email).map(|f| f.value.to_string()).unwrap_or_default(),
            has_password: value_of(item, Purpose::Password).is_some_and(|f| !f.value.is_empty()),
            urls: item.urls.iter().map(|u| UrlView { url: u.url.clone(), matching: u.rule() }).collect(),
            has_notes: !item.notes.is_empty(),
            tags: item.tags.clone(),
            group: item.group.clone(),
            favorite: item.favorite,
            color: item.color.or(AccountColor::Auto),
            mark: item.mark.clone(),
            protection: item.protection_level(),
            autofill: AutofillChoice {
                on_page_load: item.autofill.on_page_load.or(Tri::Default),
                auto_submit: item.autofill.auto_submit.or(Tri::Default),
                fill_otp: item.autofill.fill_otp.or(Tri::Default),
            },
            totp_entry: item.totp_entry,
            fields: custom
                .map(|f| FieldView {
                    id: f.id,
                    label: f.label.clone(),
                    hidden: f.hidden,
                    value: (!f.hidden).then(|| f.value.to_string()),
                    has_value: !f.value.is_empty(),
                    page_field: f.page_field.clone(),
                })
                .collect(),
            history_count: u32::try_from(item.history.len()).unwrap_or(u32::MAX),
            conflicts,
            source: item.source.clone(),
            created_at_ms: item.created_at_ms,
            updated_at_ms: item.updated_at_ms,
            last_used_at_ms: item.last_used_at_ms,
            read_only: holds_later_values(item),
        }
    }
}

/// The history of `item`, as `item_reveal` shows it.
pub(crate) fn past_views(item: &VaultItem) -> Vec<PastView> {
    item.history
        .iter()
        .map(|past| {
            let conflict = item.field(past.field).is_some_and(|f| past.wid != f.wid && f.seen.binary_search(&past.wid).is_err() && past.value != f.value);
            PastView { id: past.wid.to_string(), field: past.field, value: past.value.to_string(), set_at_ms: past.set_at.wall_ms, conflict }
        })
        .collect()
}

/// The earlier value `id` names in `item`'s history.
pub(crate) fn past_value<'a>(item: &'a VaultItem, field: Uuid, id: &str) -> Option<&'a PastValue> {
    let wid: u64 = id.parse().ok()?;
    item.history.iter().find(|p| p.field == field && p.wid == wid)
}

/// A name as an item keeps it, or `ItemInvalid` past its length.
pub(crate) fn clean_text(text: &str, max_chars: usize) -> CoreResult<String> {
    let cleaned: String = text.trim().chars().filter(|c| !c.is_control()).collect::<String>().trim().to_owned();
    if cleaned.chars().count() > max_chars {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(cleaned)
}

/// A field's value as an item keeps it: unchanged (a password may begin with a space), within
/// [`MAX_VALUE_BYTES`].
pub(crate) fn checked_value(value: Zeroizing<String>) -> CoreResult<Zeroizing<String>> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(value)
}

/// The notes as an item keeps them, within [`MAX_NOTES_BYTES`].
pub(crate) fn checked_notes(notes: Zeroizing<String>) -> CoreResult<Zeroizing<String>> {
    if notes.len() > MAX_NOTES_BYTES {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(notes)
}

/// Saved URLs as an item keeps them: trimmed, the empty ones dropped, at most [`MAX_URLS`].
pub(crate) fn clean_urls(urls: Vec<UrlDraft>) -> CoreResult<Vec<UrlRule>> {
    let mut kept = Vec::new();
    for draft in urls {
        let url = draft.url.trim().to_owned();
        if url.is_empty() {
            continue;
        }
        if url.chars().count() > MAX_URL_CHARS || url.chars().any(char::is_control) {
            return Err(ErrorCode::ItemInvalid.into());
        }
        kept.push(UrlRule { url, matching: Lenient::Known(draft.matching), extra: Default::default() });
    }
    if kept.len() > MAX_URLS {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(kept)
}

/// Tags as an item keeps them: cleaned, the empty and the repeated ones dropped.
pub(crate) fn clean_tags(tags: Vec<String>) -> CoreResult<Vec<String>> {
    let mut kept: Vec<String> = Vec::new();
    for tag in tags {
        let tag = clean_text(&tag, MAX_TAG_CHARS)?;
        if !tag.is_empty() && !kept.iter().any(|k| k.to_lowercase() == tag.to_lowercase()) {
            kept.push(tag);
        }
    }
    if kept.len() > MAX_TAGS {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(kept)
}

/// A group name as an item keeps it; none for an empty one.
pub(crate) fn clean_group(group: &str) -> Option<String> {
    Some(clean_name(group)).filter(|g| !g.is_empty())
}

/// A mark as an item keeps it.
pub(crate) fn clean_item_mark(mark: &str) -> Option<String> {
    clean_mark(mark)
}

/// A title as an item keeps it.
pub(crate) fn clean_title(title: &str) -> CoreResult<String> {
    clean_text(title, MAX_NAME_CHARS)
}

/// The autofill preferences of a choice.
pub(crate) fn autofill_prefs(choice: AutofillChoice) -> AutofillPrefs {
    AutofillPrefs {
        on_page_load: Lenient::Known(choice.on_page_load),
        auto_submit: Lenient::Known(choice.auto_submit),
        fill_otp: Lenient::Known(choice.fill_otp),
        extra: Default::default(),
    }
}

/// The fields an item may hold.
pub(crate) fn check_field_count(item: &VaultItem) -> CoreResult<()> {
    if item.fields.iter().filter(|f| !f.removed).count() > MAX_FIELDS {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(())
}

/// Domains of a site rule: host names in lower case, without a scheme, a path or a port.
pub(crate) fn clean_domains(domains: &[String]) -> CoreResult<Vec<String>> {
    let mut kept: Vec<String> = Vec::new();
    for domain in domains {
        let text = domain.trim().trim_end_matches('.').to_lowercase();
        let host = text.split("://").last().unwrap_or_default().split(['/', '?', '#']).next().unwrap_or_default();
        let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
        let host = match host.find(']') {
            Some(end) if host.starts_with('[') => host[..=end].to_owned(),
            _ => host.split(':').next().unwrap_or_default().to_owned(),
        };
        if host.is_empty() || host.len() > 253 || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ErrorCode::ItemInvalid.into());
        }
        if !kept.contains(&host) {
            kept.push(host);
        }
    }
    if kept.is_empty() || kept.len() > MAX_RULE_DOMAINS {
        return Err(ErrorCode::ItemInvalid.into());
    }
    Ok(kept)
}

/// The id of a new rule of `kind` about `domains`: the shared one for a single domain.
pub(crate) fn rule_id(kind: SiteRuleKind, domains: &[String]) -> Uuid {
    match domains {
        [domain] if kind != SiteRuleKind::Equivalent => SiteRule::single_id(kind, domain),
        _ => Uuid::new_v4(),
    }
}

#[cfg(test)]
mod tests;
