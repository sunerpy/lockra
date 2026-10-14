//! Login items (docs/formats.md §2): the passwords the vault keeps beside the codes.
//!
//! An item merges part by part, so that two devices changing different parts of it at once both
//! keep their change: its metadata (title, URLs, notes, tags, …) is one register, each field
//! (user name, email, password, custom fields) another, every register ordered by its stamp, then
//! its write id, then its content. A hidden value a write replaces stays in a short history, and
//! a conflict (two changes of one field made without seeing each other) is derived from it: each
//! value records which earlier writes its author had seen. A deletion removes the parts it covers,
//! those stamped at or before it ([`Record::forget`]); a change made after it brings the item back
//! with what came later.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use lockra_sync::{Hlc, Record};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::AccountColor;
use crate::lenient::Lenient;

/// The most fields an item holds.
pub const MAX_FIELDS: usize = 64;
/// The most URLs an item holds.
pub const MAX_URLS: usize = 32;
/// The longest notes an item holds, in bytes.
pub const MAX_NOTES_BYTES: usize = 64 * 1024;
/// The most earlier values an item's history keeps.
pub const MAX_HISTORY: usize = 20;

/// What an item is. There are logins only; an item of a later kind is kept as it was written and
/// shown read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// A sign-in: user name, password, the sites it is for.
    #[default]
    Login,
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// The user name.
    Username,
    /// The email address, kept apart from the user name.
    Email,
    /// The password.
    Password,
    /// A field of the user's own: text, or hidden like a password.
    Custom,
}

impl Purpose {
    /// Where the fields of this purpose come in an item.
    fn rank(purpose: &Lenient<Self>) -> u8 {
        match purpose {
            Lenient::Known(Self::Username) => 0,
            Lenient::Known(Self::Email) => 1,
            Lenient::Known(Self::Password) => 2,
            Lenient::Known(Self::Custom) => 3,
            Lenient::Unknown(_) => 4,
        }
    }
}

/// How a saved URL is compared with the page (lockra-autofill's rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Matching {
    /// The same registrable domain, or an equivalent one. The default.
    #[default]
    Domain,
    /// The same host and port.
    Host,
    /// The page's URL starts with the saved one.
    StartsWith,
    /// The saved text is a regular expression.
    Regex,
    /// The same URL.
    Exact,
    /// Never: this URL is kept but matches nothing.
    Never,
}

/// What the browser extension may do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    /// Fill it when asked. The default.
    #[default]
    Fill,
    /// Ask for the master password or the biometric check before showing, copying or filling it.
    Confirm,
    /// Never hand it to the extension.
    NoExtension,
}

/// An item's own choice for an autofill preference: the policy's, or on, or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tri {
    /// As the policy says.
    #[default]
    Default,
    /// On for this item.
    On,
    /// Off for this item.
    Off,
}

/// A site rule's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRuleKind {
    /// Never offer to save a password on these sites.
    NeverSave,
    /// Never fill anything on these sites.
    NeverFill,
    /// These domains share their sign-ins.
    Equivalent,
}

impl SiteRuleKind {
    fn name(self) -> &'static str {
        match self {
            Self::NeverSave => "never_save",
            Self::NeverFill => "never_fill",
            Self::Equivalent => "equivalent",
        }
    }
}

/// A URL saved with an item, with how it is compared with the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlRule {
    /// The URL, or the pattern for [`Matching::Regex`].
    pub url: String,
    /// How it is compared.
    #[serde(default, skip_serializing_if = "is_default")]
    pub matching: Lenient<Matching>,
    /// What a later Lockra keeps here: written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl UrlRule {
    /// How it is compared: a rule of a later kind matches nothing (rather not fill than fill
    /// where the user did not mean).
    pub fn rule(&self) -> Matching {
        self.matching.or(Matching::Never)
    }
}

/// An item's own autofill preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutofillPrefs {
    /// Fill it as the page loads.
    #[serde(default, skip_serializing_if = "is_default")]
    pub on_page_load: Lenient<Tri>,
    /// Submit the form once filled.
    #[serde(default, skip_serializing_if = "is_default")]
    pub auto_submit: Lenient<Tri>,
    /// Fill the linked code into a one-time-code field.
    #[serde(default, skip_serializing_if = "is_default")]
    pub fill_otp: Lenient<Tri>,
    /// What a later Lockra keeps here: written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One field of an item, a register of its own: the user name, the email, the password or a
/// custom field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Stable id.
    pub id: Uuid,
    /// What it holds.
    pub purpose: Lenient<Purpose>,
    /// Its name, for a custom field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// What it holds now.
    #[serde(default)]
    pub value: Zeroizing<String>,
    /// Shown masked; its earlier values go into the item's history.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// The page's field it fills (its `name` or `id`), for a custom field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_field: Option<String>,
    /// Removed from the item: the register stays, so that the removal syncs.
    #[serde(default, skip_serializing_if = "is_false")]
    pub removed: bool,
    /// When it was added; never changes. With the purpose and the id, it orders the fields.
    pub created: Hlc,
    /// When its content was written: the register's stamp.
    pub stamp: Hlc,
    /// Which write put the content there: a random number for new content, the same number when a
    /// known value is taken back (a restore). With the stamp, it orders two writes.
    pub wid: u64,
    /// The writes of this field the author of this content had seen (sorted): those still in the
    /// history. A value in the history that is neither seen nor this one was written apart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<u64>,
    /// What a later Lockra keeps here: written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The content of a field's register, as its order compares it.
#[derive(Serialize)]
struct FieldContent<'a> {
    purpose: &'a Lenient<Purpose>,
    label: &'a str,
    value: &'a str,
    hidden: bool,
    page_field: &'a Option<String>,
    removed: bool,
    created: Hlc,
    extra: &'a Map<String, Value>,
}

impl Field {
    /// A new, empty field added at `created`.
    pub(crate) fn new(id: Uuid, purpose: Purpose, created: Hlc) -> Self {
        Self {
            id,
            purpose: Lenient::Known(purpose),
            label: String::new(),
            value: Zeroizing::new(String::new()),
            hidden: false,
            page_field: None,
            removed: false,
            created,
            stamp: created,
            wid: 0,
            seen: Vec::new(),
            extra: Map::new(),
        }
    }

    /// The value as the history keeps it, for a hidden field.
    fn past(&self) -> Option<PastValue> {
        self.hidden.then(|| PastValue { field: self.id, value: self.value.clone(), set_at: self.stamp, wid: self.wid })
    }

    fn content(&self) -> Zeroizing<Vec<u8>> {
        let content = FieldContent {
            purpose: &self.purpose,
            label: &self.label,
            value: &self.value,
            hidden: self.hidden,
            page_field: &self.page_field,
            removed: self.removed,
            created: self.created,
            extra: &self.extra,
        };
        to_bytes(&content)
    }

    /// The order of two versions of a field: the later write, then the larger write id, then the
    /// larger content, so that every replica picks the same one.
    fn order(&self, other: &Self) -> Ordering {
        (self.stamp, self.wid).cmp(&(other.stamp, other.wid)).then_with(|| self.content().cmp(&other.content()))
    }
}

/// A hidden value a field held before: what the history keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastValue {
    /// The field it was in.
    pub field: Uuid,
    /// The value.
    pub value: Zeroizing<String>,
    /// When it was written.
    pub set_at: Hlc,
    /// The write that put it there.
    pub wid: u64,
}

/// A login, as the vault keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultItem {
    /// Stable id.
    pub id: Uuid,
    /// What it is; one of a later kind is read-only here.
    #[serde(default)]
    pub kind: Lenient<ItemKind>,
    /// Its name.
    #[serde(default)]
    pub title: String,
    /// User name, email, password and custom fields, each a register of its own, in one order
    /// (purpose, then when added, then id).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    /// The sites it is for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<UrlRule>,
    /// Notes, treated as a secret: shown only on request.
    #[serde(default, skip_serializing_if = "is_blank")]
    pub notes: Zeroizing<String>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Group (a folder name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Pinned to the top.
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    /// The colour it is shown in.
    #[serde(default, skip_serializing_if = "is_default")]
    pub color: Lenient<AccountColor>,
    /// What its avatar shows instead of the title's initial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
    /// What the browser extension may do with it; a level of a later kind gives it nothing.
    #[serde(default, skip_serializing_if = "is_default")]
    pub protection: Lenient<Protection>,
    /// Its own autofill preferences.
    #[serde(default, skip_serializing_if = "is_default")]
    pub autofill: AutofillPrefs,
    /// The account whose codes go with it (an entry's id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_entry: Option<Uuid>,
    /// Hidden values its fields held before, newest first, at most [`MAX_HISTORY`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<PastValue>,
    /// Where it came from (`"bitwarden"`, `"kdbx"`, …), when imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Unix milliseconds.
    #[serde(default)]
    pub created_at_ms: u64,
    /// The last change of any part, Unix milliseconds.
    #[serde(default)]
    pub updated_at_ms: u64,
    /// Last copy or fill, Unix milliseconds. This device's own: it does not sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<u64>,
    /// When the metadata (everything but the fields and the history) was written.
    #[serde(default)]
    pub stamp: Hlc,
    /// Which write put the metadata there; with the stamp, it orders two writes.
    #[serde(default)]
    pub mwid: u64,
    /// What a later Lockra keeps here: written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The metadata register's content, as its order compares it.
#[derive(Serialize)]
struct MetaContent<'a> {
    kind: &'a Lenient<ItemKind>,
    title: &'a str,
    urls: &'a [UrlRule],
    notes: &'a str,
    tags: &'a [String],
    group: &'a Option<String>,
    favorite: bool,
    color: &'a Lenient<AccountColor>,
    mark: &'a Option<String>,
    protection: &'a Lenient<Protection>,
    autofill: &'a AutofillPrefs,
    totp_entry: Option<Uuid>,
    source: &'a Option<String>,
    created_at_ms: u64,
    extra: &'a Map<String, Value>,
}

impl Record for VaultItem {
    fn id(&self) -> Uuid {
        self.id
    }

    /// The latest of its registers.
    fn stamp(&self) -> Hlc {
        self.fields.iter().map(|f| f.stamp).fold(self.stamp, Hlc::max)
    }

    fn fold(&mut self, theirs: &Self) -> bool {
        let history = self.history.clone();
        let mut changed = false;
        if theirs.meta_order(self) == Ordering::Greater {
            self.take_meta(theirs);
            changed = true;
        }
        if theirs.updated_at_ms > self.updated_at_ms {
            self.updated_at_ms = theirs.updated_at_ms;
            changed = true;
        }
        let index: BTreeMap<Uuid, usize> = self.fields.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
        let mut added = Vec::new();
        for field in &theirs.fields {
            let Some(&i) = index.get(&field.id) else {
                added.push(field.clone());
                continue;
            };
            let own = &mut self.fields[i];
            match field.order(own) {
                // Theirs is the later write: ours goes into the history, unless it is the same
                // write taken back.
                Ordering::Greater => {
                    let lost = std::mem::replace(own, field.clone());
                    if lost.wid != field.wid {
                        self.history.extend(lost.past());
                    }
                    changed = true;
                }
                Ordering::Less => {
                    if field.wid != own.wid {
                        self.history.extend(field.past());
                    }
                }
                // The same write on both sides (two copies of a vault that took the same value
                // back at the same moment): it has seen what either side had.
                Ordering::Equal => {
                    let seen = union(&own.seen, &field.seen);
                    if seen != own.seen {
                        own.seen = seen;
                        changed = true;
                    }
                }
            }
        }
        if !added.is_empty() {
            self.fields.extend(added);
            changed = true;
        }
        self.history.extend(theirs.history.iter().cloned());
        self.normalize();
        changed || self.history != history
    }

    fn forget(&mut self, deleted: Hlc) -> bool {
        let mut changed = false;
        if self.stamp <= deleted && (self.stamp, self.mwid) != (Hlc::default(), 0) {
            self.clear_meta();
            changed = true;
        }
        let (fields, history) = (self.fields.len(), self.history.len());
        self.fields.retain(|f| f.stamp > deleted);
        self.history.retain(|p| p.set_at > deleted);
        changed || fields != self.fields.len() || history != self.history.len()
    }
}

impl VaultItem {
    /// A new, empty login made at `now_ms`, its metadata stamped `stamp` by the fresh write `mwid`.
    pub(crate) fn login(id: Uuid, now_ms: u64, stamp: Hlc, mwid: u64) -> Self {
        Self {
            id,
            kind: Lenient::Known(ItemKind::Login),
            title: String::new(),
            fields: Vec::new(),
            urls: Vec::new(),
            notes: Zeroizing::new(String::new()),
            tags: Vec::new(),
            group: None,
            favorite: false,
            color: Lenient::default(),
            mark: None,
            protection: Lenient::default(),
            autofill: AutofillPrefs::default(),
            totp_entry: None,
            history: Vec::new(),
            source: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_used_at_ms: None,
            stamp,
            mwid,
            extra: Map::new(),
        }
    }

    /// Of a later kind: kept and shown, never changed here.
    pub fn read_only(&self) -> bool {
        self.kind.known().is_none()
    }

    /// What the browser extension may do with it: a level of a later kind gives it nothing.
    pub fn protection_level(&self) -> Protection {
        self.protection.or(Protection::NoExtension)
    }

    /// The field with `id`.
    pub fn field(&self, id: Uuid) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    /// The first field of `purpose` that is not removed.
    pub fn field_of(&self, purpose: Purpose) -> Option<&Field> {
        self.fields.iter().find(|f| !f.removed && f.purpose == Lenient::Known(purpose))
    }

    /// The fields whose current value was written without seeing another value the history
    /// holds for them, and that differs from it: two changes made apart. Settling one
    /// ([`Self::settle_field`]) writes a value that has seen them all.
    pub fn conflicted_fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(|field| {
            self.history
                .iter()
                .any(|past| past.field == field.id && past.wid != field.wid && field.seen.binary_search(&past.wid).is_err() && past.value != field.value)
        })
    }

    /// Two changes of one of its fields were made apart.
    pub fn conflict(&self) -> bool {
        self.conflicted_fields().next().is_some()
    }

    /// The metadata changed at `now_ms`: the register takes `stamp`, written by the fresh `mwid`.
    pub(crate) fn meta_written(&mut self, stamp: Hlc, mwid: u64, now_ms: u64) {
        self.stamp = stamp;
        self.mwid = mwid;
        self.updated_at_ms = self.updated_at_ms.max(now_ms);
    }

    /// Write new content into field `id` at `now_ms` (a new field of `purpose` when there is
    /// none): `change` edits a copy of what it holds; the write is stamped `stamp` and gets the
    /// fresh write id `wid`. The value it replaces goes into the history (a hidden one), and the
    /// new value has seen it and all it had seen.
    pub(crate) fn write_field(&mut self, id: Uuid, purpose: Purpose, stamp: Hlc, wid: u64, now_ms: u64, change: impl FnOnce(&mut Field)) {
        let mut field = self.field(id).cloned().unwrap_or_else(|| Field::new(id, purpose, stamp));
        change(&mut field);
        field.stamp = stamp;
        field.wid = wid;
        self.replace_field(field, Vec::new(), now_ms);
    }

    /// Take a known value back into its field at `now_ms` (a restore): its content and its write
    /// id stay, the stamp is `stamp`. It has seen what it had seen and what it replaces.
    pub(crate) fn readopt_field(&mut self, known: &Field, stamp: Hlc, now_ms: u64) {
        let seen = known.seen.clone();
        self.replace_field(Field { stamp, ..known.clone() }, seen, now_ms);
    }

    /// Settle field `id`'s conflict at `now_ms` with `value` (the one kept, or the one taken from
    /// the history): a new write that has seen every value the history holds for the field, so
    /// the conflict is gone on every replica once it syncs.
    pub(crate) fn settle_field(&mut self, id: Uuid, value: Zeroizing<String>, stamp: Hlc, wid: u64, now_ms: u64) -> bool {
        let Some(mut field) = self.field(id).cloned() else { return false };
        let seen: Vec<u64> = self.history.iter().filter(|p| p.field == id).map(|p| p.wid).collect();
        field.value = value;
        field.stamp = stamp;
        field.wid = wid;
        self.replace_field(field, seen, now_ms);
        true
    }

    /// Take a backup's version of this login back at `now_ms` (a restore that replaces): its
    /// metadata and each of its fields keep their write ids and get stamps from `tick`, later than
    /// anything the space holds; the values they replace go into the history and count as seen;
    /// the backup's history joins this one; a field the backup does not have is removed by a new
    /// write. An item new here starts from the backup's, its parts stamped anew alike.
    pub(crate) fn take_back(&mut self, restored: &Self, now_ms: u64, mut tick: impl FnMut() -> Hlc) {
        self.history.extend(restored.history.iter().cloned());
        let stamp = tick();
        self.take_meta(restored);
        self.stamp = stamp;
        for field in &restored.fields {
            self.readopt_field(field, tick(), now_ms);
        }
        let gone: Vec<Field> = self.fields.iter().filter(|f| !f.removed && restored.field(f.id).is_none()).cloned().collect();
        for field in gone {
            let purpose = field.purpose.or(Purpose::Custom);
            self.write_field(field.id, purpose, tick(), random_write_id(), now_ms, |f| {
                f.removed = true;
                f.value = Zeroizing::new(String::new());
            });
        }
        self.updated_at_ms = self.updated_at_ms.max(now_ms);
        self.normalize();
    }

    /// The latest stamp anywhere in it: its registers, when its fields were added, its history.
    pub(crate) fn latest_stamp(&self) -> Hlc {
        let fields = self.fields.iter().flat_map(|f| [f.created, f.stamp]);
        let history = self.history.iter().map(|p| p.set_at);
        fields.chain(history).fold(self.stamp, Hlc::max)
    }

    fn replace_field(&mut self, mut new: Field, mut seen: Vec<u64>, now_ms: u64) {
        if let Some(index) = self.fields.iter().position(|f| f.id == new.id) {
            let old = self.fields.swap_remove(index);
            new.created = old.created;
            if old.wid != new.wid {
                self.history.extend(old.past());
            }
            seen.push(old.wid);
            seen.extend(old.seen.iter().copied());
        }
        let id = new.id;
        new.seen = Vec::new();
        self.fields.push(new);
        self.updated_at_ms = self.updated_at_ms.max(now_ms);
        self.normalize();
        // Seen is kept to what the history still holds: past it, nothing can conflict.
        let held: BTreeSet<u64> = self.history.iter().filter(|p| p.field == id).map(|p| p.wid).collect();
        seen.retain(|wid| held.contains(wid));
        seen.sort_unstable();
        seen.dedup();
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == id) {
            field.seen = seen;
        }
    }

    /// Already in the order [`Self::normalize`] leaves an item in, as every write and merge here
    /// leave it; one taken in whole from another device's snapshot may not be.
    pub(crate) fn is_tidy(&self) -> bool {
        let field_key = |f: &Field| (Purpose::rank(&f.purpose), f.created, f.id);
        let history_key = |p: &PastValue| (p.set_at, p.wid, p.field);
        self.fields.windows(2).all(|w| field_key(&w[0]) < field_key(&w[1]))
            && self.history.len() <= MAX_HISTORY
            && self.history.windows(2).all(|w| history_key(&w[0]) > history_key(&w[1]))
            && self.history.iter().enumerate().all(|(i, p)| {
                self.history[..i].iter().all(|q| (q.field, q.wid) != (p.field, p.wid)) && !self.fields.iter().any(|f| f.id == p.field && f.wid == p.wid)
            })
    }

    /// The fields in their one order, and the history as its rules keep it: one entry per write
    /// of a field (the latest time it was set), none for a value a field holds now, the newest
    /// [`MAX_HISTORY`]. Every step is the same on every replica, whatever came in first.
    pub(crate) fn normalize(&mut self) {
        self.fields.sort_by(|a, b| (Purpose::rank(&a.purpose), a.created, a.id).cmp(&(Purpose::rank(&b.purpose), b.created, b.id)));
        let current: BTreeSet<(Uuid, u64)> = self.fields.iter().map(|f| (f.id, f.wid)).collect();
        let mut history = std::mem::take(&mut self.history);
        history.sort_by(|a, b| (a.field, a.wid).cmp(&(b.field, b.wid)).then_with(|| (b.set_at, b.value.as_bytes()).cmp(&(a.set_at, a.value.as_bytes()))));
        history.dedup_by(|later, kept| later.field == kept.field && later.wid == kept.wid);
        history.retain(|p| !current.contains(&(p.field, p.wid)));
        history.sort_by(|a, b| (b.set_at, b.wid, b.field, b.value.as_bytes()).cmp(&(a.set_at, a.wid, a.field, a.value.as_bytes())));
        history.truncate(MAX_HISTORY);
        self.history = history;
    }

    fn meta_content(&self) -> Zeroizing<Vec<u8>> {
        let content = MetaContent {
            kind: &self.kind,
            title: &self.title,
            urls: &self.urls,
            notes: &self.notes,
            tags: &self.tags,
            group: &self.group,
            favorite: self.favorite,
            color: &self.color,
            mark: &self.mark,
            protection: &self.protection,
            autofill: &self.autofill,
            totp_entry: self.totp_entry,
            source: &self.source,
            created_at_ms: self.created_at_ms,
            extra: &self.extra,
        };
        to_bytes(&content)
    }

    /// The order of two versions of the metadata: the later write, then the larger write id, then
    /// the larger content.
    fn meta_order(&self, other: &Self) -> Ordering {
        (self.stamp, self.mwid).cmp(&(other.stamp, other.mwid)).then_with(|| self.meta_content().cmp(&other.meta_content()))
    }

    fn take_meta(&mut self, theirs: &Self) {
        self.kind = theirs.kind.clone();
        self.title = theirs.title.clone();
        self.urls = theirs.urls.clone();
        self.notes = theirs.notes.clone();
        self.tags = theirs.tags.clone();
        self.group = theirs.group.clone();
        self.favorite = theirs.favorite;
        self.color = theirs.color.clone();
        self.mark = theirs.mark.clone();
        self.protection = theirs.protection.clone();
        self.autofill = theirs.autofill.clone();
        self.totp_entry = theirs.totp_entry;
        self.source = theirs.source.clone();
        self.created_at_ms = theirs.created_at_ms;
        self.extra = theirs.extra.clone();
        self.stamp = theirs.stamp;
        self.mwid = theirs.mwid;
    }

    /// The metadata a deletion covered: nothing but what the item is, where it came from and when
    /// it was made, under no write at all, so that any later one wins.
    fn clear_meta(&mut self) {
        let blank = Self::login(self.id, self.created_at_ms, Hlc::default(), 0);
        let kind = self.kind.clone();
        let source = self.source.take();
        self.take_meta(&blank);
        self.kind = kind;
        self.source = source;
    }
}

/// A rule about sites: never save there, never fill there, or domains that share their sign-ins.
/// Merged as a whole, the later version winning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteRule {
    /// Stable id; for a rule about one domain, [`SiteRule::single_id`].
    pub id: Uuid,
    /// What it says; one of a later kind says nothing here.
    pub kind: Lenient<SiteRuleKind>,
    /// The domains it is about.
    #[serde(default)]
    pub domains: Vec<String>,
    /// When it last changed.
    pub stamp: Hlc,
    /// What a later Lockra keeps here: written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Record for SiteRule {
    fn id(&self) -> Uuid {
        self.id
    }

    fn stamp(&self) -> Hlc {
        self.stamp
    }
}

impl SiteRule {
    /// The id of a rule of `kind` about the one `domain`: the same on every device, so that two
    /// devices adding the same rule end up with one. SHA-256 of the kind, a zero byte and the
    /// domain in lower case, its first 16 bytes as a version 8 UUID.
    pub fn single_id(kind: SiteRuleKind, domain: &str) -> Uuid {
        let mut hash = Sha256::new();
        hash.update(kind.name().as_bytes());
        hash.update([0]);
        hash.update(domain.to_lowercase().as_bytes());
        let digest = hash.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }
}

/// A password the generator made, in this device's history (the vault's local part).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GeneratedPassword {
    /// The password.
    pub password: Zeroizing<String>,
    /// When it was copied or used, Unix milliseconds.
    pub at_ms: u64,
    /// The site it was made for, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
}

/// How this device shows the password list (the vault's local part).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ItemListView {
    /// The groups folded in the list ("" for the items in no group).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed_groups: Vec<String>,
}

impl ItemListView {
    pub(crate) fn is_empty(&self) -> bool {
        self.collapsed_groups.is_empty()
    }
}

/// What a sign-up form is filled with by default (the vault's local part).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identity {
    /// The user name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    /// The email address.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
}

impl Identity {
    pub(crate) fn is_empty(&self) -> bool {
        self.username.is_empty() && self.email.is_empty()
    }
}

/// A new write id: 64 random bits (the two halves of a random UUID, whose fixed bits sit at
/// different places, folded together).
pub(crate) fn random_write_id() -> u64 {
    let (high, low) = Uuid::new_v4().as_u64_pair();
    high ^ low
}

/// What `seen` lists: both sides', sorted, once each.
fn union(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut all: Vec<u64> = a.iter().chain(b).copied().collect();
    all.sort_unstable();
    all.dedup();
    all
}

fn to_bytes(value: &impl Serialize) -> Zeroizing<Vec<u8>> {
    // Serializing plain data structures to JSON cannot fail.
    #[allow(clippy::expect_used)]
    Zeroizing::new(serde_json::to_vec(value).expect("item parts serialize"))
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_blank(text: &Zeroizing<String>) -> bool {
    text.is_empty()
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests;
