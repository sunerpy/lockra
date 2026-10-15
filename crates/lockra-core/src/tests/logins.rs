//! The login, generator and site-rule commands: what the webview sees, what it must prove first,
//! what goes on the clipboard, and when the list is to be asked for again.

use lockra_autofill::{GeneratorOptions, PassphraseOptions, PasswordOptions};

use super::*;
use crate::item::SiteRuleKind;
use crate::logins::{CopyPart, FieldChange, ItemDraft, ItemPatch, RevealParts, SecretChange, Settle, UrlDraft};
use crate::ui::CopiedPart;
use crate::{Matching, Protection, SYNC_DEBOUNCE};

const LEAKY: &str = "hunter2-do-not-leak";

fn set(value: &str) -> SecretChange {
    SecretChange::Set { value: pw(value) }
}

fn github(password: &str) -> ItemDraft {
    ItemDraft {
        title: "GitHub".into(),
        username: "octocat".into(),
        email: "octocat@example.com".into(),
        password: set(password),
        urls: vec![UrlDraft { url: "https://github.com/login".into(), matching: Matching::Domain }],
        notes: pw("recovery codes in the safe"),
        tags: vec!["work".into(), "Work".into(), " ".into()],
        group: Some("Dev".into()),
        fields: vec![
            FieldChange { label: Some("PIN".into()), value: set("4242"), hidden: Some(true), ..FieldChange::default() },
            FieldChange {
                label: Some("Customer no.".into()),
                value: set("C-77"),
                hidden: Some(false),
                page_field: Some("customer".into()),
                ..FieldChange::default()
            },
        ],
        ..ItemDraft::default()
    }
}

fn items(h: &Harness) -> Vec<crate::ItemView> {
    h.core.items_list().unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_login_is_listed_without_its_secrets_and_revealed_on_request() {
    let h = Harness::unlocked().await;
    let rev = h.core.state().items_rev;
    let id = h.core.item_add(github(LEAKY)).unwrap();
    let state = h.core.state();
    assert_eq!(state.items_count, 1);
    assert!(state.items_rev > rev, "the list is asked for again");
    let listed = items(&h);
    let view = &listed[0];
    assert_eq!((view.title.as_str(), view.username.as_str(), view.email.as_str()), ("GitHub", "octocat", "octocat@example.com"));
    assert_eq!((view.has_password, view.has_notes), (true, true));
    assert_eq!(view.tags, ["work"], "cleaned, the same tag once");
    assert_eq!(view.group.as_deref(), Some("Dev"));
    assert_eq!(view.fields.len(), 2);
    let pin = view.fields.iter().find(|f| f.label == "PIN").unwrap();
    assert_eq!((pin.hidden, pin.value.as_deref(), pin.has_value), (true, None, true), "a hidden field shows its name only");
    let customer = view.fields.iter().find(|f| f.label == "Customer no.").unwrap();
    assert_eq!((customer.value.as_deref(), customer.page_field.as_deref()), (Some("C-77"), Some("customer")));
    let text = serde_json::to_string(&listed).unwrap() + &serde_json::to_string(&h.core.state()).unwrap();
    for secret in [LEAKY, "recovery codes", "4242"] {
        assert!(!text.contains(secret), "{secret} in the list or the state");
    }
    // A login of the fill level shows its secrets as it would fill them.
    let parts = RevealParts { password: true, notes: true, history: true, fields: vec![pin.id] };
    let revealed = h.core.item_reveal(id, parts, None, None).await.unwrap();
    assert_eq!(revealed.password.as_deref(), Some(LEAKY));
    assert_eq!(revealed.notes.as_deref(), Some("recovery codes in the safe"));
    assert_eq!(revealed.fields.iter().map(|f| f.value.as_str()).collect::<Vec<_>>(), ["4242"]);
    assert_eq!(revealed.history, Some(Vec::new()));
    // Only what is asked for.
    let none = h.core.item_reveal(id, RevealParts::default(), None, None).await.unwrap();
    assert_eq!((none.password, none.notes, none.history, none.fields.len()), (None, None, None, 0));
    assert_eq!(code_err(h.core.item_reveal(Uuid::new_v4(), RevealParts::default(), None, None).await), ErrorCode::ItemNotFound);
}

#[tokio::test(start_paused = true)]
async fn a_login_of_the_confirm_level_asks_before_it_shows_or_copies_a_secret() {
    let h = Harness::unlocked().await;
    let id = h.core.item_add(ItemDraft { protection: Protection::Confirm, ..github(LEAKY) }).unwrap();
    let parts = || RevealParts { password: true, ..RevealParts::default() };
    // No biometric check is set up here: without the master password nothing is shown.
    assert_eq!(code_err(h.core.item_reveal(id, parts(), None, None).await), ErrorCode::BiometricUnavailable);
    assert_eq!(code_err(h.core.item_reveal(id, parts(), Some(pw("wrong password")), None).await), ErrorCode::WrongPassword);
    assert_eq!(h.core.item_reveal(id, parts(), Some(pw(MASTER)), None).await.unwrap().password.as_deref(), Some(LEAKY));
    assert_eq!(code_err(h.core.item_copy(id, CopyPart::Password, None, None).await), ErrorCode::BiometricUnavailable);
    assert!(h.clipboard.current().is_none());
    h.core.item_copy(id, CopyPart::Password, Some(pw(MASTER)), None).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some(LEAKY));
    // The user name is no secret: it copies without asking.
    h.core.item_copy(id, CopyPart::Username, None, None).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some("octocat"));
    // The protection is the login's own: an edit can lower it, and then nothing is asked.
    h.core.item_update(id, ItemPatch { protection: Some(Protection::NoExtension), ..ItemPatch::default() }).unwrap();
    assert_eq!(h.core.item_reveal(id, parts(), None, None).await.unwrap().password.as_deref(), Some(LEAKY));
}

#[tokio::test(start_paused = true)]
async fn copying_a_part_puts_it_on_the_clipboard_until_the_clipboard_time() {
    let mut h = Harness::unlocked().await;
    let entry = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let id = h.core.item_add(ItemDraft { totp_entry: Some(entry), ..github(LEAKY) }).unwrap();
    let _ = h.notices();
    h.core.item_copy(id, CopyPart::Password, None, None).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some(LEAKY));
    assert!(h.notices().contains(&Notice::ItemCopied { item_id: Some(id), part: CopiedPart::Password, clear_after_s: Some(30) }));
    assert!(items(&h)[0].last_used_at_ms.is_some(), "this device's copy time");
    // The linked account's current code.
    h.core.item_copy(id, CopyPart::Totp, None, None).await.unwrap();
    let code = h.clipboard.current().unwrap();
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
    let pin = items(&h)[0].fields.iter().find(|f| f.label == "PIN").unwrap().id;
    h.core.item_copy(id, CopyPart::Field { id: pin }, None, None).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some("4242"));
    h.core.item_copy(id, CopyPart::Email, None, None).await.unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some("octocat@example.com"));
    // Cleared after the clipboard time, as a code is.
    advance(Duration::from_secs(31)).await;
    assert_eq!(h.clipboard.current(), None);
    // Nothing to copy: an empty part, a login without a linked account.
    let bare = h.core.item_add(ItemDraft { title: "Bare".into(), ..ItemDraft::default() }).unwrap();
    assert_eq!(code_err(h.core.item_copy(bare, CopyPart::Password, None, None).await), ErrorCode::ItemNotFound);
    assert_eq!(code_err(h.core.item_copy(bare, CopyPart::Totp, None, None).await), ErrorCode::EntryNotFound);
}

#[tokio::test(start_paused = true)]
async fn an_edit_writes_only_the_parts_it_names() {
    let h = Harness::unlocked().await;
    let id = h.core.item_add(github("first")).unwrap();
    let before = h.core.data().items[0].clone();
    // A title alone: the metadata register, not the fields.
    h.core.item_update(id, ItemPatch { title: Some("GitHub (work)".into()), ..ItemPatch::default() }).unwrap();
    let after = h.core.data().items[0].clone();
    assert!(after.stamp > before.stamp && after.mwid != before.mwid);
    assert_eq!(after.fields, before.fields, "no field was written");
    // The same title again writes nothing.
    h.core.item_update(id, ItemPatch { title: Some("GitHub (work)".into()), ..ItemPatch::default() }).unwrap();
    assert_eq!(h.core.data().items[0], after);
    // A new password: its own register, the old one into the history.
    h.core.item_update(id, ItemPatch { password: set("second"), ..ItemPatch::default() }).unwrap();
    let item = h.core.data().items[0].clone();
    assert_eq!(item.stamp, after.stamp, "the metadata stays");
    assert_eq!(item.history.iter().map(|p| p.value.as_str()).collect::<Vec<_>>(), ["first"]);
    assert_eq!(items(&h)[0].history_count, 1);
    // Notes, user name, email, URLs, tags, group, pin, colour, mark, autofill and the link.
    let patch = ItemPatch {
        notes: SecretChange::Clear,
        username: Some(String::new()),
        email: Some("new@example.com".into()),
        urls: Some(vec![
            UrlDraft { url: "  https://github.com/  ".into(), matching: Matching::Host },
            UrlDraft { url: String::new(), matching: Matching::Domain },
        ]),
        tags: Some(Vec::new()),
        group: Some(String::new()),
        favorite: Some(true),
        color: Some(AccountColor::Teal),
        mark: Some("GH".into()),
        autofill: Some(crate::AutofillChoice { auto_submit: crate::Tri::On, ..Default::default() }),
        ..ItemPatch::default()
    };
    h.core.item_update(id, patch).unwrap();
    let view = items(&h)[0].clone();
    assert!(!view.has_notes);
    assert_eq!((view.username.as_str(), view.email.as_str()), ("", "new@example.com"));
    assert_eq!(view.urls.iter().map(|u| (u.url.as_str(), u.matching)).collect::<Vec<_>>(), [("https://github.com/", Matching::Host)]);
    assert!(view.tags.is_empty() && view.group.is_none() && view.favorite);
    assert_eq!((view.color, view.mark.as_deref(), view.autofill.auto_submit), (AccountColor::Teal, Some("GH"), crate::Tri::On));
    // Custom fields: change one, remove one, add one.
    let pin = view.fields.iter().find(|f| f.label == "PIN").unwrap().id;
    let customer = view.fields.iter().find(|f| f.label == "Customer no.").unwrap().id;
    let patch = ItemPatch {
        fields: vec![
            FieldChange { id: Some(pin), value: set("9999"), ..FieldChange::default() },
            FieldChange { id: Some(customer), remove: true, ..FieldChange::default() },
            FieldChange { label: Some("Recovery".into()), value: set("abc"), ..FieldChange::default() },
        ],
        ..ItemPatch::default()
    };
    h.core.item_update(id, patch).unwrap();
    let labels: Vec<String> = items(&h)[0].fields.iter().map(|f| f.label.clone()).collect();
    assert_eq!(labels, ["PIN", "Recovery"]);
    let revealed = h.core.item_reveal(id, RevealParts { fields: vec![pin], history: true, ..RevealParts::default() }, None, None).await.unwrap();
    assert_eq!(revealed.fields[0].value, "9999");
    assert!(revealed.history.unwrap().iter().any(|p| p.value == "4242"), "the old PIN is kept");
    // Edits of what is not there, or would break the limits.
    assert_eq!(code_err(h.core.item_update(Uuid::new_v4(), ItemPatch::default())), ErrorCode::ItemNotFound);
    let unknown = ItemPatch { fields: vec![FieldChange { id: Some(Uuid::new_v4()), ..FieldChange::default() }], ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, unknown)), ErrorCode::ItemNotFound);
    let long = ItemPatch { title: Some("x".repeat(201)), ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, long)), ErrorCode::ItemInvalid);
    let notes = ItemPatch { notes: set(&"n".repeat(64 * 1024 + 1)), ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, notes)), ErrorCode::ItemInvalid);
    let urls = ItemPatch { urls: Some(vec![UrlDraft { url: "https://a.example/".into(), matching: Matching::Domain }; 33]), ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, urls)), ErrorCode::ItemInvalid);
    let link = ItemPatch { totp_entry: Some(Some(Uuid::new_v4())), ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, link)), ErrorCode::EntryNotFound);
    let many = ItemPatch { fields: (0..62).map(|n| FieldChange { label: Some(format!("f{n}")), ..FieldChange::default() }).collect(), ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, many)), ErrorCode::ItemInvalid);
}

#[tokio::test(start_paused = true)]
async fn a_conflict_is_settled_by_taking_a_value() {
    let h = Harness::unlocked().await;
    let id = h.core.item_add(github("first")).unwrap();
    // Another device changed the password without seeing this one's change: written here as the
    // merge leaves it (its value lost, into the history, unseen).
    h.core.item_update(id, ItemPatch { password: set("mine"), ..ItemPatch::default() }).unwrap();
    h.core.edit_data(|data| {
        let item = &mut data.items[0];
        let field = item.field_of(crate::Purpose::Password).unwrap().id;
        item.history.insert(0, crate::PastValue { field, value: pw("theirs"), set_at: lockra_sync::Hlc::at(1), wid: 77 });
    });
    let view = items(&h)[0].clone();
    assert_eq!(view.conflicts.len(), 1);
    let field = view.conflicts[0].field;
    let history = h.core.item_reveal(id, RevealParts { history: true, ..RevealParts::default() }, None, None).await.unwrap().history.unwrap();
    let theirs = history.iter().find(|p| p.value == "theirs").unwrap();
    assert!(theirs.conflict);
    h.core.item_update(id, ItemPatch { settle: vec![Settle { field, take: Some(theirs.id.clone()) }], ..ItemPatch::default() }).unwrap();
    assert!(items(&h)[0].conflicts.is_empty());
    let revealed = h.core.item_reveal(id, RevealParts { password: true, ..RevealParts::default() }, None, None).await.unwrap();
    assert_eq!(revealed.password.as_deref(), Some("theirs"));
    let unknown = ItemPatch { settle: vec![Settle { field, take: Some("12345".into()) }], ..ItemPatch::default() };
    assert_eq!(code_err(h.core.item_update(id, unknown)), ErrorCode::ItemNotFound);
}

#[tokio::test(start_paused = true)]
async fn a_login_a_later_lockra_wrote_is_shown_and_not_changed() {
    let h = Harness::unlocked().await;
    let id = h.core.item_add(github("first")).unwrap();
    h.core.edit_data(|data| data.items[0].kind = crate::Lenient::Unknown("bank_card".into()));
    let view = items(&h)[0].clone();
    assert!(view.read_only);
    assert_eq!(view.kind, "bank_card");
    assert_eq!(code_err(h.core.item_update(id, ItemPatch { title: Some("x".into()), ..ItemPatch::default() })), ErrorCode::ItemReadOnly);
    // A URL rule of a later kind makes it read-only too, and matches nothing meanwhile.
    let other = h.core.item_add(github("second")).unwrap();
    h.core.edit_data(|data| data.items.iter_mut().find(|i| i.id == other).unwrap().urls[0].matching = crate::Lenient::Unknown("fuzzy".into()));
    let view = items(&h).into_iter().find(|i| i.id == other).unwrap();
    assert!(view.read_only);
    assert_eq!(view.urls[0].matching, Matching::Never);
    // Deleting is still the user's to do.
    h.core.item_delete(&[id, other]).unwrap();
    assert!(items(&h).is_empty());
    assert_eq!(h.core.data().item_tombstones.len(), 2);
    assert_eq!(code_err(h.core.item_delete(&[id])), ErrorCode::ItemNotFound);
}

#[tokio::test(start_paused = true)]
async fn the_generator_makes_copies_and_remembers_what_was_used() {
    let mut h = Harness::unlocked().await;
    let options = GeneratorOptions::Password(PasswordOptions { length: 24, ..PasswordOptions::default() });
    let made = h.core.generator_generate(&options, None).unwrap();
    assert_eq!(made.password.chars().count(), 24);
    assert!(made.bits > 100 && !made.rules_met);
    assert_eq!(h.core.state().generator.history_count, 0, "made is not used");
    let _ = h.notices();
    h.core.generator_copy().unwrap();
    assert_eq!(h.clipboard.current().as_deref(), Some(made.password.as_str()));
    assert!(h.notices().contains(&Notice::ItemCopied { item_id: None, part: CopiedPart::Generated, clear_after_s: Some(30) }));
    assert_eq!(h.core.state().generator.history_count, 1);
    // Saved with the generated password: the webview sends nothing back.
    let id = h.core.item_add(ItemDraft { password: SecretChange::Generated, ..github("unused") }).unwrap();
    let revealed = h.core.item_reveal(id, RevealParts { password: true, ..RevealParts::default() }, None, None).await.unwrap();
    assert_eq!(revealed.password.as_deref(), Some(made.password.as_str()));
    assert_eq!(h.core.state().generator.history_count, 1, "the same password once");
    // To a site's rules: Lockra bundles some.
    // Joined by spaces: four words of the EFF list have a hyphen of their own (t-shirt, yo-yo), so a
    // passphrase joined by "-" does not always split into its words.
    let passphrase = GeneratorOptions::Passphrase(PassphraseOptions { separator: " ".to_owned(), ..PassphraseOptions::default() });
    let words = h.core.generator_generate(&passphrase, None).unwrap();
    assert_eq!(words.password.split(' ').count(), 6);
    h.core.generator_copy().unwrap();
    // The history needs proof, newest first.
    assert_eq!(code_err(h.core.generator_history_reveal(None, None).await), ErrorCode::BiometricUnavailable);
    let history = h.core.generator_history_reveal(Some(pw(MASTER)), None).await.unwrap();
    assert_eq!(history.iter().map(|g| g.password.as_str()).collect::<Vec<_>>(), [words.password.as_str(), made.password.as_str()]);
    h.core.generator_history_clear().unwrap();
    assert_eq!(h.core.state().generator.history_count, 0);
    // Nothing made since the vault opened: nothing to copy or save.
    h.core.lock_vault();
    h.core.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(code_err(h.core.generator_copy()), ErrorCode::ItemInvalid);
    assert_eq!(code_err(h.core.item_add(ItemDraft { password: SecretChange::Generated, ..ItemDraft::default() })), ErrorCode::ItemInvalid);
    let nothing = GeneratorOptions::Password(PasswordOptions { lower: false, upper: false, digits: false, symbols: false, ..PasswordOptions::default() });
    assert_eq!(code_err(h.core.generator_generate(&nothing, None)), ErrorCode::ItemInvalid);
    let score = h.core.password_score("password").unwrap();
    assert_eq!((score.score, score.warning.as_deref()), (0, Some("top_ten")));
    assert_eq!(h.core.password_score(&made.password).unwrap().score, 4);
}

#[tokio::test(start_paused = true)]
async fn the_generator_keeps_to_a_sites_rules() {
    let h = Harness::unlocked().await;
    // Apple's list: apple.com wants at most 32 characters, no more than two the same in a row.
    let options = GeneratorOptions::Password(PasswordOptions { length: 60, ..PasswordOptions::default() });
    let made = h.core.generator_generate(&options, Some("apple.com".into())).unwrap();
    assert!(made.rules_met);
    let rules = lockra_autofill::bundled_password_rules("apple.com").unwrap();
    assert!(rules.is_satisfied_by(&made.password), "{}", made.password);
    let unsatisfiable = GeneratorOptions::Password(PasswordOptions { exclude: "abcdefghijklmnopqrstuvwxyz".into(), ..PasswordOptions::default() });
    // A site that requires lower case letters, with all of them excluded.
    assert_eq!(code_err(h.core.generator_generate(&unsatisfiable, Some("apple.com".into()))), ErrorCode::GeneratorRulesUnsatisfiable);
}

#[tokio::test(start_paused = true)]
async fn site_rules_are_added_widened_and_removed() {
    let h = Harness::unlocked().await;
    h.core.site_rule_add(SiteRuleKind::NeverSave, &["https://Example.com/login".into()]).unwrap();
    let rules = h.core.state().site_rules;
    assert_eq!(rules.len(), 1);
    assert_eq!((rules[0].kind.as_str(), rules[0].domains.clone()), ("never_save", vec!["example.com".to_owned()]));
    assert_eq!(rules[0].id, crate::SiteRule::single_id(SiteRuleKind::NeverSave, "example.com"), "the same id on every device");
    // The same rule again is the same rule.
    h.core.site_rule_add(SiteRuleKind::NeverSave, &["example.com".into()]).unwrap();
    assert_eq!(h.core.state().site_rules.len(), 1);
    h.core.site_rule_add(SiteRuleKind::Equivalent, &["example.com".into(), "example.org".into()]).unwrap();
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::Equivalent, &["example.com".into()])), ErrorCode::ItemInvalid);
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::NeverFill, &[])), ErrorCode::ItemInvalid);
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::NeverFill, &["exa mple.com".into()])), ErrorCode::ItemInvalid);
    let id = rules[0].id;
    h.core.site_rule_remove(id).unwrap();
    assert_eq!(h.core.state().site_rules.len(), 1);
    assert!(h.core.data().site_rule_tombstones.iter().any(|t| t.id == id));
    assert_eq!(code_err(h.core.site_rule_remove(id)), ErrorCode::ItemNotFound);
}

#[tokio::test(start_paused = true)]
async fn the_list_is_asked_for_again_after_every_change_and_as_the_vault_opens_and_locks() {
    let h = Harness::unlocked().await;
    let rev = || h.core.state().items_rev;
    let mut last = rev();
    let mut moved = |what: &str| {
        let now = h.core.state().items_rev;
        assert!(now > last, "{what}");
        last = now;
    };
    let id = h.core.item_add(github("first")).unwrap();
    moved("added");
    h.core.item_update(id, ItemPatch { favorite: Some(true), ..ItemPatch::default() }).unwrap();
    moved("edited");
    h.core.item_copy(id, CopyPart::Username, None, None).await.unwrap();
    moved("copied: its copy time changed");
    h.core.site_rule_add(SiteRuleKind::NeverFill, &["bank.example".into()]).unwrap();
    moved("a rule");
    h.core.lock_vault();
    assert_eq!(h.core.state().items_count, 0);
    moved("locked");
    h.core.unlock(pw(MASTER)).await.unwrap();
    moved("unlocked");
    // A restore that replaces.
    let file = h.dir.path().join("logins.lockrabackup");
    h.core.backup_to(file.clone(), None).await.unwrap();
    h.core.restore_open(file).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Replace).await.unwrap();
    moved("restored");
    // Unrelated changes leave it.
    let before = rev();
    h.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    h.core.activity();
    assert_eq!(rev(), before);
}

#[tokio::test(start_paused = true)]
async fn the_list_is_asked_for_again_when_a_sync_brings_logins_or_rules() {
    let (desktop, phone) = super::items::two_devices().await;
    let run = || async {
        advance(SYNC_DEBOUNCE).await;
        settle().await;
        phone.core.sync_now().unwrap();
        settle().await;
    };
    let before = phone.core.state().items_rev;
    desktop.core.item_add(github("first")).unwrap();
    run().await;
    assert_eq!(phone.core.state().items_count, 1);
    assert!(phone.core.state().items_rev > before, "a login came");
    let before = phone.core.state().items_rev;
    desktop.core.site_rule_add(SiteRuleKind::NeverSave, &["bank.example".into()]).unwrap();
    run().await;
    assert_eq!(phone.core.state().site_rules.len(), 1);
    assert!(phone.core.state().items_rev > before, "a rule came");
    // A run that brings only codes leaves it.
    let before = phone.core.state().items_rev;
    desktop.core.add_uri(&otpauth("Mail", "me", "GEZDGNBV")).unwrap();
    run().await;
    assert_eq!(phone.core.state().entries.len(), 1);
    assert_eq!(phone.core.state().items_rev, before);
}

#[tokio::test(start_paused = true)]
async fn the_password_lists_folds_are_this_devices_and_kept() {
    let h = Harness::unlocked().await;
    h.core.item_add(ItemDraft { group: Some("Work".into()), ..github("a") }).unwrap();
    h.core.item_add(ItemDraft { group: None, ..github("b") }).unwrap();
    assert!(h.core.state().item_collapsed_groups.is_empty());
    let rev = h.core.state().items_rev;
    h.core.collapse_item_groups(vec!["Work".into(), String::new(), "Gone".into(), " Work ".into()]).unwrap();
    let state = h.core.state();
    assert_eq!(state.item_collapsed_groups, ["", "Work"], "groups that exist, once each, and the logins in none");
    assert_eq!(state.items_rev, rev, "a fold is no change of the logins");
    h.core.lock_vault();
    assert!(h.core.state().item_collapsed_groups.is_empty(), "nothing shown while locked");
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(restarted.state().item_collapsed_groups, ["", "Work"], "kept in the vault");
    restarted.collapse_item_groups(Vec::new()).unwrap();
    assert!(restarted.state().item_collapsed_groups.is_empty());
}

#[tokio::test(start_paused = true)]
async fn nothing_of_it_answers_while_password_management_is_off() {
    let h = harness_with_features(Features { passwords: false });
    h.core.create_vault(pw(MASTER)).await.unwrap();
    assert!(!h.core.state().features.passwords);
    let id = Uuid::new_v4();
    let off = ErrorCode::FeatureUnavailable;
    assert_eq!(code_err(h.core.items_list()), off);
    assert_eq!(code_err(h.core.item_add(ItemDraft::default())), off);
    assert_eq!(code_err(h.core.item_update(id, ItemPatch::default())), off);
    assert_eq!(code_err(h.core.item_delete(&[id])), off);
    assert_eq!(code_err(h.core.item_reveal(id, RevealParts::default(), None, None).await), off);
    assert_eq!(code_err(h.core.item_copy(id, CopyPart::Password, None, None).await), off);
    assert_eq!(code_err(h.core.generator_generate(&GeneratorOptions::default(), None)), off);
    assert_eq!(code_err(h.core.generator_copy()), off);
    assert_eq!(code_err(h.core.generator_history_reveal(None, None).await), off);
    assert_eq!(code_err(h.core.generator_history_clear()), off);
    assert_eq!(code_err(h.core.password_score("x")), off);
    assert_eq!(code_err(h.core.site_rule_add(SiteRuleKind::NeverSave, &["a.example".into()])), off);
    assert_eq!(code_err(h.core.site_rule_remove(id)), off);
    assert_eq!(code_err(h.core.collapse_item_groups(Vec::new())), off);
}

#[tokio::test(start_paused = true)]
async fn the_list_of_three_thousand_logins_stays_small_and_quick() {
    let h = Harness::unlocked().await;
    h.core.edit_data(|data| {
        for n in 0..3000 {
            let now = T0 + n;
            let (stamp, mwid) = (data.tick(now), crate::item::random_write_id());
            let mut item = crate::VaultItem::login(Uuid::new_v4(), now, stamp, mwid);
            item.title = format!("Service number {n}");
            item.urls = vec![crate::UrlRule { url: format!("https://login.service{n}.example.com/"), matching: Default::default(), extra: Default::default() }];
            item.tags = vec!["work".into()];
            for (field, purpose, value) in [(1, crate::Purpose::Username, format!("user{n}")), (2, crate::Purpose::Password, format!("Kx9!vQ2#mZ7@pL4&{n}"))] {
                let (stamp, wid) = (data.tick(now), crate::item::random_write_id());
                item.write_field(Uuid::from_u128(field), purpose, stamp, wid, now, |f| {
                    f.value = pw(&value);
                    f.hidden = purpose == crate::Purpose::Password;
                });
            }
            data.items.push(item);
        }
    });
    let started = std::time::Instant::now();
    let listed = h.core.items_list().unwrap();
    let bytes = serde_json::to_vec(&listed).unwrap();
    let took = started.elapsed();
    assert_eq!(listed.len(), 3000);
    assert!(bytes.len() < 1_500_000, "{} bytes", bytes.len());
    // 10 ms in a release build; a debug build is several times slower.
    let budget = if cfg!(debug_assertions) { Duration::from_millis(250) } else { Duration::from_millis(10) };
    assert!(took < budget, "{took:?}");
}
