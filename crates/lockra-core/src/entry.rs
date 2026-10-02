//! Entries: what the vault payload stores, and the secret-free view the webview receives.

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, base32};
use lockra_sync::{Clock, Hlc, Record, Tombstone};
use lockra_transfer::{Incompatible, Origin, google, microsoft};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::{CoreResult, ErrorCode};
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
    /// Where it came from.
    pub origin: Origin,
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
            origin,
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
            origin: self.origin,
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

/// What an edit may change: the names, the group and the pin. Secrets and code parameters are
/// fixed for the life of an entry (delete it and add it again to change them).
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
}

/// The payload format this Lockra writes: 2 added the stamps, the tombstones and the local part.
pub const VAULT_DATA_FORMAT: u32 = 2;

/// The decrypted payload of the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultData {
    /// Payload format ([`VAULT_DATA_FORMAT`]; 1 before sync).
    #[serde(default = "format_one")]
    pub format: u32,
    /// The accounts, in insertion order.
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// The accounts deleted, so that a deletion on one device removes the account on the others.
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
    /// This device's own part: its clock and its sync space. In the vault file only: never in a
    /// backup, never synced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<Local>,
}

/// What only this device keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Local {
    /// Stamps this device's changes; its device number names this device in its sync space too.
    pub(crate) clock: Clock,
    /// The sync space this device belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sync: Option<SyncLocal>,
}

impl Local {
    /// A new device: a random number, nothing stamped yet, no sync.
    pub(crate) fn new() -> Self {
        Self { clock: Clock::new(random_device()), sync: None }
    }
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

#[derive(Serialize)]
struct BackupPayload<'a> {
    format: u32,
    entries: &'a [Entry],
    tombstones: &'a [Tombstone],
}

impl VaultData {
    /// A new vault's payload.
    pub fn new() -> Self {
        Self { format: VAULT_DATA_FORMAT, entries: Vec::new(), tombstones: Vec::new(), local: Some(Local::new()) }
    }

    /// The vault file's payload bytes for `lockra-vault`: everything, the local part included.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(self).expect("vault data serializes"))
    }

    /// A backup's payload bytes: the accounts and the deletions, without this device's part (its
    /// sync space and the storage's credentials stay in the vault file).
    pub fn backup_bytes(&self) -> Zeroizing<Vec<u8>> {
        let backup = BackupPayload { format: VAULT_DATA_FORMAT, entries: &self.entries, tombstones: &self.tombstones };
        // Serializing plain data structures to JSON cannot fail.
        #[allow(clippy::expect_used)]
        Zeroizing::new(serde_json::to_vec(&backup).expect("vault data serializes"))
    }

    /// Parse a payload; `None` when it is not a Lockra payload.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// A vault's or a backup's payload, ready for use: a newer format is refused, entries written
    /// before stamps get one (their last change, from this device), and a payload without a local
    /// part (a backup, an older vault) gets a new one.
    pub(crate) fn open(bytes: &[u8]) -> CoreResult<Self> {
        let mut data = Self::from_bytes(bytes).ok_or(ErrorCode::VaultCorrupted)?;
        if data.format > VAULT_DATA_FORMAT {
            return Err(ErrorCode::VaultUnsupported.into());
        }
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
        Ok(data)
    }

    pub(crate) fn local_mut(&mut self) -> &mut Local {
        self.local.get_or_insert_with(Local::new)
    }

    /// This device's number.
    pub(crate) fn device(&self) -> u64 {
        self.local.as_ref().map_or(0, |l| l.clock.device())
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

    /// Note the latest stamp of what a sync brought in: changes made here later come after it.
    pub(crate) fn observe_stamps(&mut self) {
        let latest = self.entries.iter().map(|e| e.stamp).chain(self.tombstones.iter().map(|t| t.stamp)).max();
        if let Some(latest) = latest {
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
        match self.tombstones.iter_mut().find(|t| t.id == tombstone.id) {
            Some(own) => {
                own.stamp = own.stamp.max(tombstone.stamp);
                own.counter = own.counter.max(tombstone.counter);
            }
            None => self.tombstones.push(tombstone),
        }
    }

    /// Replace the accounts by a backup's `entries` as one change made at `now_ms` (a restore
    /// that replaces): every account gets a new stamp, the ones that go get tombstones, and the
    /// backup's own deletions are kept, so the other devices of a sync space end up with the
    /// backup's accounts. An HOTP counter does not go back below what this vault had.
    pub(crate) fn replace_entries(&mut self, entries: Vec<Entry>, tombstones: &[Tombstone], now_ms: u64) {
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

    /// The entry with `id`.
    pub fn get(&self, id: Uuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry with `id`, mutably.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
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
        assert_eq!(VaultData::open(br#"{"format":3}"#).unwrap_err().code, ErrorCode::VaultUnsupported);
        assert_eq!(VaultData::open(b"not json").unwrap_err().code, ErrorCode::VaultCorrupted);
        // Opening again keeps the device and the stamps.
        let again = VaultData::open(&data.to_bytes()).unwrap();
        assert_eq!((again.device(), again.entries[0].stamp), (device, data.entries[0].stamp));
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

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  GitHub\u{0007}\n "), "GitHub");
        assert_eq!(clean_name(&"x".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }
}
