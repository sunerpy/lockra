//! The import preview: accounts found in files, the clipboard or pasted text, held until the user
//! commits a choice per account. Nothing here touches the vault until [`ImportSession::commit`].

use std::collections::{BTreeMap, BTreeSet};

use lockra_otp::OtpAuth;
use lockra_transfer::google::Batch;
use lockra_transfer::{Item, Origin};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::entry::{Entry, VaultData, clean_name};
use crate::ui::{CandidateAction, CandidateStatus, CandidateView, GoogleBatchView, ImportSource, ImportView};

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
}

/// A Lockra backup in the import, waiting for its password.
pub(crate) struct AwaitingBackup {
    pub(crate) name: String,
    pub(crate) bytes: Zeroizing<Vec<u8>>,
}

struct Candidate {
    id: u32,
    source: ImportSource,
    origin: Origin,
    item: Item,
    /// Group and pin carried over from a Lockra backup.
    extras: Option<(Option<String>, bool)>,
}

#[derive(Default)]
struct GoogleBatch {
    size: u32,
    received: BTreeSet<u32>,
}

/// The accounts found so far.
#[derive(Default)]
pub(crate) struct ImportSession {
    candidates: Vec<Candidate>,
    google: BTreeMap<i32, GoogleBatch>,
    pub(crate) awaiting: Option<AwaitingBackup>,
}

impl ImportSession {
    /// Add what one source yielded.
    pub(crate) fn add(&mut self, source: &ImportSource, origin: Origin, items: Vec<Item>) {
        for item in items {
            self.push(source.clone(), origin, item, None);
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

    /// Add the entries of a Lockra backup, keeping their group and pin.
    pub(crate) fn add_backup(&mut self, source: &ImportSource, entries: Vec<Entry>) {
        for entry in entries {
            let extras = Some((entry.group.clone(), entry.favorite));
            self.push(source.clone(), Origin::Backup, Item::Account(entry.to_auth()), extras);
        }
    }

    fn push(&mut self, source: ImportSource, origin: Origin, item: Item, extras: Option<(Option<String>, bool)>) {
        let id = u32::try_from(self.candidates.len()).unwrap_or(u32::MAX);
        self.candidates.push(Candidate { id, source, origin, item, extras });
    }

    /// Nothing found and nothing waiting.
    pub(crate) fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.awaiting.is_none()
    }

    /// The preview against the vault as it is now.
    pub(crate) fn view(&self, data: &VaultData) -> ImportView {
        let candidates = self
            .candidates
            .iter()
            .enumerate()
            .map(|(index, c)| {
                let status = self.status(index, data);
                let (issuer, account, kind, algorithm, digits, line) = match &c.item {
                    Item::Account(a) => (clean_name(&a.issuer), clean_name(&a.account), Some(a.kind), Some(a.algorithm), Some(a.digits), None),
                    Item::Rejected { label, line, .. } => (String::new(), label.clone(), None, None, None, *line),
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
        ImportView { candidates, google_batches, awaiting_password: self.awaiting.as_ref().map(|a| a.name.clone()) }
    }

    fn status(&self, index: usize, data: &VaultData) -> CandidateStatus {
        let auth = match &self.candidates[index].item {
            Item::Account(auth) => auth,
            Item::Rejected { reason, .. } => return CandidateStatus::Unsupported { reason: *reason },
        };
        if let Some(existing) = data.entries.iter().find(|e| e.same_account(auth)) {
            return CandidateStatus::Exists { entry_id: existing.id };
        }
        if self.candidates[..index].iter().any(|c| matches!(&c.item, Item::Account(earlier) if same_account(earlier, auth))) {
            return CandidateStatus::Duplicate;
        }
        if let Some(existing) = data.entries.iter().find(|e| e.same_names(auth)) {
            return CandidateStatus::Conflict { entry_id: existing.id };
        }
        CandidateStatus::New
    }

    /// Apply the choices (the default action for every candidate not named) to `data`.
    pub(crate) fn commit(self, data: &mut VaultData, choices: &[Choice], now_ms: u64) -> Outcome {
        let mut outcome = Outcome::default();
        for index in 0..self.candidates.len() {
            let status = self.status(index, data);
            let candidate = &self.candidates[index];
            let chosen = choices.iter().find(|c| c.id == candidate.id).map_or_else(|| default_action(status), |c| c.action);
            let Item::Account(auth) = &candidate.item else {
                outcome.skipped += 1;
                continue;
            };
            match (status, chosen) {
                (CandidateStatus::New | CandidateStatus::Conflict { .. }, CandidateAction::Add) | (CandidateStatus::New, CandidateAction::Replace) => {
                    let mut entry = Entry::from_auth(auth.clone(), candidate.origin, now_ms);
                    if let Some((group, favorite)) = &candidate.extras {
                        entry.group = group.clone();
                        entry.favorite = *favorite;
                    }
                    data.entries.push(entry);
                    outcome.added += 1;
                }
                (CandidateStatus::Conflict { entry_id }, CandidateAction::Replace) => {
                    if let Some(entry) = data.get_mut(entry_id) {
                        entry.issuer = clean_name(&auth.issuer);
                        entry.account = clean_name(&auth.account);
                        entry.kind = auth.kind;
                        entry.algorithm = auth.algorithm;
                        entry.digits = auth.digits;
                        entry.secret = auth.secret.clone();
                        entry.origin = candidate.origin;
                        entry.updated_at_ms = now_ms;
                        outcome.replaced += 1;
                    } else {
                        outcome.skipped += 1;
                    }
                }
                _ => outcome.skipped += 1,
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
        CandidateStatus::Exists { .. } | CandidateStatus::Duplicate | CandidateStatus::Unsupported { .. } => CandidateAction::Skip,
    }
}

#[cfg(test)]
mod tests {
    use lockra_otp::uri;
    use lockra_transfer::RejectReason;

    use super::*;

    fn auth(text: &str) -> OtpAuth {
        uri::parse(text).unwrap()
    }

    fn source() -> ImportSource {
        ImportSource::Text
    }

    fn vault_with(texts: &[&str]) -> VaultData {
        VaultData { format: 1, entries: texts.iter().map(|t| Entry::from_auth(auth(t), Origin::Uri, 1)).collect() }
    }

    #[test]
    fn statuses_follow_the_vault_and_the_earlier_candidates() {
        let data = vault_with(&["otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP", "otpauth://totp/Mail:me?secret=GEZDGNBV"]);
        let mut session = ImportSession::default();
        session.add(
            &source(),
            Origin::Uri,
            vec![
                Item::Account(auth("otpauth://totp/Renamed:x?secret=JBSWY3DPEHPK3PXP")),
                Item::Account(auth("otpauth://totp/mail:ME?secret=MZXW6YTBOI")),
                Item::Account(auth("otpauth://totp/New:one?secret=MFRGGZDF")),
                Item::Account(auth("otpauth://totp/New:again?secret=MFRGGZDF")),
                Item::Rejected { label: String::new(), line: Some(5), reason: RejectReason::NotOtpauth },
            ],
        );
        let view = session.view(&data);
        let statuses: Vec<_> = view.candidates.iter().map(|c| c.status).collect();
        assert_eq!(
            statuses,
            [
                CandidateStatus::Exists { entry_id: data.entries[0].id },
                CandidateStatus::Conflict { entry_id: data.entries[1].id },
                CandidateStatus::New,
                CandidateStatus::Duplicate,
                CandidateStatus::Unsupported { reason: RejectReason::NotOtpauth },
            ]
        );
        let actions: Vec<_> = view.candidates.iter().map(|c| c.default_action).collect();
        assert_eq!(actions, [CandidateAction::Skip, CandidateAction::Add, CandidateAction::Add, CandidateAction::Skip, CandidateAction::Skip]);
        assert_eq!(view.candidates[4].line, Some(5));
        assert!(view.candidates[4].kind.is_none());
    }

    #[test]
    fn commit_applies_defaults_and_choices() {
        let mut data = vault_with(&["otpauth://totp/Mail:me?secret=GEZDGNBV"]);
        let mut session = ImportSession::default();
        session.add(
            &source(),
            Origin::Google,
            vec![Item::Account(auth("otpauth://totp/Mail:me?secret=MZXW6YTBOI")), Item::Account(auth("otpauth://totp/A:b?secret=MFRGGZDF"))],
        );
        let original_id = data.entries[0].id;
        data.entries[0].favorite = true;
        let outcome = session.commit(&mut data, &[Choice { id: 0, action: CandidateAction::Replace }], 99);
        assert_eq!(outcome, Outcome { added: 1, replaced: 1, skipped: 0 });
        let replaced = data.get(original_id).unwrap();
        assert_eq!(replaced.secret.as_slice(), b"foobar");
        assert!(replaced.favorite, "a replace keeps the pin");
        assert_eq!((replaced.origin, replaced.updated_at_ms), (Origin::Google, 99));
        assert_eq!(data.entries.len(), 2);
    }

    #[test]
    fn invalid_choices_fall_back_to_skipping() {
        let mut data = vault_with(&["otpauth://totp/A:b?secret=GEZDGNBV"]);
        let mut session = ImportSession::default();
        session.add(
            &source(),
            Origin::Uri,
            vec![
                Item::Account(auth("otpauth://totp/A:b?secret=GEZDGNBV")),
                Item::Rejected { label: "x".into(), line: None, reason: RejectReason::Md5Algorithm },
            ],
        );
        let choices = [Choice { id: 0, action: CandidateAction::Add }, Choice { id: 1, action: CandidateAction::Add }];
        assert_eq!(session.commit(&mut data, &choices, 1), Outcome { added: 0, replaced: 0, skipped: 2 });
        assert_eq!(data.entries.len(), 1);
    }

    #[test]
    fn google_batches_track_missing_codes_and_ignore_rescans() {
        let mut session = ImportSession::default();
        let items = || vec![Item::Account(auth("otpauth://totp/G:a?secret=GEZDGNBV"))];
        session.add_google(&source(), Batch { id: 7, index: 0, size: 3 }, items());
        session.add_google(&source(), Batch { id: 7, index: 2, size: 3 }, vec![Item::Account(auth("otpauth://totp/G:c?secret=MZXW6YTBOI"))]);
        session.add_google(&source(), Batch { id: 7, index: 0, size: 3 }, items());
        let view = session.view(&VaultData::default());
        assert_eq!(view.candidates.len(), 2);
        assert_eq!(view.google_batches, [GoogleBatchView { id: 7, size: 3, received: vec![0, 2], missing: vec![1] }]);
    }

    #[test]
    fn backup_entries_keep_group_and_pin() {
        let mut backup = Entry::from_auth(auth("otpauth://totp/A:b?secret=GEZDGNBV"), Origin::Manual, 1);
        backup.group = Some("Work".into());
        backup.favorite = true;
        let mut session = ImportSession::default();
        session.add_backup(&ImportSource::File { name: "x.lockrabackup".into() }, vec![backup]);
        let mut data = VaultData::default();
        assert_eq!(session.commit(&mut data, &[], 5).added, 1);
        assert_eq!((data.entries[0].group.as_deref(), data.entries[0].favorite, data.entries[0].origin), (Some("Work"), true, Origin::Backup));
    }

    #[test]
    fn emptiness_counts_a_waiting_backup() {
        let mut session = ImportSession::default();
        assert!(session.is_empty());
        session.awaiting = Some(AwaitingBackup { name: "b".into(), bytes: Zeroizing::new(vec![1]) });
        assert!(!session.is_empty());
        assert_eq!(session.view(&VaultData::default()).awaiting_password.as_deref(), Some("b"));
    }
}
