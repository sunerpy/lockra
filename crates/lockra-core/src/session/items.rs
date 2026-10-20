//! The commands of the logins, the generator and the site rules (docs/security.md, "In the running
//! app"). They answer `feature_unavailable` while password management is off in this build.

use std::time::Duration;

use lockra_autofill::{GeneratorError, GeneratorOptions, OsRandom, WebUrl, bundled_password_rules, generate, strength};
use lockra_sync::{Hlc, Tombstone};
use lockra_transfer::App;
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{Core, PhaseState, Session, State, blocking, check_password, current_code, file_name, touch_items, unlocked, unlocked_mut, write_private};
use crate::browser::{covering, host_rule, rule_host};
use crate::entry::VaultData;
use crate::error::{CoreResult, ErrorCode};
use crate::export::items::{ItemsFile, ItemsFormat, encode, login_of};
use crate::item::{Field, GeneratedPassword, Protection, Purpose, SiteRule, SiteRuleKind, VaultItem, random_write_id};
use crate::lenient::Lenient;
use crate::logins::{
    CopyPart, FieldChange, GeneratedPastView, GeneratedView, ItemDraft, ItemPatch, ItemRevealed, ItemView, RevealParts, RevealedField, ScoreView, SecretChange,
    autofill_prefs, check_field_count, checked_notes, checked_value, clean_domains, clean_group, clean_item_mark, clean_tags, clean_text, clean_title,
    clean_urls, holds_later_values, past_value, past_views, rule_id,
};
use crate::ui::{CopiedPart, Notice};

/// The most passwords the generator's history keeps.
pub const GENERATOR_HISTORY_MAX: usize = 100;
/// How long the generator's history keeps a password.
pub const GENERATOR_HISTORY_DAYS: u64 = 90;
/// The longest custom field label (characters).
const MAX_LABEL_CHARS: usize = 100;
/// The longest name of a page's field (characters).
const MAX_PAGE_FIELD_CHARS: usize = 200;

/// A field's purpose, as a write of one names it.
fn purpose_of(field: &Field) -> Purpose {
    field.purpose.or(Purpose::Custom)
}

impl Core {
    pub(super) fn passwords_on(&self) -> CoreResult<()> {
        if self.shared.config.features.passwords { Ok(()) } else { Err(ErrorCode::FeatureUnavailable.into()) }
    }

    /// The logins `ids` as a file of `format`, once the user proved to be at this device (the
    /// master password, or without it the biometric check that unlocks this vault, asked with
    /// `reason`): a KeePass database under `kdbx_password`, or Bitwarden's CSV, which the user
    /// `acknowledged` is plain text. Made here, saved by the shell ([`Self::export_items_write`],
    /// or the phone's file picker); the webview hears the file's name only.
    pub async fn export_items(
        &self,
        ids: &[Uuid],
        format: ItemsFormat,
        password: Option<Zeroizing<String>>,
        reason: Option<String>,
        acknowledged: bool,
        kdbx_password: Option<Zeroizing<String>>,
    ) -> CoreResult<ItemsFile> {
        self.passwords_on()?;
        let logins = {
            let st = self.lock();
            let session = unlocked(&st)?;
            let app = match format {
                ItemsFormat::Kdbx => App::KeePass,
                ItemsFormat::Csv => App::Bitwarden,
            };
            let mut chosen = std::collections::HashSet::new();
            ids.iter()
                .filter(|id| chosen.insert(**id))
                .filter_map(|id| session.data.items.iter().find(|item| item.id == *id && !item.read_only()))
                .map(|item| login_of(item, &session.data, app))
                .collect::<Vec<_>>()
        };
        if logins.is_empty() {
            return Err(ErrorCode::ExportNothing.into());
        }
        // What the user chose is checked before they are asked to prove they are there.
        let kdbx_password = match format {
            ItemsFormat::Csv if !acknowledged => return Err(ErrorCode::ExportAcknowledgeNeeded.into()),
            ItemsFormat::Csv => Zeroizing::new(String::new()),
            ItemsFormat::Kdbx => {
                let password = kdbx_password.unwrap_or_default();
                check_password(&password)?;
                password
            }
        };
        self.confirm_presence(password, reason).await?;
        let cost = self.shared.config.kdf;
        blocking(move || encode(&logins, format, &kdbx_password, cost)).await
    }

    /// Save `file` ([`Self::export_items`]'s) at `path`, the shell's save dialog's, readable by its
    /// owner only; its name. The vault must still be unlocked: it may have locked while the
    /// dialog was open.
    pub async fn export_items_write(&self, file: ItemsFile, path: std::path::PathBuf) -> CoreResult<String> {
        self.ensure_unlocked()?;
        blocking(move || {
            write_private(&path, file.bytes())?;
            Ok(file_name(&path))
        })
        .await
    }

    /// Every login, without its secrets.
    pub fn items_list(&self) -> CoreResult<Vec<ItemView>> {
        self.passwords_on()?;
        let st = self.lock();
        Ok(unlocked(&st)?.data.items.iter().map(ItemView::of).collect())
    }

    /// Add a login; its id. The secrets come from the webview (or, for the password, from the
    /// generator's last one) and never go back but through `item_reveal`.
    pub fn item_add(&self, draft: ItemDraft) -> CoreResult<Uuid> {
        self.add_item(draft, false)
    }

    /// [`Self::item_add`]; `quiet`: the browser extension's, which does not restart the idle timer.
    pub(super) fn add_item(&self, draft: ItemDraft, quiet: bool) -> CoreResult<Uuid> {
        self.passwords_on()?;
        let now = self.now_ms();
        let id = {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if draft.totp_entry.is_some_and(|entry| session.data.get(entry).is_none()) {
                return Err(ErrorCode::EntryNotFound.into());
            }
            let id = Uuid::new_v4();
            let (stamp, mwid) = (session.data.tick(now), random_write_id());
            let mut item = VaultItem::login(id, now, stamp, mwid);
            item.title = clean_title(&draft.title)?;
            item.urls = clean_urls(draft.urls)?;
            item.notes = checked_notes(draft.notes)?;
            item.tags = clean_tags(draft.tags)?;
            item.group = draft.group.as_deref().and_then(clean_group);
            item.favorite = draft.favorite;
            item.color = Lenient::Known(draft.color);
            item.mark = draft.mark.as_deref().and_then(clean_item_mark);
            item.protection = Lenient::Known(draft.protection);
            item.autofill = autofill_prefs(draft.autofill);
            item.totp_entry = draft.totp_entry;
            let username = clean_text(&draft.username, crate::entry::MAX_NAME_CHARS)?;
            let email = clean_text(&draft.email, crate::entry::MAX_NAME_CHARS)?;
            for (purpose, value) in [(Purpose::Username, username), (Purpose::Email, email)] {
                if !value.is_empty() {
                    let (stamp, wid) = (session.data.tick(now), random_write_id());
                    item.write_field(Uuid::new_v4(), purpose, stamp, wid, now, |f| f.value = Zeroizing::new(value));
                }
            }
            if let Some(password) = secret_value(session, draft.password, now)? {
                let (stamp, wid) = (session.data.tick(now), random_write_id());
                item.write_field(Uuid::new_v4(), Purpose::Password, stamp, wid, now, |f| {
                    f.value = password;
                    f.hidden = true;
                });
            }
            for change in draft.fields {
                apply_field(session, &mut item, FieldChange { id: None, ..change }, now)?;
            }
            check_field_count(&item)?;
            session.data.items.push(item);
            let idle = st.last_activity;
            self.save(&mut st, true, move |s| s.data.items.retain(|i| i.id != id))?;
            if quiet {
                st.last_activity = idle;
            }
            touch_items(&mut st);
            id
        };
        self.changed();
        Ok(id)
    }

    /// Change a login. Every part the patch names takes a write of its own: the metadata one, each
    /// field one, so that a change made at the same time elsewhere to another part stays.
    pub fn item_update(&self, id: Uuid, patch: ItemPatch) -> CoreResult<()> {
        self.update_item(id, patch, false)
    }

    /// [`Self::item_update`]; `quiet`: the browser extension's, which does not restart the idle
    /// timer.
    pub(super) fn update_item(&self, id: Uuid, patch: ItemPatch, quiet: bool) -> CoreResult<()> {
        self.passwords_on()?;
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if let Some(Some(entry)) = patch.totp_entry
                && session.data.get(entry).is_none()
            {
                return Err(ErrorCode::EntryNotFound.into());
            }
            let index = session.data.items.iter().position(|i| i.id == id).ok_or(ErrorCode::ItemNotFound)?;
            let before = session.data.items[index].clone();
            if holds_later_values(&before) {
                return Err(ErrorCode::ItemReadOnly.into());
            }
            let mut item = before.clone();
            let mut meta = item.clone();
            if let Some(title) = &patch.title {
                meta.title = clean_title(title)?;
            }
            if let Some(urls) = patch.urls {
                meta.urls = clean_urls(urls)?;
            }
            match patch.notes {
                SecretChange::Keep => {}
                SecretChange::Set { value } => meta.notes = checked_notes(value)?,
                SecretChange::Clear => meta.notes = Zeroizing::new(String::new()),
                SecretChange::Generated => return Err(ErrorCode::ItemInvalid.into()),
            }
            if let Some(tags) = patch.tags {
                meta.tags = clean_tags(tags)?;
            }
            if let Some(group) = &patch.group {
                meta.group = clean_group(group);
            }
            if let Some(favorite) = patch.favorite {
                meta.favorite = favorite;
            }
            if let Some(color) = patch.color {
                meta.color = Lenient::Known(color);
            }
            if let Some(mark) = &patch.mark {
                meta.mark = clean_item_mark(mark);
            }
            if let Some(protection) = patch.protection {
                meta.protection = Lenient::Known(protection);
            }
            if let Some(choice) = patch.autofill {
                meta.autofill = autofill_prefs(choice);
            }
            if let Some(entry) = patch.totp_entry {
                meta.totp_entry = entry;
            }
            // The metadata is written once, and only when it changed.
            if meta != item {
                let (stamp, mwid) = (session.data.tick(now), random_write_id());
                item = meta;
                item.meta_written(stamp, mwid, now);
            }
            for (purpose, value) in [(Purpose::Username, patch.username), (Purpose::Email, patch.email)] {
                let Some(value) = value else { continue };
                let value = clean_text(&value, crate::entry::MAX_NAME_CHARS)?;
                let current = item.field_of(purpose);
                if current.is_some_and(|f| *f.value == value) || (current.is_none() && value.is_empty()) {
                    continue;
                }
                let field = current.map_or_else(Uuid::new_v4, |f| f.id);
                let (stamp, wid) = (session.data.tick(now), random_write_id());
                item.write_field(field, purpose, stamp, wid, now, |f| f.value = Zeroizing::new(value));
            }
            let password = match patch.password {
                SecretChange::Clear if item.field_of(Purpose::Password).is_some() => Some(Zeroizing::new(String::new())),
                other => secret_value(session, other, now)?,
            };
            if let Some(password) = password {
                let field = item.field_of(Purpose::Password).map_or_else(Uuid::new_v4, |f| f.id);
                if item.field(field).is_none_or(|f| f.value != password || f.removed) {
                    let (stamp, wid) = (session.data.tick(now), random_write_id());
                    item.write_field(field, Purpose::Password, stamp, wid, now, |f| {
                        f.value = password;
                        f.hidden = true;
                        f.removed = false;
                    });
                }
            }
            for change in patch.fields {
                apply_field(session, &mut item, change, now)?;
            }
            for settle in patch.settle {
                let field = item.field(settle.field).ok_or(ErrorCode::ItemNotFound)?;
                let value = match &settle.take {
                    Some(take) => past_value(&item, settle.field, take).ok_or(ErrorCode::ItemNotFound)?.value.clone(),
                    None => field.value.clone(),
                };
                let (stamp, wid) = (session.data.tick(now), random_write_id());
                item.settle_field(settle.field, value, stamp, wid, now);
            }
            check_field_count(&item)?;
            session.data.items[index] = item;
            let idle = st.last_activity;
            self.save(&mut st, true, move |s| {
                if let Some(slot) = s.data.items.iter_mut().find(|i| i.id == id) {
                    *slot = before;
                }
            })?;
            if quiet {
                st.last_activity = idle;
            }
            touch_items(&mut st);
        }
        self.changed();
        Ok(())
    }

    /// Delete logins, leaving tombstones so that the deletion syncs.
    pub fn item_delete(&self, ids: &[Uuid]) -> CoreResult<()> {
        self.passwords_on()?;
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if !ids.iter().any(|id| session.data.items.iter().any(|i| i.id == *id)) {
                return Err(ErrorCode::ItemNotFound.into());
            }
            let (items, tombstones) = (session.data.items.clone(), session.data.item_tombstones.clone());
            for id in ids {
                if session.data.items.iter().any(|i| i.id == *id) {
                    let stamp = session.data.tick(now);
                    session.data.items.retain(|i| i.id != *id);
                    bury(&mut session.data.item_tombstones, Tombstone { id: *id, stamp, counter: None });
                }
            }
            self.save(&mut st, true, move |s| {
                s.data.items = items;
                s.data.item_tombstones = tombstones;
            })?;
            touch_items(&mut st);
        }
        self.changed();
        Ok(())
    }

    /// Show a login's secrets: the parts asked for. A login of the `confirm` level (or of a level
    /// a later Lockra added) asks for the master password or the biometric check first; one of
    /// the `fill` or `no_extension` levels shows them as it would fill them.
    pub async fn item_reveal(&self, id: Uuid, parts: RevealParts, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<ItemRevealed> {
        self.passwords_on()?;
        let protected = {
            let st = self.lock();
            let item = unlocked(&st)?.data.items.iter().find(|i| i.id == id).ok_or(ErrorCode::ItemNotFound)?;
            needs_presence(item)
        };
        if protected {
            self.confirm_presence(password, reason).await?;
        }
        let st = self.lock();
        let item = unlocked(&st)?.data.items.iter().find(|i| i.id == id).ok_or(ErrorCode::ItemNotFound)?;
        let fields = parts
            .fields
            .iter()
            .filter_map(|field| item.field(*field))
            .filter(|f| !f.removed)
            .map(|f| RevealedField { id: f.id, value: f.value.to_string() })
            .collect();
        Ok(ItemRevealed {
            item_id: id,
            password: parts.password.then(|| item.field_of(Purpose::Password).map(|f| f.value.to_string()).unwrap_or_default()),
            notes: parts.notes.then(|| item.notes.to_string()),
            fields,
            history: parts.history.then(|| past_views(item)),
        })
    }

    /// Put a part of a login on the clipboard, written by Rust and cleared after the clipboard
    /// time if it still holds it. A secret of a `confirm` login asks for presence first; the
    /// user name, the email and the linked code never do.
    pub async fn item_copy(&self, id: Uuid, part: CopyPart, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<()> {
        self.passwords_on()?;
        let protected = {
            let st = self.lock();
            let item = unlocked(&st)?.data.items.iter().find(|i| i.id == id).ok_or(ErrorCode::ItemNotFound)?;
            let secret = match part {
                CopyPart::Password => true,
                CopyPart::Field { id: field } => item.field(field).is_some_and(|f| f.hidden),
                CopyPart::Username | CopyPart::Email | CopyPart::Totp => false,
            };
            secret && needs_presence(item)
        };
        if protected {
            self.confirm_presence(password, reason).await?;
        }
        let now = self.now_ms();
        let clear_after = {
            let mut st = self.lock();
            let clear_seconds = st.settings.clipboard_clear_seconds;
            let session = unlocked_mut(&mut st)?;
            let text = copy_part_text(session, id, part, now)?;
            self.shared.ports.clipboard.set_secret_text(&text).map_err(|_| ErrorCode::ClipboardFailed)?;
            if let Some(item) = session.data.items.iter_mut().find(|i| i.id == id) {
                item.last_used_at_ms = Some(now);
            }
            // Recency only: no automatic backup for a copy, and a failed write loses nothing.
            if let Err(error) = self.write_vault(session) {
                tracing::warn!(?error, "could not record the copy time");
            }
            st.clipboard = (clear_seconds > 0).then(|| (text, Instant::now() + Duration::from_secs(u64::from(clear_seconds))));
            st.last_activity = Instant::now();
            touch_items(&mut st);
            (clear_seconds > 0).then_some(clear_seconds)
        };
        self.changed();
        let part = match part {
            CopyPart::Username => CopiedPart::Username,
            CopyPart::Email => CopiedPart::Email,
            CopyPart::Password => CopiedPart::Password,
            CopyPart::Totp => CopiedPart::Totp,
            CopyPart::Field { .. } => CopiedPart::Field,
        };
        self.notice(Notice::ItemCopied { item_id: Some(id), part, clear_after_s: clear_after });
        Ok(())
    }

    /// Make a password with `options`, held to the rules of `domain` when Lockra knows them; the
    /// core keeps it, for `generator_copy` and a login saved with it.
    pub fn generator_generate(&self, options: &GeneratorOptions, domain: Option<String>) -> CoreResult<GeneratedView> {
        self.passwords_on()?;
        let site = domain.map(|d| d.trim().trim_end_matches('.').to_lowercase()).filter(|d| !d.is_empty() && d.len() <= 253);
        let rules = site.as_deref().and_then(bundled_password_rules);
        let made = generate(options, rules.as_ref(), &mut OsRandom::new()).map_err(|error| match error {
            GeneratorError::RulesUnsatisfiable => ErrorCode::GeneratorRulesUnsatisfiable,
            GeneratorError::NoCharacters => ErrorCode::ItemInvalid,
            GeneratorError::Random(_) => ErrorCode::Internal,
        })?;
        let view = GeneratedView { password: made.password.to_string(), bits: made.bits, rules_met: made.rules_met };
        let mut st = self.lock();
        unlocked_mut(&mut st)?.generated = Some((made.password, site));
        Ok(view)
    }

    /// Put the generator's last password on the clipboard, and into its history.
    pub fn generator_copy(&self) -> CoreResult<()> {
        self.passwords_on()?;
        let now = self.now_ms();
        let clear_after = {
            let mut st = self.lock();
            let clear_seconds = st.settings.clipboard_clear_seconds;
            let session = unlocked_mut(&mut st)?;
            let (password, site) = session.generated.clone().ok_or(ErrorCode::ItemInvalid)?;
            self.shared.ports.clipboard.set_secret_text(&password).map_err(|_| ErrorCode::ClipboardFailed)?;
            remember_generated(session, password.clone(), site, now);
            self.save(&mut st, false, |_| {})?;
            st.clipboard = (clear_seconds > 0).then(|| (password, Instant::now() + Duration::from_secs(u64::from(clear_seconds))));
            (clear_seconds > 0).then_some(clear_seconds)
        };
        self.changed();
        self.notice(Notice::ItemCopied { item_id: None, part: CopiedPart::Generated, clear_after_s: clear_after });
        Ok(())
    }

    /// The generator's history, newest first, after the master password or the biometric check.
    pub async fn generator_history_reveal(&self, password: Option<Zeroizing<String>>, reason: Option<String>) -> CoreResult<Vec<GeneratedPastView>> {
        self.passwords_on()?;
        self.confirm_presence(password, reason).await?;
        let now = self.now_ms();
        let st = self.lock();
        let history = unlocked(&st)?.data.local.as_ref().map(|l| l.generator_history.clone()).unwrap_or_default();
        Ok(kept_history(history, now).into_iter().map(|g| GeneratedPastView { password: g.password.to_string(), at_ms: g.at_ms, site: g.site }).collect())
    }

    /// Forget the generator's history.
    pub fn generator_history_clear(&self) -> CoreResult<()> {
        self.passwords_on()?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let before = std::mem::take(&mut session.data.local_mut().generator_history);
            self.save(&mut st, false, move |s| s.data.local_mut().generator_history = before)?;
        }
        self.changed();
        Ok(())
    }

    /// How strong `password` is.
    pub fn password_score(&self, password: &str) -> CoreResult<ScoreView> {
        self.passwords_on()?;
        let strength = strength(password, &[]);
        Ok(ScoreView { score: strength.score, warning: strength.warning.map(|w| w.code().to_owned()) })
    }

    /// Add a site rule (or widen the one about the same single domain). A `match_host` or
    /// `match_domain` rule is about one domain, and the other kind's rule about it goes in the same
    /// change: a domain matches one way.
    pub fn site_rule_add(&self, kind: SiteRuleKind, domains: &[String]) -> CoreResult<()> {
        self.add_site_rule(kind, domains, false)
    }

    /// [`Self::site_rule_add`]; `quiet`: the browser extension's, which does not restart the idle
    /// timer.
    pub(super) fn add_site_rule(&self, kind: SiteRuleKind, domains: &[String], quiet: bool) -> CoreResult<()> {
        self.passwords_on()?;
        let domains = clean_domains(domains)?;
        let one_domain = kind.opposite().is_some();
        if (kind == SiteRuleKind::Equivalent && domains.len() < 2) || (one_domain && domains.len() != 1) {
            return Err(ErrorCode::ItemInvalid.into());
        }
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let (rules, tombstones) = (session.data.site_rules.clone(), session.data.site_rule_tombstones.clone());
            if let (Some(other), [domain]) = (kind.opposite(), domains.as_slice()) {
                let gone: Vec<Uuid> = session
                    .data
                    .site_rules
                    .iter()
                    .filter(|r| r.kind == Lenient::Known(other) && r.domains.iter().any(|d| same_host(d, domain)))
                    .map(|r| r.id)
                    .collect();
                for id in gone {
                    remove_site_rule(&mut session.data, id, now);
                }
            }
            let id = rule_id(kind, &domains);
            let stamp = session.data.tick(now);
            let rule = SiteRule { id, kind: Lenient::Known(kind), domains, stamp, extra: Default::default() };
            match session.data.site_rules.iter_mut().find(|r| r.id == id) {
                Some(own) => *own = rule,
                None => session.data.site_rules.push(rule),
            }
            let idle = st.last_activity;
            self.save(&mut st, true, move |s| {
                s.data.site_rules = rules;
                s.data.site_rule_tombstones = tombstones;
            })?;
            if quiet {
                st.last_activity = idle;
            }
            touch_items(&mut st);
        }
        self.changed();
        Ok(())
    }

    /// The browser extension's switch for `page`'s site, quiet as its other requests: pages there
    /// match by the full host (`host`) or by the site. The `match_host` and `match_domain` rules
    /// about the site, and about the domains of `page`'s host within it, go; a rule of the asked
    /// kind about the site comes where what is left (a rule about a parent of the site, the
    /// policy's default) says otherwise. So `page` matches as asked, and no rule is left that says
    /// what the default does. One change, synced as the interface's own.
    pub(super) fn set_site_matching(&self, page: &WebUrl, host: bool) -> CoreResult<()> {
        self.passwords_on()?;
        let now = self.now_ms();
        let site = page.site();
        let kind = if host { SiteRuleKind::MatchHost } else { SiteRuleKind::MatchDomain };
        let wanted = SiteRule::single_id(kind, site);
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            // The rules about the site or a domain of the page's host below it.
            let about_page = |rule: &SiteRule| {
                matches!(rule.kind, Lenient::Known(SiteRuleKind::MatchHost | SiteRuleKind::MatchDomain))
                    && rule.domains.iter().any(|d| covering(d, page.host()).is_some_and(|length| length >= site.len()))
            };
            let is_wanted = |rule: &SiteRule| rule.id == wanted && rule.kind == Lenient::Known(kind) && rule.domains.len() == 1 && rule.domains[0] == site;
            let default = super::browser::policy(session).match_host;
            let add = host_rule(session.data.site_rules.iter().filter(|r| !about_page(r)), page).unwrap_or(default) != host;
            let gone: Vec<Uuid> = session.data.site_rules.iter().filter(|r| about_page(r) && !(add && is_wanted(r))).map(|r| r.id).collect();
            let kept = add && session.data.site_rules.iter().any(is_wanted);
            if gone.is_empty() && add == kept {
                return Ok(());
            }
            let (rules, tombstones) = (session.data.site_rules.clone(), session.data.site_rule_tombstones.clone());
            for id in gone {
                remove_site_rule(&mut session.data, id, now);
            }
            if add && !kept {
                let stamp = session.data.tick(now);
                session.data.site_rules.push(SiteRule {
                    id: wanted,
                    kind: Lenient::Known(kind),
                    domains: vec![site.to_owned()],
                    stamp,
                    extra: Default::default(),
                });
            }
            let idle = st.last_activity;
            self.save(&mut st, true, move |s| {
                s.data.site_rules = rules;
                s.data.site_rule_tombstones = tombstones;
            })?;
            st.last_activity = idle;
            touch_items(&mut st);
        }
        self.changed();
        Ok(())
    }

    /// Fold `groups` in the password list ("" for the logins in no group) and unfold the others.
    /// This device's view, kept in the vault's local part: never synced, never in a backup.
    pub fn collapse_item_groups(&self, groups: Vec<String>) -> CoreResult<()> {
        self.passwords_on()?;
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            let existing: std::collections::BTreeSet<String> = session.data.items.iter().filter_map(|i| i.group.clone()).collect();
            let folded: std::collections::BTreeSet<String> =
                groups.iter().map(|g| g.trim().to_owned()).filter(|g| g.is_empty() || existing.contains(g)).collect();
            let folded: Vec<String> = folded.into_iter().collect();
            let current = session.data.local.as_ref().map(|l| l.item_view.collapsed_groups.clone()).unwrap_or_default();
            if current == folded {
                return Ok(());
            }
            let previous = std::mem::replace(&mut session.data.local_mut().item_view.collapsed_groups, folded);
            self.save(&mut st, false, move |s| s.data.local_mut().item_view.collapsed_groups = previous)?;
        }
        self.changed();
        Ok(())
    }

    /// Remove a site rule.
    pub fn site_rule_remove(&self, id: Uuid) -> CoreResult<()> {
        self.passwords_on()?;
        let now = self.now_ms();
        {
            let mut st = self.lock();
            let session = unlocked_mut(&mut st)?;
            if !session.data.site_rules.iter().any(|r| r.id == id) {
                return Err(ErrorCode::ItemNotFound.into());
            }
            let (rules, tombstones) = (session.data.site_rules.clone(), session.data.site_rule_tombstones.clone());
            remove_site_rule(&mut session.data, id, now);
            self.save(&mut st, true, move |s| {
                s.data.site_rules = rules;
                s.data.site_rule_tombstones = tombstones;
            })?;
            touch_items(&mut st);
        }
        self.changed();
        Ok(())
    }
}

/// How many passwords the generator's history holds; 0 unless unlocked.
pub(super) fn generator_history_count(st: &State) -> u32 {
    let PhaseState::Unlocked(session) = &st.phase else { return 0 };
    let count = session.data.local.as_ref().map_or(0, |l| l.generator_history.len());
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// What a change of a secret puts there: the value typed, the generator's last password (which
/// goes into its history as used), or nothing to write.
fn secret_value(session: &mut Session, change: SecretChange, now: u64) -> CoreResult<Option<Zeroizing<String>>> {
    Ok(match change {
        SecretChange::Keep | SecretChange::Clear => None,
        SecretChange::Set { value } => Some(checked_value(value)?),
        SecretChange::Generated => {
            let (password, site) = session.generated.clone().ok_or(ErrorCode::ItemInvalid)?;
            remember_generated(session, password.clone(), site, now);
            Some(password)
        }
    })
}

/// Add, change or remove a custom field of `item`.
fn apply_field(session: &mut Session, item: &mut VaultItem, change: FieldChange, now: u64) -> CoreResult<()> {
    let existing = change.id.and_then(|id| item.field(id).cloned());
    if change.id.is_some() && existing.is_none() {
        return Err(ErrorCode::ItemNotFound.into());
    }
    if existing.as_ref().is_some_and(|f| purpose_of(f) != Purpose::Custom) {
        return Err(ErrorCode::ItemInvalid.into());
    }
    let id = change.id.unwrap_or_else(Uuid::new_v4);
    let mut next = existing.clone().unwrap_or_else(|| Field::new(id, Purpose::Custom, Hlc::default()));
    if change.remove {
        if existing.as_ref().is_none_or(|f| f.removed) {
            return Ok(());
        }
        next.removed = true;
        next.value = Zeroizing::new(String::new());
    } else {
        if let Some(label) = &change.label {
            next.label = clean_text(label, MAX_LABEL_CHARS)?;
        }
        if let Some(hidden) = change.hidden {
            next.hidden = hidden;
        }
        if let Some(page_field) = &change.page_field {
            next.page_field = Some(clean_text(page_field, MAX_PAGE_FIELD_CHARS)?).filter(|p| !p.is_empty());
        }
        match change.value {
            SecretChange::Clear => next.value = Zeroizing::new(String::new()),
            other => {
                if let Some(value) = secret_value(session, other, now)? {
                    next.value = value;
                }
            }
        }
        next.removed = false;
    }
    if existing.as_ref().is_some_and(|f| same_content(f, &next)) {
        return Ok(());
    }
    let (stamp, wid) = (session.data.tick(now), random_write_id());
    item.write_field(id, Purpose::Custom, stamp, wid, now, |f| {
        f.label = next.label;
        f.value = next.value;
        f.hidden = next.hidden;
        f.page_field = next.page_field;
        f.removed = next.removed;
    });
    Ok(())
}

fn same_content(a: &Field, b: &Field) -> bool {
    (a.label.as_str(), a.value.as_str(), a.hidden, &a.page_field, a.removed) == (b.label.as_str(), b.value.as_str(), b.hidden, &b.page_field, b.removed)
}

/// What copying `part` of login `id` puts on the clipboard at `now`: the part's value, or the
/// linked account's current code.
pub(super) fn copy_part_text(session: &Session, id: Uuid, part: CopyPart, now: u64) -> CoreResult<Zeroizing<String>> {
    let item = session.data.items.iter().find(|i| i.id == id).ok_or(ErrorCode::ItemNotFound)?;
    Ok(match part {
        CopyPart::Username => field_value(item, Purpose::Username)?,
        CopyPart::Email => field_value(item, Purpose::Email)?,
        CopyPart::Password => field_value(item, Purpose::Password)?,
        CopyPart::Field { id: field } => {
            item.field(field).filter(|f| !f.removed && !f.value.is_empty()).map(|f| f.value.clone()).ok_or(ErrorCode::ItemNotFound)?
        }
        CopyPart::Totp => {
            let entry = item.totp_entry.and_then(|entry| session.data.get(entry)).ok_or(ErrorCode::EntryNotFound)?;
            Zeroizing::new(current_code(entry, now))
        }
    })
}

/// The value of `item`'s field of `purpose`, when it holds one.
fn field_value(item: &VaultItem, purpose: Purpose) -> CoreResult<Zeroizing<String>> {
    item.field_of(purpose).filter(|f| !f.value.is_empty()).map(|f| f.value.clone()).ok_or_else(|| ErrorCode::ItemNotFound.into())
}

/// A login whose secrets the app shows only after the user proves to be there.
fn needs_presence(item: &VaultItem) -> bool {
    !matches!(item.protection, Lenient::Known(Protection::Fill | Protection::NoExtension))
}

/// Keep `password` in the generator's history as copied or used at `now`.
pub(super) fn remember_generated(session: &mut Session, password: Zeroizing<String>, site: Option<String>, now: u64) {
    let local = session.data.local_mut();
    let mut history = std::mem::take(&mut local.generator_history);
    history.retain(|g| g.password != password);
    history.insert(0, GeneratedPassword { password, at_ms: now, site });
    local.generator_history = kept_history(history, now);
}

/// The generator's history as it is kept: newest first, the last [`GENERATOR_HISTORY_DAYS`] days,
/// at most [`GENERATOR_HISTORY_MAX`].
fn kept_history(mut history: Vec<GeneratedPassword>, now: u64) -> Vec<GeneratedPassword> {
    let oldest = now.saturating_sub(GENERATOR_HISTORY_DAYS * 24 * 3_600_000);
    history.retain(|g| g.at_ms >= oldest);
    history.sort_by_key(|g| std::cmp::Reverse(g.at_ms));
    history.truncate(GENERATOR_HISTORY_MAX);
    history
}

/// Keep `tombstone`: the later deletion of the two.
fn bury(tombstones: &mut Vec<Tombstone>, tombstone: Tombstone) {
    match tombstones.iter_mut().find(|t| t.id == tombstone.id) {
        Some(own) => own.stamp = own.stamp.max(tombstone.stamp),
        None => tombstones.push(tombstone),
    }
}

/// Take site rule `id` out of `data`, leaving a tombstone stamped at `now_ms` so that the removal
/// syncs.
fn remove_site_rule(data: &mut VaultData, id: Uuid, now_ms: u64) {
    let stamp = data.tick(now_ms);
    data.site_rules.retain(|r| r.id != id);
    bury(&mut data.site_rule_tombstones, Tombstone { id, stamp, counter: None });
}

/// Whether two domains of site rules name one host (`例え.jp` and `xn--r8jz45g.jp`); text that
/// names none is compared as written.
fn same_host(a: &str, b: &str) -> bool {
    match (rule_host(a), rule_host(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}
