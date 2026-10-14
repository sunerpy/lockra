use lockra_otp::uri;
use lockra_transfer::{App, Login, LoginField, LoginUrl, RejectReason, UrlMatch};

use super::*;
use crate::item::{Matching, Protection, Purpose};
use crate::lenient::Lenient;

fn auth(text: &str) -> OtpAuth {
    uri::parse(text).unwrap()
}

fn source() -> ImportSource {
    ImportSource::Text
}

fn vault_with(texts: &[&str]) -> VaultData {
    VaultData { entries: texts.iter().map(|t| Entry::from_auth(auth(t), Origin::Uri, 1)).collect(), ..VaultData::new() }
}

fn outcome(added: u32, replaced: u32, skipped: u32, logins: (u32, u32, u32)) -> Outcome {
    Outcome { added, replaced, skipped, logins: LoginOutcome { added: logins.0, replaced: logins.1, skipped: logins.2 } }
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
    assert!(view.candidates.iter().all(|c| c.login.is_none()), "accounts, no logins");
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
    assert_eq!(outcome, Outcome { added: 1, replaced: 1, skipped: 0, logins: LoginOutcome::default() });
    // In the old entry's place, as a new account: the old one leaves a tombstone.
    let replaced = &data.entries[0];
    assert_eq!(replaced.secret.as_slice(), b"foobar");
    assert!(replaced.favorite, "a replace keeps the pin");
    assert_eq!((replaced.origin.clone(), replaced.updated_at_ms), (Origin::Google.into(), 99));
    assert_ne!(replaced.id, original_id);
    assert!(data.get(original_id).is_none());
    assert!(data.tombstones.iter().any(|t| t.id == original_id));
    assert_eq!(data.entries.len(), 2);
}

#[test]
fn invalid_choices_fall_back_to_skipping() {
    let mut data = vault_with(&["otpauth://totp/A:b?secret=GEZDGNBV"]);
    let mut session = ImportSession::default();
    session.add(
        &source(),
        Origin::Uri,
        vec![Item::Account(auth("otpauth://totp/A:b?secret=GEZDGNBV")), Item::Rejected { label: "x".into(), line: None, reason: RejectReason::Md5Algorithm }],
    );
    let choices = [Choice { id: 0, action: CandidateAction::Add }, Choice { id: 1, action: CandidateAction::Add }];
    assert_eq!(session.commit(&mut data, &choices, 1), outcome(0, 0, 2, (0, 0, 0)));
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
    session.add_backup(&ImportSource::File { name: "x.lockrabackup".into() }, vec![backup], Vec::new());
    let mut data = VaultData::default();
    assert_eq!(session.commit(&mut data, &[], 5).added, 1);
    assert_eq!((data.entries[0].group.as_deref(), data.entries[0].favorite, data.entries[0].origin.clone()), (Some("Work"), true, Origin::Backup.into()));
}

#[test]
fn emptiness_counts_a_waiting_file() {
    let mut session = ImportSession::default();
    assert!(session.is_empty());
    session.awaiting = Some(Awaiting::new("b".into(), Zeroizing::new(vec![1]), AwaitingKind::Backup));
    assert!(!session.is_empty());
    assert_eq!(session.view(&VaultData::default()).awaiting, Some(AwaitingView { name: "b".into(), kind: AwaitingKind::Backup, key_file: None }));
    let key = Arc::new(KeyFile::from_bytes(b"key").unwrap());
    session.awaiting =
        Some(Awaiting { key_file: Some(("db.keyx".into(), key)), ..Awaiting::new("db.kdbx".into(), Zeroizing::new(vec![1]), AwaitingKind::Kdbx) });
    assert_eq!(
        session.view(&VaultData::default()).awaiting,
        Some(AwaitingView { name: "db.kdbx".into(), kind: AwaitingKind::Kdbx, key_file: Some("db.keyx".into()) })
    );
}

// ---- logins -----------------------------------------------------------------------------------

const GITHUB_CODE: &str = "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP";

/// An exported login of `title` at `url` for `username` with `password`.
fn login(title: &str, url: &str, username: &str, password: &str) -> Login {
    Login {
        app: App::Bitwarden,
        title: title.into(),
        username: username.into(),
        password: Zeroizing::new(password.into()),
        urls: vec![LoginUrl::plain(url)],
        notes: Zeroizing::new(String::new()),
        folder: None,
        favorite: false,
        tags: Vec::new(),
        fields: Vec::new(),
        reprompt: false,
        totp: None,
    }
}

fn with_code(mut login: Login, code: &str) -> Login {
    login.totp = Some(Item::Account(auth(code)));
    login
}

fn found(logins: Vec<Login>) -> Vec<Found> {
    logins.into_iter().map(|l| Found::Login(Box::new(l))).collect()
}

/// A vault login, made as `item_add` makes one.
fn vault_login(data: &mut VaultData, url: &str, username: &str, password: &str, totp_entry: Option<Uuid>) -> Uuid {
    let found = FoundLogin::Export(Box::new(login("Mine", url, username, password)));
    let item = new_item(&found, totp_entry, data, 1);
    let id = item.id;
    data.items.push(item);
    id
}

fn password_of(data: &VaultData, id: Uuid) -> String {
    data.items.iter().find(|i| i.id == id).and_then(|i| i.field_of(Purpose::Password)).map(|f| f.value.to_string()).unwrap_or_default()
}

fn item<'a>(data: &'a VaultData, title: &str) -> &'a VaultItem {
    data.items.iter().find(|i| i.title == title).unwrap_or_else(|| panic!("no {title}"))
}

#[test]
fn a_login_is_listed_with_its_code_right_after_it() {
    let mut session = ImportSession::default();
    session.add_logins(
        &source(),
        vec![
            Found::Login(Box::new(with_code(login("GitHub", "https://github.com/login", "octocat", "pw-1"), GITHUB_CODE))),
            Found::Login(Box::new(login("Router", "", "admin", "pw-2"))),
            Found::Left { label: "Visa".into(), reason: RejectReason::CardItem },
        ],
    );
    let view = session.view(&VaultData::new());
    assert_eq!(view.candidates.len(), 4);
    let github = &view.candidates[0];
    assert_eq!(github.login, Some(LoginCandidateView { title: "GitHub".into(), username: "octocat".into(), host: "github.com".into(), totp: Some(1) }));
    assert_eq!((github.status, github.default_action, github.kind, github.issuer.as_str()), (CandidateStatus::New, CandidateAction::Add, None, ""));
    assert_eq!((view.candidates[1].issuer.as_str(), view.candidates[1].origin, view.candidates[1].status), ("GitHub", Origin::Uri, CandidateStatus::New));
    assert_eq!(view.candidates[2].login.as_ref().map(|l| (l.host.as_str(), l.totp)), Some(("", None)));
    let visa = &view.candidates[3];
    assert_eq!((visa.account.as_str(), visa.status, visa.login.is_none()), ("Visa", CandidateStatus::Unsupported { reason: RejectReason::CardItem }, true));
    // Nothing of the secrets in the preview.
    assert!(!format!("{view:?}").contains("pw-1"));
}

#[test]
fn a_login_becomes_an_item_of_its_parts_linked_to_its_code() {
    let mut exported = with_code(login("GitHub", "https://github.com/login", "octocat", "pw-1"), GITHUB_CODE);
    exported.urls.push(LoginUrl { url: "https://gist.github.com".into(), matching: Some(UrlMatch::Host) });
    exported.urls.push(LoginUrl { url: "^https://github\\.com/".into(), matching: Some(UrlMatch::Regex) });
    exported.notes = Zeroizing::new("recovery codes in the safe".into());
    exported.folder = Some("Work".into());
    exported.favorite = true;
    exported.tags = vec!["dev".into(), "Dev".into(), " ".into()];
    exported.fields = vec![
        LoginField { label: "PIN".into(), value: Zeroizing::new("4242".into()), hidden: true },
        LoginField { label: "Customer".into(), value: Zeroizing::new("C-77".into()), hidden: false },
    ];
    exported.reprompt = true;
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![exported]));
    let mut data = VaultData::new();
    assert_eq!(session.commit(&mut data, &[], 7), outcome(1, 0, 0, (1, 0, 0)));
    let code = &data.entries[0];
    assert_eq!((code.issuer.as_str(), code.origin.clone()), ("GitHub", Origin::Uri.into()), "an imported code is an otpauth account");
    let github = item(&data, "GitHub");
    assert_eq!(github.totp_entry, Some(code.id));
    assert_eq!((github.source.as_deref(), github.group.as_deref(), github.favorite), (Some("bitwarden"), Some("Work"), true));
    assert_eq!(github.protection, Lenient::Known(Protection::Confirm), "Bitwarden's re-prompt asks before showing it");
    let rules: Vec<(&str, Matching)> = github.urls.iter().map(|u| (u.url.as_str(), u.rule())).collect();
    assert_eq!(
        rules,
        [("https://github.com/login", Matching::Domain), ("https://gist.github.com", Matching::Host), ("^https://github\\.com/", Matching::Regex)]
    );
    assert_eq!((github.notes.as_str(), github.tags.clone()), ("recovery codes in the safe", vec!["dev".to_owned()]));
    assert_eq!(github.field_of(Purpose::Username).map(|f| f.value.as_str()), Some("octocat"));
    assert_eq!(github.field_of(Purpose::Password).map(|f| (f.value.as_str(), f.hidden)), Some(("pw-1", true)));
    let custom: Vec<(&str, &str, bool)> =
        github.fields.iter().filter(|f| f.purpose == Lenient::Known(Purpose::Custom)).map(|f| (f.label.as_str(), f.value.as_str(), f.hidden)).collect();
    assert_eq!(custom, [("PIN", "4242", true), ("Customer", "C-77", false)]);
    assert!(github.is_tidy() && !github.conflict());
}

#[test]
fn logins_of_the_vault_are_found_by_site_user_name_and_password() {
    let mut data = vault_with(&[GITHUB_CODE]);
    let entry = data.entries[0].id;
    let same = vault_login(&mut data, "https://github.com/", "octocat", "pw-1", Some(entry));
    let other_password = vault_login(&mut data, "https://www.example.com/", "me", "old", None);
    let unlinked = vault_login(&mut data, "https://mail.example/", "me", "pw-3", None);
    let mut session = ImportSession::default();
    session.add_logins(
        &source(),
        found(vec![
            // The same login, its code the vault's account: exists.
            with_code(login("GitHub", "https://github.com/session", "octocat", "pw-1"), GITHUB_CODE),
            // The same site and user name, another password.
            login("Example", "https://www.example.com/login", "me", "new"),
            // The same credentials, and a code the vault's login has none of.
            with_code(login("Mail", "https://mail.example/inbox", "me", "pw-3"), "otpauth://totp/Mail:me?secret=MZXW6YTBOI"),
            // Another site, and the same login again in this import.
            login("New", "https://new.example/", "me", "pw-5"),
            login("New again", "https://new.example/signin", "me", "pw-5"),
        ]),
    );
    let view = session.view(&data);
    let logins: Vec<(CandidateStatus, CandidateAction)> = view.candidates.iter().filter(|c| c.login.is_some()).map(|c| (c.status, c.default_action)).collect();
    assert_eq!(
        logins,
        [
            (CandidateStatus::LoginExists { item_id: same }, CandidateAction::Skip),
            (CandidateStatus::LoginConflict { item_id: other_password, reason: ConflictReason::Password }, CandidateAction::Replace),
            (CandidateStatus::LoginConflict { item_id: unlinked, reason: ConflictReason::Code }, CandidateAction::Replace),
            (CandidateStatus::New, CandidateAction::Add),
            (CandidateStatus::Duplicate, CandidateAction::Skip),
        ]
    );
    assert_eq!(session.commit(&mut data, &[], 9), outcome(1, 0, 1, (1, 2, 2)));
    // The password replaced: the old one is in the history.
    assert_eq!(password_of(&data, other_password), "new");
    let replaced = data.items.iter().find(|i| i.id == other_password).unwrap();
    assert_eq!(replaced.history.iter().map(|p| p.value.as_str()).collect::<Vec<_>>(), ["old"]);
    assert!(!replaced.conflict(), "a replacing import has seen the value it replaces");
    // The code linked to the login that had none: the new account.
    let mail_code = data.entries.iter().find(|e| e.issuer == "Mail").unwrap().id;
    assert_eq!(data.items.iter().find(|i| i.id == unlinked).unwrap().totp_entry, Some(mail_code));
    assert_eq!(data.items.len(), 4);
}

#[test]
fn one_code_two_logins_one_account() {
    let mut session = ImportSession::default();
    session.add_logins(
        &source(),
        found(vec![
            with_code(login("GitHub", "https://github.com/", "octocat", "pw-1"), GITHUB_CODE),
            with_code(login("GitHub work", "https://github.com/", "octocat-work", "pw-2"), GITHUB_CODE),
        ]),
    );
    let view = session.view(&VaultData::new());
    assert_eq!(view.candidates[3].status, CandidateStatus::Duplicate, "the second login's code is the first's");
    let mut data = VaultData::new();
    assert_eq!(session.commit(&mut data, &[], 3), outcome(1, 0, 1, (2, 0, 0)));
    assert_eq!(data.entries.len(), 1);
    let account = data.entries[0].id;
    assert_eq!((item(&data, "GitHub").totp_entry, item(&data, "GitHub work").totp_entry), (Some(account), Some(account)));
}

#[test]
fn a_login_linked_elsewhere_takes_the_import_s_code_and_the_old_account_stays() {
    let mut data = vault_with(&["otpauth://totp/Old:x?secret=GEZDGNBV", GITHUB_CODE]);
    let (old, existing) = (data.entries[0].id, data.entries[1].id);
    let mine = vault_login(&mut data, "https://github.com/", "octocat", "pw-1", Some(old));
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![with_code(login("GitHub", "https://github.com/", "octocat", "pw-1"), GITHUB_CODE)]));
    let view = session.view(&data);
    assert_eq!(view.candidates[0].status, CandidateStatus::LoginConflict { item_id: mine, reason: ConflictReason::Code });
    assert_eq!(view.candidates[1].status, CandidateStatus::Exists { entry_id: existing });
    assert_eq!(session.commit(&mut data, &[], 4), outcome(0, 0, 1, (0, 1, 0)));
    let replaced = data.items.iter().find(|i| i.id == mine).unwrap();
    assert_eq!(replaced.totp_entry, Some(existing));
    assert!(data.get(old).is_some(), "the account linked before stays in the code list");
    assert_eq!(password_of(&data, mine), "pw-1");
}

#[test]
fn a_code_kept_apart_from_the_vault_s_account_of_its_name_links_nothing() {
    // The vault's GitHub account has another secret: the import's code conflicts with it.
    let mut data = vault_with(&["otpauth://totp/GitHub:octocat?secret=GEZDGNBV"]);
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![with_code(login("GitHub", "https://github.com/", "octocat", "pw-1"), GITHUB_CODE)]));
    let view = session.view(&data);
    assert!(matches!(view.candidates[1].status, CandidateStatus::Conflict { .. }));
    // Kept the vault's (skipped): the new login is linked to no account, not to the one of its name.
    let keep = [Choice { id: 1, action: CandidateAction::Skip }];
    assert_eq!(session.commit(&mut data, &keep, 4), outcome(0, 0, 1, (1, 0, 0)));
    assert_eq!(item(&data, "GitHub").totp_entry, None);
}

#[test]
fn a_skipped_code_leaves_the_vault_s_link_alone() {
    let mut data = vault_with(&["otpauth://totp/Old:x?secret=GEZDGNBV"]);
    let old = data.entries[0].id;
    let mine = vault_login(&mut data, "https://github.com/", "octocat", "pw-1", Some(old));
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![with_code(login("GitHub", "https://github.com/", "octocat", "pw-2"), GITHUB_CODE)]));
    let choices = [Choice { id: 0, action: CandidateAction::Replace }, Choice { id: 1, action: CandidateAction::Skip }];
    assert_eq!(session.commit(&mut data, &choices, 4), outcome(0, 0, 1, (0, 1, 0)));
    let replaced = data.items.iter().find(|i| i.id == mine).unwrap();
    assert_eq!((password_of(&data, mine).as_str(), replaced.totp_entry), ("pw-2", Some(old)));
}

#[test]
fn two_logins_of_one_site_in_one_import_stay_two_as_listed() {
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![login("One", "https://a.example/", "me", "first"), login("Two", "https://a.example/", "me", "second")]));
    let view = session.view(&VaultData::new());
    assert_eq!([view.candidates[0].status, view.candidates[1].status], [CandidateStatus::New, CandidateStatus::New]);
    let mut data = VaultData::new();
    assert_eq!(session.commit(&mut data, &[], 2).logins, LoginOutcome { added: 2, replaced: 0, skipped: 0 });
    assert_eq!((password_of(&data, item(&data, "One").id), password_of(&data, item(&data, "Two").id)), ("first".into(), "second".into()));
}

#[test]
fn a_login_without_a_url_is_known_by_its_title() {
    let mut data = VaultData::new();
    let mut router = login("Router", "", "admin", "pw-1");
    router.urls.clear();
    let found_router = FoundLogin::Export(Box::new(router.clone()));
    let mine = new_item(&found_router, None, &mut data, 1);
    let mine_id = mine.id;
    data.items.push(mine);
    let mut nas = router.clone();
    nas.title = "NAS".into();
    nas.password = Zeroizing::new("pw-2".into());
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![nas, router]));
    let statuses: Vec<CandidateStatus> = session.view(&data).candidates.iter().map(|c| c.status).collect();
    assert_eq!(statuses, [CandidateStatus::New, CandidateStatus::LoginExists { item_id: mine_id }], "another entry's password is not a conflict");
}

#[test]
fn a_login_past_what_an_item_holds_is_left_out() {
    let mut big = login("Big", "https://big.example/", "me", "pw");
    big.notes = Zeroizing::new("n".repeat(crate::item::MAX_NOTES_BYTES + 1));
    let mut long = login("Long", "https://long.example/", "me", &"p".repeat(crate::logins::MAX_VALUE_BYTES + 1));
    long.totp = Some(Item::Account(auth(GITHUB_CODE)));
    let mut many = login("Many", "https://many.example/", "me", "pw");
    many.fields = (0..crate::item::MAX_FIELDS).map(|i| LoginField { label: format!("f{i}"), value: Zeroizing::new("v".into()), hidden: false }).collect();
    let mut session = ImportSession::default();
    session.add_logins(&source(), found(vec![big, long, many]));
    let view = session.view(&VaultData::new());
    assert_eq!(view.candidates.len(), 3, "its code is left out with it");
    assert!(view.candidates.iter().all(|c| c.status == CandidateStatus::Unsupported { reason: RejectReason::TooLarge }));
    let mut data = VaultData::new();
    assert_eq!(session.commit(&mut data, &[], 1), outcome(0, 0, 0, (0, 0, 3)));
}

// ---- a backup's logins ------------------------------------------------------------------------

/// A backup's vault: an account and a login linked to it.
fn backup_vault() -> (VaultData, Uuid, Uuid) {
    let mut data = vault_with(&[GITHUB_CODE]);
    let entry = data.entries[0].id;
    let item = vault_login(&mut data, "https://github.com/", "octocat", "pw-1", Some(entry));
    (data, entry, item)
}

#[test]
fn a_backup_s_new_login_gets_a_new_id_and_the_mapped_account() {
    let (backup, backup_entry, backup_item) = backup_vault();
    let mut session = ImportSession::default();
    session.add_backup(&source(), backup.entries.clone(), backup.items.clone());
    let mut data = VaultData::new();
    let view = session.view(&data);
    assert_eq!(view.candidates[1].login.as_ref().and_then(|l| l.totp), Some(0), "linked to the backup's account's candidate");
    assert_eq!(session.commit(&mut data, &[], 50), outcome(1, 0, 0, (1, 0, 0)));
    let (entry, login) = (&data.entries[0], &data.items[0]);
    assert!(entry.id != backup_entry && login.id != backup_item, "new ids, as for a backup's accounts");
    assert_eq!(login.totp_entry, Some(entry.id), "the mapped account, not the backup's id");
    assert!(login.stamp.wall_ms == 50 && login.fields.iter().all(|f| f.stamp.wall_ms == 50), "stamped now, after any deletion here");
    assert_eq!(password_of(&data, login.id), "pw-1");
}

#[test]
fn a_backup_s_login_known_here_compares_by_its_id() {
    let (backup, entry, item_id) = backup_vault();
    // This vault is the backup's, later: the login renamed here.
    let mut data = backup.clone();
    let mut session = ImportSession::default();
    session.add_backup(&source(), backup.entries.clone(), backup.items.clone());
    assert_eq!(session.view(&data).candidates[1].status, CandidateStatus::LoginExists { item_id });
    let index = data.items.iter().position(|i| i.id == item_id).unwrap();
    data.items[index].title = "Renamed here".into();
    let (stamp, mwid) = (data.tick(60), crate::item::random_write_id());
    data.items[index].meta_written(stamp, mwid, 60);
    let view = session.view(&data);
    assert_eq!(view.candidates[1].status, CandidateStatus::LoginConflict { item_id, reason: ConflictReason::Changed });
    assert_eq!(view.candidates[1].default_action, CandidateAction::Replace);
    // Replacing takes the backup's version back, its link the account the vault holds.
    assert_eq!(session.commit(&mut data, &[], 70), outcome(0, 0, 1, (0, 1, 0)));
    let restored = data.items.iter().find(|i| i.id == item_id).unwrap();
    assert_eq!((restored.title.as_str(), restored.totp_entry), ("Mine", Some(entry)));
    assert_eq!(data.items.len(), 1);
}

#[test]
fn a_backup_s_login_linked_otherwise_is_a_conflict_of_its_code() {
    let (backup, _, item_id) = backup_vault();
    let mut data = backup.clone();
    // Here the login lost its link.
    let index = data.items.iter().position(|i| i.id == item_id).unwrap();
    data.items[index].totp_entry = None;
    let mut session = ImportSession::default();
    session.add_backup(&source(), backup.entries.clone(), backup.items.clone());
    assert_eq!(session.view(&data).candidates[1].status, CandidateStatus::LoginConflict { item_id, reason: ConflictReason::Code });
    // Kept as it is here: no link.
    let keep = [Choice { id: 1, action: CandidateAction::Skip }];
    assert_eq!(session.commit(&mut data, &keep, 70), outcome(0, 0, 1, (0, 0, 1)));
    assert_eq!(data.items[index].totp_entry, None);
}
