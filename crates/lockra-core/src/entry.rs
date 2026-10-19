//! Entries: what the vault payload stores, and the secret-free view the webview receives.

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, base32};
use lockra_sync::{Clock, Hlc, Record, Tombstone};
use lockra_transfer::{Incompatible, Origin, google, microsoft};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::browser::BrowserLocal;
use crate::error::{CoreResult, ErrorCode};
use crate::item::{GeneratedPassword, ItemListView, SiteRule, VaultItem};
use crate::lenient::{self, Lenient};
use crate::sync::SyncLocal;

/// The longest issuer, account or group name kept (characters).
pub const MAX_NAME_CHARS: usize = 200;

/// One account in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable id.
    pub id: Uuid,
    /// Who issued it.
    pub issuer: String,
    /// The account name.
    pub account: String,
    /// TOTP with its period, or HOTP with the counter of the code shown now.
    pub kind: OtpKind,
    /// HMAC hash.
    pub algorithm: Algorithm,
    /// Digits per code.
    pub digits: Digits,
    /// The shared secret; Base32 inside the encrypted payload.
    #[serde(serialize_with = "secret_to_base32", deserialize_with = "secret_from_base32")]
    pub secret: Zeroizing<Vec<u8>>,
    /// Optional group (a folder name).
    pub group: Option<String>,
    /// Pinned to the top of the list.
    pub favorite: bool,
    /// The colour it is shown in; `auto` follows its name.
    #[serde(default, skip_serializing_if = "AccountColor::is_auto", deserialize_with = "lenient_color")]
    pub color: AccountColor,
    /// What its avatar shows instead of the name's initial ([`clean_mark`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
    /// Where it came from; one a later Lockra added is kept as written (devices of 0.9 cannot read
    /// one: none is added while they are supported, AGENTS.md).
    pub origin: Lenient<Origin>,
    /// Unix milliseconds.
    pub created_at_ms: u64,
    /// Unix milliseconds.
    pub updated_at_ms: u64,
    /// Last copy, Unix milliseconds. This device's own: it does not sync.
    pub last_used_at_ms: Option<u64>,
    /// When it last changed, as the sync orders changes: a hybrid logical clock stamp, renewed by
    /// every change that syncs (names, group, pin, the next HOTP counter).
    #[serde(default)]
    pub stamp: Hlc,
}

impl Record for Entry {
    fn id(&self) -> Uuid {
        self.id
    }

    fn stamp(&self) -> Hlc {
        self.stamp
    }

    /// An HOTP counter never goes back on any device: a code once shown must not come again.
    fn counter(&self) -> Option<u64> {
        match self.kind {
            OtpKind::Hotp { counter } => Some(counter),
            OtpKind::Totp { .. } => None,
        }
    }

    fn raise_counter(&mut self, floor: u64) {
        if let OtpKind::Hotp { counter } = &mut self.kind
            && *counter < floor
        {
            *counter = floor;
        }
    }
}

impl Entry {
    /// A new entry for an account.
    pub fn from_auth(auth: OtpAuth, origin: Origin, now_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            issuer: clean_name(&auth.issuer),
            account: clean_name(&auth.account),
            kind: auth.kind,
            algorithm: auth.algorithm,
            digits: auth.digits,
            secret: auth.secret,
            group: None,
            favorite: false,
            color: AccountColor::Auto,
            mark: None,
            origin: Lenient::Known(origin),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_used_at_ms: None,
            stamp: Hlc::default(),
        }
    }

    /// The account in `otpauth://` form.
    pub fn to_auth(&self) -> OtpAuth {
        OtpAuth {
            kind: self.kind,
            algorithm: self.algorithm,
            digits: self.digits,
            secret: self.secret.clone(),
            issuer: self.issuer.clone(),
            account: self.account.clone(),
        }
    }

    /// Same secret and the same code parameters: the same account, whatever the names.
    pub fn same_account(&self, auth: &OtpAuth) -> bool {
        let same_kind = match (self.kind, auth.kind) {
            (OtpKind::Totp { period: a }, OtpKind::Totp { period: b }) => a == b,
            (OtpKind::Hotp { .. }, OtpKind::Hotp { .. }) => true,
            _ => false,
        };
        same_kind && self.algorithm == auth.algorithm && self.digits == auth.digits && self.secret == auth.secret
    }

    /// Same issuer and account name (case-insensitive) as `auth`.
    pub fn same_names(&self, auth: &OtpAuth) -> bool {
        self.issuer.to_lowercase() == clean_name(&auth.issuer).to_lowercase() && self.account.to_lowercase() == clean_name(&auth.account).to_lowercase()
    }

    /// The webview's view: everything but the secret.
    pub fn view(&self) -> EntryView {
        let auth = self.to_auth();
        EntryView {
            id: self.id,
            issuer: self.issuer.clone(),
            account: self.account.clone(),
            kind: self.kind,
            algorithm: self.algorithm,
            digits: self.digits,
            group: self.group.clone(),
            favorite: self.favorite,
            color: self.color,
            mark: self.mark.clone(),
            origin: self.origin.or(Origin::Uri),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            last_used_at_ms: self.last_used_at_ms,
            export: ExportCompat { google: google::exportable(&auth).err(), microsoft: microsoft::exportable(&auth).err() },
        }
    }
}

/// An entry as the webview sees it: no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryView {
    /// Stable id.
    pub id: Uuid,
    /// Who issued it.
    pub issuer: String,
    /// The account name.
    pub account: String,
    /// TOTP with its period, or HOTP with its counter.
    pub kind: OtpKind,
    /// HMAC hash.
    pub algorithm: Algorithm,
    /// Digits per code.
    pub digits: Digits,
    /// Group.
    pub group: Option<String>,
    /// Pinned.
    pub favorite: bool,
    /// Where it came from.
    /// The colour it is shown in.
    pub color: AccountColor,
    /// What its avatar shows instead of the name's initial.
    pub mark: Option<String>,
    pub origin: Origin,
    /// Unix milliseconds.
    pub created_at_ms: u64,
    /// Unix milliseconds.
    pub updated_at_ms: u64,
    /// Last copy, Unix milliseconds.
    pub last_used_at_ms: Option<u64>,
    /// Why it cannot go to each export target; `null` when it can.
    pub export: ExportCompat,
}

/// Export compatibility per target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExportCompat {
    /// Google Authenticator's migration codes.
    pub google: Option<Incompatible>,
    /// Microsoft Authenticator's "add account" scan.
    pub microsoft: Option<Incompatible>,
}

/// An account typed in by hand.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EntryDraft {
    /// Who issued it.
    #[serde(default)]
    pub issuer: String,
    /// The account name.
    #[serde(default)]
    pub account: String,
    /// The secret as the service shows it (Base32).
    pub secret: Zeroizing<String>,
    /// TOTP or HOTP with its period or counter.
    pub kind: OtpKind,
    /// HMAC hash.
    #[serde(default)]
    pub algorithm: Algorithm,
    /// Digits per code.
    #[serde(default)]
    pub digits: Digits,
    /// Group.
    #[serde(default)]
    pub group: Option<String>,
}

/// What an edit may change: the names, the group, the pin, the colour and the mark. Secrets and
/// code parameters are fixed for the life of an entry (delete it and add it again to change them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct EntryPatch {
    /// New issuer.
    pub issuer: Option<String>,
    /// New account name.
    pub account: Option<String>,
    /// New group; `Some("")` removes it.
    pub group: Option<String>,
    /// Pin or unpin.
    pub favorite: Option<bool>,
    /// New colour; `auto` goes back to the name's.
    pub color: Option<AccountColor>,
    /// New mark; `Some("")` goes back to the name's initial.
    pub mark: Option<String>,
}

/// The colours of Settings' palette an account can be shown in (DESIGN.md, "Account colours").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountColor {
    /// One of the colours below, picked from the account's name: each service its own.
    #[default]
    Auto,
    /// Red.
    Red,
    /// Orange.
    Orange,
    /// Amber.
    Amber,
    /// Green.
    Green,
    /// Teal.
    Teal,
    /// Blue.
    Blue,
    /// Indigo.
    Indigo,
    /// Purple.
    Purple,
    /// Pink.
    Pink,
    /// Grey.
    Gray,
}

impl AccountColor {
    fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

/// A colour this version does not know (written by a newer one) reads as `auto`: neither the vault
/// nor another device's snapshot fails to open over a colour.
fn lenient_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AccountColor, D::Error> {
    let name = String::deserialize(deserializer)?;
    Ok(serde_json::from_value(serde_json::Value::String(name)).unwrap_or_default())
}

/// The most characters, as people count them, an account's mark holds.
pub const MARK_CHARS: usize = 2;

/// A mark as an account keeps it: cleaned like a name, at most [`MARK_CHARS`] characters as people
/// count them (an emoji with its joiners is one); `None` when nothing is left.
pub(crate) fn clean_mark(mark: &str) -> Option<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let kept: String = clean_name(mark).graphemes(true).take(MARK_CHARS).collect();
    Some(kept.trim().to_owned()).filter(|m| !m.is_empty())
}

/// The newest payload format this Lockra reads and writes: 2 added the stamps, the tombstones and
/// the local part; 3 the logins, the site rules and the generator's history. A payload is written
/// as 3 only when it holds any of those ([`VaultData::written_format`]), so that a vault that never
/// held a login still opens in a version before them.
pub const VAULT_DATA_FORMAT: u32 = 3;
/// The format of a payload without logins, site rules or generated passwords.
const FORMAT_WITHOUT_LOGINS: u32 = 2;

/// The decrypted payload of the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct VaultData {
    /// Payload format as read ([`VAULT_DATA_FORMAT`] once opened; 1 before sync). A payload is
    /// written in the format its content needs, whatever this says.
    #[serde(default = "format_one")]
    pub format: u32,
    /// The accounts, in insertion order.
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// The accounts deleted, so that a deletion on one device removes the account on the others.
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
    /// The logins.
    #[serde(default)]
    pub items: Vec<VaultItem>,
    /// The logins deleted.
    #[serde(default)]
    pub item_tombstones: Vec<Tombstone>,
    /// The site rules (never save, never fill, equivalent domains).
    #[serde(default)]
    pub site_rules: Vec<SiteRule>,
    /// The site rules removed.
    #[serde(default)]
    pub site_rule_tombstones: Vec<Tombstone>,
    /// This device's own part: its clock and its sync space. In the vault file only: never in a
    /// backup, never synced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<Local>,
}

/// What only this device keeps. Every part but the clock and the sync space reads leniently: one
/// that does not read (a later version's shape) is reset alone, and the vault opens (R16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Local {
    /// Stamps this device's changes; its device number names this device in its sync space too.
    pub(crate) clock: Clock,
    /// The sync space this device belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sync: Option<SyncLocal>,
    /// How this device shows the code list.
    #[serde(default, skip_serializing_if = "View::is_empty", deserialize_with = "lenient::or_default")]
    pub(crate) view: View,
    /// The passwords the generator made here: secrets, kept only in the vault file.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient::or_default")]
    pub(crate) generator_history: Vec<GeneratedPassword>,
    /// How this device shows the password list.
    #[serde(default, skip_serializing_if = "ItemListView::is_empty", deserialize_with = "lenient::or_default")]
    pub(crate) item_view: ItemListView,
    /// The browser extension: the app's static key, the paired extensions, the policy. One that
    /// does not read is reset alone: the extensions pair again.
    #[serde(default, skip_serializing_if = "BrowserLocal::is_empty", deserialize_with = "lenient::or_default")]
    pub(crate) browser: BrowserLocal,
}

impl Local {
    /// A new device: a random number, nothing stamped yet, no sync.
    pub(crate) fn new() -> Self {
        Self {
            clock: Clock::new(random_device()),
            sync: None,
            view: View::default(),
            generator_history: Vec::new(),
            item_view: ItemListView::default(),
            browser: BrowserLocal::default(),
        }
    }
}

/// How this device shows the code list. In the vault, not in `settings.json`: group names are the
/// vault's, and the settings file is not encrypted. Never synced: each device keeps its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct View {
    /// The groups folded in the code list; "" folds the accounts in no group.
    #[serde(default)]
    pub collapsed_groups: Vec<String>,
    /// The accounts in the order they were dragged into (`SortOrder::Manual`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_order: Vec<Uuid>,
    /// The groups in the order they were dragged into ("" for the accounts in no group); groups
    /// never dragged come after, by name, and the accounts in no group last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_order: Vec<String>,
    /// The group shown first: its section heads the code list and the list opens on it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "pinned_or_none")]
    pub pinned_group: Option<PinnedGroup>,
}

/// A pin this version cannot read (a later Lockra's kind) is no pin, not a reason to lose the folds
/// and the order beside it.
fn pinned_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<PinnedGroup>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

impl View {
    fn is_empty(&self) -> bool {
        self.collapsed_groups.is_empty() && self.entry_order.is_empty() && self.group_order.is_empty() && self.pinned_group.is_none()
    }
}

/// The group this device pins at the top of the code list: the pinned accounts (收藏), one group,
/// or the accounts in no group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PinnedGroup {
    /// The accounts pinned to the top (favourites), gathered in a section of their own.
    Favorites,
    /// A group, by its name.
    Group {
        /// The group's name.
        name: String,
    },
    /// The accounts in no group.
    Ungrouped,
}

/// A random device number (nonzero: zero marks stamps written before devices had numbers).
pub(crate) fn random_device() -> u64 {
    loop {
        let (high, _) = Uuid::new_v4().as_u64_pair();
        if high != 0 {
            return high;
        }
    }
}

fn format_one() -> u32 {
    1
}

/// The vault file's payload as written: [`VaultData`] with the format its content needs.
#[derive(Serialize)]
struct Written<'a> {
    format: u32,
    entries: &'a [Entry],
    tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    items: &'a [VaultItem],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    item_tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rules: &'a [SiteRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rule_tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "Option::is_none")]
    local: &'a Option<Local>,
}

/// A backup's payload: the accounts, the logins, the site rules and their deletions, without this
/// device's part.
#[derive(Serialize)]
struct BackupPayload<'a> {
    format: u32,
    entries: &'a [Entry],
    tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    items: &'a [VaultItem],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    item_tombstones: &'a [Tombstone],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rules: &'a [SiteRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    site_rule_tombstones: &'a [Tombstone],
}

/// A payload read without its sync space: [`VaultData`] field for field, the local part without
/// `sync` (serde skips it unread), its other parts read leniently.
#[derive(Deserialize)]
struct WithoutSync {
    #[serde(default = "format_one")]
    format: u32,
    #[serde(default)]
    entries: Vec<Entry>,
    #[serde(default)]
    tombstones: Vec<Tombstone>,
    #[serde(default)]
    items: Vec<VaultItem>,
    #[serde(default)]
    item_tombstones: Vec<Tombstone>,
    #[serde(default)]
    site_rules: Vec<SiteRule>,
    #[serde(default)]
    site_rule_tombstones: Vec<Tombstone>,
    #[serde(default)]
    local: Option<LocalWithoutSync>,
}

#[derive(Deserialize)]
struct LocalWithoutSync {
    clock: Clock,
    #[serde(default, deserialize_with = "lenient::or_default")]
    view: View,
    #[serde(default, deserialize_with = "lenient::or_default")]
    generator_history: Vec<GeneratedPassword>,
    #[serde(default, deserialize_with = "lenient::or_default")]
    item_view: ItemListView,
    #[serde(default, deserialize_with = "lenient::or_default")]
    browser: BrowserLocal,
}

impl VaultData {
    /// A new vault's payload.
    pub fn new() -> Self {
        Self { format: FORMAT_WITHOUT_LOGINS, local: Some(Local::new()), ..Self::default() }
    }

    /// The format this payload is written in: 3 once it holds a login, a site rule, one of their
    /// deletions or a generated password, 2 otherwise.
    pub fn written_format(&self) -> u32 {
        let generated = self.local.as_ref().is_some_and(|l| !l.generator_history.is_empty());
        if self.holds_logins() || generated { VAULT_DATA_FORMAT } else { FORMAT_WITHOUT_LOGINS }
    }

    /// It holds logins or deletions of logins: what a merge without the logins' preview would
    /// lose.
    pub(crate) fn holds_items(&self) -> bool {
        !(self.items.is_empty() && self.item_tombstones.is_empty())
    }

    /// It holds logins, site rules or their deletions: what a backup of format 3 carries.
    pub(crate) fn holds_logins(&self) -> bool {
        !(self.items.is_empty() && self.item_tombstones.is_empty() && self.site_rules.is_empty() && self.site_rule_tombstones.is_empty())
    }

    /// The vault file's payload bytes for `lockra-vault`: everything, the local part included.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let written = Written {
            format: self.written_format(),
            entries: &self.entries,
            tombstones: &self.tombstones,
            items: &self.items,
            item_tombstones: &self.item_tombstones,
            site_rules: &self.site_rules,
            site_rule_tombstones: &self.site_rule_tombstones,
            local: &self.local,
        };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(&written).expect("vault data serializes"))
    }

    /// A backup's payload bytes: the accounts, the logins, the site rules and the deletions,
    /// without this device's part (its sync space, the storage's credentials and the generated
    /// passwords stay in the vault file).
    pub fn backup_bytes(&self) -> Zeroizing<Vec<u8>> {
        let backup = BackupPayload {
            format: if self.holds_logins() { VAULT_DATA_FORMAT } else { FORMAT_WITHOUT_LOGINS },
            entries: &self.entries,
            tombstones: &self.tombstones,
            items: &self.items,
            item_tombstones: &self.item_tombstones,
            site_rules: &self.site_rules,
            site_rule_tombstones: &self.site_rule_tombstones,
        };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(&backup).expect("vault data serializes"))
    }

    /// Parse a payload; `None` when it is not a Lockra payload. A sync space this version cannot
    /// read (kept by an earlier build) is left out, not the vault: sync is off on this device until
    /// it is set up again. The logins, the site rules and the rest of the local part stay.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok().or_else(|| {
            let data: WithoutSync = serde_json::from_slice(bytes).ok()?;
            let local = data.local.map(|local| Local {
                clock: local.clock,
                sync: None,
                view: local.view,
                generator_history: local.generator_history,
                item_view: local.item_view,
                browser: local.browser,
            });
            Some(Self {
                format: data.format,
                entries: data.entries,
                tombstones: data.tombstones,
                items: data.items,
                item_tombstones: data.item_tombstones,
                site_rules: data.site_rules,
                site_rule_tombstones: data.site_rule_tombstones,
                local,
            })
        })
    }

    /// A vault's or a backup's payload, ready for use: a newer format is refused, entries written
    /// before stamps get one (their last change, from this device), and a payload without a local
    /// part (a backup, an older vault) gets a new one.
    pub(crate) fn open(bytes: &[u8]) -> CoreResult<Self> {
        Self::open_versioned(bytes).map(|(data, _)| data)
    }

    /// [`Self::open`], with the format the payload was written in (a restore tells a backup of
    /// format 2, which holds no logins, from one of format 3, which does).
    pub(crate) fn open_versioned(bytes: &[u8]) -> CoreResult<(Self, u32)> {
        let mut data = Self::from_bytes(bytes).ok_or(ErrorCode::VaultCorrupted)?;
        if data.format > VAULT_DATA_FORMAT {
            return Err(ErrorCode::VaultUnsupported.into());
        }
        let source_format = data.format;
        data.format = VAULT_DATA_FORMAT;
        let device = data.local_mut().clock.device();
        for entry in &mut data.entries {
            if entry.stamp == Hlc::default() {
                entry.stamp = Hlc { wall_ms: entry.updated_at_ms, counter: 0, device };
            }
        }
        // A clock behind the payload's stamps (a new local part, another device's backup) would
        // stamp the next change before the version it replaces.
        data.observe_stamps();
        Ok((data, source_format))
    }

    pub(crate) fn local_mut(&mut self) -> &mut Local {
        self.local.get_or_insert_with(Local::new)
    }

    /// This device's number.
    pub(crate) fn device(&self) -> u64 {
        self.local.as_ref().map_or(0, |l| l.clock.device())
    }

    /// The groups folded in this device's code list.
    pub(crate) fn collapsed_groups(&self) -> &[String] {
        self.local.as_ref().map_or(&[], |l| l.view.collapsed_groups.as_slice())
    }

    /// The accounts in the order they were dragged into on this device.
    pub(crate) fn entry_order(&self) -> &[Uuid] {
        self.local.as_ref().map_or(&[], |l| l.view.entry_order.as_slice())
    }

    /// The groups in the order they were dragged into on this device.
    pub(crate) fn group_order(&self) -> &[String] {
        self.local.as_ref().map_or(&[], |l| l.view.group_order.as_slice())
    }

    /// The group this device pins at the top of the code list.
    pub(crate) fn pinned_group(&self) -> Option<&PinnedGroup> {
        self.local.as_ref().and_then(|l| l.view.pinned_group.as_ref())
    }

    /// The sync space this device belongs to.
    pub(crate) fn sync(&self) -> Option<&SyncLocal> {
        self.local.as_ref().and_then(|l| l.sync.as_ref())
    }

    pub(crate) fn sync_mut(&mut self) -> Option<&mut SyncLocal> {
        self.local.as_mut().and_then(|l| l.sync.as_mut())
    }

    /// A stamp for a change made at `now_ms`.
    pub(crate) fn tick(&mut self, now_ms: u64) -> Hlc {
        self.local_mut().clock.tick(now_ms)
    }

    /// The latest stamp the payload holds: the accounts', the logins' (each of their parts), the
    /// site rules' and every deletion's.
    fn latest_stamp(&self) -> Option<Hlc> {
        let entries = self.entries.iter().map(|e| e.stamp);
        let items = self.items.iter().map(VaultItem::latest_stamp);
        let rules = self.site_rules.iter().map(|r| r.stamp);
        let deletions = [&self.tombstones, &self.item_tombstones, &self.site_rule_tombstones].into_iter().flatten().map(|t| t.stamp);
        entries.chain(items).chain(rules).chain(deletions).max()
    }

    /// Note the latest stamp of what a sync brought in: changes made here later come after it.
    pub(crate) fn observe_stamps(&mut self) {
        if let Some(latest) = self.latest_stamp() {
            self.local_mut().clock.observe(latest);
        }
    }

    /// Delete entry `id`, leaving a tombstone (with its HOTP counter): the entry and where it was.
    pub(crate) fn remove(&mut self, id: Uuid, now_ms: u64) -> Option<(usize, Entry)> {
        let index = self.entries.iter().position(|e| e.id == id)?;
        let stamp = self.tick(now_ms);
        let removed = self.entries.remove(index);
        self.bury(Tombstone { id, stamp, counter: removed.counter() });
        Some((index, removed))
    }

    /// Keep `tombstone`: the later deletion and the higher counter of the two.
    fn bury(&mut self, tombstone: Tombstone) {
        bury(&mut self.tombstones, tombstone);
    }

    /// Replace the accounts by a backup's `entries` as one change made at `now_ms` (a restore
    /// that replaces): every account gets a new stamp, the ones that go get tombstones, and the
    /// backup's own deletions are kept, so that the other devices of a sync space end up with the
    /// backup's accounts. An HOTP counter does not go back below what this vault had.
    pub(crate) fn replace_entries(&mut self, entries: Vec<Entry>, tombstones: &[Tombstone], now_ms: u64) {
        // The new stamps come after everything the backup holds, its deletions included (a backup
        // from a device whose clock ran ahead), so no kept tombstone removes a restored account.
        if let Some(latest) = entries.iter().map(|e| e.stamp).chain(tombstones.iter().map(|t| t.stamp)).max() {
            self.local_mut().clock.observe(latest);
        }
        for tombstone in tombstones {
            self.bury(*tombstone);
        }
        let previous = std::mem::replace(&mut self.entries, entries);
        let gone: Vec<&Entry> = previous.iter().filter(|e| !self.entries.iter().any(|n| n.id == e.id)).collect();
        for entry in gone {
            let stamp = self.tick(now_ms);
            self.bury(Tombstone { id: entry.id, stamp, counter: entry.counter() });
        }
        for index in 0..self.entries.len() {
            let stamp = self.tick(now_ms);
            let id = self.entries[index].id;
            let floor = previous.iter().find(|p| p.id == id).and_then(Record::counter).max(self.tombstones.iter().find(|t| t.id == id).and_then(|t| t.counter));
            let entry = &mut self.entries[index];
            entry.stamp = stamp;
            if let Some(floor) = floor {
                entry.raise_counter(floor);
            }
        }
    }

    /// Replace the logins by a backup's `items` as one change made at `now_ms`, as
    /// [`Self::replace_entries`] does the accounts: the backup's deletions are kept, the logins
    /// that go get tombstones, and each restored login takes its backup version back with new
    /// stamps (its write ids kept), later than anything the space holds, so that no other device's
    /// older snapshot changes it back and none raises a conflict over it.
    pub(crate) fn replace_items(&mut self, items: &[VaultItem], tombstones: &[Tombstone], now_ms: u64) {
        let backup_latest = items.iter().map(VaultItem::latest_stamp).chain(tombstones.iter().map(|t| t.stamp)).max();
        if let Some(latest) = backup_latest {
            self.local_mut().clock.observe(latest);
        }
        for tombstone in tombstones {
            bury(&mut self.item_tombstones, *tombstone);
        }
        let mut clock = self.local_mut().clock;
        let gone: Vec<Uuid> = self.items.iter().map(|i| i.id).filter(|id| !items.iter().any(|r| r.id == *id)).collect();
        for id in gone {
            bury(&mut self.item_tombstones, Tombstone { id, stamp: clock.tick(now_ms), counter: None });
        }
        let previous = std::mem::take(&mut self.items);
        self.items = items
            .iter()
            .map(|restored| {
                let mut item = previous.iter().find(|p| p.id == restored.id).cloned().unwrap_or_else(|| restored.clone());
                item.take_back(restored, now_ms, || clock.tick(now_ms));
                item.last_used_at_ms = previous.iter().find(|p| p.id == restored.id).and_then(|p| p.last_used_at_ms);
                item
            })
            .collect();
        self.local_mut().clock = clock;
    }

    /// Replace the site rules by a backup's, as one change made at `now_ms`: the backup's deletions
    /// are kept, the rules that go get tombstones, and every restored rule a new stamp.
    pub(crate) fn replace_site_rules(&mut self, rules: &[SiteRule], tombstones: &[Tombstone], now_ms: u64) {
        if let Some(latest) = rules.iter().map(|r| r.stamp).chain(tombstones.iter().map(|t| t.stamp)).max() {
            self.local_mut().clock.observe(latest);
        }
        for tombstone in tombstones {
            bury(&mut self.site_rule_tombstones, *tombstone);
        }
        let gone: Vec<Uuid> = self.site_rules.iter().map(|r| r.id).filter(|id| !rules.iter().any(|r| r.id == *id)).collect();
        for id in gone {
            let stamp = self.tick(now_ms);
            bury(&mut self.site_rule_tombstones, Tombstone { id, stamp, counter: None });
        }
        self.site_rules = rules.to_vec();
        for index in 0..self.site_rules.len() {
            let stamp = self.tick(now_ms);
            self.site_rules[index].stamp = stamp;
        }
    }

    /// The entry with `id`.
    pub fn get(&self, id: Uuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry with `id`, mutably.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }
}

/// Keep `tombstone` among `tombstones`: the later deletion and the higher counter of the two.
fn bury(tombstones: &mut Vec<Tombstone>, tombstone: Tombstone) {
    match tombstones.iter_mut().find(|t| t.id == tombstone.id) {
        Some(own) => {
            own.stamp = own.stamp.max(tombstone.stamp);
            own.counter = own.counter.max(tombstone.counter);
        }
        None => tombstones.push(tombstone),
    }
}

/// Trimmed, control characters removed, at most [`MAX_NAME_CHARS`].
pub fn clean_name(name: &str) -> String {
    name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect::<String>().trim().to_owned()
}

fn secret_to_base32<S: Serializer>(secret: &Zeroizing<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&base32::encode(secret))
}

fn secret_from_base32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Zeroizing<Vec<u8>>, D::Error> {
    let text = Zeroizing::new(String::deserialize(deserializer)?);
    base32::decode(&text).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use lockra_otp::{Period, uri};

    use super::*;

    fn auth(text: &str) -> OtpAuth {
        uri::parse(text).unwrap()
    }

    #[test]
    fn payload_round_trips_with_the_secret_in_base32() {
        let entry = Entry::from_auth(auth("otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 5);
        let data = VaultData { entries: vec![entry.clone()], ..VaultData::new() };
        let bytes = data.to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("\"secret\":\"JBSWY3DPEHPK3PXP\""), "{text}");
        assert_eq!(VaultData::from_bytes(&bytes).unwrap(), data);
        assert!(VaultData::from_bytes(b"[1,2]").is_none());
        assert_eq!(VaultData::from_bytes(b"{}").unwrap(), VaultData { format: 1, ..VaultData::default() });
    }

    #[test]
    fn a_format_one_payload_opens_with_stamps_and_a_local_part_and_newer_formats_are_refused() {
        let legacy = br#"{"format":1,"entries":[{"id":"0f3f1a1e-8d4b-4c8e-9f7a-000000000001","issuer":"GitHub","account":"octocat","kind":{"type":"totp","period":30},"algorithm":"sha1","digits":6,"secret":"JBSWY3DPEHPK3PXP","group":null,"favorite":false,"origin":"uri","created_at_ms":5,"updated_at_ms":7,"last_used_at_ms":null}]}"#;
        let data = VaultData::open(legacy).unwrap();
        assert_eq!(data.format, VAULT_DATA_FORMAT);
        let device = data.device();
        assert_ne!(device, 0);
        assert_eq!(data.entries[0].stamp, Hlc { wall_ms: 7, counter: 0, device }, "the last change, from this device");
        assert!(data.tombstones.is_empty() && data.sync().is_none());
        let newer = format!(r#"{{"format":{}}}"#, VAULT_DATA_FORMAT + 1);
        assert_eq!(VaultData::open(newer.as_bytes()).unwrap_err().code, ErrorCode::VaultUnsupported);
        assert_eq!(VaultData::open(b"not json").unwrap_err().code, ErrorCode::VaultCorrupted);
        // Opening again keeps the device and the stamps.
        let again = VaultData::open(&data.to_bytes()).unwrap();
        assert_eq!((again.device(), again.entries[0].stamp), (device, data.entries[0].stamp));
    }

    #[test]
    fn a_sync_space_this_version_cannot_read_is_left_out_not_the_vault() {
        let mut data = VaultData::new();
        data.entries.push(Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1));
        let storage = lockra_sync::StorageConfig::Webdav {
            url: "https://dav.example.com/".into(),
            prefix: String::new(),
            username: "me".into(),
            password: Zeroizing::new("x".into()),
        };
        let sync_key = lockra_sync::SyncKey::generate().unwrap();
        let keys = lockra_sync::SpaceKeys::generate(sync_key.space_id()).unwrap();
        data.local_mut().sync = Some(SyncLocal::new(storage, &keys, &sync_key, "Laptop".into(), b"keyring"));
        let whole = VaultData::open(&data.to_bytes()).unwrap();
        assert!(whole.sync().is_some());
        // Kept by an earlier build, its space without this device's keyring: the vault opens, its
        // accounts and its device number with it, and sync is off here until it is set up again.
        let mut kept: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        kept["local"]["sync"].as_object_mut().unwrap().remove("keyring");
        let opened = VaultData::open(&serde_json::to_vec(&kept).unwrap()).unwrap();
        assert!(opened.sync().is_none());
        assert_eq!((&opened.entries, opened.device()), (&whole.entries, whole.device()));
        // The same for an empty keyring, as a build that defaulted it saved it.
        kept["local"]["sync"]["keyring"] = serde_json::json!("");
        assert!(VaultData::open(&serde_json::to_vec(&kept).unwrap()).unwrap().sync().is_none());
        // And for a storage of a kind this version does not know, as Lockra up to 0.7.3 sees a
        // cloud drive's folder.
        let mut later: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        later["local"]["sync"]["storage"] = serde_json::json!({ "kind": "a later kind", "path": "/x" });
        let opened = VaultData::open(&serde_json::to_vec(&later).unwrap()).unwrap();
        assert!(opened.sync().is_none());
        assert_eq!(opened.entries, whole.entries);
        // A folder this version knows stays, with its path.
        let folder = lockra_sync::StorageConfig::Folder { path: std::env::temp_dir().join("Dropbox") };
        data.local_mut().sync = Some(SyncLocal::new(folder.clone(), &keys, &sync_key, "Laptop".into(), b"keyring"));
        assert_eq!(VaultData::open(&data.to_bytes()).unwrap().sync().map(|s| s.storage.clone()), Some(folder));
        // Anything else damaged is still a damaged vault.
        kept["entries"] = serde_json::json!("not a list");
        assert_eq!(VaultData::open(&serde_json::to_vec(&kept).unwrap()).unwrap_err().code, ErrorCode::VaultCorrupted);
    }

    #[test]
    fn the_pinned_group_is_kept_and_one_of_a_later_kind_reads_as_none() {
        let mut data = VaultData::new();
        data.local_mut().view.pinned_group = Some(PinnedGroup::Group { name: "Work".into() });
        let value: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        assert_eq!(value["local"]["view"]["pinned_group"], serde_json::json!({ "type": "group", "name": "Work" }));
        assert_eq!(VaultData::open(&data.to_bytes()).unwrap().pinned_group(), Some(&PinnedGroup::Group { name: "Work".into() }));
        // A later Lockra's kind of pin is no pin here: the vault opens, the rest of the view kept.
        let mut later = value.clone();
        later["local"]["view"]["pinned_group"] = serde_json::json!({ "type": "a later kind" });
        later["local"]["view"]["group_order"] = serde_json::json!(["Work", ""]);
        let opened = VaultData::open(&serde_json::to_vec(&later).unwrap()).unwrap();
        assert_eq!(opened.pinned_group(), None);
        assert_eq!(opened.group_order(), ["Work", ""]);
        // A vault from before pins has none.
        let mut older = value;
        older["local"]["view"].as_object_mut().unwrap().remove("pinned_group");
        assert_eq!(VaultData::open(&serde_json::to_vec(&older).unwrap()).unwrap().pinned_group(), None);
    }

    #[test]
    fn a_backup_carries_the_accounts_and_deletions_but_not_the_local_part() {
        let mut data = VaultData::new();
        data.entries.push(Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1));
        let gone = Entry::from_auth(auth("otpauth://totp/C:d?secret=GEZDGNBV"), Origin::Uri, 1);
        let gone_id = gone.id;
        data.entries.push(gone);
        assert!(data.remove(gone_id, 10).is_some());
        assert_eq!(data.tombstones.len(), 1);
        let backup = data.backup_bytes();
        let value: serde_json::Value = serde_json::from_slice(&backup).unwrap();
        assert!(value.get("local").is_none(), "{value}");
        let restored = VaultData::open(&backup).unwrap();
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.tombstones, data.tombstones);
        assert_ne!(restored.device(), data.device(), "a restored backup is a new device");
        assert!(serde_json::from_slice::<serde_json::Value>(&data.to_bytes()).unwrap().get("local").is_some());
    }

    #[test]
    fn changes_are_stamped_in_order_and_a_replace_buries_what_goes() {
        let mut data = VaultData::new();
        let a = data.tick(100);
        let b = data.tick(100);
        assert!(b > a && b.device == data.device());
        let mut keep = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        keep.stamp = a;
        let drop = Entry::from_auth(auth("otpauth://totp/C:d?secret=GEZDGNBV"), Origin::Uri, 1);
        let drop_id = drop.id;
        data.entries = vec![keep.clone(), drop];
        data.replace_entries(vec![keep.clone()], &[], 50);
        assert_eq!(data.entries.len(), 1);
        assert!(data.entries[0].stamp > b, "the restore is a change made now");
        assert_eq!(data.tombstones.iter().map(|t| t.id).collect::<Vec<_>>(), [drop_id]);
        assert!(data.remove(Uuid::new_v4(), 60).is_none());
        // A stamp seen from another device moves the clock past it.
        data.entries[0].stamp = Hlc { wall_ms: 9_999_999, counter: 3, device: 1 };
        data.observe_stamps();
        assert!(data.tick(200) > data.entries[0].stamp);
    }

    #[test]
    fn the_view_has_no_secret_and_names_export_limits() {
        let entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP&period=60&digits=8"), Origin::Manual, 1);
        let view = serde_json::to_value(entry.view()).unwrap();
        assert!(view.get("secret").is_none());
        assert_eq!(view["kind"], serde_json::json!({"type": "totp", "period": 60}));
        assert_eq!(view["export"]["google"], "period_not_30");
        assert_eq!(view["export"]["microsoft"], "period_not_30");
        let plain = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Manual, 1).view();
        assert_eq!((plain.export.google, plain.export.microsoft), (None, None));
    }

    #[test]
    fn sameness_is_about_secrets_and_parameters_not_names() {
        let entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        assert!(entry.same_account(&auth("otpauth://totp/Other:name?secret=JBSWY3DPEHPK3PXP")));
        assert!(!entry.same_account(&auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP&digits=8")));
        assert!(!entry.same_account(&auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP")));
        assert!(entry.same_names(&auth("otpauth://totp/a:B?secret=GEZDGNBV")));
        assert!(!entry.same_names(&auth("otpauth://totp/a:c?secret=GEZDGNBV")));
        let hotp = Entry::from_auth(auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP&counter=5"), Origin::Uri, 1);
        assert!(hotp.same_account(&auth("otpauth://hotp/A:b?secret=JBSWY3DPEHPK3PXP&counter=9")), "a counter is state, not identity");
        let _ = Period::THIRTY;
    }

    #[test]
    fn an_hotp_counter_survives_a_deletion_and_a_restore_but_never_goes_back() {
        let mut data = VaultData::new();
        let mut hotp = Entry::from_auth(auth("otpauth://hotp/Bank:card?secret=JBSWY3DPEHPK3PXP&counter=9"), Origin::Uri, 1);
        hotp.stamp = data.tick(1);
        let id = hotp.id;
        data.entries.push(hotp.clone());
        data.remove(id, 2).unwrap();
        assert_eq!(data.tombstones[0].counter, Some(9), "the tombstone keeps the counter");
        // A backup from before the deletion, at counter 4, with a deletion of its own.
        let backup = Entry { kind: OtpKind::Hotp { counter: 4 }, ..hotp };
        let elsewhere = Tombstone { id: Uuid::new_v4(), stamp: Hlc::at(3), counter: None };
        data.replace_entries(vec![backup], &[elsewhere], 10);
        assert_eq!(data.entries[0].kind, OtpKind::Hotp { counter: 9 }, "restored at the counter this vault reached");
        assert!(data.entries[0].stamp > data.tombstones.iter().find(|t| t.id == id).unwrap().stamp);
        assert!(data.tombstones.contains(&elsewhere), "the backup's deletions are kept");
        // A backup from a clock far ahead: an account deleted there and brought back later stays.
        let mut revived = Entry::from_auth(auth("otpauth://totp/Mail:me?secret=GEZDGNBV"), Origin::Uri, 1);
        revived.stamp = Hlc { wall_ms: 9_000_000_000_000, counter: 5, device: 3 };
        let old_deletion = Tombstone { id: revived.id, stamp: Hlc { wall_ms: 9_000_000_000_000, counter: 4, device: 3 }, counter: None };
        data.replace_entries(vec![revived.clone()], &[old_deletion], 20);
        let tombstone = data.tombstones.iter().find(|t| t.id == revived.id).unwrap();
        assert!(data.entries[0].stamp > tombstone.stamp, "{:?} is not after {:?}", data.entries[0].stamp, tombstone.stamp);
        // TOTP accounts have no counter.
        let totp = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        assert_eq!(totp.counter(), None);
        let mut raised = totp.clone();
        raised.raise_counter(5);
        assert_eq!(raised, totp);
    }

    #[test]
    fn opening_a_payload_moves_the_clock_past_its_stamps() {
        let mut ahead = VaultData::new();
        let mut entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        entry.stamp = Hlc { wall_ms: 9_000_000_000_000, counter: 2, device: 77 };
        ahead.entries.push(entry.clone());
        // A backup: no local part, so a new clock that has seen nothing.
        let mut restored = VaultData::open(&ahead.backup_bytes()).unwrap();
        assert!(restored.tick(1_000) > entry.stamp, "a change made now comes after what the backup holds");
    }

    use crate::item::{Field, Purpose, SiteRuleKind};

    /// A login with a user name and a hidden password, stamped by `data`'s clock at `now_ms`.
    fn login(data: &mut VaultData, title: &str, password: &str, now_ms: u64) -> VaultItem {
        let (stamp, mwid) = (data.tick(now_ms), crate::item::random_write_id());
        let mut item = VaultItem::login(Uuid::new_v4(), now_ms, stamp, mwid);
        item.title = title.into();
        let (stamp, wid) = (data.tick(now_ms), crate::item::random_write_id());
        item.write_field(Uuid::from_u128(1), Purpose::Username, stamp, wid, now_ms, |f| f.value = Zeroizing::new("octocat".into()));
        let (stamp, wid) = (data.tick(now_ms), crate::item::random_write_id());
        item.write_field(Uuid::from_u128(2), Purpose::Password, stamp, wid, now_ms, |f| {
            f.value = Zeroizing::new(password.into());
            f.hidden = true;
        });
        item
    }

    fn site_rule(data: &mut VaultData, kind: SiteRuleKind, domain: &str, now_ms: u64) -> SiteRule {
        SiteRule {
            id: SiteRule::single_id(kind, domain),
            kind: kind.into(),
            domains: vec![domain.into()],
            stamp: data.tick(now_ms),
            extra: serde_json::Map::new(),
        }
    }

    fn format_of(bytes: &[u8]) -> u64 {
        serde_json::from_slice::<serde_json::Value>(bytes).unwrap()["format"].as_u64().unwrap()
    }

    fn password(item: &VaultItem) -> &str {
        item.field_of(Purpose::Password).map_or("", |f| f.value.as_str())
    }

    #[test]
    fn a_vault_is_written_as_format_3_only_once_it_holds_logins() {
        let mut data = VaultData::new();
        data.entries.push(Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1));
        let bytes = data.to_bytes();
        assert_eq!(format_of(&bytes), 2, "no logins: an earlier version still opens it");
        let text = std::str::from_utf8(&bytes).unwrap();
        for absent in ["items", "item_tombstones", "site_rules", "site_rule_tombstones", "generator_history"] {
            assert!(!text.contains(absent), "{absent} in {text}");
        }
        // Opened, it says 3 (what this version reads), and still writes 2.
        let opened = VaultData::open(&bytes).unwrap();
        assert_eq!((opened.format, format_of(&opened.to_bytes())), (VAULT_DATA_FORMAT, 2));
        // Each of the new parts alone makes it 3.
        let mut with_item = opened.clone();
        let item = login(&mut with_item, "GitHub", "hunter2", 10);
        with_item.items.push(item);
        assert_eq!(format_of(&with_item.to_bytes()), 3);
        let mut with_deletion = opened.clone();
        with_deletion.item_tombstones.push(Tombstone { id: Uuid::new_v4(), stamp: Hlc::at(1), counter: None });
        assert_eq!(format_of(&with_deletion.to_bytes()), 3);
        let mut with_rule = opened.clone();
        let rule = site_rule(&mut with_rule, SiteRuleKind::NeverSave, "example.com", 10);
        with_rule.site_rules.push(rule);
        assert_eq!(format_of(&with_rule.to_bytes()), 3);
        let mut with_rule_deletion = opened.clone();
        with_rule_deletion.site_rule_tombstones.push(Tombstone { id: Uuid::new_v4(), stamp: Hlc::at(1), counter: None });
        assert_eq!(format_of(&with_rule_deletion.to_bytes()), 3);
        let mut with_generated = opened.clone();
        with_generated.local_mut().generator_history.push(GeneratedPassword { password: Zeroizing::new("x".into()), at_ms: 1, site: None });
        assert_eq!(format_of(&with_generated.to_bytes()), 3);
        // A round trip keeps everything.
        let again = VaultData::open(&with_item.to_bytes()).unwrap();
        assert_eq!((again.items.clone(), again.device()), (with_item.items.clone(), with_item.device()));
    }

    #[test]
    fn a_vault_of_0_9_opens_and_is_written_back_as_it_was() {
        // As 0.9.1 writes a vault with a sync space and a view: format 2, no new part.
        let mut data = VaultData::new();
        let mut entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Google, 1);
        entry.stamp = data.tick(1);
        data.entries.push(entry);
        data.local_mut().view.collapsed_groups = vec!["Work".into()];
        let storage = lockra_sync::StorageConfig::relay("https://relay.example.com");
        let sync_key = lockra_sync::SyncKey::generate().unwrap();
        let keys = lockra_sync::SpaceKeys::generate(sync_key.space_id()).unwrap();
        data.local_mut().sync = Some(SyncLocal::new(storage, &keys, &sync_key, "Laptop".into(), b"keyring"));
        let written_by_0_9 = data.to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&written_by_0_9).unwrap();
        let keys_0_9: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys_0_9, ["entries", "format", "local", "tombstones"], "what 0.9 writes, nothing more");
        let local_keys: Vec<&str> = value["local"].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(local_keys, ["clock", "sync", "view"]);
        let opened = VaultData::open(&written_by_0_9).unwrap();
        assert!(opened.sync().is_some() && opened.items.is_empty());
        assert_eq!(opened.collapsed_groups(), ["Work"]);
        assert_eq!(*opened.to_bytes(), *written_by_0_9, "written back byte for byte");
    }

    #[test]
    fn logins_rules_and_generated_passwords_survive_a_sync_space_this_version_cannot_read() {
        let mut data = VaultData::new();
        let item = login(&mut data, "GitHub", "hunter2", 10);
        data.items.push(item);
        let rule = site_rule(&mut data, SiteRuleKind::NeverFill, "bank.example", 11);
        data.site_rules.push(rule);
        data.item_tombstones.push(Tombstone { id: Uuid::new_v4(), stamp: Hlc::at(5), counter: None });
        data.local_mut().generator_history.push(GeneratedPassword {
            password: Zeroizing::new("generated".into()),
            at_ms: 12,
            site: Some("example.com".into()),
        });
        data.local_mut().view.group_order = vec!["Work".into()];
        data.local_mut().browser.key = Some(crate::browser::StaticKey(Zeroizing::new([3; 32])));
        let storage = lockra_sync::StorageConfig::Folder { path: std::env::temp_dir().join("Drive") };
        let sync_key = lockra_sync::SyncKey::generate().unwrap();
        let keys = lockra_sync::SpaceKeys::generate(sync_key.space_id()).unwrap();
        data.local_mut().sync = Some(SyncLocal::new(storage, &keys, &sync_key, "Laptop".into(), b"keyring"));
        let mut broken: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        assert_eq!(broken["format"], 3);
        broken["local"]["sync"]["keyring"] = serde_json::json!("");
        let opened = VaultData::open(&serde_json::to_vec(&broken).unwrap()).unwrap();
        assert!(opened.sync().is_none(), "the sync space goes");
        assert_eq!(opened.items, data.items, "the logins stay");
        assert_eq!((opened.site_rules.clone(), opened.item_tombstones.clone()), (data.site_rules.clone(), data.item_tombstones.clone()));
        let local = opened.local.as_ref().unwrap();
        assert_eq!(local.generator_history, data.local.as_ref().unwrap().generator_history, "and the generated passwords");
        assert_eq!(local.view.group_order, ["Work"], "and every part that reads");
        assert_eq!(local.browser, data.local.as_ref().unwrap().browser, "the browser extension's pairings too");
        assert_eq!(opened.device(), data.device());
    }

    #[test]
    fn pairing_a_browser_extension_keeps_the_vault_in_format_2() {
        let mut data = VaultData::new();
        data.local_mut().browser.key = Some(crate::browser::StaticKey(Zeroizing::new([3; 32])));
        let bytes = data.to_bytes();
        assert_eq!(format_of(&bytes), 2, "0.9 still opens it, and drops the part it does not know");
        assert_eq!(VaultData::open(&bytes).unwrap().local.unwrap().browser, data.local.unwrap().browser);
    }

    #[test]
    fn a_default_identity_of_a_development_build_opens_and_is_written_no_more() {
        // Development builds of 0.10 had `local.identity` (a sign-up form's default user name and
        // email), which nothing set: a file with one opens, and the next write leaves it out.
        let mut data = VaultData::new();
        data.local_mut().item_view.collapsed_groups = vec!["Bank".into()];
        let mut earlier: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        earlier["local"]["identity"] = serde_json::json!({"username": "me", "email": "me@example.com"});
        let opened = VaultData::open(&serde_json::to_vec(&earlier).unwrap()).unwrap();
        assert_eq!(opened.local.as_ref().unwrap().item_view.collapsed_groups, ["Bank"]);
        let written: serde_json::Value = serde_json::from_slice(&opened.to_bytes()).unwrap();
        assert!(written["local"].get("identity").is_none(), "{written}");
    }

    #[test]
    fn a_local_part_of_a_later_shape_is_reset_alone() {
        let mut data = VaultData::new();
        data.local_mut().generator_history.push(GeneratedPassword { password: Zeroizing::new("generated".into()), at_ms: 12, site: None });
        data.local_mut().item_view.collapsed_groups = vec!["Bank".into()];
        data.local_mut().browser.key = Some(crate::browser::StaticKey(Zeroizing::new([3; 32])));
        let mut later: serde_json::Value = serde_json::from_slice(&data.to_bytes()).unwrap();
        later["local"]["browser"]["paired"] = serde_json::json!([{"id": "not a uuid"}]);
        later["local"]["generator_history"] = serde_json::json!({"a later": "shape"});
        later["local"]["view"] = serde_json::json!("not a view");
        let opened = VaultData::open(&serde_json::to_vec(&later).unwrap()).unwrap();
        let local = opened.local.as_ref().unwrap();
        assert!(local.generator_history.is_empty() && local.view.is_empty(), "each reset alone");
        assert!(local.browser.is_empty(), "the browser part too: the extensions pair again");
        assert_eq!(local.item_view.collapsed_groups, ["Bank"], "the rest kept");
        assert_eq!(opened.device(), data.device());
    }

    #[test]
    fn an_origin_a_later_lockra_added_is_kept_and_shown_as_a_link() {
        let mut entry = Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1);
        entry.origin = Lenient::Unknown("kdbx".into());
        let text = serde_json::to_string(&entry).unwrap();
        assert!(text.contains(r#""origin":"kdbx""#), "{text}");
        let read: Entry = serde_json::from_str(&text).unwrap();
        assert_eq!(read.origin, Lenient::Unknown("kdbx".into()));
        assert_eq!(read.view().origin, Origin::Uri, "the interface knows the five it always knew");
    }

    #[test]
    fn a_backup_carries_the_logins_and_rules_and_says_so_in_its_format() {
        let mut data = VaultData::new();
        data.entries.push(Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1));
        assert_eq!(format_of(&data.backup_bytes()), 2);
        let item = login(&mut data, "GitHub", "hunter2", 10);
        data.items.push(item);
        let rule = site_rule(&mut data, SiteRuleKind::NeverSave, "example.com", 11);
        data.site_rules.push(rule);
        data.local_mut().generator_history.push(GeneratedPassword { password: Zeroizing::new("generated".into()), at_ms: 12, site: None });
        let backup = data.backup_bytes();
        assert_eq!(format_of(&backup), 3);
        let text = std::str::from_utf8(&backup).unwrap();
        assert!(!text.contains("generated") && !text.contains("\"local\""), "the generated passwords stay on this device: {text}");
        let (mut restored, source_format) = VaultData::open_versioned(&backup).unwrap();
        assert_eq!(source_format, 3);
        assert_eq!((restored.items.clone(), restored.site_rules.clone()), (data.items.clone(), data.site_rules.clone()));
        assert!(restored.tick(1) > data.items[0].latest_stamp(), "a change made now comes after what the backup holds");
        // Only generated passwords: nothing a backup carries, so a backup of format 2.
        let mut only_generated = VaultData::new();
        only_generated.local_mut().generator_history.push(GeneratedPassword { password: Zeroizing::new("g".into()), at_ms: 1, site: None });
        assert_eq!((format_of(&only_generated.to_bytes()), format_of(&only_generated.backup_bytes())), (3, 2));
    }

    /// What 0.9.1 reads of a vault or a backup, frozen: it refuses any format above 2.
    fn open_as_0_9(bytes: &[u8]) -> Result<usize, &'static str> {
        #[derive(Deserialize)]
        struct Frozen {
            #[serde(default = "format_one")]
            format: u32,
            #[serde(default)]
            entries: Vec<serde_json::Value>,
        }
        let data: Frozen = serde_json::from_slice(bytes).map_err(|_| "vault_corrupted")?;
        if data.format > 2 {
            return Err("vault_unsupported");
        }
        Ok(data.entries.len())
    }

    #[test]
    fn lockra_0_9_refuses_a_payload_with_logins_instead_of_dropping_them() {
        let mut data = VaultData::new();
        data.entries.push(Entry::from_auth(auth("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP"), Origin::Uri, 1));
        assert_eq!(open_as_0_9(&data.to_bytes()), Ok(1));
        assert_eq!(open_as_0_9(&data.backup_bytes()), Ok(1));
        let item = login(&mut data, "GitHub", "hunter2", 10);
        data.items.push(item);
        assert_eq!(open_as_0_9(&data.to_bytes()), Err("vault_unsupported"));
        assert_eq!(open_as_0_9(&data.backup_bytes()), Err("vault_unsupported"));
    }

    #[test]
    fn a_restore_that_replaces_takes_the_backups_logins_back_after_everything() {
        // The backup: two logins and a rule; then this vault changes the first login's password,
        // adds a third login and a field, deletes the second login, and adds a rule.
        let mut data = VaultData::new();
        let mut kept = login(&mut data, "GitHub", "in the backup", 10);
        let deleted_since = login(&mut data, "Mail", "mail password", 11);
        data.items = vec![kept.clone(), deleted_since.clone()];
        let backup_rule = site_rule(&mut data, SiteRuleKind::NeverSave, "example.com", 12);
        data.site_rules.push(backup_rule.clone());
        let backup = VaultData::open(&data.backup_bytes()).unwrap();
        let backup_password = kept.field(Uuid::from_u128(2)).unwrap().clone();
        let (stamp, wid) = (data.tick(20), crate::item::random_write_id());
        kept.write_field(Uuid::from_u128(2), Purpose::Password, stamp, wid, 20, |f| f.value = Zeroizing::new("changed since".into()));
        let (stamp, wid) = (data.tick(21), crate::item::random_write_id());
        kept.write_field(Uuid::from_u128(9), Purpose::Custom, stamp, wid, 21, |f| f.label = "PIN".into());
        let new_since = login(&mut data, "Bank", "bank password", 22);
        data.items = vec![kept.clone(), new_since.clone()];
        let deleted_at = data.tick(23);
        data.item_tombstones.push(Tombstone { id: deleted_since.id, stamp: deleted_at, counter: None });
        let local_rule = site_rule(&mut data, SiteRuleKind::NeverFill, "bank.example", 24);
        data.site_rules.push(local_rule.clone());
        let before = data.clone();

        data.replace_items(&backup.items, &backup.item_tombstones, 100);
        data.replace_site_rules(&backup.site_rules, &backup.site_rule_tombstones, 100);
        let latest_before = before.latest_stamp().unwrap();
        // The backup's logins, and only them.
        let ids: Vec<Uuid> = data.items.iter().map(|i| i.id).collect();
        assert_eq!(ids, [kept.id, deleted_since.id]);
        let restored = &data.items[0];
        assert_eq!(password(restored), "in the backup");
        assert!(restored.field(Uuid::from_u128(9)).unwrap().removed, "the field added since is removed by a write of its own");
        assert!(data.items.iter().all(|i| i.stamp > latest_before && i.fields.iter().all(|f| f.stamp > latest_before)), "stamped after everything");
        assert_eq!(restored.field(Uuid::from_u128(2)).unwrap().wid, backup_password.wid, "the backup's write, taken back");
        assert!(!restored.conflict());
        // The login new since is deleted, the one deleted since is back.
        let tombstone = data.item_tombstones.iter().find(|t| t.id == new_since.id).unwrap();
        assert!(tombstone.stamp > latest_before);
        assert!(data.items.iter().all(|i| Record::stamp(i) > data.item_tombstones.iter().find(|t| t.id == i.id).map_or(Hlc::default(), |t| t.stamp)));
        // The rules: the backup's, stamped anew; the local one deleted.
        assert_eq!(data.site_rules.iter().map(|r| r.id).collect::<Vec<_>>(), [backup_rule.id]);
        assert!(data.site_rules[0].stamp > latest_before);
        assert!(data.site_rule_tombstones.iter().any(|t| t.id == local_rule.id && t.stamp > latest_before));

        // Another device's snapshots from before the restore change nothing back and raise no
        // conflict: one holding the backup's own values, one holding this vault's old ones.
        for older in [backup.clone(), before.clone()] {
            let mut merged = data.clone();
            crate::sync::merge_items(&mut merged.items, &mut merged.item_tombstones, &older.items, &older.item_tombstones);
            lockra_sync::merge(&mut merged.site_rules, &mut merged.site_rule_tombstones, &older.site_rules, &older.site_rule_tombstones);
            assert_eq!(merged.items.iter().map(|i| i.id).collect::<Vec<_>>(), [kept.id, deleted_since.id]);
            assert_eq!(password(&merged.items[0]), "in the backup");
            assert!(merged.items.iter().all(|i| !i.conflict()), "{:?}", merged.items[0]);
            assert_eq!(merged.site_rules.len(), 1);
        }
    }

    #[test]
    fn a_restored_field_has_seen_the_value_it_replaced() {
        let mut data = VaultData::new();
        let original = login(&mut data, "GitHub", "old", 10);
        let in_backup: Field = original.field(Uuid::from_u128(2)).unwrap().clone();
        let mut changed = original.clone();
        let (stamp, wid) = (data.tick(20), crate::item::random_write_id());
        changed.write_field(Uuid::from_u128(2), Purpose::Password, stamp, wid, 20, |f| f.value = Zeroizing::new("new".into()));
        let replaced_wid = changed.field(Uuid::from_u128(2)).unwrap().wid;
        data.items = vec![changed];
        data.replace_items(std::slice::from_ref(&original), &[], 30);
        let field = data.items[0].field(Uuid::from_u128(2)).unwrap();
        assert_eq!((field.wid, field.value.as_str()), (in_backup.wid, "old"));
        assert!(field.seen.contains(&replaced_wid), "{:?}", field.seen);
        assert!(data.items[0].history.iter().any(|p| p.wid == replaced_wid));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  GitHub\u{0007}\n "), "GitHub");
        assert_eq!(clean_name(&"x".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }
}
