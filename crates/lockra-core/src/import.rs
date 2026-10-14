//! The import preview: accounts and logins found in files, the clipboard or pasted text, held
//! until the user commits a choice per candidate. Nothing here touches the vault until
//! [`ImportSession::commit`].
//!
//! A login's code is a candidate of its own, before which comes the login: the commit takes the
//! codes first and maps each to the account it ends as (docs/formats.md §11, "Codes"), then the
//! logins, each linked to its code's account through that map: none when its code ends as no
//! account, never the id the source gave.

mod logins;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use lockra_otp::OtpAuth;
use lockra_transfer::google::Batch;
use lockra_transfer::kdbx::KeyFile;
use lockra_transfer::{Found, Item, Origin, RejectReason};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use self::logins::{FoundLogin, new_item, replace, same_content, too_large};
use crate::entry::{AccountColor, Entry, VaultData, clean_mark, clean_name};
use crate::item::VaultItem;
use crate::ui::{
    AwaitingKind, AwaitingView, CandidateAction, CandidateStatus, CandidateView, ConflictReason, GoogleBatchView, ImportSource, ImportView, LoginCandidateView,
};

/// What the user chose for one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Choice {
    /// The candidate.
    pub id: u32,
    /// What to do with it.
    pub action: CandidateAction,
}

/// What a commit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Outcome {
    /// New entries.
    pub added: u32,
    /// Entries whose secret was replaced.
    pub replaced: u32,
    /// Candidates left out.
    pub skipped: u32,
    /// What became of the logins, and of the other apps' items left out.
    pub logins: LoginOutcome,
}

/// What a commit did with the logins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LoginOutcome {
    /// New logins.
    pub added: u32,
    /// Logins of the vault changed: a new password, a new code linked, a backup's version.
    pub replaced: u32,
    /// Logins and other apps' items left out.
    pub skipped: u32,
}

/// A file in the import waiting for its password: a Lockra backup, or a KeePass database (which
/// may take a key file too).
pub(crate) struct Awaiting {
    pub(crate) name: String,
    pub(crate) bytes: Zeroizing<Vec<u8>>,
    pub(crate) kind: AwaitingKind,
    /// The key file chosen for a KeePass database: its name, and the key it brings.
    pub(crate) key_file: Option<(String, Arc<KeyFile>)>,
}

impl Awaiting {
    pub(crate) fn new(name: String, bytes: Zeroizing<Vec<u8>>, kind: AwaitingKind) -> Self {
        Self { name, bytes, kind, key_file: None }
    }
}

struct Candidate {
    id: u32,
    source: ImportSource,
    origin: Origin,
    body: Body,
}

enum Body {
    /// An account, or one the import cannot take.
    Code {
        item: Item,
        /// What a Lockra backup's entry brings along.
        extras: Option<Extras>,
    },
    /// A login.
    Login(Box<LoginCandidate>),
    /// Another app's item the import does not take (a card, a secure note, …).
    Left { label: String, reason: RejectReason },
}

struct LoginCandidate {
    found: FoundLogin,
    /// The candidate of its code.
    totp: Option<usize>,
}

/// A backup entry's group, pin, colour and mark: the account is restored as it looked.
struct Extras {
    group: Option<String>,
    favorite: bool,
    color: AccountColor,
    mark: Option<String>,
}

#[derive(Default)]
struct GoogleBatch {
    size: u32,
    received: BTreeSet<u32>,
}

/// What a login's code ends as, for comparing the login with the vault's: no account (the import
/// has no code for it, or leaves its code out), one the vault holds, or a new one (the preview's
/// guess for a code the commit adds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    None,
    Existing(Uuid),
    New,
}

impl Link {
    /// The link says nothing of the vault's login's, or the same.
    fn agrees(self, local: Option<Uuid>) -> bool {
        match self {
            Self::None => true,
            Self::Existing(id) => local == Some(id),
            Self::New => false,
        }
    }

    fn account(self) -> Option<Uuid> {
        match self {
            Self::Existing(id) => Some(id),
            Self::None | Self::New => None,
        }
    }
}

/// The candidates found so far.
#[derive(Default)]
pub(crate) struct ImportSession {
    candidates: Vec<Candidate>,
    google: BTreeMap<i32, GoogleBatch>,
    pub(crate) awaiting: Option<Awaiting>,
}

impl ImportSession {
    /// Add what one source yielded.
    pub(crate) fn add(&mut self, source: &ImportSource, origin: Origin, items: Vec<Item>) {
        for item in items {
            self.push(source.clone(), origin, Body::Code { item, extras: None });
        }
    }

    /// Add one Google export code.
    pub(crate) fn add_google(&mut self, source: &ImportSource, batch: Batch, items: Vec<Item>) {
        let entry = self.google.entry(batch.id).or_default();
        entry.size = entry.size.max(batch.size);
        if !entry.received.insert(batch.index) {
            // The same code scanned twice: its accounts are already listed.
            return;
        }
        self.add(source, Origin::Google, items);
    }

    /// Add the entries of a Lockra backup, keeping their group, pin, colour and mark, and its
    /// logins, each with the candidate of the backup's account it is linked to.
    pub(crate) fn add_backup(&mut self, source: &ImportSource, entries: Vec<Entry>, items: Vec<VaultItem>) {
        let mut of_entry: HashMap<Uuid, usize> = HashMap::new();
        for entry in entries {
            let auth = entry.to_auth();
            of_entry.insert(entry.id, self.candidates.len());
            let extras = Extras { group: entry.group, favorite: entry.favorite, color: entry.color, mark: entry.mark };
            self.push(source.clone(), Origin::Backup, Body::Code { item: Item::Account(auth), extras: Some(extras) });
        }
        for item in items {
            let totp = item.totp_entry.and_then(|id| of_entry.get(&id).copied());
            self.push(source.clone(), Origin::Backup, Body::Login(Box::new(LoginCandidate { found: FoundLogin::Backup(Box::new(item)), totp })));
        }
    }

    /// Add what another app's export holds: each login, its code right after it, and the items
    /// left out.
    pub(crate) fn add_logins(&mut self, source: &ImportSource, found: Vec<Found>) {
        for found in found {
            match found {
                Found::Login(mut login) => {
                    if too_large(&login) {
                        self.push(source.clone(), Origin::Uri, Body::Left { label: clean_name(&login.title), reason: RejectReason::TooLarge });
                        continue;
                    }
                    let code = login.totp.take();
                    let index = self.candidates.len();
                    let totp = code.is_some().then_some(index + 1);
                    self.push(source.clone(), Origin::Uri, Body::Login(Box::new(LoginCandidate { found: FoundLogin::Export(login), totp })));
                    if let Some(item) = code {
                        self.push(source.clone(), Origin::Uri, Body::Code { item, extras: None });
                    }
                }
                Found::Left { label, reason } => self.push(source.clone(), Origin::Uri, Body::Left { label: clean_name(&label), reason }),
            }
        }
    }

    fn push(&mut self, source: ImportSource, origin: Origin, body: Body) {
        let id = u32::try_from(self.candidates.len()).unwrap_or(u32::MAX);
        self.candidates.push(Candidate { id, source, origin, body });
    }

    /// Nothing found and nothing waiting.
    pub(crate) fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.awaiting.is_none()
    }

    /// The preview against the vault as it is now. A login's status takes its code's default
    /// choice: a code the commit would add is a new account.
    pub(crate) fn view(&self, data: &VaultData) -> ImportView {
        let mut links = vec![Link::None; self.candidates.len()];
        let mut statuses = Vec::with_capacity(self.candidates.len());
        for index in 0..self.candidates.len() {
            let status = match &self.candidates[index].body {
                Body::Code { .. } => {
                    let (status, duplicate_of) = self.code_status(index, data);
                    links[index] = match status {
                        CandidateStatus::New | CandidateStatus::Conflict { .. } => Link::New,
                        CandidateStatus::Exists { entry_id } => Link::Existing(entry_id),
                        CandidateStatus::Duplicate => duplicate_of.map_or(Link::None, |d| links[d]),
                        _ => Link::None,
                    };
                    status
                }
                Body::Login(_) | Body::Left { .. } => CandidateStatus::New,
            };
            statuses.push(status);
        }
        for (index, status) in statuses.iter_mut().enumerate() {
            match &self.candidates[index].body {
                Body::Login(login) => *status = self.login_status(index, &data.items, login.totp.map_or(Link::None, |c| links[c])).0,
                Body::Left { reason, .. } => *status = CandidateStatus::Unsupported { reason: *reason },
                Body::Code { .. } => {}
            }
        }
        let candidates = self
            .candidates
            .iter()
            .zip(statuses)
            .map(|(c, status)| {
                let (issuer, account, kind, algorithm, digits, line) = match &c.body {
                    Body::Code { item: Item::Account(a), .. } => {
                        (clean_name(&a.issuer), clean_name(&a.account), Some(a.kind), Some(a.algorithm), Some(a.digits), None)
                    }
                    Body::Code { item: Item::Rejected { label, line, .. }, .. } => (String::new(), label.clone(), None, None, None, *line),
                    Body::Left { label, .. } => (String::new(), label.clone(), None, None, None, None),
                    Body::Login(_) => (String::new(), String::new(), None, None, None, None),
                };
                let login = match &c.body {
                    Body::Login(login) => Some(LoginCandidateView {
                        title: login.found.title(),
                        username: login.found.username(),
                        host: login.found.host(),
                        totp: login.totp.and_then(|t| u32::try_from(t).ok()),
                    }),
                    _ => None,
                };
                CandidateView {
                    id: c.id,
                    source: c.source.clone(),
                    origin: c.origin,
                    issuer,
                    account,
                    kind,
                    algorithm,
                    digits,
                    line,
                    status,
                    default_action: default_action(status),
                    login,
                }
            })
            .collect();
        let google_batches = self
            .google
            .iter()
            .map(|(&id, b)| GoogleBatchView {
                id,
                size: b.size,
                received: b.received.iter().copied().collect(),
                missing: (0..b.size).filter(|i| !b.received.contains(i)).collect(),
            })
            .collect();
        let awaiting =
            self.awaiting.as_ref().map(|a| AwaitingView { name: a.name.clone(), kind: a.kind, key_file: a.key_file.as_ref().map(|(name, _)| name.clone()) });
        ImportView { candidates, google_batches, awaiting }
    }

    /// A code candidate's status, and the earlier candidate it duplicates.
    fn code_status(&self, index: usize, data: &VaultData) -> (CandidateStatus, Option<usize>) {
        let auth = match &self.candidates[index].body {
            Body::Code { item: Item::Account(auth), .. } => auth,
            Body::Code { item: Item::Rejected { reason, .. }, .. } => return (CandidateStatus::Unsupported { reason: *reason }, None),
            Body::Login(_) | Body::Left { .. } => return (CandidateStatus::New, None),
        };
        if let Some(existing) = data.entries.iter().find(|e| e.same_account(auth)) {
            return (CandidateStatus::Exists { entry_id: existing.id }, None);
        }
        let earlier =
            self.candidates[..index].iter().position(|c| matches!(&c.body, Body::Code { item: Item::Account(earlier), .. } if same_account(earlier, auth)));
        if let Some(earlier) = earlier {
            return (CandidateStatus::Duplicate, Some(earlier));
        }
        if let Some(existing) = data.entries.iter().find(|e| e.same_names(auth)) {
            return (CandidateStatus::Conflict { entry_id: existing.id }, None);
        }
        (CandidateStatus::New, None)
    }

    /// A login candidate's status against the vault's logins `items`, its code ending as
    /// `link`, and the earlier candidate it duplicates. A backup's login compares by its id
    /// first: the same login, changed or not; any other by its place, user name and password,
    /// its code as mapped.
    fn login_status(&self, index: usize, items: &[VaultItem], link: Link) -> (CandidateStatus, Option<usize>) {
        let Body::Login(login) = &self.candidates[index].body else { return (CandidateStatus::New, None) };
        let found = &login.found;
        if let Some(backup) = found.backup()
            && let Some(own) = items.iter().find(|i| i.id == backup.id)
        {
            let status = match (same_content(own, backup), link.agrees(own.totp_entry)) {
                (true, true) => CandidateStatus::LoginExists { item_id: own.id },
                (true, false) => CandidateStatus::LoginConflict { item_id: own.id, reason: ConflictReason::Code },
                (false, _) => CandidateStatus::LoginConflict { item_id: own.id, reason: ConflictReason::Changed },
            };
            return (status, None);
        }
        if let Some(own) = found.same_login(items)
            && link.agrees(own.totp_entry)
        {
            return (CandidateStatus::LoginExists { item_id: own.id }, None);
        }
        let earlier = self.candidates[..index].iter().position(|c| matches!(&c.body, Body::Login(earlier) if earlier.found.same_credentials(found)));
        if let Some(earlier) = earlier {
            return (CandidateStatus::Duplicate, Some(earlier));
        }
        if let Some(own) = found.same_site(items) {
            let reason = if found.same_password(own) { ConflictReason::Code } else { ConflictReason::Password };
            return (CandidateStatus::LoginConflict { item_id: own.id, reason }, None);
        }
        (CandidateStatus::New, None)
    }

    /// Apply the choices (the default action for every candidate not named) to `data`: the codes
    /// first, each mapped to the account it ends as, then the logins, linked through that map.
    pub(crate) fn commit(self, data: &mut VaultData, choices: &[Choice], now_ms: u64) -> Outcome {
        let mut outcome = Outcome::default();
        let chosen = |id: u32, status: CandidateStatus| choices.iter().find(|c| c.id == id).map_or_else(|| default_action(status), |c| c.action);
        let mut accounts: Vec<Option<Uuid>> = vec![None; self.candidates.len()];
        for index in 0..self.candidates.len() {
            let candidate = &self.candidates[index];
            let Body::Code { item, extras } = &candidate.body else { continue };
            let (status, duplicate_of) = self.code_status(index, data);
            let Item::Account(auth) = item else {
                outcome.skipped += 1;
                continue;
            };
            accounts[index] = match (status, chosen(candidate.id, status)) {
                (CandidateStatus::New | CandidateStatus::Conflict { .. }, CandidateAction::Add) | (CandidateStatus::New, CandidateAction::Replace) => {
                    let mut entry = Entry::from_auth(auth.clone(), candidate.origin, now_ms);
                    entry.stamp = data.tick(now_ms);
                    if let Some(extras) = extras {
                        entry.group = extras.group.clone();
                        entry.favorite = extras.favorite;
                        entry.color = extras.color;
                        entry.mark = extras.mark.as_deref().and_then(clean_mark);
                    }
                    let id = entry.id;
                    data.entries.push(entry);
                    outcome.added += 1;
                    Some(id)
                }
                // A new secret is a new account to the other devices of a sync space (an HOTP counter
                // starts again with it): the old entry goes, and the new one takes its place, its
                // group, its pin and its look.
                (CandidateStatus::Conflict { entry_id }, CandidateAction::Replace) => match data.remove(entry_id, now_ms) {
                    Some((position, old)) => {
                        let mut entry = Entry::from_auth(auth.clone(), candidate.origin, now_ms);
                        entry.stamp = data.tick(now_ms);
                        entry.group = old.group;
                        entry.favorite = old.favorite;
                        entry.color = old.color;
                        entry.mark = old.mark;
                        entry.created_at_ms = old.created_at_ms;
                        let id = entry.id;
                        data.entries.insert(position, entry);
                        outcome.replaced += 1;
                        Some(id)
                    }
                    None => {
                        outcome.skipped += 1;
                        None
                    }
                },
                // The account the vault holds already, or the one an earlier candidate ends as.
                (CandidateStatus::Exists { entry_id }, _) => {
                    outcome.skipped += 1;
                    Some(entry_id)
                }
                (CandidateStatus::Duplicate, _) => {
                    outcome.skipped += 1;
                    duplicate_of.and_then(|d| accounts[d])
                }
                // Skipped, or kept beside an account of the same name and another secret: no
                // account of the vault is this code's.
                _ => {
                    outcome.skipped += 1;
                    None
                }
            };
        }
        // The logins compare with the vault's as the preview showed them: two of this import of
        // the same site are two logins, as they were listed, not one replacing the other.
        let before = data.items.clone();
        for index in 0..self.candidates.len() {
            let candidate = &self.candidates[index];
            let login = match &candidate.body {
                Body::Login(login) => login,
                Body::Left { .. } => {
                    outcome.logins.skipped += 1;
                    continue;
                }
                Body::Code { .. } => continue,
            };
            let link = login.totp.and_then(|c| accounts[c]).map_or(Link::None, Link::Existing);
            let (status, _) = self.login_status(index, &before, link);
            match (status, chosen(candidate.id, status)) {
                (CandidateStatus::New, CandidateAction::Add | CandidateAction::Replace) | (CandidateStatus::LoginConflict { .. }, CandidateAction::Add) => {
                    let item = new_item(&login.found, link.account(), data, now_ms);
                    data.items.push(item);
                    outcome.logins.added += 1;
                }
                (CandidateStatus::LoginConflict { item_id, reason }, CandidateAction::Replace) => {
                    if replace(data, item_id, &login.found, reason, link.account(), now_ms) {
                        outcome.logins.replaced += 1;
                    } else {
                        outcome.logins.skipped += 1;
                    }
                }
                _ => outcome.logins.skipped += 1,
            }
        }
        outcome
    }
}

fn same_account(a: &OtpAuth, b: &OtpAuth) -> bool {
    let probe = Entry::from_auth(a.clone(), Origin::Uri, 0);
    probe.same_account(b)
}

fn default_action(status: CandidateStatus) -> CandidateAction {
    match status {
        CandidateStatus::New | CandidateStatus::Conflict { .. } => CandidateAction::Add,
        // The import's version of a login wins by default; the vault's password goes into the
        // history.
        CandidateStatus::LoginConflict { .. } => CandidateAction::Replace,
        CandidateStatus::Exists { .. } | CandidateStatus::LoginExists { .. } | CandidateStatus::Duplicate | CandidateStatus::Unsupported { .. } => {
            CandidateAction::Skip
        }
    }
}

#[cfg(test)]
mod tests;
